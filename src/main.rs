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
#[command(name = "fqdt", version, about = "📖 番茄小说下载器")]
struct Cli {
    /// 搜索API地址,逗号分隔多个按顺序尝试,用{}代替关键词和页码
    #[arg(long, global = true)]
    search_url: Option<String>,
    /// 目录API地址,用{}代替bookId
    #[arg(long, global = true)]
    catalog_url: Option<String>,
    /// 内容API地址,逗号分隔多个按顺序尝试,用{}代替item_id
    #[arg(long, global = true)]
    content_url: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 搜索小说并交互选择下载
    Search {
        /// 搜索关键词
        keyword: String,
        /// 页码(默认1)
        #[arg(short='p', long, default_value="1")]
        page: usize,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并发下载线程数
        #[arg(short='c', long)]
        concurrent: Option<usize>,
        /// 章节范围如 1-50 / -5(前5章) / 10-(10章起)
        #[arg(short='r', long)]
        range: Option<String>,
        /// 输出格式 txt/html/epub
        #[arg(short='t', long)]
        format: Option<String>,
        /// 调试输出
        #[arg(short='v', long)]
        verbose: bool,
        /// 自动下载指定序号(跳过交互选择)
        #[arg(short='D', long)]
        auto: Option<usize>,
        /// 仅搜索不下载
        #[arg(short='n', long)]
        no_download: bool,
    },
    /// 查看小说目录
    Info {
        /// 小说bookId
        book_id: String,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
    },
    /// 下载小说(需指定bookId)
    Download {
        /// 小说bookId
        book_id: String,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并发下载线程数
        #[arg(short='c', long)]
        concurrent: Option<usize>,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
        /// 输出格式 txt/html/epub
        #[arg(short='t', long)]
        format: Option<String>,
        /// 调试输出
        #[arg(short='v', long)]
        verbose: bool,
    },
    /// 增量更新(只下载新章节)
    Update {
        /// 小说bookId
        book_id: String,
        /// 输出目录(已有txt文件所在目录)
        #[arg(short='o', long)]
        output: Option<String>,
        /// 并发下载线程数
        #[arg(short='c', long)]
        concurrent: Option<usize>,
        /// 输出格式
        #[arg(short='t', long)]
        format: Option<String>,
        /// 调试输出
        #[arg(short='v', long)]
        verbose: bool,
    },
    /// 书架管理: list / add / remove / download
    Shelf {
        /// 添加书架: <ID>:<标题>
        #[arg(short='a', long)]
        add: Option<String>,
        /// 删除书架: 序号
        #[arg(short='d', long)]
        delete: Option<usize>,
        /// 下载书架: 序号
        #[arg(short='D', long)]
        dl: Option<usize>,
    },
    /// 生成默认配置文件
    Init,
    /// 测试API连接(搜索/目录/内容)
    #[command(name = "test-api")]
    TestApi,
    /// 下载语音(有声书MP3)
    Audio {
        /// 小说bookId
        book_id: String,
        /// 输出目录
        #[arg(short='o', long)]
        output: Option<String>,
        /// 章节范围
        #[arg(short='r', long)]
        range: Option<String>,
        /// 语音音色(1/2/4/5/6/74/91)
        #[arg(long, default_value = "1")]
        tone: usize,
        /// 调试输出
        #[arg(short='v', long)]
        verbose: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let mut cfg = Config::load();
    cfg.apply_cli_overrides(cli.search_url.as_deref(), cli.catalog_url.as_deref(), cli.content_url.as_deref());
    cfg.ensure_dirs();
    Config::save_default().ok();

    match cli.cmd {
        Cmd::Search { keyword, page, output, concurrent, range, format, verbose, auto, no_download } =>
            search(&keyword, page, output.as_deref(), concurrent, range.as_deref(), format.as_deref(), verbose, auto, no_download, &cfg),
        Cmd::Info { book_id, range } =>
            info(&book_id, range.as_deref(), &cfg),
        Cmd::Download { book_id, output, concurrent, range, format, verbose } =>
            download(&book_id, output.as_deref(), concurrent, range.as_deref(), format.as_deref(), verbose, &cfg, None),
        Cmd::Update { book_id, output, concurrent, format, verbose } =>
            update(&book_id, output.as_deref(), concurrent, format.as_deref(), verbose, &cfg),
        Cmd::Shelf { add, delete, dl } =>
            shelf(add, delete, dl, &cfg),
        Cmd::Init => {
            Config::save_default().ok();
            println!("  ✓ ~/.config/fqdt/config.ini");
        }
        Cmd::TestApi => test_api(&cfg),
        Cmd::Audio { book_id, output, range, tone, verbose } =>
            audio_dl(&book_id, output.as_deref(), range.as_deref(), tone, verbose, &cfg),
    }
}

fn search(keyword: &str, page: usize, output: Option<&str>, concurrent: Option<usize>,
          range: Option<&str>, format: Option<&str>, verbose: bool, auto: Option<usize>,
          no_download: bool, cfg: &Config) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose);
    print!("  🔍 \"{}\" (第{}页)... ", keyword, page);
    flush();
    let books = match api.search(keyword, page) {
        Ok(b) => b, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
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
    }, |n| if n >= 1 && n <= books.len() { n - 1 } else { println!("  ✗ 无效序号"); books.len() });

    if idx >= books.len() { return; }
    let book = &books[idx];

    // fold search results, show selected book
    if auto.is_none() { print!("\x1b[{}F\x1b[J", list_lines); }
    println!("  \x1b[1;36m{}\x1b[0m  {} \x1b[33m{}\x1b[0m | {} | \x1b[35m{}\x1b[0m",
        book.title, book.author, book.category, book.status_text(), book.score);
    config::add_bookmark(&book.book_id, &book.title).ok();
    download(&book.book_id, output, concurrent, range, format, verbose, cfg, Some(&book.title));
}

