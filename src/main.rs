mod api;
mod audio;
mod config;
mod download;
mod epub;
mod types;

use std::fs;
use std::path::PathBuf;

use api::Client;
use clap::{Parser, Subcommand};
use types::{ChapterRange, Config};

#[derive(Parser)]
#[command(name = "fqdt", version, about = "番茄小说下载器")]
struct Cli {
    #[arg(long, global = true, help = "搜索 API 地址，逗号分隔多个")]
    search_url: Option<String>,
    #[arg(long, global = true, help = "目录 API 地址")]
    catalog_url: Option<String>,
    #[arg(long, global = true, help = "内容 API 地址，逗号分隔多个")]
    content_url: Option<String>,
    #[arg(long, global = true, help = "HTTP 超时(秒)")]
    timeout: Option<u64>,
    #[arg(long, global = true, help = "HTTP 方式: auto/minreq/curl")]
    http: Option<String>,
    #[arg(long, global = true, help = "curl 额外参数")]
    curl_args: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 搜索并下载小说
    Search {
        keyword: String,
        /// 页码
        #[arg(short='p', long, default_value="1")]
        page: usize,
        /// 自动下载第N本（跳过交互）
        #[arg(short='D', long)]
        auto: Option<usize>,
        /// 仅搜索不下载
        #[arg(long)]
        dry_run: bool,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并行下载数
        #[arg(short='j', long)]
        jobs: Option<usize>,
        /// 章节范围 1-50 / -5 / 10-
        #[arg(short='r', long)]
        range: Option<String>,
        /// 输出格式 txt/epub
        #[arg(short='t', long)]
        format: Option<String>,
        /// 下载间隔(ms)
        #[arg(short='i', long, default_value = "0")]
        interval: u64,
        /// 显示详细输出
        #[arg(short='v', long)]
        verbose: bool,
    },
    /// 查看目录或内容
    Info {
        book_id: String,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
        /// 显示章节正文
        #[arg(short='s', long)]
        show: bool,
        /// 显示详细输出
        #[arg(short='v', long)]
        verbose: bool,
    },
    /// 下载章节正文
    Download {
        book_id: String,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并行下载数
        #[arg(short='j', long)]
        jobs: Option<usize>,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
        /// 输出格式 txt/epub
        #[arg(short='t', long)]
        format: Option<String>,
        /// 下载间隔(ms)
        #[arg(short='i', long, default_value = "0")]
        interval: u64,
        /// 显示详细输出
        #[arg(short='v', long)]
        verbose: bool,
    },
    /// 增量更新（只下载新章节）
    Update {
        /// bookId 或已有目录（含 info.list）
        book_id: Option<String>,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并行下载数
        #[arg(short='j', long)]
        jobs: Option<usize>,
        /// 章节范围 1-50 / -5 / 10-
        #[arg(short='r', long)]
        range: Option<String>,
        /// 更新音频而非正文
        #[arg(long)]
        audio: bool,
        /// 显示详细输出
        #[arg(short='v', long)]
        verbose: bool,
        /// 下载间隔(ms)
        #[arg(short='i', long, default_value = "0")]
        interval: u64,
    },
    /// 书架管理
    Shelf {
        /// 添加 <ID>:<标题>
        #[arg(short='a', long)]
        add: Option<String>,
        /// 删除第N本
        #[arg(short='d', long)]
        delete: Option<usize>,
        /// 下载第N本
        #[arg(short='D', long)]
        dl: Option<usize>,
    },
    /// 生成默认配置
    Init,
    /// 测试 API 连接
    #[command(name = "test-api")]
    TestApi,
    /// 下载语音或 TTS 转语音
    Audio {
        /// bookId（下载官方语音用）
        book_id: Option<String>,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
        /// 音色 1/2/4/5/6/74/91
        #[arg(long, default_value = "1")]
        tone: usize,
        /// 文本转语音：文件或目录路径
        #[arg(short='t', long)]
        tts: Option<String>,
        /// TTS 语音名
        #[arg(long, default_value = "zh-CN-XiaoxiaoNeural")]
        voice: String,
        /// TTS 语速 (+0% / -50% / +100%)
        #[arg(long)]
        rate: Option<String>,
        /// TTS 音量 (+0% / -50%)
        #[arg(long)]
        volume: Option<String>,
        /// TTS 音调 (+0Hz / -10Hz / +10Hz)
        #[arg(long)]
        pitch: Option<String>,
        /// MP3 编码码率 kbps (0=原始, 32/64/128)
        #[arg(long)]
        abr: Option<u32>,
        /// 播放变速 (0.5/1.0/1.5/2.0)
        #[arg(long)]
        speed: Option<f32>,
        /// 音量归一化
        #[arg(long)]
        normalize: bool,
        /// 后处理命令模板 {input} {output}
        #[arg(long)]
        audio_cmd: Option<String>,
        /// LRC 模式: external/embed/both/off
        #[arg(long, default_value = "external")]
        lrc: String,
        /// 并行下载数
        #[arg(short='j', long)]
        jobs: Option<usize>,
        /// 下载间隔(ms)
        #[arg(short='i', long, default_value = "0")]
        interval: u64,
        /// 显示详细输出
        #[arg(short='v', long)]
        verbose: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let mut cfg = Config::load();
    cfg.apply_cli_overrides(cli.search_url.as_deref(), cli.catalog_url.as_deref(), cli.content_url.as_deref());
    if let Some(to) = cli.timeout { cfg.timeout = to; }
    if let Some(h) = cli.http { cfg.http_method = h; }
    if let Some(c) = cli.curl_args { cfg.curl_args = c; }
    cfg.ensure_dirs();
    Config::save_default().ok();

    match cli.cmd {
        Cmd::Search { keyword, page, auto, dry_run, output, jobs, range, format, interval, verbose } =>
            search(&keyword, page, output.as_deref(), jobs, range.as_deref(), format.as_deref(), verbose, auto, dry_run, interval, &cfg),
        Cmd::Info { book_id, range, show, verbose } =>
            info(&book_id, range.as_deref(), show, verbose, &cfg),
        Cmd::Download { book_id, output, jobs, range, format, interval, verbose } =>
            download(&book_id, output.as_deref(), jobs, range.as_deref(), format.as_deref(), verbose, interval, &cfg, None),
        Cmd::Update { book_id, output, jobs, range, audio, verbose, interval } =>
            update(book_id.as_deref(), output.as_deref(), jobs, range.as_deref(), audio, verbose, interval, &cfg),
        Cmd::Shelf { add, delete, dl } =>
            shelf(add, delete, dl, &cfg),
        Cmd::Init => {
            Config::save_default().ok();
            println!("  ok ~/.config/fqdt/config.ini");
        }
        Cmd::TestApi => test_api(&cfg),
        Cmd::Audio { book_id, output, range, tone, tts, voice, rate, volume, pitch, abr, speed, normalize, audio_cmd, lrc, jobs, interval, verbose } =>
            audio_dl(book_id.as_deref(), output.as_deref(), range.as_deref(), tone, verbose, tts.as_deref(), &voice, rate, volume, pitch, abr, speed, normalize, audio_cmd, &lrc, jobs, interval, &cfg),
    }
}

fn search(keyword: &str, page: usize, output: Option<&str>, concurrent: Option<usize>,
          range: Option<&str>, format: Option<&str>, verbose: bool, auto: Option<usize>,
          no_download: bool, interval: u64, cfg: &Config) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone());
    print!("  搜索 \"{}\" (第{}页)... ", keyword, page);
    flush();
    let books = match api.search(keyword, page) {
        Ok(b) => b, Err(e) => { eprintln!("\n  err {}", e); return; }
    };
    if books.is_empty() { println!("无结果"); return; }
    println!("{} 本", books.len());
    println!();

    let mut list_lines = 2;
    for (i, b) in books.iter().enumerate() {
        let status = b.status_text();
        let abs: String = b.abstract_.chars().take(60).collect();
        let abs = if b.abstract_.chars().count() > 60 { format!("{}...", abs) } else { b.abstract_.clone() };
    println!("  \x1b[1;36m{:>2}.\x1b[0m {}", i+1, b.title);
    println!("      {} \x1b[33m{}\x1b[0m | {} | \x1b[35m{}\x1b[0m \x1b[2m#{}\x1b[0m",
        b.author, b.category, status, b.score, b.book_id);
    list_lines += 2;
    if !abs.is_empty() {
        println!("      {}", abs);
        list_lines += 1;
    }
    println!();
    list_lines += 1;
}

