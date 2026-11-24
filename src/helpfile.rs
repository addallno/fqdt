//! 配置目录下的 help.txt：解释性参考——命令的格式与目的、各参数/端点的作用、
//! 以及每个命令底层实际发出的 API 请求格式。**不是** clap help 的复制（用户要求：
//! 要讲清格式与目的、要什么命令、端点有什么作用，不要 example）。
//! 启动时 ensure：文件不存在或版本标记不一致才重写。

use clap::Command;
use crate::types::get_home;

/// 启动时确保 `~/.config/fqdt/help.txt` 存在且为当前版本。
pub fn ensure_help_file() {
    let dir = get_home().join(".config/fqdt");
    let path = dir.join("help.txt");
    let marker = format!("fqdt {} -- 命令参考", env!("CARGO_PKG_VERSION"));
    if let Ok(old) = std::fs::read_to_string(&path) {
        if old.starts_with(&marker) { return; }
    }
    if std::fs::create_dir_all(&dir).is_err() { return; }
    let body = render(&crate::cli_command());
    let _ = std::fs::write(path, format!("{marker}\n\n{body}"));
}

fn render(root: &Command) -> String {
    let mut out = String::from(CONVENTION);
    render_cmd(root, "", 0, &mut out);
    out.push_str(&api_section());
    out.push_str("\n优先级: 命令行 flag > config.ini 同义项\n");
    strip_ansi(out)
}

/// 递归渲染一条命令：目的 + 格式 + 参数作用 + 选项作用 + 子命令一览。
fn render_cmd(cmd: &Command, prefix: &str, depth: usize, out: &mut String) {
    let full = if prefix.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{} {}", prefix, cmd.get_name())
    };
    if depth == 0 {
        render_subs(cmd, "", "顶层命令:", out);
    } else {
        let pad = "  ".repeat(depth);
        out.push_str(&format!("\n{}### fqdt {}\n", pad, full));
        if let Some(a) = cmd.get_about() {
            out.push_str(&format!("{}目的: {}\n", pad, a));
        }
        out.push_str(&format!("{}格式: {}\n", pad, usage_of(cmd, &full)));
        render_args(cmd, &pad, out);
        render_subs(cmd, &format!("{}  ", pad), "子命令:", out);
    }
    let sub_prefix: &str = if depth == 0 { "" } else { &full };
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" || sub.is_hide_set() { continue; }
        render_cmd(sub, sub_prefix, depth + 1, out);
    }
}

/// 参数与选项：每条一行，左侧对齐签名，右侧是作用说明。
fn render_args(cmd: &Command, pad: &str, out: &mut String) {
    let pos: Vec<&clap::Arg> = cmd.get_arguments()
        .filter(|a| a.is_positional() && !a.is_hide_set()).collect();
    if !pos.is_empty() {
        out.push_str(&format!("{}参数:\n", pad));
        for a in pos {
            if let Some(l) = arg_line(a) { out.push_str(&format!("{}{}\n", pad, l)); }
        }
    }
    let opts: Vec<&clap::Arg> = cmd.get_arguments().filter(|a| !a.is_positional()).collect();
    if !opts.is_empty() {
        out.push_str(&format!("{}选项:\n", pad));
        for a in opts {
            let long = a.get_long().unwrap_or("");
            let is_sub = crate::SUBITEMS.iter().any(|(_, ch)| ch.contains(&long));
            if a.is_hide_set() && !is_sub { continue; }
            if let Some(l) = arg_line(a) {
                let l = if is_sub { format!("  {l}") } else { l };
                out.push_str(&format!("{}{}\n", pad, l));
            }
        }
    }
}

/// 子命令一览：名字 + 一句话目的。
fn render_subs(cmd: &Command, pad: &str, label: &str, out: &mut String) {
    let subs: Vec<&Command> = cmd.get_subcommands()
        .filter(|s| s.get_name() != "help" && !s.is_hide_set()).collect();
    if subs.is_empty() { return; }
    out.push_str(&format!("{}{}\n", pad, label));
    for s in subs {
        let about = s.get_about().map(|a| a.to_string()).unwrap_or_default();
        out.push_str(&format!("{}  {:<12} {}\n", pad, s.get_name(), about));
    }
}

