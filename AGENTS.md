# fqdt — 番茄小说下载器

- **版本**: v0.26.0（`Cargo.toml` 中定义，每次变更更新）
- **目标**: 番茄小说离线下载 + 音频获取 + 语音合成
- **运行**: Android Termux (aarch64)
- **许可**: GPL-3.0（见 LICENSE）；核心部分源自 [Maicy0609/fanqie-lite-api](https://github.com/Maicy0609/fanqie-lite-api)（`crates/fqapi-core/`）
- **编译**: GitHub Actions musl 静态构建（x86_64 + aarch64 `x86_64/aarch64-unknown-linux-musl`，static-pie/静态链接 + strip）
- **核心原则**: 基本函数原子化 → 高阶函数组合 → 预编译工作流 → 配置文件可扩展

---

## 1. 架构愿景

### 1.1 三层架构

```
[基本函数]  ← 原子操作，每个做且只做一件事
     ↓ 组合
[高阶函数]  ← 接收函数参数，包装行为（重试/缓存/日志/进度/过滤）
     ↓ 编排
[工作流]    ← 预定义的函数调用序列，一条命令完成复杂任务
     ↓ 扩展
[配置驱动]  ← config.ini 可注册自定义函数和工作流
```

### 1.2 基本函数（Atoms）

每个基本函数满足：
- **一个函数一个职责**
- **纯输入→输出**，不处理 UI/日志/并发（这些由高阶函数包装）
- **返回 `Result<T, String>`**，调用者决定错误处理
- **上方有文档注释**：功能说明 + 被哪些工作流调用 + CLI 调用方式

| 分类 | 函数 | 输入 | 输出 | 被工作流调用 |
|------|------|------|------|-------------|
| **API** | `search_books()` | keyword, page | `Vec<Book>` | search |
|  | `fetch_catalog()` | book_id | `Vec<Chapter>` | download, update, info |
|  | `fetch_content()` | item_id | `String` | download_text, info --show |
|  | `fetch_audio()` | book_id, item_id, tone | `(String, String)` (URL, key_hex) | download_audio |
|  | `http_get()` | url | `String` | 以上所有 |
| **正文** | `download_text_chapter()` | Chapter, dir, fmt | `PathBuf` | download, update |
|  | `generate_epub()` | title, chapters, path | `()` | download (epub模式) |
|  | `append_epub_chapter()` | epub_path, Chapter, text | `()` | download (epub模式) |
| **音频** | `download_audio_chapter()` | Chapter, dir, tone | `PathBuf` | download_audio, update_audio |
|  | `compress_mp3()` | input, output, abr | `()` | download_audio |
|  | `post_process_file()` | input, output, cmd | `()` | download_audio |
| **LRC** | `gen_lrc_text()` | Chapter | `String` | download_audio, embed |
|  | `write_lrc_file()` | path, text | `()` | download_audio |
|  | `embed_lrc()` | mp3_path, Chapter, content? | `()` | download_audio |
| **TTS** | `edge_tts_speak()` | text, voice, params | `PathBuf` | audio --tts |
|  | `convert_tts_file()` | txt_path, params | `PathBuf` | audio --tts |
|  | `convert_tts_dir()` | dir_path, params | `Vec<PathBuf>` | audio --tts |
| **元数据** | `read_info_list()` | dir | `(book_id, title, fmt, chapters)` | update |
|  | `write_info_list()` | dir, metadata | `()` | download, update |
|  | `read_audio_info_list()` | dir | `(book_id, title, chapters)` | update_audio |
|  | `write_audio_info_list()` | dir, metadata | `()` | download_audio |
| **书架** | `load_bookmarks()` | — | `Vec<(id, title)>` | shelf |
|  | `save_bookmark()` | id, title | `()` | shelf add |
|  | `remove_bookmark()` | idx | `()` | shelf delete |
| **范围** | `parse_range()` | str | `Option<ChapterRange>` | download, update, info |
|  | `filter_by_range()` | chapters, range | `Vec<&Chapter>` | download, update, info |
| **工具** | `sanitize_filename()` | str | `String` | 全局 |
|  | `strip_html()` | html_str | `String` | fetch_content 后处理 |
|  | `chapter_heading()` | Chapter | `String` | dl_file, gen_lrc |
|  | `bar_style()` | — | `ProgressStyle` | 全局 |
| **透传** | `api_call()` | method, path, query, body | `ApiReply` | api |
| **提取** | `jq_extract()` | Value, 路径 | `Option<Value>` | api, discover |
|  | `jq_display()` | Value | `String` | api, discover |

### 1.3 高阶函数（Wrappers）

高阶函数接收一个基本函数作为参数，为它添加横切关注点：

```
fn with_verbose<F, T>(f: F, args, label) → T     // 调用 fn 并记录详细日志
fn with_cache<F>(f: F, args, cache_key, ttl) → T  // 缓存检查 → 调用 → 写入缓存
fn with_retry<F>(f: F, args, max_retries) → T      // 失败重试（指数退避）
fn with_progress<F>(f: F, items, concurrent) → T   // 并发执行 + 进度条
fn with_output<F>(f: F, args, format) → T          // 格式化输出
fn with_filter<F>(f: F, items, predicate) → T      // 先过滤再执行
fn with_elapsed<F>(f: F, args) → T                 // 计时并输出耗时
fn with_dry_run<F>(f: F, args) → T                 // 模拟执行，只打印不实际调用
```

**设计模式**：高阶函数不修改基本函数的签名，通过闭包包装：

```rust
// 高阶函数示例: with_retry
fn with_retry<T>(f: impl Fn() -> Result<T, String>, max: u32, label: &str) -> Result<T, String> {
    let mut last_err = String::new();
    for i in 0..max {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => { last_err = e; sleep(Duration::from_secs(1 << i)); }
        }
    }
    Err(format!("重试{}次失败: {}", max, last_err))
}

// 使用: 带重试的基本函数调用
let chapters = with_retry(|| api.fetch_catalog(bid), 3, "fetch_catalog")?;
```

```rust
// 更高阶: with_verbose + with_retry 可以链式组合
fn verbose_retry<F, T>(f: F, max: u32, label: &str, vb: bool) -> Result<T, String>
where F: Fn() -> Result<T, String> {
    let f1 = || { if vb { eprintln!("  [verbose] {} 开始", label); } f() };
    let result = with_retry(f1, max, label);
    if vb { eprintln!("  [verbose] {} 完成: {:?}", label, result.is_ok()); }
    result
}
```

### 1.4 工作流（Workflows）

工作流是预定义的函数调用序列。每个工作流：
- 是 `pub fn`，位于独立模块 `src/workflow/`
- 上方有**详细注释**：工作流流程、调用的基本函数、适用场景
- 不直接处理 `struct Client` 创建——由调用者传入

```rust
/// # 下载书籍工作流
///
/// ## 流程
/// 1. `api.fetch_catalog(book_id)` — 获取全本目录
/// 2. `filter_by_range(chapters, range)` — 按范围筛选
/// 3. `download::Downloader::run(chapters, concurrent)` — 并发下载正文
/// 4. `write_info_list(dir, meta)` — 写入元数据
/// 5. [可选] 如果 audio=true:
///    a. `api.fetch_catalog(book_id)` — 重新获取目录（已缓存）
///    b. `AudioDownloader::run(chapters, tone, abr, lrc)` — 并发下载音频
///    c. `write_audio_info_list(dir, meta)` — 写入音频元数据
///
/// ## 调用
/// - CLI: `fqdt download <book_id> [-r 范围] [-t txt|epub]`；音频续下见 fetch_and_dl_audio
pub fn workflow_download(api: &Client, book_id: &str, out: PathBuf, range: Option<&str>,
                         fmt: &str, ft: &str, concurrent: usize, force: bool, vb: bool,
                         audio: bool, tone: usize, abr: u32, lrc: &str, fallbacks: &[usize]) { ... }
```

预编译工作流清单：

| 工作流 | 命令 | 功能 |
|--------|------|------|
| `download::run` | `download` | 下载正文 |
| `update::run` | `update` | 增量更新（正文/音频） |
| `audio::run` | `audio` | 纯音频下载 |
| `search::run` | `search` | 搜索列表（`-D` 可续下载） |
| `audio::run_tts` | `tts` / `audio --tts` | TTS 文件/目录转换 |
| `shelf::run` | `shelf <子命令>` | 本地/云端书架管理 |
| `info::run` | `info` | 书籍概览/sections/个人数据 |
| `discover::recommend` | `info recommend` | 首页推荐流 |
| `discover::category` | `info category` | 分类列表 |
| `discover::comments` | `info <id> comments` | 书评 |
| `discover::author` | `info <id> author` | 作者资料 + 书单 |

### 1.5 基本函数的 CLI 暴露

`function` 子命令已于 **v0.24.0 删除**（用户判定 worthless，管道需求由 `api` 与既有命令覆盖）。
基本函数的 CLI 暴露途径：

- 端点级透传：`fqdt api <path>`（见 1.5.1）
- 业务封装：`search` / `info` / `download` / `audio` 等顶层命令（见 1.5.2）
- 正文获取：`fqdt api /api/v1/chapters/{item_id} --jq data.content` 直接吐正文明文

#### 1.5.1 `api` 子命令（端点透传）

`api` 将路径原样交给 `fqapi_core::dispatch`，**不含任何端点特定逻辑**——核心新增端点后 CLI 自动可用，无需改 fqdt 代码。

```sh
fqdt api --list                                  # 列出常用端点（fqapi.rs ENDPOINTS 单点定义）
fqdt api /api/v1/books/123 --jq data.book_name   # 默认 GET，响应体原样输出
fqdt api /api/v1/search -q query=诡秘 -q tab_type=3   # -q 追加 query（值需 URL 编码）
fqdt api /api/v1/books/123/directory --jq 'data.item_data_list[0].title'
fqdt api /api/v1/videos/1 -X POST -d '{"a":1}' --status
```

- `--jq`：内置 jq 子集（`a.b[0].c`、`a[].b[]` 展开），字符串去引号输出；`util::jq_extract`
- 输出约定：body 原样走 stdout（管道友好），日志/状态走 stderr；EPIPE 静默退出
- 业务错误（HTTP 200 + `code≠0`）按原文透传；HTTP ≥400 退出码 1

#### 1.5.2 封装子命令（发现/查询）

高阶工作流（`src/workflow/discover.rs`），默认彩色表格，`--json` 输出机器可读原文：

```sh
fqdt info recommend                     # 首页推荐流（16 本书）
fqdt info recommend --tab-type 25 --tab-index 0   # 2 推荐 / 25 小说 / 11 经典 / 5 听书
fqdt info category --tab 1              # 1 男频 / 2 女频 / 3 出版 / 4 听书
fqdt info recommend --json | jq .       # 机器可读
fqdt info <book_id> comments            # 书评（原顶层 comments 命令，v0.13.0 迁入 info）
fqdt info <book_id> author              # 作者资料 + 推荐书单（原顶层 author 命令，v0.13.0 迁入 info）
```

- **breaking（v0.13.0）**：顶层 `comments` / `author` 删除；**breaking（v0.24.0）**：顶层 `recommend` / `category` 删除并入 info——info 的 book_id 改 Option，无 id 且首参为二者时走 discover
- info 的 section：`chapters`（目录含 item_id）/ `intro`（完整简介）/ `author` / `comments` / `cover` / `relative`（原 `related` 更名，HTTP 路径 `/related` 不动）/ `audio`，无 id = `recommend` / `category` 流，无 section = 概览
- **个人数据 sections（v0.24.0，原 account 只读迁入）**：`folders` / `history` / `progress`（可 `--search`）/ `user` / `highlights`（需登录，`--json` 机器可读）
- 结构约定：`取 JSON → 字段提取 → 渲染管线`；新封装只需复用 `get/clip/pad/book_line` 辅助
- `rank` 端点只返回区块模板配置、无实体书单，故不封装，用 `fqdt api /api/v1/rank/<id>` 透传
- `author` 的 bookshelf 是作者**推荐书单**，非全部作品

#### 1.5.3 `serve` 子命令（HTTP 服务）

进程内启动 `fqapi_core` HTTP 运行时，直接暴露全部 API 端点：

```sh
fqdt serve                    # 默认 127.0.0.1:18921（回环）
fqdt serve --port 18999 --lan # 指定端口 + 监听局域网
```

- 实现在 `src/fqapi.rs::serve_at`（与 `ensure_runtime` 共用 `prep_runtime`：数据目录/默认 config.json/pool 密钥）
- `runtime::init` 幂等，成功与复用均返回 `"running"`，不可据此判重
- Ctrl-C 退出前调用 `fqapi::shutdown()` 刷盘设备池

### 1.6 配置驱动的扩展

`config.ini` 允许**不修改代码**添加自定义命令（现状仅 `[workflow_cmd]` 已实现；`[function]`/`[workflow]` 节为愿景，见待办 5）：

```ini
[function]
# 格式: name = command_template
# {} 会被 CLI 参数替换
decode_woff = "python3 ~/decode_woff.py {}"
fetch_raw = "curl -s 'https://api.example.com/raw?item_id={}'"

[function_arg]
# 可选: 定义参数名和默认值
decode_woff = "font_path,output"

[workflow]
# 格式: name = step1 | step2 | step3 ...
# 每一步是 function name 或 shell 命令
download_custom = "search $1 -D 1 | fetch-catalog {} | fetch-content {}"

[workflow_cmd]
# 注册为主命令（等价于 clap 子命令）
# 将出现在 `fqdt --help` 中
echo = "echo hello"
```

**实现方案**：CLI 解析时，先读 clap 静态命令，再读 `config.ini` 的 `[workflow_cmd]` 节，动态注入到匹配中。未识别的子命令回退到 config 查找。

---

## 2. 目录结构（目标状态）

```
crates/
└── fqapi-core/      ← vendor 自 Maicy0609/fanqie-lite-api（签名/设备池/端点）
src/
├── main.rs          CLI 入口 + 静态命令解析 + 配置命令注入
├── fqapi.rs         fqapi-core 运行时单例（数据目录/pool.key/init_core/dispatch 直调）
├── api.rs           Client 结构体 + 所有 HTTP 基本函数（fqapi 主通道 + 第三方降级）
├── audio.rs         AudioDownloader + 音频/封面/LRC 基本函数
├── download.rs      Downloader + 正文基本函数
├── config.rs        Config 解析 + 书架 + 配置驱动扩展
├── types.rs         数据结构定义
├── epub.rs          EPUB 生成函数
├── workflow/        工作流（mod.rs 调度 + 9 模块）
│   ├── mod.rs       模块导出
│   ├── download.rs  下载正文 / 目录模式 / 音频续下
│   ├── update.rs    增量更新
│   ├── audio.rs     纯音频 + TTS 入口
│   ├── search.rs    搜索列表 + 交互选书 pick
│   ├── info.rs      书籍概览 / sections / 读者资料渲染
│   ├── discover.rs  推荐流 / 分类 / 书评 / 作者
│   ├── shelf.rs     书架子命令分发（本地/云端）
│   └── account.rs   登录 / 云端书架读写 / 个人数据 sections
├── store.rs         运行时状态（cookie/书架/作者缓存/快照）
└── util.rs          工具函数 + 高阶函数 + 颜色常量
```

---

## 3. 注释规范

### 3.1 基本函数注释

每个 `pub fn` 基本函数必须带这样的注释：

```rust
/// 搜索小说
///
/// 调用 snssdk 官方搜索 API，返回匹配的书籍列表。
/// 结果按相关度排序，每页 10 本。
///
/// # 参数
/// - `keyword`: 搜索关键词（URL 编码自动处理）
/// - `page`: 页码（1-indexed）
///
/// # 被工作流调用
/// - `workflow_search_dl` — 搜索→选择→下载
///
/// # CLI 调用
/// - `fqdt search <keyword> -p <page>`
///
/// # 可选高阶函数包装
/// - `with_verbose` — 打印 API URL 和响应摘要
/// - `with_cache` — 搜索结果可缓存（TTL=300s）
/// - `with_retry(3)` — 搜索 API 偶有超时，重试 3 次
pub fn search_books(api: &Client, keyword: &str, page: usize) -> Result<Vec<Book>, String> { ... }
```

### 3.2 工作流注释

```rust
/// # workflow_download — 下载书籍（正文 + 可选音频）
///
/// ## 前置条件
/// - `book_id` 有效
/// - 如果 `audio=true`, 需要 `lame` 命令可用（或 `abr=0` 跳过压缩）
///
/// ## 工作流图
/// ```
/// fetch_catalog(book_id)
///     │
///     ▼
/// filter_by_range(chapters, range)
///     │
///     ├──→ [正文] Downloader::run(chapters)
///     │       │
///     │       ▼
///     │   write_info_list(dir)
///     │
///     └──→ [音频] fetch_catalog(book_id)  ← 缓存命中则跳过
///             │
///             ▼
///         AudioDownloader::run(chapters)
///                 │
///                 ├──→ compress_mp3 (if abr>0)
///                 ├──→ post_process_file (if cmd not empty)
///                 └──→ handle_lrc (gen + embed)
/// ```
pub fn workflow_download(...) { ... }
```

---

## 4. 编码约定

### 4.1 命名

- 基本函数：`动词_名词()`，如 `fetch_catalog()`、`compress_mp3()`、`write_lrc_file()`
- 高阶函数：`with_动词()`，如 `with_retry()`、`with_progress()`
- 工作流：`workflow_名词()`，如 `workflow_download()`
- 结构体：`Downloader`、`AudioDownloader`、`Client`、`Config`、`ChapterRange`
- 变量：`snake_case`，避免缩写（除 `api`、`cfg`、`ch`、`idx` 等非常通用的）

### 4.2 函数长度

- 基本函数：**≤30 行**（超出则再拆分）
- 高阶函数：**≤20 行**
- 工作流：**≤60 行**（超出则拆为子步骤）

### 4.3 Option 参数

所有可选行为通过 `Option<T>` 参数控制，不通过全局变量或配置隐式改变：

```rust
// ✅ 正确：显式 Option 参数
pub fn compress_mp3(input: &Path, output: &Path, abr: Option<u32>) -> Result<(), String> {
    let rate = abr.unwrap_or(0);
    if rate == 0 { return Ok(()); }  // abr=None 或 0 表示不压缩
    // ... 实际压缩逻辑
}

// ❌ 错误：隐式依赖 cfg
pub fn compress_mp3(input: &Path, output: &Path) -> Result<(), String> {
    let cfg = Config::load();  // 不要这样！函数应该是纯的
    // ...
}
```

### 4.4 日志开关

- 全局 flag：`fqdt -v <子命令>`（clamp `global=true`，可置于任意位置）开启详细日志
- core 日志默认**不打印**（仍落盘 `rust.log`）；`-v` 时 stderr 镜像（`core_log::set_console`）
- stdout 永远只放数据，stderr 放日志与诊断（详见 TUI.md）

### 4.5 高阶函数模式

高阶函数的基本模式是接收闭包，包装行为：

```rust
/// 带重试的高阶函数
///
/// 接受一个返回 `Result<T, String>` 的闭包，在失败时自动重试。
/// 可用于任何基本函数（API 调用、文件写入等）。
pub fn with_retry<T>(f: impl Fn() -> Result<T, String>, max: u32) -> Result<T, String> {
    let mut last_err = String::new();
    for i in 0..max {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => { last_err = e; if i + 1 < max { sleep(Duration::from_secs(1 << i)); } }
        }
    }
    Err(format!("重试{}次后失败: {}", max, last_err))
}

