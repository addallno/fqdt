mod api;
mod audio;
mod config;
mod download;
mod epub;
mod fqapi;
mod types;
mod util;
mod workflow;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use types::Config;
use crate::util::{CYAN, DIM, GREEN, RST, YELLOW};
use std::path::Path;

#[derive(Parser)]
#[command(name = "fqdt", version, about = "番茄小说下载器")]
struct Cli {
    #[arg(long, global = true, help = "搜索 API 源 (逗号分隔；覆盖 config api.search_url)")]
    search_url: Option<String>,
    #[arg(long, global = true, help = "目录 API 源 (覆盖 config api.catalog_url)")]
    catalog_url: Option<String>,
    #[arg(long, global = true, help = "内容 API 源 (覆盖 config api.content_url)")]
    content_url: Option<String>,
    #[arg(long, global = true, help = "请求超时秒 (覆盖 config api.timeout)")]
    timeout: Option<u64>,
    #[arg(short = 'v', long, global = true, help = "详细日志 (core 日志 stderr 镜像 + 请求详情)")]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 搜索小说（纯列表展示）
    Search {
        keyword: String,
        #[arg(short='p', long, default_value="1", help = "页码")]
        page: usize,
        #[arg(short='D', long, help = "自动下载第 N 本结果")]
        auto: Option<usize>,
        #[arg(long, help = "仅展示搜索结果 (默认)")]
        dry_run: bool,
        #[arg(short='o', long, help = "输出目录 (配合 -D)")]
        output: Option<String>,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
        #[arg(short='r', long, help = "章节范围 1-50 / -5 / 10-")]
        range: Option<String>,
        #[arg(short='t', long, help = "输出格式: txt / epub")]
        format: Option<String>,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒")]
        interval: u64,
    },
    /// 查看书籍信息
    #[command(alias = "i")]
    Info {
        #[arg(help = "id 自动识别: 书/章节 item_id/作者(N_前缀)/读者 uid")]
        book_id: String,
        #[arg(help = "部分: chapters/intro/author/comments/cover/related/audio（仅书 id）")]
        section: Option<String>,
        #[arg(short='r', long, help = "章节范围")]
        range: Option<String>,
        #[arg(short='s', long, help = "显示正文预览")]
        show: bool,
        #[arg(long, help = "加入云端书架（仅书 id，展示后执行）")]
        add: bool,
    },
    /// 下载正文（简写 d）
    #[command(alias = "d")]
    Download {
        #[arg(help = "book_id 或书名关键词 (多结果时选择序号)")]
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
    /// 下载语音（简写 a）
    #[command(alias = "a")]
    Audio {
        book_id: String,
        #[arg(short='o', long, help = "输出目录")]
        output: Option<String>,
        #[arg(short='r', long, help = "章节范围 1-50 / -5 / 10-")]
        range: Option<String>,
        #[arg(long, default_value = "1", help = "AI 音色编号")]
        tone: usize,
        #[arg(long, help = "歌词模式")]
        lrc: Option<String>,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
        #[arg(short='f', long, help = "强制覆盖已存在文件")]
        force: bool,
        #[arg(short='i', long, default_value = "0", help = "请求间隔毫秒")]
        interval: u64,
        #[arg(long, help = "真人讲书版", conflicts_with_all = ["ai", "tts"])]
        real: bool,
        #[arg(long, help = "智能朗读", conflicts_with_all = ["tts"])]
        ai: bool,
        #[arg(long, help = "Edge-TTS 本地合成")]
        tts: bool,
        #[arg(long, default_value = "zh-CN-XiaoxiaoNeural", help = "Edge-TTS 语音名")]
        voice: String,
    },
    /// 增量更新已下载的书籍（简写 u）
    #[command(alias = "u")]
    Update {
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
    /// 文本转语音（简写 t）
    #[command(alias = "t")]
    Tts {
        path: String,
        #[arg(long, default_value = "zh-CN-XiaoxiaoNeural", help = "Edge-TTS 语音名")]
        voice: String,
        #[arg(short='j', long, help = "并发数")]
        jobs: Option<usize>,
    },
    /// 书架管理（简写 s）
    #[command(alias = "s")]
    Shelf {
        #[arg(short='a', long)]
        add: Option<String>,
        #[arg(short='d', long)]
        delete: Option<usize>,
        #[arg(short='D', long)]
        dl: Option<usize>,
        #[arg(short='U', long)]
        update: bool,
    },
    /// 生成默认配置文件
    Init,
    /// 登录态管理（Cookie 来自浏览器登录 fanqienovel.com 复制）
    ///
    /// 上游无账号密码登录；Cookie 存 ~/.config/fqdt/cookie 独立文件。
    Login {
        /// Cookie 字符串（省略则显示当前状态）
        cookie: Option<String>,
        /// 用当前 Cookie 请求书架验证登录态
        #[arg(long, help = "验证登录态（请求书架列表）")]
        check: bool,
        /// 清除已保存的 Cookie
        #[arg(long, help = "清除 Cookie")]
        clear: bool,
    },
    /// 个人数据（书架/书单/阅读历史/进度，cookie 鉴权；需先 fqdt login）
    Account {
        /// 部分: shelf(默认)/folders/history/progress/user
        section: Option<String>,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
        /// 加书到云端书架
        #[arg(long, value_name = "BOOK_ID", conflicts_with_all = ["remove", "check", "regroup"])]
        add: Option<String>,
        /// 从云端书架移出
        #[arg(long, value_name = "BOOK_ID", conflicts_with_all = ["add", "check", "regroup"])]
        remove: Option<String>,
        /// 检查是否在云端书架
        #[arg(long, value_name = "BOOK_ID", conflicts_with_all = ["add", "remove", "regroup"])]
        check: Option<String>,
        /// 移动书到分组（需联用 --to，组不存在则自动创建）
        #[arg(long = "move", value_name = "BOOK_ID", requires = "to", conflicts_with_all = ["add", "remove", "check"])]
        regroup: Option<String>,
        /// 目标分组名（与 --move 联用）
        #[arg(long, value_name = "GROUP", requires = "regroup")]
        to: Option<String>,
        /// 按书名过滤（仅 shelf，本地匹配）
        #[arg(long, value_name = "KEYWORD")]
        search: Option<String>,
        /// 拉取每本书作者（仅 shelf，本地缓存 30 天，首次较慢）
        #[arg(long)]
        author: bool,
    },
    /// 查看/修改配置（git config 式: --list / <key> / <key> <value> / --unset <key>）
    ///
    /// 键为点号形式，如 info.top_chapters、audio.abr、ui.clip_len
    Config {
        /// 键（点号形式）
        key: Option<String>,
        /// 新值（缺省为读取）
        value: Option<String>,
        #[arg(long, help = "列出全部键值")]
        list: bool,
        #[arg(long, help = "删除该键（恢复默认）")]
        unset: bool,
    },
    /// 小工具集 + 函数管道（简写 fn）
    #[command(name = "function", alias = "fn")]
    Function {
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// 透传调用 fqapi 端点 (gh api 式)
    ///
    /// 路径如 /api/v1/books/{id}/directory，query 拼在 ? 后或用 -q 追加。
    Api {
        /// 端点路径（含可选 ?query）
        path: Option<String>,
        /// HTTP 方法
        #[arg(short = 'X', long, default_value = "GET", help = "HTTP 方法")]
        method: String,
        /// 追加查询参数 k=v（可重复）
        #[arg(short = 'q', long = "query", value_name = "K=V", help = "查询参数")]
        query: Vec<String>,
        /// 请求体（POST/PUT 用）
        #[arg(short = 'd', long = "data", help = "请求体")]
        data: Option<String>,
        /// 简易 JSON 提取（.a.b[0].c 形式，字符串无引号输出）
        #[arg(long = "jq", help = "JSON 字段提取路径")]
        jq: Option<String>,
        /// stderr 打印 HTTP 状态码
        #[arg(long = "status", help = "stderr 打印状态码")]
        status: bool,
        /// 列出常用端点
        #[arg(long = "list", help = "列出常用端点")]
        list: bool,
    },
    /// 首页推荐流
    Recommend {
        #[arg(long, default_value = "2", help = "区块类型 (2 推荐/25 小说/11 经典/5 听书)")]
        tab_type: u32,
        #[arg(long, default_value = "0", help = "区块索引")]
        tab_index: u32,
        #[arg(long, default_value = "0", help = "偏移")]
        offset: u32,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
    },
    /// 分类列表（男频/女频/出版/听书）
    Category {
        #[arg(long, default_value = "1", help = "1 男频 / 2 女频 / 3 出版 / 4 听书")]
        tab: u32,
        #[arg(long, help = "输出原始 JSON")]
        json: bool,
    },
    /// 对外 HTTP 服务（复用 fqapi-core server，Ctrl-C 退出）
    Serve {
        #[arg(long, default_value = "18921", help = "监听端口")]
        port: u16,
        #[arg(long, help = "监听局域网地址（默认仅本机回环）")]
        lan: bool,
    },
    /// [兼容] 统一下载命令
    #[command(hide = true)]
    Get {
        target: Option<String>,
        #[arg(short='o', long)] output: Option<String>,
        #[arg(short='r', long)] range: Option<String>,
        #[arg(short='t', long)] format: Option<String>,
        #[arg(short='j', long)] jobs: Option<usize>,
        #[arg(short='f', long)] force: bool,
        #[arg(long)] audio: bool,
        #[arg(long)] audio_only: bool,
        #[arg(long, default_value = "1")] tone: usize,
        #[arg(long)] lrc: Option<String>,
        #[arg(long)] update: bool,
        #[arg(short='i', long, default_value = "0")] interval: u64,
    },
    /// 自定义命令（来自 config.ini [workflow_cmd]）
    #[command(external_subcommand)]
    Custom(Vec<String>),
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
        ("取值", "1-91，默认 1（可被 config audio.tone 覆盖）"),
        ("说明", "文字版 AI 朗读音色；各书可用集合不同"),
        ("查询", "fqdt info <book_id> audio 查看本书可用编号"),
        ("配置", "config.ini [audio] audio_tone（fqdt config audio.tone）"),
        ("示例", "fqdt audio <book_id> --tone 6"),
        ("相关", "--ai --real --tts"),
    ]}),
    ("audio", "--lrc", FlagDoc { brief: "歌词模式", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "external / embed / both / off，默认 external"),
        ("说明", "external=同名 .lrc 文件；embed=嵌入 MP3 ID3 USLT；both=两者；off=不生成"),
        ("限制", "仅音频下载路径生效；MP4 容器嵌入无效故默认 external"),
        ("示例", "fqdt audio <book_id> --lrc embed"),
        ("相关", "--tts"),
    ]}),
    ("audio", "--real", FlagDoc { brief: "真人讲书版", takes_value: false, fields: &[
        ("说明", "自动发现最热真人听书版（related_audio_bookids 中听书量最大的 book_type=1）"),
        ("互斥", "--ai --tts"),
        ("示例", "fqdt audio <book_id> --real"),
        ("查询", "fqdt info <book_id> related 查看该书真人版"),
    ]}),
    ("audio", "--ai", FlagDoc { brief: "智能朗读", takes_value: false, fields: &[
        ("说明", "使用文字版 AI 音色，配合 --tone 选编号；related_novel_bookid 自动转回文字版"),
        ("互斥", "--tts"),
        ("示例", "fqdt audio <book_id> --ai --tone 6"),
        ("相关", "--tone --real"),
    ]}),
    ("audio", "--tts", FlagDoc { brief: "Edge-TTS 本地合成", takes_value: false, fields: &[
        ("说明", "先下载正文 txt 再本地合成 MP3；需安装 edge-tts（pip install --user --break-system-packages edge-tts）"),
        ("互斥", "--real --ai"),
        ("示例", "fqdt audio <book_id> --tts --voice zh-CN-YunxiNeural"),
        ("相关", "--voice"),
    ]}),
    ("audio", "--voice", FlagDoc { brief: "Edge-TTS 语音名", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("默认", "zh-CN-XiaoxiaoNeural"),
        ("说明", "仅 --tts 时生效"),
        ("示例", "fqdt audio <book_id> --tts --voice zh-CN-YunxiNeural"),
        ("相关", "--tts"),
    ]}),
    ("download", "--range", FlagDoc { brief: "章节范围", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "N 单章 / N-M 区间 / -M 末 M 章 / N- 自 N 起"),
        ("说明", "按目录序筛选；缺省全部章节"),
        ("示例", "fqdt download <book_id> -r 1-50   fqdt download <book_id> -r -5"),
        ("相关", "-t -j"),
    ]}),
    ("download", "--format", FlagDoc { brief: "输出格式", takes_value: true, fields: &[
        ("类型", "字符串"),
        ("取值", "txt（默认）/ epub"),
        ("配置", "config.ini [download] format（fqdt config download.format）"),
        ("示例", "fqdt download <book_id> -t epub"),
    ]}),
    ("download", "--jobs", FlagDoc { brief: "并发数", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "按 CPU 核数"),
        ("配置", "config.ini [download] concurrent（fqdt config download.concurrent）"),
        ("示例", "fqdt download <book_id> -j 4"),
    ]}),
    ("download", "--force", FlagDoc { brief: "强制覆盖", takes_value: false, fields: &[
        ("说明", "已存在文件重新下载；缺省跳过已下载章节"),
        ("示例", "fqdt download <book_id> --force"),
    ]}),
    ("download", "--interval", FlagDoc { brief: "请求间隔毫秒", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "0（不限速）"),
        ("说明", "每章请求之间的等待，限速用"),
        ("示例", "fqdt download <book_id> -i 500"),
    ]}),
    ("search", "--auto", FlagDoc { brief: "自动下载第 N 本", takes_value: true, fields: &[
        ("类型", "整数（1-based）"),
        ("说明", "跳过交互选择，直接下载第 N 条结果；配合 -o/-r/-t"),
        ("示例", "fqdt search 诡秘 -D 1"),
        ("相关", "-o -r -t"),
    ]}),
    ("search", "--page", FlagDoc { brief: "页码", takes_value: true, fields: &[
        ("类型", "整数"),
        ("默认", "1（每页 10 本）"),
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
        ("取值", "txt（默认）/ epub"),
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
        ("示例", "fqdt info <book_id> chapters -r 1-50"),
    ]}),
    ("info", "--show", FlagDoc { brief: "正文预览", takes_value: false, fields: &[
        ("说明", "概览前列章节附带正文（前 40 行折叠，完整用 function fetch-content）"),
        ("示例", "fqdt info <book_id> --show"),
    ]}),
];

