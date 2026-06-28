use crate::api::Client;
use crate::types::{Chapter, sanitize_filename};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

pub struct AudioDownloader {
    api: Client,
    out_dir: PathBuf,
    tone: usize,
    fallbacks: Vec<usize>,
    ft: String,
    verbose: bool,
}

impl AudioDownloader {
    pub fn new(api: Client, out_dir: PathBuf, tone: usize, fallbacks: Vec<usize>, ft: &str, verbose: bool) -> Self {
        AudioDownloader { api, out_dir, tone, fallbacks, ft: ft.into(), verbose }
    }

    pub fn run(&self, chapters: &[&Chapter], book_title: Option<&str>) {
        if chapters.is_empty() { println!("  无章节"); return; }
        fs::create_dir_all(&self.out_dir).expect("创建目录失败");

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
            self.write_info_list(chapters, book_title);
            return;
        }

        let failed = Arc::new(AtomicUsize::new(0));
        let pending = Arc::new(pending);
        let n = pending.len();
        let concurrent = 4;
        let mut handles = vec![];

        for w in 0..concurrent {
            let p = pending.clone();
            let pb = pb.clone();
            let fl = failed.clone();
            let vb = self.verbose;
            let a = Client::new(self.api.cache_dir.clone(), self.api.cache_enabled, self.api.cache_ttl,
                self.api.search_urls.clone(), self.api.catalog_url.clone(), self.api.content_urls.clone(),
                self.api.audio_content_urls.clone(), vb, self.api.timeout);
            let od = self.out_dir.clone();
            let ft = self.ft.clone();
            let tone = self.tone;
            let fallbacks = self.fallbacks.clone();

            handles.push(thread::spawn(move || {
                for i in (w..n).step_by(concurrent) {
                    let ch = &p[i];
                    let r = download_audio(&a, &od, &ft, ch, tone, &fallbacks, vb);
                    match &r {
                        Ok(_) => pb.set_message(format!("✓{:04}", ch.index)),
                        Err(e) => { fl.fetch_add(1, Ordering::SeqCst); pb.set_message(format!("✗{:04}:{}", ch.index, e)); }
                    }
                    pb.inc(1);
                }
            }));
        }
        for h in handles { h.join().unwrap(); }

        pb.finish_and_clear();
        let f = failed.load(Ordering::SeqCst);
        println!("  完成 {}/{} (跳过 {})", n - f, total, skipped);
        if f > 0 { println!("  失败 {} 章", f); }

        self.write_info_list(chapters, book_title);
    }

    fn write_info_list(&self, chapters: &[&Chapter], book_title: Option<&str>) {
        let path = self.out_dir.join("info.list");
        let title = book_title.unwrap_or("未知");
        let mut json = format!("{{\n  \"book_title\": \"{}\",\n  \"tone\": {},\n  \"chapters\": [\n",
            title.replace('\\', "\\\\").replace('"', "\\\""), self.tone);
        for (i, ch) in chapters.iter().enumerate() {
            let file = self.fname(ch);
            let comma = if i + 1 < chapters.len() { "," } else { "" };
            json.push_str(&format!(
                "    {{\"idx\":{}, \"title\":\"{}\", \"file\":\"{}\"}}{}\n",
                ch.index, ch.title.replace('\\', "\\\\").replace('"', "\\\""), file, comma));
        }
        json.push_str("  ]\n}\n");
        fs::write(&path, json).ok();
    }

    fn fname(&self, ch: &Chapter) -> String {
        self.ft.replace("{idx04}", &format!("{:04}", ch.index))
            .replace("{idx}", &ch.index.to_string())
            .replace("{title}", &sanitize_filename(&ch.title))
            + ".mp3"
    }
}

fn download_audio(api: &Client, out_dir: &PathBuf, ft: &str, ch: &Chapter,
                  tone: usize, fallbacks: &[usize], verbose: bool) -> Result<(), String> {
    let name = ft.replace("{idx04}", &format!("{:04}", ch.index))
        .replace("{idx}", &ch.index.to_string())
        .replace("{title}", &sanitize_filename(&ch.title));
    let path = out_dir.join(format!("{}.mp3", name));

    if path.exists() { return Ok(()); }

    let mut all_tones = vec![tone];
    all_tones.extend(fallbacks.iter().filter(|&&t| t != tone));

    let mut last_err = String::new();
    for &t in &all_tones {
        let audio_url = match api.fetch_audio_url(&ch.item_id, t) {
            Ok(u) => u,
            Err(e) => { last_err = e; continue; }
        };
        match download_file(&audio_url, &path, verbose) {
            Ok(_) => return Ok(()),
            Err(e) => { last_err = format!("tone={}: {}", t, e); }
        }
    }
    Err(last_err)
}

