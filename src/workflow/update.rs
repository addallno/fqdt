use crate::api::Client;
use crate::audio;
use crate::download;
use crate::types::{ChapterRange, Config};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub async fn run(book_id: Option<&str>, output: Option<&str>, concurrent: Option<usize>,
           range: Option<&str>, force: bool, audio: bool, verbose: bool,
           _interval: u64, cfg: &Config) {
    let path = match book_id {
        Some(s) => PathBuf::from(s),
        None => {
            eprintln!("  err 需要 book_id 或目录");
            return;
        }
    };
    let vb = verbose || cfg.verbose;

    if path.is_dir() {
        return update_dir(&path, range, force, audio, vb, cfg, concurrent).await;
    }

    let bid = book_id.unwrap();
    let api = Client::from_config(cfg, vb);
    print!("  获取目录... ");
    std::io::stdout().flush().unwrap();
    let all = match api.fetch_catalog(bid).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\n  err {}", e);
            return;
        }
    };
    if all.is_empty() {
        println!("  err 空目录");
        return;
    }

    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or(cfg.output_dir.clone());
    let r = range.and_then(ChapterRange::parse);

    if audio {
        update_audio(bid, &all, &out_dir, r.as_ref(), force, vb, cfg).await;
        return;
    }

    let mut max_existing = 0usize;
    if out_dir.exists() {
        if let Ok(entries) = fs::read_dir(&out_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".txt")
                    && name.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    if let Ok(n) = name.trim_end_matches(".txt").parse::<usize>() {
                        if n > max_existing {
                            max_existing = n;
                        }
                    }
                }
            }
        }
    }
    let new_chs: Vec<&crate::types::Chapter> = all
        .iter()
        .filter(|c| c.index > max_existing)
        .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
        .collect();
    if new_chs.is_empty() {
        println!("  已是最新 (共{}章)", all.len());
        return;
    }
    println!("  发现 {} 章新章节 (共{}→{})", new_chs.len(), max_existing, all.len());
    let dler = download::Downloader::new(
        api, out_dir, &cfg.format, &cfg.filename_template, force, vb, bid, "小说",
    );
    dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent)).await;
}

async fn update_dir(path: &std::path::Path, range: Option<&str>, force: bool, audio: bool,
              vb: bool, cfg: &Config, concurrent: Option<usize>) {
    if audio {
        let audio_dir = path.join("Audio");
        if !audio_dir.exists() {
            eprintln!("  err Audio/ 目录不存在");
            return;
        }
        let (bid, btitle, existing) = match download::read_audio_info_list(&audio_dir) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  err {}", e);
                return;
            }
        };
        let api = Client::from_config(cfg, vb);
        print!("  获取目录... ");
        std::io::stdout().flush().unwrap();
        let all = match api.fetch_catalog(&bid).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("\n  err {}", e);
                return;
            }
        };

        let r = range.and_then(ChapterRange::parse);
        let new_chs: Vec<&crate::types::Chapter> = all.iter()
            .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
            .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
            .collect();
        if new_chs.is_empty() {
            println!("  音频已是最新 (共{}章)", all.len());
            return;
        }
        println!("  发现 {} 章新音频 (共{}/{})", new_chs.len(), existing.len(), all.len());
        let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
            vec![2, 4, 5, 6, 74, 91]
        } else {
            cfg.audio_tone_fallbacks.clone()
        };
        let dler = audio::AudioDownloader::new(api, audio_dir, audio::AudioParams {
            book_id: bid.clone(),
            tone: cfg.audio_tone, fallbacks,
            ft: cfg.filename_template.clone(), force, verbose: vb,
            abr: cfg.abr, speed: None, normalize: false,
            post_cmd: cfg.post_process.clone(), lrc_mode: "external".into(),
        });
        dler.run(&new_chs, Some(&btitle)).await;
        return;
    }

    let (bid, btitle, fmt, existing) = match download::read_info_list(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  err {}", e);
            return;
        }
    };
    let api = Client::from_config(cfg, vb);
    print!("  获取目录... ");
    std::io::stdout().flush().unwrap();
    let all = match api.fetch_catalog(&bid).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\n  err {}", e);
            return;
        }
    };
    if all.is_empty() {
        println!("  err 空目录");
        return;
    }
    let r = range.and_then(ChapterRange::parse);
    let new_chs: Vec<&crate::types::Chapter> = all
        .iter()
        .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
        .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
        .collect();
    if new_chs.is_empty() {
        println!("  已是最新 (共{}章)", all.len());
        return;
    }
    println!("  发现 {} 章新章节 (共{}/{})", new_chs.len(), existing.len(), all.len());
    let dler = download::Downloader::new(
        api, path.to_path_buf(), &fmt, &cfg.filename_template, force, vb, &bid, &btitle,
    );
    dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent)).await;
}

async fn update_audio(book_id: &str, all: &[crate::types::Chapter], out_dir: &std::path::Path,
                range: Option<&ChapterRange>, force: bool, vb: bool, cfg: &Config) {
    let audio_dir = out_dir.join("Audio");
    let mut max_existing = 0usize;
    if audio_dir.exists() {
        if let Ok(entries) = fs::read_dir(&audio_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".mp3")
                    && name.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    if let Ok(n) = name.trim_end_matches(".mp3").parse::<usize>() {
                        if n > max_existing {
                            max_existing = n;
                        }
                    }
                }
            }
        }
    }
    let new_chs: Vec<&crate::types::Chapter> = all
        .iter()
        .filter(|c| c.index > max_existing)
        .filter(|c| range.is_none_or(|x| x.contains(c.index)))
        .collect();
    if new_chs.is_empty() {
        println!("  音频已是最新 (共{}章)", all.len());
        return;
    }
    println!("  发现 {} 章新音频 (共{}→{})", new_chs.len(), max_existing, all.len());
    let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
        vec![2, 4, 5, 6, 74, 91]
    } else {
        cfg.audio_tone_fallbacks.clone()
    };
    let dler = audio::AudioDownloader::new(
        Client::from_config(cfg, vb), audio_dir,
        audio::AudioParams {
            book_id: book_id.to_string(),
            tone: cfg.audio_tone, fallbacks,
            ft: cfg.filename_template.clone(), force, verbose: vb,
            abr: cfg.abr, speed: None, normalize: false,
            post_cmd: cfg.post_process.clone(), lrc_mode: "external".into(),
        },
    );
    dler.run(&new_chs, None).await;
}
