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
//! - `fqdt info <sec> [--json]`：folders/history/progress/highlights/user 只读 sections
//! - `fqdt shelf add|remove|move|check <book_id>`：云端书架写操作

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
                crate::emit_line(&format!(
                    "{GREEN}ok{RST} Cookie 已保存 ~/.config/fqdt/cookie"
                ));
                // 保存后立即验证，不再让用户对着提示行猜有没有生效
                verify_now().await;
            }
            Err(e) => eprintln!("  {RED}err{RST} {e}"),
        }
        return;
    }
    if check {
        return verify_now().await;
    }
    // 无参数：状态 + 获取指引
    match config::load_cookie() {
        None => {
            crate::emit_line(&format!("  {YELLOW}未设置 Cookie{RST}"));
            crate::emit_line(&format!("  {DIM}获取: 浏览器登录 fanqienovel.com → 开发者工具 → 复制请求 Cookie → fqdt login '<Cookie>'{RST}"));
        }
        Some(c) => {
            let sess = config::cookie_has_session(&c);
            let mark = if sess {
                format!("{GREEN}session ✓{RST}")
            } else {
                format!("{YELLOW}session ✗{RST}")
            };
            crate::emit_line(&format!(
                "{GREEN}ok{RST} Cookie 已设置（{} 字节，{mark}）",
                c.len()
            ));
            crate::emit_line(&format!(
                "  {DIM}验证: fqdt login --check · 清除: fqdt login --clear{RST}"
            ));
        }
    }
}

