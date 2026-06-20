use crate::api::Client;
use crate::epub;
use crate::types::{Chapter, sanitize_filename};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

pub struct Downloader {
    api: Client,
    out_dir: PathBuf,
    format: String,
    ft: String,
    verbose: bool,
}

impl Downloader {
    pub fn new(api: Client, out_dir: PathBuf, format: &str, ft: &str, verbose: bool) -> Self {
        Downloader { api, out_dir, format: format.into(), ft: ft.into(), verbose }
    }

    pub fn run(&self, chapters: &[&Chapter], book_title: Option<&str>, concurrent: usize) {
        if chapters.is_empty() { println!("  无章节"); return; }
        fs::create_dir_all(&self.out_dir).expect("创建目录失败");
        if self.format == "epub" {
            return self.do_epub(chapters, book_title, concurrent);
        }
        self.do_files(chapters, concurrent);
    }

    fn do_files(&self, chapters: &[&Chapter], concurrent: usize) {
        let total = chapters.len();
        let pb = ProgressBar::new(total as u64);
        pb.set_style(bar_style());

        let pending: Vec<Chapter> = chapters.iter()
            .filter(|c| !self.out_dir.join(self.fname(c)).exists())
            .map(|c| (*c).clone()).collect();

        let skipped = total - pending.len();
        pb.inc(skipped as u64);

        if pending.is_empty() {
            pb.finish_and_clear();
            println!("  全部已存在 ({}/{})", skipped, total);
            return;
        }

        let failed = Arc::new(AtomicUsize::new(0));
        let pending = Arc::new(pending);
        let n = pending.len();
        let count = concurrent.max(1);
        let mut handles = vec![];

        for w in 0..count {
            let p = pending.clone();
            let pb = pb.clone();
            let fl = failed.clone();
            let vb = self.verbose;
            let a = Client::new(self.api.cache_dir.clone(), self.api.cache_enabled, self.api.cache_ttl,
                self.api.search_urls.clone(), self.api.catalog_url.clone(), self.api.content_urls.clone(),
                vb);
            let od = self.out_dir.clone();
            let ft = self.ft.clone();

            handles.push(thread::spawn(move || {
                for i in (w..n).step_by(count) {
                    let ch = &p[i];
                    let r = dl_file(&a, &od, &ft, ch, vb);
                    match &r { Ok(_) => pb.set_message(format!("✓{:04}", ch.index)),
                        Err(e) => { fl.fetch_add(1, Ordering::SeqCst); pb.set_message(format!("✗{:04}:{}", ch.index, e)); }
                    }
                    pb.inc(1);
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }));
        }
        for h in handles { h.join().unwrap(); }

        pb.finish_and_clear();
        let f = failed.load(Ordering::SeqCst);
        println!("  完成 {}/{} (跳过 {})", n - f, total, skipped);
        if f > 0 { println!("  失败 {} 章", f); }
    }

    fn do_epub(&self, chapters: &[&Chapter], book_title: Option<&str>, concurrent: usize) {
        let title = sanitize_filename(book_title.unwrap_or("小说"));
        let epub_path = self.out_dir.join(format!("{}.epub", title));

        let resolved: Vec<Chapter> = chapters.iter().map(|c| (*c).clone()).collect();
        println!("  生成 EPUB...");
        if let Err(e) = epub::generate(&title, &resolved, &epub_path) {
            eprintln!("  EPUB 创建失败: {}", e); return;
        }

        let total = chapters.len();
        let pb = ProgressBar::new(total as u64);
        pb.set_style(bar_style());
        let failed = Arc::new(AtomicUsize::new(0));
        let count = concurrent.max(1);
        let mut handles = vec![];
        let ep = Arc::new(epub_path);

        for w in 0..count {
            let chs = resolved.clone();
            let pb = pb.clone();
            let fl = failed.clone();
            let ep = ep.clone();
            let vb = self.verbose;
            let a = Client::new(self.api.cache_dir.clone(), self.api.cache_enabled, self.api.cache_ttl,
                self.api.search_urls.clone(), self.api.catalog_url.clone(), self.api.content_urls.clone(),
                vb);

            handles.push(thread::spawn(move || {
                for i in (w..total).step_by(count) {
                    let ch = &chs[i];
                    match a.fetch_content(&ch.item_id) {
                        Ok(text) => {
                            if let Err(e) = epub::update_chapter(&ep, ch, &text) {
                                fl.fetch_add(1, Ordering::SeqCst);
                                pb.set_message(format!("✗{:04}:{}", ch.index, e));
                            } else { pb.set_message(format!("✓{:04}", ch.index)); }
                        }
                        Err(e) => { fl.fetch_add(1, Ordering::SeqCst); pb.set_message(format!("✗{:04}:{}", ch.index, e)); }
                    }
                    pb.inc(1);
                    if vb { eprintln!("  {:04} {} ✓", ch.index, ch.title); }
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }));
        }
        for h in handles { h.join().unwrap(); }

        pb.finish_and_clear();
        let f = failed.load(Ordering::SeqCst);
        println!("  完成 {}/{} → {}", total - f, total, ep.as_ref().display());
        if f > 0 { println!("  失败 {} 章", f); }
    }

    fn fname(&self, ch: &Chapter) -> String {
        self.ft.replace("{idx04}", &format!("{:04}", ch.index))
            .replace("{idx}", &ch.index.to_string())
            .replace("{title}", &sanitize_filename(&ch.title))
    }
}

fn dl_file(api: &Client, out_dir: &PathBuf, ft: &str, ch: &Chapter, verbose: bool) -> Result<(), String> {
    let content = api.fetch_content(&ch.item_id)?;
    let name = ft.replace("{idx04}", &format!("{:04}", ch.index))
        .replace("{idx}", &ch.index.to_string())
        .replace("{title}", &sanitize_filename(&ch.title));
    let path = out_dir.join(format!("{}.txt", name));
    let heading = if has_chapter_prefix(&ch.title) { ch.title.clone() } else { format!("第{}章 {}", ch.index, ch.title) };
    let text = format!("{}\n\n{}\n", heading, content);
    let mut f = fs::File::create(&path).map_err(|e| format!("写入: {}", e))?;
    f.write_all(text.as_bytes()).map_err(|e| format!("写入: {}", e))?;
    if verbose { eprintln!("  {:04} {} ✓", ch.index, ch.title); }
    Ok(())
}

fn has_chapter_prefix(title: &str) -> bool {
    title.starts_with('第') || title.starts_with(|c: char| c.is_ascii_digit())
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:28.cyan/blue} {pos}/{len} {msg}")
        .unwrap().progress_chars("━▶")
}
