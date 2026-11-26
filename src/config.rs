use crate::types::{Config, get_home};
use crate::util::{CYAN, DIM, GREEN, RED, RST, YELLOW};
use std::fs;
use std::path::PathBuf;

impl Config {
    pub fn load() -> Self {
        let path = get_home().join(".config/fqdt/config.ini");
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(&path) {
            let mut section = String::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some(sec) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    section = sec.trim().to_lowercase();
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim(); let v = v.trim();
                    if section == "workflow_cmd" && !k.is_empty() {
                        cfg.custom_commands.insert(k.to_string(), v.to_string());
                        continue;
                    }
                    match k {
                        "concurrent" => { let n: usize = v.parse().unwrap_or(0); if n > 0 { cfg.concurrent = n; } }
                        "format" => cfg.format = v.into(),
                        "output_dir" => cfg.output_dir = PathBuf::from(v),
                        "filename_template" => cfg.filename_template = v.into(),
                        "verbose" => cfg.verbose = v == "true",
                        "cache_enabled" => cfg.cache_enabled = v != "false",
                        "cache_ttl" => cfg.cache_ttl = v.parse().unwrap_or(86400),
                        "search_url" => cfg.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "catalog_url" => cfg.catalog_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "content_url" => cfg.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "batch_url" => cfg.batch_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "detail_url" => cfg.detail_url = v.into(),
                        "audio_content_url" => cfg.audio_content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                        "audio_tone" => cfg.audio_tone = v.parse().unwrap_or(1),
                        "audio_tone_fallbacks" => cfg.audio_tone_fallbacks = v.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
                        "interval_ms" => cfg.interval_ms = v.parse().unwrap_or(0),
                        "timeout" => cfg.timeout = v.parse().unwrap_or(15),
                        "tts_rate" => cfg.tts_rate = v.into(),
                        "tts_volume" => cfg.tts_volume = v.into(),
                        "tts_pitch" => cfg.tts_pitch = v.into(),
                        "abr" => cfg.abr = v.parse().unwrap_or(0),
                        "post_process" => cfg.post_process = v.into(),
                        "audio_keep_m4a" => cfg.audio_keep_m4a = v == "true",
                        "audio_mp3" => cfg.audio_mp3 = v == "true",
                        "audio_cover" => cfg.audio_cover = v == "true",
                        "info_top_chapters" => cfg.info_top_chapters = v.parse().unwrap_or(10),
                        "info_top_comments" => cfg.info_top_comments = v.parse().unwrap_or(3),
                        "info_hot" => cfg.info_hot = v != "false",
                        "info_intro_len" => cfg.info_intro_len = v.parse().unwrap_or(80),
                        "info_comments_count" => cfg.info_comments_count = v.parse().unwrap_or(20),
                        "info_author_count" => cfg.info_author_count = v.parse().unwrap_or(20),
                        "ui_clip_len" => cfg.ui_clip_len = v.parse().unwrap_or(40),
                        "ui_color" => cfg.ui_color = v != "false",
                        "ui_help_examples" => cfg.ui_help_examples = v != "false",
                        "page_mode" => cfg.ui_page_mode = v.into(),
                        "view_mode" => cfg.ui_view_mode = v.into(),
                        _ => {}
                    }
                }
            }
        }
        cfg
    }

    /// 生成默认 config.ini；force=false 时已存在则不动（不覆盖用户改动）。
    pub fn save_default(force: bool) -> Result<(), String> {
        let d = get_home().join(".config/fqdt");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let path = d.join("config.ini");
        if path.exists() && !force { return Ok(()); }
        fs::write(&path, Self::DEFAULT_INI).map_err(|e| e.to_string())
    }

    // ===== 默认 config.ini 模板（save_default） =====
// 节名必须与 KEYS 各条目的节一致（ini_write 按 KEYS 节写入，节名不同会产生重复键）。
// 每键上方一行中文注释说明作用；配置说明唯一来源=本文件 + `fqdt config <key> --help`。
const DEFAULT_INI: &str = "\
# 番茄小说下载器配置
# 直接编辑本文件，或 fqdt config <点号键> <值>（如 fqdt config download.format epub）
# 优先级: 命令行 flag > 本文件同义项

