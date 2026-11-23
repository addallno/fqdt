use crate::api::Client;
use crate::types::{default_concurrent, Chapter};
use crate::util;
use crate::util::{GREEN, RST};
use indicatif::ProgressBar;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;

#[derive(Clone)]
pub struct AudioParams {
    /// fqapi 音频接口必填；第三方降级路径忽略。
    pub book_id: String,
    pub tone: usize,
    pub fallbacks: Vec<usize>,
    pub ft: String,
    pub force: bool,
    pub verbose: bool,
    pub abr: u32,
    pub speed: Option<f32>,
    pub normalize: bool,
    pub post_cmd: String,
    pub lrc_mode: String,
}

pub struct TtsParams {
    pub voice: String,
    pub rate: String,
    pub volume: String,
    pub pitch: String,
    pub abr: u32,
    pub speed: Option<f32>,
    pub normalize: bool,
    pub cmd: String,
    pub lrc_mode: String,
    pub verbose: bool,
}

pub struct AudioDownloader {
    api: Client,
    out_dir: PathBuf,
    p: AudioParams,
}

impl AudioDownloader {
    pub fn new(api: Client, out_dir: PathBuf, p: AudioParams) -> Self {
        AudioDownloader { api, out_dir, p }
    }

    pub async fn run(&self, chapters: &[&Chapter], book_title: Option<&str>) {
        if chapters.is_empty() { println!("  无章节"); return; }
        fs::create_dir_all(&self.out_dir).expect("创建目录失败");

        let total = chapters.len();
        let need_lrc = self.p.lrc_mode != "off";
        let pending: Vec<Chapter> = chapters.iter()
            .filter(|c| {
                if self.p.force { return true; }
                let stem = util::format_filename(&self.p.ft, c);
                let Some(audio) = existing_audio(&self.out_dir, &stem) else { return true; };
                if need_lrc && !audio.with_extension("lrc").exists() { return true; }
                false
            })
            .map(|c| (*c).clone()).collect();
        let skip = total - pending.len();

        if pending.is_empty() {
            println!("{GREEN}ok{RST} 全部已存在 · {} 章", total);
            self.write_info_list(chapters, book_title);
            return;
        }

        let api = self.api.clone();
        let od = self.out_dir.clone();
        let p = AudioParams { ..self.p.clone() };

        // 批量预取 playinfo（单请求逗号多 id）；失败退化为空表，逐章兜底
        util::progress(&format!("批量获取音频信息（{} 章）", pending.len()));
        let ids: Vec<String> = pending.iter().map(|c| c.item_id.clone()).collect();
        let pre_map = api.fetch_audio_batch(&p.book_id, &ids, p.tone).await.unwrap_or_default();
        util::clear_progress();
        if p.verbose {
            eprintln!("  [verbose] audio: 批量 playinfo 命中 {}/{} 章", pre_map.len(), ids.len());
        }
        let pre = std::sync::Arc::new(pre_map);

        let max_jobs = default_concurrent();
        let pending_n = pending.len();
        if self.p.verbose {
            eprintln!("  [verbose] audio: {}章, {}线程, force={}, lrc={}, abr={}",
                pending_n, max_jobs, self.p.force, self.p.lrc_mode, self.p.abr);
        }
        let (ok, fail, errs) = util::with_progress_async(pending, total, max_jobs, "green/cyan", move |ch, pb| {
            let api = api.clone();
            let od = od.clone();
            let p = p.clone();
            let pre = pre.clone();
            async move {
                let r = dl_one(&api, &od, &ch, &p, &pre).await;
                match &r {
                    Ok(_) => pb.set_message(format!("✓{:04}", ch.index)),
                    Err(e) => pb.set_message(format!("✗{:04}:{}", ch.index, e)),
                }
                r
            }
        }).await;

        println!("{GREEN}ok{RST} {ok} 章 · 跳过 {skip} 章");
        if fail > 0 { util::print_failures(&errs, " · 重跑同一命令可续传"); }

        self.write_info_list(chapters, book_title);
    }

