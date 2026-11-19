//! # login / account — 登录态与个人数据
//!
//! 上游无账号密码登录端点：登录态 = 浏览器登录 fanqienovel.com 后复制的 Cookie。
//! cookie 存独立文件 `~/.config/fqdt/cookie`（不进 config.ini，防 `config --list` 泄露）。
//! 调用 core 两个 cookie 鉴权只读端点：
//! - POST `/api/v1/account/fanqie/library`（shelf/folders/progress 等 resource）
//! - POST `/api/v1/account/fanqie/history`（阅读历史）
//!
//! # CLI 调用
//! - `fqdt login [COOKIE] [--check] [--clear]`
//! - `fqdt account [shelf|folders|history|progress|user] [--json]`

use crate::config;
use crate::util::{BLUE, CYAN, DIM, GREEN, MAGENTA, RED, RST, YELLOW};
use crate::Config;
use serde_json::{json, Value};

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

/// 用已存 Cookie 请求书架列表验证登录态。
async fn verify_now() {
    let Some(c) = config::load_cookie() else {
        return eprintln!("  {RED}err{RST} 未设置 Cookie（fqdt login '<Cookie>'）");
    };
    let body = json!({"cookie": c, "resource": "shelf_list", "offset": 0, "book_type": 0})
        .to_string().into_bytes();
    match call_library_raw(body).await.and_then(|v| upstream_ok(&v)) {
        Ok(()) => crate::emit_line(&format!("{GREEN}ok{RST} 登录态有效（书架可读）")),
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// `fqdt account [SECTION] [--json]`：个人数据只读展示。
pub async fn account(section: Option<String>, json: bool, _cfg: &Config) {
    let sec = section.as_deref().unwrap_or("shelf");
    if sec == "user" { return render_user(); }
    let label = match sec {
        "shelf" | "folders" | "progress" | "history" => sec,
        _ => {
            eprintln!("  {RED}err{RST} 未知部分: {sec}");
            crate::emit_line(&format!("  {DIM}可选: shelf(默认) · folders · history · progress · user{RST}"));
            return;
        }
    };
    let Some(cookie) = config::load_cookie() else {
        eprintln!("  {RED}err{RST} 未设置登录 Cookie");
        crate::emit_line(&format!("  {DIM}先 fqdt login '<Cookie>'（浏览器登录 fanqienovel.com 复制）{RST}"));
        return;
    };
    util_progress(&format!("获取 {label}"));
    let res = fetch_section(&cookie, label).await;
    util_clear();
    match res {
        Ok(v) => render_section(&v, json, label),
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

/// `fqdt account user`：登录态与可用能力说明（上游无 profile 端点）。
fn render_user() {
    match config::load_cookie() {
        None => {
            crate::emit_line(&format!("  {YELLOW}未登录{RST}"));
            crate::emit_line(&format!("  {DIM}fqdt login '<Cookie>' 设置（浏览器登录 fanqienovel.com 复制）{RST}"));
        }
        Some(c) => {
            let sess = config::cookie_has_session(&c);
            let mark = if sess { format!("{GREEN}有效键 ✓{RST}") } else { format!("{YELLOW}缺 sessionid/sid_tt ✗{RST}") };
            crate::emit_line(&format!("{GREEN}ok{RST} 已登录（{} 字节，{mark}）", c.len()));
            crate::emit_line(&format!("  {DIM}验证 fqdt login --check · 清除 fqdt login --clear{RST}"));
        }
    }
    crate::emit_line(&format!("  {YELLOW}能力{RST} shelf 书架 · folders 书单 · history 阅读历史 · progress 阅读进度"));
    crate::emit_line(&format!("  {DIM}上游无个人资料端点：昵称/头像等 profile 无法获取；全部为 cookie 鉴权只读{RST}"));
    crate::emit_line(&format!("  {DIM}查询: fqdt account shelf --json 查看原始结构{RST}"));
}

/// 进度提示（复用 util，非 TTY 静默）。
fn util_progress(label: &str) { crate::util::progress(label); }
fn util_clear() { crate::util::clear_progress(); }