if no_download {
    println!("\n  \x1b[2m提示: 使用 info <book_id> 查看目录, download <book_id> 下载\x1b[0m");
    return;
}

let idx = auto.map_or_else(|| {
        print!("  \x1b[2m输入序号 (1-{}, 0=取消): \x1b[0m", books.len());
        flush();
        list_lines += 1;
        let mut inp = String::new();
        std::io::stdin().read_line(&mut inp).unwrap();
        match inp.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= books.len() => n - 1,
            _ => { println!("  \x1b[2m取消\x1b[0m"); books.len() }
        }
    }, |n| if n >= 1 && n <= books.len() { n - 1 } else { println!("  err 无效序号"); books.len() });

    if idx >= books.len() { return; }
    let book = &books[idx];

    // fold search results, show selected book
    if auto.is_none() { print!("\x1b[{}F\x1b[J", list_lines); }
    println!("  \x1b[1;36m{}\x1b[0m  {} \x1b[33m{}\x1b[0m | {} | \x1b[35m{}\x1b[0m",
        book.title, book.author, book.category, book.status_text(), book.score);
    config::add_bookmark(&book.book_id, &book.title).ok();
    download(&book.book_id, output, concurrent, range, format, verbose, interval, cfg, Some(&book.title));
}

fn info(book_id: &str, range: Option<&str>, show: bool, verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), vb, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone());
    print!("  获取目录... ");
    flush();
    let chs = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let v: Vec<&types::Chapter> = chs.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    println!("{} 章", chs.len());

    if show {
        for c in &v {
            println!("\n  \x1b[1;36m{:04} {}\x1b[0m", c.index, c.title);
            match api.fetch_content(&c.item_id) {
                Ok(text) => {
                    for line in text.lines().take(40) {
                        println!("  {}", line);
                    }
                    if text.lines().count() > 40 { println!("  \x1b[2m... (共{}行)\x1b[0m", text.lines().count()); }
                }
                Err(e) => println!("  err {}", e),
            }
        }
    } else {
        for c in &v { println!("  {:04}  {}", c.index, c.title); }
        println!("\n  共 {} 章", v.len());
    }
}

