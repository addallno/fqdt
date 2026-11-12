use crate::api::Client;
use crate::audio;
use crate::types::{ChapterRange, Config};
use crate::util;
use std::path::PathBuf;

/// TTS 转换入口（从 get 命令调用）
#[allow(clippy::too_many_arguments)]
pub async fn run_tts(path: &str, output: Option<&str>, voice: &str, rate: &str, volume: &str, pitch: &str,
               abr: u32, speed: Option<f32>, normalize: bool, cmd: &str, lrc_mode: &str, vb: bool) {
    let p = std::path::Path::new(path);
    let params = audio::TtsParams {
        voice: voice.into(), rate: rate.into(), volume: volume.into(), pitch: pitch.into(),
        abr, speed, normalize, cmd: cmd.into(), lrc_mode: lrc_mode.into(), verbose: vb,
    };
    if p.is_dir() { audio::convert_tts_dir(p, output.map(PathBuf::from), &params); }
    else if p.is_file() { audio::convert_tts_file(p, output.map(PathBuf::from), &params); }
    else { eprintln!("  err 文件不存在: {}", path); }
}

#[allow(clippy::too_many_arguments, dead_code)]
pub async fn run(book_id: Option<&str>, output: Option<&str>, range: Option<&str>, tone: usize,
           verbose: bool, tts_path: Option<&str>, voice: &str,
           rate: Option<String>, volume: Option<String>, pitch: Option<String>,
           abr: Option<u32>, speed: Option<f32>, normalize: bool,
           audio_cmd: Option<String>, lrc_mode: &str, force: bool, _jobs: Option<usize>,
           _interval: u64, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let tts_rate = rate.as_deref().unwrap_or(&cfg.tts_rate);
    let tts_volume = volume.as_deref().unwrap_or(&cfg.tts_volume);
    let tts_pitch = pitch.as_deref().unwrap_or(&cfg.tts_pitch);
    let abr_val = abr.unwrap_or(cfg.abr);
    let post_cmd = audio_cmd.as_deref().unwrap_or(&cfg.post_process);

    if let Some(path) = tts_path {
        let p = std::path::Path::new(path);
        let tts_params = audio::TtsParams {
            voice: voice.into(),
            rate: tts_rate.into(),
            volume: tts_volume.into(),
            pitch: tts_pitch.into(),
            abr: abr_val,
            speed,
            normalize,
            cmd: post_cmd.into(),
            lrc_mode: lrc_mode.into(),
            verbose: vb,
        };
        if p.is_dir() {
            audio::convert_tts_dir(p, output.map(PathBuf::from), &tts_params);
        } else if p.is_file() {
            audio::convert_tts_file(p, output.map(PathBuf::from), &tts_params);
        } else {
            eprintln!("  err 文件不存在: {}", path);
        }
        return;
    }

    let bid = match book_id {
        Some(id) => id,
        None => {
            eprintln!("  err 需要 book_id, 目录或 --tts");
            return;
        }
    };

    if std::path::Path::new(bid).is_dir() {
        return run_dir(std::path::Path::new(bid), range, tone, abr_val, speed, normalize, post_cmd, lrc_mode, force, vb, cfg).await;
    }

    let api = Client::from_config(cfg, vb);
    util::progress("获取目录");
    let all = match api.fetch_catalog(bid).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
    };
    util::clear_progress();
    let r = range.and_then(ChapterRange::parse);
    let chs = util::filter_by_range(&all, r.as_ref());
    if chs.is_empty() {
        eprintln!("  err 空范围");
        return;
    }

    let out = output
        .map(|s| PathBuf::from(s).join("Audio"))
        .unwrap_or_else(|| {
            let mut p = cfg.output_dir.clone();
            p.push("Audio");
            p
        });
    run_download(&chs, &api, &out, bid, tone, &cfg.audio_tone_fallbacks, abr_val, speed, normalize, post_cmd, lrc_mode, force, &cfg.filename_template, vb).await;
}

#[allow(clippy::too_many_arguments, dead_code)]
async fn run_dir(path: &std::path::Path, range: Option<&str>, tone: usize, abr: u32, speed: Option<f32>,
           normalize: bool, post_cmd: &str, lrc_mode: &str, force: bool, vb: bool, cfg: &Config) {
    use crate::download;

    let audio_dir;
    let (bid, _btitle, existing) = if path.join("Audio/info.list").exists() {
        audio_dir = path.join("Audio");
        match download::read_audio_info_list(&audio_dir) {
            Ok(v) => v,
            Err(e) => { eprintln!("  err {}", e); return; }
        }
    } else if path.join("info.list").exists() {
        audio_dir = path.join("Audio");
        match download::read_info_list(path) {
            Ok((bid, btitle, _, _)) => (bid, btitle, vec![]),
            Err(e) => { eprintln!("  err {}", e); return; }
        }
    } else {
        eprintln!("  err 目录中未找到 info.list");
        return;
    };

    let api = Client::from_config(cfg, vb);
    util::progress("获取目录");
    let all = match api.fetch_catalog(&bid).await {
        Ok(c) => c,
        Err(e) => { util::clear_progress(); eprintln!("  err {}", e); return; }
    };
    util::clear_progress();

    let r = range.and_then(ChapterRange::parse);
    let new_chs: Vec<&crate::types::Chapter> = all.iter()
        .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
        .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
        .collect();

    if new_chs.is_empty() {
        println!("  音频已是最新 · 共 {} 章", all.len());
        return;
    }
    println!("  新增 {} 章音频 · 已有 {}/{} 章", new_chs.len(), existing.len(), all.len());

    let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
        vec![2, 4, 5, 6, 74, 91]
    } else {
        cfg.audio_tone_fallbacks.clone()
    };
    run_download(&new_chs, &api, &audio_dir, &bid, tone, &fallbacks, abr, speed, normalize, post_cmd, lrc_mode, force, &cfg.filename_template, vb).await;
}

#[allow(clippy::too_many_arguments, dead_code)]
async fn run_download(chs: &[&crate::types::Chapter], api: &Client, out: &std::path::Path,
                book_id: &str, tone: usize, fallbacks: &[usize], abr: u32, speed: Option<f32>,
                normalize: bool, post_cmd: &str, lrc_mode: &str, force: bool, ft: &str, vb: bool) {
    let fb = if fallbacks.is_empty() {
        vec![2, 4, 5, 6, 74, 91]
    } else {
        fallbacks.to_vec()
    };
    let dler = audio::AudioDownloader::new(
        api.clone(), out.to_path_buf(), audio::AudioParams {
            book_id: book_id.to_string(),
            tone, fallbacks: fb,
            ft: ft.to_string(), force, verbose: vb,
            abr, speed, normalize,
            post_cmd: post_cmd.to_string(), lrc_mode: lrc_mode.to_string(),
        },
    );
    dler.run(chs, None).await;
}
