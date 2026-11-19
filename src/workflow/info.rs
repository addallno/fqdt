use serde_json::Value;

use crate::api::Client;
use crate::types::{ChapterRange, Config};
use crate::util::{self, BLUE, CYAN, DIM, GREEN, MAGENTA, RED, RST, YELLOW};

const SECTIONS: &str = "chapters · intro · author · comments · cover · related · audio";

/// # info — 书籍信息（概览 + 可选 section）
///
/// 无 section：概览 = 标题/作者/热度/简介 + 前 N 章 + 前 N 条评论
/// （N 与开关由 config.ini `[info]` 节控制，见 info_top_chapters 等）。
/// section：
/// - `chapters`  目录（含 item_id，`-r` 可筛选）
/// - `intro`     完整简介
/// - `author`    作者资料 + 书单（原顶层 `author` 命令迁入）
/// - `comments`  书评（原顶层 `comments` 命令迁入）
/// - `cover`     封面图 URL
pub async fn run(
    book_id: &str,
    section: Option<&str>,
    range: Option<&str>,
    show: bool,
    verbose: bool,
    cfg: &Config,
) {
    let vb = verbose || cfg.verbose;
    let api = Client::from_config(cfg, vb);

    if let Some(s) = section {
        match s {
            "chapters" => return chapters(&api, book_id, range).await,
            "intro" => return intro(&api, book_id).await,
            "author" => return author(book_id, cfg).await,
            "comments" => {
                let n = cfg.info_comments_count;
                crate::workflow::discover::comments(book_id, n, 0, false, cfg).await;
                return;
            }
            "cover" => return cover(book_id).await,
            "related" => return related(book_id, cfg).await,
            "audio" => return audio_section(book_id, cfg).await,
            other => {
                eprintln!("  {RED}err{RST} 未知 section: {other}");
                eprintln!("  可用: {SECTIONS}");
                return;
            }
        }
    }
    overview(&api, book_id, range, show, cfg).await;
}

/// 概览：头部（标题/热度/简介）+ 前 N 章 + 前 N 条评论（config `[info]` 节控制）。
async fn overview(api: &Client, book_id: &str, range: Option<&str>, show: bool, cfg: &Config) {
    if let Some(d) = detail_json(book_id).await {
        print_head(&d, cfg);
    }
    let chs = match fetch_catalog(api, book_id).await {
        Ok(c) => c,
        Err(e) => return eprintln!("  {RED}err{RST} {}", e),
    };
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());
    print_count(&v, &chs);
    print_top_chapters(&v, cfg.info_top_chapters, show, api).await;
    if cfg.info_top_comments > 0 {
        print_top_comments(book_id, cfg.info_top_comments, cfg.ui_clip_len).await;
    }
    println!("  {DIM}more: intro · author · comments · cover{RST}");
}

/// 概览头部：标题·作者 + 热度行（info_hot）+ 简介截断（info_intro_len，0=关）。
fn print_head(d: &Value, cfg: &Config) {
    let s = |p: &str| d.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
    println!("  {} {YELLOW}{}{RST}", s("/data/book_name"), s("/data/author"));
    if cfg.info_hot {
        let mut hot: Vec<String> = Vec::new();
        let read = {
            let t = s("/data/read_cnt_text");
            if t.is_empty() { s("/data/read_cnt_fallback_text") } else { t }
        };
        if !read.is_empty() { hot.push(read); }
        if let Ok(sc) = s("/data/score").parse::<f64>() {
            if sc > 0.0 { hot.push(format!("{}分", sc)); }
        }
        if let Ok(n) = s("/data/all_bookshelf_count").parse::<u64>() {
            if n > 0 { hot.push(format!("收藏 {}", fmt_wan(n))); }
        }
        if !hot.is_empty() { println!("  {DIM}{}{RST}", hot.join(" · ")); }
    }
    if cfg.info_intro_len > 0 {
        let intro = s("/data/abstract").replace('\n', " ");
        if !intro.is_empty() {
            let n = cfg.info_intro_len;
            let abbr: String = intro.chars().take(n).collect();
            let tail = if intro.chars().count() > n { "…" } else { "" };
            println!("  {DIM}{}{tail}{RST}", abbr);
        }
    }
}

/// 目录前 N 章（`-s` 附正文预览），超出部分给跳转尾注；top=0 关闭。
async fn print_top_chapters(v: &[&crate::types::Chapter], top: usize, show: bool, api: &Client) {
    if top == 0 { return; }
    let take = v.len().min(top);
    for c in v.iter().take(take) {
        if !show {
            println!("  {:04}  {}", c.index, c.title);
            continue;
        }
        println!("\n  {CYAN}{:04} {}{RST}", c.index, c.title);
        match api.fetch_content(&c.item_id).await {
            Ok(text) => {
                let total = text.lines().count();
                for line in text.lines().take(40) { println!("    {}", line); }
                if total > 40 {
                    println!("  {DIM}… 共 {} 行 · 全文 function fetch-content {}{RST}", total, c.item_id);
                }
            }
            Err(e) => eprintln!("  {RED}err{RST} {:04}: {}", c.index, e),
        }
    }
    if v.len() > take {
        println!("  {DIM}… 更多 → info <book_id> chapters{RST}");
    }
}