/// 格式行：取 clap usage，去掉 `Usage: ` 前缀并补全父命令路径。
pub(crate) fn usage_of(cmd: &Command, full: &str) -> String {
    let u = cmd.clone().render_usage().to_string();
    let u = u.lines().find(|l| l.contains("Usage:"))
        .map(|l| l.trim_start().trim_start_matches("Usage: ").to_string())
        .unwrap_or_else(|| format!("fqdt {}", cmd.get_name()));
    let u = if u.starts_with("fqdt ") { u } else { format!("fqdt {u}") };
    let name = format!("fqdt {} ", cmd.get_name());
    match u.strip_prefix(&name) {
        Some(rest) => format!("fqdt {full} {rest}"),
        None => u,
    }
}

/// flag 左列签名：`-r, --range <RANGE> `（尾随单空格）；无短名用 6 空格对齐。
pub(crate) fn arg_left(a: &clap::Arg) -> String {
    use clap::ArgAction;
    let mut left = String::new();
    match (a.get_short(), a.get_long()) {
        (Some(s), Some(l)) => left.push_str(&format!("  -{}, --{} ", s, l)),
        (None, Some(l)) => left.push_str(&format!("      --{} ", l)),
        (Some(s), None) => left.push_str(&format!("  -{} ", s)),
        (None, None) => left.push_str("  "),
    }
    let takes_val = !matches!(a.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count);
    if takes_val {
        if let Some(v) = a.get_value_names().and_then(|v| v.first()) {
            left.push_str(&format!("<{}> ", v));
        }
    }
    left
}

/// 单条参数行：`-r, --range <RANGE>   作用`；自动 help/version flag 跳过（hide 过滤在调用方）。
fn arg_line(a: &clap::Arg) -> Option<String> {
    use clap::ArgAction;
    if matches!(a.get_action(), ArgAction::Help | ArgAction::Version) { return None; }
    let left = arg_left(a);
    let help = a.get_help().or_else(|| a.get_long_help()).map(|h| h.to_string()).unwrap_or_default();
    let first = help.lines().next().unwrap_or("").trim().to_string();
    Some(format!("{:<36} {}", left.trim_end(), first).trim_end().to_string())
}

/// API 章节：api 命令格式与各 flag 作用 + 端点清单（与 ENDPOINTS 同步生成）。
fn api_section() -> String {
    let mut s = String::from(API_HEAD);
    for e in crate::fqapi::ENDPOINTS {
        s.push_str(&format!("  {:<4} {:<54} {}\n", e.method, e.path, e.note));
    }
    s.push_str(API_TAIL);
    s
}

/// 开篇约定与全局说明。
const CONVENTION: &str = "\
fqdt 命令参考 — 命令的格式与目的、各参数与端点的作用、底层 API 请求格式

  <尖括号> 必填参数 · [方括号] 可选参数 · | 分隔二选一
  全局选项，任何命令都可用: --timeout <秒> 请求超时 · -v 详细日志
  单条命令的完整帮助: fqdt <命令> --help

";

/// API 端点透传命令的格式与各 flag 作用。
const API_HEAD: &str = "\
━━━ API 请求 ━━━

fqdt api 把任意端点路径交给 fqapi 核心执行，自动携带签名与设备池，响应体原样写 stdout。
格式: fqdt api <路径> [-X 方法] [-q 键=值]... [-d 请求体] [--jq 路径] [--status] [--list]

  <路径>     端点路径，? 后可直接带 query；{xxx} 占位换真实值
  -X 方法    HTTP 方法，默认 GET，按端点要求可 POST/PUT
  -q 键=值   追加 query 参数，可重复；值需自行 URL 编码
  -d 请求体  POST/PUT 的 body 原文
  --jq 路径  按点号路径从响应取值，如 data.book_name；无匹配退出码 1
  --status   HTTP 状态码打到 stderr，stdout 只留数据
  --list     打印端点清单（方法/路径/必填参数/说明）

端点清单（未列入的路径同样可透传；fqdt api --list 输出同表）:
";

/// 端点清单之后：各命令实际发出的请求格式。
const API_TAIL: &str = "
━━━ 各命令底层的 API 请求格式 ━━━

fqdt search <关键词> -p N
  GET /api/v1/search?query=<关键词>&page=<N>&tab_type=1     综合搜索，tab_type 1综合 3小说 2听书

