# config.ini 编写指南

> 配置的**唯一长期存放处**：`~/.config/fqdt/config.ini`（首次运行自动生成完整模板）。
> 键的权威说明 = 本文件 + `fqdt config <点号键> --help`；`DEFAULT_INI`（src/config.rs）是模板源码。

## 1. 基本语法

```ini
# # 开头是注释（行首），值里的 # 不是注释
[download]              # 方括号 = 节，节名全小写英文
format = epub           # 键 = 值，等号两侧空格可选
concurrent = 4          # 整数
verbose = false         # 布尔只能 true / false（全小写）
```

- 文件编码 UTF-8；未知键会被忽略但打印 `warn`（拼错键名时会看到提示）。
- 值**不要加引号**——引号会成为值的一部分（`format = "epub"` 会解析成 `"epuc"` 之外的带引号字符串，行为未定义）。

## 2. 分节约定（写模板/改模板时遵守）

```ini
## Download             ← 每节上方一行英文标题注释（## 开头），给人看的分组名
[download]              ← 节名本身：小写、与 KEYS 表一致
# 每个键上方一行中文注释：作用 + 取值范围
concurrent = 0
```

**硬约束**：节名必须与 `KEYS`（src/config.rs）各条目的节字段一致，且保持小写。
`ini_write` 按 KEYS 的节回写——大小写不一致（如 `[Download]` vs `[download]`）会产生**重复节**，
这是历史上出过的 bug（DEFAULT_INI 顶部注释有警告）。

## 3. 值类型与特殊格式

| 类型 | 写法 | 说明 |
|---|---|---|
| 整数 | `timeout = 15` | 带单位说明见键注释（秒/毫秒/kbps） |
| 布尔 | `true` / `false` | 只认这两个词，`1/0/yes` 不行 |
| 字符串 | `format = txt` | 不加引号、不带尾随空格 |
| 逗号列表 | `audio_tone_fallbacks = 2,4,5` | 空格会被 trim；空值 = 空列表 |
| URL 模板 | `...?q={}&offset={}` | `{}` 是占位符，运行时按位置换关键词/页码等，**顺序不能改** |
| 命令模板 | `post_process = lame --abr 32 {input} {output}` | `{input}`/`{output}` 占位，整条原样交给 shell |

`api.*_url` 系列支持多个源逗号分隔：**从前往后**依次尝试，主通道（fqapi）失败才走降级链。

## 4. 三种修改方式与优先级

```bash
# ① 直接编辑文件（推荐批量改 / 加注释）
vim ~/.config/fqdt/config.ini

# ② 命令行改单键（点号键，自动保留注释与格式）
fqdt config download.format epub
fqdt config --list          # 列出全部键与当前值
fqdt config download.format --help   # 单键详情

# ③ 命令行 flag（临时覆盖）
fqdt download <id> -t epub -j 8
```

**优先级：命令行 flag > config.ini 同义项 > 内置默认值。**
例如同时设了 `download.format = txt` 和 `-t epub`，本次执行用 epub，ini 不被改写。

## 5. 全部键清单（30 键）

### [download] 下载
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `download.concurrent` | `concurrent` | int | `0` | 并发线程数；0=按 CPU 核数×2，上限 32；`-j` 同义 |
| `download.format` | `format` | str | `txt` | 默认输出格式 txt/epub；`-t` 同义 |
| `download.output_dir` | `output_dir` | str | `.` | 默认输出目录，相对路径基于当前工作目录；`-o` 同义 |
| `download.filename_template` | `filename_template` | str | `{idx04}_{title}` | 文件名模板；`{idx04}` 章序四位、`{title}` 章标题 |
| `download.verbose` | `verbose` | bool | `false` | 默认详细日志，等价全局 `-v` |

### [cache] 缓存
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `cache.enabled` | `cache_enabled` | bool | `true` | 目录/详情文件缓存开关 |
| `cache.ttl` | `cache_ttl` | int | `86400` | 缓存有效期秒，过期下次请求时刷新 |

### [api] 请求与降级源
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `api.timeout` | `timeout` | int | `15` | 请求超时秒；单章下载另有 10s 内层超时 |
| `api.search_url` | `search_url` | str | 内置多源 | 第三方搜索降级，`{}` 换关键词与页码 |
| `api.catalog_url` | `catalog_urls` | str | 内置多源 | 第三方目录降级，`{}` 换 book_id |
| `api.content_url` | `content_urls` | str | 内置多源 | 第三方正文降级，`{}` 换 item_id |
| `api.batch_url` | `batch_urls` | str | 内置 | 批量正文源，仅历史兼容 |
| `api.detail_url` | `detail_url` | str | 内置 | 第三方详情降级，`{}` 换 book_id |
| `api.audio_content_url` | `audio_content_urls` | str | 内置 | 音频内容源，两个 `{}` = item_id、tone_id |

### [audio] 有声书
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `audio.tone` | `audio_tone` | int | `1` | 默认 AI 音色 1-91；`fqdt info audio <id>` 查本书可用 |
| `audio.tone_fallbacks` | `audio_tone_fallbacks` | str | `2,4,5,6,74,91` | 主音色失败时依次尝试 |
| `audio.abr` | `abr` | int | `0` | MP3 码率 32/64/128 kbps；0=不重编码 |
| `audio.post_process` | `post_process` | str | 空 | 后处理命令，`{input}`/`{output}` 占位 |

### [tts] Edge-TTS
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `tts.rate` | `tts_rate` | str | `+0%` | 语速，如 `+10%` |
| `tts.volume` | `tts_volume` | str | `+0%` | 音量，如 `-20%` |
| `tts.pitch` | `tts_pitch` | str | `+0Hz` | 音调，如 `+5Hz` |

