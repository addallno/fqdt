use crate::api::Client;
use crate::config;
use crate::types::{Book, Config, DownloadParams};
use crate::util::{self, CYAN, DIM, MAGENTA, RST, YELLOW};
use std::io::Write;
use std::time::Instant;

/// 搜索结果列表：一行一本（序号 书名 作者 · 分类 · 评分 · 状态 #id）。
fn print_list(books: &[Book]) {
    for (i, b) in books.iter().enumerate() {
        println!(
            "  {CYAN}{:>2}.{RST} 「{}」  {YELLOW}{}{RST} · {} · {MAGENTA}{}{RST} · {} {DIM}#{}{RST}",
            i + 1, b.title, b.author, b.category, b.score, b.status_text(), b.book_id
        );
    }
}

/// 只搜索并组织化展示书籍信息；`-D N` 时改为下载第 N 本。
#[allow(clippy::too_many_arguments)]
pub async fn run(
    keyword: &str,
    page: usize,
    output: Option<&str>,
    concurrent: Option<usize>,
    range: Option<&str>,
    format: Option<&str>,
    verbose: bool,
    auto: Option<usize>,
    _no_download: bool,
    _interval: u64,
    cfg: &Config,
) {
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
    print_list(&books);

    let Some(n) = auto else {
        eprintln!(
            "\n  {DIM}download <书名|ID> 下载 · info <id> 看目录 · search -p {} 下一页{RST}",
            page + 1
        );
        return;
    };
    if n < 1 || n > books.len() {
        eprintln!("  err 无效序号 (1-{})", books.len());
        return;
    }
    let book = &books[n - 1];
    println!(
        "\n  {CYAN}「{title}」{RST}  {YELLOW}{author}{RST} · {category} · {MAGENTA}{score}{RST} · {status}",
        title = book.title, author = book.author, category = book.category,
        score = book.score, status = book.status_text(),
    );
    config::add_bookmark(&book.book_id, &book.title).ok();
    super::download::run(
        &book.book_id,
        &DownloadParams {
            output: output.map(|s| s.to_string()),
            range: range.map(|s| s.to_string()),
            format: format.map(|s| s.to_string()),
            concurrent,
            audio: false,
            tone: 1,
            abr: 0,
            lrc: "external".into(),
            force: false,
            verbose: vb,
            only: None,
            book_title: Some(book.title.clone()),
        },
        cfg,
    )
    .await;
}

/// 选书序号决策（99/104）：`-n` 最高 → 单结果直选 → `search.mode=first`（默认）取第 1 → `pick` 交互。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PickChoice {
    /// 0 基下标
    At(usize),
    /// 需要 TTY 输入序号
    Prompt,
}

/// 决策纯函数（可单测）：`-n` 越界返回 `Err` 文案。
pub(crate) fn pick_choice(
    nth: Option<usize>,
    mode: &str,
    len: usize,
) -> Result<PickChoice, String> {
    if let Some(n) = nth {
        if n < 1 || n > len {
            return Err(format!("-n {n} 超出范围（1-{len}）"));
        }
        return Ok(PickChoice::At(n - 1));
    }
    if len == 1 || mode != "pick" {
        return Ok(PickChoice::At(0));
    }
    Ok(PickChoice::Prompt)
}

/// 搜索并选择一本书，返回 `(book_id, title)`。
///
/// 选择规则（99/104）：`-n <n>` 指定第 n 个最高优先；否则单结果直选；
/// `search.mode=first`（默认）多结果取第 1；`pick` 模式多结果 TTY 输入序号
/// （非交互报错引导）；无结果/取消返回 `None`（错误已打印）。
pub async fn pick(
    keyword: &str,
    cfg: &Config,
    vb: bool,
    nth: Option<usize>,
) -> Option<(String, String)> {
    let api = Client::from_config(cfg, vb);
    util::progress(&format!("搜索 \"{keyword}\""));
    let books = match api.search(keyword, 1).await {
        Ok(b) => b,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return None;
        }
    };
    util::clear_progress();
    if books.is_empty() {
        eprintln!("  err 无结果: {keyword}");
        return None;
    }
    println!("  {CYAN}{} 本{RST}", books.len());
    println!();
    print_list(&books);

    let idx = match pick_choice(nth, &cfg.search_mode, books.len()) {
        Ok(PickChoice::At(i)) => {
            if nth.is_none() && books.len() > 1 {
                println!(
                    "  {DIM}取第 {} 个（search.mode={} · -n 可指定）{RST}",
                    i + 1,
                    cfg.search_mode
                );
            }
            i
        }
        Err(e) => {
            eprintln!("  err {e}");
            return None;
        }
        Ok(PickChoice::Prompt) => {
            use std::io::IsTerminal;
            if !std::io::stdin().is_terminal() {
                eprintln!("  err 多个结果且非交互 · search.mode=first 或 -n <n> 可跳过选择");
                return None;
            }
            eprint!("\n  {DIM}输入序号 (1-{}, 0=取消): {RST}", books.len());
            std::io::stderr().flush().ok();
            let mut inp = String::new();
            std::io::stdin().read_line(&mut inp).ok()?;
            match inp.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= books.len() => n - 1,
                _ => {
                    eprintln!("  {DIM}取消{RST}");
                    return None;
                }
            }
        }
    };
    let b = &books[idx];
    Some((b.book_id.clone(), b.title.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 99/104：-n 优先、越界报错、first 默认取第 1、pick 才交互
    #[test]
    fn pick_choice_priority() {
        // -n 最高：覆盖 first/pick 两种 mode
        assert_eq!(pick_choice(Some(2), "first", 5), Ok(PickChoice::At(1)));
        assert_eq!(pick_choice(Some(1), "pick", 5), Ok(PickChoice::At(0)));
        // -n 越界（含 0）→ Err
        assert!(pick_choice(Some(0), "first", 5).is_err());
        assert!(pick_choice(Some(6), "first", 5).is_err());
        // 单结果无论 mode 都直选
        assert_eq!(pick_choice(None, "first", 1), Ok(PickChoice::At(0)));
        assert_eq!(pick_choice(None, "pick", 1), Ok(PickChoice::At(0)));
        // 默认 first：多结果取第 1，不交互
        assert_eq!(pick_choice(None, "first", 5), Ok(PickChoice::At(0)));
        // pick 模式：多结果才交互
        assert_eq!(pick_choice(None, "pick", 5), Ok(PickChoice::Prompt));
    }
}
