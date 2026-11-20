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

use crate::config;
use crate::util::{BLUE, CYAN, DIM, GREEN, MAGENTA, RED, RST, YELLOW};
use crate::Config;
use serde_json::{json, Value};

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
                if check { return verify_now().await; }
                crate::emit_line(&format!("  {DIM}验证: fqdt login --check · 查看: fqdt account user{RST}"));
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
            let id = d.and_then(|x| x.get("id")).and_then(|x| x.as_u64()).unwrap_or(0);
            crate::emit_line(&format!("{GREEN}ok{RST} 登录有效（{CYAN}{name}{RST} · {BLUE}#{id}{RST}）"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// GET 网页端点（带 Cookie）→ JSON + 业务码统一判定。
async fn page_json(url: &str, cookie: &str) -> Result<Value, String> {
    let text = crate::api::http_get_cookie(url, cookie).await?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "上游返回非 JSON".to_string())?;
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

/// `fqdt account [SECTION] [--json]`：个人数据只读展示。
pub async fn account(section: Option<String>, json: bool, _cfg: &Config) {
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
    let Some(cookie) = config::load_cookie() else {
        eprintln!("  {RED}err{RST} 未设置登录 Cookie");
        crate::emit_line(&format!("  {DIM}先 fqdt login '<Cookie>'（浏览器登录 fanqienovel.com 复制）{RST}"));
        return;
    };
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
    util_progress(&format!("获取 {sec}"));
    let res = fetch_section(&cookie, sec).await;
    util_clear();
    match res {
        Ok(v) => render_section(&v, json, sec),
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
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
    let id = d.and_then(|x| x.get("id")).and_then(|x| x.as_u64()).unwrap_or(0);
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