### [info] 概览展示
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `info.top_chapters` | `info_top_chapters` | int | `10` | 概览前 N 章；0=关闭该段 |
| `info.top_comments` | `info_top_comments` | int | `3` | 概览前 N 条书评；0=关闭 |
| `info.hot` | `info_hot` | bool | `true` | 热度行（阅读量/评分/收藏）开关 |
| `info.intro_len` | `info_intro_len` | int | `80` | 简介截断字数；0=不显示简介 |
| `info.comments_count` | `info_comments_count` | int | `20` | `info comments` 取前 N 条 |
| `info.author_count` | `info_author_count` | int | `20` | `info author` 书单取前 N 条 |

### [ui] 界面
| 点号键 | ini 键 | 类型 | 默认 | 作用 |
|---|---|---|---|---|
| `ui.clip_len` | `ui_clip_len` | int | `40` | 评论/摘要/作者名截断字数；0=不截断 |
| `ui.color` | `ui_color` | bool | `true` | 彩色输出；false 等价 `NO_COLOR=1` |
| `ui.help_examples` | `ui_help_examples` | bool | `false` | help 底部示例段开关 |

## 6. 完整示例

```ini
# 番茄小说下载器配置
# 优先级: 命令行 flag > 本文件同义项

## Download
[download]
# 默认 epub，慢机器改小并发
concurrent = 4
format = epub
output_dir = ~/books
filename_template = {idx04}_{title}
verbose = false

## Cache
[cache]
cache_enabled = true
cache_ttl = 86400

## API
[api]
timeout = 20
# 只留一个自建降级源
search_url = http://101.35.133.34:5000/api/search?key={}&offset={}

## Audio
[audio]
audio_tone = 5
abr = 64

## TTS
[tts]
tts_rate = +10%

## Info
[info]
info_top_chapters = 5
info_intro_len = 120

## UI
[ui]
ui_help_examples = true
```

## 7. 常见坑

1. **节名大小写**——必须全小写，否则回写时产生重复节。
2. **布尔写 `1`/`0`**——不认，只认 `true`/`false`。
3. **值加引号**——`format = "epuc"` 之类引号进值，解析异常。
4. **`{}` 占位符删改**——URL 模板里 `{}` 数量与顺序必须保留，删掉会导致降级请求 404。
5. **改了键没生效**——先 `fqdt config --list` 确认值；再确认命令行有没有同名 flag 覆盖（flag 优先）。
6. **手改模板**——改 `DEFAULT_INI` 源码时同步更新本文件键清单；用户已存在的 config.ini 不会被启动覆盖（仅首次生成）。

## 8. 待调查：还有哪些该进 config（便利性候选）

**调查原则**（新增键必须过这四关）：

1. **长期偏好**——用户改一次就想一直生效的值（端口、语音名、间隔）才进 config；一次性运行参数（页码 `-p`、`-X` 方法、`--jq` 路径、book_id）**不进**。
2. **flag 同义**——已有 CLI flag 的，config 提供同义项即可，保持「flag > ini > 默认」优先级（见 §4）。
3. **成本合算**——改动点 = `KEYS` + `DEFAULT_INI` + `CONFIG_HELP` + 本文件清单 + `fqdt config <key> --help` 测试，四处缺一不可；收益小的硬编码常量宁可不动。
4. **先查证再立项**——按下面的方法扫代码，确认「只有 flag / 只有硬编码」而非已有键。

**调查方法**：

```bash
# ① 列出所有带默认值的 flag（候选 = 没有对应 config 键的）
grep -rn "default_value" src/main.rs
# ② 列出代码里的硬编码常量（重试/超时/块大小/阈值）
grep -rnE "with_retry_async|from_secs\(|const CHUNK|fail >=" src/
# ③ 对照 KEYS 表看哪些缺键
grep -n "const KEYS" -A 40 src/config.rs
```

**当前发现的候选（截至 v0.26.0，尚未立项，逐条需按四关复核）**：

| 候选键（建议名） | 现状 | 类型 | 便利性 | 初判 |
|---|---|---|---|---|
| `tts.voice` | 只有 flag `--voice`，默认 `zh-CN-XiaoxiaoNeural`（main.rs）；config 已有 rate/volume/pitch **独缺 voice** | str | 常换音色的用户每次都要打长参数 | **建议加** |
| `serve.port` | 只有 flag `--port`，默认 `18921`（main.rs:198） | int | 固定部署场景免打参数 | 建议加 |
| `download.interval` / `audio.interval` | 只有 flag `-i/--interval` 默认 `0`，config 无 | int | 想固定限速防封的用户 | 建议加（或合并 `api.interval`） |
| `download.range` | flag `-r` 无默认（=全量），config 无 | str | 常只下前 N 章的用户 | 可选 |
| 批量块大小 `CHUNK=100` | 硬编码 api.rs:615 | int | 网络环境差才需调小 | 暂不加（风险>收益） |
| HTTP 重试次数 `2` | 硬编码 `with_retry_async(..., 2)` 多处 | int | 同上 | 暂不加 |
| 熔断阈值 `fail≥2` | 硬编码 `source_dead()` api.rs:74 | int | 同上 | 暂不加 |
| 单章内层超时 `10s` | 硬编码 | int | 与 `api.timeout` 概念重叠，易混淆 | 暂不加 |

> 流程：确认要加的候选 → 按 §2 写进 `DEFAULT_INI`（节标题+中文注释）→ `KEYS`/`CONFIG_HELP` 同步 → 本文件 §5 清单补行 → `fqdt config <key>` 冒烟 → 随版本发布。

