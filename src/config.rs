use crate::types::{Config, get_home};
use crate::util::{RST, YELLOW};
use std::fs;
use std::path::PathBuf;

impl Config {
    pub fn load() -> Self {
        let path = get_home().join(".config/fqdt/config.ini");
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(&path) {
            let mut section = String::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some(sec) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    section = sec.trim().to_lowercase();
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim(); let v = v.trim();
                    if section == "workflow_cmd" && !k.is_empty() {
                        cfg.custom_commands.insert(k.to_string(), v.to_string());
                        continue;
                    }
                    match k {
                        "concurrent" => { let n: usize = v.parse().unwrap_or(0); if n > 0 { cfg.concurrent = n; } }
                        "format" => cfg.format = v.into(),
                        "output_dir" => cfg.output_dir = PathBuf::from(v),
                        "filename_template" => cfg.filename_template = v.into(),
                        "verbose" => cfg.verbose = v == "true",
                        "cache_enabled" => cfg.cache_enabled = v != "false",
                        "cache_ttl" => cfg.cache_ttl = v.parse().unwrap_or(86400),
                        "search_url" => cfg.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "catalog_url" => cfg.catalog_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "content_url" => cfg.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "batch_url" => cfg.batch_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "detail_url" => cfg.detail_url = v.into(),
                        "audio_content_url" => cfg.audio_content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "audio_tone" => cfg.audio_tone = v.parse().unwrap_or(1),
                        "audio_tone_fallbacks" => cfg.audio_tone_fallbacks = v.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
                        "interval_ms" => cfg.interval_ms = v.parse().unwrap_or(0),
                        "timeout" => cfg.timeout = v.parse().unwrap_or(15),
                        "tts_rate" => cfg.tts_rate = v.into(),
                        "tts_volume" => cfg.tts_volume = v.into(),
                        "tts_pitch" => cfg.tts_pitch = v.into(),
                        "abr" => cfg.abr = v.parse().unwrap_or(0),
                        "post_process" => cfg.post_process = v.into(),
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
# 并发数 (默认: CPU核心数*2, 最高32)
concurrent = 0
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
catalog_url = https://fanqienovel.com/api/reader/directory/detail?bookId={},http://101.35.133.34:5000/api/book?book_id={},http://101.35.133.34:5000/api/directory?book_id={}
# 内容API(逗号分隔,从前往后尝试,{}会被item_id替换)
content_url = http://101.35.133.34:5000/api/content?tab=小说&item_id={},http://101.35.133.34:5000/api/chapter?item_id={},http://101.35.133.34:5000/api/raw_full?item_id={},https://tt.sjmyzq.cn/api/raw_full?item_id={}
# 批量内容API({}会被book_id和item_ids(逗号分隔)替换)
batch_url = http://101.35.133.34:5000/api/content?tab=批量&book_id={}&item_ids={}
# 书籍详情API
detail_url = http://101.35.133.34:5000/api/detail?book_id={}
# 语音内容API(两个{}: 第一个item_id, 第二个tone_id)
audio_content_url = http://101.35.133.34:5000/api/content?tab=听书&item_id={}&tone_id={}
# 默认语音音色(1-91)
audio_tone = 1
# 音色回退列表(主音色失败时依次尝试)
audio_tone_fallbacks = 2,4,5,6,74,91

[tts]
# edge-tts 语速/音量/音调
tts_rate = +0%
tts_volume = +0%
tts_pitch = +0Hz

[audio]
# MP3 编码码率 0=原始(不重编码), 32/64/128
abr = 0
# 后处理命令模板 {input} {output} (如 lame --abr 32 {input} {output})
post_process =
";
        fs::write(&path, ini).map_err(|e| e.to_string())
    }

    pub fn apply_cli_overrides(&mut self, search_url: Option<&str>, catalog_url: Option<&str>, content_url: Option<&str>) {
        if let Some(v) = search_url { self.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
        if let Some(v) = catalog_url { self.catalog_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
        if let Some(v) = content_url { self.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
    }

    pub fn ensure_dirs(&self) {
        fs::create_dir_all(&self.cache_dir).ok();
        if let Some(p) = self.bookmark_file.parent() { fs::create_dir_all(p).ok(); }
    }

    pub fn validate(&self) {
        let mut issues: Vec<String> = vec![];
        if self.search_urls.is_empty() { issues.push("search_urls 为空".into()); }
        if self.catalog_urls.is_empty() { issues.push("catalog_urls 为空".into()); }
        if self.content_urls.is_empty() { issues.push("content_urls 为空".into()); }
        if self.concurrent == 0 { issues.push("concurrent=0 无效，使用默认值".into()); }
        if self.format != "txt" && self.format != "epub" { issues.push("format 必须是 txt 或 epub".into()); }
        if !self.cache_dir.exists() {
            if let Err(e) = fs::create_dir_all(&self.cache_dir) {
                issues.push(format!("cache_dir 创建失败: {}", e));
            }
        }
        if !issues.is_empty() {
            eprintln!("  {YELLOW}配置警告:{RST}");
            for i in &issues { eprintln!("    - {}", i); }
        }
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
