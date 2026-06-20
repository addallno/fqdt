use std::path::PathBuf;

#[derive(Clone)]
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

#[derive(Clone)]
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
    pub catalog_url: String,
    pub content_urls: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        let home = get_home();
        Config {
            concurrent: 4,
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
            catalog_url: "https://fanqienovel.com/api/reader/directory/detail?bookId={}".into(),
            content_urls: vec![
                "https://tt.sjmyzq.cn/api/raw_full?item_id={}".into(),
                "http://101.35.133.34:5000/api/raw_full?item_id={}".into(),
            ],
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
