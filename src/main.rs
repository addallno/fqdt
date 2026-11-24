mod api;
mod audio;
mod config;
mod download;
mod epub;
mod fqapi;
mod helpfile;
mod store;
mod types;
mod util;
mod workflow;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use types::Config;
use crate::util::{CYAN, DIM, GREEN, RST, TITLE, YELLOW};
use std::path::Path;

#[derive(Parser)]
#[command(name = "fqdt", version, about = "番茄小说下载器", subcommand_help_heading = "Subcommands")]
struct Cli {
    // 仅顶层出现（不进子命令 help）；须置于子命令前：fqdt --catalog-url X search …
    #[arg(long, help = "搜索 API 源")]
    search_url: Option<String>,
    #[arg(long, help = "目录 API 源")]
    catalog_url: Option<String>,
    #[arg(long, help = "内容 API 源")]
    content_url: Option<String>,
    #[arg(long, global = true, help = "请求超时秒")]
    timeout: Option<u64>,
    #[arg(short = 'v', long, global = true, help = "详细日志")]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 搜索小说
    #[command(display_order = 10)]
    Search {
        #[arg(help = "搜索关键词")]
        keyword: String,
        #[arg(short='p', long, default_value="1", help = "页码")]
        page: usize,
        #[arg(short='D', long, help = "自动下载第 N 本结果")]
        auto: Option<usize>,
        #[arg(long, help = "仅展示搜索结果")]
        dry_run: bool,
        #[arg(short='o', long, help = "输出目录（仅 -D）")]
        output: Option<String>,
        #[arg(short='j', long, help = "并发数（仅 -D）")]
        jobs: Option<usize>,
        #[arg(short='r', long, help = "章节范围 1-50 / -5 / 10-（仅 -D）")]
        range: Option<String>,
        #[arg(short='t', long, help = "输出格式: txt / epub（仅 -D）")]
        format: Option<String>,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒（仅 -D）")]
        interval: u64,
    },
    /// 查看书籍信息
    #[command(alias = "i")]
    #[command(display_order = 11)]
    #[command(subcommand_help_heading = "Subcommands")]
    Info {
        #[command(subcommand)]
        cmd: InfoCmd,
    },
    /// 下载正文
    #[command(alias = "d")]
    #[command(display_order = 20)]
    Download {
        #[arg(value_name = "id|path", help = "id、书名关键词或已下载目录")]
        book_id: String,
        #[arg(short = 'o', long, help = "输出目录")]
        output: Option<String>,
        #[arg(short='r', long, help = "章节范围 1-50 / -5 / 10-")]
        range: Option<String>,
        #[arg(short='t', long, help = "输出格式: txt / epub")]
        format: Option<String>,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
        #[arg(short='f', long, help = "强制覆盖已存在文件")]
        force: bool,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒")]
        interval: u64,
    },
    /// 下载语音
    #[command(alias = "a")]
    #[command(display_order = 21)]
    Audio {
        #[arg(value_name = "id", help = "id 或书名关键词")]
        book_id: String,
        #[arg(short='o', long, help = "输出目录")]
        output: Option<String>,
        #[arg(short='r', long, help = "章节范围 1-50 / -5 / 10-")]
        range: Option<String>,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
        #[arg(short='f', long, help = "强制覆盖已存在文件")]
        force: bool,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒")]
        interval: u64,
        #[arg(long, help = "歌词模式 external/embed/both/off")]
        lrc: Option<String>,
        #[arg(long, help = "真人讲书版", conflicts_with_all = ["ai", "tts"])]
        real: bool,
        // --tone/--voice 为 --ai/--tts 的子项：help 单行，Options 中缩进挂父下（SUBITEMS）
        #[arg(long, conflicts_with = "tts", help = "智能朗读")]
        ai: bool,
        #[arg(long, hide = true, default_value = "1", help = "AI 音色编号，fqdt info audio <id> 查可用")]
        tone: usize,
        #[arg(long, help = "Edge-TTS 本地合成")]
        tts: bool,
        #[arg(long, hide = true, default_value = "zh-CN-XiaoxiaoNeural", help = "语音名")]
        voice: String,
        #[arg(long, conflicts_with = "mp3", help = "保持 m4a 原样不转码")]
        keep_m4a: bool,
        #[arg(long, conflicts_with = "keep_m4a", help = "转码 MP3 而非默认 Ogg Opus")]
        mp3: bool,
        #[arg(long, help = "新下载 MP3 嵌入书籍封面")]
        cover: bool,
    },
    /// 增量更新已下载的书籍
    #[command(alias = "u")]
    #[command(display_order = 22)]
    Update {
        #[arg(value_name = "id|path", help = "id、书名关键词或已下载目录")]
        target: String,
        #[arg(short='o', long, help = "输出目录")]
        output: Option<String>,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
        #[arg(short='f', long, help = "强制覆盖已存在文件")]
        force: bool,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒")]
        interval: u64,
    },
    /// 文本转语音
    #[command(alias = "t")]
    #[command(display_order = 31)]
    Tts {
        path: String,
        #[arg(long, default_value = "zh-CN-XiaoxiaoNeural", help = "Edge-TTS 语音名")]
        voice: String,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
    },
    /// 书架管理
    #[command(alias = "s")]
    #[command(display_order = 30)]
    #[command(subcommand_help_heading = "Subcommands")]
    Shelf {
        #[command(subcommand)]
        cmd: Option<ShelfCmd>,
    },
    /// 登录态管理
    #[command(display_order = 32)]
    Login {
        /// Cookie 字符串
        cookie: Option<String>,
        /// 用当前 Cookie 请求书架验证登录态
        #[arg(long, help = "验证登录态")]
        check: bool,
        /// 清除已保存的 Cookie
        #[arg(long, help = "清除 Cookie")]
        clear: bool,
    },
    /// 查看/修改配置
    #[command(display_order = 33)]
    Config {
        /// 键
        key: Option<String>,
        /// 新值
        value: Option<String>,
        #[arg(long, help = "列出全部键值")]
        list: bool,
        #[arg(long, help = "删除该键")]
        unset: bool,
        #[arg(long, help = "config init 时覆盖已存在文件（升级键说明模板）")]
        force: bool,
    },
    /// 透传调用 fqapi 端点
    #[command(display_order = 41)]
    Api {
        /// 端点路径
        path: Option<String>,
        /// HTTP 方法
        #[arg(short = 'X', long, default_value = "GET", help = "HTTP 方法")]
        method: String,
        /// 追加查询参数 k=v
        #[arg(short = 'q', long = "query", value_name = "K=V", help = "查询参数")]
        query: Vec<String>,
        /// 请求体
        #[arg(short = 'd', long = "data", help = "请求体")]
        data: Option<String>,
        /// 简易 JSON 提取
        #[arg(long = "jq", help = "JSON 字段提取路径")]
        jq: Option<String>,
        /// stderr 打印 HTTP 状态码
        #[arg(long = "status", help = "stderr 打印状态码")]
        status: bool,
        /// 列出常用端点
        #[arg(long = "list", help = "列出常用端点")]
        list: bool,
    },
    /// 阅读书籍
    #[command(display_order = 34)]
    Read {
        #[arg(value_name = "id", help = "id 或书名关键词")]
        book_id: String,
        #[arg(short = 'r', long, help = "章节范围 1-50 / -5 / 10-")]
        range: Option<String>,
    },
    /// 对外 HTTP 服务
    #[command(display_order = 40)]
    Serve {
        #[arg(long, default_value = "18921", help = "监听端口")]
        port: u16,
        #[arg(long, help = "监听局域网地址")]
        lan: bool,
    },
    /// 显示版本号
    #[command(display_order = 90)]
    Version,
    /// 自定义命令
    #[command(external_subcommand)]
    Custom(Vec<String>),
}

