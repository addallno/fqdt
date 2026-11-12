use crate::config;
use crate::types;
use crate::types::Config;
use crate::util::{CYAN, DIM, RST};

pub async fn run(add: Option<String>, delete: Option<usize>, dl: Option<usize>, update: bool, cfg: &Config) {
    if update {
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
                verbose: false, book_title: Some(title.clone()),
            }, cfg).await;
        }
        return;
    }
    if let Some(id_title) = add {
        if let Some((id, title)) = id_title.split_once(':') {
            match config::add_bookmark(id, title) {
                Ok(_) => println!("  ok 已添加"),
                Err(e) => eprintln!("  err {}", e),
            }
        } else {
            eprintln!("  err 格式: <ID>:<标题>");
        }
        return;
    }
    if let Some(idx) = delete {
        match config::remove_bookmark(idx) {
            Ok(_) => println!("  ok 已删除 #{}", idx),
            Err(e) => eprintln!("  err {}", e),
        }
        return;
    }
    if let Some(idx) = dl {
        let books = config::load_bookmarks();
        if idx == 0 || idx > books.len() {
            eprintln!("  err 无效编号");
            return;
        }
        let (id, title) = &books[idx - 1];
        super::download::run(id, &types::DownloadParams {
            output: None, range: None, format: None, concurrent: None,
            audio: false, tone: 1, abr: 0,
            lrc: "external".into(), force: false,
            verbose: false, book_title: Some(title.clone()),
        }, cfg).await;
        return;
    }
    let books = config::load_bookmarks();
    if books.is_empty() {
        println!("  书架为空");
        eprintln!("  {DIM}添加: fqdt shelf -a <ID>:<标题>{RST}");
        return;
    }
    println!("  {CYAN}【书架】{RST} · {} 本", books.len());
    for (i, (id, t)) in books.iter().enumerate() {
        println!("  {:>2}. {CYAN}{}{RST}  {DIM}#{}{RST}", i + 1, t, id);
    }
    eprintln!("\n  {DIM}添加 -a <ID>:<标题> · 删除 -d <编号> · 下载 -D <编号> · 更新 -U{RST}");
}
