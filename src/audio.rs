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
    abr: u32,
    speed: Option<f32>,
    normalize: bool,
    post_cmd: String,
    lrc_mode: String,
}

impl AudioDownloader {
    pub fn new(api: Client, out_dir: PathBuf, tone: usize, fallbacks: Vec<usize>, ft: &str, verbose: bool,
               abr: u32, speed: Option<f32>, normalize: bool, post_cmd: &str, lrc_mode: &str) -> Self {
        AudioDownloader { api, out_dir, tone, fallbacks, ft: ft.into(), verbose, abr, speed, normalize, post_cmd: post_cmd.into(), lrc_mode: lrc_mode.into() }
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
                self.api.audio_content_urls.clone(), vb, self.api.timeout,
                self.api.http_method.clone(), self.api.curl_args.clone());
            let od = self.out_dir.clone();
            let ft = self.ft.clone();
            let tone = self.tone;
            let fallbacks = self.fallbacks.clone();
            let abr = self.abr;
            let speed = self.speed;
            let normalize = self.normalize;
            let post_cmd = self.post_cmd.clone();
            let lrc_mode = self.lrc_mode.clone();

            handles.push(thread::spawn(move || {
                for i in (w..n).step_by(concurrent) {
                    let ch = &p[i];
                    let r = dl_one(&a, &od, &ft, ch, tone, &fallbacks, abr, speed, normalize, &post_cmd, &lrc_mode, vb);
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

// ── Download single chapter ─────────────────────────────────

fn dl_one(api: &Client, out_dir: &PathBuf, ft: &str, ch: &Chapter,
          tone: usize, fallbacks: &[usize],
          abr: u32, speed: Option<f32>, normalize: bool, post_cmd: &str, lrc_mode: &str,
          verbose: bool) -> Result<(), String> {
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
            Ok(_) => {
                post_process(&path, abr, speed, normalize, post_cmd, verbose);
                handle_lrc(&path, ch, lrc_mode, verbose);
                return Ok(());
            }
            Err(e) => { last_err = format!("tone={}: {}", t, e); }
        }
    }
    Err(last_err)
}

fn download_file(url: &str, path: &PathBuf, verbose: bool) -> Result<(), String> {
    if verbose { eprintln!("  [verbose] DL {}", &url[..url.len().min(80)]); }

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

    let q = format!("curl -sfL --connect-timeout 15 --max-time 120 -o '{}' '{}'",
        path.to_string_lossy().replace('\'', "'\\''"), url);
    let r = std::process::Command::new("grun").args(["-s", &q]).output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
    }

    let resp = minreq::get(url).with_timeout(120).send().map_err(|e| format!("请求失败: {}", e))?;
    let data = resp.as_bytes().to_vec();
    if data.len() < 1000 { return Err(format!("文件太小: {}b", data.len())); }
    fs::write(path, &data).map_err(|e| format!("写入: {}", e))
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:28.green/cyan} {pos}/{len} {msg}")
        .unwrap().progress_chars("━▶")
}

// ── Post-processing ─────────────────────────────────────────

pub fn post_process(path: &Path, abr: u32, speed: Option<f32>, normalize: bool,
                    cmd_template: &str, verbose: bool) {
    if abr == 0 && speed.is_none() && !normalize && cmd_template.is_empty() { return; }
    if !path.exists() { return; }

    let tmp = path.with_extension("tmp.mp3");

    if !cmd_template.is_empty() {
        let input_s = path.to_string_lossy().to_string();
        let output_s = tmp.to_string_lossy().to_string();
        let cmd = cmd_template.replace("{input}", &input_s).replace("{output}", &output_s);
        if verbose { eprintln!("  [verbose] post-process: {}", cmd); }
        if let Ok(out) = std::process::Command::new("sh").arg("-c").arg(&cmd).output() {
            if out.status.success() && tmp.exists() { fs::rename(&tmp, path).ok(); }
            else if verbose { eprintln!("  [verbose] post-process fail: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        }
        return;
    }

    if abr > 0 {
        let p_s = path.to_string_lossy().to_string();
        let t_s = tmp.to_string_lossy().to_string();
        let abr_s = abr.to_string();
        let args = vec!["--abr", &abr_s, "--silent", &p_s, &t_s];
        if let Ok(out) = std::process::Command::new("lame").args(&args).output() {
            if out.status.success() && tmp.exists() { fs::rename(&tmp, path).ok(); }
            else if verbose { eprintln!("  [verbose] lame fail: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        }
    }
}

// ── LRC ─────────────────────────────────────────────────────

fn chapter_heading(ch: &Chapter) -> String {
    if ch.title.starts_with('第') || ch.title.starts_with(|c: char| c.is_ascii_digit()) {
        ch.title.clone()
    } else {
        format!("第{}章 {}", ch.index, ch.title)
    }
}

pub fn gen_lrc_text(ch: &Chapter) -> String {
    format!("[00:00.00]{}\n", chapter_heading(ch))
}

pub fn write_lrc_file(path: &Path, ch: &Chapter) {
    let lrc_path = path.with_extension("lrc");
    fs::write(&lrc_path, gen_lrc_text(ch)).ok();
}

pub fn embed_lrc(mp3_path: &Path, ch: &Chapter, verbose: bool) {
    use id3::{Tag, TagLike, Frame, Content};
    use id3::frame::Lyrics;

    let lrc_text = gen_lrc_text(ch);
    let mut tag = match Tag::read_from_path(mp3_path) {
        Ok(t) => t,
        Err(_) => Tag::new(),
    };

    tag.add_frame(Frame::with_content("USLT", Content::Lyrics(Lyrics {
        lang: "chi".into(),
        description: "lrc".into(),
        text: lrc_text,
    })));

    if let Err(e) = tag.write_to_path(mp3_path, id3::Version::Id3v24) {
        if verbose { eprintln!("  [verbose] embed lrc fail: {}", e); }
    }
}

fn handle_lrc(path: &Path, ch: &Chapter, mode: &str, verbose: bool) {
    match mode {
        "external" => write_lrc_file(path, ch),
        "embed" => embed_lrc(path, ch, verbose),
        "both" => { write_lrc_file(path, ch); embed_lrc(path, ch, verbose); }
        _ => {} // "off" or unknown
    }
}

// ── TTS conversion ──────────────────────────────────────────

pub fn convert_tts_file(input: &Path, output_dir: Option<PathBuf>, voice: &str,
                        rate: &str, volume: &str, pitch: &str,
                        abr: u32, speed: Option<f32>, normalize: bool,
                        cmd: &str, lrc_mode: &str, verbose: bool) {
    let name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let out = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
    fs::create_dir_all(&out).expect("创建目录失败");
    let out_path = out.join(format!("{}.mp3", name));
    if out_path.exists() {
        println!("  ok 已存在: {}", out_path.display());
        return;
    }
    let text = match fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => { eprintln!("  err 读文件: {}", e); return; }
    };
    println!("  {} → {}", name, out_path.display());
    if let Err(e) = run_edge_tts(&text, voice, rate, volume, pitch, &out_path, verbose) {
        eprintln!("  err {}", e);
        return;
    }
    post_process(&out_path, abr, speed, normalize, cmd, verbose);
    handle_lrc(&out_path, &Chapter { index: 0, title: name.into(), item_id: String::new() }, lrc_mode, verbose);
}

pub fn convert_tts_dir(input: &Path, output_dir: Option<PathBuf>, voice: &str,
                       rate: &str, volume: &str, pitch: &str,
                       abr: u32, speed: Option<f32>, normalize: bool,
                       cmd: &str, lrc_mode: &str, verbose: bool) {
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
        Err(e) => { eprintln!("  err 读目录: {}", e); return; }
    };
    if entries.is_empty() { println!("  无 .txt 文件"); return; }

    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:28.magenta/cyan} {pos}/{len} {msg}")
        .unwrap().progress_chars("━▶"));

    let mut failed = 0usize;
    for entry in &entries {
        let inp = entry.path();
        let name = inp.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
        let out_path = out.join(format!("{}.mp3", name));
        if out_path.exists() { pb.inc(1); continue; }
        let text = match fs::read_to_string(&inp) {
            Ok(t) => t,
            Err(e) => { pb.set_message(format!("✗{}: {}", name, e)); failed += 1; pb.inc(1); continue; }
        };
        pb.set_message(name.clone());
        if let Err(e) = run_edge_tts(&text, voice, rate, volume, pitch, &out_path, verbose) {
            pb.set_message(format!("✗{}: {}", name, e));
            failed += 1;
        } else {
            post_process(&out_path, abr, speed, normalize, cmd, verbose);
            handle_lrc(&out_path, &Chapter { index: 0, title: name.clone(), item_id: String::new() }, lrc_mode, verbose);
        }
        pb.inc(1);
        thread::sleep(std::time::Duration::from_millis(100));
    }
    pb.finish_and_clear();
    let total = entries.len();
    println!("  完成 {}/{}", total - failed, total);
    if failed > 0 { println!("  失败 {} 文件", failed); }
}

fn run_edge_tts(text: &str, voice: &str, rate: &str, volume: &str, pitch: &str,
                out_path: &Path, verbose: bool) -> Result<(), String> {
    let r = std::process::Command::new("edge-tts")
        .arg("-t").arg(text)
        .arg("-v").arg(voice)
        .arg("--rate").arg(rate)
        .arg("--volume").arg(volume)
        .arg("--pitch").arg(pitch)
        .arg("--write-media").arg(&out_path.to_string_lossy().to_string())
        .output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
        if verbose {
            eprintln!("  [verbose] edge-tts: status={}, err={}", out.status, String::from_utf8_lossy(&out.stderr).trim());
        }
        return Err(format!("edge-tts 退出码 {}", out.status));
    }
    Err("找不到 edge-tts 命令".into())
}