/// `fqdt info` 子命令：书籍 section + 发现流 + 个人数据。
/// 原位置参数 section 改为真子命令，flag 按子命令归属（通用 flag 仅 --timeout/-v）。
#[derive(Subcommand)]
enum InfoCmd {
    /// 概览：标题/热度/简介 + 前 N 章 + 前 N 条书评
    Overview {
        /// id 或书名关键词
        #[arg(value_name = "id")]
        book_id: String,
        #[arg(short = 'r', long, help = "章节范围")]
        range: Option<String>,
        #[arg(short = 's', long, help = "显示正文预览")]
        show: bool,
        #[arg(long, help = "加入云端书架")]
        add: bool,
    },
    /// 目录（含 item_id）
    Chapters {
        #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String,
        #[arg(short = 'r', long, help = "章节范围")]
        range: Option<String>,
        #[arg(long, help = "加入云端书架")]
        add: bool,
    },
    /// 完整简介
    Intro { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 作者资料与书单
    Author { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 书评
    Comments { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 封面图 URL
    Cover { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 相关推荐
    Relative { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 本书可用 AI 音色
    Audio { #[arg(value_name = "id", help = "id 或书名关键词")] book_id: String, #[arg(long, help = "加入云端书架")] add: bool },
    /// 首页推荐
    Recommend {
        #[arg(long, default_value = "2", help = "区块类型")]
        tab_type: u32,
        #[arg(long, default_value = "0", help = "区块索引")]
        tab_index: u32,
        #[arg(long, default_value = "0", help = "偏移")]
        offset: u32,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
    },
    /// 分类列表
    Category {
        #[arg(long, default_value = "1", help = "1 男频 / 2 女频 / 3 出版 / 4 听书")]
        tab: u32,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
    },
    /// 云端文件夹
    Folders { #[arg(long, value_name = "KEYWORD", help = "关键词过滤")] search: Option<String>, #[arg(long, help = "输出原始 JSON")] json: bool },
    /// 阅读历史
    History { #[arg(long, value_name = "KEYWORD", help = "关键词过滤")] search: Option<String>, #[arg(long, help = "输出原始 JSON")] json: bool },
    /// 阅读进度
    Progress { #[arg(long, value_name = "KEYWORD", help = "关键词过滤")] search: Option<String>, #[arg(long, help = "输出原始 JSON")] json: bool },
    /// 账号信息
    User { #[arg(long, help = "输出原始 JSON")] json: bool },
    /// 划线笔记
    Highlights { #[arg(long, help = "输出原始 JSON")] json: bool },
}

/// `fqdt shelf` 子命令（42②：裸 shelf 报错要求子命令，list 为书架入口）。
/// 预留：查看类可加 `--dump` flag 打印原始结构（42④，暂未实现）。
#[derive(Subcommand)]
enum ShelfCmd {
    /// 书架列表：未登录显示本地，已登录显示云端，--local 强制本地
    List {
        /// 组名：指定则显示该组内书目，无参显示第一层（未分组书 + 分组条目）
        group: Option<String>,
        #[arg(long, help = "显示本地书架")]
        local: bool,
        #[arg(long, value_name = "KEYWORD", help = "关键词过滤")]
        search: Option<String>,
        #[arg(long, help = "逐本拉取作者")]
        author: bool,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
    },
    /// 加书：本地 <id>:<标题> 含冒号，云端纯 <id>
    Add {
        #[arg(help = "<id>:<标题> 本地 · <id> 云端")]
        book: String,
    },
    /// 从云端书架移出
    Remove {
        #[arg(value_name = "id", help = "id")]
        book_id: String,
    },
    /// 移动云端书到分组，组不存在时自动建组
    Move {
        #[arg(value_name = "id", help = "id")]
        book_id: String,
        #[arg(help = "目标分组名")]
        group: String,
    },
    /// 检查是否在云端书架
    Check {
        #[arg(value_name = "id", help = "id")]
        book_id: String,
    },
    /// 删除本地书架条目
    Delete {
        #[arg(help = "条目编号")]
        index: usize,
    },
    /// 下载本地书架某本
    Download {
        #[arg(help = "条目编号")]
        index: usize,
    },
    /// 逐本更新本地书架
    Update,
}

// ===== 逐 flag 详情帮助（v0.17.0） =====

/// 详情字段标签统一用 2 字中文，渲染时按字符宽对齐。
type Field = (&'static str, &'static str);

/// 逐 flag 详情：(子命令主名, 规范长 flag, 详情)。短名经 SHORTS 归一匹配。
struct FlagDoc { brief: &'static str, takes_value: bool, fields: &'static [Field] }

/// 长 flag → 短名（匹配 `-r` 与 `--range` 等价）。
const SHORTS: &[(&str, &str)] = &[
    ("--range", "-r"), ("--format", "-t"), ("--jobs", "-j"), ("--force", "-f"),
    ("--interval", "-i"), ("--auto", "-D"), ("--page", "-p"), ("--show", "-s"),
];

const FLAG_HELP: &[(&str, &str, FlagDoc)] = &[
    ("audio", "--tone", FlagDoc { brief: "AI 音色编号", takes_value: true, fields: &[
        ("类型", "整数"),
        ("取值", "默认 1；常见 1 甜美少女 · 2 清亮青叔 · 4 成熟大叔 · 5 开朗青年 · 6 温柔淑女 · 74 大叔升级 · 51 多人发音"),
        ("说明", "文字版 AI 朗读音色；各书可用集合不同"),
        ("查询", "fqdt info audio <id> 查看本书可用编号"),
        ("配置", "config.ini [audio] audio_tone"),
        ("示例", "fqdt audio <id> --tone 6"),
        ("相关", "--ai --real --tts"),
    ]}),
    ("audio", "--lrc", FlagDoc { brief: "歌词模式", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "external / embed / both / off，默认 external"),
        ("说明", "external=同名 .lrc 文件；embed=嵌入 MP3 ID3 USLT；both=两者；off=不生成"),
        ("限制", "仅音频下载路径生效；MP4 容器嵌入无效故默认 external"),
        ("示例", "fqdt audio <id> --lrc embed"),
        ("相关", "--tts"),
    ]}),
    ("audio", "--real", FlagDoc { brief: "真人讲书版", takes_value: false, fields: &[
        ("说明", "自动发现最热真人听书版"),
        ("互斥", "--ai --tts"),
        ("示例", "fqdt audio <id> --real"),
        ("查询", "fqdt info relative <id> 查看该书真人版"),
    ]}),
    ("audio", "--ai", FlagDoc { brief: "智能朗读", takes_value: false, fields: &[
        ("说明", "使用文字版 AI 音色，配合 --tone 选编号；related_novel_bookid 自动转回文字版"),
        ("互斥", "--tts"),
        ("示例", "fqdt audio <id> --ai --tone 6"),
        ("相关", "--tone --real"),
    ]}),
    ("audio", "--tts", FlagDoc { brief: "Edge-TTS 本地合成", takes_value: false, fields: &[
        ("说明", "先下载正文 txt 再本地合成 MP3；需安装 edge-tts"),
        ("互斥", "--real --ai"),
        ("示例", "fqdt audio <id> --tts --voice zh-CN-YunxiNeural"),
        ("相关", "--voice"),
    ]}),
    ("audio", "--voice", FlagDoc { brief: "Edge-TTS 语音名", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("默认", "zh-CN-XiaoxiaoNeural"),
        ("说明", "仅 --tts 时生效"),
        ("示例", "fqdt audio <id> --tts --voice zh-CN-YunxiNeural"),
        ("相关", "--tts"),
    ]}),
    ("read", "--range", FlagDoc { brief: "章节范围", takes_value: true, fields: &[
        ("取值", "1-50 区间 / -5 前 5 章 / 10- 自 10 起到末尾"),
        ("说明", "缺省读全本；进度按章记忆，重入自动回位"),
    ]}),
    ("download", "--range", FlagDoc { brief: "章节范围", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "N 单章 / N-M 区间 / -M 末 M 章 / N- 自 N 起"),
        ("说明", "按目录序筛选；缺省全部章节"),
        ("示例", "fqdt download <id> -r 1-50   fqdt download <id> -r -5"),
        ("相关", "-t -j"),
    ]}),
    ("download", "--format", FlagDoc { brief: "输出格式", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "txt/ epub"),
        ("配置", "config.ini [download] format"),
        ("示例", "fqdt download <id> -t epub"),
    ]}),
    ("download", "--jobs", FlagDoc { brief: "并发数", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "按 CPU 核数"),
        ("配置", "config.ini [download] concurrent"),
        ("示例", "fqdt download <id> -j 4"),
    ]}),
    ("download", "--force", FlagDoc { brief: "强制覆盖", takes_value: false, fields: &[
        ("说明", "已存在文件重新下载；缺省跳过已下载章节"),
        ("示例", "fqdt download <id> --force"),
    ]}),
    ("download", "--interval", FlagDoc { brief: "请求间隔毫秒", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "0"),
        ("说明", "每章请求之间的等待，限速用"),
        ("示例", "fqdt download <id> -i 500"),
    ]}),
    ("search", "--auto", FlagDoc { brief: "自动下载第 N 本", takes_value: true, fields: &[
        ("类型", "整数"),
        ("说明", "跳过交互选择，直接下载第 N 条结果；配合 -o/-r/-t"),
        ("示例", "fqdt search 诡秘 -D 1"),
        ("相关", "-o -r -t"),
    ]}),
    ("search", "--page", FlagDoc { brief: "页码", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "1"),
        ("示例", "fqdt search 诡秘 -p 2"),
    ]}),
    ("search", "--range", FlagDoc { brief: "章节范围", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "同 download：N / N-M / -M / N-"),
        ("说明", "仅 -D 自动下载时生效"),
        ("示例", "fqdt search 诡秘 -D 1 -r 1-50"),
    ]}),
    ("search", "--format", FlagDoc { brief: "输出格式", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "txt/ epub"),
        ("说明", "仅 -D 自动下载时生效"),
        ("示例", "fqdt search 诡秘 -D 1 -t epub"),
    ]}),
    ("search", "--jobs", FlagDoc { brief: "并发数", takes_value: true, fields: &[
        ("类型", "整数"), ("默认", "按 CPU 核数"), ("说明", "仅 -D 自动下载时生效"),
    ]}),
    ("search", "--interval", FlagDoc { brief: "请求间隔毫秒", takes_value: true, fields: &[
        ("类型", "整数"), ("默认", "0"), ("说明", "仅 -D 自动下载时生效"),
    ]}),
    ("info", "--range", FlagDoc { brief: "章节范围", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "同 download：N / N-M / -M / N-"),
        ("限制", "仅 chapters 与概览生效"),
        ("示例", "fqdt info chapters <id> -r 1-50"),
    ]}),
    ("info", "--show", FlagDoc { brief: "正文预览", takes_value: false, fields: &[
        ("说明", "概览前列章节附带正文"),
        ("示例", "fqdt info overview <id> --show"),
    ]}),
];