fn download(book_id: &str, output: Option<&str>, concurrent: Option<usize>,
            range: Option<&str>, format: Option<&str>, verbose: bool,
            _interval: u64, cfg: &Config, book_title: Option<&str>) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone());
    print!("  获取目录... ");
    flush();
    let all = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let chs: Vec<&types::Chapter> = all.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    if chs.is_empty() { println!("  err 空范围"); return; }

    let fmt = format.unwrap_or(&cfg.format);
    let out = output.map(|s| s.into()).unwrap_or(cfg.output_dir.clone());
    let bt = book_title.unwrap_or("小说");
    let dler = download::Downloader::new(api, out, fmt, &cfg.filename_template, verbose || cfg.verbose,
        book_id, bt);
    dler.run(&chs, concurrent.unwrap_or(cfg.concurrent));
}

fn shelf(add: Option<String>, delete: Option<usize>, dl: Option<usize>, cfg: &Config) {
    if let Some(id_title) = add {
        if let Some((id, title)) = id_title.split_once(':') {
            match config::add_bookmark(id, title) {
                Ok(_) => println!("  ok 已添加"),
                Err(e) => eprintln!("  err {}", e),
            }
        } else { eprintln!("  err 格式: <ID>:<标题>"); }
        return;
    }
    if let Some(idx) = delete {
        match config::remove_bookmark(idx) {
            Ok(_) => println!("  ok 已删除 #{}", idx),
            Err(e) => eprintln!("  ✗ {}", e),
        }
        return;
    }
    if let Some(idx) = dl {
        let books = config::load_bookmarks();
        if idx == 0 || idx > books.len() { eprintln!("  err 无效编号"); return; }
        let (id, title) = &books[idx - 1];
        download(id, None, None, None, None, false, 0, cfg, Some(title));
        return;
    }
    let books = config::load_bookmarks();
    if books.is_empty() { println!("  书架为空"); return; }
    println!("  书架 ({}):\n", books.len());
    for (i, (id, t)) in books.iter().enumerate() {
        println!("  {:>2}. \x1b[1;36m{}\x1b[0m  (ID:{})", i+1, t, id);
    }
    println!("\n  添加: fqdt shelf -a <ID>:<标题>");
    println!("  删除: fqdt shelf -d <编号>");
    println!("  下载: fqdt shelf -D <编号>");
}