fn download_file(url: &str, path: &PathBuf, verbose: bool) -> Result<(), String> {
    if verbose { eprintln!("  [verbose] DL {}", &url[..url.len().min(80)]); }

    // try curl
    let r = std::process::Command::new("curl")
        .args(["-sfL", "--connect-timeout", "15", "--max-time", "120",
            "-o", &path.to_string_lossy(), url])
        .output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
        if verbose {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("  [verbose] curl fail: {}, err={}", out.status, stderr.trim());
        }
    }

    // try grun curl
    let q = format!("curl -sfL --connect-timeout 15 --max-time 120 -o '{}' '{}'",
        path.to_string_lossy().replace('\'', "'\\''"), url);
    let r = std::process::Command::new("grun")
        .args(["-s", &q])
        .output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
    }

    // try minreq streaming
    let resp = minreq::get(url)
        .with_timeout(120)
        .send()
        .map_err(|e| format!("请求失败: {}", e))?;
    let data = resp.as_bytes().to_vec();
    if data.len() < 1000 { return Err(format!("文件太小: {}b", data.len())); }
    fs::write(path, &data).map_err(|e| format!("写入: {}", e))
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:28.green/cyan} {pos}/{len} {msg}")
        .unwrap().progress_chars("━▶")
}

// ── TTS conversion ──────────────────────────────────────────

pub fn convert_tts_file(input: &Path, output_dir: Option<PathBuf>, voice: &str, verbose: bool) {
    let name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let out = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
    fs::create_dir_all(&out).expect("创建目录失败");
    let out_path = out.join(format!("{}.mp3", name));
    if out_path.exists() {
        println!("  ✓ 已存在: {}", out_path.display());
        return;
    }
    let text = match fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => { eprintln!("  ✗ 读文件: {}", e); return; }
    };
    println!("  \"{}\" → {}", name, out_path.display());
    if let Err(e) = run_edge_tts(&text, voice, &out_path, verbose) {
        eprintln!("  ✗ {}", e);
    }
}

pub fn convert_tts_dir(input: &Path, output_dir: Option<PathBuf>, voice: &str, verbose: bool) {
    let out = output_dir.unwrap_or_else(|| {
        let mut p = input.to_path_buf();
        p.push("Audio");
        p
    });
    fs::create_dir_all(&out).expect("创建目录失败");

    let entries: Vec<_> = match fs::read_dir(input) {
        Ok(d) => d.filter_map(|e| e.ok()).filter(|e| {
            e.path().extension().map(|ext| ext == "txt").unwrap_or(false)
        }).collect(),
        Err(e) => { eprintln!("  ✗ 读目录: {}", e); return; }
    };
    if entries.is_empty() { println!("  无 .txt 文件"); return; }

    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:28.magenta/cyan} {pos}/{len} {msg}")
        .unwrap().progress_chars("━▶"));

    let mut failed = 0usize;
    for entry in &entries {
        let inp = entry.path();
        let name = inp.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        let out_path = out.join(format!("{}.mp3", name));
        if out_path.exists() {
            pb.inc(1);
            continue;
        }
        let text = match fs::read_to_string(&inp) {
            Ok(t) => t,
            Err(e) => { pb.set_message(format!("✗{}: {}", name, e)); failed += 1; pb.inc(1); continue; }
        };
        pb.set_message(format!("{}", name));
        if let Err(e) = run_edge_tts(&text, voice, &out_path, verbose) {
            pb.set_message(format!("✗{}: {}", name, e));
            failed += 1;
        }
        pb.inc(1);
        thread::sleep(std::time::Duration::from_millis(100));
    }
    pb.finish_and_clear();
    let total = entries.len();
    println!("  完成 {}/{}", total - failed, total);
    if failed > 0 { println!("  失败 {} 文件", failed); }
}

fn run_edge_tts(text: &str, voice: &str, out_path: &Path, verbose: bool) -> Result<(), String> {
    // try edge-tts CLI
    let r = std::process::Command::new("edge-tts")
        .arg("-t").arg(text)
        .arg("-v").arg(voice)
        .arg("--write-media").arg(&out_path.to_string_lossy().to_string())
        .output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
        if verbose {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("  [verbose] edge-tts: status={}, err={}", out.status, stderr.trim());
        }
        return Err(format!("edge-tts 退出码 {}", out.status));
    }
    Err("找不到 edge-tts 命令".into())
}