/// 子命令别名 → 主名。
fn canonical_sub(s: &str) -> &str {
    match s {
        "a" => "audio", "d" => "download", "i" => "info", "s" => "shelf",
        "u" => "update", "t" => "tts", other => other,
    }
}

/// 扫描参数找 (子命令, flag) 详情场景；未命中返回 None 交 clap。
/// 场景 a: `<cmd> <flag> --help`；场景 b: `help <cmd> <flag>`（flag 非 --help 本身）。
fn scan_flag(args: &[String]) -> Option<(String, String)> {
    if args.first().map(|s| s == "help").unwrap_or(false)
        && args.len() >= 3 && args[2].starts_with('-')
        && args[2] != "--help" && args[2] != "-h"
    {
        return Some((args[1].clone(), args[2].clone()));
    }
    let i = args.iter().position(|a| a == "--help" || a == "-h")?;
    if i == 0 { return None; }
    let flag = &args[i - 1];
    if !flag.starts_with('-') || flag == "--help" || flag == "-h" { return None; }
    let sub = if i >= 2 && !args[i - 2].starts_with('-') && args[i - 2] != "help" {
        args[i - 2].clone()
    } else {
        args.iter().find(|a| !a.starts_with('-'))?.clone()
    };
    Some((sub, flag.clone()))
}