fn update(book_id: Option<&str>, output: Option<&str>, concurrent: Option<usize>,
          range: Option<&str>, audio: bool, verbose: bool, _interval: u64, cfg: &Config) {
    let path = match book_id { Some(s) => PathBuf::from(s), None => { eprintln!("  err 需要 book_id 或目录"); return; } };
    let vb = verbose || cfg.verbose;

    // 目录检测模式: 参数是已有目录
    if path.is_dir() {
        if audio {
            let audio_dir = path.join("Audio");
            if !audio_dir.exists() { eprintln!("  err Audio/ 目录不存在"); return; }
            let (bid, btitle, existing) = match download::read_audio_info_list(&audio_dir) {
                Ok(v) => v, Err(e) => { eprintln!("  err {}", e); return; }
            };
            let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
                cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
                cfg.audio_content_urls.clone(), vb, cfg.timeout,
                cfg.http_method.clone(), cfg.curl_args.clone());
            print!("  获取目录... "); flush();
            let all = match api.fetch_catalog(&bid) { Ok(c) => c, Err(e) => { eprintln!("\n  err {}", e); return; } };
            let r = range.and_then(ChapterRange::parse);
            let new_chs: Vec<&types::Chapter> = all.iter()
                .filter(|c| !existing.iter().any(|(idx,_,_)| *idx == c.index))
                .filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index)))
                .collect();
            if new_chs.is_empty() { println!("  音频已是最新 (共{}章)", all.len()); return; }
            println!("  发现 {} 章新音频 (共{}/{})", new_chs.len(), existing.len(), all.len());
            let fallbacks = if cfg.audio_tone_fallbacks.is_empty() { vec![2,4,5,6,74,91] } else { cfg.audio_tone_fallbacks.clone() };
            let dler = audio::AudioDownloader::new(api, audio_dir, cfg.audio_tone, fallbacks, &cfg.filename_template, vb,
                cfg.abr, None, false, &cfg.post_process, "external");
            dler.run(&new_chs, Some(&btitle));
            return;
        }

        let (bid, btitle, fmt, existing) = match download::read_info_list(&path) {
            Ok(v) => v, Err(e) => { eprintln!("  err {}", e); return; }
        };
        let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
            cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
            cfg.audio_content_urls.clone(), vb, cfg.timeout,
            cfg.http_method.clone(), cfg.curl_args.clone());
        print!("  获取目录... "); flush();
        let all = match api.fetch_catalog(&bid) { Ok(c) => c, Err(e) => { eprintln!("\n  err {}", e); return; } };
        if all.is_empty() { println!("  err 空目录"); return; }
        let r = range.and_then(ChapterRange::parse);
        let new_chs: Vec<&types::Chapter> = all.iter()
            .filter(|c| !existing.iter().any(|(idx,_,_)| *idx == c.index))
            .filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index)))
            .collect();
        if new_chs.is_empty() { println!("  已是最新 (共{}章)", all.len()); return; }
        println!("  发现 {} 章新章节 (共{}/{})", new_chs.len(), existing.len(), all.len());
        let dler = download::Downloader::new(api, path, &fmt, &cfg.filename_template, vb, &bid, &btitle);
        dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent));
        return;
    }

    let bid = book_id.unwrap();
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), vb, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone());
    print!("  获取目录... "); flush();
    let all = match api.fetch_catalog(bid) { Ok(c) => c, Err(e) => { eprintln!("\n  err {}", e); return; } };
    if all.is_empty() { println!("  err 空目录"); return; }

    let out_dir = output.map(PathBuf::from).unwrap_or(cfg.output_dir.clone());
    let r = range.and_then(ChapterRange::parse);

    if audio {
        let audio_dir = out_dir.join("Audio");
        let mut max_existing = 0usize;
        if audio_dir.exists() {
            if let Ok(entries) = fs::read_dir(&audio_dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.ends_with(".mp3") && name.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        if let Ok(n) = name.trim_end_matches(".mp3").parse::<usize>() {
                            if n > max_existing { max_existing = n; }
                        }
                    }
                }
            }
        }
        let new_chs: Vec<&types::Chapter> = all.iter()
            .filter(|c| c.index > max_existing)
            .filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index)))
            .collect();
        if new_chs.is_empty() { println!("  音频已是最新 (共{}章)", all.len()); return; }
        println!("  发现 {} 章新音频 (共{}→{})", new_chs.len(), max_existing, all.len());
        let fallbacks = if cfg.audio_tone_fallbacks.is_empty() { vec![2,4,5,6,74,91] } else { cfg.audio_tone_fallbacks.clone() };
        let dler = audio::AudioDownloader::new(api, audio_dir, cfg.audio_tone, fallbacks, &cfg.filename_template, vb,
            cfg.abr, None, false, &cfg.post_process, "external");
        dler.run(&new_chs, None);
        return;
    }

    let mut max_existing = 0usize;
    if out_dir.exists() {
        if let Ok(entries) = fs::read_dir(&out_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".txt") && name.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    if let Ok(n) = name.trim_end_matches(".txt").parse::<usize>() {
                        if n > max_existing { max_existing = n; }
                    }
                }
            }
        }
    }
    let new_chs: Vec<&types::Chapter> = all.iter()
        .filter(|c| c.index > max_existing)
        .filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index)))
        .collect();
    if new_chs.is_empty() { println!("  已是最新 (共{}章)", all.len()); return; }
    println!("  发现 {} 章新章节 (共{}→{})", new_chs.len(), max_existing, all.len());
    let dler = download::Downloader::new(api, out_dir, &cfg.format, &cfg.filename_template, vb, bid, "小说");
    dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent));
}

