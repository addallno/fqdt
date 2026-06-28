use crate::types::{Book, Chapter};
use serde_json::Value;
use std::path::PathBuf;
use std::time::SystemTime;
use std::fs;

pub struct Client {
    pub cache_dir: PathBuf,
    pub cache_enabled: bool,
    pub cache_ttl: u64,
    pub search_urls: Vec<String>,
    pub catalog_url: String,
    pub content_urls: Vec<String>,
    pub audio_content_urls: Vec<String>,
    pub verbose: bool,
}

impl Client {
    pub fn new(cache_dir: PathBuf, cache_enabled: bool, cache_ttl: u64,
               search_urls: Vec<String>, catalog_url: String, content_urls: Vec<String>,
               audio_content_urls: Vec<String>, verbose: bool) -> Self {
        Client { cache_dir, cache_enabled, cache_ttl, search_urls, catalog_url, content_urls, audio_content_urls, verbose }
    }

    pub fn search(&self, keyword: &str, page: usize) -> Result<Vec<Book>, String> {
        let offset = (page.saturating_sub(1)) * 10;
        let kw = urlencode(keyword);

        let mut last_err = String::new();
        for tmpl in &self.search_urls {
            let url = tmpl.replacen("{}", &kw, 1).replacen("{}", &offset.to_string(), 1);
            if self.verbose { eprintln!("  [verbose] GET {}", url); }
            match self.http_get(&url) {
                Ok(text) => {
                    match Self::parse_search_results(&text, self.verbose) {
                        Ok(books) => return Ok(books),
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
        if let Some(arr) = root.pointer("/data/ret_data") {
            if let Value::Array(items) = arr {
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
                    if !id.is_empty() { books.push(Book {
                        book_id: id.into(), title: title.into(), author: author.into(),
                        score: score.into(), category: category.into(), abstract_: abstract_.into(),
                        status,
                    }); }
                }
            }
        }
        if books.is_empty() { Err("无结果".into()) } else { Ok(books) }
    }

    pub fn fetch_catalog(&self, book_id: &str) -> Result<Vec<Chapter>, String> {
        let url = self.catalog_url.replacen("{}", book_id, 1);
        if self.verbose { eprintln!("  [verbose] GET {}", url); }
        let text = self.get_cached(&url, book_id)?;
        let root: Value = serde_json::from_str(&text).map_err(|e| {
            if self.verbose { eprintln!("  [verbose] JSON解析失败: {}\n  原始响应:\n{}", e, text); }
            format!("JSON: {}", e)
        })?;
        let mut chapters = vec![];
        let mut idx = 1;
        if let Some(volumes) = root.pointer("/data/chapterListWithVolume") {
            if let Value::Array(vlist) = volumes {
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
            }
        }
        if chapters.is_empty() { return Err("未获取到章节".into()); }
        Ok(chapters)
    }

    pub fn fetch_content(&self, item_id: &str) -> Result<String, String> {
        let mut last_err = String::new();
        for tmpl in &self.content_urls {
            let url = tmpl.replacen("{}", item_id, 1);
            let content = self.fetch_content_from(&url, item_id);
            if content.is_ok() { return content; }
            last_err = content.unwrap_err();
        }
        Err(last_err)
    }

    fn fetch_content_from(&self, url: &str, item_id: &str) -> Result<String, String> {
        if self.verbose { eprintln!("  [verbose] GET {}", url); }
        let raw = self.http_get(url)?;
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

    fn get_cached(&self, url: &str, book_id: &str) -> Result<String, String> {
        if !self.cache_enabled { return self.http_get(url); }
        let cp = self.cache_dir.join(format!("cat_{}.json", book_id));
        if cp.exists() {
            if let Ok(meta) = fs::metadata(&cp) {
                if let Ok(mtime) = meta.modified() {
                    let age = SystemTime::now().duration_since(mtime).unwrap_or_default().as_secs();
                    if age < self.cache_ttl {
                        if let Ok(c) = fs::read_to_string(&cp) { return Ok(c); }
                    }
                }
            }
        }
        let text = self.http_get(url)?;
        // only cache valid JSON responses
        if serde_json::from_str::<Value>(&text).is_ok() {
            fs::write(&cp, &text).ok();
        }
        Ok(text)
    }

    pub fn http_get(&self, url: &str) -> Result<String, String> {
        // fast path: minreq (in-process, no fork overhead)
        if self.verbose { eprintln!("  [verbose] minreq GET {}", url); }
        let resp = minreq::get(url)
            .with_header("User-Agent", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36")
            .with_timeout(15)
            .send()
            .map_err(|e| format!("请求失败: {}", e))?;
        let status = resp.status_code;
        let text = resp.as_str().map_err(|e| format!("编码错误: {}", e))?.to_string();
        if status == 200 { return Ok(text); }
        if self.verbose { eprintln!("  [verbose] minreq {} {}b, status={}", url, text.len(), status); }

        // slow path: curl fallback (only when minreq fails)
        if self.verbose { eprintln!("  [verbose] fallback curl GET {}", url); }
        let r = std::process::Command::new("curl")
            .args(["-sf", "--connect-timeout", "15", "--max-time", "30",
                "-A", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36", url])
            .output();
        if let Ok(out) = r {
            if out.status.success() && !out.stdout.is_empty() {
                let ct = String::from_utf8(out.stdout).map_err(|e| format!("编码: {}", e))?;
                if self.verbose { eprintln!("  [verbose] OK curl ({}b)", ct.len()); }
                return Ok(ct);
            }
            if self.verbose {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("  [verbose] curl fail: status={}, err={}", out.status, stderr.trim());
            }
        }

        let snippet: String = text.chars().take(200).collect();
        Err(format!("HTTP {}: {}", status, snippet))
    }

    pub fn fetch_audio_url(&self, item_id: &str, tone_id: usize) -> Result<String, String> {
        let mut last_err = String::new();
        for tmpl in &self.audio_content_urls {
            let url = tmpl.replacen("{}", item_id, 1).replacen("{}", &tone_id.to_string(), 1);
            if self.verbose { eprintln!("  [verbose] GET {}", url); }
            let raw = self.http_get(&url)?;
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
            last_err = if msg.is_empty() { "无可听音频".into() } else { format!("{}", msg) };
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
}
