//! 向外交互层：fqapi 进程内直调为主通道，第三方 URL 源降级备选。
//!
//! 主通道走 `crate::fqapi`（fanqie-lite-api 核心，签名 + 设备池直连官方）；
//! 任一主通道调用失败时按源轮换降级到旧的第三方模板，保证可用性。
//! 被 workflow/download/audio/update/info/search 等调用。

use crate::fqapi;
use crate::types::{Book, Chapter};
use crate::util;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct Client {
    pub cache_dir: PathBuf,
    pub cache_enabled: bool,
    #[allow(dead_code)]
    pub cache_ttl: u64,
    pub search_urls: Vec<String>,
    pub catalog_urls: Vec<String>,
    pub content_urls: Vec<String>,
    pub batch_urls: Vec<String>,
    pub detail_url: String,
    pub audio_content_urls: Vec<String>,
    pub verbose: bool,
    pub reqwest_client: reqwest::Client,
    pub source_stats: Arc<Mutex<HashMap<String, SourceStats>>>,
}

#[derive(Clone, Default)]
pub struct SourceStats {
    pub ok_count: u32,
    pub fail_count: u32,
    pub total_time_ms: u64,
}

impl SourceStats {
    fn score(&self) -> f64 {
        let total = self.ok_count + self.fail_count;
        if total == 0 { return 1.0; }
        let rate = self.ok_count as f64 / total as f64;
        let avg_time = if self.ok_count > 0 { self.total_time_ms as f64 / self.ok_count as f64 } else { 9999.0 };
        rate * 100.0 - avg_time * 0.1
    }
}

fn sort_urls(urls: &[String], stats: &HashMap<String, SourceStats>) -> Vec<String> {
    let mut scored: Vec<(f64, &String)> = urls.iter()
        .map(|u| {
            let s = stats.get(u).cloned().unwrap_or_default();
            (-s.score(), u)
        })
        .collect();
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.iter().map(|(_, u)| (*u).clone()).collect()
}

fn maybe_reset_stats(stats: &Arc<Mutex<HashMap<String, SourceStats>>>, counter: &AtomicU64) {
    let c = counter.fetch_add(1, Ordering::Relaxed);
    if c > 0 && c.is_multiple_of(500) {
        if let Ok(mut s) = stats.lock() {
            s.clear();
        }
    }
}

// ---------- fqapi 响应解析辅助 ----------

/// 业务错误检查：`success=false` 或 `code ∉ {0,200}` 视为失败。
fn check_biz(v: &Value) -> Result<(), String> {
    if let Value::Object(m) = v {
        if m.get("success").and_then(|x| x.as_bool()) == Some(false) {
            let msg = m.get("error").and_then(|x| x.as_str())
                .or_else(|| m.get("message").and_then(|x| x.as_str()))
                .unwrap_or("请求失败");
            return Err(msg.to_string());
        }
        if let Some(code) = m.get("code").and_then(|x| x.as_i64()) {
            if code != 0 && code != 200 {
                let msg = m.get("message").and_then(|x| x.as_str())
                    .or_else(|| m.get("msg").and_then(|x| x.as_str()))
                    .unwrap_or("业务错误");
                return Err(format!("{} (code {})", msg, code));
            }
        }
    }
    Ok(())
}

/// fqapi 调用 + 业务错误检查，返回原始 JSON。
async fn core_json(path: &str, query: &str) -> Result<Value, String> {
    let v = fqapi::get(path, query).await?;
    check_biz(&v)?;
    Ok(v)
}

/// 深度优先查找第一个"含目标键的数组"（元素为对象且含 `keys` 之一）。
fn find_items<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    match v {
        Value::Array(arr) => {
            if let Some(first) = arr.first() {
                if first.as_object().is_some_and(|m| keys.iter().any(|k| m.contains_key(*k))) {
                    return Some(arr);
                }
            }
            arr.iter().find_map(|x| find_items(x, keys))
        }
        Value::Object(m) => m.values().find_map(|x| find_items(x, keys)),
        _ => None,
    }
}

/// 深度优先查找第一个键命中且值为非空字符串的字段。
fn find_str(v: &Value, keys: &[&str]) -> Option<String> {
    if let Value::Object(m) = v {
        for k in keys {
            if let Some(s) = m.get(*k).and_then(|x| x.as_str()) {
                if !s.trim().is_empty() {
                    return Some(s.trim().to_string());
                }
            }
        }
        for val in m.values() {
            if let Some(s) = find_str(val, keys) {
                return Some(s);
            }
        }
    }
    None
}

