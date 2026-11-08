use crate::api::Client;
use crate::types::{ChapterRange, Config};
use crate::util::{CYAN, DIM, RST, YELLOW};

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
            let abbr = if intro.len() > 120 { format!("{}...", abbr) } else { abbr };
            println!("  {DIM}{}{RST}", abbr);
        }
    }

    print!("  获取目录... ");
    flush();
    let chs = match api.fetch_catalog(book_id).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\n  err {}", e);
            return;
        }
    };
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());

    if detail.is_some() { println!("  共{}章 显示{}章", chs.len(), v.len()); }
    else { println!("{} 章", chs.len()); }

    if show {
        for c in &v {
            println!("\n  {CYAN}{:04} {}{RST}", c.index, c.title);
            match api.fetch_content(&c.item_id).await {
                Ok(text) => {
                    for line in text.lines().take(40) {
                        println!("  {}", line);
                    }
                    if text.lines().count() > 40 {
                        println!("  {DIM}... (共{}行){RST}", text.lines().count());
                    }
                }
                Err(e) => println!("  err {}", e),
            }
        }
    } else {
        for c in &v {
            println!("  {:04}  {}", c.index, c.title);
        }
        if v.len() < chs.len() {
            println!("\n  {} / {}", v.len(), chs.len());
        } else {
            println!("\n  共 {} 章", v.len());
        }
    }
}

fn flush() {
    use std::io::Write;
    std::io::stdout().flush().unwrap();
}