/// 查 flag 详情表（主名 + 长短名归一）。
fn find_flag_doc(sub: &str, flag: &str) -> Option<&'static FlagDoc> {
    let sub = canonical_sub(sub);
    FLAG_HELP.iter().find(|e| e.0 == sub && {
        e.1 == flag || SHORTS.iter().any(|(long, short)| *long == e.1 && *short == flag)
    }).map(|e| &e.2)
}

/// 格式化渲染单条 flag 详情（标签 2 字对齐）。
fn render_flag(sub: &str, flag: &str, d: &FlagDoc) {
    let name = flag.trim_start_matches('-').to_uppercase();
    let usage = if d.takes_value { format!("{flag} <{name}>") } else { flag.to_string() };
    crate::emit_line(&format!("  {CYAN}{usage}{RST}  {YELLOW}{}{RST}", d.brief));
    crate::emit_line(&format!("  {DIM}子命令 fqdt {sub} · 优先级 flag > config{RST}"));
    crate::emit_line("");
    for (k, v) in d.fields {
        crate::emit_line(&format!("    {k:<4}  {v}"));
    }
}

/// 逐 flag / config 键详情拦截：命中则渲染并退出；未交 clap。
fn try_flag_help() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((sub, flag)) = scan_flag(&args) {
        if let Some(doc) = find_flag_doc(&sub, &flag) {
            render_flag(&sub, &flag, doc);
            std::process::exit(0);
        }
    }
    if let Some(key) = scan_config_key(&args) {
        if crate::config::key_help(&key) { std::process::exit(0); }
    }
}

/// config 键详情：`fqdt config <key> --help`（尾段模糊匹配；多命中列候选）。
fn scan_config_key(args: &[String]) -> Option<String> {
    if args.first().map(|s| s != "config").unwrap_or(true) { return None; }
    if !args.iter().any(|a| a == "--help" || a == "-h") { return None; }
    args.iter().skip(1).find(|a| !a.starts_with('-')).cloned()
}


// ===== 子命令 help 自定义渲染（子项缩进，v0.26.1） =====

/// 子项归属：(父 flag 长名, 子 flag 长名列表)。子项在 Options 中缩进 +2 挂父下，不独立成行。
pub(crate) const SUBITEMS: &[(&str, &[&str])] = &[("ai", &["tone"]), ("tts", &["voice"])];

/// 子命令 help 请求：`<path...> --help|-h` 或 `help <path...>`；返回子命令路径。
/// 路径中含非子命令 token（如未知 flag）时不拦截，交 clap / flag 详情。
fn sub_help_requested(args: &[String]) -> Option<Vec<String>> {
    let toks: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let path: &[&str] = if toks.first() == Some(&"help") {
        &toks[1..]
    } else {
        match toks.last() {
            Some(&"--help") | Some(&"-h") => &toks[..toks.len() - 1],
            _ => return None,
        }
    };
    if path.is_empty() { return None; }
    let mut cur = Cli::command();
    for t in path {
        let next = cur.find_subcommand(t)?.clone();
        cur = next;
    }
    Some(path.iter().map(|s| s.to_string()).collect())
}

/// flag 右列说明：help 首行 + `[default: ...]` 后缀；-h/-V 给中文文案。
fn flag_help_text(a: &clap::Arg) -> String {
    match a.get_long() {
        Some("help") => return "查看帮助".to_string(),
        Some("version") => return "显示版本".to_string(),
        _ => {}
    }
    let help = a.get_help().or_else(|| a.get_long_help()).map(|h| h.to_string()).unwrap_or_default();
    let first = help.lines().next().unwrap_or("").trim().to_string();
    match a.get_default_values().first() {
        Some(d) if !first.is_empty() => format!("{} [default: {}]", first, d.to_string_lossy()),
        Some(d) => format!("[default: {}]", d.to_string_lossy()),
        None => first,
    }
}

/// Options 行集：(左列, 说明)。子项缩进 +2 紧跟父 flag，隐藏 flag 除非是子项否则跳过。
fn option_rows(cmd: &clap::Command) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for a in cmd.get_arguments() {
        if a.is_positional() { continue; }
        let long = a.get_long().unwrap_or("");
        let is_sub = SUBITEMS.iter().any(|(_, ch)| ch.contains(&long));
        if is_sub { continue; }
        if a.is_hide_set() { continue; }
        rows.push((crate::helpfile::arg_left(a), flag_help_text(a)));
        for (parent, children) in SUBITEMS {
            if *parent != long { continue; }
            for c in cmd.get_arguments() {
                if let Some(cl) = c.get_long() {
                    if children.contains(&cl) {
                        rows.push((format!("  {}", crate::helpfile::arg_left(c)), flag_help_text(c)));
                    }
                }
            }
        }
    }
    rows
}

/// 两列对齐输出行集：左列最大宽 + 2 为说明列。
fn emit_rows(rows: &[(String, String)]) {
    if rows.is_empty() { return; }
    let width = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0) + 2;
    for (left, help) in rows {
        crate::emit_line(&format!("{left:<width$}{help}"));
    }
}

