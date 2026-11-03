use crate::api::Client;
use crate::audio;
use crate::download;
use crate::types::{ChapterRange, Config, DownloadParams};
use crate::util;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

pub async fn run(book_id: &str, p: &DownloadParams, cfg: &Config) {
    let start = Instant::now();
    let vb = p.verbose || cfg.verbose;
    let path = Path::new(book_id);

    if path.is_dir() {
        return run_dir(path, p, vb, cfg).await;
    }

    let api = Client::from_config(cfg, vb);
    print!("  获取目录... ");
    std::io::stdout().flush().unwrap();
    let all = match api.fetch_catalog(book_id).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\n  err {}", e);
            return;
        }
    };
    let r = p.range.as_deref().and_then(ChapterRange::parse);
    let chs = util::filter_by_range(&all, r.as_ref());
    if chs.is_empty() {
        println!("  err 空范围");
        return;
    }

    let fmt = p.format.as_deref().unwrap_or(&cfg.format);
    let out = p.output
        .as_ref()
        .map(|s| s.into())
        .unwrap_or(cfg.output_dir.clone());
    let bt: String = match p.book_title {
        Some(ref title) => title.clone(),
        None => match api.fetch_detail(book_id).await {
            Ok(detail) => detail.split('|').next().unwrap_or("小说").to_string(),
            Err(_) => "小说".into(),
        },
    };
    let dler = download::Downloader::new(api, out.clone(), fmt, &cfg.filename_template, false, vb,
                                          book_id, &bt);
    dler.run(&chs, p.concurrent.unwrap_or(cfg.concurrent)).await;

    if p.audio {
        let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
            vec![2, 4, 5, 6, 74, 91]
        } else {
            cfg.audio_tone_fallbacks.clone()
        };
        fetch_and_dl_audio(book_id, &out.join("Audio"), &p.range, p.tone, p.abr, &p.lrc, p.force, &fallbacks, vb, cfg).await;
    }
    let secs = start.elapsed().as_secs();
    if secs > 0 { println!("  \x1b[2m已用时 {}s\x1b[0m", secs); }
}

pub async fn run_dir(path: &Path, p: &DownloadParams, vb: bool, cfg: &Config) {
    let has_text = path.join("info.list").exists();
    let has_audio = path.join("Audio/info.list").exists();
    if !has_text && !has_audio {
        eprintln!("  err 目录中未找到 info.list");
        return;
    }

    if has_text {
        if let Ok((bid, btitle, fmt, existing)) = download::read_info_list(path) {
            let api = Client::from_config(cfg, vb);
            print!("  检查正文更新... ");
            std::io::stdout().flush().unwrap();
            if let Ok(all) = api.fetch_catalog(&bid).await {
                let r = p.range.as_deref().and_then(ChapterRange::parse);
                let new_chs: Vec<&crate::types::Chapter> = all
                    .iter()
                    .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
                    .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
                    .collect();
                if new_chs.is_empty() {
                    println!("  正文已是最新 (共{}章)", all.len());
                } else {
                    println!("  发现 {} 章新正文 (共{}/{})", new_chs.len(), existing.len(), all.len());
                    let dler = download::Downloader::new(api, path.to_path_buf(), &fmt,
                                                          &cfg.filename_template, p.force, vb, &bid, &btitle);
                    dler.run(&new_chs, p.concurrent.unwrap_or(cfg.concurrent)).await;
                }
            }
        }
    } else {
        println!("  正文已存在, 跳过");
    }

    if p.audio || has_audio {
        let audio_dir = path.join("Audio");
        let (bid, btitle, existing) = if has_audio {
            download::read_audio_info_list(&audio_dir).unwrap_or_default()
        } else {
            (String::new(), String::new(), vec![])
        };

        if bid.is_empty() {
            if let Ok((bid2, _, _, _)) = download::read_info_list(path) {
                let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
                    vec![2, 4, 5, 6, 74, 91]
                } else {
                    cfg.audio_tone_fallbacks.clone()
                };
                fetch_and_dl_audio(&bid2, &audio_dir, &p.range, p.tone, p.abr, &p.lrc, p.force, &fallbacks, vb, cfg).await;
            }
        } else {
            let api = Client::from_config(cfg, vb);
            print!("  检查音频更新... ");
            std::io::stdout().flush().unwrap();
            if let Ok(all) = api.fetch_catalog(&bid).await {
                let r = p.range.as_deref().and_then(ChapterRange::parse);
                let new_chs: Vec<&crate::types::Chapter> = all
                    .iter()
                    .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
                    .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
                    .collect();
                if new_chs.is_empty() {
                    println!("  音频已是最新 (共{}章)", all.len());
                } else {
                    println!("  发现 {} 章新音频 (共{}/{})", new_chs.len(), existing.len(), all.len());
                    let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
                        vec![2, 4, 5, 6, 74, 91]
                    } else {
                        cfg.audio_tone_fallbacks.clone()
                    };
                    let dler = audio::AudioDownloader::new(api, audio_dir, audio::AudioParams {
                        book_id: bid.clone(),
                        tone: p.tone, fallbacks,
                        ft: cfg.filename_template.clone(), force: p.force, verbose: vb,
                        abr: p.abr, speed: None, normalize: false,
                        post_cmd: String::new(), lrc_mode: p.lrc.clone(),
                    });
                    dler.run(&new_chs, Some(&btitle)).await;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_and_dl_audio(book_id: &str, audio_dir: &Path, range: &Option<String>, tone: usize,
                      abr: u32, lrc: &str, force: bool, fallbacks: &[usize],
                      verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);
    print!("  获取目录... ");
    std::io::stdout().flush().unwrap();
    match api.fetch_catalog(book_id).await {
        Ok(all) => {
            if all.is_empty() {
                println!("  err 空目录");
                return;
            }
            let r = range.as_deref().and_then(ChapterRange::parse);
            let chs = util::filter_by_range(&all, r.as_ref());
            if chs.is_empty() {
                println!("  err 空范围");
                return;
            }
            let fb = if fallbacks.is_empty() {
                vec![2, 4, 5, 6, 74, 91]
            } else {
                fallbacks.to_vec()
            };
            let dler = audio::AudioDownloader::new(api, audio_dir.to_path_buf(), audio::AudioParams {
                book_id: book_id.to_string(),
                tone, fallbacks: fb,
                ft: cfg.filename_template.clone(), force, verbose: vb,
                abr, speed: None, normalize: false,
                post_cmd: String::new(), lrc_mode: lrc.into(),
            });
            dler.run(&chs, None).await;
        }
        Err(e) => eprintln!("  err {}", e),
    }
}
