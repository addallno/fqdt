# fqdt — 番茄小说下载器

- **版本**: v0.9.0（`Cargo.toml` 中定义，每次变更更新）
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
- **上方有文档注释**：功能说明 + 被哪些工作流调用 + 可被 `function` 子命令直接调用

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
|  | `compress_mp3()` | input, output, abr | `()` | download_audio, process |
|  | `post_process_file()` | input, output, cmd | `()` | download_audio, process |
|  | `normalize_audio()` | input, output | `()` | process |
|  | `speed_audio()` | input, output, rate | `()` | process |
| **LRC** | `gen_lrc_text()` | Chapter | `String` | download_audio, embed |
|  | `write_lrc_file()` | path, text | `()` | download_audio, embed |
|  | `embed_lrc_uslt()` | mp3_path, text | `()` | download_audio, embed |
| **封面** | `embed_cover_apic()` | mp3_path, image_path | `()` | embed |
| **TTS** | `edge_tts_speak()` | text, voice, params | `PathBuf` | tts_convert |
|  | `convert_tts_file()` | txt_path, params | `PathBuf` | tts_convert |
|  | `convert_tts_dir()` | dir_path, params | `Vec<PathBuf>` | tts_convert |
| **元数据** | `read_info_list()` | dir | `(book_id, title, fmt, chapters)` | update |
|  | `write_info_list()` | dir, metadata | `()` | download, update |
|  | `read_audio_info_list()` | dir | `(book_id, title, chapters)` | update_audio |
|  | `write_audio_info_list()` | dir, metadata | `()` | download_audio |
| **书架** | `load_bookmarks()` | — | `Vec<(id, title)>` | shelf |
|  | `save_bookmark()` | id, title | `()` | shelf --add |
|  | `remove_bookmark()` | idx | `()` | shelf --del |
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
/// - CLI: `fqdt download <book_id> [--audio] [--abr N]`
/// - Config: `[workflow.download]` 可覆盖步骤
pub fn workflow_download(api: &Client, book_id: &str, out: PathBuf, range: Option<&str>,
                         fmt: &str, ft: &str, concurrent: usize, force: bool, vb: bool,
                         audio: bool, tone: usize, abr: u32, lrc: &str, fallbacks: &[usize]) { ... }
```

预编译工作流清单：

| 工作流 | 命令 | 功能 |
|--------|------|------|
| `workflow_download` | `download` | 下载正文 + 可选音频 |
| `workflow_update` | `update` | 增量更新（正文/音频） |
| `workflow_audio` | `audio` | 纯音频下载 |
| `workflow_search_dl` | `search` | 搜索 → 选择 → 下载 |
| `workflow_tts_convert` | `audio --tts` | TTS 文件/目录转换 |
| `workflow_embed` | `function embed` | 嵌入 LRC/封面 |
| `workflow_process` | `function process` | 后处理（压缩/变速/归一化） |
| `workflow_shelf` | `shelf` | 书架管理 |
| `discover::recommend` | `recommend` | 首页推荐流 |
| `discover::category` | `category` | 分类列表 |
| `discover::comments` | `comments` | 书评 |
| `discover::author` | `author` | 作者资料 + 书单 |

### 1.5 `function` 子命令

`function` 子命令**直接暴露基本函数**，供 CLI 调用：

```sh
fqdt function fetch-catalog <book_id>          # 获取目录并打印
fqdt function fetch-content <item_id>           # 获取正文并打印
fqdt function fetch-audio-url <book_id> <item_id> [tone]  # 获取音频 URL（+ CENC 密钥）
fqdt function compress <input.mp3> --abr 32     # 压缩 MP3
fqdt function gen-lrc <chapter.json>            # 生成 LRC 文本
fqdt function embed-lrc <file.mp3>              # 嵌入 LRC 到 ID3
fqdt function embed-cover <file.mp3> --cover img.jpg
fqdt function strip-html <file.html>            # 剥离 HTML 标签
fqdt function search <keyword>                  # 搜索并打印结果（JSON）
fqdt function read-info <dir>                   # 读取 info.list 并打印
```

**实现方式**：`function` 子命令是 clap 的 `#[command(subcommand)]`，每个基本函数映射到一个 `FunctionCmd` 变体。函数实现本身在各自的模块中（`api.rs`/`audio.rs`/`download.rs`），`main.rs` 只做分发。

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

