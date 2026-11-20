//! # login / account — 登录态与个人数据
//!
//! 上游无账号密码登录端点：登录态 = 浏览器登录 fanqienovel.com 后复制的 Cookie。
//! cookie 存独立文件 `~/.config/fqdt/cookie`（不进 config.ini，防 `config --list` 泄露）。
//! 两条数据通道：
//! - core 端点（cookie 放 body）：library（shelf/folders/progress）、history
//! - 网页端点（fanqienovel.com 同源，Cookie 放 header）：user/info/v2、basic_info、
//!   bookmark/personal_list（划线书单）——逆向自开源项目 fysh1010/mcp-server-fanqie、
//!   naiyQAQ/fanqie-assistant、hesan1232/fanqie.koplugin
//!
//! # CLI 调用
//! - `fqdt login [COOKIE] [--check] [--clear]`
//! - `fqdt account [shelf|folders|history|progress|highlights|user] [--json]`
//! - `fqdt account --add|--remove|--check <book_id>`（云端书架增删/在架检查）

use crate::config;
use crate::util::{BLUE, CYAN, DIM, GREEN, MAGENTA, RED, RST, YELLOW};
use crate::Config;
use serde_json::{json, Value};
use std::collections::HashMap;

/// 番茄网页端点基址（同源 Cookie 鉴权）。
const FQ_WEB: &str = "https://fanqienovel.com";

/// `fqdt login`：设置/查看/验证/清除 Cookie。
pub async fn login(cookie: Option<String>, check: bool, clear: bool) {
    if clear {
        return match config::clear_cookie() {
            Ok(()) => crate::emit_line(&format!("{GREEN}ok{RST} Cookie 已清除")),
            Err(e) => eprintln!("  {RED}err{RST} {e}"),
        };
    }
    if let Some(raw) = cookie {
        match config::save_cookie(&raw) {
            Ok(()) => {
                crate::emit_line(&format!("{GREEN}ok{RST} Cookie 已保存 ~/.config/fqdt/cookie"));
                // 保存后立即验证，不再让用户对着提示行猜有没有生效
                verify_now().await;
                crate::emit_line(&format!("  {DIM}查看: fqdt account user · 清除: fqdt login --clear{RST}"));
            }
            Err(e) => eprintln!("  {RED}err{RST} {e}"),
        }
        return;
    }
    if check { return verify_now().await; }
    // 无参数：状态 + 获取指引
    match config::load_cookie() {
        None => {
            crate::emit_line(&format!("  {YELLOW}未设置 Cookie{RST}"));
            crate::emit_line(&format!("  {DIM}获取: 浏览器登录 fanqienovel.com → 开发者工具 → 复制请求 Cookie → fqdt login '<Cookie>'{RST}"));
        }
        Some(c) => {
            let sess = config::cookie_has_session(&c);
            let mark = if sess { format!("{GREEN}session ✓{RST}") } else { format!("{YELLOW}session ✗{RST}") };
            crate::emit_line(&format!("{GREEN}ok{RST} Cookie 已设置（{} 字节，{mark}）", c.len()));
            crate::emit_line(&format!("  {DIM}验证: fqdt login --check · 清除: fqdt login --clear{RST}"));
        }
    }
}

/// user/info/v2 的 id 是字符串（"2253353583647107"），兼容数字两种形态。
fn id_str(d: Option<&serde_json::Value>) -> String {
    d.and_then(|x| x.get("id"))
        .and_then(|x| x.as_str().map(String::from).or_else(|| x.as_u64().map(|n| n.to_string())))
        .unwrap_or_else(|| "0".into())
}