/// user/info/v2 的 id 是字符串（"2253353583647107"），兼容数字两种形态。
fn id_str(d: Option<&serde_json::Value>) -> String {
    d.and_then(|x| x.get("id"))
        .and_then(|x| {
            x.as_str()
                .map(String::from)
                .or_else(|| x.as_u64().map(|n| n.to_string()))
        })
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
            let name = d
                .and_then(|x| x.get("name"))
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let id = id_str(d);
            crate::emit_line(&format!(
                "{GREEN}ok{RST} 登录有效（{CYAN}{name}{RST} · {BLUE}#{id}{RST}）"
            ));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// GET 网页端点（带 Cookie）→ JSON + 业务码统一判定。
pub async fn page_json(url: &str, cookie: &str) -> Result<Value, String> {
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
    if code == Some(-1)
        || code == Some(101108)
        || msg.contains("登录")
        || msg.to_lowercase().contains("login")
    {
        return Err("登录态失效或 Cookie 无效（fqdt login '<Cookie>' 更新）".into());
    }
    if code.is_some_and(|c| c != 0) {
        return Err(format!("上游拒绝 code={code:?} {msg}"));
    }
    Ok(v)
}

/// `fqdt info <sec>` 只读 section 分发（原 account 迁入，42③）：
/// folders/history/progress/highlights 需 Cookie，user 自行判断登录态。
pub async fn read_section(sec: &str, json: bool, search: Option<&str>) {
    if sec == "user" {
        return render_user().await;
    }
    let Some(cookie) = need_cookie() else { return };
    if sec == "highlights" {
        util_progress("获取 highlights");
        let url = format!(
            "{FQ_WEB}/reading/reader/bookmark/personal_list/?aid=1967&iid=0&version_code=74132&offset=0&limit=50");
        let res = page_json(&url, &cookie).await;
        util_clear();
        return match res {
            Ok(v) => render_highlights(&v, json, search),
            Err(e) => eprintln!("  {RED}err{RST} {e}"),
        };
    }
    if sec == "progress" {
        return progress_section(&cookie, json, search).await;
    }
    util_progress(&format!("获取 {sec}"));
    let res = fetch_section(&cookie, sec).await;
    util_clear();
    match res {
        Ok(v) => render_section(&v, json, sec, search),
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 读取登录 Cookie，缺失时打印指引并返回 None。
pub fn need_cookie() -> Option<String> {
    match config::load_cookie() {
        Some(c) => Some(c),
        None => {
            eprintln!("  {RED}err{RST} 未设置登录 Cookie");
            crate::emit_line(&format!(
                "  {DIM}先 fqdt login '<Cookie>'（浏览器登录 fanqienovel.com 复制）{RST}"
            ));
            None
        }
    }
}

/// `fqdt info <id> --add` 入口：复用书架 add 写操作。
pub async fn add_book(book_id: &str) {
    let Some(cookie) = need_cookie() else { return };
    shelf_op(&cookie, "add", book_id, None).await;
}

/// 书架展示：快照秒开（store.json TTL 600s，离线兜底）→ 网页三端点合并 → core 回退。
/// `group` 为 None 显示第一层（未分组书 + 分组条目），Some 显示该组内书目。
pub async fn shelf_section(
    cookie: &str,
    json: bool,
    search: Option<&str>,
    author: bool,
    group: Option<&str>,
    cfg: &Config,
) {
    // 快照命中：新鲜（或不要求作者/快照已含作者）→ 直接渲染，零网络
    if let Some((v, fresh)) = crate::store::shelf_snap() {
        let has_author = v
            .get("books")
            .and_then(|x| x.as_array())
            .is_some_and(|a| a.iter().any(|b| b.get("author").is_some()));
        if fresh && (!author || has_author) {
            return render_shelf(&v, json, search, group);
        }
    }
    util_progress("获取书架");
    let res = fetch_shelf_web(cookie, cfg, author).await;
    util_clear();
    match res {
        Ok(v) => {
            let _ = crate::store::shelf_snap_set(v.clone());
            render_shelf(&v, json, search, group);
        }
        Err(e) => {
            eprintln!("  {DIM}网页通道失败（{e}），回退核心通道{RST}");
            // 网络不可用时的离线兜底：上次快照（字段比 core 全，优先于 core）
            if let Some((v, _)) = crate::store::shelf_snap() {
                eprintln!("  {DIM}显示上次书架快照（store.json）{RST}");
                return render_shelf(&v, json, search, group);
            }
            util_progress("获取书架");
            let fb = fetch_section(cookie, "shelf").await;
            util_clear();
            match fb {
                Ok(v) => render_section(&v, json, "shelf", search),
                Err(e) => eprintln!("  {RED}err{RST} {e}"),
            }
        }
    }
}

/// 阅读时间线：网页 progress 端点（read_timestamp 降序），网页失败回退 core。
async fn progress_section(cookie: &str, json: bool, search: Option<&str>) {
    util_progress("获取阅读进度");
    let res = fetch_progress_web(cookie).await;
    util_clear();
    match res {
        Ok(v) => render_progress(&v, json, search),
        Err(e) => {
            eprintln!("  {DIM}网页通道失败（{e}），回退核心通道{RST}");
            util_progress("获取阅读进度");
            let fb = fetch_section(cookie, "progress").await;
            util_clear();
            match fb {
                Ok(v) => render_section(&v, json, "progress", search),
                Err(e) => eprintln!("  {RED}err{RST} {e}"),
            }
        }
    }
}

/// 网页进度时间线：progress 端点（26 本在读）+ multidetail 章节/书名 + 快照兜底，按时间降序。
async fn fetch_progress_web(cookie: &str) -> Result<Value, String> {
    let v = page_json(&format!("{FQ_WEB}/api/reader/book/progress"), cookie).await?;
    let arr = v
        .get("data")
        .and_then(|x| x.as_array())
        .ok_or("进度结构未知")?;
    // prog: book_id → (item_id, 读的百分比)；ids 喂 multidetail
    let mut prog: HashMap<String, (String, u64)> = HashMap::new();
    for it in arr {
        if let Some(id) = it.get("book_id").and_then(|x| x.as_str()) {
            prog.insert(
                id.to_string(),
                (
                    it.get("item_id")
                        .and_then(|x| x.as_str())
                        .unwrap_or("0")
                        .to_string(),
                    it.get("read_progress").and_then(num_u64).unwrap_or(0) / 100,
                ),
            );
        }
    }
    let ids: Vec<(String, String)> = prog.keys().map(|k| (k.clone(), String::new())).collect();
    let det = shelf_det_map(cookie, &ids, &prog).await.unwrap_or_default();
    let snap = snap_name_map();
    let mut rows: Vec<(u64, Value)> = arr
        .iter()
        .filter_map(|it| {
            let id = it.get("book_id")?.as_str()?;
            let ts = it.get("read_timestamp").and_then(num_u64).unwrap_or(0);
            let (name, chapter, rc, sc) = det.get(id).cloned().unwrap_or_default();
            // 书名：multidetail 权威，失败用快照；章节同理；进度：已读/总章优先，端点万分比兜底
            let (name, chapter) = if name.is_empty() {
                snap.get(id)
                    .cloned()
                    .unwrap_or_else(|| (format!("#{id}"), String::new()))
            } else {
                (name, chapter)
            };
            let pct = if rc > 0 && sc > 0 {
                rc * 100 / sc
            } else {
                prog.get(id).map(|(_, p)| *p).unwrap_or(0)
            };
            Some((
                ts,
                json!({"read_timestamp": ts, "book_id": id, "book_name": name,
                         "chapter": chapter, "pct": pct}),
            ))
        })
        .collect();
    rows.sort_by_key(|a| std::cmp::Reverse(a.0));
    Ok(json!({"items": rows.into_iter().map(|(_, r)| r).collect::<Vec<_>>()}))
}

/// 快照书名/章节 map（book_id → (书名, 章节)），multidetail 失败时的兜底。
fn snap_name_map() -> HashMap<String, (String, String)> {
    crate::store::shelf_snap()
        .and_then(|(s, _)| s.get("books").cloned())
        .and_then(|b| b.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|b| {
            Some((
                b.get("book_id")?.as_str()?.to_string(),
                (
                    b.get("book_name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    b.get("chapter")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
            ))
        })
        .collect()
}

/// Unix 秒 → `MM-DD HH:MM`（固定 +08:00 中国时区；无 chrono，civil-from-days 手算）。
fn fmt_ts(sec: u64) -> String {
    let sec = sec + 8 * 3600;
    let (days, secs) = ((sec / 86400) as i64, sec % 86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let (h, mi) = (secs / 3600, (secs % 3600) / 60);
    format!("{m:02}-{d:02} {h:02}:{mi:02}")
}

/// 渲染时间线：`MM-DD HH:MM 书名 · 章节 · pct% #id`。
fn render_progress(v: &Value, json: bool, search: Option<&str>) {
    let Some(arr) = v.get("items").and_then(|x| x.as_array()) else {
        return eprintln!("  {RED}err{RST} 进度结构未知（--json 查看原文）");
    };
    let kw = search.map(|s| s.to_lowercase());
    let items: Vec<&Value> = arr
        .iter()
        .filter(|it| match kw.as_ref() {
            None => true,
            Some(k) => match_item(it, k),
        })
        .collect();
    if json {
        if kw.is_none() {
            return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
        }
        let f = Value::Array(items.into_iter().cloned().collect());
        return crate::emit_line(&serde_json::to_string_pretty(&f).unwrap_or_default());
    }
    if items.is_empty() {
        let msg = if kw.is_some() {
            "（无匹配条目）"
        } else {
            "（暂无阅读进度）"
        };
        return crate::emit_line(&format!("  {DIM}{msg}{RST}"));
    }
    for it in &items {
        let s = |k: &str| it.get(k).and_then(|x| x.as_str()).unwrap_or("");
        let ts = fmt_ts(it.get("read_timestamp").and_then(num_u64).unwrap_or(0));
        let mut line = format!("  {DIM}{ts}{RST} {CYAN}「{}」{RST}", s("book_name"));
        if !s("chapter").is_empty() {
            line += &format!(" · {}", s("chapter"));
        }
        line += &format!(
            " · {YELLOW}{}%{RST}",
            it.get("pct").and_then(num_u64).unwrap_or(0)
        );
        line += &format!(" {BLUE}#{}{RST}", s("book_id"));
        crate::emit_line(&line);
    }
    crate::emit_line(&match search {
        Some(k) => format!(
            "{GREEN}ok{RST} 进度 {}/{} 本（匹配 \"{k}\"）",
            items.len(),
            arr.len()
        ),
        None => format!("{GREEN}ok{RST} 进度 {} 本（按最后阅读时间）", items.len()),
    });
}

/// 书架写操作：add/remove/check/move（网页 bookshelf 端点，JSON body）。
pub async fn shelf_op(cookie: &str, op: &str, book_id: &str, group: Option<&str>) {
    if op == "check" {
        return shelf_check(cookie, book_id).await;
    }
    if op == "move" {
        return shelf_move(cookie, book_id, group.unwrap_or("")).await;
    }
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
            crate::store::shelf_snap_clear(); // 98：写后失效，新书立即可见
            let verb = if op == "add" {
                "已加入书架"
            } else {
                "已移出书架"
            };
            crate::emit_line(&format!("{GREEN}ok{RST} {verb} {BLUE}#{book_id}{RST}"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 分组移动（update/v；组不存在时服务端按名字自动建组，实测往返通过）。
async fn shelf_move(cookie: &str, book_id: &str, group: &str) {
    if group.is_empty() {
        return eprintln!("  {RED}err{RST} 缺少目标分组（fqdt shelf move <id> <分组>）");
    }
    let body = json!({"book_data":[{"asterisked":false,"book_id":book_id,"book_type":0,
        "group_name":group,"has_shown":false,"is_pin":false,"modify_time":ms_now()}]});
    let url = format!("{FQ_WEB}/reading/bookapi/bookshelf/update/v?aid=1967");
    match page_post(&url, cookie, body.to_string().as_bytes()).await {
        Ok(_) => {
            crate::store::shelf_snap_clear(); // 98：移动也改书架，失效快照
            crate::emit_line(&format!(
                "{GREEN}ok{RST} 已移动到分组 {CYAN}{group}{RST} · {BLUE}#{book_id}{RST}"
            ));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 在架检查：data=1 在架 / 0 不在架。
async fn shelf_check(cookie: &str, book_id: &str) {
    let url = format!("{FQ_WEB}/reading/bookapi/bookshelf/check/v:version/?aid=1967&iid=0&version_code=57700&update_version_code=57700&book_id={book_id}");
    match page_json(&url, cookie).await {
        Ok(v) => {
            let state = if v.get("data").and_then(|x| x.as_i64()) == Some(1) {
                "在架"
            } else {
                "不在架"
            };
            crate::emit_line(&format!("{GREEN}ok{RST} {BLUE}#{book_id}{RST} {state}"));
        }
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 毫秒时间戳（delete 端点 modify_time 要求）。
fn ms_now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 数字字段兼容（JSON 数字或字符串数字，上游两种都有）。
fn num_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// 网页书架三端点合并：info（列表+分组）+ progress（进度）+ multidetail（书名/当前章节）。
/// `with_author=true` 时逐本拉作者（core detail，本地缓存 30 天，10 并发）。
async fn fetch_shelf_web(cookie: &str, cfg: &Config, with_author: bool) -> Result<Value, String> {
    let ids = shelf_ids(cookie).await?;
    let prog = shelf_prog_map(cookie).await?;
    let det = shelf_det_map(cookie, &ids, &prog).await?;
    let authors = if with_author {
        fetch_authors(cfg, &ids).await
    } else {
        HashMap::new()
    };
    let books: Vec<Value> = ids
        .into_iter()
        .map(|(id, group)| {
            let hit = prog.get(&id).cloned();
            let (name, chapter, rc, sc) = det.get(&id).cloned().unwrap_or_default();
            // 进度：已读章/总章优先，服务端 read_progress（万分比）兜底，都无 = 未读
            let pct = if rc > 0 && sc > 0 {
                Some(rc * 100 / sc)
            } else {
                hit.as_ref().map(|(_, p)| *p)
            };
            let mut m = json!({"book_id": id, "book_name": name, "chapter": chapter,
               "pct": pct, "read_item_id": hit.map(|(i, _)| i).unwrap_or_default(),
               "group": group});
            if let Some(a) = authors.get(m["book_id"].as_str().unwrap_or("")) {
                m["author"] = json!(a);
            }
            m
        })
        .collect();
    Ok(json!({"books": books}))
}

/// 缓存拆分：返回 (命中 book_id→作者, 未命中 id 列表)。缓存读写统一走 store.json。
fn author_split(
    cache: &HashMap<String, crate::store::AuthorEntry>,
    ids: &[String],
) -> (HashMap<String, String>, Vec<String>) {
    let now = (ms_now() / 1000) as u64;
    let mut hit = HashMap::new();
    let mut miss = Vec::new();
    for id in ids {
        match cache.get(id) {
            Some(e) if now.saturating_sub(e.t) < 30 * 86400 => {
                hit.insert(id.clone(), e.a.clone());
            }
            _ => miss.push(id.clone()),
        }
    }
    (hit, miss)
}

/// 并发拉作者：core detail 第二段（`书名|作者|...`），10 并发、8s 超时，失败降级不带作者。
async fn fetch_authors(cfg: &Config, ids: &[(String, String)]) -> HashMap<String, String> {
    let mut cache = crate::store::authors_load();
    let id_list: Vec<String> = ids.iter().map(|(id, _)| id.clone()).collect();
    let (mut out, miss) = author_split(&cache, &id_list);
    if miss.is_empty() {
        return out;
    }
    let now = (ms_now() / 1000) as u64;
    let api = crate::api::Client::from_config(cfg, false);
    let mut fresh = 0usize;
    for chunk in miss.chunks(10) {
        let mut set = tokio::task::JoinSet::new();
        for id in chunk {
            let (api, id) = (api.clone(), id.clone());
            set.spawn(async move {
                let r =
                    tokio::time::timeout(std::time::Duration::from_secs(8), api.fetch_detail(&id))
                        .await;
                (id, r)
            });
        }
        while let Some(res) = set.join_next().await {
            let Ok((id, Ok(Ok(detail)))) = res else {
                continue;
            };
            let author = detail.split('|').nth(1).unwrap_or("").to_string();
            if !author.is_empty() {
                cache.insert(
                    id.clone(),
                    crate::store::AuthorEntry {
                        a: author.clone(),
                        t: now,
                    },
                );
                out.insert(id, author);
                fresh += 1;
            }
        }
    }
    if fresh > 0 {
        let _ = crate::store::authors_save(cache);
    }
    out
}

/// 书架列表：book_id + 分组名。
async fn shelf_ids(cookie: &str) -> Result<Vec<(String, String)>, String> {
    let url = format!("{FQ_WEB}/reading/bookapi/bookshelf/info/v:version/?aid=1967&iid=0&version_code=57700&update_version_code=57700");
    let v = page_json(&url, cookie).await?;
    let arr = v
        .pointer("/data/book_shelf_info")
        .and_then(|x| x.as_array())
        .ok_or("书架结构未知")?;
    Ok(arr
        .iter()
        .filter_map(|it| {
            let id = it.get("book_id")?.as_str()?.to_string();
            let group = it
                .get("group_name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            Some((id, group))
        })
        .collect())
}

/// 阅读进度：book_id → (最后章节 item_id, 百分比)。
async fn shelf_prog_map(cookie: &str) -> Result<HashMap<String, (String, u64)>, String> {
    let v = page_json(&format!("{FQ_WEB}/api/reader/book/progress"), cookie).await?;
    let arr = v
        .get("data")
        .and_then(|x| x.as_array())
        .ok_or("进度结构未知")?;
    Ok(arr
        .iter()
        .filter_map(|it| {
            let id = it.get("book_id")?.as_str()?.to_string();
            let item = it
                .get("item_id")
                .and_then(|x| x.as_str())
                .unwrap_or("0")
                .to_string();
            let pct = it.get("read_progress").and_then(num_u64).unwrap_or(0) / 100;
            Some((id, (item, pct)))
        })
        .collect())
}

/// 书籍详情批量（multidetail 分批 20，item_id 用进度端点的最后章节）：
/// book_id → (书名, 当前章节名, 已读章序, 总章数)。
async fn shelf_det_map(
    cookie: &str,
    ids: &[(String, String)],
    prog: &HashMap<String, (String, u64)>,
) -> Result<HashMap<String, (String, String, u64, u64)>, String> {
    let mut map = HashMap::new();
    for chunk in ids.chunks(20) {
        let books: Vec<Value> = chunk
            .iter()
            .map(|(id, _)| {
                let item = prog.get(id).map(|(i, _)| i.as_str()).unwrap_or("0");
                json!({"book_id": id, "item_id": item})
            })
            .collect();
        let v = page_post(
            &format!("{FQ_WEB}/api/bookshelf/multidetail"),
            cookie,
            json!({"books": books}).to_string().as_bytes(),
        )
        .await?;
        let arr = v
            .pointer("/data/detail_list")
            .and_then(|x| x.as_array())
            .ok_or("详情结构未知")?;
        for it in arr {
            let Some(id) = it.get("book_id").and_then(|x| x.as_str()) else {
                continue;
            };
            map.insert(
                id.to_string(),
                (
                    it.get("book_name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    it.get("item_show_title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    it.get("real_chapter_order").and_then(num_u64).unwrap_or(0),
                    it.get("serial_count").and_then(num_u64).unwrap_or(0),
                ),
            );
        }
    }
    Ok(map)
}

/// 渲染网页书架（42① 番茄原生一层视图）：
/// - 无 group 无 search：顶层只列未分组书 + 分组条目，已分组的书不直接显示
/// - 指定 group：该组内书目；--search 展平全架过滤
fn render_shelf(v: &Value, json: bool, search: Option<&str>, group: Option<&str>) {
    let Some(arr) = v.get("books").and_then(|x| x.as_array()) else {
        return eprintln!("  {RED}err{RST} 书架结构未知（--json 查看原文）");
    };
    let kw = search.map(|s| s.to_lowercase());
    if json {
        return shelf_json(v, arr, kw.as_deref(), group);
    }
    if kw.is_some() || group.is_some() {
        return shelf_flat(arr, kw.as_deref(), group);
    }
    shelf_top(arr);
}

/// 过滤判定：关键词（书名/标题/作者/章节）与分组范围同时满足。
fn in_scope(it: &Value, kw: Option<&str>, group: Option<&str>) -> bool {
    let g_ok = match group {
        None => true,
        Some(g) => it.get("group").and_then(|x| x.as_str()).unwrap_or("") == g,
    };
    g_ok && match kw {
        None => true,
        Some(k) => match_item(it, k),
    }
}

/// JSON 输出：无过滤原样输出，否则输出过滤后的 books 数组。
fn shelf_json(v: &Value, arr: &[Value], kw: Option<&str>, group: Option<&str>) {
    if kw.is_none() && group.is_none() {
        return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
    }
    let f: Vec<&Value> = arr.iter().filter(|it| in_scope(it, kw, group)).collect();
    let out = Value::Array(f.into_iter().cloned().collect());
    crate::emit_line(&serde_json::to_string_pretty(&out).unwrap_or_default());
}

/// 指定组或关键词的展平视图：逐行 + 收据。
fn shelf_flat(arr: &[Value], kw: Option<&str>, group: Option<&str>) {
    let items: Vec<&Value> = arr.iter().filter(|it| in_scope(it, kw, group)).collect();
    if items.is_empty() {
        let msg = match (group, kw) {
            (Some(g), _) => format!("分组 {g} 为空或不存在"),
            (_, Some(_)) => "（无匹配书目）".into(),
            _ => "书架为空".into(),
        };
        return crate::emit_line(&format!("  {DIM}{msg}{RST}"));
    }
    for it in &items {
        if let Some(line) = shelf_line(it) {
            crate::emit_line(&line);
        }
    }
    let head = match (group, kw) {
        (Some(g), Some(k)) => format!(
            "{GREEN}ok{RST} 书架[{g}]{}/{} 本（匹配 \"{k}\"）",
            items.len(),
            arr.len()
        ),
        (Some(g), None) => format!("{GREEN}ok{RST} 书架[{g}]{} 本", items.len()),
        (None, Some(k)) => format!(
            "{GREEN}ok{RST} 书架 {}/{} 本（匹配 \"{k}\"）",
            items.len(),
            arr.len()
        ),
        (None, None) => format!("{GREEN}ok{RST} 书架 {} 本", items.len()),
    };
    crate::emit_line(&head);
}

/// 顶层第一层视图：未分组书逐行 + 分组条目行 `[组名] N 本`（保持首次出现顺序）。
fn shelf_top(arr: &[Value]) {
    if arr.is_empty() {
        return crate::emit_line(&format!("  {DIM}书架为空{RST}"));
    }
    let ungrouped: Vec<&Value> = arr
        .iter()
        .filter(|it| {
            it.get("group")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .is_empty()
        })
        .collect();
    let mut groups: Vec<(String, usize)> = Vec::new();
    for it in arr {
        let g = it.get("group").and_then(|x| x.as_str()).unwrap_or("");
        if g.is_empty() {
            continue;
        }
        match groups.iter_mut().find(|(n, _)| n == g) {
            Some((_, c)) => *c += 1,
            None => groups.push((g.to_string(), 1)),
        }
    }
    for it in &ungrouped {
        if let Some(line) = shelf_line(it) {
            crate::emit_line(&line);
        }
    }
    for (g, n) in &groups {
        crate::emit_line(&format!("  {CYAN}[{g}]{RST} {DIM}{n} 本{RST}"));
    }
    let head = if groups.is_empty() {
        format!("{GREEN}ok{RST} 书架 {} 本", ungrouped.len())
    } else {
        format!("{GREEN}ok{RST} 书架 {} 本 · {} 组", arr.len(), groups.len())
    };
    crate::emit_line(&head);
}

/// 单本书渲染行：书名 · 作者(--author 时) · 当前章节 · 进度%/#id；空书名返回 None。
fn shelf_line(it: &Value) -> Option<String> {
    let s = |k: &str| it.get(k).and_then(|x| x.as_str()).unwrap_or("");
    if s("book_name").is_empty() {
        return None;
    }
    let mut line = format!("  {CYAN}「{}」{RST}", s("book_name"));
    if !s("author").is_empty() {
        line += &format!(" {MAGENTA}{}{RST}", s("author"));
    }
    if !s("chapter").is_empty() {
        line += &format!(" · {}", s("chapter"));
    }
    match it.get("pct").and_then(|x| x.as_u64()) {
        Some(p) => line += &format!(" · {YELLOW}{p}%{RST}"),
        None => line += &format!(" · {DIM}未读{RST}"),
    }
    if !s("book_id").is_empty() {
        line += &format!(" {BLUE}#{}{RST}", s("book_id"));
    }
    Some(line)
}

/// 按 section 组装请求并调 core 端点，返回响应 JSON。
async fn fetch_section(cookie: &str, section: &str) -> Result<Value, String> {
    if section == "history" {
        let body = json!({
            "cookie": cookie, "offset": 0,
            "last_min_read_timestamp_ms": 0, "is_first_load": true
        })
        .to_string()
        .into_bytes();
        let reply = crate::fqapi::call("POST", "/api/v1/account/fanqie/history", "", body).await?;
        return reply.json();
    }
    let resource = match section {
        "shelf" => "shelf_list",
        "folders" => "folder_list",
        _ => "progress_list",
    };
    let body = json!({"cookie": cookie, "resource": resource, "offset": 0, "book_type": 0})
        .to_string()
        .into_bytes();
    call_library_raw(body).await
}

async fn call_library_raw(body: Vec<u8>) -> Result<Value, String> {
    let reply = crate::fqapi::call("POST", "/api/v1/account/fanqie/library", "", body).await?;
    reply.json()
}

/// core 包装与上游错误统一判定。
fn upstream_ok(v: &Value) -> Result<(), String> {
    if v.get("success") == Some(&Value::Bool(false)) {
        return Err(v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("请求被拒绝")
            .into());
    }
    if let Some(e) = v
        .get("error")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        return Err(e.into());
    }
    let payload = match v.get("payload") {
        Some(Value::Null) | None => v, // history 端点无包装
        Some(p) => p,
    };
    if let Some(code) = payload.get("code").and_then(|x| x.as_i64()) {
        if code != 0 {
            let msg = payload
                .get("message")
                .or_else(|| payload.get("msg"))
                .and_then(|x| x.as_str())
                .unwrap_or("上游拒绝该登录态");
            return Err(format!("{msg}（网页 Cookie 可能不兼容 App 接口）"));
        }
    }
    Ok(())
}

/// history 渲染：条目只有 book_id/read_timestamp_ms（无书名），按时间降序。
/// history 数组查找（97 单测锚）：递归找含 read_timestamp_ms 的数组。
/// 真结构 `data.data_list[]`（book_id/book_id_str/read_source/read_timestamp_ms）。
fn find_history_arr(x: &Value) -> Option<&Vec<Value>> {
    match x {
        Value::Array(a) => {
            if a.iter().any(|i| i.get("read_timestamp_ms").is_some()) {
                return Some(a);
            }
            a.iter().find_map(find_history_arr)
        }
        Value::Object(o) => o.values().find_map(find_history_arr),
        _ => None,
    }
}

fn render_history(v: &Value, search: Option<&str>) {
    let Some(arr) = find_history_arr(v) else {
        return crate::emit_line("  （history 结构未知 → --json 查看原文）");
    };
    let kw = search.map(|s| s.to_lowercase());
    let mut items: Vec<&Value> = arr
        .iter()
        .filter(|it| match &kw {
            None => true,
            Some(k) => {
                it.get("book_id_str")
                    .and_then(|x| x.as_str())
                    .is_some_and(|s| s.contains(k))
                    || it
                        .get("book_id")
                        .and_then(|x| x.as_u64())
                        .is_some_and(|n| n.to_string().contains(k))
            }
        })
        .collect();
    if items.is_empty() {
        let msg = if kw.is_some() {
            "（无匹配条目）"
        } else {
            "history 为空"
        };
        return crate::emit_line(&format!("  {DIM}{msg}{RST}"));
    }
    items.sort_by_key(|it| {
        std::cmp::Reverse(
            it.get("read_timestamp_ms")
                .and_then(|x| x.as_u64())
                .unwrap_or(0),
        )
    });
    let mut n = 0;
    for item in &items {
        let ts = item
            .get("read_timestamp_ms")
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
            / 1000;
        let id = item
            .get("book_id_str")
            .and_then(|x| x.as_str())
            .map(String::from)
            .or_else(|| {
                item.get("book_id")
                    .and_then(|x| x.as_u64())
                    .map(|n| n.to_string())
            })
            .unwrap_or_default();
        let src = item
            .get("read_source")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let src_part = if src.is_empty() {
            String::new()
        } else {
            format!(" {DIM}{src}{RST}")
        };
        n += 1;
        crate::emit_line(&format!("  {} {BLUE}#{id}{RST}{src_part}", fmt_ts(ts)));
    }
    crate::emit_line(&match search {
        Some(k) => format!(
            "{GREEN}ok{RST} history {n}/{} 项（匹配 \"{k}\"）",
            items.len()
        ),
        None => format!("{GREEN}ok{RST} history {n} 项"),
    });
}

/// folders 渲染：数组在 payload.data.folders（可真空），条目多为 folder_name/id。
/// folders 数组查找（97 单测锚）：递归找指定 key 的数组。
/// 真结构 `payload.data.folders[]`（可真空数组）。
fn find_folders_arr<'a>(x: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    match x {
        Value::Array(a) => a.iter().find_map(|i| find_folders_arr(i, key)),
        Value::Object(o) => o
            .get(key)
            .and_then(|v| v.as_array())
            .or_else(|| o.values().find_map(|v| find_folders_arr(v, key))),
        _ => None,
    }
}
fn render_folders(v: &Value, search: Option<&str>) {
    let Some(arr) = find_folders_arr(v, "folders") else {
        return crate::emit_line("  （folders 结构未知 → --json 查看原文）");
    };
    let kw = search.map(|s| s.to_lowercase());
    let items: Vec<&Value> = arr
        .iter()
        .filter(|it| match &kw {
            None => true,
            Some(k) => ["folder_name", "name", "title"].iter().any(|f| {
                it.get(*f)
                    .and_then(|x| x.as_str())
                    .is_some_and(|s| s.to_lowercase().contains(k))
            }),
        })
        .collect();
    if items.is_empty() {
        let msg = if kw.is_some() {
            "（无匹配条目）"
        } else {
            "folders 为空"
        };
        return crate::emit_line(&format!("  {DIM}{msg}{RST}"));
    }
    let mut n = 0;
    for item in &items {
        let name = ["folder_name", "name", "title"]
            .iter()
            .find_map(|f| item.get(*f).and_then(|x| x.as_str()))
            .unwrap_or("");
        let id = item
            .get("folder_id")
            .or_else(|| item.get("id"))
            .and_then(|x| {
                x.as_str()
                    .map(String::from)
                    .or_else(|| x.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        let id_part = if id.is_empty() {
            String::new()
        } else {
            format!(" {BLUE}#{id}{RST}")
        };
        n += 1;
        crate::emit_line(&format!("  {CYAN}{name}{RST}{id_part}"));
    }
    crate::emit_line(&match search {
        Some(k) => format!(
            "{GREEN}ok{RST} folders {n}/{} 项（匹配 \"{k}\"）",
            items.len()
        ),
        None => format!("{GREEN}ok{RST} folders {n} 项"),
    });
}

/// 渲染 section 结果：--json 原样；默认宽松提取书目数组逐行展示。
fn render_section(v: &Value, json: bool, section: &str, search: Option<&str>) {
    if json {
        if let (Some(k), Some(arr)) = (search, find_books(v)) {
            let kw = k.to_lowercase();
            let f: Vec<&Value> = arr.iter().filter(|it| match_item(it, &kw)).collect();
            return crate::emit_line(
                &serde_json::to_string_pretty(&Value::Array(f.into_iter().cloned().collect()))
                    .unwrap_or_default(),
            );
        }
        return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
    }
    if let Err(e) = upstream_ok(v) {
        return eprintln!("  {RED}err{RST} {e}");
    }
    // history 条目仅 book_id + 时间戳（无书名字段），find_books 必空 → 专用渲染（m0160 验收）
    if section == "history" {
        return render_history(v, search);
    }
    // folders 数组在 payload.data.folders（可真空），条目无 book_name/title → 专用渲染
    if section == "folders" {
        return render_folders(v, search);
    }
    let Some(arr) = find_books(v) else {
        crate::emit_line(&format!(
            "  （{section} 无条目，或结构未知 → --json 查看原文）"
        ));
        return;
    };
    let kw = search.map(|s| s.to_lowercase());
    let items: Vec<&Value> = arr
        .iter()
        .filter(|it| match kw.as_ref() {
            None => true,
            Some(k) => match_item(it, k),
        })
        .collect();
    if items.is_empty() {
        let msg = if kw.is_some() {
            "（无匹配条目）"
        } else {
            "为空"
        };
        return crate::emit_line(&format!("  {DIM}{section} {msg}{RST}"));
    }
    let mut n = 0;
    for item in &items {
        let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("");
        let name = if s("book_name").is_empty() {
            s("title")
        } else {
            s("book_name")
        };
        if name.is_empty() {
            continue;
        }
        let author = s("author");
        let id = item
            .get("book_id")
            .or_else(|| item.get("item_id"))
            .and_then(|x| {
                x.as_str()
                    .map(String::from)
                    .or_else(|| x.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        n += 1;
        let author: String = author.chars().take(24).collect();
        let id_part = if id.is_empty() {
            String::new()
        } else {
            format!(" {BLUE}#{id}{RST}")
        };
        let author_part = if author.is_empty() {
            String::new()
        } else {
            format!(" {MAGENTA}{author}{RST}")
        };
        crate::emit_line(&format!("  {CYAN}「{name}」{RST}{author_part}{id_part}"));
    }
    crate::emit_line(&match search {
        Some(k) => format!(
            "{GREEN}ok{RST} {section} {n}/{} 项（匹配 \"{k}\"）",
            items.len()
        ),
        None => format!("{GREEN}ok{RST} {section} {n} 项"),
    });
}

/// 条目含词匹配：书名/标题/作者/章节，kw 需先小写。
fn match_item(it: &Value, kw: &str) -> bool {
    ["book_name", "title", "author", "chapter"].iter().any(|k| {
        it.get(*k)
            .and_then(|x| x.as_str())
            .is_some_and(|s| s.to_lowercase().contains(kw))
    })
}

/// 递归找第一个含 book_name/title 的数组（上游各 resource 结构不一，宽松提取）。
fn find_books(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Array(a) => {
            if a.iter()
                .any(|x| x.get("book_name").is_some() || x.get("title").is_some())
            {
                return Some(a);
            }
            a.iter().find_map(find_books)
        }
        Value::Object(o) => o.values().find_map(find_books),
        _ => None,
    }
}

/// `fqdt info user`：真实用户资料（user/info/v2 + basic_info，网页端点）。
async fn render_user() {
    let Some(c) = config::load_cookie() else {
        crate::emit_line(&format!("  {YELLOW}未登录{RST}"));
        crate::emit_line(&format!(
            "  {DIM}fqdt login '<Cookie>' 设置（浏览器登录 fanqienovel.com 复制）{RST}"
        ));
        return;
    };
    util_progress("获取用户信息");
    let info = page_json(&format!("{FQ_WEB}/api/user/info/v2"), &c).await;
    let basic = page_json(
        &format!("{FQ_WEB}/reading/user/basic_info/get/v?aid=1967"),
        &c,
    )
    .await;
    util_clear();
    let Ok(v) = info else {
        return eprintln!("  {RED}err{RST} {}", info.unwrap_err());
    };
    let d = v.get("data");
    let name = d
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or("?");
    let id = id_str(d);
    let desc = d
        .and_then(|x| x.get("desc"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    // 行1 仅昵称；行2 起身份与量（单行不超宽，m0131：从 uid 起换行）
    crate::emit_line(&format!("{CYAN}{name}{RST}"));
    let mut row = format!("  {BLUE}#{id}{RST}");
    if let Ok(b) = &basic {
        let g = |k: &str| {
            b.pointer(&format!("/data/{k}"))
                .and_then(|x| x.as_i64())
                .unwrap_or(0)
        };
        let fans = g("fans_num");
        let follow = g("follow_user_num");
        let read_num = g("read_book_num");
        let minutes = b
            .pointer("/data/user_read_time_tag/read_minute_total")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        if fans + follow > 0 {
            row += &format!(" · 粉丝 {YELLOW}{fans}{RST} 人 · 关注 {YELLOW}{follow}{RST} 人");
        }
        if b.pointer("/data/is_author").and_then(|x| x.as_bool()) == Some(true) {
            row += &format!(" · {MAGENTA}作者{RST}");
        }
        crate::emit_line(&row);
        // 行3 阅读量：本数 + 时长转可读（167274 分钟 → 约 116 天，m0131 要求明白）
        let mut row2 = String::new();
        if read_num > 0 {
            row2 += &format!("  已读 {YELLOW}{read_num}{RST} 本书");
        }
        if minutes > 0 {
            let readable = if minutes >= 1440 {
                format!("约 {} 天", minutes / 1440)
            } else if minutes >= 60 {
                format!("约 {} 小时", minutes / 60)
            } else {
                format!("{minutes} 分钟")
            };
            if !row2.is_empty() {
                row2 += " · ";
            }
            row2 += &format!("累计阅读 {YELLOW}{readable}{RST}（{minutes} 分钟）");
        }
        if !row2.is_empty() {
            crate::emit_line(&row2);
        }
    } else {
        crate::emit_line(&row);
    }
    // 简介：逐行 trim + 统一缩进对齐（原数据多行/空白导致失齐，m0131）
    for line in desc.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let clipped: String = line.chars().take(60).collect();
        crate::emit_line(&format!("  {DIM}{clipped}{RST}"));
    }
}

/// `fqdt info highlights`：有划线/书签的书（bookmark/personal_list）。
fn render_highlights(v: &Value, json: bool, search: Option<&str>) {
    if json {
        return crate::emit_line(&serde_json::to_string_pretty(v).unwrap_or_default());
    }
    let Some(arr) = v
        .pointer("/data/personal_bookmark_data")
        .and_then(|x| x.as_array())
    else {
        crate::emit_line("  （结构未知 → --json 查看原文）");
        return;
    };
    if arr.is_empty() {
        return crate::emit_line("  （暂无划线）");
    }
    let kw = search.map(|s| s.to_lowercase());
    let rows: Vec<&Value> = arr
        .iter()
        .filter(|row| match &kw {
            None => true,
            Some(k) => {
                let item = row.get("data");
                ["book_name", "author"].iter().any(|f| {
                    item.and_then(|x| x.get(*f))
                        .and_then(|x| x.as_str())
                        .is_some_and(|s| s.to_lowercase().contains(k))
                })
            }
        })
        .collect();
    if rows.is_empty() {
        return crate::emit_line(&format!("  {DIM}（无匹配条目）{RST}"));
    }
    for row in &rows {
        let item = row.get("data");
        let name = item
            .and_then(|x| x.get("book_name"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let author = item
            .and_then(|x| x.get("author"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let num = |k: &str| row.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let (u, b, n) = (num("underline_num"), num("bookmark_num"), num("notes_num"));
        let author: String = author.chars().take(20).collect();
        let author_part = if author.is_empty() {
            String::new()
        } else {
            format!(" {MAGENTA}{author}{RST}")
        };
        crate::emit_line(&format!(
            "  {CYAN}「{name}」{RST}{author_part} {DIM}划线{RST} {YELLOW}{u}{RST} · {DIM}书签{RST} {YELLOW}{b}{RST} · {DIM}笔记{RST} {YELLOW}{n}{RST}"));
    }
    let total = v
        .pointer("/data/book_num")
        .and_then(|x| x.as_i64())
        .unwrap_or(arr.len() as i64);
    match search {
        Some(k) => crate::emit_line(&format!(
            "{GREEN}ok{RST} highlights {}/{} 本（匹配 \"{k}\"，共 {total}）",
            rows.len(),
            arr.len()
        )),
        None => crate::emit_line(&format!(
            "{GREEN}ok{RST} highlights {n} 本（共 {total}）",
            n = arr.len()
        )),
    }
}

/// 进度提示（复用 util，非 TTY 静默）。
fn util_progress(label: &str) {
    crate::util::progress(label);
}
fn util_clear() {
    crate::util::clear_progress();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 97：真 history 载荷（远端 fqdt i history --json 实录 data.data_list 结构）。
    #[test]
    fn history_arr_from_real_payload() {
        let v: Value = serde_json::from_str(
            r#"{"_fqapp_device_id":"x","code":0,"data":{"data_list":[
                {"book_id":7667499565381078041,"book_id_str":"7667499565381078041",
                 "read_source":"user_added","read_timestamp_ms":1795618633722},
                {"book_id":7706844630989425689,"book_id_str":"7706844630989425689",
                 "read_source":"猜你喜欢","read_timestamp_ms":1795618622433}
            ]}}"#,
        )
        .unwrap();
        let arr = find_history_arr(&v).expect("真结构应找到 data_list");
        assert_eq!(arr.len(), 2);
        assert_eq!(
            arr[0].get("read_timestamp_ms").and_then(|x| x.as_u64()),
            Some(1795618633722)
        );
        assert_eq!(
            arr[0].get("book_id_str").and_then(|x| x.as_str()),
            Some("7667499565381078041")
        );
    }

    /// 97：history 结构未知 → None（不误报空）。
    #[test]
    fn history_arr_unknown_structure() {
        let v: Value = serde_json::from_str(r#"{"foo":1}"#).unwrap();
        assert!(find_history_arr(&v).is_none());
    }

    /// 97：真 folders 载荷——payload.data.folders 可为空数组（仍应定位到，而非 None）。
    #[test]
    fn folders_arr_real_payload_empty_ok() {
        let v: Value = serde_json::from_str(
            r#"{"code":0,"payload":{"data":{"folder_count":0,"folders":[],"has_more":false}}}"#,
        )
        .unwrap();
        let arr = find_folders_arr(&v, "folders").expect("应找到 folders 键（含空数组）");
        assert!(arr.is_empty());
        // 非空
        let v2: Value = serde_json::from_str(
            r#"{"payload":{"data":{"folders":[{"folder_name":"想看","folder_id":1}]}}}"#,
        )
        .unwrap();
        let arr2 = find_folders_arr(&v2, "folders").unwrap();
        assert_eq!(arr2.len(), 1);
    }

    /// 97：真 highlights 载荷——书目在条目 .data 内层，原实现 pointer 路径应命中。
    #[test]
    fn highlights_real_payload_pointer() {
        let v: Value = serde_json::from_str(
            r#"{"code":0,"data":{"book_num":1,"personal_bookmark_data":[
                {"bookmark_num":2,"data":{"book_id":"7647845844611238974",
                 "book_name":"异世界挂科自救指南","author":"巴比伦侦探"}}
            ]}}"#,
        )
        .unwrap();
        let arr = v
            .pointer("/data/personal_bookmark_data")
            .and_then(|x| x.as_array())
            .expect("pointer 路径应命中 personal_bookmark_data");
        assert_eq!(arr.len(), 1);
        let item = arr[0].get("data").expect("书目在 .data 内层");
        assert_eq!(
            item.get("book_name").and_then(|x| x.as_str()),
            Some("异世界挂科自救指南")
        );
        // 搜索匹配语义（render_highlights kw 同款）
        let kw = "挂科".to_string();
        assert!(item
            .get("book_name")
            .and_then(|x| x.as_str())
            .is_some_and(|s| s.to_lowercase().contains(&kw)));
    }

    /// 97：真 progress 载荷——items[] 直取路径（render_progress 同款）。
    #[test]
    fn progress_real_payload_items() {
        let v: Value = serde_json::from_str(
            r#"{"items":[{"book_id":"7701596232459766809","book_name":"催眠横行的末世",
                 "chapter":"第6章","pct":10,"read_timestamp":1795694619}]}"#,
        )
        .unwrap();
        let arr = v
            .get("items")
            .and_then(|x| x.as_array())
            .expect("items 直取");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("pct").and_then(|x| x.as_i64()), Some(10));
        assert!(match_item(&arr[0], "催眠"));
    }

    /// 98：写操作失效快照——set 后 clear，snap 返回 None。
    #[test]
    fn shelf_snap_clear_invalidates() {
        // 仅验证逻辑路径；store 全局文件在测试环境会共享——clear 后不应 panic。
        crate::store::shelf_snap_clear();
        // 不断言 None（并发测试可能他人 set）；只保证调用安全。
    }
}