/// Options 段（子项缩进挂父下）+ 顶层 global flag（--timeout/-v 等）。
fn render_options(cmd: &clap::Command) {
    let mut rows = option_rows(cmd);
    for a in Cli::command().get_arguments() {
        let Some(long) = a.get_long() else { continue };
        if !a.is_global_set() { continue; }
        if rows.iter().any(|(l, _)| l.contains(&format!("--{long}"))) { continue; }
        rows.push((crate::helpfile::arg_left(a), flag_help_text(a)));
    }
    if rows.iter().all(|(l, _)| !l.contains("--help")) {
        rows.push(("  -h, --help ".to_string(), "查看帮助".to_string()));
    }
    crate::emit_line(&format!("{TITLE}Options:{RST}"));
    emit_rows(&rows);
}

/// Arguments 段：位置参数两列对齐。
fn render_pos_args(cmd: &clap::Command) {
    let rows: Vec<(String, String)> = cmd.get_positionals().filter(|a| !a.is_hide_set()).map(|a| {
        let name = a.get_value_names().and_then(|v| v.first())
            .map(|s| s.to_string()).unwrap_or_else(|| a.get_id().to_string());
        (format!("  <{name}> "), flag_help_text(a))
    }).collect();
    if rows.is_empty() { return; }
    crate::emit_line(&format!("{TITLE}Arguments:{RST}"));
    emit_rows(&rows);
}

/// Commands 段：子命令名 + 一句话目的（跳过 clap 自动 help 子命令）。
fn render_commands(cmd: &clap::Command) {
    let subs: Vec<&clap::Command> = cmd.get_subcommands()
        .filter(|s| s.get_name() != "help" && !s.is_hide_set()).collect();
    if subs.is_empty() { return; }
    crate::emit_line(&format!("{TITLE}Subcommands:{RST}"));
    let width = subs.iter().map(|s| s.get_name().chars().count()).max().unwrap_or(0) + 2;
    for s in subs {
        let about = s.get_about().map(|a| a.to_string()).unwrap_or_default();
        let first = about.lines().next().unwrap_or("").trim();
        crate::emit_line(&format!("  {:<width$}{}", s.get_name(), first));
    }
}

/// 自定义子命令 help：about / Usage / Arguments / Subcommands / Options（子项缩进）/ 尾部。
fn render_sub_help(path: &[String], examples: bool) {
    let mut cmd = Cli::command();
    for t in path { cmd = cmd.find_subcommand(t).expect("path validated").clone(); }
    let about = cmd.get_about().map(|a| a.to_string()).unwrap_or_default();
    crate::emit_line(about.lines().next().unwrap_or("").trim());
    crate::emit_line("");
    let usage = crate::helpfile::usage_of(&cmd, &path.join(" "));
    crate::emit_line(&format!("{TITLE}Usage:{RST} {usage}"));
    crate::emit_line("");
    let pos: Vec<&clap::Arg> = cmd.get_positionals().filter(|a| !a.is_hide_set()).collect();
    if !pos.is_empty() { render_pos_args(&cmd); crate::emit_line(""); }
    if cmd.has_subcommands() { render_commands(&cmd); crate::emit_line(""); }
    render_options(&cmd);
    let tail = help_tail_text(path.last().map(|s| s.as_str()).unwrap_or(""), examples);
    if !tail.is_empty() {
        crate::emit_line("");
        for l in tail.lines() { crate::emit_line(l); }
    }
}

// ===== 顶层分组 help（v0.24.0，49④） =====

/// 顶层分组：(组名, 子命令)。clap 模板无分组能力，手写渲染（FLAG_HELP 先例）。
const TOP_GROUPS: &[(&str, &[&str])] = &[
    ("查询", &["search", "info"]),
    ("下载", &["download", "audio", "update"]),
    ("管理", &["shelf", "read", "tts", "login", "config"]),
    ("服务", &["serve", "api"]),
    ("其他", &["version", "help"]),
];

/// 顶层选项段（硬编码；子命令 about 从 Cli::command() 内省，避免双份维护）。
const TOP_OPTIONS: &[(&str, &str)] = &[
    ("      --search-url <SEARCH_URL>", "搜索 API 源"),
    ("      --catalog-url <CATALOG_URL>", "目录 API 源"),
    ("      --content-url <CONTENT_URL>", "内容 API 源"),
    ("      --timeout <TIMEOUT>", "请求超时秒"),
    ("  -v, --verbose", "详细日志"),
    ("  -h, --help", "查看帮助"),
    ("  -V, --version", "显示版本"),
];

/// 顶层 help 请求判定：裸调用 / --help / -h / help，且尚未出现子命令。
/// `fqdt search --help`、`fqdt help search` 等仍交 clap。
fn top_help_requested(args: &[String]) -> bool {
    if args.is_empty() { return true; }
    if args.len() == 1 && args[0] == "help" { return true; }
    let mut saw_sub = false;
    for a in args {
        if a == "--help" || a == "-h" { return !saw_sub; }
        if !a.starts_with('-') { saw_sub = true; }
    }
    false
}

/// 无样式的根命令，供 help.txt 生成（纯文本）。
pub(crate) fn cli_command() -> clap::Command { Cli::command() }

/// clap help 段落标题（Usage/Arguments/Options/Commands）统一加粗 + 下划线。
fn with_help_styles(cmd: clap::Command) -> clap::Command {
    use clap::builder::styling::{Effects, Style, Styles};
    let hdr = Style::new().effects(Effects::BOLD | Effects::UNDERLINE);
    let style = Styles::default().header(hdr).usage(hdr);
    cmd.styles(style).mut_subcommands(with_help_styles)
}

/// 手写顶层 help：子命令按分类顺序平铺（不显示分组标题）+ Options（优先级尾行 m0117 移除）。
fn render_top_help() {
    let cmd = Cli::command();
    crate::emit_line(&format!("{CYAN}fqdt{RST} — 番茄小说下载器"));
    crate::emit_line("");
    crate::emit_line(&format!("  {DIM}用法: fqdt [OPTIONS] <COMMAND>{RST}"));
    crate::emit_line("");
    crate::emit_line(&format!("{TITLE}Subcommands:{RST}"));
    // 分类只决定顺序，不再打印组名（用户：按顺序列出即可，看得出用途）
    for (_, subs) in TOP_GROUPS {
        for s in *subs {
            crate::emit_line(&format!("  {s:<9} {}", sub_about(&cmd, s)));
        }
    }
    crate::emit_line("");
    crate::emit_line(&format!("{TITLE}Options:{RST}"));
    for (flag, about) in TOP_OPTIONS {
        crate::emit_line(&format!("  {flag}  {about}"));
    }
}