## Download
[download]
# 正文/音频并发下载线程数，0=按 CPU 核数*2 自动，上限 32
concurrent = 0
# 默认输出格式 txt 或 epub
format = txt
# 默认输出目录，相对路径基于当前工作目录
output_dir = .
# 文件名模板，{idx04}=章序四位，{title}=章标题
filename_template = {idx04}_{title}
# 默认详细日志，true 等价全局 -v
verbose = false

## Cache
[cache]
# true=启用目录/详情缓存（减少上游请求），false=每次请求上游
cache_enabled = true
# 缓存有效期秒，过期条目下次请求时刷新
cache_ttl = 86400

## API
[api]
# 请求超时秒数（全局默认 15）；单章下载另有 10s 内层超时
timeout = 15
# 第三方搜索降级源（主源失败时用），逗号分隔按序尝试；{} 依次换关键词与页码
search_url = https://novel.snssdk.com/api/novel/channel/homepage/search/search/v1/?aid=1967&q={}&offset={},http://101.35.133.34:5000/api/search?key={}&offset={}
# 第三方目录降级源，{} 换 book_id
catalog_url = https://fanqienovel.com/api/reader/directory/detail?bookId={},http://101.35.133.34:5000/api/book?book_id={},http://101.35.133.34:5000/api/directory?book_id={}
# 第三方正文降级源，{} 换 item_id
content_url = http://101.35.133.34:5000/api/content?tab=小说&item_id={},http://101.35.133.34:5000/api/chapter?item_id={},http://101.35.133.34:5000/api/raw_full?item_id={},https://tt.sjmyzq.cn/api/raw_full?item_id={}
# 批量正文源，{} 换 book_id 与逗号分隔 item_ids，仅历史兼容
batch_url = http://101.35.133.34:5000/api/content?tab=批量&book_id={}&item_ids={}
# 第三方详情降级源，{} 换 book_id
detail_url = http://101.35.133.34:5000/api/detail?book_id={}
# 第三方语音内容源，两个 {} 依次为 item_id 与 tone_id
audio_content_url = http://101.35.133.34:5000/api/content?tab=听书&item_id={}&tone_id={}

## Audio
[audio]
# 默认音色编号（AI 音色 1-91；真人讲书用 --real 选集内音色），fqdt info audio <id> 查本书可用集合
audio_tone = 1
# 音色回退列表，主音色失败时依次尝试
audio_tone_fallbacks = 2,4,5,6,74,91
# MP3 压缩码率 kbps，0=不重编码，可选 32/64/128
abr = 0
# 下载后处理命令模板（如 ffmpeg 转码），{input} {output} 为占位符，留空=不执行
post_process =
# 保持 m4a 原样不转 MP3，true/false
audio_keep_m4a = false
# --mp3：true=转码 MP3（iOS/WMP 兜底），false=默认 remux Ogg Opus
audio_mp3 = false
# 下载时嵌入书籍封面到 MP3，true/false
audio_cover = false

## TTS
[tts]
# Edge-TTS 语速，如 +0% / +10%
tts_rate = +0%
# Edge-TTS 音量，如 +0% / -20%
tts_volume = +0%
# Edge-TTS 音调，如 +0Hz / +5Hz
tts_pitch = +0Hz

## Info
[info]
# 概览显示目录前 N 章，0=关闭该段
info_top_chapters = 10
# 概览显示前 N 条书评，0=关闭
info_top_comments = 3
# true=显示热度行（阅读量/评分/收藏），false=隐藏
info_hot = true
# 简介截断字数，0=不显示简介
info_intro_len = 80
# info comments 章节/书评取前 N 条
info_comments_count = 20
# info author 推荐书单取前 N 条
info_author_count = 20

