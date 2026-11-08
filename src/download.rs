use crate::api::Client;
use crate::epub;
use crate::types::{sanitize_filename, Chapter};
use crate::util;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub type InfoList = (String, String, String, Vec<(usize, String, String)>);
pub type AudioInfoList = (String, String, Vec<(usize, String, String)>);

pub struct Downloader {
    pub api: Client,
    out_dir: PathBuf,
    format: String,
    ft: String,
    force: bool,
    verbose: bool,
    book_id: String,
    book_title: String,
}

impl Downloader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(api: Client, out_dir: PathBuf, format: &str, ft: &str, force: bool, verbose: bool,
               book_id: &str, book_title: &str) -> Self {
        Downloader { api, out_dir, format: format.into(), ft: ft.into(), force, verbose,
            book_id: book_id.into(), book_title: book_title.into() }
    }

    pub async fn run(&self, chapters: &[&Chapter], concurrent: usize) {
        if chapters.is_empty() { println!("  无章节"); return; }
        fs::create_dir_all(&self.out_dir).expect("创建目录失败");
        if self.format == "epub" {
            self.do_epub(chapters, concurrent).await;
        } else {
            self.do_files(chapters, concurrent).await;
        }
        self.write_info_list(chapters);
    }

    async fn do_files(&self, chapters: &[&Chapter], concurrent: usize) {
        let total = chapters.len();
        let pending: Vec<Chapter> = chapters.iter()
            .filter(|c| self.force || !self.out_dir.join(self.fname(c)).exists())
            .map(|c| (*c).clone()).collect();
        let skipped = total - pending.len();

        if pending.is_empty() {
            println!("  全部已存在 ({}/{})", skipped, total);
            return;
        }

        let api = self.api.clone();
        let out_dir = self.out_dir.clone();
        let ft = self.ft.clone();
        let vb = self.verbose;
        let bid = self.book_id.clone();

        // 尝试批量获取，失败则逐章获取
        let item_ids: Vec<&str> = pending.iter().map(|c| c.item_id.as_str()).collect();
        let batch_map = api.fetch_content_batch(&bid, &item_ids).await.ok();

        let (ok, fail) = util::with_progress_async(pending, total, concurrent, "cyan/blue", move |ch, pb| {
            let api = api.clone();
            let out_dir = out_dir.clone();
            let ft = ft.clone();
            let batch_map = batch_map.clone();
            async move {
                let r = dl_file_batch(&api, &out_dir, &ft, &ch, vb, &batch_map).await;
                match &r {
                    Ok(_) => pb.set_message(format!("✓{:04}", ch.index)),
                    Err(e) => pb.set_message(format!("✗{:04}:{}", ch.index, e)),
                }
                r
            }
        }).await;

        println!("  ok {ok} 章 · 跳过 {skipped} 章{}", if fail > 0 { format!(" · 失败 {fail} 章") } else { String::new() });
    }

    async fn do_epub(&self, chapters: &[&Chapter], concurrent: usize) {
        let title = sanitize_filename(&self.book_title);
        let epub_path = self.out_dir.join(format!("{}.epub", title));
        let resolved: Vec<Chapter> = chapters.iter().map(|c| (*c).clone()).collect();

        println!("  生成 EPUB...");
        if let Err(e) = epub::generate(&title, &resolved, &epub_path) {
            eprintln!("  EPUB 创建失败: {}", e); return;
        }

        let total = chapters.len();
        let api = self.api.clone();
        let ep = epub_path.clone();
        let vb = self.verbose;

        let (ok, fail) = util::with_progress_async(resolved, total, concurrent, "cyan/blue", move |ch, pb| {
            let api = api.clone();
            let ep = ep.clone();
            async move {
                match api.fetch_content(&ch.item_id).await {
                    Ok(text) => {
                        if let Err(e) = epub::update_chapter(&ep, &ch, &text) {
                            pb.set_message(format!("✗{:04}:{}", ch.index, e));
                            Err(e)
                        } else {
                            pb.set_message(format!("✓{:04}", ch.index));
                            if vb { eprintln!("  {:04} {} ✓", ch.index, ch.title); }
                            Ok(())
                        }
                    }
                    Err(e) => {
                        pb.set_message(format!("✗{:04}:{}", ch.index, e));
                        Err(e)
                    }
                }
            }
        }).await;
        println!("  ok {} 章 → {}", ok, epub_path.display());
        if fail > 0 { println!("  失败 {} 章", fail); }
    }

    fn fname(&self, ch: &Chapter) -> String {
        util::format_filename(&self.ft, ch)
    }
}

