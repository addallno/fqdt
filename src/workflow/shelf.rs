//! # shelf — 书架管理（本地书架 + 云端书架入口）
//!
//! 42②：`fqdt shelf` 子命令化，裸输入报错；`list` 为书架入口——
//! 未登录/`--local` 显示本地书架，已登录显示云端（42① 一层视图）。

use super::account;
use crate::config;
use crate::types;
use crate::types::Config;
use crate::util::{CYAN, DIM, RED, RST};
use crate::ShelfCmd;

/// `fqdt shelf <sub>` 分发；裸 shelf 报错要求子命令（不打迁移提示，46）。
pub async fn run(cmd: Option<ShelfCmd>, cfg: &Config) {
    let Some(cmd) = cmd else {
        return eprintln!("  {RED}err{RST} 用法: fqdt shelf <list|add|remove|move|check|delete|download|update>");
    };
    match cmd {
        ShelfCmd::List { group, local, search, author, json } =>
            list(group, local, search, author, json, cfg).await,
        ShelfCmd::Add { book } => add(&book).await,
        ShelfCmd::Remove { book_id } => cloud_op("remove", &book_id, None).await,
        ShelfCmd::Move { book_id, group } => cloud_op("move", &book_id, Some(&group)).await,
        ShelfCmd::Check { book_id } => cloud_op("check", &book_id, None).await,
        ShelfCmd::Delete { index } => local_delete(index),
        ShelfCmd::Download { index } => local_download(index, cfg).await,
        ShelfCmd::Update => local_update(cfg).await,
    }
}

/// 书架入口：`--local` 或未登录 → 本地；已登录 → 云端（组名/搜索/作者/JSON 透传）。
async fn list(group: Option<String>, local: bool, search: Option<String>,
    author: bool, json: bool, cfg: &Config) {
    let cookie = if local { None } else { config::load_cookie() };
    match cookie {
        Some(c) => {
            account::shelf_section(&c, json, search.as_deref(), author, group.as_deref(), cfg).await;
        }
        None => {
            if group.is_some() {
                return eprintln!("  {RED}err{RST} 本地书架无分组（组名需登录后使用）");
            }
            if !local {
                crate::emit_line(&format!("  {DIM}未登录，显示本地书架（fqdt login 设置后查看云端）{RST}"));
            }
            local_list();
        }
    }
}

/// 加书：含冒号走本地 `<id>:<标题>`，否则走云端 add。
async fn add(book: &str) {
    if let Some((id, title)) = book.split_once(':') {
        return match config::add_bookmark(id, title) {
            Ok(_) => println!("  ok 已添加"),
            Err(e) => eprintln!("  err {e}"),
        };
    }
    cloud_op("add", book, None).await;
}

/// 云端写操作统一入口：读 Cookie 后交 account::shelf_op。
async fn cloud_op(op: &str, book_id: &str, group: Option<&str>) {
    let Some(cookie) = account::need_cookie() else { return };
    account::shelf_op(&cookie, op, book_id, group).await;
}

/// 本地书架列表。
fn local_list() {
    let books = config::load_bookmarks();
    if books.is_empty() {
        println!("  书架为空");
        return;
    }
    println!("  {CYAN}【书架】{RST} · {} 本", books.len());
    for (i, (id, t)) in books.iter().enumerate() {
        println!("  {:>2}. {CYAN}{}{RST}  {DIM}#{}{RST}", i + 1, t, id);
    }
}

/// 删除本地书架条目（编号从 1 起）。
fn local_delete(index: usize) {
    match config::remove_bookmark(index) {
        Ok(_) => println!("  ok 已删除 #{index}"),
        Err(e) => eprintln!("  err {e}"),
    }
}

/// 下载本地书架某本（编号从 1 起）。
async fn local_download(index: usize, cfg: &Config) {
    let books = config::load_bookmarks();
    if index == 0 || index > books.len() {
        eprintln!("  err 无效编号");
        return;
    }
    let (id, title) = &books[index - 1];
    super::download::run(id, &types::DownloadParams {
        output: None, range: None, format: None, concurrent: None,
        audio: false, tone: 1, abr: 0,
        lrc: "external".into(), force: false,
        verbose: false, only: None, book_title: Some(title.clone()),
    }, cfg).await;
}

/// 逐本更新本地书架。
async fn local_update(cfg: &Config) {
    let books = config::load_bookmarks();
    if books.is_empty() {
        println!("  书架为空");
        return;
    }
    println!("  {CYAN}【更新书架】{RST} · {} 本", books.len());
    for (i, (id, title)) in books.iter().enumerate() {
        println!("\n  [{}/{}] {CYAN}{}{RST}", i + 1, books.len(), title);
        super::download::run(id, &types::DownloadParams {
            output: None, range: None, format: None, concurrent: None,
            audio: false, tone: 1, abr: 0,
            lrc: "external".into(), force: false,
            verbose: false, only: None, book_title: Some(title.clone()),
        }, cfg).await;
    }
}