    fn write_info_list(&self, chapters: &[&Chapter], book_title: Option<&str>) {
        let path = self.out_dir.join("info.list");
        let title = book_title.unwrap_or("未知");
        let mut json = format!("{{\n  \"book_title\": \"{}\",\n  \"tone\": {},\n  \"chapters\": [\n",
            title.replace('\\', "\\\\").replace('"', "\\\""), self.p.tone);
        for (i, ch) in chapters.iter().enumerate() {
            let file = self.file_name(ch);
            let comma = if i + 1 < chapters.len() { "," } else { "" };
            json.push_str(&format!(
                "    {{\"idx\":{}, \"title\":\"{}\", \"file\":\"{}\"}}{}\n",
                ch.index, ch.title.replace('\\', "\\\\").replace('"', "\\\""), file, comma));
        }
        json.push_str("  ]\n}\n");
        fs::write(&path, json).ok();
    }

    /// 章节默认文件名（无已落盘文件时的兜底扩展名）。
    fn fname(&self, ch: &Chapter) -> String {
        util::format_filename(&self.p.ft, ch) + ".mp3"
    }

    /// 章节音频实际文件名：优先已落盘的 `.m4a`/`.mp3`。
    fn file_name(&self, ch: &Chapter) -> String {
        let stem = util::format_filename(&self.p.ft, ch);
        match existing_audio(&self.out_dir, &stem) {
            Some(p) => p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.fname(ch)),
            None => self.fname(ch),
        }
    }
}

/// 依容器魔数判断音频实际扩展名：MP4（`ftyp`）→ `.m4a`，其余按 MP3。
///
/// 番茄音频解密后仍是 MP4 容器（Opus 编码）；`dl_one` 有 ffmpeg 时会再
/// 把它转码成 MP3 落盘，`.m4a` 仅在无 ffmpeg/转码失败时作为降级输出。
fn detect_audio_ext(path: &Path) -> &'static str {
    let mut buf = [0u8; 12];
    if let Ok(mut f) = fs::File::open(path) {
        if f.read_exact(&mut buf).is_ok() && &buf[4..8] == b"ftyp" {
            return ".m4a";
        }
    }
    ".mp3"
}

/// ffmpeg 是否可用（进程内只探测一次）。
fn ffmpeg_available() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// 降级提示（每次运行只提示一次，避免进度期间刷屏）。
fn warn_fallback(msg: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ONCE: AtomicBool = AtomicBool::new(false);
    if !ONCE.swap(true, Ordering::Relaxed) {
        util::note(msg);
    }
}