/// 前 N 条书评（单行式：用户 · 赞 N  文本）。
async fn print_top_comments(book_id: &str, n: usize, clip: usize) {
    util::progress("获取书评");
    let res = crate::fqapi::get_json(
        &format!("/api/v1/books/{book_id}/reviews"),
        &format!("count={n}&offset=0"),
    )
    .await;
    util::clear_progress();
    let Ok(v) = res else { return };
    let Some(items) = v.pointer("/data/data_list").and_then(|x| x.as_array()) else {
        println!("  {DIM}（暂无书评）{RST}");
        return;
    };
    for item in items.iter().take(n) {
        let Some(c) = item.get("comment") else { continue };
        let user = c.pointer("/common/user_info/base_info/user_name")
            .or_else(|| c.pointer("/common/user_info/user_name"))
            .and_then(|x| x.as_str()).filter(|s| !s.is_empty()).unwrap_or("匿名");
        let digg = c.pointer("/stat/digg_count").and_then(|x| x.as_u64()).unwrap_or(0);
        let text: String = c.pointer("/common/content/text").and_then(|x| x.as_str())
            .unwrap_or("").chars().take(clip).collect::<String>().replace('\n', " ");
        println!("  {YELLOW}{user}{RST} {DIM}· 赞 {digg}{RST}  {text}");
    }
}

/// 万/亿缩写（收藏数等大数字）。
fn fmt_wan(n: u64) -> String {
    if n >= 100_000_000 { format!("{:.1}亿", n as f64 / 100_000_000.0) }
    else if n >= 10_000 { format!("{:.1}万", n as f64 / 10_000.0) }
    else { n.to_string() }
}

/// 目录（含 item_id），`-r` 可筛选。
async fn chapters(api: &Client, book_id: &str, range: Option<&str>) {
    util::progress("获取目录");
    let chs = match fetch_catalog(api, book_id).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            return eprintln!("  {RED}err{RST} {}", e);
        }
    };
    util::clear_progress();
    let r = range.and_then(ChapterRange::parse);
    let v = crate::util::filter_by_range(&chs, r.as_ref());
    print_count(&v, &chs);
    for c in &v {
        println!("  {:04}  {}  {DIM}#{}{RST}", c.index, c.title, c.item_id);
    }
}

/// 完整简介。
async fn intro(api: &Client, book_id: &str) {
    match api.fetch_detail(book_id).await {
        Ok(d) => {
            let parts: Vec<&str> = d.splitn(3, '|').collect();
            println!("  {} {YELLOW}{}{RST}", parts[0], parts[1]);
            if parts.len() > 2 && !parts[2].is_empty() {
                println!("\n{}", parts[2]);
            } else {
                println!("  {DIM}（无简介）{RST}");
            }
        }
        Err(e) => eprintln!("  {RED}err{RST} {}", e),
    }
}

/// 封面图 URL。
async fn cover(book_id: &str) {
    let Some(v) = detail_json(book_id).await else { return };
    let url = ["thumb_url", "detail_page_thumb_url"]
        .iter()
        .find_map(|k| v.pointer(&format!("/data/{k}")).and_then(|x| x.as_str()))
        .unwrap_or("");
    if url.is_empty() {
        eprintln!("  {RED}err{RST} 该书无封面字段");
    } else {
        println!("  {url}");
    }
}

/// 作者资料 + 书单（id 取 detail 的 bind_author_ids / author_id）。
async fn author(book_id: &str, cfg: &Config) {
    let Some(v) = detail_json(book_id).await else { return };
    let id = detail_author_id(&v);
    if id.is_empty() {
        return eprintln!("  {RED}err{RST} 详情中无作者 id");
    }
    crate::workflow::discover::author(&id, cfg.info_author_count, false, cfg).await;
}

/// 相关书籍 + 听书/音色 misc（`fqdt info <id> related`）。
async fn related(book_id: &str, cfg: &Config) {
    let misc_lines = related_misc(book_id).await;
    util::progress("获取相关书籍");
    let res = crate::fqapi::get_json(&format!("/api/v1/books/{book_id}/related"), "").await;
    util::clear_progress();
    let Ok(v) = res else { return };
    let mut n = 0;
    if let Some(cells) = v.pointer("/data/cell_data").and_then(|x| x.as_array()) {
        for cell in cells {
            let Some(books) = cell.get("book_data").and_then(|x| x.as_array()) else { continue };
            for b in books {
                let s = |k: &str| b.get(k).and_then(|x| x.as_str()).unwrap_or("");
                let (id, name, author) = (s("book_id"), s("book_name"), s("author"));
                if name.is_empty() { continue; }
                n += 1;
                let author: String = author.chars().take(cfg.ui_clip_len).collect();
                crate::emit_line(&format!("  {CYAN}{name}{RST} {MAGENTA}{author}{RST} {BLUE}#{id}{RST}"));
            }
        }
    }
    if n == 0 { crate::emit_line("  （无相关书籍）"); }
    for m in &misc_lines { crate::emit_line(&format!("  {DIM}{m}{RST}")); }
}

