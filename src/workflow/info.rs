use crate::api::Client;
use crate::types::{ChapterRange, Config};
use crate::util::{self, CYAN, DIM, RST, YELLOW};

pub async fn run(book_id: &str, range: Option<&str>, show: bool, verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);

    // 获取书籍详情
    let detail = api.fetch_detail(book_id).await.ok();
    if let Some(d) = &detail {
        let parts: Vec<&str> = d.splitn(3, '|').collect();
        let title = parts.first().unwrap_or(&"");
        let author = parts.get(1).unwrap_or(&"");
        let intro = parts.get(2).unwrap_or(&"");
        println!("  {} {YELLOW}{}{RST}", title, author);
        if !intro.is_empty() {
            let abbr: String = intro.chars().take(120).collect();
            let abbr = if intro.chars().count() > 120 { format!("{}…", abbr) } else { abbr };
            println!("  {DIM}{}{RST}", abbr);
        }
    }

    util::progress("获取目录");
    let chs = match api.fetch_catalog(book_id).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
    };
    util::clear_progress();
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());

    // 目录计数只报一次：有筛选显示 m/n，无筛选显示总数
    if v.len() < chs.len() {
        println!("  显示 {DIM}{}{RST}/{} 章", v.len(), chs.len());
    } else {
        println!("  共 {} 章", chs.len());
    }

    if show {
        for c in &v {
            println!("\n  {CYAN}{:04} {}{RST}", c.index, c.title);
            match api.fetch_content(&c.item_id).await {
                Ok(text) => {
                    let total = text.lines().count();
                    for line in text.lines().take(40) {
                        println!("    {}", line);
                    }
                    if total > 40 {
                        println!("  {DIM}… 共 {} 行 · 全文 function fetch-content {}{RST}", total, c.item_id);
                    }
                }
                Err(e) => eprintln!("  err {:04}: {}", c.index, e),
            }
        }
    } else {
        for c in &v {
            println!("  {:04}  {}", c.index, c.title);
        }
    }
}