/// 使用示例：将高阶函数应用到基本函数
fn do_search(api: &Client, kw: &str, page: usize) -> Result<Vec<Book>, String> {
    with_retry(|| api.search_books(kw, page), 3)
}
```

```rust
/// 带进度条的高阶函数
///
/// 将闭包应用到 items 的每个元素上，显示进度条。
/// closure 签名: `Fn(&T) -> Result<U, String>`
pub fn with_progress<T, U>(items: &[T], f: impl Fn(&T) -> Result<U, String>,
                           label: &str) -> Vec<Result<U, String>> { ... }
```

### 4.6 导入顺序

```rust
// 标准库
use std::fs;
use std::path::{Path, PathBuf};

// 外部 crate
use indicatif::{ProgressBar, ProgressStyle};

// 内部模块
use crate::api::Client;
use crate::types::Chapter;
```

---

## 5. 接口设计

### 5.1 基本函数的暴露方式

基本函数不再有独立 CLI 映射（`function` 子命令 v0.24.0 已删）。暴露方式：

- **端点函数**（`api.rs`）→ `fqdt api` 透传 + 各封装命令内部调用
- **原子函数**（`audio.rs`/`download.rs`/`epub.rs`）→ 仅工作流内部调用，CLI 不直出
- 新增基本函数时：补文档注释（3.1 模板，写明被哪些工作流调用），**不**再加 CLI 变体

### 5.2 工作流参数设计

每个工作流函数接收 `WorkflowParams` 结构体而不是散落的参数：

```rust
struct DownloadParams {
    book_id: String, output: PathBuf, range: Option<String>,
    format: String, concurrent: usize, force: bool,
    audio: bool, tone: usize, abr: u32, lrc: String,
}