/// 子命令别名 → 主名（表内统一用主名）。
fn canonical_sub(s: &str) -> &str {
    match s {
        "a" => "audio", "d" => "download", "i" => "info", "s" => "shelf",
        "u" => "update", "t" => "tts", "fn" => "function", other => other,
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

/// help 底部附加段：(子命令名, 配置对应, 示例)。示例受 config `ui.help_examples` 控制。
const HELP_TAILS: &[(&str, &str, &str)] = &[
    ("", "优先级: 命令行 flag > config.ini 同义项", ""),
    ("search", "flag 详情: fqdt search -D --help", "示例:\n  fqdt search 诡秘之主\n  fqdt search 诡秘 -D 1"),
    ("info", "配置: [info] top_chapters/top_comments/hot/intro_len/comments_count/author_count  [ui] clip_len\nflag 详情: fqdt info -r --help",
     "示例:\n  fqdt info <book_id>\n  fqdt info <book_id> chapters\n  fqdt info <book_id> --add（收藏）\n  fqdt info <item_id>（章节卡）\n  fqdt info 2_698…（作者）"),
    ("download", "配置: [download] concurrent/format/output_dir/filename_template\nflag 详情: fqdt download -r --help",
     "示例:\n  fqdt download <book_id> -r 1-50 -t epub"),
    ("audio", "配置: [audio] tone/tone_fallbacks/abr/post_process  [tts] rate/volume/pitch\nflag 详情: fqdt audio --tone --help · fqdt help audio --lrc",
     "示例:\n  fqdt audio <book_id> --real\n  fqdt audio <book_id> --tone 6"),
    ("update", "配置: 同 [download]", ""),
    ("tts", "配置: [tts] rate/volume/pitch", "示例:\n  fqdt tts book.txt"),
    ("shelf", "", ""),
    ("init", "", ""),
    ("login", "配置: Cookie 独立存 ~/.config/fqdt/cookie（不进 config.ini）",
     "示例:\n  fqdt login '<Cookie>'\n  fqdt login --check"),
    ("account", "", "示例:\n  fqdt account shelf\n  fqdt account user\n  fqdt account --add <book_id>\n  fqdt account --move <book_id> --to Done\n  fqdt account shelf --search 词\n  fqdt account shelf --author"),
    ("config", "配置: 读写 config.ini 全部键\n键详情: fqdt config <key> --help",
     "示例:\n  fqdt config --list\n  fqdt config info.top_chapters 20"),
    ("function", "", ""),
    ("api", "", "示例:\n  fqdt api /api/v1/search -q query=诡秘 -q tab_type=3\n  fqdt api /api/v1/books/123 --jq data.book_name"),
    ("recommend", "", ""),
    ("category", "", ""),
    ("serve", "", ""),
];

/// 注入 help 底部：配置对应段恒开；示例段受 `ui.help_examples` 控制（false 关闭）。
fn apply_help_tails(cmd: &mut clap::Command, examples: bool) {
    let set_tail = |c: &mut clap::Command, name: &str| {
        if let Some((_, conf, ex)) = HELP_TAILS.iter().find(|e| e.0 == name) {
            let mut tail = String::new();
            if !conf.is_empty() { tail.push_str(conf); }
            if examples && !ex.is_empty() {
                if !tail.is_empty() { tail.push_str("\n\n"); }
                tail.push_str(ex);
            }
            if !tail.is_empty() {
                let old = std::mem::replace(c, clap::Command::new("_"));
                *c = old.after_help(tail);
            }
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
    let mut cmd = Cli::command();
    let mut cfg = Config::load();
    if !cfg.ui_color { std::env::set_var("NO_COLOR", "1"); }
    apply_help_tails(&mut cmd, cfg.ui_help_examples);
    crate::util::init_color();
    let cli = Cli::from_arg_matches(&cmd.get_matches()).unwrap_or_else(|e| e.exit());
    cfg.apply_cli_overrides(
        cli.search_url.as_deref(),
        cli.catalog_url.as_deref(),
        cli.content_url.as_deref(),
    );
    if let Some(to) = cli.timeout { cfg.timeout = to; }
    cfg.ensure_dirs();
    cfg.validate();
    Config::save_default().ok();

    // 全局 -v：core 日志 stderr 镜像 + workflow 请求详情（日志默认静默，仍落盘 rust.log）
    let verbose = cli.verbose || cfg.verbose;
    fqapi_core::core_log::set_console(verbose);

    match cli.cmd {
        Cmd::Search { keyword, page, auto, dry_run, output, jobs, range, format, interval } =>
            workflow::search::run(&keyword, page, output.as_deref(), jobs, range.as_deref(),
                format.as_deref(), verbose, auto, dry_run, interval, &cfg).await,
        Cmd::Info { book_id, section, range, show, add } => {
            workflow::info::run(&book_id, section.as_deref(), range.as_deref(), show, verbose, &cfg).await;
            if add { workflow::account::add_book(&book_id).await; }
        }
        Cmd::Download { book_id, output, range, format, jobs, force, interval } =>
            dispatch_download(book_id, output, range, format, jobs, force, verbose, interval, &cfg).await,
        Cmd::Audio { book_id, output, range, tone, lrc, jobs, force, interval, real, ai, tts, voice } =>
            dispatch_audio(book_id, output, range, tone, lrc, jobs, force, verbose, interval,
                real, ai, tts, &voice, &cfg).await,
        Cmd::Update { target, output, jobs, force, interval } =>
            dispatch_update(target, output, jobs, force, verbose, interval, &cfg).await,
        Cmd::Tts { path, voice, jobs } =>
            dispatch_tts(path, &voice, jobs, verbose, &cfg).await,
        Cmd::Get { target, output, range, format, jobs, force, audio, audio_only, tone, lrc, update, interval } =>
            dispatch_get(target, output, range, format, jobs, force, verbose, audio, audio_only, tone, lrc, update, interval, &cfg).await,
        Cmd::Shelf { add, delete, dl, update } => workflow::shelf::run(add, delete, dl, update, &cfg).await,
        Cmd::Init => { Config::save_default().ok(); println!("  {GREEN}ok{RST} ~/.config/fqdt/config.ini"); }
        Cmd::Login { cookie, check, clear } =>
            workflow::account::login(cookie, check, clear).await,
        Cmd::Account { section, json, add, remove, check, regroup, to, search, author } => {
            let args = workflow::account::AccountArgs { add, remove, check, regroup, to, search, author };
            workflow::account::account(section, json, args, &cfg).await;
        }
        Cmd::Config { key, value, list, unset } =>
            crate::config::run_cmd(key, value, list, unset),
        Cmd::Function { args } => run_function(args, &cfg).await,
        Cmd::Api { path, method, query, data, jq, status, list } =>
            run_api(path, method, query, data, jq, status, list).await,
        Cmd::Recommend { tab_type, tab_index, offset, json } =>
            workflow::discover::recommend(tab_type, tab_index, offset, json, &cfg).await,
        Cmd::Category { tab, json } =>
            workflow::discover::category(tab, json, &cfg).await,
        Cmd::Serve { port, lan } => run_serve(port, lan).await,
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
    eprintln!("  可用命令: search, info, download, audio, update, tts, shelf, init, login, account, function");
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_download(target: String, output: Option<String>, range: Option<String>,
    format: Option<String>, jobs: Option<usize>, force: bool, verbose: bool,
    _interval: u64, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    // 纯数字（≥10 位）视为 book_id，否则当搜索词：搜索 → 交互选择 → 下载
    let book_id = if target.chars().all(|c| c.is_ascii_digit()) && target.len() >= 10 {
        target
    } else {
        match workflow::search::pick(&target, cfg, vb).await {
            Some((id, title)) => {
                config::add_bookmark(&id, &title).ok();
                id
            }
            None => return,
        }
    };
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
    _interval: u64, real: bool, ai: bool, tts: bool, voice: &str, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let lrc_mode = lrc.as_deref().unwrap_or("external");
    let path = Path::new(&book_id);

    // Edge-TTS：先下载正文（txt），再整目录合成 mp3 到 <out>/Audio/
    if tts {
        let p = types::DownloadParams {
            output: output.clone(), range, format: None, concurrent: None,
            audio: false, tone, abr: cfg.abr, lrc: "external".into(), force,
            verbose: vb, book_title: None,
        };
        workflow::download::run(&book_id, &p, cfg).await;
        let dir = output.map(std::path::PathBuf::from).unwrap_or_else(|| cfg.output_dir.clone());
        workflow::audio::run_tts(&dir.to_string_lossy(), None, voice,
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
    let out = output
        .map(|s| std::path::PathBuf::from(s).join("Audio"))
        .unwrap_or_else(|| cfg.output_dir.join("Audio"));
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

#[allow(clippy::too_many_arguments)]
async fn dispatch_get(target: Option<String>, output: Option<String>, range: Option<String>,
    format: Option<String>, jobs: Option<usize>, force: bool, verbose: bool,
    audio: bool, audio_only: bool, tone: usize, lrc: Option<String>,
    update: bool, interval: u64, cfg: &Config) {
    let vb = verbose || cfg.verbose;
    let target = match target {
        Some(t) => t,
        None => { eprintln!("  err 需要书籍 ID、目录或 --audio-only"); return; }
    };

    if update {
        workflow::update::run(Some(&target), output.as_deref(), jobs, range.as_deref(), force, audio, vb, interval, cfg).await;
        return;
    }

    if audio_only {
        let lrc_mode = lrc.as_deref().unwrap_or("external");
        let fallbacks = if cfg.audio_tone_fallbacks.is_empty() {
            vec![2, 4, 5, 6, 74, 91]
        } else { cfg.audio_tone_fallbacks.clone() };
        let out = output
            .map(|s| std::path::PathBuf::from(s).join("Audio"))
            .unwrap_or_else(|| cfg.output_dir.join("Audio"));
        workflow::download::fetch_and_dl_audio(&target, &out, &range, tone, cfg.abr, lrc_mode, force, &fallbacks, vb, cfg).await;
        return;
    }

    let lrc_mode = lrc.as_deref().unwrap_or("external");
    let p = types::DownloadParams {
        output, range, format, concurrent: jobs,
        audio, tone, abr: cfg.abr,
        lrc: lrc_mode.into(), force, verbose: vb, book_title: None,
    };
    workflow::download::run(&target, &p, cfg).await;
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

async fn run_function(args: Vec<String>, cfg: &Config) {
    if args.is_empty() {
        eprintln!("  err 需要函数名或管道\n  用法:\n  \
            fqdt function embed <file> [--lrc] [--cover <img>]\n  \
            fqdt function process <file> [--abr N] [--speed X] [--normalize] [--cmd <cmd>]\n  \
            fqdt function fetch-catalog <book_id>\n  \
            fqdt function fetch-content <item_id>\n  \
            fqdt function fetch-audio-url <book_id> <item_id> [tone]\n  \
            fqdt function search <keyword> [-p <page>]\n  \
            fqdt function strip-html <file|->\n  \
            fqdt function compress <file> [--abr N]\n  \
            fqdt function embed-lrc <file>\n  \
            fqdt function embed-cover <file> <cover>\n  \
            fqdt function read-info <dir>\n  \
            fqdt function read-audio-info <dir>\n  \
            fqdt function save <file>\n  \
            fqdt function save <file> (管道末端保存输出)\n  \
            fqdt function ... \\; ... \\; ...  (chaining with ;)\n  \
            \n  主命令:\n  \
            fqdt download <book_id> [选项]\n  \
            fqdt audio <book_id> [选项]\n  \
            fqdt update <book_id|目录> [选项]\n  \
            fqdt tts <路径> [选项]\n  \
            fqdt search <keyword> [选项]\n  \
            fqdt info <book_id> [选项]\n  \
            fqdt shelf [选项]\n  \
            fqdt init\n  \
            fqdt function <fn> [参数]");
        return;
    }

    let sep_positions: Vec<usize> = args.iter().enumerate()
        .filter(|(_, s)| *s == ";").map(|(i, _)| i).collect();

    if sep_positions.is_empty() {
        let out = dispatch_one(&args, cfg, "").await;
        if !out.is_empty() { println!("{}", out); }
    } else {
        let mut segments: Vec<&[String]> = vec![];
        let mut start = 0usize;
        for &pos in &sep_positions {
            if pos > start {
                segments.push(&args[start..pos]);
            }
            start = pos + 1;
        }
        if start < args.len() {
            segments.push(&args[start..]);
        }

        let mut piped = String::new();
        for seg in &segments {
            if seg.is_empty() { continue; }
            let resolved: Vec<String> = seg.iter()
                .map(|a| a.replace("{}", &piped))
                .collect();
            piped = dispatch_one(&resolved, cfg, &piped).await;
            if !piped.is_empty() {
                println!("{}", piped);
            }
        }
    }
}

async fn dispatch_one(args: &[String], cfg: &Config, piped: &str) -> String {
    fn err(e: String) -> String { eprintln!("  err {}", e); String::new() }

    if args.is_empty() { return String::new(); }
    let func = &args[0];
    let rest: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();

    match func.as_str() {
        "embed" | "emb" => {
            if rest.is_empty() { return err("需要 input 路径".into()); }
            let input = rest[0].to_string();
            let lrc = rest.contains(&"--lrc");
            let cover = rest.iter().position(|a| *a == "--cover").and_then(|i| rest.get(i+1).map(|s| s.to_string()));
            workflow::embed::run(&input, lrc, cover, cfg);
            input
        }
        "process" | "proc" => {
            if rest.is_empty() { return err("需要 input 路径".into()); }
            let input = rest[0].to_string();
            let abr = rest.iter().position(|a| *a == "--abr").and_then(|i| rest.get(i+1).and_then(|s| s.parse::<u32>().ok()));
            let speed = rest.iter().position(|a| *a == "--speed").and_then(|i| rest.get(i+1).and_then(|s| s.parse::<f32>().ok()));
            let normalize = rest.contains(&"--normalize");
            let cmd = rest.iter().position(|a| *a == "--cmd").and_then(|i| rest.get(i+1).map(|s| s.to_string()));
            workflow::process::run(&input, abr, speed, normalize, cmd, cfg);
            input
        }
        "fetch-catalog" | "cat" => {
            if rest.is_empty() { return err("需要 book_id".into()); }
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.fetch_catalog(rest[0]).await {
                Ok(chs) => {
                    let json = serde_json::to_string_pretty(&chs).unwrap_or_default();
                    println!("{}", json);
                    chs.iter().map(|c| c.item_id.clone()).next().unwrap_or_default()
                }
                Err(e) => err(e)
            }
        }
        "fetch-content" | "content" => {
            if rest.is_empty() { return err("需要 item_id".into()); }
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.fetch_content(rest[0]).await {
                Ok(text) => { println!("{}", text); text }
                Err(e) => err(e)
            }
        }
        "fetch-audio-url" | "audio-url" => {
            if rest.len() < 2 { return err("需要 book_id item_id [tone]".into()); }
            let tone: usize = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.fetch_audio(rest[0], rest[1], tone).await {
                Ok((url, key)) => {
                    println!("{}", url);
                    if !key.is_empty() { eprintln!("  {GREEN}ok{RST} CENC 密钥 {}", key); }
                    url
                }
                Err(e) => err(e)
            }
        }
        "search" => {
            if rest.is_empty() { return err("需要 keyword".into()); }
            let keyword = rest[0].to_string();
            let page: usize = rest.iter().position(|a| *a == "-p" || *a == "--page")
                .and_then(|i| rest.get(i+1).and_then(|s| s.parse().ok())).unwrap_or(1);
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.search(&keyword, page).await {
                Ok(books) => {
                    let json = serde_json::to_string_pretty(&books).unwrap_or_default();
                    println!("{}", json);
                    books.iter().map(|b| b.book_id.clone()).next().unwrap_or_default()
                }
                Err(e) => err(e)
            }
        }
        "strip-html" | "html" => {
            if rest.is_empty() { return err("需要文件路径或 -".into()); }
            let text = if rest[0] == "-" {
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf).ok();
                buf
            } else {
                std::fs::read_to_string(rest[0]).unwrap_or_default()
            };
            let out = api::strip_html(&text);
            println!("{}", out);
            out
        }
        "compress" | "mp3" => {
            if rest.is_empty() { return err("需要文件路径".into()); }
            let input = rest[0].to_string();
            let abr: u32 = rest.iter().position(|a| *a == "--abr")
                .and_then(|i| rest.get(i+1).and_then(|s| s.parse().ok())).unwrap_or(32);
            let path = std::path::Path::new(&input);
            audio::post_process(path, abr, None, false, "", cfg.verbose);
            println!("  {GREEN}ok{RST} {}", input);
            input
        }
        "embed-lrc" | "lrc" => {
            if rest.is_empty() { return err("需要 MP3 路径".into()); }
            let input = rest[0].to_string();
            let path = std::path::Path::new(&input);
            let ch = types::Chapter {
                index: 0,
                title: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                item_id: String::new(),
            };
            audio::embed_lrc(path, &ch, cfg.verbose, None);
            println!("  {GREEN}ok{RST} {}", input);
            input
        }
        "embed-cover" | "cover" => {
            if rest.len() < 2 { return err("需要 MP3 路径和封面路径".into()); }
            let mp3 = rest[0];
            let cover = rest[1];
            audio::embed_cover(std::path::Path::new(mp3), std::path::Path::new(cover), cfg.verbose);
            println!("  {GREEN}ok{RST} {} ← {}", mp3, cover);
            mp3.to_string()
        }
        "read-info" | "info" => {
            if rest.is_empty() { return err("需要目录路径".into()); }
            match download::read_info_list(std::path::Path::new(rest[0])) {
                Ok((bid, title, fmt, chapters)) => {
                    let out = format!("book_id={}, title={}, format={}, chapters={}",
                        bid, title, fmt, chapters.len());
                    println!("  {}", out);
                    out
                }
                Err(e) => err(e)
            }
        }
        "read-audio-info" | "audio-info" => {
            if rest.is_empty() { return err("需要目录路径".into()); }
            let dir = std::path::Path::new(rest[0]);
            let audio_dir = if dir.join("Audio").exists() { dir.join("Audio") } else { dir.to_path_buf() };
            match download::read_audio_info_list(&audio_dir) {
                Ok((bid, title, chapters)) => {
                    let out = format!("book_id={}, title={}, chapters={}", bid, title, chapters.len());
                    println!("  {}", out);
                    out
                }
                Err(e) => err(e)
            }
        }
        "fetch-detail" | "detail" => {
            if rest.is_empty() { return err("需要 book_id".into()); }
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.fetch_detail(rest[0]).await {
                Ok(info) => { println!("  {}", info); info }
                Err(e) => err(e)
            }
        }
        "fetch-content-batch" | "batch" => {
            if rest.len() < 2 { return err("需要 book_id 和 item_ids".into()); }
            let item_ids: Vec<&str> = rest[1..].to_vec();
            let api = api::Client::from_config(cfg, cfg.verbose);
            match api.fetch_content_batch(rest[0], &item_ids).await {
                Ok(map) => {
                    for (id, content) in &map {
                        println!("  [{}]\n{}\n", id, content);
                    }
                    map.into_values().next().unwrap_or_default()
                }
                Err(e) => err(e)
            }
        }
        "save" => {
            if rest.is_empty() { return err("需要输出路径".into()); }
            let path = rest[0].to_string();
            if let Err(e) = std::fs::write(&path, piped) {
                return err(format!("写入失败: {}", e));
            }
            println!("  {GREEN}ok{RST} 保存 {} ({} 字节)", path, piped.len());
            piped.to_string()
        }
        _ => err(format!("未知函数: {}", func))
    }
}

