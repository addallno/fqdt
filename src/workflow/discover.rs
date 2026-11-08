//! 发现类只读子命令：推荐流 / 分类 / 书评 / 作者资料
//!
//! 统一管线：取 JSON → `util::jq_extract` 路径提取 → 终端渲染；
//! `--json` 时直接输出原始响应，供脚本消费。
//! 端点行为说明见 `src/fqapi.rs::ENDPOINTS` 与 core 的 docs/API.md。

use crate::fqapi;
use crate::types::Config;
use crate::util::{jq_display, jq_extract};
use serde_json::Value;

use crate::util::{CYAN, DIM, RST, YELLOW};

/// 取路径标量；无匹配返回空串
fn get(v: &Value, path: &str) -> String {
    jq_extract(v, path).map(|x| jq_display(&x)).unwrap_or_default()
}

/// 按字符截断并追加省略号
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// 按终端显示宽度补空格（CJK 字符记 2 列）
fn pad(s: &str, width: usize) -> String {
    let shown: usize = s
        .chars()
        .map(|c| if (c as u32) > 0x1100 { 2 } else { 1 })
        .sum();
    format!("{}{}", s, " ".repeat(width.saturating_sub(shown)))
}

/// 拉取 JSON；失败打印错误并返回 None
async fn fetch(path: &str, query: &str, label: &str) -> Option<Value> {
    match fqapi::get_json(path, query).await {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("  err {label}失败: {e}");
            None
        }
    }
}

/// 打印原始 JSON（`--json` 模式）
fn pretty(v: &Value) {
    match serde_json::to_string_pretty(v) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{v}"),
    }
}

/// 单本书的表格行
fn book_line(book: &Value, indent: &str) -> String {
    let name = get(book, "book_name");
    let author = get(book, "author");
    let cat = get(book, "category");
    let id = get(book, "book_id");
    format!(
        "{indent}{CYAN}{name}{RST} {YELLOW}{author}{RST} {DIM}{cat} #{id}{RST}"
    )
}

/// 首页推荐流：区块 → 卡片 → 书单
pub async fn recommend(tab_type: u32, tab_index: u32, offset: u32, json: bool, _cfg: &Config) {
    let q = format!("tab_type={tab_type}&tab_index={tab_index}&offset={offset}");
    let Some(v) = fetch("/api/v1/recommend/homepage", &q, "推荐流").await else {
        return;
    };
    if json {
        return pretty(&v);
    }
    let Some(tabs) = v
        .get("data")
        .and_then(|d| d.get("tab_item"))
        .and_then(|t| t.as_array())
    else {
        println!("  {DIM}（无推荐数据）{RST}");
        return;
    };
    for tab in tabs {
        let title = get(tab, "title");
        if !title.is_empty() {
            println!("  {CYAN}【{title}】{RST}");
        }
        let cells = tab
            .get("cell_data")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        for cell in &cells {
            let cname = get(cell, "cell_name");
            if !cname.is_empty() {
                println!("  {DIM}· {cname}{RST}");
            }
            let inner = cell
                .get("cell_data")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for card in &inner {
                let books = card
                    .get("book_data")
                    .and_then(|b| b.as_array())
                    .cloned()
                    .unwrap_or_default();
                for b in &books {
                    println!("{}", book_line(b, "    "));
                }
            }
        }
    }
}

/// 分类列表（男频/女频/出版/听书）
pub async fn category(tab: u32, json: bool, _cfg: &Config) {
    let q = format!("category_tab={tab}");
    let Some(v) = fetch("/api/v1/category", &q, "分类").await else {
        return;
    };
    if json {
        return pretty(&v);
    }
    let Some(list) = v
        .get("data")
        .and_then(|d| d.get("down_category"))
        .and_then(|c| c.as_array())
    else {
        println!("  {DIM}（无分类数据）{RST}");
        return;
    };
    println!("  {} {} 推荐组", pad("ID", 8), pad("分类", 16));
    for c in list {
        let id = get(c, "id");
        let name = get(c, "name");
        let group = get(c, "recommend_group_id");
        println!("  {} {} {DIM}{group}{RST}", pad(&id, 8), pad(&name, 16));
    }
}

/// 书评列表
pub async fn comments(book_id: &str, count: usize, offset: usize, json: bool, _cfg: &Config) {
    let q = format!("count={count}&offset={offset}");
    let path = format!("/api/v1/books/{book_id}/reviews");
    let Some(v) = fetch(&path, &q, "书评").await else {
        return;
    };
    if json {
        return pretty(&v);
    }
    let book = get(&v, "data.extra.book_info.book_name");
    let score = get(&v, "data.extra.score_text");
    println!(
        "  {CYAN}{}{RST} {DIM}{score}{RST}",
        if book.is_empty() { book_id } else { &book }
    );
    let Some(items) = v
        .get("data")
        .and_then(|d| d.get("data_list"))
        .and_then(|l| l.as_array())
    else {
        println!("  {DIM}（暂无书评）{RST}");
        return;
    };
    for item in items {
        let Some(c) = item.get("comment") else { continue };
        let user = {
            let u = get(c, "common.user_info.base_info.user_name");
            if u.is_empty() { get(c, "common.user_info.user_name") } else { u }
        };
        let digg = get(c, "stat.digg_count");
        let text = clip(&get(c, "common.content.text"), 100);
        println!(
            "  {YELLOW}{}{RST} {DIM}赞 {digg}{RST}",
            if user.is_empty() { "匿名" } else { &user }
        );
        println!("    {text}");
        println!();
    }
}

/// 作者资料 + 作者书单
pub async fn author(author_id: &str, count: usize, json: bool, _cfg: &Config) {
    let Some(info) = fetch(
        &format!("/api/v1/authors/{author_id}"),
        "",
        "作者资料",
    )
    .await
    else {
        return;
    };
    let bookshelf = fetch(
        &format!("/api/v1/authors/{author_id}/bookshelf"),
        &format!("count={count}&offset=0"),
        "作者书单",
    )
    .await;
    if json {
        let merged = serde_json::json!({
            "author": info.get("data").cloned().unwrap_or(Value::Null),
            "bookshelf": bookshelf
                .as_ref()
                .and_then(|b| b.get("data"))
                .cloned()
                .unwrap_or(Value::Null),
        });
        return pretty(&merged);
    }
    let name = get(&info, "data.user_name");
    let fans = get(&info, "data.fans_num");
    let follow = get(&info, "data.follow_user_num");
    let read = get(&info, "data.read_book_num");
    let desc = get(&info, "data.author_desc");
    println!(
        "  {CYAN}{}{RST} {DIM}粉丝 {fans} | 关注 {follow} | 阅读 {read}{RST}",
        if name.is_empty() { author_id } else { &name }
    );
    if !desc.is_empty() {
        println!("  {DIM}{}{RST}", clip(&desc, 120));
    }
    let Some(items) = bookshelf
        .as_ref()
        .and_then(|b| b.get("data"))
        .and_then(|d| d.get("items"))
        .and_then(|i| i.as_array())
    else {
        return;
    };
    println!("\n  {CYAN}书单{RST}");
    for it in items {
        if let Some(b) = it.get("book") {
            if !get(b, "book_name").is_empty() {
                println!("{}", book_line(b, "    "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{clip, pad};

    #[test]
    fn test_clip() {
        assert_eq!(clip("abc", 2), "ab…");
        assert_eq!(clip("ab", 2), "ab");
    }

    #[test]
    fn test_pad_cjk() {
        assert_eq!(pad("玄幻", 6), "玄幻  ");
        assert_eq!(pad("ab", 4), "ab  ");
    }
}