/// 按键序取对象第一个可用字符串（兼容 `{text}` 包装与数字）。
fn field(m: &Map<String, Value>, keys: &[&str]) -> String {
    for k in keys {
        let Some(v) = m.get(*k) else { continue };
        match v {
            Value::String(s) => {
                if !s.trim().is_empty() {
                    return s.trim().to_string();
                }
            }
            Value::Number(n) => return n.to_string(),
            Value::Object(o) => {
                if let Some(t) = o.get("text").and_then(|x| x.as_str()) {
                    if !t.trim().is_empty() {
                        return t.trim().to_string();
                    }
                }
            }
            _ => {}
        }
    }
    String::new()
}

/// 解析搜索响应为书籍列表。
///
/// reading 搜索返回 cell 形状（实体嵌在 `book_data[0]`，高亮标题在
/// `search_high_light`）；其他形状（如 `ret_data` 扁平条目）直接取字段。
fn parse_books(v: &Value) -> Vec<Book> {
    if let Some(items) = find_items(v, &["book_data"]) {
        return items.iter().filter_map(parse_cell_book).collect();
    }
    let Some(items) = find_items(v, &["book_id", "bookId"]) else {
        return Vec::new();
    };
    items.iter().filter_map(|it| {
        let m = it.as_object()?;
        let book_id = field(m, &["book_id", "bookId", "id"]);
        if book_id.is_empty() {
            return None;
        }
        Some(Book {
            book_id,
            title: field(m, &["title", "book_name", "name"]),
            author: field(m, &["author", "author_name"]),
            score: norm_score(&field(m, &["score"])),
            category: field(m, &["category", "category_name"]),
            abstract_: field(m, &["abstract", "book_abstract", "summary", "description"]),
            status: parse_status(&field(m, &["creation_status", "book_state", "status"])),
        })
    }).collect()
}

/// 解析一个 reading 搜索 cell 条目。
fn parse_cell_book(it: &Value) -> Option<Book> {
    let m = it.as_object()?;
    let bd = match m.get("book_data") {
        Some(Value::Array(a)) => a.first()?.as_object()?,
        Some(Value::Object(o)) => o,
        _ => return None,
    };
    let book_id = field(bd, &["book_id"]);
    if book_id.is_empty() {
        return None;
    }
    let hl_title = hl_text(m, "title");
    let title = if hl_title.is_empty() {
        field(bd, &["book_name", "title"])
    } else {
        hl_title
    };
    let hl_abs = hl_text(m, "abstract");
    let abstract_ = if hl_abs.is_empty() {
        field(bd, &["abstract", "book_abstract_v2"])
    } else {
        hl_abs
    };
    Some(Book {
        book_id,
        title,
        author: field(bd, &["author", "author_name"]),
        score: norm_score(&field(bd, &["score"])),
        category: field(bd, &["category"]),
        abstract_,
        status: parse_status(&field(bd, &["creation_status", "book_state", "status"])),
    })
}

/// 取 `search_high_light.{key}` 文本：优先 `text`，其次剥离标签的 `rich_text`。
fn hl_text(m: &Map<String, Value>, key: &str) -> String {
    let Some(hl) = m.get("search_high_light").and_then(|x| x.as_object()) else {
        return String::new();
    };
    match hl.get(key) {
        Some(Value::Object(o)) => {
            let text = o.get("text").and_then(|x| x.as_str())
                .unwrap_or_else(|| o.get("rich_text").and_then(|x| x.as_str()).unwrap_or(""));
            strip_html(text).trim().to_string()
        }
        Some(Value::String(s)) => strip_html(s).trim().to_string(),
        _ => String::new(),
    }
}

/// 评分归一：空或 0 显示为 `-`。
fn norm_score(s: &str) -> String {
    if s.is_empty() || s.parse::<f64>().is_ok_and(|n| n == 0.0) {
        "-".into()
    } else {
        s.to_string()
    }
}

/// 状态字段归一：完结 → 1，其余 → 0（`status_text` 据此显示）。
fn parse_status(s: &str) -> i64 {
    if s == "完结" || s == "1" {
        1
    } else {
        0
    }
}

/// 解析目录响应为章节列表（顺序编号从 1 开始）。
fn parse_chapters(v: &Value) -> Vec<Chapter> {
    // 首选官方阅读目录 data.item_data_list；catalog_data 等仅含卷信息，
    // 因此不能靠泛化的含 item_id 数组探测。
    let items: &[Value] = match v.pointer("/data/item_data_list").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => find_items(v, &["item_id", "itemId"]).map_or(&[], |a| a),
    };
    let mut chapters = Vec::new();
    for it in items {
        let Some(m) = it.as_object() else { continue };
        let item_id = field(m, &["item_id", "itemId"]);
        if item_id.is_empty() {
            continue;
        }
        let mut title = field(m, &["title", "episode_title", "chapter_title", "name", "item_title"]);
        if title.is_empty() {
            title = format!("第{}章", chapters.len() + 1);
        }
        chapters.push(Chapter { index: chapters.len() + 1, item_id, title });
    }
    chapters
}