fqdt info overview <id>
  GET /api/v1/books/{book_id}/detail                       书籍详情：标题/作者/热度/简介
  GET /api/v1/books/{book_id}/directory                    章节目录，概览取前 N 章
  GET /api/v1/books/{book_id}/reviews                      书评，概览取前 N 条

fqdt info chapters <id> [-r 范围]
  GET /api/v1/books/{book_id}/directory                    全量目录含 item_id，本地按范围筛选

fqdt info intro <id>
  GET /api/v1/books/{book_id}/detail                       完整简介字段

fqdt info author <id>
  GET /api/v1/books/{book_id}/detail                       取 author_id，含 N_ 前缀原样使用
  GET /api/v1/authors/{author_id}                          作者资料
  GET /api/v1/authors/{author_id}/bookshelf                作者推荐书单

fqdt info comments <id>
  GET /api/v1/books/{book_id}/reviews                      书评列表

fqdt info cover <id>
  GET /api/v1/books/{book_id}/detail                       封面 thumb_url 字段

fqdt info relative <id>
  GET /api/v1/books/{book_id}/related                      相关推荐书单与杂项

fqdt info audio <id>
  GET /api/v1/books/{book_id}/tones                        本书可用 AI 音色集合

fqdt info recommend [--tab-type N] [--tab-index N] [--offset N]
  GET /api/v1/recommend/homepage?tab_type=&tab_index=&offset=   首页推荐流

fqdt info category [--tab N]
  GET /api/v1/category?category_tab=<tab>                  分类首页，tab 1男频 2女频 3出版 4听书

fqdt info folders / history
  POST /api/v1/account/fanqie/library|history              个人数据，需登录 Cookie

fqdt info progress
  GET /api/reader/book/progress                            网页阅读进度，需 Cookie；失败回退 POST library progress_list

fqdt info user
  GET /api/user/info/v2                                    昵称/uid/简介，需 Cookie
  GET /reading/user/basic_info/get/v                       粉丝/关注/已读统计，需 Cookie

fqdt info highlights
  GET /reading/reader/bookmark/personal_list/              划线书单，需 Cookie

fqdt read <id> [-r 范围]
  GET /api/v1/books/{book_id}/directory                    取目录与 item_id（TUI 逐章）
  GET /api/v1/chapters/{item_id}                           单章正文，n/p 切章时加载
  GET /api/v1/books/{book_id}                              书名（顶栏，失败回退 id）

fqdt download <id> [-r 范围]
  GET /api/v1/books/{book_id}/directory                    取目录与 item_id
  GET /api/v1/chapters/full?book_id=&item_ids=             批量正文，每 100 章一块
  GET /api/v1/chapters/{item_id}                           单章正文，批量失败逐章回退

fqdt update <已下载目录>
  同 download：先 directory 再补缺失章节，即增量更新

fqdt audio <id> [--real|--ai|--tts]
  GET /api/v1/books/{book_id}/tones                        音色列表与回退
  GET /api/v1/audio/play?item_ids=&book_id=                播放源，返回地址与解密 key
  GET /api/v1/chapters/{item_id}                           --ai/--tts 的文本来源

fqdt tts <本地txt> / fqdt audio --tts
  本地 edge-tts 合成，不请求音频端点；文本先经 download 同款正文端点取得

fqdt shelf list / add / remove / move / download
  GET /reading/bookapi/bookshelf/info/v:version/           云端书架一层与分组，需 Cookie
  POST /reading/bookapi/bookshelf/add/v|delete/v           加书 / 移出
  POST /reading/bookapi/bookshelf/update/v                 分组移动
  POST /api/bookshelf/multidetail                          批量补书名与当前章节

fqdt login [COOKIE] / --check
  GET /api/user/info/v2                                    验证 Cookie 是否有效

fqdt info overview <id> --add / shelf add <id>
  POST /reading/bookapi/bookshelf/add/v                    收藏到云端书架

fqdt api <路径>
  指定路径原样透传，不经过以上任何封装
";

/// 去掉 ANSI 序列（写文件须纯文本）。
fn strip_ansi(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() { break; }
            }
        } else {
            out.push(c);
        }
    }
    out
}
