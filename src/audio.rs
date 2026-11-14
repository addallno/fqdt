use crate::api::Client;
use crate::types::{default_concurrent, Chapter};
use crate::util;
use indicatif::ProgressBar;
use std::fs;
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
                let mp3 = self.out_dir.join(self.fname(c));
                if !mp3.exists() { return true; }
                if need_lrc && !mp3.with_extension("lrc").exists() { return true; }
                false
            })
            .map(|c| (*c).clone()).collect();
        let skip = total - pending.len();

        if pending.is_empty() {
            println!("  ok 全部已存在 · {} 章", total);
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

        println!("  ok {ok} 章 · 跳过 {skip} 章");
        if fail > 0 { util::print_failures(&errs, " · 重跑同一命令可续传"); }

        self.write_info_list(chapters, book_title);
    }

    fn write_info_list(&self, chapters: &[&Chapter], book_title: Option<&str>) {
        let path = self.out_dir.join("info.list");
        let title = book_title.unwrap_or("未知");
        let mut json = format!("{{\n  \"book_title\": \"{}\",\n  \"tone\": {},\n  \"chapters\": [\n",
            title.replace('\\', "\\\\").replace('"', "\\\""), self.p.tone);
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
        util::format_filename(&self.p.ft, ch) + ".mp3"
    }
}

// ── Download single chapter ─────────────────────────────────

async fn dl_one(api: &Client, out_dir: &Path, ch: &Chapter, p: &AudioParams,
                pre: &std::collections::HashMap<String, (String, String)>) -> Result<(), String> {
    let name = util::format_filename(&p.ft, ch);
    let path = out_dir.join(format!("{}.mp3", name));

    // 如果 MP3 已存在但缺少 LRC，补生成
    if !p.force && path.exists() {
        let lrc_path = path.with_extension("lrc");
        if p.lrc_mode != "off" && !lrc_path.exists() {
            let content = api.fetch_content(&ch.item_id).await.ok();
            handle_lrc(&path, ch, &p.lrc_mode, p.verbose, content.as_deref());
        }
        return Ok(());
    }

    // 哈希对比：已存在文件且 --force 时，先对比大小再决定是否跳过
    if p.force && path.exists() {
        let old_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut all_tones_force = vec![p.tone];
        all_tones_force.extend(p.fallbacks.iter().filter(|&&t| t != p.tone));
        for &t in &all_tones_force {
            if let Ok((audio_url, _)) = api.fetch_audio(&p.book_id, &ch.item_id, t).await {
                if let Ok(resp) = api.reqwest_client.get(&audio_url).send().await {
                    let new_size = resp.content_length().unwrap_or(0);
                    if new_size == old_size && new_size > 1000 {
                        if p.verbose { eprintln!("  {:04} {} 不变 ✓", ch.index, ch.title); }
                        let content = api.fetch_content(&ch.item_id).await.ok();
                        handle_lrc(&path, ch, &p.lrc_mode, p.verbose, content.as_deref());
                        return Ok(());
                    }
                }
            }
        }
        // 大小不同或无法获取 → 继续下载覆盖
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
        match download_file(&audio_url, &path, p.verbose) {
            Ok(_) => {
                if let Err(e) = decrypt_audio_file(&path, &key_hex) {
                    last_err = format!("tone={} 解密失败: {}", t, e);
                    continue;
                }
                post_process(&path, p.abr, p.speed, p.normalize, &p.post_cmd, p.verbose);
                let content = api.fetch_content(&ch.item_id).await.ok();
                handle_lrc(&path, ch, &p.lrc_mode, p.verbose, content.as_deref());
                return Ok(());
            }
            Err(e) => { last_err = format!("tone={}: {}", t, e); }
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
        if verbose { eprintln!("  [verbose] post-process: {}", cmd); }
        match std::process::Command::new("sh").arg("-c").arg(&cmd).output() {
            Ok(out) if out.status.success() => true,
            Ok(out) => { eprintln!("  err 后处理失败 (exit={}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()); false }
            Err(e) => { eprintln!("  err 后处理命令执行失败: {}", e); false }
        }
    } else if abr > 0 {
        let p_s = path.to_string_lossy().to_string();
        let t_s = tmp.to_string_lossy().to_string();
        let abr_s = abr.to_string();
        let args = vec!["--abr", &abr_s, "--silent", &p_s, &t_s];
        match std::process::Command::new("lame").args(&args).output() {
            Ok(out) if out.status.success() => true,
            Ok(out) => { eprintln!("  err lame 压缩失败 (exit={}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()); false }
            Err(e) => { eprintln!("  err lame 未安装或执行失败: {}", e); false }
        }
    } else {
        true
    };

    if ok && tmp.exists() {
        let new_size = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if new_size > 1000 {
            fs::rename(&tmp, path).unwrap_or_else(|e| eprintln!("  err 重命名失败: {}", e));
            if verbose { eprintln!("  [verbose] 压缩完成: {}→{} ({}% of original)", path.display(), new_size, (new_size as f64 / orig_size.max(1) as f64 * 100.0) as u32); }
        } else {
            eprintln!("  err 压缩后文件过小 ({}b), 保留原文件", new_size);
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
        if verbose { eprintln!("  [verbose] embed lrc fail: {}", e); }
    }
}

pub fn embed_cover(mp3_path: &Path, cover_path: &Path, verbose: bool) {
    use id3::frame::{Picture, PictureType};
    use id3::{Content, Frame, Tag, TagLike};

    let mime = match cover_path.extension().and_then(|s| s.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => {
            eprintln!("  warn 不支持的封面格式 (支持 jpg/png): {}", cover_path.display());
            return;
        }
    };
    let img_data = match std::fs::read(cover_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  err 读封面: {}", e);
            return;
        }
    };
    let mut tag = match Tag::read_from_path(mp3_path) {
        Ok(t) => t,
        Err(_) => Tag::new(),
    };
    tag.add_frame(Frame::with_content(
        "APIC",
        Content::Picture(Picture {
            mime_type: mime.into(),
            picture_type: PictureType::CoverFront,
            description: "cover".into(),
            data: img_data,
        }),
    ));
    if let Err(e) = tag.write_to_path(mp3_path, id3::Version::Id3v24) {
        eprintln!("  err 写入封面: {}", e);
    } else if verbose {
        eprintln!("  [verbose] 封面已嵌入: {}", mp3_path.display());
    }
}

fn handle_lrc(path: &Path, ch: &Chapter, mode: &str, verbose: bool, content: Option<&str>) {
    match mode {
        "external" => { if let Err(e) = write_lrc_file(path, ch, content) { eprintln!("  err LRC: {}", e); } }
        "embed" => embed_lrc(path, ch, verbose, content),
        "both" => {
            if let Err(e) = write_lrc_file(path, ch, content) { eprintln!("  err LRC: {}", e); }
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
        println!("  ok 已存在: {}", out_path.display());
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
            eprintln!("  [verbose] edge-tts: status={}, err={}", out.status, String::from_utf8_lossy(&out.stderr).trim());
        }
        return Err(format!("edge-tts 退出码 {}", out.status));
    }
    Err("找不到 edge-tts 命令".into())
}