/// related 杂项（两行）：书目（字数/章数/分类/听书量/起书时间）+ 音频（真人版数/AI 音色数）。
async fn related_misc(book_id: &str) -> Vec<String> {
    let Some(d) = detail_json(book_id).await else { return vec![] };
    let s = |p: &str| d.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let num = |p: &str| s(p).parse::<u64>().unwrap_or(0);
    let mut lines = vec![];
    let mut book = vec![];
    let w = num("/data/word_number");
    if w > 0 { book.push(format!("字数 {}", fmt_wan(w))); }
    let sc = num("/data/serial_count");
    if sc > 0 { book.push(format!("{sc}章")); }
    let cat = s("/data/category");
    if !cat.is_empty() { book.push(cat); }
    let lc = num("/data/listen_count");
    if lc > 0 { book.push(format!("听书 {}", fmt_wan(lc))); }
    let ct = s("/data/create_time");
    if ct.len() >= 7 { book.push(format!("{} 起", &ct[..7])); }
    if !book.is_empty() { lines.push(book.join(" · ")); }
    let audio_n = s("/data/related_audio_bookids").trim_matches(|c| c == '[' || c == ']')
        .split(',').filter(|x| !x.trim().is_empty()).count();
    let tone_n = s("/data/tones").split(',').filter(|x| !x.trim().is_empty()).count();
    let mut audio = vec![];
    if audio_n > 0 { audio.push(format!("真人听书版 {audio_n}")); }
    if tone_n > 0 { audio.push(format!("AI 音色 {tone_n}")); }
    if !audio.is_empty() { lines.push(audio.join(" · ")); }
    lines
}

/// 听书版本与音色（`fqdt info <id> audio`）：AI 音色 id 列表 + 各听书版主播阵容。
async fn audio_section(book_id: &str, cfg: &Config) {
    let Some(d) = detail_json(book_id).await else { return };
    let s = |p: &str| d.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let tones = s("/data/tones");
    if !tones.is_empty() {
        let ids: Vec<&str> = tones.split(',').filter(|x| !x.trim().is_empty()).collect();
        crate::emit_line(&format!("  {CYAN}AI 音色 {}{RST} ({})  {DIM}{}{RST}",
            ids.len(), cfg.audio_tone, ids.join(",")));
    }
    util::progress("获取听书版本");
    let res = crate::fqapi::get_json(&format!("/api/v1/books/{book_id}/tones"), "").await;
    util::clear_progress();
    let Ok(v) = res else { return };
    let Some(list) = v.pointer("/data/audio_tones").and_then(|x| x.as_array()) else {
        crate::emit_line("  （无听书版本）");
        return;
    };
    for t in list {
        let id = t.get("abook_id").and_then(|x| x.as_str().map(String::from))
            .or_else(|| t.get("abook_id").and_then(|x| x.as_u64().map(|n| n.to_string())))
            .unwrap_or_default();
        let title = t.get("title").and_then(|x| x.as_str()).unwrap_or("");
        let title: String = title.chars().take(cfg.ui_clip_len.saturating_mul(3)).collect();
        crate::emit_line(&format!("  {GREEN}{title}{RST} {BLUE}#{id}{RST}"));
    }
}

/// 目录（带进度与错误处理的共用包装）。
async fn fetch_catalog(api: &Client, book_id: &str) -> Result<Vec<crate::types::Chapter>, String> {
    util::progress("获取目录");
    let r = api.fetch_catalog(book_id).await;
    util::clear_progress();
    r
}

/// 目录计数只报一次：有筛选显示 m/n，无筛选显示总数。
fn print_count(v: &[&crate::types::Chapter], all: &[crate::types::Chapter]) {
    if v.len() < all.len() {
        println!("  显示 {DIM}{}{RST}/{} 章", v.len(), all.len());
    } else {
        println!("  共 {} 章", all.len());
    }
}

/// 书籍详情 JSON（fqapi 直连，同 discover::fetch）。
async fn detail_json(book_id: &str) -> Option<Value> {
    match crate::fqapi::get_json(&format!("/api/v1/books/{book_id}"), "").await {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("  {RED}err{RST} 获取详情失败: {e}");
            None
        }
    }
}

/// 从详情提取作者 id（`author_id` 原样，如 `2_698…`；裸数字会错拿他人）。
fn detail_author_id(v: &Value) -> String {
    v.pointer("/data/author_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}