fn workflow_download(api: &Client, p: DownloadParams, cfg: &Config) { ... }
```

---

## 6. API 探索指南

> ⚠️ **重要**: 以下 API 信息是截至 v0.4.0 已知的。**你应该自行探索是否还有新的接口可用**。番茄小说（字节跳动）的 API 经常变化，第三方 API 服务器也可能增减。

### 6.1 已知 API

| 用途 | URL | 来源 | 状态 |
|------|-----|------|------|
| 搜索 | `novel.snssdk.com/api/novel/channel/homepage/search/search/v1/?aid=1967&q={}&offset={}` | 官方 | ✅ 稳定 |
| 搜索备用 | `101.35.133.34:5000/api/search?key={}&offset={}` | 第三方 | ✅ 可靠 |
| 目录 | `fanqienovel.com/api/reader/directory/detail?bookId={}` | 官方 | ✅ 稳定 |
| 正文 | `101.35.133.34:5000/api/content?tab=小说&item_id={}` | 第三方 | ✅ 首选 |
| 正文备用 | `tt.sjmyzq.cn/api/raw_full?item_id={}` | 第三方 | ⚠️ 偶有5xx |
| 正文备用2 | `101.35.133.34:5000/api/raw_full?item_id={}` | 第三方 | ✅ 可靠 |
| 音频 URL | `101.35.133.34:5000/api/content?tab=听书&item_id={}&tone_id={}` | 第三方 | ✅ 稳定 |
| 音频文档 | `101.35.133.34:5000/docs` | 第三方 | ✅ 有完整文档 |

### 6.2 探索方向

探索以下可能性：
1. **官方 WEB 端 API** — 检查 `fanqienovel.com` 的 Network 请求，寻找未文档化的 JSON 接口
2. **第三方服务器 `/docs`** — `101.35.133.34:5000/docs` 可能有新的端点
3. **`tt.sjmyzq.cn`** — 尝试其 `/api/` 下其他路径
4. **章节推荐 API** — 有些服务器提供相关书籍推荐
5. **书籍详情 API** — 获取封面图 URL、分类、标签等元数据
6. **用户书架 API** — 官方可能提供用户收藏列表接口
7. **批量内容 API** — 一次请求返回多章内容，减少 HTTP 开销

探索时注意：
- 使用 `curl -v` 查看完整响应头
- 检查 `Set-Cookie` 看是否需要鉴权
- 响应 JSON 的结构可能有新字段
- 第三方 API 可能有使用限制/频率控制

### 6.3 音频音色

已知 Book 7185478854738185271 可用音色：
```
1, 2, 4, 5, 6, 74, 91
```

**探索**: 这是否是所有书籍通用的？尝试用 `tone_id=100`+ 看是否返回更多。检查服务器是否有 `/api/voices` 或 `/api/tones` 端点。

---

## 7. 当前状态

### 7.1 已完成

| 模块 | 功能 | 状态 |
|------|------|------|
| **API 层** | fqapi-core 进程内直调（search/catalog/content/audio/detail，签名+设备池） | ✅ v0.6.0 |
|  | 搜索/目录/正文/音频 API 调用 + URL 轮换（第三方，降级备选） | ✅ |
|  | HTTP 三模式（auto/minreq/curl） | ✅ |
|  | 文件缓存（TTL 控制） | ✅ |
|  | HTML 标签剥离 | ✅ |
| **正文下载** | 并发下载（线程池 + round-robin） | ✅ |
|  | EPUB 生成（zip 库，零系统依赖） | ✅ |
|  | info.list 读写 | ✅ |
|  | 目录模式增量更新 | ✅ |
|  | `--force` 覆盖控制 | ✅ |
| **音频下载** | 并发音频下载 + 音色回退 | ✅ |
|  | CENC (cenc-aes-ctr) 解密（fqapi 返回 key_hex 时自动解密） | ✅ v0.6.0 |
|  | ABR 流式压缩（lame pipe） | ✅ |
|  | 后处理命令模板 | ✅ |
|  | LRC 生成 + ID3v2 USLT 嵌入 | ✅ |
|  | 封面嵌入（APIC, jpg/png） | ✅ |
|  | Audio/info.list 读写 | ✅ |
| **TTS** | edge-tts 文件/目录转换 | ✅ |
|  | 参数化（rate/volume/pitch） | ✅ |
| **CLI** | 分组顶层 help + 子命令（search/info/download/audio/update/shelf/tts/login/config/serve/api/version） | ✅ v0.24.0 |
|  | `api` 端点透传（--list/--jq/-X/--data，46 端点直通） | ✅ v0.7.0 |
|  | 封装子命令（recommend/category/comments/author，`--json`） | ✅ v0.8.0 |
|  | `serve` HTTP 服务模式（--port/--lan） | ✅ v0.9.0 |
|  | 简体中文帮助 | ✅ |
|  | 彩色终端 + 进度条 | ✅ |
| **配置** | INI 解析 + 书架 | ✅ |
| **CI** | GitHub Actions cross 编译 | ✅ |

### 7.2 待办（按优先级）

1. **⚡ 代码拆分** — 将当前 `download()`、`update()`、`audio_dl()` 等大函数拆成基本函数
2. **⚡ 高阶函数提取** — 实现 `with_retry`、`with_cache`、`with_progress`、`with_verbose`、`with_filter`、`with_output`
3. **⚡ workflow 模块** — 创建 `src/workflow/`，将预编译工作流从 main.rs 移出
4. **📦 function 子命令扩展** — 为每个基本函数添加 `FunctionCmd` 变体（已废弃：function 于 v0.24.0 删除）
5. **📦 配置驱动扩展** — 实现 `[function]`、`[workflow]`、`[workflow_cmd]` 节
6. **📦 高阶 verbose** — 用 `with_verbose` 替换所有 `if verbose { ... }` 散落代码
7. **🔍 API 探索** — 探索是否有新的官方/第三方 API 端点
8. **💡 智能 `--force`** — 内容哈希比对，相同则跳过
9. **💡 Rust 原生 lame** — 用 `lame-rs` 消除外部依赖
10. **🧹 clippy 清理** — 解决 31 个 warnings
11. **⚡ vendor fqapi-core** — 从 fanqie-lite-api 复制 `crates/fqapi-core/` 入仓，fqdt 改 workspace，reqwest 对齐 0.13 ✅
12. **⚡ 运行时单例** — `src/fqapi.rs` 提供 `ensure_runtime()`，数据目录 `~/.local/share/fqdt/`（pool.key/设备池/媒体缓存） ✅
13. **⚡ 主通道切换** — search/info/catalog/download/audio 改走 fqapi 进程内直调；第三方 URL 源降级为备选（fqapi 失败时 fallback），源轮换逻辑保留给降级路径 ✅
14. **⚡ `fqdt api` 透传** — gh api 式：46 端点全覆盖，`-X`/`--data`/`--jq`，默认 JSON 管道友好 ✅ v0.7.0
15. **📦 封装子命令扩展** — recommend/category/comments/author 表格+`--json`（`src/workflow/discover.rs`）；rank 响应无实体书单，改走 `fqdt api` 透传；**不做漫画/短剧封装** ✅ v0.8.0
16. **📦 `fqdt serve`** — 对外 HTTP 服务模式（复用 `runtime::server` + `ListenMode`），默认端口 18921，`--lan` 监听局域网，Ctrl-C 退出刷盘 ✅ v0.9.0
17. **🧹 LICENSE** — 添加 GPL-3.0 全文 + AGENTS 注明核心 fqapi-core 源自 Maicy0609/fanqie-lite-api ✅ v0.9.0
18. **📦 部署自动化** — 每次版本更新（tag v*）将 aarch64 musl 二进制推送到 47.95.13.125:48022 的 `~/cmd/fqdt`（CI release job ssh/scp，deploy key 走 GH secret） ✅ v0.8.0（run 41776280009）
19. **🐛 静态构建系统 CA 修复** — 静态 musl 二进制无系统证书，reqwest 默认 native-certs 在 Android panic（`No CA certificates were loaded`）；`src/api.rs::tls_builder` 用内置 webpki 根证书预配置 TLS（同 core `build_tls_config`）✅ v0.9.0
20. **🐛 静态构建系统 DNS 修复** — 静态 musl 的 getaddrinfo 只读 `/etc/resolv.conf`，Android 上不存在 → 所有域名解析失败（`dns error: Try again`，device_register 等全部走不通）；新增 `upstream/dns.rs` 进程内解析（`FQDT_DNS` env → `getprop net.dns1/2` → `/proc/net/route` 网关 → 223.5.5.5/119.29.29.29 兜底，手写 UDP A 查询零依赖），挂到 core HttpTransport、`streaming_client`、fqdt `tls_builder` 三处 client ✅ v0.9.1
21. **🎨 输出精简 + 日志开关** — core 日志（`[I] core: ...`）默认不打印（仍落盘 rust.log），全局 `-v/--verbose` 才开 console 镜像（子命令级 -v 改为全局 flag）；search 列表 4 行/本 → 1 行/本（去简介/空行），下载总结合并单行，ANSI 样式常量集中到 util（TUI.md 设计规范落地） ✅ v0.10.0
22. **🐛 下载慢/章节失败修复** — 四个根因：① DNS 探测链把 WSL 网关当 nameserver（双重重试最坏 12s/主机，api 单请求 25s）→ `dns.rs` 改 `FQDT_DNS` 短路 + `/etc/resolv.conf` 优先 + 去 `lookup_a` 外层重试（search 13.7s→1.8s）；② `get_json_timeout` 走 `runtime::request` bridge 挂死 → 改 `call()` dispatch + `tokio::time::timeout`；③ core batch 返回 object map 而 `parse_batch` 只吃数组 → 兼容两形态并跳过 `content="Invalid"`；④ 无熔断+全量 item_ids 拼单 → 100/块 JoinSet 分块、`source_dead` 熔断（ok=0 且 fail≥2）、reqwest 8s/内容 retry 2/单章 10s 超时、stats 按模板记键使 sort_urls 生效、第三方源全灭时给出明确错误。冒烟：10 章 `-j 4` 20.5s 全成功（修复前 3 章 1m52s 全败）。✅ v0.11.0
23. **🎨 全子命令 UI 重写** — 按新版 tui-design-pro skill（web 调研 clig.dev/TUIkit/Tufte/VT-HIG 等）重写全部子命令输出：过程进度一律 `util::progress/clear_progress`（stderr 仅 TTY）、结果收据单行（`ok N 章 · 跳过 x 章`/`新增 N 章 · 已有 x/y 章`）、失败用 `util::print_failures`（≤3 条+计数+重跑提示）、提示/交互全部 stderr、`Color` 改 Display 静态 + `init_color()`（NO_COLOR/CLICOLOR=0/非 TTY 纯文本）、info show 40 行折叠+`function fetch-content` 逃生口、shelf 尾注单行、discover 书评单行。TUI.md 已同步。✅ v0.11.0
24. **🐛 batch 端点对匿名设备无效 → full mget 替代** — `batch_full/v` 对匿名设备恒回 `content="Invalid"`（book_id 注入/`req_type=0`/去 `novel_text_type` 三变体均无效）。**已解决（v0.13.1）**：`core_content_batch` 首选 `/api/v1/chapters/full`（novelfm playerapi mget，匿名可用，`data.item_infos.{id}.content` 已解密明文，单次≤3000章），失败才回退 batch_full；`parse_batch` 兼容 `data.item_infos` 形态。实测 10 章 4.9s（逐章 20.5s，4 倍）。✅ v0.13.1
25. **📦 audio 三模式 + search 纯搜索 + download 关键词** — ① `audio` 新增 `--real`（真人讲书，`resolve_audio_book` 经 related_audio_bookids 选 book_type=1 且 listen_count 最大的真人主版）/`--ai`（智能朗读，related_novel_bookid 转回文字版后按 `--tone` 取 AI 音色）/`--tts`（Edge-TTS：先下正文 txt 再 `run_tts` 合成）/`--voice`，real/ai/tts 互斥；② `search` 改纯搜索（`print_list` 一行一本组织化展示 + 尾注指引，不再交互选中，`-D N` 保留自动下载）；③ `download` 接受书名关键词（`workflow::search::pick` 交互选序号，非 TTY 报错指引，选中自动加书架）。edge-tts 需 `pip install --user --break-system-packages edge-tts` 且 PATH 含 ~/.local/bin。✅ v0.12.0
26. **📦 info section 扩展（breaking）** — `fqdt info <book_id> [SECTION]`：`chapters`（目录含 item_id）、`intro`（完整简介）、`author`（id 取 detail `author_id` **原样含 `N_` 前缀**，裸数字会错拿他人）、`comments`（count=20）、`cover`（thumb_url）；无 section=概览+sections 尾注。**顶层 `comments`/`author` 命令删除**（discover.rs 函数保留由 info 调用）；hots/shelf 用户暂缓。✅ v0.13.0
27. **📋 info 概览默认行为改版** — 默认 `info <id>` 不再全量列章节：标题/作者 + 热度行（`read_cnt_text`·评分·收藏，评分与收藏是 JSON 字符串需 parse）+ 简介截断 + 前 N 章（超出尾注跳 chapters）+ 前 N 条书评；条数与开关由 config.ini `[info]` 节控制（`info_top_chapters=10`/`info_top_comments=3`/`info_hot=true`/`info_intro_len=120`，0=关闭该项），不同 action 全可配。完整列表仍在各自 section。`[x]` ✅ v0.15.0
28. **📦 下载增强批次** — ① EPUB O(n) 重构：删 `generate`+`update_chapter`（旧每章全量重写 zip，1928章≈7GB写放大），新 `epub::write_book(title, &[(Chapter, Option<String>)], path)` 单遍一次落盘；`do_epub` 改批量取正文（fetch_content_batch + Arc<Mutex<HashMap>> 收集，失败章占位"获取失败"）。② 进度系统：`bar_style` 加 `{spinner} {percent}` + braille tick_chars；`progress()` 升级 braille 动画线程（120ms，非TTY静默）；pb `enable_steady_tick(120ms)`；阶段文案（启动 core/批量获取正文/批量获取音频信息/生成 EPUB 等）。③ 音频 playinfo 批量：`fetch_audio_batch`（逗号多 id 单请求，上游行按请求顺序对应、行内 item_id 统一填末 id 不可用→zip 匹配），`dl_one` 主音色查 `pre` 表兜底逐章；实测 3/3 命中。④ 发版清理规范 AGENTS 8.2.1。`[x]` ✅ v0.14.0
29. **🎨 help 重整 + info related/audio + 色彩分层 + config 子命令** — ① help：全子命令 doc/flag 重写（search 去"并下载"、--tone 1-91 说明、flag 可操作语义），`HELP_TAILS`+`apply_help_tails` 底部 gh 式（配置段恒开、示例受 `ui.help_examples=false` 关闭），顶部注明 flag>config 优先级，简介绍不枚举子项；行为变更当次同步 help。② info 新增 `related`（相关书 CYAN/MAGENTA/BLUE + misc `真人听书版 N · AI 音色 N`）与 `audio`（AI 音色列表 + 听书版主播阵容）。③ 色彩 8 色语义分层（util 加 GREEN/BLUE/RED；ok 绿/err 红/#id 蓝/热度黄），TUI.md §2/§2.1 已同步。④ git 式 `fqdt config --list/<key>/<key> <value>/--unset`（30 键点号映射表 KEYS、ini_write 保留注释、EPIPE 走 emit_line）；新增 `ui.help_examples` 键。`[x]` ✅ v0.16.0
30. **📖 逐 flag 详情 help + login/account + related 杂项** — ① help 逐 flag 详情：`fqdt audio --tone --help`、`fqdt help audio --lrc`、`fqdt download -r --help`、`fqdt config tone --help` 四场景拦截渲染格式化详情（`FLAG_HELP` 表 main.rs + `CONFIG_HELP` 表 config.rs；flag 短 help 去括号长注释，内容移详情）；`--tone 1 --help` 走 clap 原生子命令 help。② `fqdt login [COOKIE]`（--check/--clear，cookie 存 `~/.config/fqdt/cookie` 独立文件防 --list 泄露；上游无账号密码登录，登录态=浏览器复制 Cookie）。③ `fqdt account [shelf|folders|history|progress|user]`：core account 端点封装（cookie 鉴权只读），info 保持 book 语义不塞个人数据；ENDPOINTS 补两条 account。④ info related misc 行扩展杂项（字数/章数/分类/听书量/起书时间）。`[x]` ✅ v0.17.0
31. **🔍 网页端点逆向 + account user/highlights 真资料** — 不依赖上游现成端点，自行逆向探索（来源 fysh1010/mcp-server-fanqie、naiyQAQ/fanqie-assistant、hesan1232/fanqie.koplugin）：① `api.rs` 加 `http_get_cookie`（共享 web client：webpki+进程内 DNS，Cookie 走 header）；② `account user` 从"能力说明"升级为真实资料——`/api/user/info/v2`（昵称/id/简介）+ `/reading/user/basic_info/get/v`（粉丝/关注/已读本数/累计分钟/是否作者）；③ `login --check` 只信 user/info/v2（实测假 Cookie 下 core library 也回 code:0，回退会误报成功，已去回退）；④ 新 section `account highlights`——`/reading/reader/bookmark/personal_list` 有划线的书（划线/书签/笔记计数；**遗留：未登录实测回 101003 PARAM_INVALID，盲试参数无解，待真 Cookie 验证**）。`[x]` ✅ v0.18.0
32. **🔍 info 智能 ID 识别** — `fqdt info <id>` 自动识别四类 ID（信号全部实测）：① `_` 前缀（N_/2_）→ 作者直达 `discover::author`；② 纯数字依次探测 detail（code0+book_name=书，章节 id 回 101109 BOOK_REMOVE）→ items（有 item_id+title=章节，返回章节+所属书全量：title/book_id/order/word_number/tones/abstract）→ authors（is_author=true=作者；user_name 非"用户{id前8位}"占位=读者 uid 公开资料卡；占位=无效）；③ 章节卡片含所属书跳转指引、`--show` 正文预览（40 行折叠）；④ 无效 id 友好报错列支持形态。**遗留：真实读者裸 uid 无样本（占位保护已备），待真机验证**。`[x]` ✅ v0.18.0
33. **📦 account shelf 网页通道 + 书架增删** — ① `account shelf` 升级网页端点（`/reading/bookapi/bookshelf/info/v` + `/api/reader/book/progress` + `POST /api/bookshelf/multidetail` 合并）：每行显示书名/**当前章节**/进度%（real_chapter_order/serial_count 优先，read_progress 万分比兜底，koplugin 算法），失败回退 core 通道；② 书架写操作：`fqdt account --add <book_id>` / `--remove <book_id>`（`POST /reading/bookapi/bookshelf/add/v|delete/v`，body `identify_data`，毫秒 modify_time）+ `--check <book_id>`（`GET .../check/v`，data=1/0），三 flag clap 互斥；③ api.rs 新增 `http_post_cookie`，account.rs 抽 `page_check`/`page_post` 统一判定。逆向源 naiyQAQ/fanqie-assistant bookshelf.ts + hesan1232/fanqie.koplugin bookshelf.lua；远端真 cookie 实测：info/progress/multidetail/simple-info/check/add/delete 全通、写端点往返（check1→add→check1→delete→check0）。**遗留：作者列**（网页无批量作者端点：simple/info/multidetail 均无 author，`/api/v1/books/{id}` 网页域回 HTML，逐本 core detail N 请求成本高暂不做）。`[x]` ✅ v0.19.0
34. **📦 书架整理三连（info 收藏 / shelf 过滤 / 分组移动）** — ① `fqdt info <id> --add`：概览展示后直接收藏（复用 shelf_op add）；② `fqdt account shelf --search <词>`：书名本地模糊过滤（147 本大书架可用性，收据显示 `n/总数`，--json 同步过滤）；③ `fqdt account --move <book_id> --to <group>`：分组移动（`POST /reading/bookapi/bookshelf/update/v?aid=1967`，body `book_data[{asterisked,book_id,book_type,group_name,has_shown,is_pin,modify_time}]`，两 flag clap 互 requires，**实测 group_name 为目标组名、组不存在服务端自动建组**（响应 `new_booklist_name_id_map`），测试书 Reading→Done→Reading 往返通过）；④ help 示例段补 account 写操作与 info --add。**遗留：account 参数改 `AccountArgs` struct 收拢（7 参数签名爆炸）**。`[x]` ✅ v0.20.0
35. **📦 shelf 分组视图 + 作者列 + highlights 修复** — ① shelf 按 `group` 分节渲染（`【组名】` 分区头，TUI.md §5，保持首次出现顺序，多组时尾行加 `· N 组`）；② 作者列：`fqdt account shelf --author` 才拉——core detail 第二段（`书名|作者|...`）逐本并发（JoinSet 分批 10、8s 超时降级不带作者），本地缓存 `~/.config/fqdt/authors.json`（{id:{a,t}}，TTL 30 天，config.rs `authors_path()`），行内 `CYAN书名 MAGENTA作者`；③ highlights 101003 **结论：非参数问题**——真 cookie 下现有 URL 直接 `code:0`（21 本划线，结构 `{bookmark_num/underline_num/notes_num, data:{book_id,book_name,author}}` 与 render_highlights 已匹配），101003 是未登录态的登录检测，**零代码改动，仅需真机验证**。`[x]` ✅ v0.21.0
36. **📦 统一存储 store.json** — 运行时状态收进单一 JSON（`~/.config/fqdt/store.json`，0600，原子写 tmp+rename）：① cookie（{value,saved_at}，替代纯文本 cookie 文件）；② 本地书架 bookmarks（替代 `books.txt` 的 `id:title` 行格式）；③ 作者缓存 authors（替代 authors.json）；④ **shelf 快照**（{books,fetched_at}，TTL 600s——离线可看上次书架+秒开，网络成功时刷新）。新模块 `src/store.rs`：`load()/save()` 首启自动迁移旧格式（旧文件改 `.bak` 备份），损坏时回退默认不崩溃。**不收**：config.ini（人手编+git式子命令已成熟）、cache/（性能层）、info.list（跟书目录走）。收益：单点管理、权限统一、后续新状态字段直接加 key。`[x]` ✅ v0.22.0
37. **📦 account progress 网页化** — `fqdt account progress` 从 core `progress_list` 升级网页 `GET /api/reader/book/progress`（实测 26 本在读，字段 `book_id/item_id/read_progress万分比/read_timestamp秒/genre_type/index`）：按 `read_timestamp` **降序时间线**渲染 `MM-DD HH:MM 书名 · 章节 · pct% #id`（书名/章节优先取 store shelf 快照 map，缺的书 multidetail 批量补）；网页失败回退 core 通道。收益：阅读时间线（shelf 无时间维度）。`[x]` ✅ v0.23.0
38. **🎨 help 全面去括号 + tone 音色标注** — ① 所有面向用户的 help（clap doc/`help=`、FLAG_HELP/CONFIG_HELP 详情、HELP_TAILS）删除括号及其内容（用户明确不要括号注释；纯代码注释与运行输出不动）；② `--tone` 取值行标注数字→音色名（权威端点逆向：`GET /reading/bookapi/audio/toneinfo/?book_id={id}&aid=1967` 无需登录，`data.tts_tones[] {id,title,description}`，逐书可用集不同）；③ `info <id> audio` 升级带名渲染（tone_names 端点失败回退 core id 列表）。`[x]` ✅ v0.23.0
39. **🧹 子命令去重精简** — 删 `get`（hide=true 兼容残留，=download+audio+update 合体；变体/match 臂/dispatch_get 全清，别名与 HELP_TAILS 本无引用）。**修正决策：`tts` 保留**——`fqdt tts <本地txt>` 与 `fqdt audio --tts <book_id>` 输入源不同非重复。`shelf`（本地）vs `account shelf`（云端）、`function` vs `api`（管道 vs 透传）语义不同均保留。`[x]` ✅ v0.23.0
40. **📖 TUI 阅读器** — `fqdt read <book_id>` 真 TUI 连贯阅读一整本书（章节切换/j-k 滚动/进度记忆进 store/与 download·shelf·account 联动）。**UI 层直接抄现成实现**（用户明确要求：自己写容易出各种问题）——调研成熟 Rust TUI 阅读器/阅读器类开源项目（Ratatui 生态）抄其布局与事件循环，业务层接 fqdt fetch_content。备选：调用外部 pager（less）降级。`[ ]`
41. **📦 account 列表搜索扩展 + 机器可解析输出** — ① `--search` 从 shelf 扩到 progress/history/folders（新 `match_item`：book_name/title/author/chapter 小写包含；render_section/render_progress 带 search 参数，json+search 出过滤数组，尾行 `{n}/{total} 项`；render_shelf 匹配统一用 match_item 后作者/章节也能搜）；② 机器解析指引：HELP_TAILS account 第二列 `解析: fqdt account <section> --json | jq`，`--search` doc 更新覆盖范围。`[x]` ✅ v0.23.0
42. **📐 shelf/account/info 结构重构** — ① 云端书架番茄原生一层：顶层只列**未分组书 + 分组条目**（已分组的书不直接显示，显示整个分组）；`shelf list [组名]` 无参显示第一层、指定组名显示该组内所有书；② 顶层 `fqdt shelf` 子命令化：**裸输入 shelf 直接报错要求输入子命令**（现有 -a/-d/-D/-U flag 改子命令），`list` 为书架入口——未登录显示本地书架，已登录显示云端、加 flag 显示本地书架；③ account 只读功能（folders/history/progress/user/highlights，**不含 shelf**）迁入 `fqdt info` 作为 subcommands；account sections 也从位置参数变 subcommands；④ 未来预留：查看类子命令加 flags（如 `dump` 打印到终端）、`shelf view <路径|书名>`（路径或书名在书架内搜索查看，暂不做，设计留口）。`[x]` ✅ v0.24.0
43. **✏️ related → relative 更名** — `fqdt info <id> related` 子命令改名 `relative`（main.rs help/FLAG_HELP --real 查询行、info.rs SECTIONS/match/fn `related`→`relative`、`related_misc`→`relative_misc` 同步改；HTTP 路径 `/related` 与 API 字段 `related_*` 不动；AGENTS 1.5.1 section 表已同步）。`[x]` ✅ v0.25.0
44. **🔊 audio 目录规则** — 下载音频/保存 tts 落点（已定案并验证）：**`fqdt audio <book_id>`（直接书籍 ID，含关键词选书）→ 不建 Audio 子文件夹**，mp3 直接落目标目录（`-o dir` → dir 根；无 -o → output_dir 根）；**`audio --tts` 无 -o（存储书籍目录）→ 仍建 `<out>/Audio/`（现状）**，`-o dir` → 直接落 dir 根；**书籍目录场景不变**（`audio <目录>`、`update` → path/Audio）。改动点：dispatch_audio out 计算去 join("Audio")、tts 分支 run_tts 传 output.as_deref()、download.rs:56 按 p.output 条件 join（该分支现为不可达死代码，download 无 --audio flag；main.rs 旧997 function read-audio-info 已随 function 删除消失）。六场景隔离实测通过。`[x]` ✅ v0.25.0
45. **🐛 进度条双行 bug** — 已定位修复（script pty 字节流证实）：进度行（Braille 帧 / indicatif pb）不以换行结束，**进度期间的插入输出（-v core_log 镜像、verbose eprintln）直接拼在进度行尾部**，其 `\n` 把光标推到下一行 → 下一帧与 `clear_progress` 的 `\x1b[2K` 都作用于新行，旧进度行从未被清 → 残留双行。修复：① util.rs 新增 `pub fn note(line)` 统一出口（ACTIVE_PB → pb.println 自带清行重画；Braille 运行中 → PAUSE 暂停协议擦行后输出；否则 eprintln），`with_verbose_async`、core_log 镜像（fqapi_core `set_console_hook`，main.rs 注册）、进度期间的 eprintln 全部走 note（download.rs 3 处 / api.rs 9 处 / src/audio.rs 15 处）；② pty 实测 -v download 与 audio 双场景“帧后直接拼日志”=0、收据前均 `[2K` 擦行；③ TUI.md §2/§8 已加纪律。`[x]` ✅ v0.25.0
46. **🧹 输出整洁（用户：别 tips 乱信息）** — ① 结构重构落地后 `fqdt account` 移除**不打任何"已移除/请改用 xxx"迁移提示**（直接走未知命令 err 或隐藏兼容）；② 各命令运行输出删指引类 tips 尾行（能力:/查询:/获取:/查看:/验证: 等 DIM 行），错误一行一记录为主，仅未登录/未设置 Cookie 保留单行指引。`[x]` ✅ v0.24.0
47. **🎨 help 精简（用户批次：m0105/m0117/m0118）** — ① `--search-url/--catalog-url/--content-url` 等 `--*url` 只在 `fqdt --help` 顶层出现，子命令 help 不再显示（去掉 `global=true`，仅顶层解析传 `apply_cli_overrides`；`-v/timeout` 仍 global）；② 删 HELP_TAILS 中 `flag 详情: ...` 与 `配置: ...` 尾行（示例段保留、仍受 `ui.help_examples` 控制）——帮助里不再教人用 help、不再列配置键；③ `--tone` 在 help 中作 `--ai` 的下级展示（声明紧跟 `--ai`，缩进/树符号表达从属，`fqdt help audio --tone` 详情不变）；④ 顺带排查其他可归组的 flag（如 `--voice` 归 `--tts`）。`[x]` v0.24.0 ✅
48. **📄 config.ini 分节模板（m0118）** — `save_default` 重写：**按子命令分节**（[search]/[download]/[audio]/[tts]/[info]/[ui]/[shelf]…），每节列出该子命令全部相关键（预留/空值或默认值），**每个键上方一行中文注释说明作用**；help 不再承载配置说明，配置说明唯一来源=config.ini 注释（与 `fqdt config <key> --help` CONFIG_HELP 并存）。旧键名与解析逻辑不变。`[x]` v0.24.0 ✅
49. **📐 子命令大重构（breaking，m0128+m0105）** — ① **删 `function` 子命令**（用户判定 worthless：Cmd::Function/run_function/别名 `fn`/HELP_TAILS/别名映射/601 提示全清；管道需求由 `api`/既有命令覆盖）；② `init` 并入 config：`fqdt config init`（顶层 Init 删）；③ `recommend`/`category` 并入 info（`fqdt info recommend`、`fqdt info category`——info 的 book_id 改 Option，无 id 且 section 为二者时走 discover，实现时核对参数形态）；④ **顶层子命令重排分组**：查询(search, info) / 下载(download, audio, update) / 管理(shelf, tts, login, config) / 服务(serve, api…) / help, version——按序 `display_order`，分组标题自定义顶层 help（clap 模板无分组能力，参照 FLAG_HELP 先例手写渲染）；`version` 补为子命令；⑤ **account 移除**（m0105 用户重申）：只读 sections 迁 `info` 子命令、云端书架写操作（--add/--remove/--move/--check/--author/--search）迁 `shelf` 子命令——即待办 42 的落地并入本项。`[x]` ✅ v0.24.0
50. **🤔 download+audio 合并评估（m0130）** — 结论：**不合并命令，仅共享选书逻辑**（已执行）。利弊：① 复用面——二者已共享 `fetch_catalog`/`Downloader`/`AudioDownloader`/`run_dir`/range/tone/abr 参数与 `workflow::search::pick`（仅 download 用），进一步合并只省一个 match 臂；② 参数面——download 有 `-t txt/epub`、audio 有三模式（`--real/--ai/--tts`）+`--lrc`+`--tone/--voice`，合并后 15+ flag、互斥组（format vs lrc vs real/ai/tts）复杂，与 47 help 精简方向相悖；③ 用户心智——"下载正文"与"下载有声书"是两个清晰动词，按媒体类型分命令；④ 与 49 分组关系——下载组 `download, audio, update` 三子命令正好成组，合并后组内只剩 update 显得失衡；⑤ 先例反证——v0.13.0 删 `get`（download+audio+update 合体，待办 39），合体命令已被否决过一次，合并是走回头路。**已执行部分**：`audio` 位置参数支持书名关键词（与 download 同规则：纯数字≥10 位=book_id，否则 `workflow::search::pick` 交互选书并自动加书架），help 文案同步"book_id 或书名关键词"。`[x]` ✅ v0.24.0
51. **📚 help 设计指南（m0132，本批最后做）** — 全部 help/子命令重构完成后，写 `HELP-DESIGN.md`（项目根）：面向其他 agent 的 help 消息设计与重写规范——结构（顶层分组/子命令段/flag 段/示例段）、文案（去括号、不教 help、不列配置、动词化 flag）、机制（FLAG_HELP/HELP_TAILS/CONFIG_HELP 三表分工、clap 拦截点）、评审 checklist；含联网调研的优秀 CLI help 写法要点（clig.dev 等）。`[x]` ✅ v0.24.0
52. **🧭 help 输出重构 + info 子命令化（m0049 用户全量反馈）** — ① **紧凑布局**：短介绍不另起一行，直接空几格接在 flag/子命令名后；flag/subcommand 之间不强制回车空行；消除 clap long_about 触发的长格式。② **info 的 section 改真子命令**（clap subcommand，各自专属 flag）：overview/chapters/intro/author/comments/cover/relative/audio/recommend/category/folders/history/progress/user/highlights；通用 flag（--timeout/-v 等全局）留父级，专属 flag 归各自子命令；其他命令层级同理。③ 顶层 help **去掉板块标题**（查询/下载/… 组名不显示），直接按分类顺序列出子命令；选项段标题改 **Options**（英文）。④ `ui.help_examples` **默认改为关闭**。⑤ 配置文件目录下生成 **help.txt** 完整参考：所有命令/子命令/子子命令 + flags 通用用法与目的；**API 子命令着重说明如何请求**（路径/-X/-q/-d/--jq/--status/--list/示例/常用端点）。启动时确保存在，版本变化才重写。⑥ **config.ini 模板**：分板块 + 英文标题（`## Download` 式）+ 每键中文注释说明作用。⑦ 同步 `HELP-DESIGN.md`。`[x] ✅ v0.26.0`

