use std::path::PathBuf;

#[derive(Clone, serde::Serialize)]
pub struct Book {
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub score: String,
    pub category: String,
    pub abstract_: String,
    pub status: i64,
}

impl Book {
    pub fn status_text(&self) -> &str {
        match self.status { 1 => "完结", _ => "连载" }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct Chapter {
    pub index: usize,
    pub item_id: String,
    pub title: String,
}

#[derive(Clone)]
pub struct ChapterRange {
    pub start: usize,
    pub end: usize,
}

impl ChapterRange {
    pub fn parse(s: &str) -> Option<ChapterRange> {
        let s = s.trim();
        if s.is_empty() { return None; }
        if let Some(rest) = s.strip_prefix('-') {
            let end: usize = rest.parse().ok()?;
            Some(ChapterRange { start: 1, end })
        } else if let Some(prefix) = s.strip_suffix('-') {
            let start: usize = prefix.parse().ok()?;
            Some(ChapterRange { start, end: usize::MAX })
        } else if let Some((a, b)) = s.split_once('-') {
            let start: usize = a.parse().ok()?;
            let end: usize = b.parse().ok()?;
            Some(ChapterRange { start, end })
        } else {
            let n: usize = s.parse().ok()?;
            Some(ChapterRange { start: n, end: n })
        }
    }

    pub fn contains(&self, i: usize) -> bool {
        i >= self.start && i <= self.end
    }
}

#[derive(Clone)]
pub struct Config {
    pub concurrent: usize,
    pub format: String,
    pub output_dir: PathBuf,
    pub filename_template: String,
    pub verbose: bool,
    pub cache_enabled: bool,
    pub cache_dir: PathBuf,
    pub cache_ttl: u64,
    pub bookmark_file: PathBuf,
    pub search_urls: Vec<String>,
    pub catalog_urls: Vec<String>,
    pub content_urls: Vec<String>,
    pub batch_urls: Vec<String>,
    pub audio_content_urls: Vec<String>,
    pub detail_url: String,
    pub audio_tone: usize,
    pub audio_tone_fallbacks: Vec<usize>,
    pub interval_ms: u64,
    pub timeout: u64,
    pub tts_rate: String,
    pub tts_volume: String,
    pub tts_pitch: String,
    pub abr: u32,
    pub post_process: String,
    /// 音频保持 m4a 原样不转 MP3
    pub audio_keep_m4a: bool,
    /// --mp3：转码 MP3 而非默认 remux Ogg Opus
    pub audio_mp3: bool,
    /// 音频嵌入书籍封面到 MP3 ID3 APIC
    pub audio_cover: bool,
    pub custom_commands: std::collections::HashMap<String, String>,
    /// info 概览：目录显示前 N 章（0=不显示）
    pub info_top_chapters: usize,
    /// info 概览：评论显示前 N 条（0=不显示）
    pub info_top_comments: usize,
    /// info 概览：是否显示热度行（read_cnt_text/评分/收藏）
    pub info_hot: bool,
    /// info 概览：简介截断字数（0=不显示简介）
    pub info_intro_len: usize,
    /// info comments section：书评条数
    pub info_comments_count: usize,
    /// info author section：书单条数
    pub info_author_count: usize,
    /// UI：评论/摘要文本截断字数
    pub ui_clip_len: usize,
    /// UI：彩色输出（false 等价 NO_COLOR）
    pub ui_color: bool,
    /// UI：help 底部示例（false 关闭，gh 式示例可配）
    pub ui_help_examples: bool,
    /// UI：阅读翻页模式（90/91）：right-left=左右渲染（默认）/ up-down=上下渲染
    pub ui_page_mode: String,
    /// UI：阅读视图默认模式（89）：normal=分栏（默认）/ zen=全屏四边框（TUI 内 z 临时切换）
    pub ui_view_mode: String,
}

/// 下载工作流参数（来自 CLI 或默认值）
pub struct DownloadParams {
    pub output: Option<String>,
    pub range: Option<String>,
    pub format: Option<String>,
    pub concurrent: Option<usize>,
    pub audio: bool,
    pub tone: usize,
    pub abr: u32,
    pub lrc: String,
    pub force: bool,
    pub verbose: bool,
    pub book_title: Option<String>,
    /// 分类增量类别（-T/--type，逗号分隔 content/audio；None=全部）
    pub only: Option<String>,
}

impl DownloadParams {
    /// 是否包含类别（-T/--type 解析）：None=全部包含；大小写不敏感，逗号分隔多值
    pub fn wants(&self, kind: &str) -> bool {
        match &self.only {
            None => true,
            Some(s) => s.split(',').any(|p| p.trim().eq_ignore_ascii_case(kind)),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let home = get_home();
        Config {
            concurrent: default_concurrent(),
            format: "txt".into(),
            output_dir: PathBuf::from("."),
            filename_template: "{idx04}_{title}".into(),
            verbose: false,
            cache_enabled: true,
            cache_dir: home.join(".config/fqdt/cache"),
            cache_ttl: 86400,
            bookmark_file: home.join(".config/fqdt/books.txt"),
            search_urls: vec![
                "https://novel.snssdk.com/api/novel/channel/homepage/search/search/v1/?aid=1967&q={}&offset={}".into(),
                "http://101.35.133.34:5000/api/search?key={}&offset={}".into(),
            ],
            catalog_urls: vec![
                "https://fanqienovel.com/api/reader/directory/detail?bookId={}".into(),
                "http://101.35.133.34:5000/api/book?book_id={}".into(),
                "http://101.35.133.34:5000/api/directory?book_id={}".into(),
            ],
            content_urls: vec![
                "http://101.35.133.34:5000/api/content?tab=小说&item_id={}".into(),
                "http://101.35.133.34:5000/api/chapter?item_id={}".into(),
                "http://101.35.133.34:5000/api/raw_full?item_id={}".into(),
                "https://tt.sjmyzq.cn/api/raw_full?item_id={}".into(),
            ],
            batch_urls: vec![
                "http://101.35.133.34:5000/api/content?tab=批量&book_id={}&item_ids={}".into(),
            ],
            audio_content_urls: vec![
                "http://101.35.133.34:5000/api/content?tab=听书&item_id={}&tone_id={}".into(),
            ],
            detail_url: "http://101.35.133.34:5000/api/detail?book_id={}".into(),
            audio_tone: 1,
            audio_tone_fallbacks: vec![2, 4, 5, 6, 74, 91],
            interval_ms: 0,
            timeout: 15,
            tts_rate: "+0%".into(),
            tts_volume: "+0%".into(),
            tts_pitch: "+0Hz".into(),
            abr: 0,
            post_process: String::new(),
            audio_keep_m4a: false,
            audio_mp3: false,
            audio_cover: false,
            custom_commands: std::collections::HashMap::new(),
            info_top_chapters: 10,
            info_top_comments: 3,
            info_hot: true,
            info_intro_len: 80,
            info_comments_count: 20,
            info_author_count: 20,
            ui_clip_len: 40,
            ui_color: true,
            ui_help_examples: false,
            ui_page_mode: "right-left".into(),
            ui_view_mode: "normal".into(),
        }
    }
}

pub fn get_home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        PathBuf::from(h)
    } else {
        PathBuf::from(".")
    }
}

pub fn sanitize_filename(s: &str) -> String {
    s.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
        _ => c,
    }).collect()
}

/// 根据主机 CPU 核心数自动选择并发上限（不超过 16）
pub fn default_concurrent() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() * 2).min(32))
        .unwrap_or(8)
}