## UI
[ui]
# 评论/摘要/相关书作者名截断字数，0=不截断
ui_clip_len = 40
# true=彩色输出，false=纯文本（等价 NO_COLOR=1）
ui_color = true
# false=help 不带 Examples 段（默认），true=各命令 help 底部追加示例
ui_help_examples = false
# 阅读翻页模式：right-left=左右渲染（默认）/ up-down=上下渲染
page_mode = right-left
# 阅读视图默认模式：normal=分栏（默认）/ zen=全屏四边框（TUI 内 z 临时切换当次）
view_mode = normal
";

    pub fn apply_cli_overrides(&mut self, search_url: Option<&str>, catalog_url: Option<&str>, content_url: Option<&str>) {
        if let Some(v) = search_url { self.search_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
        if let Some(v) = catalog_url { self.catalog_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
        if let Some(v) = content_url { self.content_urls = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); }
    }

    pub fn ensure_dirs(&self) {
        fs::create_dir_all(&self.cache_dir).ok();
        if let Some(p) = self.bookmark_file.parent() { fs::create_dir_all(p).ok(); }
    }

    pub fn validate(&self) {
        let mut issues: Vec<String> = vec![];
        if self.search_urls.is_empty() { issues.push("search_urls 为空".into()); }
        if self.catalog_urls.is_empty() { issues.push("catalog_urls 为空".into()); }
        if self.content_urls.is_empty() { issues.push("content_urls 为空".into()); }
        if self.concurrent == 0 { issues.push("concurrent=0 无效，使用默认值".into()); }
        if self.format != "txt" && self.format != "epub" { issues.push("format 必须是 txt 或 epub".into()); }
        if !self.cache_dir.exists() {
            if let Err(e) = fs::create_dir_all(&self.cache_dir) {
                issues.push(format!("cache_dir 创建失败: {}", e));
            }
        }
        if !issues.is_empty() {
            eprintln!("  {YELLOW}配置警告:{RST}");
            for i in &issues { eprintln!("    - {}", i); }
        }
    }
}

// ===== git 式 config 子命令（fqdt config --list / get / set / unset） =====

#[derive(Clone, Copy, PartialEq)]
pub enum Kind { Int, Bool, Str }