### 7.3 已知问题

- 第三方 API 不稳定（偶发 5xx）→ **已解决（v0.6.0）**：vendor fqapi-core（签名+设备池直连官方）为主通道，第三方源降级备选
- 音频流实际是 MP4/AAC 容器（上游 mime_type=video_mp4），文件后缀仍为 .mp3；CENC 解密后有效，但 ID3 嵌 LRC 对 MP4 无效（默认 external 模式无碍）
- edge-tts 需要 Python 环境
- 编译只能用 rustls（Termux 无 libssl-dev）
- 本地 `cargo build` 为 debug 模式（release 需 cross，耗时长）

---

## 8. 工作规范

### 8.0 终端输出设计规范（强制）

- **任何涉及 CLI 输出/日志/颜色/进度的改动，必须先读项目根 `TUI.md` 并遵守**（流分工、三色分层、一行一记录、日志默认静默、四问检查清单）
- **任何涉及 help 文案/子命令结构的改动，必须先读项目根 `HELP-DESIGN.md` 并遵守**（四层结构、去括号/不教 help/不列配置、四表分工、checklist）
- 颜色常量只能来自 `src/util.rs`（RST/DIM/CYAN/YELLOW/MAGENTA），禁止裸写 ANSI

### 8.1 To-Do 管理

