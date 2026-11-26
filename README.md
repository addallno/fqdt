# fqdt — A Rust FanqieNovel CLI

番茄小说（FanqieNovel）命令行下载器：正文 / 有声书 / 书架 / 阅读数据，一条命令离线归档。

- **正文**：txt / epub，批量并发、增量更新、章节范围
- **有声书**：官方音源 CENC 解密，默认 remux Ogg Opus（零转码可播优先），`-c mp3` 转 MP3，`-c m4a` 保原容器，`--cover` 嵌封面
- **发现**：搜索、首页推荐、分类、书评、作者书单
- **书架**：本地 / 云端（需登录 Cookie），分组、过滤、一键更新
- **阅读数据**：进度时间线、阅读历史、云端文件夹、划线笔记
- **透传**：`fqdt api <path>` 直达任意端点，`serve` 起本地 HTTP 服务
- **TTS**：edge-tts 把正文合成为语音

目标环境 Android Termux（aarch64），也提供 x86_64 Linux 静态二进制。

## 安装

```sh
# 从 GitHub Releases 下载（x86_64；aarch64 用 fqdt-aarch64-linux-musl）
curl -L https://github.com/addallno/fqdt/releases/latest/download/fqdt-x86_64-linux-musl -o ~/fqdt
chmod +x ~/fqdt
~/fqdt --version
```

首次运行自动生成 `~/.config/fqdt/config.ini`（分节、每键带中文注释）。配置说明见 [CONFIG-INI.md](CONFIG-INI.md)，完整命令参考见运行时生成的 `~/.config/fqdt/help.txt`。

## 快速上手

```sh
fqdt search 诡秘之主              # 搜索
fqdt download 7185478854738185271 # 下载正文（纯数字 = book_id）
fqdt download 诡秘之主            # 书名关键词 → 交互选书
fqdt download <id> -t epub -r 1-50 -j 8
fqdt view 诡秘之主               # TUI 阅读器（分栏目录/正文，j/k 段光标，翻页键位见下方「view TUI 快捷键」，q 退出存进度）
fqdt audio <id> -T ai --tone 6    # 有声书：AI 朗读指定音色
fqdt info overview <id>           # 概览：热度 / 简介 / 前几章 / 书评
fqdt shelf list                   # 书架（登录后为云端）
fqdt --help                       # 全部命令
```

## 命令一览

| 命令 | 别名 | 功能 |
|------|------|------|
| `search <关键词>` | | 搜索列表（`-D N` 顺带下载第 N 本） |
| `info <section> [id]` | `i` | 书籍信息与个人数据（见下） |
| `download <id\|书名>` | `d` | 下载正文 txt/epub |
| `audio <id\|书名>` | `a` | 下载有声书（`-T real/ai/tts` 三来源，默认 Ogg，`-c mp3` 转 MP3） |
| `view <id\|key>` | | TUI 阅读器：分栏目录+正文、段落光标高亮、翻页/切章/看图、zen 全屏、进度自动记忆（键位见「view TUI 快捷键」） |
| `update <id\|目录>` | `u` | 增量更新（并入 download，`-T content/audio` 分类，默认全部） |
| `tts <txt\|目录>` | `t` | edge-tts 文本转语音 |
| `shelf <子命令>` | `s` | 书架管理（list/add/remove/move/download） |
| `login [COOKIE]` | | 登录态（`--check`/`--clear`） |
| `config <子命令>` | | 查看/修改配置（`get <键>`/`get --all`/`set`/`unset`/`init [--force]`） |
| `serve` | | 本地 HTTP 服务（默认 127.0.0.1:18921） |
| `api <路径>` | | 端点透传（`--list`/`-X`/`-q`/`--jq`） |
| `version` | | 显示版本号 |

## view TUI 快捷键

阅读界面按 `?` 或 `h` 打开内置帮助（叙述 UI；本节为完整键位明细）：

| 键 | 作用 |
|---|---|
| `j k` / `↑ ↓` | 移动段落选中光标（整段高亮，图片段不高亮） |
| `← →` | 平行翻页（光标落同相对段；上下滚到屏底自动翻页） |
| `,` / `.` | 上翻 2/3 页 / 下翻 1/2 页 |
| `<>` 或 `n p` | 上一章 / 下一章 |
| `d u` / `g G` | 半页滚动 / 章首尾 |
| `Tab` / `t` / `Esc` | 目录面板（`Enter` 跳转） |
| `space` | 放大光标处图片（`i` 视窗第一张） |
| `z` | zen 全屏（四边框、标题章节名、底框进度） |
| `?` / `h` | 打开 / 关闭帮助（`Esc` 返回） |
| `e` | 外部编辑器只读打开当前章（`-e` / `config ui.editor` / `EDITOR`；vim 系自动 `-R`） |
| `q` | 退出（记忆进度） |

翻页方向：`config ui.page_mode` = `right-left`（默认）左右渲染 / `up-down` 上下渲染。

`info` 子命令：`overview`（默认）/ `chapters` / `intro` / `author` / `comments` / `cover` / `relative` / `audio`；发现类 `recommend` / `category`；个人数据 `folders` / `history` / `progress` / `user` / `highlights`（需登录，均可 `--json`）。

## download — 下载正文