/// 用已存 Cookie 验证登录态：网页 user/info/v2 是权威（显示昵称）。
/// 不回退 core library——实测假 Cookie 下 library 也回 code:0，会误报成功。
async fn verify_now() {
    let Some(c) = config::load_cookie() else {
        return eprintln!("  {RED}err{RST} 未设置 Cookie（fqdt login '<Cookie>'）");
    };
    match page_json(&format!("{FQ_WEB}/api/user/info/v2"), &c).await {
        Ok(v) => {
            let d = v.get("data");
            let name = d.and_then(|x| x.get("name")).and_then(|x| x.as_str()).unwrap_or("?");
            let id = id_str(d);
            crate::emit_line(&format!("{GREEN}ok{RST} 登录有效（{CYAN}{name}{RST} · {BLUE}#{id}{RST}）"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// GET 网页端点（带 Cookie）→ JSON + 业务码统一判定。
async fn page_json(url: &str, cookie: &str) -> Result<Value, String> {
    let text = crate::api::http_get_cookie(url, cookie).await?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "上游返回非 JSON".to_string())?;
    page_check(v)
}

/// POST 网页端点（JSON body）并做统一响应判定。
async fn page_post(url: &str, cookie: &str, body: &[u8]) -> Result<Value, String> {
    let text = crate::api::http_post_cookie(url, cookie, body).await?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "上游返回非 JSON".to_string())?;
    page_check(v)
}

/// 网页响应统一判定：登录失效 / 非 0 code → Err。
fn page_check(v: Value) -> Result<Value, String> {
    let code = v.get("code").and_then(|x| x.as_i64());
    let msg = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
    if code == Some(-1) || code == Some(101108) || msg.contains("登录") || msg.to_lowercase().contains("login") {
        return Err("登录态失效或 Cookie 无效（fqdt login '<Cookie>' 更新）".into());
    }
    if code.is_some_and(|c| c != 0) {
        return Err(format!("上游拒绝 code={code:?} {msg}"));
    }
    Ok(v)
}

/// `fqdt account [SECTION] [--json] [--add/--remove/--check <id>]`：个人数据展示与书架增删。
pub async fn account(section: Option<String>, json: bool,
    add: Option<String>, remove: Option<String>, check: Option<String>, _cfg: &Config) {
    let op = add.map(|b| ("add", b))
        .or_else(|| remove.map(|b| ("remove", b)))
        .or_else(|| check.map(|b| ("check", b)));
    let Some(cookie) = config::load_cookie() else {
        eprintln!("  {RED}err{RST} 未设置登录 Cookie");
        crate::emit_line(&format!("  {DIM}先 fqdt login '<Cookie>'（浏览器登录 fanqienovel.com 复制）{RST}"));
        return;
    };
    if let Some((op, id)) = op {
        return shelf_op(&cookie, op, &id).await;
    }
    let sec = section.as_deref().unwrap_or("shelf");
    match sec {
        "user" => return render_user().await,
        "shelf" | "folders" | "progress" | "history" | "highlights" => {}
        _ => {
            eprintln!("  {RED}err{RST} 未知部分: {sec}");
            crate::emit_line(&format!("  {DIM}可选: shelf(默认) · folders · history · progress · highlights · user{RST}"));
            return;
        }
    }
    if sec == "highlights" {
        util_progress("获取 highlights");
        let url = format!(
            "{FQ_WEB}/reading/reader/bookmark/personal_list/?aid=1967&iid=0&version_code=74132&offset=0&limit=50");
        let res = page_json(&url, &cookie).await;
        util_clear();
        return match res {
            Ok(v) => render_highlights(&v, json),
            Err(e) => eprintln!("  {RED}err{RST} {e}"),
        };
    }
    if sec == "shelf" { return shelf_section(&cookie, json).await; }
    util_progress(&format!("获取 {sec}"));
    let res = fetch_section(&cookie, sec).await;
    util_clear();
    match res {
        Ok(v) => render_section(&v, json, sec),
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 书架展示：网页三端点合并（当前章节/进度），失败回退 core 通道。
async fn shelf_section(cookie: &str, json: bool) {
    util_progress("获取书架");
    let res = fetch_shelf_web(cookie).await;
    util_clear();
    match res {
        Ok(v) => render_shelf(&v, json),
        Err(e) => {
            eprintln!("  {DIM}网页通道失败（{e}），回退核心通道{RST}");
            util_progress("获取书架");
            let fb = fetch_section(cookie, "shelf").await;
            util_clear();
            match fb {
                Ok(v) => render_section(&v, json, "shelf"),
                Err(e) => eprintln!("  {RED}err{RST} {e}"),
            }
        }
    }
}

/// 书架写操作 add/remove（网页 bookshelf 端点，JSON body）。
async fn shelf_op(cookie: &str, op: &str, book_id: &str) {
    if op == "check" { return shelf_check(cookie, book_id).await; }
    let base = format!("{FQ_WEB}/reading/bookapi/bookshelf");
    let path = if op == "add" { "add" } else { "delete" };
    let body = if op == "add" {
        json!({"add_book_source":0,"identify_data":[{"asterisked":false,"book_id":book_id,"book_type":0,"modify_time":0}]})
    } else {
        json!({"identify_data":[{"asterisked":false,"book_id":book_id,"book_type":0,"modify_time":ms_now()}]})
    };
    let url = format!("{base}/{path}/v?aid=1967");
    match page_post(&url, cookie, body.to_string().as_bytes()).await {
        Ok(_) => {
            let verb = if op == "add" { "已加入书架" } else { "已移出书架" };
            crate::emit_line(&format!("{GREEN}ok{RST} {verb} {BLUE}#{book_id}{RST}"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 在架检查：data=1 在架 / 0 不在架。
async fn shelf_check(cookie: &str, book_id: &str) {
    let url = format!("{FQ_WEB}/reading/bookapi/bookshelf/check/v:version/?aid=1967&iid=0&version_code=57700&update_version_code=57700&book_id={book_id}");
    match page_json(&url, cookie).await {
        Ok(v) => {
            let state = if v.get("data").and_then(|x| x.as_i64()) == Some(1) { "在架" } else { "不在架" };
            crate::emit_line(&format!("{GREEN}ok{RST} {BLUE}#{book_id}{RST} {state}"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 毫秒时间戳（delete 端点 modify_time 要求）。
fn ms_now() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis()).unwrap_or(0)
}

/// 数字字段兼容（JSON 数字或字符串数字，上游两种都有）。
fn num_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// 网页书架三端点合并：info（列表+分组）+ progress（进度）+ multidetail（书名/当前章节）。
async fn fetch_shelf_web(cookie: &str) -> Result<Value, String> {
    let ids = shelf_ids(cookie).await?;
    let prog = shelf_prog_map(cookie).await?;
    let det = shelf_det_map(cookie, &ids, &prog).await?;
    let books: Vec<Value> = ids.into_iter().map(|(id, group)| {
        let hit = prog.get(&id).cloned();
        let (name, chapter, rc, sc) = det.get(&id).cloned().unwrap_or_default();
        // 进度：已读章/总章优先，服务端 read_progress（万分比）兜底，都无 = 未读
        let pct = if rc > 0 && sc > 0 { Some(rc * 100 / sc) }
            else { hit.as_ref().map(|(_, p)| *p) };
        json!({"book_id": id, "book_name": name, "chapter": chapter,
               "pct": pct, "read_item_id": hit.map(|(i, _)| i).unwrap_or_default(),
               "group": group})
    }).collect();
    Ok(json!({"books": books}))
}

/// 书架列表：book_id + 分组名。
async fn shelf_ids(cookie: &str) -> Result<Vec<(String, String)>, String> {
    let url = format!("{FQ_WEB}/reading/bookapi/bookshelf/info/v:version/?aid=1967&iid=0&version_code=57700&update_version_code=57700");
    let v = page_json(&url, cookie).await?;
    let arr = v.pointer("/data/book_shelf_info").and_then(|x| x.as_array())
        .ok_or("书架结构未知")?;
    Ok(arr.iter().filter_map(|it| {
        let id = it.get("book_id")?.as_str()?.to_string();
        let group = it.get("group_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        Some((id, group))
    }).collect())
}

/// 阅读进度：book_id → (最后章节 item_id, 百分比)。
async fn shelf_prog_map(cookie: &str) -> Result<HashMap<String, (String, u64)>, String> {
    let v = page_json(&format!("{FQ_WEB}/api/reader/book/progress"), cookie).await?;
    let arr = v.get("data").and_then(|x| x.as_array()).ok_or("进度结构未知")?;
    Ok(arr.iter().filter_map(|it| {
        let id = it.get("book_id")?.as_str()?.to_string();
        let item = it.get("item_id").and_then(|x| x.as_str()).unwrap_or("0").to_string();
        let pct = it.get("read_progress").and_then(num_u64).unwrap_or(0) / 100;
        Some((id, (item, pct)))
    }).collect())
}

/// 书籍详情批量（multidetail 分批 20，item_id 用进度端点的最后章节）：
/// book_id → (书名, 当前章节名, 已读章序, 总章数)。
async fn shelf_det_map(cookie: &str, ids: &[(String, String)],
    prog: &HashMap<String, (String, u64)>) -> Result<HashMap<String, (String, String, u64, u64)>, String> {
    let mut map = HashMap::new();
    for chunk in ids.chunks(20) {
        let books: Vec<Value> = chunk.iter().map(|(id, _)| {
            let item = prog.get(id).map(|(i, _)| i.as_str()).unwrap_or("0");
            json!({"book_id": id, "item_id": item})
        }).collect();
        let v = page_post(&format!("{FQ_WEB}/api/bookshelf/multidetail"), cookie,
            json!({"books": books}).to_string().as_bytes()).await?;
        let arr = v.pointer("/data/detail_list").and_then(|x| x.as_array()).ok_or("详情结构未知")?;
        for it in arr {
            let Some(id) = it.get("book_id").and_then(|x| x.as_str()) else { continue };
            map.insert(id.to_string(), (
                it.get("book_name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                it.get("item_show_title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                it.get("real_chapter_order").and_then(num_u64).unwrap_or(0),
                it.get("serial_count").and_then(num_u64).unwrap_or(0),
            ));
        }
    }
    Ok(map)
}

/// 渲染网页书架：一行一本（书名 · 当前章节 · 进度% · #id · 分组）。
fn render_shelf(v: &Value, json: bool) {
    if json { return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default()); }
    let Some(arr) = v.get("books").and_then(|x| x.as_array()) else {
        return eprintln!("  {RED}err{RST} 书架结构未知（--json 查看原文）");
    };
    if arr.is_empty() {
        return crate::emit_line(&format!("  {DIM}书架为空（fqdt info <id> 后 account --add <id> 加书）{RST}"));
    }
    let mut n = 0;
    for it in arr {
        let s = |k: &str| it.get(k).and_then(|x| x.as_str()).unwrap_or("");
        if s("book_name").is_empty() { continue; }
        n += 1;
        let mut line = format!("  {CYAN}{}{RST}", s("book_name"));
        if !s("chapter").is_empty() { line += &format!(" · {}", s("chapter")); }
        match it.get("pct").and_then(|x| x.as_u64()) {
            Some(p) => line += &format!(" · {YELLOW}{p}%{RST}"),
            None => line += &format!(" · {DIM}未读{RST}"),
        }
        if !s("book_id").is_empty() { line += &format!(" {BLUE}#{}{RST}", s("book_id")); }
        if !s("group").is_empty() { line += &format!(" {DIM}· {}{RST}", s("group")); }
        crate::emit_line(&line);
    }
    crate::emit_line(&format!("{GREEN}ok{RST} 书架 {n} 本"));
    crate::emit_line(&format!("  {DIM}加: fqdt account --add <id> · 移出: --remove <id> · 检查: --check <id>{RST}"));
}

/// 按 section 组装请求并调 core 端点，返回响应 JSON。
async fn fetch_section(cookie: &str, section: &str) -> Result<Value, String> {
    if section == "history" {
        let body = json!({
            "cookie": cookie, "offset": 0,
            "last_min_read_timestamp_ms": 0, "is_first_load": true
        }).to_string().into_bytes();
        let reply = crate::fqapi::call("POST", "/api/v1/account/fanqie/history", "", body).await?;
        return reply.json();
    }
    let resource = match section {
        "shelf" => "shelf_list",
        "folders" => "folder_list",
        _ => "progress_list",
    };
    let body = json!({"cookie": cookie, "resource": resource, "offset": 0, "book_type": 0})
        .to_string().into_bytes();
    call_library_raw(body).await
}

async fn call_library_raw(body: Vec<u8>) -> Result<Value, String> {
    let reply = crate::fqapi::call("POST", "/api/v1/account/fanqie/library", "", body).await?;
    reply.json()
}

/// core 包装与上游错误统一判定。
fn upstream_ok(v: &Value) -> Result<(), String> {
    if v.get("success") == Some(&Value::Bool(false)) {
        return Err(v.get("error").and_then(|x| x.as_str()).unwrap_or("请求被拒绝").into());
    }
    if let Some(e) = v.get("error").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
        return Err(e.into());
    }
    let payload = match v.get("payload") {
        Some(Value::Null) | None => v, // history 端点无包装
        Some(p) => p,
    };
    if let Some(code) = payload.get("code").and_then(|x| x.as_i64()) {
        if code != 0 {
            let msg = payload.get("message").or_else(|| payload.get("msg"))
                .and_then(|x| x.as_str()).unwrap_or("上游拒绝该登录态");
            return Err(format!("{msg}（网页 Cookie 可能不兼容 App 接口）"));
        }
    }
    Ok(())
}

/// 渲染 section 结果：--json 原样；默认宽松提取书目数组逐行展示。
fn render_section(v: &Value, json: bool, section: &str) {
    if json {
        return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
    }
    if let Err(e) = upstream_ok(v) {
        return eprintln!("  {RED}err{RST} {e}");
    }
    let Some(arr) = find_books(v) else {
        crate::emit_line(&format!("  （{section} 无条目，或结构未知 → --json 查看原文）"));
        return;
    };
    if arr.is_empty() { return crate::emit_line(&format!("  （{section} 为空）")); }
    let mut n = 0;
    for item in arr {
        let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("");
        let name = if s("book_name").is_empty() { s("title") } else { s("book_name") };
        if name.is_empty() { continue; }
        let author = s("author");
        let id = item.get("book_id").or_else(|| item.get("item_id"))
            .and_then(|x| x.as_str().map(String::from).or_else(|| x.as_u64().map(|n| n.to_string())))
            .unwrap_or_default();
        n += 1;
        let author: String = author.chars().take(24).collect();
        let id_part = if id.is_empty() { String::new() } else { format!(" {BLUE}#{id}{RST}") };
        let author_part = if author.is_empty() { String::new() } else { format!(" {MAGENTA}{author}{RST}") };
        crate::emit_line(&format!("  {CYAN}{name}{RST}{author_part}{id_part}"));
    }
    crate::emit_line(&format!("{GREEN}ok{RST} {section} {n} 项"));
}

/// 递归找第一个含 book_name/title 的数组（上游各 resource 结构不一，宽松提取）。
fn find_books(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Array(a) => {
            if a.iter().any(|x| x.get("book_name").is_some() || x.get("title").is_some()) {
                return Some(a);
            }
            a.iter().find_map(find_books)
        }
        Value::Object(o) => o.values().find_map(find_books),
        _ => None,
    }
}

/// `fqdt account user`：真实用户资料（user/info/v2 + basic_info，网页端点）。
async fn render_user() {
    let Some(c) = config::load_cookie() else {
        crate::emit_line(&format!("  {YELLOW}未登录{RST}"));
        crate::emit_line(&format!("  {DIM}fqdt login '<Cookie>' 设置（浏览器登录 fanqienovel.com 复制）{RST}"));
        crate::emit_line(&format!("  {DIM}能力: shelf · folders · history · progress · highlights · user{RST}"));
        return;
    };
    util_progress("获取用户信息");
    let info = page_json(&format!("{FQ_WEB}/api/user/info/v2"), &c).await;
    let basic = page_json(&format!("{FQ_WEB}/reading/user/basic_info/get/v?aid=1967"), &c).await;
    util_clear();
    let Ok(v) = info else {
        let e = info.unwrap_err();
        eprintln!("  {RED}err{RST} {e}");
        crate::emit_line(&format!("  {DIM}能力: shelf · folders · history · progress · highlights{RST}"));
        return;
    };
    let d = v.get("data");
    let name = d.and_then(|x| x.get("name")).and_then(|x| x.as_str()).unwrap_or("?");
    let id = id_str(d);
    let desc = d.and_then(|x| x.get("desc")).and_then(|x| x.as_str()).unwrap_or("");
    let mut head = format!("{GREEN}ok{RST} {CYAN}{name}{RST} · {BLUE}#{id}{RST}");
    if let Ok(b) = &basic {
        let g = |k: &str| b.pointer(&format!("/data/{k}")).and_then(|x| x.as_i64()).unwrap_or(0);
        let fans = g("fans_num");
        let follow = g("follow_user_num");
        let read_num = g("read_book_num");
        let minutes = b.pointer("/data/user_read_time_tag/read_minute_total")
            .and_then(|x| x.as_i64()).unwrap_or(0);
        if fans + follow > 0 { head += &format!(" · 粉丝 {YELLOW}{fans}{RST} · 关注 {YELLOW}{follow}{RST}"); }
        if read_num > 0 { head += &format!(" · 已读 {YELLOW}{read_num}{RST} 本"); }
        if minutes > 0 { head += &format!(" · 累计 {YELLOW}{minutes}{RST} 分钟"); }
        if b.pointer("/data/is_author").and_then(|x| x.as_bool()) == Some(true) {
            head += &format!(" · {MAGENTA}作者{RST}");
        }
    }
    crate::emit_line(&head);
    if !desc.is_empty() {
        let desc: String = desc.chars().take(60).collect();
        crate::emit_line(&format!("  {DIM}{desc}{RST}"));
    }
    crate::emit_line(&format!("  {YELLOW}能力{RST} shelf 书架 · folders 书单 · history 阅读历史 · progress 阅读进度 · highlights 划线"));
    crate::emit_line(&format!("  {DIM}查询: fqdt account shelf --json 查看原始结构{RST}"));
}

/// `fqdt account highlights`：有划线/书签的书（bookmark/personal_list）。
fn render_highlights(v: &Value, json: bool) {
    if json {
        return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
    }
    let Some(arr) = v.pointer("/data/personal_bookmark_data").and_then(|x| x.as_array()) else {
        crate::emit_line("  （结构未知 → --json 查看原文）");
        return;
    };
    if arr.is_empty() { return crate::emit_line("  （暂无划线）"); }
    for row in arr {
        let item = row.get("data");
        let name = item.and_then(|x| x.get("book_name")).and_then(|x| x.as_str()).unwrap_or("?");
        let author = item.and_then(|x| x.get("author")).and_then(|x| x.as_str()).unwrap_or("");
        let num = |k: &str| row.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let (u, b, n) = (num("underline_num"), num("bookmark_num"), num("notes_num"));
        let author: String = author.chars().take(20).collect();
        let author_part = if author.is_empty() { String::new() } else { format!(" {MAGENTA}{author}{RST}") };
        crate::emit_line(&format!(
            "  {CYAN}{name}{RST}{author_part} {DIM}划线{RST} {YELLOW}{u}{RST} · {DIM}书签{RST} {YELLOW}{b}{RST} · {DIM}笔记{RST} {YELLOW}{n}{RST}"));
    }
    let total = v.pointer("/data/book_num").and_then(|x| x.as_i64()).unwrap_or(arr.len() as i64);
    crate::emit_line(&format!("{GREEN}ok{RST} highlights {n} 本（共 {total}）", n = arr.len()));
}

/// 进度提示（复用 util，非 TTY 静默）。
fn util_progress(label: &str) { crate::util::progress(label); }
fn util_clear() { crate::util::clear_progress(); }