/// 点号键表项：(点号键, ini 键, 节, 类型, 取值器)
type KeyEntry = (&'static str, &'static str, &'static str, Kind, fn(&Config) -> String);

/// 点号键 → (ini 键, 所属节, 类型, 取值器)。`fqdt config --list` 与 set/get 共用。
pub const KEYS: &[KeyEntry] = &[
    ("download.concurrent", "concurrent", "download", Kind::Int, |c| c.concurrent.to_string()),
    ("download.format", "format", "download", Kind::Str, |c| c.format.clone()),
    ("download.output_dir", "output_dir", "download", Kind::Str, |c| c.output_dir.display().to_string()),
    ("download.filename_template", "filename_template", "download", Kind::Str, |c| c.filename_template.clone()),
    ("download.verbose", "verbose", "download", Kind::Bool, |c| c.verbose.to_string()),
    ("cache.enabled", "cache_enabled", "cache", Kind::Bool, |c| c.cache_enabled.to_string()),
    ("cache.ttl", "cache_ttl", "cache", Kind::Int, |c| c.cache_ttl.to_string()),
    ("api.search_url", "search_url", "api", Kind::Str, |c| c.search_urls.join(",")),
    ("api.catalog_url", "catalog_urls", "api", Kind::Str, |c| c.catalog_urls.join(",")),
    ("api.content_url", "content_urls", "api", Kind::Str, |c| c.content_urls.join(",")),
    ("api.batch_url", "batch_urls", "api", Kind::Str, |c| c.batch_urls.join(",")),
    ("api.detail_url", "detail_url", "api", Kind::Str, |c| c.detail_url.clone()),
    ("api.audio_content_url", "audio_content_urls", "api", Kind::Str, |c| c.audio_content_urls.join(",")),
    ("api.timeout", "timeout", "api", Kind::Int, |c| c.timeout.to_string()),
    ("audio.tone", "audio_tone", "audio", Kind::Int, |c| c.audio_tone.to_string()),
    ("audio.tone_fallbacks", "audio_tone_fallbacks", "audio", Kind::Str, |c| c.audio_tone_fallbacks.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")),
    ("audio.abr", "abr", "audio", Kind::Int, |c| c.abr.to_string()),
    ("audio.post_process", "post_process", "audio", Kind::Str, |c| c.post_process.clone()),
    ("audio.keep_m4a", "audio_keep_m4a", "audio", Kind::Bool, |c| c.audio_keep_m4a.to_string()),
    ("audio.mp3", "audio_mp3", "audio", Kind::Bool, |c| c.audio_mp3.to_string()),
    ("audio.cover", "audio_cover", "audio", Kind::Bool, |c| c.audio_cover.to_string()),
    ("tts.rate", "tts_rate", "tts", Kind::Str, |c| c.tts_rate.clone()),
    ("tts.volume", "tts_volume", "tts", Kind::Str, |c| c.tts_volume.clone()),
    ("tts.pitch", "tts_pitch", "tts", Kind::Str, |c| c.tts_pitch.clone()),
    ("info.top_chapters", "info_top_chapters", "info", Kind::Int, |c| c.info_top_chapters.to_string()),
    ("info.top_comments", "info_top_comments", "info", Kind::Int, |c| c.info_top_comments.to_string()),
    ("info.hot", "info_hot", "info", Kind::Bool, |c| c.info_hot.to_string()),
    ("info.intro_len", "info_intro_len", "info", Kind::Int, |c| c.info_intro_len.to_string()),
    ("info.comments_count", "info_comments_count", "info", Kind::Int, |c| c.info_comments_count.to_string()),
    ("info.author_count", "info_author_count", "info", Kind::Int, |c| c.info_author_count.to_string()),
    ("ui.clip_len", "ui_clip_len", "ui", Kind::Int, |c| c.ui_clip_len.to_string()),
    ("ui.color", "ui_color", "ui", Kind::Bool, |c| c.ui_color.to_string()),
    ("ui.help_examples", "ui_help_examples", "ui", Kind::Bool, |c| c.ui_help_examples.to_string()),
    ("ui.page_mode", "page_mode", "ui", Kind::Str, |c| c.ui_page_mode.clone()),
    ("ui.view_mode", "view_mode", "ui", Kind::Str, |c| c.ui_view_mode.clone()),
];

// ===== config 键详情（`fqdt config <key> --help`，v0.17.0） =====

/// 键详情：(点号键, 一句话标题, 详细说明)。
const CONFIG_HELP: &[(&str, &str, &str)] = &[
    ("download.concurrent", "并发下载线程数", "0=按 CPU 核数自动；上限 32。与 -j 同义，flag 优先"),
    ("download.format", "默认输出格式", "txt 或 epub；-t/--format 同义覆盖"),
    ("download.output_dir", "默认输出目录", "相对路径基于当前工作目录；-o 同义覆盖"),
    ("download.filename_template", "文件名模板", "{idx04} 章序四位、{title} 章标题；占位符按序替换"),
    ("download.verbose", "默认详细日志", "true 等价全局 -v；stdout 仍只放数据"),
    ("cache.enabled", "文件缓存开关", "false 关闭目录/详情缓存"),
    ("cache.ttl", "缓存有效期秒", "默认 86400；过期条目下次请求时刷新"),
    ("api.search_url", "第三方搜索降级源", "逗号分隔从前往后尝试；{} 换关键词与页码；fqapi 主通道失败才用"),
    ("api.catalog_url", "第三方目录降级源", "同上；{} 换 book_id"),
    ("api.content_url", "第三方正文降级源", "同上；{} 换 item_id；fqapi full 批量优先"),
    ("api.batch_url", "第三方批量正文源", "已不作首选，仅历史兼容"),
    ("api.detail_url", "第三方详情降级源", "{} 换 book_id；主通道为 fqapi /api/v1/books/{id}"),
    ("api.audio_content_url", "第三方音频内容源", "两个 {}：item_id 与 tone_id；主通道 fqapi audio play"),
    ("api.timeout", "请求超时秒", "全局默认 15；单章下载另有 10s 内层超时"),
    ("audio.tone", "默认 AI 音色编号", "1-91；audio --tone 与 info audio <id> 查询本书可用集合"),
    ("audio.tone_fallbacks", "音色回退列表", "主音色失败时依次尝试的编号"),
    ("audio.abr", "MP3 压缩码率", "0=不重编码；32/64/128 kbps"),
    ("audio.post_process", "后处理命令模板", "{input} {output} 占位；如 lame --abr 32 {input} {output}"),
    ("audio.keep_m4a", "保持 m4a 原样", "true=不解码重编码；默认 false 有 ffmpeg 即 remux Ogg；audio --keep-m4a 同义覆盖"),
    ("audio.mp3", "转码 MP3", "true=有 ffmpeg 时 libmp3lame 转码 MP3（iOS/WMP 兜底）；默认 false 输出 Ogg Opus；audio --mp3 同义覆盖，与 --keep-m4a 互斥"),
    ("audio.cover", "嵌入书籍封面", "true=新下载 MP3 写入 ID3 APIC；audio --cover 同义覆盖"),
    ("tts.rate", "Edge-TTS 语速", "如 +0% / +10%；audio --tts 时生效"),
    ("tts.volume", "Edge-TTS 音量", "如 +0% / -20%"),
    ("tts.pitch", "Edge-TTS 音调", "如 +0Hz / +5Hz"),
    ("info.top_chapters", "概览前 N 章", "0=关闭该段；完整列表用 info chapters <id>"),
    ("info.top_comments", "概览前 N 条书评", "0=关闭；完整用 info comments <id>"),
    ("info.hot", "概览热度行开关", "false 隐藏 read_cnt_text·评分·收藏行"),
    ("info.intro_len", "简介截断字数", "概览与 discover 描述截断；0=不显示简介"),
    ("info.comments_count", "书评条数", "info comments 取前 N 条"),
    ("info.author_count", "书单条数", "info author 推荐书单取前 N 条"),
    ("ui.clip_len", "UI 文本截断字数", "评论/摘要/相关书作者名截断；0=不截断"),
    ("ui.color", "彩色输出开关", "false 等价 NO_COLOR=1；非 TTY 本就纯文本"),
    ("ui.help_examples", "help 示例段开关", "默认关闭；true 在子命令 help 底部追加示例段"),
    ("ui.page_mode", "阅读翻页模式", "right-left=左右渲染（默认）/ up-down=上下渲染"),
    ("ui.view_mode", "视图默认模式", "normal=分栏（默认）/ zen=全屏四边框；view TUI 内 z 临时切换当次"),
];

fn kind_label(k: Kind) -> &'static str {
    match k { Kind::Int => "整数", Kind::Bool => "布尔 true/false", Kind::Str => "字符串" }
}

/// `fqdt config <key> --help`：命中渲染详情返回 true，未命中返回 false。
pub fn key_help(part: &str) -> bool {
    let tail = |e: &KeyEntry| e.0.split('.').next_back().unwrap_or("");
    let hits: Vec<&KeyEntry> = KEYS.iter()
        .filter(|e| e.0 == part || tail(e) == part || tail(e).ends_with(part)).collect();
    match hits.len() {
        0 => false,
        1 => { render_key_help(hits[0]); true }
        _ => {
            crate::emit_line(&format!("  {YELLOW}多个键匹配 {part}:{RST}"));
            for e in &hits {
                let brief = CONFIG_HELP.iter().find(|c| c.0 == e.0).map(|c| c.1).unwrap_or("");
                crate::emit_line(&format!("    {}  {DIM}{brief}{RST}", e.0));
            }
            crate::emit_line(&format!("  {DIM}补全点号键后重试: fqdt config {} --help{RST}", hits[0].0));
            true
        }
    }
}

/// 渲染单个 config 键的结构化详情。
fn render_key_help(e: &KeyEntry) {
    let (dotted, ini, sec, kind, get) = *e;
    let help = CONFIG_HELP.iter().find(|c| c.0 == dotted);
    let (brief, desc) = match help { Some(c) => (c.1, c.2), None => ("", "") };
    crate::emit_line(&format!("  {CYAN}{dotted}{RST}  {YELLOW}{brief}{RST}"));
    crate::emit_line(&format!("  {DIM}fqdt config set {dotted} <value> · flag 优先于 config{RST}"));
    crate::emit_line("");
    crate::emit_line(&format!("    {:<4}  {}", "类型", kind_label(kind)));
    crate::emit_line(&format!("    {:<4}  config.ini [{sec}] {ini}", "位置"));
    crate::emit_line(&format!("    {:<4}  {}", "当前", get(&Config::load())));
    crate::emit_line(&format!("    {:<4}  {}", "默认", get(&Config::default())));
    if !desc.is_empty() { crate::emit_line(&format!("    {:<4}  {}", "说明", desc)); }
    let example_val = match kind { Kind::Int => "10", Kind::Bool => "false", Kind::Str => "<值>" };
    crate::emit_line(&format!("    {:<4}  fqdt config set {dotted} {example_val}", "设置"));
    crate::emit_line(&format!("    {:<4}  fqdt config unset {dotted}（恢复默认）", "恢复"));
}

// ===== 登录 cookie 存取（v0.17.0） =====

/// cookie 存取走统一 store.json（此处只做校验与规范化）。
pub fn load_cookie() -> Option<String> { crate::store::cookie_get() }

pub fn save_cookie(raw: &str) -> Result<(), String> {
    let v = raw.trim();
    if v.is_empty() { return Err("Cookie 为空".into()); }
    if v.len() > 12 * 1024 || !v.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err("Cookie 只能含可打印 ASCII 且不超过 12KB".into());
    }
    // 规范化：浏览器复制出的 `key = value`（等号带空格）会骗过 session 检测，存盘前压掉
    let norm = v.split(';').map(str::trim).filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, val)) => format!("{}={}", k.trim(), val.trim()),
            None => p.to_string(),
        })
        .collect::<Vec<_>>().join("; ");
    crate::store::cookie_set(norm)
}