fn audio_dl(book_id: Option<&str>, output: Option<&str>, range: Option<&str>, tone: usize, verbose: bool,
            tts_path: Option<&str>, voice: &str,
            rate: Option<String>, volume: Option<String>, pitch: Option<String>,
            abr: Option<u32>, speed: Option<f32>, normalize: bool,
            audio_cmd: Option<String>, lrc_mode: &str, _jobs: Option<usize>, _interval: u64, cfg: &Config) {
    let tts_rate = rate.as_deref().unwrap_or(&cfg.tts_rate);
    let tts_volume = volume.as_deref().unwrap_or(&cfg.tts_volume);
    let tts_pitch = pitch.as_deref().unwrap_or(&cfg.tts_pitch);
    let abr_val = abr.unwrap_or(cfg.abr);
    let post_cmd = audio_cmd.as_deref().unwrap_or(&cfg.post_process);

    if let Some(path) = tts_path {
        let p = std::path::Path::new(path);
        if p.is_dir() {
            audio::convert_tts_dir(p, output.map(PathBuf::from), voice, tts_rate, tts_volume, tts_pitch,
                abr_val, speed, normalize, post_cmd, lrc_mode, verbose || cfg.verbose);
        } else if p.is_file() {
            audio::convert_tts_file(p, output.map(PathBuf::from), voice, tts_rate, tts_volume, tts_pitch,
                abr_val, speed, normalize, post_cmd, lrc_mode, verbose || cfg.verbose);
        } else {
            eprintln!("  err 文件不存在: {}", path);
        }
        return;
    }

    let bid = match book_id { Some(id) => id, None => { eprintln!("  err 需要 book_id 或 --tts"); return; } };
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone());
    print!("  获取目录... "); flush();
    let all = match api.fetch_catalog(bid) {
        Ok(c) => c, Err(e) => { eprintln!("\n  err {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let chs: Vec<&types::Chapter> = all.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    if chs.is_empty() { println!("  err 空范围"); return; }

    let out = output.map(|s| PathBuf::from(s).join("Audio")).unwrap_or_else(|| {
        let mut p = cfg.output_dir.clone();
        p.push("Audio");
        p
    });
    let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
        vec![2, 4, 5, 6, 74, 91]
    } else {
        cfg.audio_tone_fallbacks.clone()
    };
    let dler = audio::AudioDownloader::new(api, out, tone, fallbacks, &cfg.filename_template, verbose || cfg.verbose,
        abr_val, speed, normalize, post_cmd, lrc_mode);
    dler.run(&chs, None);
}

fn test_api(cfg: &Config) {
    fn test(api: &Client, label: &str, url: &str, desc: &str) {
        print!("  {} {} ... ", label, desc);
        flush();
        match api.http_get(url) {
            Ok(text) if !text.is_empty() => {
                let snippet: String = text.chars().take(120).collect();
                println!("\x1b[32m✓\x1b[0m {}b", text.len());
                println!("    {}", snippet);
            }
            Ok(_) => println!("\x1b[33m⚠\x1b[0m 空响应"),
            Err(e) => println!("\x1b[31m✗ {}\x1b[0m", e),
        }
    }

    let api = Client::new(
        cfg.cache_dir.clone(), false, 0,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), cfg.verbose, cfg.timeout,
        cfg.http_method.clone(), cfg.curl_args.clone(),
    );

    println!("\n  📡 API 测试\n");

    println!("  ── 搜索 ──");
    for tmpl in &cfg.search_urls {
        let url = tmpl.replacen("{}", "凡人", 1).replacen("{}", "0", 1);
        test(&api, "", &url, "search?q=凡人");
    }

    println!("\n  ── 目录 ──");
    let url = cfg.catalog_url.replacen("{}", "7481975434217786393", 1);
    test(&api, "", &url, "catalog?bookId=...");

    println!("\n  ── 内容 ──");
    for tmpl in &cfg.content_urls {
        let url = tmpl.replacen("{}", "7481975434217786393", 1);
        test(&api, "", &url, "content?item_id=...");
    }

    println!();
}


fn flush() {
    use std::io::Write;
    std::io::stdout().flush().unwrap();
}
