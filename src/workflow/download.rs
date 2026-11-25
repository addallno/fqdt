use crate::api::Client;
use crate::audio;
use crate::download;
use crate::types::{ChapterRange, Config, DownloadParams};
use crate::util;
use std::path::Path;

pub async fn run(book_id: &str, p: &DownloadParams, cfg: &Config) {
    let vb = p.verbose || cfg.verbose;
    let path = Path::new(book_id);

    if path.is_dir() {
        return run_dir(path, p, vb, cfg).await;
    }

    let api = Client::from_config(cfg, vb);
    util::progress("获取目录");
    let all = match api.fetch_catalog(book_id).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
    };
    util::clear_progress();
    let r = p.range.as_deref().and_then(ChapterRange::parse);
    let chs = util::filter_by_range(&all, r.as_ref());
    if chs.is_empty() {
        eprintln!("  err 空范围");
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
        // -o 显式指定 → 音频直接落该目录；默认（存储书籍目录）→ 仍建 Audio 子文件夹
        let audio_out = if p.output.is_some() { out.clone() } else { out.join("Audio") };
        fetch_and_dl_audio(book_id, &audio_out, &p.range, p.tone, p.abr, &p.lrc, p.force, &fallbacks, vb, cfg).await;
    }
}

/// 目录增量：正文+音频各按 `-T/--type` 类别过滤（p.wants），默认全部。
pub async fn run_dir(path: &Path, p: &DownloadParams, vb: bool, cfg: &Config) {
    let has_text = path.join("info.list").exists();
    let has_audio = path.join("Audio/info.list").exists();
    if !has_text && !has_audio {
        eprintln!("  err 目录中未找到 info.list");
        return;
    }

    if has_text && p.wants("content") {
        if let Ok((bid, btitle, fmt, existing)) = download::read_info_list(path) {
            let api = Client::from_config(cfg, vb);
            util::progress("检查正文更新");
            let fetched = api.fetch_catalog(&bid).await;
            util::clear_progress();
            if let Ok(all) = fetched {
                let r = p.range.as_deref().and_then(ChapterRange::parse);
                let new_chs: Vec<&crate::types::Chapter> = all
                    .iter()
                    .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
                    .filter(|c| r.as_ref().is_none_or(|x| x.contains(c.index)))
                    .collect();
                if new_chs.is_empty() {
                    println!("  正文已是最新 · 共 {} 章", all.len());
                } else {
                    println!("  新增 {} 章正文 · 已有 {}/{} 章", new_chs.len(), existing.len(), all.len());
                    let dler = download::Downloader::new(api, path.to_path_buf(), &fmt,
                                                          &cfg.filename_template, p.force, vb, &bid, &btitle);
                    dler.run(&new_chs, p.concurrent.unwrap_or(cfg.concurrent)).await;
                }
            }
        }
    } else {
        println!("  正文已存在, 跳过");
    }

    if (p.audio || has_audio) && p.wants("audio") {
        let audio_dir = path.join("Audio");
        if has_audio {
            // 已有音频 info.list → 宽松增量（缺 book_id 可按书名解析，update 下沉复用）
            audio_increment(&audio_dir, p.range.as_deref(), p.force, vb, cfg,
                            Some(p.tone), p.abr, "", &p.lrc).await;
        } else if let Ok((bid2, _, _, _)) = download::read_info_list(path) {
            // 首次补音频：从正文 info.list 取 book_id 全量下（Downloader 跳已有文件）
            let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
                vec![2, 4, 5, 6, 74, 91]
            } else {
                cfg.audio_tone_fallbacks.clone()
            };
            fetch_and_dl_audio(&bid2, &audio_dir, &p.range, p.tone, p.abr, &p.lrc, p.force, &fallbacks, vb, cfg).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn fetch_and_dl_audio(book_id: &str, audio_dir: &Path, range: &Option<String>, tone: usize,
                      abr: u32, lrc: &str, force: bool, fallbacks: &[usize],
                      verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);
    util::progress("获取目录");
    let fetched = api.fetch_catalog(book_id).await;
    util::clear_progress();
    match fetched {
        Ok(all) => {
            if all.is_empty() {
                eprintln!("  err 空目录");
                return;
            }
            let r = range.as_deref().and_then(ChapterRange::parse);
            let chs = util::filter_by_range(&all, r.as_ref());
            if chs.is_empty() {
                eprintln!("  err 空范围");
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
                keep_m4a: cfg.audio_keep_m4a, mp3: cfg.audio_mp3, cover: cfg.audio_cover, cover_img: None,
            });
            dler.run(&chs, None).await;
        }
        Err(e) => eprintln!("  err {}", e),
    }
}

/// 音频目录增量（update 下沉至此，download/audio 共用）：info.list 宽松读取（存量目录可无 book_id → 按书名搜索解析），
/// 按原 tone 补下缺失章节。dir 为 info.list 所在目录（Audio/ 或纯音频根目录）。
#[allow(clippy::too_many_arguments)]
pub async fn audio_increment(dir: &std::path::Path, range: Option<&str>, force: bool,
                          vb: bool, cfg: &Config, tone_override: Option<usize>,
                          abr: u32, post_cmd: &str, lrc_mode: &str) {
    let (mut bid, btitle, tone, existing) = match download::read_audio_list_loose(dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  err {}", e);
            return;
        }
    };
    if bid.is_empty() {
        let Some(resolved) = resolve_bid_by_title(&btitle, cfg, vb).await else { return };
        bid = resolved;
    }
    let api = Client::from_config(cfg, vb);
    util::progress("获取目录");
    let all = match api.fetch_catalog(&bid).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
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
    let tone = tone_override.unwrap_or(tone);
    let dler = audio::AudioDownloader::new(api, dir.to_path_buf(), audio::AudioParams {
        book_id: bid, tone, fallbacks,
        ft: cfg.filename_template.clone(), force, verbose: vb,
        abr, speed: None, normalize: false,
        post_cmd: post_cmd.into(), lrc_mode: lrc_mode.into(),
        keep_m4a: cfg.audio_keep_m4a, mp3: cfg.audio_mp3, cover: cfg.audio_cover, cover_img: None,
    });
    dler.run(&new_chs, Some(&btitle)).await;
}

/// 存量目录 info.list 无 book_id：按书名搜索取首结果解析。
/// 必须标题匹配（互含）才接受——实测模糊词首结果会误配完全不同的书（481 章下错教训）。
async fn resolve_bid_by_title(title: &str, cfg: &Config, vb: bool) -> Option<String> {
    if title.is_empty() || title == "未知" {
        eprintln!("  err info.list 缺少 book_id 且无书名可解析");
        return None;
    }
    let norm = |s: &str| s.chars()
        .filter(|c| !c.is_whitespace() && !"《》·:：!！,，.。-—".contains(*c))
        .collect::<String>();
    let want = norm(title);
    let api = Client::from_config(cfg, vb);
    match api.search(title, 1).await {
        Ok(books) => {
            let hit = books.iter().find(|b| {
                let got = norm(&b.title);
                !want.is_empty() && !got.is_empty() && (got.contains(&want) || want.contains(&got))
            });
            match hit {
                Some(b) => {
                    println!("  按书名解析 book_id {}（{}）", b.book_id, b.title);
                    Some(b.book_id.clone())
                }
                None => {
                    eprintln!("  err 书名搜索无精确匹配（防误下错书），请手动在 info.list 补 book_id 后重试");
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("  err 搜索解析 book_id 失败: {}", e);
            None
        }
    }
}

