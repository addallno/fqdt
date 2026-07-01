# fqdt — 番茄小说下载器

FanqieNovel (番茄小说) 下载器，Rust 实现，支持正文下载、语音下载、TTS 合成。

## 安装

从 GitHub Releases 下载静态链接的 aarch64 二进制：

```sh
# 直接下载最新版
curl -L -o fqdt https://github.com/addallno/fqdt/releases/latest/download/fqdt
chmod +x fqdt
./fqdt --help
```

或从 CI artifact 下载：

```sh
bash <(curl -s https://raw.githubusercontent.com/addallno/fqdt/main/dl.sh)
```

## 用法

### 搜索

```sh
fqdt search 凡人              # 交互选择后自动下载
fqdt search 凡人 -D 1         # 自动下载第一本
fqdt search 凡人 -p 2         # 翻页
fqdt search 凡人 --dry-run    # 只搜索不下载
```

### 查看目录

```sh
fqdt info <book_id>
fqdt info <book_id> -r 10-20
fqdt info <book_id> -r 1-3 -s  # 显示正文
```

### 下载正文

```sh
fqdt download <book_id> -o <目录> -r 1-50 -j 6
fqdt download <book_id> -r=-5            # 前 5 章
fqdt download <book_id> -r 10-           # 10 章到结尾
```

### 增量更新

```sh
fqdt update <目录>       # 自动从 info.list 检测
fqdt update <book_id> -o <目录>
```

### 下载语音

```sh
fqdt audio <book_id> -r 1-10     # 默认音色 1
fqdt audio <book_id> --tone 4    # 指定音色
```

### TTS 合成（需安装 edge-tts）

```sh
fqdt audio -t 第1章.txt           # 单个文件转 MP3
fqdt audio -t ./chapters/         # 目录下所有 txt 转 MP3
fqdt audio -t ./chapters/ --voice zh-CN-XiaoyiNeural
```

### 书架

```sh
fqdt shelf                                # 列出
fqdt shelf -a <ID>:<标题>                 # 添加
fqdt shelf -d <编号>                      # 删除
fqdt shelf -D <编号>                      # 下载
```

## 选项

| 参数 | 说明 |
|------|------|
| `-j, --jobs` | 并行下载数 (默认 4) |
| `-r, --range` | 章节范围 `1-50` / `-5` / `10-` |
| `-o, --output` | 输出目录 |
| `-i, --interval` | 下载间隔毫秒 |
| `-t, --format` | 输出格式 txt/epub |
| `--timeout` | HTTP 超时秒数 (默认 15) |
| `-v, --verbose` | 详细输出 |

## 配置

首次运行自动生成 `~/.config/fqdt/config.ini`，可自定义：

```ini
[download]
concurrent = 4
format = txt
output_dir = .
filename_template = {idx04}_{title}

[cache]
cache_enabled = true
cache_ttl = 86400

[api]
search_url = https://novel.snssdk.com/...
catalog_url = https://fanqienovel.com/...
content_url = http://101.35.133.34:5000/...
```

## 编译

```sh
# 本地
cargo build --release

# 交叉编译 (aarch64)
cross build --release --target aarch64-unknown-linux-gnu
```

## 文件结构

```
<输出目录>/
├── info.list          # JSON 索引
├── 0001_第1章.txt     # 正文
├── 0002_第2章.txt
└── Audio/
    ├── info.list      # 语音索引
    ├── 0001_第1章.mp3
    └── 0002_第2章.mp3
```

## 依赖

- **运行时**: 无 (静态链接)
- **TTS** (可选): `pip install edge-tts`
- **编译**: Rust + cross (aarch64)