四个高阶工作流（`src/workflow/discover.rs`），默认彩色表格，`--json` 输出机器可读原文：

```sh
fqdt recommend                          # 首页推荐流（16 本书）
fqdt recommend --tab-type 25 --tab-index 0   # 2 推荐 / 25 小说 / 11 经典 / 5 听书
fqdt category --tab 1                   # 1 男频 / 2 女频 / 3 出版 / 4 听书
fqdt comments 7143038691944959011 -n 20 # 书评（书名+评分+用户+正文+点赞）
fqdt author 123456 -n 20                # 作者资料 + 推荐书单
fqdt recommend --json | jq .            # 机器可读
```

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

`config.ini` 允许**不修改代码**添加新的 function 和工作流：

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
├── workflow.rs 或 workflow/
│   ├── mod.rs       工作流调度
│   ├── download.rs  workflow_download
│   ├── update.rs    workflow_update
│   ├── audio.rs     workflow_audio
│   ├── embed.rs     workflow_embed
│   └── process.rs   workflow_process
└── util.rs          工具函数 + 高阶函数
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
/// - `fqdt function search <keyword> -p <page>`
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

### 4.4 高阶函数模式

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

### 4.5 导入顺序

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

### 5.1 基本函数 → `function` 子命令映射

每个基本函数在 `FunctionCmd` 枚举中有一个对应变体：

```rust
enum FunctionCmd {
    Search { keyword: String, page: Option<usize> },
    FetchCatalog { book_id: String },
    FetchContent { item_id: String },
    FetchAudioUrl { book_id: String, item_id: String, tone: Option<usize> },
    Compress { input: String, abr: u32 },
    EmbedLrc { input: String },
    EmbedCover { input: String, cover: String },
    StripHtml { input: String },
    ReadInfo { dir: String },
    GenLrc { chapter_json: String },
}
```

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
| **CLI** | 6 个子命令 + function 子命令 | ✅ |
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
4. **📦 function 子命令扩展** — 为每个基本函数添加 `FunctionCmd` 变体
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

### 7.3 已知问题

- 第三方 API 不稳定（偶发 5xx）→ **已解决（v0.6.0）**：vendor fqapi-core（签名+设备池直连官方）为主通道，第三方源降级备选
- 音频流实际是 MP4/AAC 容器（上游 mime_type=video_mp4），文件后缀仍为 .mp3；CENC 解密后有效，但 ID3 嵌 LRC 对 MP4 无效（默认 external 模式无碍）
- edge-tts 需要 Python 环境
- 编译只能用 rustls（Termux 无 libssl-dev）
- 本地 `cargo build` 为 debug 模式（release 需 cross，耗时长）

---

## 8. 工作规范

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
    ├── workflow/          ← 工作流（download/update/audio/embed/process/discover）
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
6. **re-vendor 流程** — 上游 fanqie-lite-api 更新时，替换 `crates/fqapi-core` 后跑 `cargo test` 与冒烟（search/catalog/content/audio），并在本节记录本地改动。目前本地改动两处：`core_log.rs` 日志改走 stderr（防污染 JSON 管道）；`endpoints/video.rs` 导出 `needs_cenc_decrypt`/`decrypt_cenc_content` 包装。
7. **新增功能落地顺序** — 更新 7.2 待办 → 加 ENDPOINTS 索引 → 实现提取/渲染 → 单测 → 手测 → 同步本文件。
8. **版本节奏** — 新增 CLI 能力升 MINOR，重构/修复升 PATCH；`Cargo.toml` 与本文件版本号同步。
