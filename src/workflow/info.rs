use serde_json::Value;

use crate::api::Client;
use crate::types::{ChapterRange, Config};
use crate::util::{self, CYAN, DIM, RST, YELLOW};

const SECTIONS: &str = "chapters · intro · author · comments · cover";

/// # info — 书籍信息（概览 + 可选 section）
///
/// 无 section：标题/作者/简介缩略 + 目录计数与列表（概览）。
/// section：
/// - `chapters`  目录（含 item_id，`-r` 可筛选）
/// - `intro`     完整简介
/// - `author`    作者资料 + 书单（原顶层 `author` 命令迁入）
/// - `comments`  书评（原顶层 `comments` 命令迁入）
/// - `cover`     封面图 URL
pub async fn run(
    book_id: &str,
    section: Option<&str>,
    range: Option<&str>,
    show: bool,
    verbose: bool,
    cfg: &Config,
) {
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);

    if let Some(s) = section {
        match s {
            "chapters" => return chapters(&api, book_id, range).await,
            "intro" => return intro(&api, book_id).await,
            "author" => return author(book_id, cfg).await,
            "comments" => {
                crate::workflow::discover::comments(book_id, 20, 0, false, cfg).await;
                return;
            }
            "cover" => return cover(book_id).await,
            other => {
                eprintln!("  err 未知 section: {other}");
                eprintln!("  可用: {SECTIONS}");
                return;
            }
        }
    }
    overview(&api, book_id, range, show).await;
}

/// 概览：标题/作者/简介缩略 + 目录（`-s` 附正文预览）。
async fn overview(api: &Client, book_id: &str, range: Option<&str>, show: bool) {
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

    let chs = match fetch_catalog(api, book_id).await {
        Ok(c) => c,
        Err(e) => return eprintln!("  err {}", e),
    };
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());
    print_count(&v, &chs);

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
    println!("  {DIM}sections: {SECTIONS}{RST}");
}

/// 目录（含 item_id），`-r` 可筛选。
async fn chapters(api: &Client, book_id: &str, range: Option<&str>) {
    util::progress("获取目录");
    let chs = match fetch_catalog(api, book_id).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            return eprintln!("  err {}", e);
        }
    };
    util::clear_progress();
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());
    print_count(&v, &chs);
    for c in &v {
        println!("  {:04}  {}  {DIM}#{}{RST}", c.index, c.title, c.item_id);
    }
}

/// 完整简介。
async fn intro(api: &Client, book_id: &str) {
    match api.fetch_detail(book_id).await {
        Ok(d) => {
            let parts: Vec<&str> = d.splitn(3, '|').collect();
            println!("  {} {YELLOW}{}{RST}", parts[0], parts[1]);
            if parts.len() > 2 && !parts[2].is_empty() {
                println!("\n{}", parts[2]);
            } else {
                println!("  {DIM}（无简介）{RST}");
            }
        }
        Err(e) => eprintln!("  err {}", e),
    }
}

/// 封面图 URL。
async fn cover(book_id: &str) {
    let Some(v) = detail_json(book_id).await else { return };
    let url = ["thumb_url", "detail_page_thumb_url"]
        .iter()
        .find_map(|k| v.pointer(&format!("/data/{k}")).and_then(|x| x.as_str()))
        .unwrap_or("");
    if url.is_empty() {
        eprintln!("  err 该书无封面字段");
    } else {
        println!("  {url}");
    }
}

/// 作者资料 + 书单（id 取 detail 的 bind_author_ids / author_id）。
async fn author(book_id: &str, cfg: &Config) {
    let Some(v) = detail_json(book_id).await else { return };
    let id = detail_author_id(&v);
    if id.is_empty() {
        return eprintln!("  err 详情中无作者 id");
    }
    crate::workflow::discover::author(&id, 20, false, cfg).await;
}

/// 目录（带进度与错误处理的共用包装）。
async fn fetch_catalog(api: &Client, book_id: &str) -> Result<Vec<crate::types::Chapter>, String> {
    util::progress("获取目录");
    let r = api.fetch_catalog(book_id).await;
    util::clear_progress();
    r
}

/// 目录计数只报一次：有筛选显示 m/n，无筛选显示总数。
fn print_count(v: &[&crate::types::Chapter], all: &[crate::types::Chapter]) {
    if v.len() < all.len() {
        println!("  显示 {DIM}{}{RST}/{} 章", v.len(), all.len());
    } else {
        println!("  共 {} 章", all.len());
    }
}

/// 书籍详情 JSON（fqapi 直连，同 discover::fetch）。
async fn detail_json(book_id: &str) -> Option<Value> {
    match crate::fqapi::get_json(&format!("/api/v1/books/{book_id}"), "").await {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("  err 获取详情失败: {e}");
            None
        }
    }
}

/// 从详情提取作者 id（`author_id` 原样，如 `2_698…`；裸数字会错拿他人）。
fn detail_author_id(v: &Value) -> String {
    v.pointer("/data/author_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}