/// 解析批量正文响应为 item_id → 正文映射。
fn parse_batch(v: &Value) -> HashMap<String, String> {
    let Some(items) = find_items(v, &["item_id", "itemId"]) else {
        return HashMap::new();
    };
    items.iter().filter_map(|it| {
        let m = it.as_object()?;
        let id = field(m, &["item_id", "itemId"]);
        let content = m.get("content").and_then(|x| x.as_str())?;
        if id.is_empty() || content.is_empty() {
            return None;
        }
        Some((id, strip_html(content)))
    }).collect()
}

/// 解析书籍详情为 `title|author|intro`。
fn parse_detail(v: &Value) -> Result<String, String> {
    let title = find_str(v, &["book_name", "bookName", "title"]).unwrap_or_default();
    if title.is_empty() {
        return Err("未获取到书籍信息".into());
    }
    let author = find_str(v, &["author", "author_name"]).unwrap_or_default();
    let intro = find_str(v, &["abstract", "book_abstract", "summary"]).unwrap_or_default();
    Ok(format!("{}|{}|{}", title, author, intro).trim_end_matches('|').to_string())
}

/// 提取流内容密钥：明文返回空串；加密流须有 32 位 hex `key_hex`，否则跳过。
fn stream_key(enc: Option<&Value>) -> Option<String> {
    let Some(Value::Object(m)) = enc else {
        return Some(String::new());
    };
    if m.get("encrypt").and_then(|x| x.as_bool()) != Some(true) {
        return Some(String::new());
    }
    let method = m.get("encryption_method").and_then(|x| x.as_str()).unwrap_or("");
    if !method.is_empty() && method != "cenc-aes-ctr" {
        return None;
    }
    let key = m.get("key_hex").and_then(|x| x.as_str())?.trim().to_lowercase();
    (key.len() == 32 && key.chars().all(|c| c.is_ascii_hexdigit())).then_some(key)
}

/// 提取流地址：绝对 URL 原样返回，base64 形态解码后须以 http(s) 开头。
fn stream_url(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Some(s.to_string());
    }
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    (text.starts_with("http://") || text.starts_with("https://")).then_some(text)
}

/// 从 playinfo/mget 响应提取音频流 `(url, key_hex)`；key_hex 空串为明文流。
fn parse_audio_source(v: &Value, item_id: &str) -> Result<(String, String), String> {
    check_biz(v)?;
    let rows = find_items(v, &["video_model", "item_id"]).ok_or("未获取到音频地址")?;
    for row in rows {
        let Some(m) = row.as_object() else { continue };
        if let Some(code) = m.get("code").and_then(|x| x.as_i64()) {
            if code != 0 && code != 200 {
                continue;
            }
        }
        if let Some(sid) = m.get("item_id").and_then(|x| x.as_str()) {
            if !sid.is_empty() && sid != item_id {
                continue;
            }
        }
        if let Some(raw) = m.get("video_model").and_then(|x| x.as_str()) {
            if let Some(pair) = model_stream(raw) {
                return Ok(pair);
            }
            continue;
        }
        // 旧行内直连形态：main_url/backup_url + encrypt_info 平铺在行上。
        let Some(key) = stream_key(m.get("encrypt_info")) else { continue };
        if let Some(url) = stream_url(m.get("main_url")).or_else(|| stream_url(m.get("backup_url"))) {
            return Ok((url, key));
        }
    }
    Err("未获取到音频地址".into())
}

/// 从 `video_model` JSON 字符串提取第一个可用流。
fn model_stream(raw: &str) -> Option<(String, String)> {
    let model: Value = serde_json::from_str(raw).ok()?;
    if let Some(mt) = model.get("media_type").and_then(|x| x.as_str()) {
        if mt != "audio" {
            return None;
        }
    }
    if let Some(st) = model.get("status").and_then(|x| x.as_i64()) {
        if st != 10 {
            return None;
        }
    }
    let streams = model.get("video_list").and_then(|x| x.as_array())?;
    for s in streams {
        let Some(sm) = s.as_object() else { continue };
        let Some(key) = stream_key(sm.get("encrypt_info")) else { continue };
        if let Some(url) = stream_url(sm.get("main_url")).or_else(|| stream_url(sm.get("backup_url"))) {
            return Some((url, key));
        }
    }
    None
}