/// 子命令 about 内省（取首行）；clap 自动 help 子命令 build 前取不到，兜底中文。
fn sub_about(cmd: &clap::Command, name: &str) -> String {
    let about = cmd.find_subcommand(name).and_then(|c| c.get_about())
        .map(|a| a.to_string()).unwrap_or_default();
    let first = about.lines().next().unwrap_or("").to_string();
    if first.is_empty() && name == "help" { "查看帮助".into() } else { first }
}

/// help 底部附加段：(子命令名, 附加说明, 示例)。示例受 config `ui.help_examples` 控制。
/// 附加说明不写"flag 详情/配置键"类内容（用户明确：帮助里不教用 help、不列配置键）。
const HELP_TAILS: &[(&str, &str, &str)] = &[
    ("search", "", "Examples:\n  fqdt search 诡秘之主\n  fqdt search 诡秘 -D 1"),
    ("info", "",
     "Examples:\n  fqdt info overview <id>\n  fqdt info chapters <id> -r 1-50\n  fqdt info overview <id> --add\n  fqdt info recommend\n  fqdt info category --tab 1\n  fqdt info progress --search 词\n  fqdt info overview <id>"),
    ("download", "", "Examples:\n  fqdt download <id> -r 1-50 -t epub"),
    ("read", "", "Examples:\n  fqdt read 诡秘之主\n  fqdt read 7185478854738185271 -r 1-50"),
    ("audio", "",
     "Examples:\n  fqdt audio <id> --real\n  fqdt audio <id> --ai --tone 6\n  fqdt audio 诡秘之主 --ai"),
    ("update", "", ""),
    ("tts", "", "Examples:\n  fqdt tts book.txt"),
    ("shelf", "",
     "Examples:\n  fqdt shelf list\n  fqdt shelf list 已读\n  fqdt shelf list --local\n  fqdt shelf add <id>:<标题>\n  fqdt shelf add <id>\n  fqdt shelf move <id> 已读\n  fqdt shelf download 1"),
    ("login", "",
     "Examples:\n  fqdt login '<Cookie>'\n  fqdt login --check"),
    ("config", "",
     "Examples:\n  fqdt config init\n  fqdt config --list\n  fqdt config info.top_chapters 20"),
    ("api", "", "Examples:\n  fqdt api /api/v1/search -q query=诡秘 -q tab_type=3\n  fqdt api /api/v1/books/123 --jq data.book_name"),
    ("serve", "", ""),
];

/// help 尾部文本（conf 恒开 + examples 开关；Examples 标题套 TITLE 样式）。clap 与自定义渲染共用。
fn help_tail_text(name: &str, examples: bool) -> String {
    let Some((_, conf, ex)) = HELP_TAILS.iter().find(|e| e.0 == name) else { return String::new(); };
    let mut tail = String::new();
    if !conf.is_empty() { tail.push_str(conf); }
    if examples && !ex.is_empty() {
        if !tail.is_empty() { tail.push_str("\n\n"); }
        tail.push_str(ex);
    }
    tail.replace("Examples:", &format!("{TITLE}Examples:{RST}"))
}

/// 注入 help 底部（clap 路径）：配置对应段恒开；示例段受 `ui.help_examples` 控制（false 关闭）。
fn apply_help_tails(cmd: &mut clap::Command, examples: bool) {
    let set_tail = |c: &mut clap::Command, name: &str| {
        let tail = help_tail_text(name, examples);
        if !tail.is_empty() {
            let old = std::mem::replace(c, clap::Command::new("_"));
            *c = old.after_help(tail);
        }
    };
    set_tail(cmd, "");
    for sub in cmd.get_subcommands_mut() {
        let n = sub.get_name().to_string();
        set_tail(sub, &n);
    }
}

#[tokio::main]
async fn main() {
    // 逐 flag / config 键详情：在 clap 解析前拦截（`fqdt audio --tone --help` 等四场景）
    try_flag_help();
    let mut cmd = with_help_styles(Cli::command());
    let mut cfg = Config::load();
    if !cfg.ui_color { std::env::set_var("NO_COLOR", "1"); }
    apply_help_tails(&mut cmd, cfg.ui_help_examples);
    crate::util::init_color();
    // 顶层分组 help 拦截：裸调用/--help/help（子命令级 help 仍走 clap）
    let top_args: Vec<String> = std::env::args().skip(1).collect();
    if top_help_requested(&top_args) {
        render_top_help();
        return;
    }
    if let Some(path) = sub_help_requested(&top_args) {
        render_sub_help(&path, cfg.ui_help_examples);
        return;
    }
    let cli = Cli::from_arg_matches(&cmd.get_matches()).unwrap_or_else(|e| e.exit());
    cfg.apply_cli_overrides(
        cli.search_url.as_deref(),
        cli.catalog_url.as_deref(),
        cli.content_url.as_deref(),
    );
    if let Some(to) = cli.timeout { cfg.timeout = to; }
    cfg.ensure_dirs();
    cfg.validate();
    Config::save_default(false).ok(); // 启动自动 init：永不覆盖
    crate::helpfile::ensure_help_file();

    // 全局 -v：core 日志 stderr 镜像 + workflow 请求详情（日志默认静默，仍落盘 rust.log）
    let verbose = cli.verbose || cfg.verbose;
    fqapi_core::core_log::set_console(verbose);
    // 进度条运行期间日志走 util::note（清行/重画），避免拼在进度行尾部造成双行残留
    fqapi_core::core_log::set_console_hook(crate::util::note);

    match cli.cmd {
        Cmd::Search { keyword, page, auto, dry_run, output, jobs, range, format, interval } =>
            workflow::search::run(&keyword, page, output.as_deref(), jobs, range.as_deref(),
                format.as_deref(), verbose, auto, dry_run, interval, &cfg).await,
        Cmd::Info { cmd } =>
            dispatch_info(cmd, verbose, &cfg).await,
        Cmd::Download { book_id, output, range, format, jobs, force, interval } =>
            dispatch_download(book_id, output, range, format, jobs, force, verbose, interval, &cfg).await,
        Cmd::Audio { book_id, output, range, tone, lrc, jobs, force, interval, real, ai, tts, voice, keep_m4a, mp3, cover } =>
            dispatch_audio(book_id, output, range, tone, lrc, jobs, force, verbose, interval,
                real, ai, tts, &voice, keep_m4a, mp3, cover, &cfg).await,
        Cmd::Update { target, output, jobs, force, interval } =>
            dispatch_update(target, output, jobs, force, verbose, interval, &cfg).await,
        Cmd::Tts { path, voice, jobs } =>
            dispatch_tts(path, &voice, jobs, verbose, &cfg).await,
        Cmd::Shelf { cmd } => workflow::shelf::run(cmd, &cfg).await,
        Cmd::Read { book_id, range } => {
            let Some(id) = resolve_book_id(book_id, &cfg, verbose).await else { return; };
            workflow::read::run(&id, range.as_deref(), &cfg).await;
        }
        Cmd::Login { cookie, check, clear } =>
            workflow::account::login(cookie, check, clear).await,
        Cmd::Config { key, value, list, unset, force } =>
            crate::config::run_cmd(key, value, list, unset, force),
        Cmd::Api { path, method, query, data, jq, status, list } =>
            run_api(path, method, query, data, jq, status, list).await,
        Cmd::Serve { port, lan } => run_serve(port, lan).await,
        Cmd::Version => println!("fqdt {}", env!("CARGO_PKG_VERSION")),
        Cmd::Custom(args) => dispatch_custom(args, &cfg),
    }
}