- **永远保留原有待办项**，新需求追加到末尾
- 用户提出需求 → **先更新 AGENTS.md 待办** → 再实现代码
- 待办状态标记：`[ ]` 未开始 / `[x]` 已完成 / `[-]` 进行中
- 每完成一项，在 `### 7.2 待办` 对应项后加 `✅`

### 8.2 版本管理

- 每次发布前更新 `Cargo.toml` 的 `version`
- 每次 AGENTS.md 变更时更新开头的版本号
- 版本格式：`vMAJOR.MINOR.PATCH`
- PATCH：bug 修复 / 重构（不新加功能）
- MINOR：新功能（向后兼容）
- MAJOR：破坏性变更

### 8.2.1 发版清理（每次发版前必做）

磁盘长期紧张（rootfs 常驻 97%），**每次发版（打 tag 前）执行一次清理**：

```sh
cd ~/openwork/fqdt
cargo clean                      # 删 target/（~9G，下次 debug build 重编 ~2min）
rm -rf /tmp/fqdt-prof-* /tmp/ids100 /tmp/full100b.json /tmp/prof*.txt
rm -f rust.log                   # 本地日志（远端 rust.log 另行处理）
du -sh ~/.local/share/fqdt       # 检查数据目录（媒体缓存大了才清）
df -h /                          # 确认释放
```