impl Client {
    /// 静态 musl 构建无系统 CA，用内置 webpki 根证书预配置 TLS（同 core build_tls_config），
    /// 否则 reqwest 默认走 native-certs 在 Android 上加载不到证书直接 panic。
    fn tls_builder() -> reqwest::ClientBuilder {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("tls 协议版本")
            .with_root_certificates(roots)
            .with_no_client_auth();
        reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            // 静态 musl 无系统 DNS（/etc/resolv.conf），复用 core 进程内解析
            .dns_resolver(fqapi_core::upstream::dns::DnsResolver)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(cache_dir: PathBuf, cache_enabled: bool, cache_ttl: u64,
               search_urls: Vec<String>, catalog_urls: Vec<String>, content_urls: Vec<String>,
               batch_urls: Vec<String>, detail_url: String,
               audio_content_urls: Vec<String>, verbose: bool) -> Self {
        fqapi::install_crypto();
        Client {
            cache_dir, cache_enabled, cache_ttl, search_urls, catalog_urls,
            content_urls, batch_urls, detail_url, audio_content_urls,
            verbose,
            reqwest_client: Self::tls_builder()
                .user_agent("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36")
                .timeout(std::time::Duration::from_secs(15))
                .build().unwrap(),
            source_stats: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_config(cfg: &crate::types::Config, verbose: bool) -> Self {
        Client::new(
            cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
            cfg.search_urls.clone(), cfg.catalog_urls.clone(), cfg.content_urls.clone(),
            cfg.batch_urls.clone(), cfg.detail_url.clone(),
            cfg.audio_content_urls.clone(), verbose,
        )
    }

    fn log_fallback(&self, what: &str, e: &str) {
        if self.verbose {
            eprintln!("  [verbose] fqapi {} 失败，降级第三方: {}", what, e);
        }
    }

    // ---------- 主通道（fqapi） ----------

    async fn core_search(&self, keyword: &str, page: usize) -> Result<Vec<Book>, String> {
        let offset = (page.saturating_sub(1)) * 10;
        let q = format!("query={}&offset={}&count=10&tab_type=3", urlencode(keyword), offset);
        let v = core_json("/api/v1/search", &q).await?;
        let books = parse_books(&v);
        if books.is_empty() { Err("无结果".into()) } else { Ok(books) }
    }

    async fn core_catalog(&self, book_id: &str) -> Result<Vec<Chapter>, String> {
        let v = core_json(&format!("/api/v1/books/{}/directory", book_id), "").await?;
        let chapters = parse_chapters(&v);
        if chapters.is_empty() { Err("空目录".into()) } else { Ok(chapters) }
    }

    async fn core_content(&self, item_id: &str) -> Result<String, String> {
        let v = core_json(&format!("/api/v1/chapters/{}", item_id), "").await?;
        let content = find_str(&v, &["content"]).ok_or("未获取到正文")?;
        if self.cache_enabled {
            let cp = self.cache_dir.join(format!("c_{}.json", item_id));
            fs::write(&cp, v.to_string()).ok();
        }
        Ok(strip_html(&content))
    }

    async fn core_content_batch(&self, book_id: &str, item_ids: &[&str]) -> Result<HashMap<String, String>, String> {
        let q = format!("item_ids={}&book_id={}", item_ids.join(","), book_id);
        let v = core_json("/api/v1/chapters/batch", &q).await?;
        let map = parse_batch(&v);
        if map.is_empty() { Err("批量API未返回数据".into()) } else { Ok(map) }
    }

    async fn core_detail(&self, book_id: &str) -> Result<String, String> {
        let v = core_json(&format!("/api/v1/books/{}", book_id), "").await?;
        parse_detail(&v)
    }

    async fn core_audio(&self, book_id: &str, item_id: &str, tone: usize) -> Result<(String, String), String> {
        let q = format!("tone_id={}", tone);
        let path = format!("/api/v1/audio/books/{}/chapters/{}", book_id, item_id);
        let v = core_json(&path, &q).await?;
        parse_audio_source(&v, item_id)
    }

    // ---------- 公开接口：主通道优先，失败降级 ----------

    /// 搜索小说（fqapi 优先，第三方源轮换降级）。
    ///
    /// # 参数
    /// - `keyword`: 搜索关键词
    /// - `page`: 页码（1-indexed，每页 10 本）
    pub async fn search(&self, keyword: &str, page: usize) -> Result<Vec<Book>, String> {
        match self.core_search(keyword, page).await {
            Ok(books) => Ok(books),
            Err(e) => {
                self.log_fallback("search", &e);
                self.search_legacy(keyword, page).await
            }
        }
    }

    /// 获取全本目录（fqapi 优先，第三方降级）。
    pub async fn fetch_catalog(&self, book_id: &str) -> Result<Vec<Chapter>, String> {
        match self.core_catalog(book_id).await {
            Ok(chs) => Ok(chs),
            Err(e) => {
                self.log_fallback("catalog", &e);
                self.catalog_legacy(book_id).await
            }
        }
    }

    /// 获取单章正文纯文本（fqapi 优先，第三方降级）。
    pub async fn fetch_content(&self, item_id: &str) -> Result<String, String> {
        match self.core_content(item_id).await {
            Ok(text) => Ok(text),
            Err(e) => {
                self.log_fallback("content", &e);
                self.content_legacy(item_id).await
            }
        }
    }

    /// 批量获取正文（fqapi 优先，第三方降级）。
    pub async fn fetch_content_batch(&self, book_id: &str, item_ids: &[&str]) -> Result<HashMap<String, String>, String> {
        match self.core_content_batch(book_id, item_ids).await {
            Ok(map) => Ok(map),
            Err(e) => {
                self.log_fallback("batch", &e);
                self.batch_legacy(book_id, item_ids).await
            }
        }
    }

    /// 获取书籍详情 `title|author|intro`（fqapi 优先，第三方降级）。
    pub async fn fetch_detail(&self, book_id: &str) -> Result<String, String> {
        match self.core_detail(book_id).await {
            Ok(s) => Ok(s),
            Err(e) => {
                self.log_fallback("detail", &e);
                self.detail_legacy(book_id).await
            }
        }
    }

    /// 获取音频流 `(url, key_hex)`（fqapi 优先，第三方降级）。
    ///
    /// `key_hex` 非空表示 CENC 加密流，下载后需按密钥解密才是可播文件；
    /// 第三方降级路径恒返回空串（明文）。
    pub async fn fetch_audio(&self, book_id: &str, item_id: &str, tone: usize) -> Result<(String, String), String> {
        match self.core_audio(book_id, item_id, tone).await {
            Ok(pair) => Ok(pair),
            Err(e) => {
                self.log_fallback("audio", &e);
                self.audio_legacy(item_id, tone).await.map(|u| (u, String::new()))
            }
        }
    }

    // ---------- 降级路径（第三方 URL 模板 + 源轮换） ----------

    async fn search_legacy(&self, keyword: &str, page: usize) -> Result<Vec<Book>, String> {
        static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);
        maybe_reset_stats(&self.source_stats, &REQ_COUNTER);
        let offset = (page.saturating_sub(1)) * 10;
        let kw = urlencode(keyword);
        let mut last_err = String::new();
        let sorted = {
            let stats = self.source_stats.lock().unwrap();
            sort_urls(&self.search_urls, &stats)
        };
        for tmpl in &sorted {
            let url = tmpl.replacen("{}", &kw, 1).replacen("{}", &offset.to_string(), 1);
            let start = Instant::now();
            let result = util::with_verbose_async(|| async {
                util::with_retry_async(|| self.http_get(&url), 2).await
            }, &format!("search {}", &url[..url.len().min(60)]), self.verbose).await;
            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(mut stats) = self.source_stats.lock() {
                let entry = stats.entry(url.clone()).or_default();
                match &result {
                    Ok(_) => { entry.ok_count += 1; entry.total_time_ms += elapsed; }
                    Err(_) => { entry.fail_count += 1; }
                }
            }
            match result {
                Ok(text) => {
                    match Self::parse_search_results(&text, self.verbose) {
                        Ok(books) if !books.is_empty() => return Ok(books),
                        Ok(_) => { last_err = "无结果".into(); continue; }
                        Err(e) => { last_err = e; continue; }
                    }
                }
                Err(e) => { last_err = e; continue; }
            }
        }
        Err(last_err)
    }

    fn parse_search_results(text: &str, verbose: bool) -> Result<Vec<Book>, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| {
            if verbose { eprintln!("  [verbose] JSON解析失败: {}\n  原始响应:\n{}", e, text); }
            format!("JSON: {}", e)
        })?;
        let mut books = vec![];
        if let Some(Value::Array(items)) = root.pointer("/data/ret_data") {
            for item in items {
                let (id, title, author, score, category, abstract_) = (
                    item["book_id"].as_str().unwrap_or(""),
                    item["title"].as_str().unwrap_or(""),
                    item["author"].as_str().unwrap_or(""),
                    item["score"].as_str().unwrap_or("-"),
                    item["category"].as_str().unwrap_or(""),
                    item["abstract"].as_str().unwrap_or(""),
                );
                let status = item["creation_status"].as_i64().unwrap_or(0);
                if !id.is_empty() {
                    books.push(Book {
                        book_id: id.into(), title: title.into(), author: author.into(),
                        score: score.into(), category: category.into(), abstract_: abstract_.into(),
                        status,
                    });
                }
            }
        }
        if books.is_empty() { Err("无结果".into()) } else { Ok(books) }
    }