pub fn clear_cookie() -> Result<(), String> { crate::store::cookie_clear() }

/// cookie 是否含有效 session 键（与 core 校验同规则）。
pub fn cookie_has_session(c: &str) -> bool {
    c.split(';').any(|part| {
        part.trim().split_once('=').is_some_and(|(k, v)| {
            matches!(k.trim(), "sessionid" | "sessionid_ss" | "sid_tt") && !v.trim().is_empty()
        })
    })
}

/// `fqdt config --list` | `<key>` | `<key> <value>` | `--unset <key>`（git config 式）。
/// `fqdt config get <key>|--all`：单键详情 / -a 列全部。
pub fn run_get(key: Option<String>, all: bool) {
    if all { return list_all(); }
    let Some(k) = key else {
        return eprintln!("  err 用法: fqdt config get <key> | fqdt config get --all");
    };
    if !key_help(&k) { eprintln!("  err 未知键: {k}（get --all 查看全部）"); }
}

/// `fqdt config set <key> <value>`。
pub fn run_set(key: String, value: String) { set_key(&key, &value); }

/// `fqdt config unset <key>`（恢复默认值）。
pub fn run_unset(key: String) { unset_key(&key); }

/// `fqdt config init [--force]`：生成默认配置文件；--force 覆盖旧模板（键说明升级用）。
pub fn run_init(force: bool) {
    let existed = get_home().join(".config/fqdt/config.ini").exists();
    match Config::save_default(force) {
        Ok(()) if existed && !force => {
            crate::emit_line(&format!("  {DIM}已存在，未覆盖；fqdt config init --force 用新模板（含键说明）重建{RST}"));
        }
        Ok(()) => crate::emit_line(&format!("{GREEN}ok{RST} ~/.config/fqdt/config.ini")),
        Err(e) => eprintln!("  {RED}err{RST} {e}"),
    }
}