fn dispatch_custom(args: Vec<String>, cfg: &Config) {
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    if let Some(template) = cfg.custom_commands.get(cmd) {
        let rest: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        let cmd_str = if rest.is_empty() {
            template.clone()
        } else {
            let mut s = template.clone();
            for arg in &rest {
                s = s.replacen("{}", arg, 1);
            }
            s
        };
        if cfg.verbose { eprintln!("  [verbose] exec: {}", cmd_str); }
        match std::process::Command::new("sh").arg("-c").arg(&cmd_str).output() {
            Ok(out) => {
                print!("{}", String::from_utf8_lossy(&out.stdout));
                if !out.status.success() {
                    eprintln!("  err 退出码 {}", out.status);
                }
            }
            Err(e) => eprintln!("  err 执行失败: {}", e),
        }
        return;
    }
    eprintln!("  err 未知命令: {}", cmd);
    eprintln!("  可用命令: search, info, download, audio, update, tts, shelf, login, config");
}

/// info 子命令分发：书籍 section 走 workflow::info::run；发现流/个人数据各走原入口。
async fn dispatch_info(cmd: InfoCmd, verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    match cmd {
        InfoCmd::Recommend { tab_type, tab_index, offset, json } =>
            workflow::discover::recommend(tab_type, tab_index, offset, json, cfg).await,
        InfoCmd::Category { tab, json } => workflow::discover::category(tab, json, cfg).await,
        InfoCmd::Folders { search, json } => workflow::account::read_section("folders", json, search.as_deref()).await,
        InfoCmd::History { search, json } => workflow::account::read_section("history", json, search.as_deref()).await,
        InfoCmd::Progress { search, json } => workflow::account::read_section("progress", json, search.as_deref()).await,
        InfoCmd::User { json } => workflow::account::read_section("user", json, None).await,
        InfoCmd::Highlights { json } => workflow::account::read_section("highlights", json, None).await,
        book => dispatch_info_book(book, vb, cfg).await,
    }
}

/// 书籍类 info 子命令：统一映射到 workflow::info::run 的 section。
async fn dispatch_info_book(cmd: InfoCmd, vb: bool, cfg: &Config) {
    let (id, section, range, show, add) = match cmd {
        InfoCmd::Overview { book_id, range, show, add } => (book_id, None, range, show, add),
        InfoCmd::Chapters { book_id, range, add } => (book_id, Some("chapters"), range, false, add),
        InfoCmd::Intro { book_id, add } => (book_id, Some("intro"), None, false, add),
        InfoCmd::Author { book_id, add } => (book_id, Some("author"), None, false, add),
        InfoCmd::Comments { book_id, add } => (book_id, Some("comments"), None, false, add),
        InfoCmd::Cover { book_id, add } => (book_id, Some("cover"), None, false, add),
        InfoCmd::Relative { book_id, add } => (book_id, Some("relative"), None, false, add),
        InfoCmd::Audio { book_id, add } => (book_id, Some("audio"), None, false, add),
        _ => return,
    };
    // id 位支持书名关键词交互选书（纯数字/作者 id 原样，同 download/audio）
    let Some(id) = resolve_book_id(id, cfg, vb).await else { return };
    workflow::info::run(&id, section, range.as_deref(), show, vb, cfg).await;
    if add { workflow::account::add_book(&id).await; }
}

