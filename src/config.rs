use crate::types::{Config, get_home};
use std::fs;
use std::path::PathBuf;

impl Config {
    pub fn load() -> Self {
        let path = get_home().join(".config/fqdt/config.ini");
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('[') || line.starts_with('#') { continue; }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim(); let v = v.trim();
                    match k {
                        "concurrent" => cfg.concurrent = v.parse().unwrap_or(4),
                        "format" => cfg.format = v.into(),
                        "output_dir" => cfg.output_dir = PathBuf::from(v),
                        "filename_template" => cfg.filename_template = v.into(),
                        "verbose" => cfg.verbose = v == "true",
                        "cache_enabled" => cfg.cache_enabled = v != "false",
                        "cache_ttl" => cfg.cache_ttl = v.parse().unwrap_or(86400),
                        "search_url" => cfg.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "catalog_url" => cfg.catalog_url = v.into(),
                        "content_url" => cfg.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        _ => {}
                    }
                }
            }
        }
        cfg
    }

    pub fn save_default() -> Result<(), String> {
        let d = get_home().join(".config/fqdt");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let path = d.join("config.ini");
        if path.exists() { return Ok(()); }
        let ini = "\
# 番茄小说下载器配置
[download]
concurrent = 4
format = txt
output_dir = .
filename_template = {idx04}_{title}
verbose = false

[cache]
cache_enabled = true
cache_ttl = 86400

[api]
# 搜索API(逗号分隔,从前往后尝试,{}会被关键词和页码替换)
search_url = https://novel.snssdk.com/api/novel/channel/homepage/search/search/v1/?aid=1967&q={}&offset={},http://101.35.133.34:5000/api/search?key={}&offset={}
# 目录API
catalog_url = https://fanqienovel.com/api/reader/directory/detail?bookId={}
# 内容API(逗号分隔)
content_url = https://tt.sjmyzq.cn/api/raw_full?item_id={},http://101.35.133.34:5000/api/raw_full?item_id={}
";
        fs::write(&path, ini).map_err(|e| e.to_string())
    }

    pub fn apply_cli_overrides(&mut self, search_url: Option<&str>, catalog_url: Option<&str>, content_url: Option<&str>) {
        if let Some(v) = search_url { self.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
        if let Some(v) = catalog_url { self.catalog_url = v.into(); }
        if let Some(v) = content_url { self.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
    }

    pub fn ensure_dirs(&self) {
        fs::create_dir_all(&self.cache_dir).ok();
        if let Some(p) = self.bookmark_file.parent() { fs::create_dir_all(p).ok(); }
    }
}

pub fn load_bookmarks() -> Vec<(String, String)> {
    let path = get_home().join(".config/fqdt/books.txt");
    let mut books = vec![];
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if let Some((id, title)) = line.split_once(':') {
                books.push((id.trim().into(), title.trim().into()));
            }
        }
    }
    books
}

pub fn add_bookmark(id: &str, title: &str) -> Result<(), String> {
    let path = get_home().join(".config/fqdt/books.txt");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut text = String::new();
    if let Ok(t) = fs::read_to_string(&path) { text = t; }
    text.push_str(&format!("{}:{}\n", id, title));
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn remove_bookmark(idx: usize) -> Result<(), String> {
    let path = get_home().join(".config/fqdt/books.txt");
    let books = load_bookmarks();
    if idx == 0 || idx > books.len() { return Err("无效编号".into()); }
    let mut out = String::new();
    for (i, (id, t)) in books.iter().enumerate() {
        if i + 1 != idx { out.push_str(&format!("{}:{}\n", id, t)); }
    }
    fs::write(&path, out).map_err(|e| e.to_string())
}