```sh
fqdt d <id>
fqdt d <id> -o ./books -t epub -j 8   # 输出 + 格式 + 并发
fqdt d <id> -r 1-100                 # 章节范围（-r=-10 前 10 章）
fqdt d <id> -f                       # 强制覆盖
fqdt d <书名>                             # 关键词交互选书
```

## audio — 下载有声书

```sh
fqdt a <id> -r 1-50 -j 6       # 范围 + 并发
fqdt a <id> -T real            # 真人讲书
fqdt a <id> -T ai --tone 6     # AI 朗读（fqdt info audio <id> 查可用音色）
fqdt a <id> -T tts --voice zh-CN-XiaoxiaoNeural  # Edge-TTS（需本机 edge-tts）
fqdt a <id> --lrc embed        # 歌词嵌入（非 MP3 退化外置；听书章节自动用 timeline 精确时间轴，文字书按段落估算）
fqdt a <id> -c mp3             # 输出转 MP3（默认是 Ogg Opus）
fqdt a <id> -c m4a             # 保持 m4a 原样，不转码
fqdt a <id> --cover            # 新下载的 MP3 嵌入书籍封面
```

> **输出格式**：第三方源直出真 MP3；番茄官方源是 Opus 编码的 MP4，装有 ffmpeg 时默认 `ffmpeg -c copy` remux 为 Ogg Opus（零转码、秒级、通用可播），`-c mp3` 改走 libmp3lame 64k 转码（iOS/Windows Media Player 兼容），无 ffmpeg 则降级保留 `.m4a`（仅 VLC/mpv/Android 10+ 等可播，下载时提示一次）。`-c m4a` 跳过转码；`--cover` 整本书取一次封面写入每章 APIC，仅对新下载的 MP3 生效；同义配置键 `audio.mp3` / `audio.keep_m4a` / `audio.cover`。
>

## update — 增量更新

```sh
fqdt u <id>               # 按 ID 更新（按文件名补缺，含中间缺章）
fqdt u ./output           # 按目录增量（info.list 比对，含音频）
fqdt u ./output -T content   # 只补正文；-T audio 只补音频（可逗号分隔）
```

## info — 书籍信息与个人数据

```sh
fqdt i overview <id>           # 概览（默认，直接 fqdt i <id> 等价）
fqdt i chapters <id> -r 1-50   # 目录（含 item_id）
fqdt i intro / author / comments / cover / relative / audio <id>
fqdt i recommend               # 首页推荐流
fqdt i category --tab 1        # 分类（1 男频 / 2 女频 / 3 出版 / 4 听书）
fqdt i progress                # 阅读进度时间线（需登录）
fqdt i history / folders / user / highlights
fqdt i overview <id> --add     # 概览后直接收藏进云端书架
```

`<id>` 支持 book_id / 章节 item_id / 作者 id（`N_` 前缀）/ 读者 uid，自动识别。

## shelf — 书架管理

```sh
fqdt s list                    # 未登录=本地，已登录=云端
fqdt s list 已读                # 指定分组
fqdt s list --local --search 词 --author --json
fqdt s add <id>[:标题]
fqdt s remove <编号>
fqdt s move <id> <组名>   # 组不存在则自动建组
fqdt s download 1              # 下载书架第 1 本
```

## login / config / api / serve

```sh
fqdt login '<Cookie>'          # 浏览器复制 Cookie；--check 验证；--clear 清除
fqdt config get --all          # 全部键与当前值（旧 --list 仍兼容）
fqdt config download.format epub
fqdt config download.format --help   # 单键详情

fqdt api --list                # 常用端点清单
fqdt api /api/v1/search -q query=诡秘 -q tab_type=3
fqdt api /api/v1/books/<id> --jq data.book_name
fqdt serve --port 18999 --lan  # 局域网 HTTP 服务
```

## 智能特性

- **主通道 + 降级**：fqapi-core 进程内直调官方接口（签名 + 设备池），失败自动轮换第三方源
- **自动并发**：0 = CPU 核数 × 2，上限 32
- **增量**：`update` 并入 `download`（静默转调），按 info.list 比对只补缺失章节；`-T content/audio` 分类
- **缓存**：目录/详情文件缓存（TTL 可配）
- **静态构建**：musl 静态链接，Android 上自带根证书与进程内 DNS，无系统依赖

## 输出结构

```
output/
├── info.list
├── 0001_第一章_穿越.txt        # 或 .epub
└── Audio/                     # 书籍目录场景下的音频子目录
    ├── info.list
    ├── 0001_第一章_穿越.mp3
    ├── 0001_第一章_穿越.lrc
    └── ...
```

## 已知限制

- **RTL 语言（希伯来语/阿拉伯语）正文**：终端负责 bidi 双向重排，而 ratatui 按字符显示宽度排版、不感知 bidi；RTL 混排段落的右边框对齐与换行位置可能错位（已验证按显示簇计数 82 列全齐，属终端重排表现）。纯 LTR 正文不受影响。

## 编译

```sh
cargo build --release
# CI（GitHub Actions）产出 x86_64 + aarch64 musl 静态二进制并附到 Release
```

## 许可

GPL-3.0。核心 `crates/fqapi-core/` 源自 [Maicy0609/fanqie-lite-api](https://github.com/Maicy0609/fanqie-lite-api)。