    async fn catalog_legacy(&self, book_id: &str) -> Result<Vec<Chapter>, String> {
        let mut last_err = String::new();
        for tmpl in &self.catalog_urls {
            let url = tmpl.replacen("{}", book_id, 1);
            let result = util::with_verbose_async(|| async {
                util::with_retry_async(|| self.http_get(&url), 2).await
            }, &format!("catalog {}", book_id), self.verbose).await;
            match result {
                Ok(text) => match self.parse_catalog(&text) {
                    Ok(chs) if !chs.is_empty() => return Ok(chs),
                    Ok(_) => { last_err = "空目录".into(); continue; }
                    Err(e) => { last_err = e; continue; }
                },
                Err(e) => { last_err = e; continue; }
            }
        }
        Err(last_err)
    }

    fn parse_catalog(&self, text: &str) -> Result<Vec<Chapter>, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| {
            if self.verbose { eprintln!("  [verbose] JSON解析失败: {}\n  原始响应:\n{}", e, text); }
            format!("JSON: {}", e)
        })?;
        let mut chapters = vec![];

        // 官方 fanqienovel 格式: /data/chapterListWithVolume[{items:[{itemId,title}]}]
        if let Some(Value::Array(vlist)) = root.pointer("/data/chapterListWithVolume") {
            let mut idx = 1;
            for vol in vlist {
                if let Value::Array(items) = vol {
                    for item in items {
                        let item_id = item["itemId"].as_str().unwrap_or("").to_string();
                        let title = item["title"].as_str().unwrap_or("未知章节").to_string();
                        if !item_id.is_empty() {
                            chapters.push(Chapter { index: idx, item_id, title });
                            idx += 1;
                        }
                    }
                }
            }
        // 第三方服务器格式: /data/lists[{item_id, title}]
        } else if let Some(Value::Array(items)) = root.pointer("/data/lists") {
            for (idx, item) in items.iter().enumerate() {
                let item_id = item["item_id"].as_str().or_else(|| item["itemId"].as_str()).unwrap_or("").to_string();
                let title = item["title"].as_str().unwrap_or("未知章节").to_string();
                if !item_id.is_empty() {
                    chapters.push(Chapter { index: idx + 1, item_id, title });
                }
            }
        }

        if chapters.is_empty() { return Err("未获取到章节".into()); }
        Ok(chapters)
    }

    async fn content_legacy(&self, item_id: &str) -> Result<String, String> {
        static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);
        maybe_reset_stats(&self.source_stats, &REQ_COUNTER);
        let sorted = {
            let stats = self.source_stats.lock().unwrap();
            sort_urls(&self.content_urls, &stats)
        };
        let mut last_err = String::new();
        for tmpl in &sorted {
            let url = tmpl.replacen("{}", item_id, 1);
            let start = Instant::now();
            let result = self.fetch_content_from(&url, item_id).await;
            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(mut stats) = self.source_stats.lock() {
                let entry = stats.entry(url.clone()).or_default();
                match &result {
                    Ok(_) => { entry.ok_count += 1; entry.total_time_ms += elapsed; }
                    Err(_) => { entry.fail_count += 1; }
                }
            }
            if result.is_ok() { return result; }
            last_err = result.unwrap_err();
        }
        Err(last_err)
    }

    async fn fetch_content_from(&self, url: &str, item_id: &str) -> Result<String, String> {
        let raw = util::with_verbose_async(|| async {
            util::with_retry_async(|| self.http_get(url), 3).await
        }, &format!("content {}", &url[..url.len().min(60)]), self.verbose).await?;
        let root: Value = serde_json::from_str(&raw).map_err(|e| {
            if self.verbose { eprintln!("  [verbose] JSON解析失败: {}\n  原始响应:\n{}", e, raw); }
            format!("JSON: {}", item_id)
        })?;
        let content = root.pointer("/data/content").and_then(|v| v.as_str()).unwrap_or("");
        if self.cache_enabled {
            let cp = self.cache_dir.join(format!("c_{}.json", item_id));
            fs::write(&cp, &raw).ok();
        }
        Ok(strip_html(content))
    }

    pub async fn http_get(&self, url: &str) -> Result<String, String> {
        if self.verbose { eprintln!("  [verbose] GET {}", url); }
        let resp = self.reqwest_client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(|e| format!("读取响应: {}", e))?;
        if status == 200 { return Ok(text); }
        Err(format!("HTTP {}: {}..", status, &text[..text.len().min(200)]))
    }

    async fn audio_legacy(&self, item_id: &str, tone_id: usize) -> Result<String, String> {
        static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);
        maybe_reset_stats(&self.source_stats, &REQ_COUNTER);
        let sorted = {
            let stats = self.source_stats.lock().unwrap();
            sort_urls(&self.audio_content_urls, &stats)
        };
        let mut last_err = String::new();
        for tmpl in &sorted {
            let url = tmpl.replacen("{}", item_id, 1).replacen("{}", &tone_id.to_string(), 1);
            let start = Instant::now();
            let result = self.fetch_audio_from(&url, item_id).await;
            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(mut stats) = self.source_stats.lock() {
                let entry = stats.entry(url.clone()).or_default();
                match &result {
                    Ok(_) => { entry.ok_count += 1; entry.total_time_ms += elapsed; }
                    Err(_) => { entry.fail_count += 1; }
                }
            }
            if result.is_ok() { return result; }
            last_err = result.unwrap_err();
        }
        Err(last_err)
    }

    async fn fetch_audio_from(&self, url: &str, item_id: &str) -> Result<String, String> {
        let raw = util::with_verbose_async(|| async {
            util::with_retry_async(|| self.http_get(url), 2).await
        }, &format!("audio_url {}", &item_id[..item_id.len().min(16)]), self.verbose).await?;
        let root: Value = serde_json::from_str(&raw).map_err(|e| {
            if self.verbose { eprintln!("  [verbose] JSON解析失败: {}\n  原始响应:\n{}", e, raw); }
            format!("JSON: {}", item_id)
        })?;
        let audio_url = root.pointer("/data/content").and_then(|v| v.as_str()).unwrap_or("");
        if !audio_url.is_empty() {
            if self.verbose { eprintln!("  [verbose] audio URL: {}b", audio_url.len()); }
            return Ok(audio_url.to_string());
        }
        let msg = root.pointer("/message").and_then(|v| v.as_str()).unwrap_or("");
        Err(if msg.is_empty() { "无可听音频".into() } else { msg.to_string() })
    }

    async fn detail_legacy(&self, book_id: &str) -> Result<String, String> {
        let url = self.detail_url.replacen("{}", book_id, 1);
        let raw = util::with_verbose_async(|| async {
            util::with_retry_async(|| self.http_get(&url), 2).await
        }, &format!("detail {}", book_id), self.verbose).await?;
        let root: Value = serde_json::from_str(&raw).map_err(|e| {
            if self.verbose { eprintln!("  [verbose] JSON: {}\n  {}", e, raw); }
            format!("JSON: {}", e)
        })?;
        let title = root.pointer("/data/data/book_name")
            .or_else(|| root.pointer("/data/data/bookName"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if title.is_empty() { return Err("未找到书籍信息".into()); }
        let author = root.pointer("/data/data/author").and_then(|v| v.as_str()).unwrap_or("");
        let intro = root.pointer("/data/data/abstract").and_then(|v| v.as_str()).unwrap_or("");
        Ok(format!("{}|{}|{}", title, author, intro).trim_end_matches('|').to_string())
    }

    async fn batch_legacy(&self, book_id: &str, item_ids: &[&str]) -> Result<HashMap<String, String>, String> {
        static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);
        maybe_reset_stats(&self.source_stats, &REQ_COUNTER);
        let batch = item_ids.join(",");
        let sorted = {
            let stats = self.source_stats.lock().unwrap();
            sort_urls(&self.batch_urls, &stats)
        };
        let mut last_err = String::new();
        for tmpl in &sorted {
            let url = tmpl.replacen("{}", book_id, 1).replacen("{}", &batch, 1);
            let start = Instant::now();
            let result = async {
                let raw = util::with_verbose_async(|| async {
                    util::with_retry_async(|| self.http_get(&url), 2).await
                }, &format!("batch {} chapters", item_ids.len()), self.verbose).await?;
                let root: Value = serde_json::from_str(&raw).map_err(|e| {
                    if self.verbose { eprintln!("  [verbose] JSON: {}\n  {}", e, raw); }
                    format!("JSON: {}", e)
                })?;
                let mut map = HashMap::new();
                if let Some(arr) = root.pointer("/data").and_then(|v| v.as_array()) {
                    for item in arr {
                        let id = item["item_id"].as_str()
                            .or_else(|| item["itemId"].as_str())
                            .unwrap_or("");
                        let content = item["content"].as_str().unwrap_or("");
                        if !id.is_empty() && !content.is_empty() {
                            map.insert(id.to_string(), strip_html(content));
                        }
                    }
                }
                if !map.is_empty() { return Ok(map); }
                Err("批量API未返回数据".into())
            }.await;
            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(mut stats) = self.source_stats.lock() {
                let entry = stats.entry(url).or_default();
                match &result {
                    Ok(_) => { entry.ok_count += 1; entry.total_time_ms += elapsed; }
                    Err(_) => { entry.fail_count += 1; }
                }
            }
            if result.is_ok() { return result; }
            last_err = result.unwrap_err();
        }
        Err(last_err)
    }
}