/// 选书：纯数字（≥10 位，book_id/item_id/uid）或作者 id（N_数字）视为 id 原样，
/// 否则关键词交互选书并自动加书架（download/audio/info 共用）
async fn resolve_book_id(target: String, cfg: &Config, vb: bool) -> Option<String> {
    if target.chars().all(|c| c.is_ascii_digit()) && target.len() >= 10 {
        return Some(target);
    }
    if let Some(rest) = target.strip_prefix("N_") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return Some(target);
        }
    }
    match workflow::search::pick(&target, cfg, vb).await {
        Some((id, title)) => {
            config::add_bookmark(&id, &title).ok();
            Some(id)
        }
        None => None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_download(target: String, output: Option<String>, range: Option<String>,
    format: Option<String>, jobs: Option<usize>, force: bool, verbose: bool,
    _interval: u64, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let Some(book_id) = resolve_book_id(target, cfg, vb).await else { return };
    let p = types::DownloadParams {
        output, range, format, concurrent: jobs,
        audio: false, tone: 1, abr: cfg.abr,
        lrc: "external".into(), force, verbose: vb, book_title: None,
    };
    workflow::download::run(&book_id, &p, cfg).await;
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_audio(book_id: String, output: Option<String>, range: Option<String>,
    tone: usize, lrc: Option<String>, _jobs: Option<usize>, force: bool, verbose: bool,
    _interval: u64, real: bool, ai: bool, tts: bool, voice: &str, keep_m4a: bool, mp3: bool,
    cover: bool, base_cfg: &Config) {
    // flag 优先于 config 键：置位后以克隆配置下发到全部下游
    let mut cfg2 = base_cfg.clone();
    if keep_m4a { cfg2.audio_keep_m4a = true; }
    if mp3 { cfg2.audio_mp3 = true; }
    if cover { cfg2.audio_cover = true; }
    let cfg = &cfg2;
    let vb = verbose || cfg.verbose;
    let lrc_mode = lrc.as_deref().unwrap_or("external");
    // 本地目录原样；否则选书（纯数字=book_id，关键词交互选，与 download 共用）
    let book_id = if Path::new(&book_id).is_dir() {
        book_id
    } else {
        match resolve_book_id(book_id, cfg, vb).await {
            Some(id) => id,
            None => return,
        }
    };
    let path = Path::new(&book_id);

    // Edge-TTS：先下载正文（txt），再整目录合成 mp3（-o 显式指定 → 直接落该目录；默认 → 存储书籍目录下 Audio/）
    if tts {
        let p = types::DownloadParams {
            output: output.clone(), range, format: None, concurrent: None,
            audio: false, tone, abr: cfg.abr, lrc: "external".into(), force,
            verbose: vb, book_title: None,
        };
        workflow::download::run(&book_id, &p, cfg).await;
        let dir = output.as_deref().map(std::path::PathBuf::from).unwrap_or_else(|| cfg.output_dir.clone());
        workflow::audio::run_tts(&dir.to_string_lossy(), output.as_deref(), voice,
            &cfg.tts_rate, &cfg.tts_volume, &cfg.tts_pitch, cfg.abr, None, false, "",
            "external", vb).await;
        return;
    }

    if path.is_dir() {
        let p = types::DownloadParams {
            output: None, range, format: None, concurrent: None,
            audio: true, tone, abr: cfg.abr,
            lrc: lrc_mode.into(), force, verbose: vb, book_title: None,
        };
        workflow::download::run_dir(path, &p, vb, cfg).await;
        return;
    }

    // --real/--ai：真人讲书版 ↔ 文字版（AI 音色）自动互转
    let book_id = if real || ai {
        util::progress("解析音频版本");
        let r = api::Client::from_config(cfg, vb).resolve_audio_book(&book_id, real, ai).await;
        util::clear_progress();
        match r {
            Ok(id) => {
                if id != book_id {
                    eprintln!("  {DIM}版本 → {id}{RST}");
                }
                id
            }
            Err(e) => {
                eprintln!("  err {e}");
                return;
            }
        }
    } else {
        book_id
    };

    let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
        vec![2, 4, 5, 6, 74, 91]
    } else { cfg.audio_tone_fallbacks.clone() };
    // 直接书籍 ID 下载：不建 Audio 子文件夹，mp3 直接落目标目录（-o 或 output_dir 根）
    let out = output
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.output_dir.clone());
    workflow::download::fetch_and_dl_audio(&book_id, &out, &range, tone, cfg.abr, lrc_mode, force, &fallbacks, vb, cfg).await;
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_update(target: String, output: Option<String>, jobs: Option<usize>,
    force: bool, verbose: bool, _interval: u64, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    workflow::update::run(Some(&target), output.as_deref(), jobs, None, force, false, vb, _interval, cfg).await;
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_tts(path: String, voice: &str, _jobs: Option<usize>, verbose: bool, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    workflow::audio::run_tts(&path, None, voice, &cfg.tts_rate, &cfg.tts_volume, &cfg.tts_pitch,
        cfg.abr, None, false, "", "external", vb).await;
}


/// 写一行到 stdout；下游管道已关闭（EPIPE）时静默退出，避免 panic
pub(crate) fn emit_line(s: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{s}").is_err() {
        std::process::exit(0);
    }
}

/// `fqdt serve`：前台运行 HTTP 服务，Ctrl-C 退出并刷盘
async fn run_serve(port: u16, lan: bool) {
    match fqapi::serve_at(port, lan).await {
        Ok(url) => {
            println!("  {GREEN}ok{RST} 监听 {url}{}", if lan { "（局域网）" } else { "" });
            println!("  {DIM}Ctrl-C 退出{RST}");
            tokio::signal::ctrl_c().await.ok();
            fqapi::shutdown().await;
            println!("  {GREEN}ok{RST} 已退出（设备池已刷盘）");
        }
        Err(e) => {
            eprintln!("  err {e}");
            std::process::exit(1);
        }
    }
}

/// 拆分路径内的 `?query` 并合并 `-q` 追加项
fn merge_query(path: &str, extra: &[String]) -> (String, String) {
    let (p, mut q) = match path.split_once('?') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (path.to_string(), String::new()),
    };
    for kv in extra {
        if !q.is_empty() {
            q.push('&');
        }
        q.push_str(kv);
    }
    (p, q)
}

/// `fqdt api` 分发：列出端点，或透传调用后按 `--jq` 输出
async fn run_api(
    path: Option<String>,
    method: String,
    query: Vec<String>,
    data: Option<String>,
    jq: Option<String>,
    show_status: bool,
    list: bool,
) {
    if list {
        for ep in fqapi::ENDPOINTS {
            let req = if ep.required.is_empty() { "-" } else { ep.required };
            emit_line(&format!("{:<4} {:<52} {:<22} {}", ep.method, ep.path, req, ep.note));
        }
        return;
    }
    let Some(path) = path else {
        eprintln!(
            "  err 缺少端点路径\n  \
             用法: fqdt api <path> [-X METHOD] [-q k=v]... [--data BODY] [--jq PATH] [--status]\n  \
             \x20     fqdt api --list"
        );
        std::process::exit(1);
    };
    let (p, q) = merge_query(&path, &query);
    let body = data.map(|s| s.into_bytes()).unwrap_or_default();
    let reply = match fqapi::call(&method.to_uppercase(), &p, &q, body).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  err {e}");
            std::process::exit(1);
        }
    };
    if show_status {
        eprintln!("  {GREEN}ok{RST} HTTP {}", reply.status);
    }
    match jq {
        Some(filter) => match reply.json().ok().as_ref().and_then(|v| util::jq_extract(v, &filter)) {
            Some(v) => emit_line(&util::jq_display(&v)),
            None => {
                eprintln!("  err jq 无匹配: {filter}");
                std::process::exit(1);
            }
        },
        None => emit_line(reply.text().trim_end()),
    }
    if reply.status >= 400 {
        std::process::exit(1);
    }
}
