# fqdt — 番茄小说下载器

## 安装

```sh
# 从 GitHub Releases 下载
curl -L https://github.com/addallno/fqdt/releases/download/v0.5.1/fqdt-aarch64-linux -o ~/fqdt
chmod +x ~/fqdt

# 初始化配置
~/fqdt init
```

## 用法一览

| 命令 | 别名 | 功能 |
|------|------|------|
| `search` | | 搜索 + 交互下载 |
| `download` | `d` | 下载正文 |
| `audio` | `a` | 下载语音 |
| `update` | `u` | 增量更新 |
| `tts` | `t` | 文本转语音 |
| `info` | `i` | 查看目录/内容 |
| `shelf` | `s` | 书架管理 |
| `function` | `fn` | 原子函数 + 管道 |
| `init` | | 生成默认配置 |

---

## download — 下载正文（简写 d）

```sh
fqdt download <book_id>
fqdt d <book_id>                    # 简写
fqdt d <book_id> -o ./books -t epub -j 8   # 输出+格式+并发
fqdt d <book_id> -r 1-100                 # 章节范围
fqdt d <book_id> -r=-10                   # 前10章
fqdt d <book_id> -f                       # 强制覆盖
```

## audio — 下载语音（简写 a）

```sh
fqdt audio <book_id>
fqdt a <book_id>                    # 简写
fqdt a <book_id> -r 1-50 -j 6       # 范围+并发
fqdt a <book_id> --tone 5           # 指定音色
fqdt a <book_id> --lrc embed        # 歌词嵌入（降级 m4a 自动改为外置 .lrc）
```

> 输出统一为 `.mp3`：第三方源直出真 MP3；番茄官方源是 Opus 编码的 MP4，
> 安装了 ffmpeg 时自动转码 MP3（libmp3lame 64k，全平台可播）。未安装
> ffmpeg 时降级保存 `.m4a`——仅 VLC/mpv/Android 10+ 等支持 Opus-in-MP4
> 的播放器可播，转码缺失会在下载时提示一次。

## update — 增量更新（简写 u）

```sh
fqdt update <book_id>               # 按ID更新
fqdt update ./output                # 按目录自动检测
fqdt u ./output -j 4                # 指定并发
```

## tts — 文本转语音（简写 t）

```sh
fqdt tts novel.txt                  # 单文件
fqdt tts novel_dir/                 # 整个目录
fqdt tts file.txt --voice zh-CN-XiaoxiaoNeural
```

## search — 搜索并下载

```sh
fqdt search 凡人修仙传               # 搜索并交互选择
fqdt search 凡人 -D 1               # 自动下载第1本
fqdt search 凡人 -p 2 -D 3          # 第2页第3本
fqdt search 凡人 --dry-run          # 仅搜索
```

## info — 查看目录（简写 i）

```sh
fqdt info <book_id>
fqdt i <book_id>                    # 简写
fqdt i <book_id> -r 1-5 -s          # 章节范围+显示正文
```

## shelf — 书架管理（简写 s）

```sh
fqdt s                              # 列出
fqdt shelf -a <ID>:<标题>          # 添加
fqdt shelf -d <编号>               # 删除
fqdt shelf -D <编号>               # 下载
fqdt shelf -U                       # 一键更新所有
```

## function — 原子函数 + 管道（简写 fn）

```sh
fqdt fn fetch-catalog <book_id>
fqdt fn fetch-content <item_id>
fqdt fn search "凡人"
fqdt fn compress audio.mp3 --abr 32
fqdt fn embed-lrc audio.mp3
fqdt fn save output.txt             # 管道保存到文件

# 管道组合
fqdt fn fetch-catalog <id> \; fetch-content {}
fqdt fn search "凡人" \; fetch-detail {}
fqdt fn ... \; save result.txt      # 保存管道输出
```

## 自定义命令

在 `~/.config/fqdt/config.ini` 添加：

```ini
[workflow_cmd]
echo = echo 你输入了: {}
decode = python3 ~/decode_woff.py {}
```

使用：

```sh
fqdt echo 你好世界
fqdt decode font.woff
```

## 智能功能

- **智能源切换**: 自动按成功率+延迟排序 API 源，失败 URL 自动降级
- **自动并发**: 根据 CPU 核心自动选择并发数（核心×2，最高 32）
- **哈希增量**: `--force` 时对比已有内容，相同则跳过写入
- **书架一键更新**: `fqdt shelf -U` 更新所有收藏书籍

## 输出结构

```
output/
├── info.list
├── 0001_第一章_穿越.txt
├── 0002_第二章_奇遇.txt
├── 书名.epub
└── Audio/
    ├── info.list
    ├── 0001_第一章_穿越.mp3     # 第三方源（明文，真MP3）
    ├── 0002_第二章_奇遇.mp3     # 番茄官方源（ffmpeg 从 Opus-in-MP4 转码）
    ├── 0002_第二章_奇遇.lrc
    └── ...
```

## 编译

```sh
cargo build --release
```
