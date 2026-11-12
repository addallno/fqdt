use crate::api::Client;
use crate::config;
use crate::types::{Config, DownloadParams};
use crate::util::{self, CYAN, DIM, MAGENTA, RST, YELLOW};
use std::io::Write;
use std::time::Instant;

#[allow(clippy::too_many_arguments)]
pub async fn run(keyword: &str, page: usize, output: Option<&str>, concurrent: Option<usize>,
           range: Option<&str>, format: Option<&str>, verbose: bool, auto: Option<usize>,
           no_download: bool, _interval: u64, cfg: &Config) {
    let start = Instant::now();
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);

    util::progress(&format!("搜索 \"{keyword}\" · 第{page}页"));
    let books = match api.search(keyword, page).await {
        Ok(b) => b,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
    };
    util::clear_progress();
    if books.is_empty() {
        println!("  无结果");
        eprintln!("  {DIM}换关键词 · search <kw> -p {} 下一页{RST}", page + 1);
        return;
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("  {CYAN}{} 本{RST} · {:.1}s", books.len(), elapsed);
    println!();

    let mut list_lines = 2;
    for (i, b) in books.iter().enumerate() {
        println!(
            "  {CYAN}{:>2}.{RST} {}  {YELLOW}{}{RST} · {} · {MAGENTA}{}{RST} · {} {DIM}#{}{RST}",
            i + 1, b.title, b.author, b.category, b.score, b.status_text(), b.book_id
        );
        list_lines += 1;
    }

    if no_download {
        eprintln!("\n  {DIM}info <book_id> 看目录 · download <book_id> 下载 · search -p {} 下一页{RST}", page + 1);
        return;
    }

    let idx = auto.map_or_else(
        || {
            eprint!("  {DIM}输入序号 (1-{}, 0=取消): {RST}", books.len());
            std::io::stderr().flush().unwrap();
            let mut inp = String::new();
            std::io::stdin().read_line(&mut inp).unwrap();
            match inp.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= books.len() => n - 1,
                _ => {
                    eprintln!("  {DIM}取消{RST}");
                    books.len()
                }
            }
        },
        |n| {
            if n >= 1 && n <= books.len() {
                n - 1
            } else {
                eprintln!("  err 无效序号 (1-{})", books.len());
                books.len()
            }
        },
    );

    if idx >= books.len() {
        return;
    }
    let book = &books[idx];

    if auto.is_none() {
        print!("\x1b[{}F\x1b[J", list_lines);
    }
    println!(
        "  {CYAN}{title}{RST}  {YELLOW}{author}{RST} · {category} · {MAGENTA}{score}{RST} · {status}",
        title = book.title,
        author = book.author,
        category = book.category,
        score = book.score,
        status = book.status_text(),
    );
    config::add_bookmark(&book.book_id, &book.title).ok();
    super::download::run(
        &book.book_id, &DownloadParams {
            output: output.map(|s| s.to_string()),
            range: range.map(|s| s.to_string()),
            format: format.map(|s| s.to_string()),
            concurrent,
            audio: false, tone: 1, abr: 0,
            lrc: "external".into(), force: false,
            verbose: vb, book_title: Some(book.title.clone()),
        }, cfg,
    ).await;
}