- 顺序：**质量门全过 → 提交 push → 清理 → 打 tag**（清理后无需再 build；CI 用干净环境）
- 若清理后可用 <2G，先查 `du -sh ~/*` 大头再动手，禁止删 `.git` 与源码

### 8.3 交互规范

- **全部使用简体中文**（注释、帮助、错误消息、此文件）
- 错误前缀 `  err `，成功前缀 `  ok `（2空格缩进）
- 不使用 emoji
- 保持输出简洁对齐
- 对用户直接给结论，不解释推理过程

### 8.4 代码原则

- 每个基本函数 ≤30 行
- 每个工作流 ≤60 行
- 高阶函数 ≤20 行
- 不重复造轮子（优先用已有的库）
- 不引入无依赖的代码（每次加 crate 要评估必要性）
- CLI 参数短名遵循惯例：`-o` `-r` `-v` `-j` `-h` `-f` `-e`

### 8.5 持续审视（Crucial）

每次改动前和改动后，必须主动检查以下方面，并将发现的问题写入待办：

**🐛 Bug 检查清单：**
- 边界条件：空输入、超大输入、负值、特殊字符（Unicode/emoji/shell 元字符）
- 错误路径：网络超时、API 返回异常、文件权限、磁盘满、缓存损坏
- 并发安全：`AtomicUsize` 是否正确使用、`thread::spawn` 闭包是否捕获了 `&self`
- 资源泄漏：`File` 是否及时关闭、临时文件是否清理、`thread::join` 是否处理 panic
- 状态不一致：info.list 和实际文件列表不匹配、音频和 LRC 文件不对应