async fn dl_file_batch(api: &Client, out_dir: &Path, ft: &str, ch: &Chapter, verbose: bool,
                 batch_map: &Option<std::collections::HashMap<String, String>>) -> Result<(), String> {
    let content = match batch_map.as_ref().and_then(|m| m.get(&ch.item_id)) {
        Some(c) => c.clone(),
        None => api.fetch_content(&ch.item_id).await?,
    };
    write_chapter(out_dir, ft, ch, &content, verbose)
}

fn write_chapter(out_dir: &Path, ft: &str, ch: &Chapter, content: &str, verbose: bool) -> Result<(), String> {
    let name = util::format_filename(ft, ch);
    let path = out_dir.join(format!("{}.txt", name));
    let heading = if util::has_chapter_prefix(&ch.title) { ch.title.clone() } else { format!("第{}章 {}", ch.index, ch.title) };
    let text = format!("{}\n\n{}\n", heading, content);
    // 哈希对比：已存在且内容相同则跳过写入
    if path.exists() {
        if let Ok(old) = std::fs::read_to_string(&path) {
            if old == text {
                if verbose { eprintln!("  {:04} {} 不变 ✓", ch.index, ch.title); }
                return Ok(());
            }
        }
    }
    let mut f = std::fs::File::create(&path).map_err(|e| format!("写入: {}", e))?;
    f.write_all(text.as_bytes()).map_err(|e| format!("写入: {}", e))?;
    if verbose { eprintln!("  {:04} {} ✓", ch.index, ch.title); }
    Ok(())
}

impl Downloader {
    fn write_info_list(&self, chapters: &[&Chapter]) {
        let path = self.out_dir.join("info.list");
        let items: Vec<String> = chapters.iter().map(|ch| {
            format!("    {{\"idx\":{}, \"title\":\"{}\", \"file\":\"{}\"}}",
                ch.index, util::json_esc(&ch.title), util::json_esc(&self.fname(ch)))
        }).collect();
        let json = format!("{{\n  \"book_id\": \"{}\",\n  \"book_title\": \"{}\",\n  \"format\": \"{}\",\n  \"chapters\": [\n{}\n  ]\n}}\n",
            util::json_esc(&self.book_id), util::json_esc(&self.book_title), self.format, items.join(",\n"));
        fs::write(&path, json).ok();
    }
}

pub fn read_info_list(dir: &std::path::Path) -> Result<InfoList, String> {
    let text = fs::read_to_string(dir.join("info.list")).map_err(|e| format!("读 info.list: {}", e))?;
    let root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("JSON: {}", e))?;
    let book_id = root["book_id"].as_str().unwrap_or("").to_string();
    let book_title = root["book_title"].as_str().unwrap_or("").to_string();
    let format = root["format"].as_str().unwrap_or("txt").to_string();
    let mut chapters = vec![];
    if let Some(arr) = root["chapters"].as_array() {
        for item in arr {
            let idx = item["idx"].as_u64().unwrap_or(0) as usize;
            let title = item["title"].as_str().unwrap_or("").to_string();
            let file = item["file"].as_str().unwrap_or("").to_string();
            chapters.push((idx, title, file));
        }
    }
    if book_id.is_empty() { return Err("info.list 缺少 book_id".into()); }
    Ok((book_id, book_title, format, chapters))
}

pub fn read_audio_info_list(dir: &std::path::Path) -> Result<AudioInfoList, String> {
    let text = fs::read_to_string(dir.join("info.list")).map_err(|e| format!("读 Audio/info.list: {}", e))?;
    let root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("JSON: {}", e))?;
    let book_id = root["book_id"].as_str().unwrap_or("").to_string();
    let book_title = root["book_title"].as_str().unwrap_or("").to_string();
    let mut chapters = vec![];
    if let Some(arr) = root["chapters"].as_array() {
        for item in arr {
            let idx = item["idx"].as_u64().unwrap_or(0) as usize;
            let title = item["title"].as_str().unwrap_or("").to_string();
            let file = item["file"].as_str().unwrap_or("").to_string();
            chapters.push((idx, title, file));
        }
    }
    if book_id.is_empty() { return Err("Audio/info.list 缺少 book_id".into()); }
    Ok((book_id, book_title, chapters))
}