/// 列全部键值（get --all / 旧 --list）。
fn list_all() {
    for (dotted, _, _, _, get) in KEYS { crate::emit_line(&format!("{}={}", dotted, get(&Config::load()))); }
}

/// 旧式 git 用法兼容入口（`config <key> [value]` / --list / --unset / init）——help 不再宣传。
pub fn run_cmd(key: Option<String>, value: Option<String>, list: bool, unset: bool, force: bool) {
    if list { return list_all(); }
    let Some(k) = key else {
        eprintln!("  err 用法: fqdt config get <key>|--all | set <key> <value> | unset <key> | init [--force]");
        return;
    };
    if k == "init" && value.is_none() && !unset { return run_init(force); }
    if unset { return unset_key(&k); }
    match value {
        Some(v) => set_key(&k, &v),
        None => {
            if !key_help(&k) { eprintln!("  err 未知键: {k}（get --all 查看全部）"); }
        }
    }
}

/// 校验并写入单键；成功打 `  ok` 收据。
fn set_key(dotted: &str, val: &str) {
    let Some(e) = KEYS.iter().find(|e| e.0 == dotted) else {
        return eprintln!("  err 未知键: {dotted}（--list 查看全部）");
    };
    let bad = match e.3 {
        Kind::Int => val.parse::<u64>().is_err(),
        Kind::Bool => val != "true" && val != "false",
        Kind::Str => false,
    };
    if bad { return eprintln!("  err 值类型不符: {dotted} 需 {}", match e.3 { Kind::Int => "整数", Kind::Bool => "true/false", Kind::Str => "字符串" }); }
    let path = get_home().join(".config/fqdt/config.ini");
    if let Err(err) = ini_write(&path, e.1, e.2, val) { return eprintln!("  err {err}"); }
    crate::emit_line(&format!("{GREEN}ok{RST} {dotted} = {val}"));
}