**⚡ 优化检查清单：**
- 重复请求：连续多次调用同一 API？（加缓存或用局部变量复用结果）
- 冗余 I/O：每次循环都 `fs::metadata`？可以批量读取
- 内存：大文件是否全部读入内存？（用 stream）
- 锁/竞争：`Arc<AtomicUsize>` 是否可以用更轻量的方式（如 `crossbeam`）
- 冷热分离：频繁访问的数据是否在 hot path 上？（如 `Config::load()` 每次调用都读文件？）

**💡 现代化设计检查清单：**
- 2018/2021 edition 特性是否充分利用？（`impl Trait`、`async/await`、`let-else`、`matches!` 等）
- 能否用 `trait` 抽象出通用行为？（如 `Downloader` + `AudioDownloader` 共享的并发逻辑）
- `enum` + `match` 是否优于多重 `if-else`？
- 能否用 `Result` + `?` 替代散落的 `unwrap()`/`expect()`？
- 函数签名是否过于复杂？（考虑用 `struct` 折叠参数）
- 有无死代码？（`pub` 但现在无人调用的函数）
- 依赖是否过时？（检查 `cargo outdated`）
- 能否用 `clap` 的 `value_parser` 做更严格的参数校验？

**发现任何上述问题 → 立即更新 7.2 待办 → 再实施修复。**