fn info(book_id: &str, range: Option<&str>, cfg: &Config) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), cfg.verbose);
    print!("  获取目录... ");
    flush();
    let chs = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let v: Vec<&types::Chapter> = chs.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    println!("{} 章", chs.len());
    for c in &v { println!("  {:04}  {}", c.index, c.title); }
    println!("\n  共 {} 章", v.len());
}

fn download(book_id: &str, output: Option<&str>, concurrent: Option<usize>,
            range: Option<&str>, format: Option<&str>, verbose: bool,
            cfg: &Config, book_title: Option<&str>) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose);
    print!("  获取目录... ");
    flush();
    let all = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let chs: Vec<&types::Chapter> = all.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    if chs.is_empty() { println!("  ❌ 空范围"); return; }

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
                Ok(_) => println!("  ✓ 已添加"),
                Err(e) => eprintln!("  ✗ {}", e),
            }
        } else { eprintln!("  ✗ 格式: <ID>:<标题>"); }
        return;
    }
    if let Some(idx) = delete {
        match config::remove_bookmark(idx) {
            Ok(_) => println!("  ✓ 已删除 #{}", idx),
            Err(e) => eprintln!("  ✗ {}", e),
        }
        return;
    }
    if let Some(idx) = dl {
        let books = config::load_bookmarks();
        if idx == 0 || idx > books.len() { eprintln!("  ✗ 无效编号"); return; }
        let (id, title) = &books[idx - 1];
        download(id, None, None, None, None, false, cfg, Some(title));
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

fn update(book_id: &str, output: Option<&str>, concurrent: Option<usize>,
          format: Option<&str>, verbose: bool, cfg: &Config) {
    let path = PathBuf::from(book_id);

    // 如果参数是已有目录, 从 info.list 自动检测
    if path.is_dir() {
        let (bid, btitle, fmt, existing) = match download::read_info_list(&path) {
            Ok(v) => v,
            Err(e) => { eprintln!("  ✗ {}", e); return; }
        };
        let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
            cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
            cfg.audio_content_urls.clone(), verbose || cfg.verbose);
        print!("  获取目录... ");
        flush();
        let all = match api.fetch_catalog(&bid) {
            Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
        };
        if all.is_empty() { println!("  ❌ 空目录"); return; }

        let new_chs: Vec<&types::Chapter> = all.iter()
            .filter(|c| !existing.iter().any(|(idx, _, _)| *idx == c.index))
            .collect();
        if new_chs.is_empty() {
            println!("  已是最新 (共{}章)", all.len());
            return;
        }
        println!("  发现 {} 章新章节 (共{}/{})", new_chs.len(), existing.len(), all.len());

        let f = format.unwrap_or(&fmt);
        let dler = download::Downloader::new(api, path, f, &cfg.filename_template, verbose || cfg.verbose,
            &bid, &btitle);
        dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent));
        return;
    }

    // 旧方式: book_id 参数
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose);
    print!("  获取目录... ");
    flush();
    let all = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    if all.is_empty() { println!("  ❌ 空目录"); return; }

    let fmt = format.unwrap_or(&cfg.format);
    let out_dir = output.map(PathBuf::from).unwrap_or(cfg.output_dir.clone());

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

    let new_chs: Vec<&types::Chapter> = all.iter().filter(|c| c.index > max_existing).collect();
    if new_chs.is_empty() {
        println!("  已是最新 (共{}章)", all.len());
        return;
    }
    println!("  发现 {} 章新章节 (共{}→{})", new_chs.len(), max_existing, all.len());

    let dler = download::Downloader::new(api, out_dir, fmt, &cfg.filename_template, verbose || cfg.verbose,
        book_id, "小说");
    dler.run(&new_chs, concurrent.unwrap_or(cfg.concurrent));
}