/// 删除键行（恢复代码默认值）。
fn unset_key(dotted: &str) {
    let Some(e) = KEYS.iter().find(|e| e.0 == dotted) else {
        return eprintln!("  err 未知键: {dotted}（--list 查看全部）");
    };
    let path = get_home().join(".config/fqdt/config.ini");
    let Ok(text) = fs::read_to_string(&path) else { return eprintln!("  err 配置未设置过: {dotted}"); };
    let kept: Vec<&str> = text.lines().filter(|l| {
        l.trim().split_once('=').map(|(k, _)| k.trim() != e.1).unwrap_or(true)
    }).collect();
    if kept.len() == text.lines().count() { return eprintln!("  err 配置未设置过: {dotted}"); }
    let mut out = kept.join("\n"); out.push('\n');
    if let Err(err) = fs::write(&path, out) { return eprintln!("  err {err}"); }
    crate::emit_line(&format!("{GREEN}ok{RST} {dotted} 已删除（默认 {}）", e.4(&Config::default())));
}

/// ini 写回：已有键原位替换；否则插入对应节尾（节不存在则新建）。保留原有注释与其他键。
fn ini_write(path: &std::path::Path, ini_key: &str, section: &str, val: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut done = false;
    for l in &mut lines {
        if let Some((k, _)) = l.trim().split_once('=') {
            if k.trim() == ini_key { *l = format!("{ini_key} = {val}"); done = true; break; }
        }
    }
    if !done {
        let sec = format!("[{section}]");
        let pos = lines.iter().position(|l| l.trim() == sec);
        let ins = match pos {
            Some(i) => lines[i + 1..].iter().rposition(|l| !l.trim().is_empty())
                .map(|r| i + 1 + r + 1).unwrap_or(i + 1),
            None => { lines.push(String::new()); lines.push(sec); lines.len() - 1 }
        };
        lines.insert(ins, format!("{ini_key} = {val}"));
    }
    let mut out = lines.join("\n"); out.push('\n');
    fs::write(path, out).map_err(|e| e.to_string())
}

pub fn load_bookmarks() -> Vec<(String, String)> { crate::store::bookmarks() }

pub fn add_bookmark(id: &str, title: &str) -> Result<(), String> { crate::store::bookmark_add(id, title) }

/// 按 1-based 编号移除（shelf --del 语义）。
pub fn remove_bookmark(idx: usize) -> Result<(), String> {
    if idx == 0 { return Err("无效编号".into()); }
    crate::store::bookmark_remove(idx - 1).map(|_| ()).ok_or_else(|| "无效编号".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_and_page_mode() {
        let c = Config::default();
        assert_eq!(c.ui_view_mode, "normal");
        assert_eq!(c.ui_page_mode, "right-left");
    }
}