---

## 9. 外部依赖总览

| 依赖 | 用途 | 必须？ | 替代方案 |
|------|------|--------|---------|
| `curl` (外部命令) | HTTP 下载 fallback | 否 | minreq |
| `minreq` (crate) | HTTP 库（rustls-tls） | 是 | reqwest |
| `lame` (外部命令) | MP3 压缩 | 否 | lame-rs（未来） |
| `edge-tts` (Python) | TTS 语音合成 | 否 | 官方语音 API |
| `indicatif` | 进度条 | 是 | — |
| `id3` | ID3v2 tag 读写 | 是 | mp3-metadata |
| `zip` | EPUB 生成 | 是 | — |
| `clap` | CLI 参数解析 | 是 | — |
| `serde_json` | JSON 解析 | 是 | — |
| `cross` (CI tool) | 交叉编译 | CI only | — |

---

## 10. 文件清单

```
/root/build/fqdt/
├── AGENTS.md              ← 本文件（AI agent 主文档）
├── Cargo.toml             ← 版本号 + 依赖（workspace）
├── Cargo.lock
├── .github/workflows/
│   └── build.yml          ← CI: musl 静态双矩阵（x86_64 + aarch64，cross 交叉 aarch64）
├── crates/
│   └── fqapi-core/        ← vendor 自 Maicy0609/fanqie-lite-api（GPL-3.0）
└── src/
    ├── main.rs            ← CLI + 分发 + 配置注入（待拆分）
    ├── fqapi.rs           ← fqapi-core 运行时单例 + dispatch 直调封装
    ├── api.rs             ← HTTP 客户端 + API 调用（fqapi 主通道/第三方降级）
    ├── audio.rs           ← 音频/TTS/LRC/封面/后处理 + CENC 解密
    ├── download.rs        ← 正文下载 + info.list
    ├── config.rs          ← 配置解析 + 书架
    ├── types.rs           ← 数据结构
    ├── workflow/          ← 工作流（download/update/audio/shelf/info/search/discover/account）
    ├── workflow/discover.rs ← 发现/查询封装（recommend/category/comments/author）
    └── epub.rs            ← EPUB 生成
```

---

## 11. 长期规划（可维护性设计）

扩展原则（新功能按此落地，避免散落硬编码）：

1. **端点单点定义** — 常用端点集中在 `src/fqapi.rs::ENDPOINTS`（方法/路径/必填/说明）。`fqdt api --list`、封装子命令、`fqdt serve` 共用同一索引；新增端点先加索引项。
2. **透传零业务逻辑** — `fqdt api` 不加端点特定分支，core 新增端点后自动可用；只有需要友好输出时才另写封装。
3. **封装 = 提取 + 渲染管线** — 新封装子命令（todo15）统一为「取 JSON → 路径提取 → 表格/JSON 渲染」，参数与输出（`--json`）保持一致，不重复 HTTP 代码。
4. **解析辅助集中** — `api.rs` 的 `find_items`/`find_str`/`parse_*` 与 `util::jq_extract` 是唯一 JSON 提取入口，禁止各处手写路径遍历。
5. **运行时单一入口** — 对 core 的调用一律经 `src/fqapi.rs`（`ensure_runtime`/`call`/`get`/`get_json_timeout`），不直接依赖 `fqapi_core::runtime` 细节。
6. **re-vendor 流程** — 上游 fanqie-lite-api 更新时，替换 `crates/fqapi-core` 后跑 `cargo test --workspace` 与冒烟（search/catalog/content/audio），并在本节记录本地改动。目前本地改动六处：① `core_log.rs` 日志改走 stderr（防污染 JSON 管道）；② `endpoints/video.rs` 导出 `needs_cenc_decrypt`/`decrypt_cenc_content` 包装、`streaming_client` 挂 `DnsResolver`；③ 新增 `upstream/dns.rs`（进程内 DNS：探测 nameserver + 手写 UDP A 查询，静态 musl Android 无 /etc/resolv.conf）并在 `upstream/http.rs` 挂 `DnsResolver`、网络错误改 `format!("{e:?}")` 展开 source 链；④ （fqdt 侧非 vendor）`src/api.rs::tls_builder` webpki 根证书 + `DnsResolver`；⑤ `upstream/dns.rs` 探测顺序改 `FQDT_DNS` 短路 → `/etc/resolv.conf` 优先 → getprop/网关（WSL 网关不答 DNS 白等 12s/主机），`lookup_a` 去外层双重重试（v0.11.0）；⑥ `endpoints/content.rs::handle_batch` 接受真实 `book_id` 替换 URL 中 `book_id=0`（v0.11.0，batch 对匿名仍回元数据 Invalid，逐章 full 兜底）。
7. **新增功能落地顺序** — 更新 7.2 待办 → 加 ENDPOINTS 索引 → 实现提取/渲染 → 单测 → 手测 → 同步本文件。
8. **版本节奏** — 新增 CLI 能力升 MINOR，重构/修复升 PATCH；`Cargo.toml` 与本文件版本号同步。