fn audio_dl(book_id: &str, output: Option<&str>, range: Option<&str>, tone: usize, verbose: bool, cfg: &Config) {
    let api = Client::new(cfg.cache_dir.clone(), cfg.cache_enabled, cfg.cache_ttl,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), verbose || cfg.verbose);
    print!("  获取目录... ");
    flush();
    let all = match api.fetch_catalog(book_id) {
        Ok(c) => c, Err(e) => { eprintln!("\n  ✗ {}", e); return; }
    };
    let r = range.and_then(ChapterRange::parse);
    let chs: Vec<&types::Chapter> = all.iter().filter(|c| r.as_ref().map_or(true, |x| x.contains(c.index))).collect();
    if chs.is_empty() { println!("  ❌ 空范围"); return; }

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
    let dler = audio::AudioDownloader::new(api, out, tone, fallbacks, &cfg.filename_template, verbose || cfg.verbose);
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
            Ok(_) => println!("\x1b[33m⚠ 空响应\x1b[0m"),
            Err(e) => println!("\x1b[31m✗ {}\x1b[0m", e),
        }
    }

    let api = Client::new(
        cfg.cache_dir.clone(), false, 0,
        cfg.search_urls.clone(), cfg.catalog_url.clone(), cfg.content_urls.clone(),
        cfg.audio_content_urls.clone(), cfg.verbose,
    );

    println!("\n  📡 API 测试\n");

    println!("  ── 搜索 ──");
    for tmpl in &cfg.search_urls {
        let url = tmpl.replacen("{}", "凡人", 1).replacen("{}", "0", 1);
        test(&api, "  🔍", &url, "search?q=凡人");
    }

    println!("\n  ── 目录 ──");
    let url = cfg.catalog_url.replacen("{}", "7481975434217786393", 1);
    test(&api, "  📖", &url, "catalog?bookId=...");

    println!("\n  ── 内容 ──");
    for tmpl in &cfg.content_urls {
        let url = tmpl.replacen("{}", "7481975434217786393", 1);
        test(&api, "  📄", &url, "content?item_id=...");
    }

    println!();
}

fn flush() {
    use std::io::Write;
    std::io::stdout().flush().unwrap();
}