/// 把 Opus-in-MP4 转码为 MP3（libmp3lame 64k）。
///
/// 番茄官方音源是 Opus 编码的 MP4，Android ≤9 与 iOS 的系统播放器不支持
/// 该组合；转成 MP3 后全平台可播，并恢复 ID3 嵌入/封面/abr 特性链。
/// 由 `dl_one` 在解密落盘前调用。
fn transcode_to_mp3(src: &Path, dst: &Path) -> Result<(), String> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        // 清除输入（mp4 容器）残留元数据，避免 major_brand/compatible_brands 等垃圾标签写入 ID3
        .args(["-map_metadata", "-1", "-vn", "-c:a", "libmp3lame", "-b:a", "64k"])
        .arg(dst)
        .output()
        .map_err(|e| format!("ffmpeg 执行失败: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "ffmpeg 转码失败 (exit={}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let size = fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
    if size <= 1000 {
        return Err(format!("转码输出过小 ({}b)", size));
    }
    Ok(())
}

/// 查找已存在的音频文件（兼容旧 `.mp3` 与新 `.m4a`）。
fn existing_audio(out_dir: &Path, stem: &str) -> Option<PathBuf> {
    ["m4a", "mp3"].iter()
        .map(|ext| out_dir.join(format!("{}.{}", stem, ext)))
        .find(|p| p.exists())
}

// ── Download single chapter ─────────────────────────────────

async fn dl_one(api: &Client, out_dir: &Path, ch: &Chapter, p: &AudioParams,
                pre: &std::collections::HashMap<String, (String, String)>) -> Result<(), String> {
    let name = util::format_filename(&p.ft, ch);
    let work = out_dir.join(format!("{}.download", name));

    // 已存在音频但缺 LRC，补生成（兼容 .m4a / .mp3）
    if !p.force {
        if let Some(existing) = existing_audio(out_dir, &name) {
            let lrc_path = existing.with_extension("lrc");
            if p.lrc_mode != "off" && !lrc_path.exists() {
                let content = api.fetch_content(&ch.item_id).await.ok();
                handle_lrc(&existing, ch, &p.lrc_mode, p.verbose, content.as_deref());
            }
            return Ok(());
        }
    }

    // 哈希对比：已存在文件且 --force 时，先对比大小再决定是否跳过
    if p.force {
        if let Some(existing) = existing_audio(out_dir, &name) {
            let old_size = std::fs::metadata(&existing).map(|m| m.len()).unwrap_or(0);
            let mut all_tones_force = vec![p.tone];
            all_tones_force.extend(p.fallbacks.iter().filter(|&&t| t != p.tone));
            for &t in &all_tones_force {
                if let Ok((audio_url, _)) = api.fetch_audio(&p.book_id, &ch.item_id, t).await {
                    if let Ok(resp) = api.reqwest_client.get(&audio_url).send().await {
                        let new_size = resp.content_length().unwrap_or(0);
                        if new_size == old_size && new_size > 1000 {
                            if p.verbose { crate::util::note(&format!("  {:04} {} 不变 ✓", ch.index, ch.title)); }
                            let content = api.fetch_content(&ch.item_id).await.ok();
                            handle_lrc(&existing, ch, &p.lrc_mode, p.verbose, content.as_deref());
                            return Ok(());
                        }
                    }
                }
            }
            // 大小不同或无法获取 → 继续下载覆盖
        }
    }

    let mut all_tones = vec![p.tone];
    all_tones.extend(p.fallbacks.iter().filter(|&&t| t != p.tone));

    let mut last_err = String::new();
    for &t in &all_tones {
        // 主音色优先用批量预取结果；回退音色仍逐章获取
        let res = if t == p.tone {
            match pre.get(&ch.item_id) {
                Some(pair) => Ok(pair.clone()),
                None => api.fetch_audio(&p.book_id, &ch.item_id, t).await,
            }
        } else {
            api.fetch_audio(&p.book_id, &ch.item_id, t).await
        };
        let (audio_url, key_hex) = match res {
            Ok(v) => v,
            Err(e) => { last_err = e; continue; }
        };
        match download_file(&audio_url, &work, p.verbose) {
            Ok(_) => {
                if let Err(e) = decrypt_audio_file(&work, &key_hex) {
                    let _ = fs::remove_file(&work);
                    last_err = format!("tone={} 解密失败: {}", t, e);
                    continue;
                }
                // 番茄音源解密后是 MP4(Opus)，普通播放器不支持 → 有 ffmpeg 就转 MP3
                let mut ext = detect_audio_ext(&work);
                let mut src = work.clone();
                if ext == ".m4a" {
                    if ffmpeg_available() {
                        let mp3_tmp = out_dir.join(format!("{}.download.mp3", name));
                        match transcode_to_mp3(&work, &mp3_tmp) {
                            Ok(()) => {
                                let _ = fs::remove_file(&work);
                                src = mp3_tmp;
                                ext = ".mp3";
                            }
                            Err(e) => {
                                let _ = fs::remove_file(&mp3_tmp);
                                if p.verbose {
                                    warn_fallback(&format!("  warn 转码 MP3 失败，保留 .m4a: {}", e));
                                } else {
                                    warn_fallback("  warn 转码 MP3 失败，保留 .m4a（普通播放器可能无法播放）");
                                }
                            }
                        }
                    } else {
                        warn_fallback("  warn 未检测到 ffmpeg，番茄音频保留 .m4a（Android ≤9/iOS 等播放器可能无法播放）");
                    }
                }
                let final_path = out_dir.join(format!("{}{}", name, ext));
                // 清掉同章另一扩展名的旧文件，避免残留两份
                for other in ["mp3", "m4a"] {
                    let p_other = out_dir.join(format!("{}.{}", name, other));
                    if p_other != final_path {
                        let _ = fs::remove_file(&p_other);
                    }
                }
                if let Err(e) = fs::rename(&src, &final_path) {
                    let _ = fs::remove_file(&src);
                    last_err = format!("重命名音频失败: {}", e);
                    continue;
                }
                post_process(&final_path, p.abr, p.speed, p.normalize, &p.post_cmd, p.verbose);
                let content = api.fetch_content(&ch.item_id).await.ok();
                handle_lrc(&final_path, ch, &p.lrc_mode, p.verbose, content.as_deref());
                return Ok(());
            }
            Err(e) => {
                let _ = fs::remove_file(&work);
                last_err = format!("tone={}: {}", t, e);
            }
        }
    }
    Err(last_err)
}

/// CENC 加密音频就地解密（密钥来自 fqapi playinfo 的 `key_hex`）。
///
/// 明文流（`key_hex` 空）或非 mp4 容器直接返回 Ok。
fn decrypt_audio_file(path: &Path, key_hex: &str) -> Result<(), String> {
    if key_hex.is_empty() {
        return Ok(());
    }
    let mut data = fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;
    if !fqapi_core::endpoints::video::needs_cenc_decrypt(&data) {
        return Ok(());
    }
    let key = crate::fqapi::decode_hex(key_hex)?;
    fqapi_core::endpoints::video::decrypt_cenc_content(&mut data, &key)?;
    fs::write(path, &data).map_err(|e| format!("写回解密文件失败: {}", e))
}

fn download_file(url: &str, path: &PathBuf, verbose: bool) -> Result<(), String> {
    if verbose { crate::util::note(&format!("  [verbose] DL {}", &url[..url.len().min(80)])); }

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
            crate::util::note(&format!("  [verbose] curl fail: {}, err={}", out.status, stderr.trim()));
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

    Err(format!("下载失败: curl 和 grun 均失败 ({})", &url[..url.len().min(80)]))
}

// ── Post-processing ─────────────────────────────────────────

pub fn post_process(path: &Path, abr: u32, speed: Option<f32>, normalize: bool,
                    cmd_template: &str, verbose: bool) {
    if abr == 0 && speed.is_none() && !normalize && cmd_template.is_empty() { return; }
    if !path.exists() { eprintln!("  err {} 不存在", path.display()); return; }

    let orig_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let tmp = path.with_extension("tmp.mp3");
    let _ = fs::remove_file(&tmp); // 清理上次残留

    let ok = if !cmd_template.is_empty() {
        let input_s = path.to_string_lossy().to_string();
        let output_s = tmp.to_string_lossy().to_string();
        let cmd = cmd_template.replace("{input}", &input_s).replace("{output}", &output_s);
        if verbose { crate::util::note(&format!("  [verbose] post-process: {}", cmd)); }
        match std::process::Command::new("sh").arg("-c").arg(&cmd).output() {
            Ok(out) if out.status.success() => true,
            Ok(out) => { crate::util::note(&format!("  err 后处理失败 (exit={}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim())); false }
            Err(e) => { crate::util::note(&format!("  err 后处理命令执行失败: {}", e)); false }
        }
    } else if abr > 0 {
        // lame 只接受 MP3；Opus/m4a 等容器直接跳过，保留原文件
        let is_mp3 = path.extension()
            .map(|e| e.eq_ignore_ascii_case("mp3"))
            .unwrap_or(false);
        if !is_mp3 {
            if verbose { crate::util::note("  [verbose] 非 MP3 容器，跳过 lame 压缩"); }
            true
        } else {
            let p_s = path.to_string_lossy().to_string();
            let t_s = tmp.to_string_lossy().to_string();
            let abr_s = abr.to_string();
            let args = vec!["--abr", &abr_s, "--silent", &p_s, &t_s];
            match std::process::Command::new("lame").args(&args).output() {
                Ok(out) if out.status.success() => true,
                Ok(out) => { crate::util::note(&format!("  err lame 压缩失败 (exit={}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim())); false }
                Err(e) => { crate::util::note(&format!("  err lame 未安装或执行失败: {}", e)); false }
            }
        }
    } else {
        true
    };

    if ok && tmp.exists() {
        let new_size = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if new_size > 1000 {
            fs::rename(&tmp, path).unwrap_or_else(|e| crate::util::note(&format!("  err 重命名失败: {}", e)));
            if verbose { crate::util::note(&format!("  [verbose] 压缩完成: {}→{} ({}% of original)", path.display(), new_size, (new_size as f64 / orig_size.max(1) as f64 * 100.0) as u32)); }
        } else {
            crate::util::note(&format!("  err 压缩后文件过小 ({}b), 保留原文件", new_size));
            let _ = fs::remove_file(&tmp);
        }
    }
}

// ── LRC ─────────────────────────────────────────────────────

pub fn gen_lrc_text(ch: &Chapter) -> String {
    format!("[ti:{}]\n[ar:fqdt]\n[00:00.00]{}\n", util::chapter_heading(ch), util::chapter_heading(ch))
}

pub fn gen_lrc_text_full(ch: &Chapter, content: &str) -> String {
    let heading = util::chapter_heading(ch);
    let mut out = format!("[ti:{}]\n[ar:fqdt]\n[00:00.00]{}\n", heading, heading);
    let mut sec: u32 = 3;
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.len() < 5 { continue; }
        let m = sec / 60;
        let s = sec % 60;
        out.push_str(&format!("[{:02}:{:05.2}]", m, s as f64));
        out.push_str(t);
        out.push('\n');
        sec += (t.len() as f64 / 3.5).max(1.0) as u32;
    }
    out
}

pub fn write_lrc_file(path: &Path, ch: &Chapter, content: Option<&str>) -> Result<(), String> {
    let lrc_path = path.with_extension("lrc");
    let text = match content {
        Some(c) => gen_lrc_text_full(ch, c),
        None => gen_lrc_text(ch),
    };
    fs::write(&lrc_path, &text).map_err(|e| format!("写入 LRC: {}", e))?;
    if text.lines().count() < 2 {
        return Err("LRC 内容过短".into());
    }
    Ok(())
}

pub fn embed_lrc(mp3_path: &Path, ch: &Chapter, verbose: bool, content: Option<&str>) {
    use id3::frame::Lyrics;
    use id3::{Content, Frame, Tag, TagLike};

    let lrc_text = match content {
        Some(c) => gen_lrc_text_full(ch, c),
        None => gen_lrc_text(ch),
    };
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
        if verbose { crate::util::note(&format!("  [verbose] embed lrc fail: {}", e)); }
    }
}

fn handle_lrc(path: &Path, ch: &Chapter, mode: &str, verbose: bool, content: Option<&str>) {
    // MP4/m4a 不能用 ID3 嵌入歌词，embed/both 退化为外置 .lrc，避免写坏容器
    let is_mp3 = path.extension()
        .map(|e| e.eq_ignore_ascii_case("mp3"))
        .unwrap_or(false);
    let effective = if is_mp3 {
        mode
    } else {
        match mode {
            "embed" | "both" => "external",
            m => m,
        }
    };
    match effective {
        "external" => { if let Err(e) = write_lrc_file(path, ch, content) { crate::util::note(&format!("  err LRC: {}", e)); } }
        "embed" => embed_lrc(path, ch, verbose, content),
        "both" => {
            if let Err(e) = write_lrc_file(path, ch, content) { crate::util::note(&format!("  err LRC: {}", e)); }
            embed_lrc(path, ch, verbose, content);
        }
        _ => {}
    }
}

// ── TTS conversion ──────────────────────────────────────────

pub fn convert_tts_file(input: &Path, output_dir: Option<PathBuf>, p: &TtsParams) {
    let name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let out = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
    fs::create_dir_all(&out).expect("创建目录失败");
    let out_path = out.join(format!("{}.mp3", name));
    if out_path.exists() {
        println!("{GREEN}ok{RST} 已存在: {}", out_path.display());
        return;
    }
    let text = match fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => { eprintln!("  err 读文件: {}", e); return; }
    };
    println!("  {} → {}", name, out_path.display());
    if let Err(e) = run_edge_tts(&text, &p.voice, &p.rate, &p.volume, &p.pitch, &out_path, p.verbose) {
        eprintln!("  err {}", e);
        return;
    }
    post_process(&out_path, p.abr, p.speed, p.normalize, &p.cmd, p.verbose);
    handle_lrc(&out_path, &Chapter { index: 0, title: name.into(), item_id: String::new() }, &p.lrc_mode, p.verbose, Some(&text));
}

pub fn convert_tts_dir(input: &Path, output_dir: Option<PathBuf>, params: &TtsParams) {
    let out = output_dir.unwrap_or_else(|| {
        let mut dir = input.to_path_buf();
        dir.push("Audio");
        dir
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
    pb.set_style(util::bar_style("magenta/cyan", 1));
    pb.enable_steady_tick(std::time::Duration::from_millis(120));

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
        if let Err(e) = run_edge_tts(&text, &params.voice, &params.rate, &params.volume, &params.pitch, &out_path, params.verbose) {
            pb.set_message(format!("✗{}: {}", name, e));
            failed += 1;
        } else {
            post_process(&out_path, params.abr, params.speed, params.normalize, &params.cmd, params.verbose);
            handle_lrc(&out_path, &Chapter { index: 0, title: name.clone(), item_id: String::new() }, &params.lrc_mode, params.verbose, Some(&text));
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
        .arg("--write-media").arg(out_path.to_string_lossy().to_string())
        .output();
    if let Ok(out) = r {
        if out.status.success() {
            let size = fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
            if size > 1000 { return Ok(()); }
        }
        if verbose {
            crate::util::note(&format!("  [verbose] edge-tts: status={}, err={}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
        }
        return Err(format!("edge-tts 退出码 {}", out.status));
    }
    Err("找不到 edge-tts 命令".into())
}