fn urlencode(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => r.push(b as char),
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    let mut tag_name = String::new();
    let mut ent = false;
    let mut buf = String::new();
    let mut is_close = false;
    for c in s.chars() {
        if tag {
            if c == '>' {
                tag = false;
                if is_close && (tag_name == "p" || tag_name == "div" || tag_name == "h1" || tag_name == "h2" || tag_name == "h3") {
                    out.push_str("\n\n");
                }
                if tag_name == "br" {
                    out.push('\n');
                }
                tag_name.clear();
                is_close = false;
                continue;
            }
            if c == '/' && tag_name.is_empty() { is_close = true; continue; }
            if !c.is_ascii_whitespace() { tag_name.push(c.to_ascii_lowercase()); }
            continue;
        }
        if ent {
            if c == ';' {
                match buf.as_str() {
                    "lt" => out.push('<'), "gt" => out.push('>'),
                    "amp" => out.push('&'), "nbsp" => out.push(' '),
                    "quot" => out.push('"'), _ => {}
                }
                ent = false; buf.clear();
            } else { buf.push(c); }
            continue;
        }
        if c == '<' { tag = true; tag_name.clear(); is_close = false; continue; }
        if c == '&' { ent = true; buf.clear(); continue; }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_paragraphs() {
        let s = "<article><p idx=\"0\">第一段</p><p idx=\"1\">第二段</p></article>";
        let out = strip_html(s);
        assert_eq!(out, "第一段\n\n第二段\n\n");
    }

    #[test]
    fn test_strip_html_br() {
        let s = "第一行<br>第二行";
        let out = strip_html(s);
        assert_eq!(out, "第一行\n第二行");
    }

    #[test]
    fn test_strip_html_entities() {
        let s = "&lt;tag&gt; &amp; &quot;hello&quot;";
        let out = strip_html(s);
        assert_eq!(out, "<tag> & \"hello\"");
    }

    #[test]
    fn test_strip_html_heading() {
        let s = "<h2>标题</h2><p>内容</p>";
        let out = strip_html(s);
        assert_eq!(out, "标题\n\n内容\n\n");
    }

    #[test]
    fn test_parse_books_nested() {
        let v: Value = serde_json::from_str(r#"{
            "code":0,
            "data":{"ret_data":[
                {"book_id":"123","title":{"text":"测试书"},"author":"某作者","score":"9.5",
                 "category":"玄幻","abstract":"简介","creation_status":1}
            ]}
        }"#).unwrap();
        let books = parse_books(&v);
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].book_id, "123");
        assert_eq!(books[0].title, "测试书");
        assert_eq!(books[0].status, 1);
    }

    #[test]
    fn test_parse_chapters_item_data_list() {
        let v: Value = serde_json::from_str(r#"{
            "code":0,"data":{"item_data_list":[
                {"item_id":"a1","title":"第一章"},
                {"item_id":"a2","title":{"text":"第二章"}}
            ]}
        }"#).unwrap();
        let chs = parse_chapters(&v);
        assert_eq!(chs.len(), 2);
        assert_eq!(chs[0].index, 1);
        assert_eq!(chs[1].title, "第二章");
    }

    #[test]
    fn test_stream_url_base64() {
        use base64::Engine as _;
        let enc = base64::engine::general_purpose::STANDARD
            .encode("https://cdn.example.com/a.mp3");
        let v = Value::String(enc);
        assert_eq!(stream_url(Some(&v)).unwrap(), "https://cdn.example.com/a.mp3");
        assert!(stream_url(Some(&Value::String("not-url".into()))).is_none());
    }

    #[test]
    fn test_stream_key_plain_and_encrypted() {
        assert_eq!(stream_key(None).unwrap(), "");
        let plain: Value = serde_json::from_str(r#"{"encrypt":false}"#).unwrap();
        assert_eq!(stream_key(Some(&plain)).unwrap(), "");
        let enc: Value = serde_json::from_str(
            r#"{"encrypt":true,"encryption_method":"cenc-aes-ctr","key_hex":"00112233445566778899aabbccddeeff"}"#,
        ).unwrap();
        assert_eq!(stream_key(Some(&enc)).unwrap(), "00112233445566778899aabbccddeeff");
        let bad: Value = serde_json::from_str(
            r#"{"encrypt":true,"encryption_method":"cenc-aes-ctr","key_hex":"zz"}"#,
        ).unwrap();
        assert!(stream_key(Some(&bad)).is_none());
    }

    #[test]
    fn test_parse_audio_source_playinfo() {
        let v: Value = serde_json::from_str(
            r#"{"code":0,"data":[{"item_id":"7579","video_model":"{\"media_type\":\"audio\",\"status\":10,\"video_list\":[{\"main_url\":\"https://cdn.example.com/x.mp3\",\"encrypt_info\":{\"encrypt\":false}}]}"}]}"#,
        ).unwrap();
        let (url, key) = parse_audio_source(&v, "7579").unwrap();
        assert_eq!(url, "https://cdn.example.com/x.mp3");
        assert_eq!(key, "");
    }

    #[test]
    fn test_check_biz_error() {
        let v: Value = serde_json::from_str(r#"{"success":false,"error":"池不可用"}"#).unwrap();
        assert!(check_biz(&v).is_err());
        let v2: Value = serde_json::from_str(r#"{"code":103001,"message":"参数错误"}"#).unwrap();
        assert!(check_biz(&v2).is_err());
        let ok: Value = serde_json::from_str(r#"{"code":0,"data":{}}"#).unwrap();
        assert!(check_biz(&ok).is_ok());
    }
}
