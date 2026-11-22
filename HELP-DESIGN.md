# fqdt help 设计与重写规范

> 面向修改 fqdt CLI 帮助文案的 agent。改任何 help/输出前必读；总输出规范见 `TUI.md`。
> 依据：clig.dev（Command Line Interface Guidelines）、Vercel CLI Design Guidelines、bettercli.org，
> 以及本项目 v0.16.0–v0.24.0 的迭代结论（去括号、不教 help、不列配置、分组顶层 help）。

---

## 1. 结构（四层 help，各管一段）

| 层 | 触发 | 载体 | 内容 |
|---|---|---|---|
| 顶层 help | `fqdt` / `fqdt --help` / `fqdt help` / `fqdt -h` | `main.rs::render_top_help`（手写）+ `TOP_GROUPS`/`TOP_OPTIONS` | 标题、用法行、**按任务分组**的子命令表、全局选项、优先级尾行 |
| 子命令 help | `fqdt <cmd> --help` / `fqdt help <cmd>` | clap derive doc + `HELP_TAILS` 追加示例段 | 一句话摘要、位置参数、flags（长 help）、示例 |
| flag 详情 | `fqdt <cmd> <flag> --help` | `main.rs::FLAG_HELP` / `config.rs::CONFIG_HELP` | 取值范围、语义、示例、关联查询/配置键 |
| config 键详情 | `fqdt config <key> --help` | `CONFIG_HELP` | 同上（键专属） |
| help.txt 参考 | 启动自动生成 `~/.config/fqdt/help.txt` | `helpfile.rs` | 解释性参考：命令格式与目的、参数/端点作用、各命令底层 API 请求格式（**非** clap help 复制、**无** example） |

**分层原则**：顶层只教"有哪些命令、干什么"；子命令层教"这个命令怎么用"；
flag 详情层教"这个取值是什么"。每层不越界——顶层不列 flag 详情，子命令 help 不教 help、不列配置键。

### 1.1 顶层分组（TOP_GROUPS）

按**任务**分组，不按"常用/高级"（clig.dev：git 式按场景分组）：

```
查询      search, info
下载      download, audio, update
管理      shelf, tts, login, config
服务      serve, api
其他      version, help
```

- 分组顺序与组内顺序 = `display_order`（10/11、20/21/22、30/31/32/33、40/41、90），改顺序两处同步。
- 子命令摘要从 `Cli::command()` 内省（`get_about()`），**不双份维护**——改 doc 注释即改顶层 help。
- 选项段 `TOP_OPTIONS` 手写硬编码（clap 模板无分组能力，参照 FLAG_HELP 先例）。

### 1.2 子命令 help（clap + HELP_TAILS）

**紧凑布局（v0.26.0 硬规则）**：
- 短介绍**不另起一行**——直接空几格贴在 flag/子命令名后（一行式，两列对齐）；
- flag 与 subcommand 之间**不加回车空行**；
- 实现手段：变体的 doc 注释**只留一行**（多段注释会触发 clap long_about → `--help` 落入两行式长格式）；补充信息进 `help.txt` / `HELP_TAILS`；
- 标题样式：clap `Options/Usage/Arguments/Commands` 与手写 `Options`、`Examples` 一律**加粗+下划线**（`util::TITLE` / `with_help_styles()` 的 `Effects::BOLD | Effects::UNDERLINE`）；示例段标题用英文 `Examples:`。

- 变体 doc 注释 = 摘要行，写**动词化一句话**（"下载正文"、"搜索并选择"），不写功能清单。
- flag 的 `help` 属性：短、无括号、写**取值语义**（"章节范围 1-50 / -5 / 10-"）。
- `HELP_TAILS` 只放**示例段**（受 `ui.help_examples` 开关控制），每命令 2–7 行真实可跑的命令。
- 从属 flag 用**多行 help 缩进**表达（先例：`--tone` 嵌 `--ai`、`--voice` 嵌 `--tts`；声明加 `hide = true` 保解析）。

### 1.3 flag 详情（FLAG_HELP）

`(命令, flag, FlagDoc)` 三元组，字段：`brief`（一句话）、`takes_value`、`fields`（取值/语义/查询/配置/示例，成对出现）。
新增会取值的 flag 时**同步加详情行**，否则 `fqdt <cmd> <flag> --help` 退化为 clap 粗 help。

---

## 2. 文案规则（硬约束）

1. **去括号**——help 里禁止括号及括号内注释（v0.23.0 起，用户明确要求）；改用换行缩进或独立行表达补充信息。代码注释、运行输出不受此限。
2. **不教 help**——删除 `flag 详情: ...`、`用 fqdt xxx --help 查看` 类尾行（v0.24.0）。
3. **不列配置键**——help 不写 `配置: config.ini [x] y` 说明；配置说明唯一来源 = `config.ini` 分节模板注释（v0.24.0）+ `CONFIG_HELP`。
4. **不打迁移提示**——命令移除后直接未知命令 err，不打"已移除/请改用 xxx"（v0.24.0）。
5. **动词化、短**——摘要行 ≤20 字；flag help ≤1 行；占位符用 `<book_id>` 形式（clap 惯例）。
6. **示例真实可跑**——每条示例用当前真实语法（改子命令/flag 当次同步示例，people copy examples verbatim）；示例 1–3 条覆盖不同场景，不用 `[options]` 占位。
7. **简体中文**，错误前缀 `  err `、成功 `  ok `，不用 emoji（AGENTS 8.3）。
8. **一行一记录**——help 与运行输出同守 TUI.md：进度走 stderr、结果单行、指引仅未登录/未 Cookie 保留单行。
9. **顶层无板块标题**——分组只决定 `display_order` 顺序，直接平铺列出子命令（不打印 查询:/下载: 组名）；选项段标题 `Options:`（英文）。
10. **examples 默认关**——`ui.help_examples` 默认 `false`，示例段仅用户显式开启才出现；`Examples:` 标题保留加粗下划线样式。

---

### 2.1 config.ini 模板（config.rs::DEFAULT_INI）

- 分板块：每节前一行 **英文标题注释** `## Download` / `## Cache` / `## API` / `## Audio` / `## TTS` / `## Info` / `## UI`（`##` 是 ini 注释）；
- **节名本身保持小写英文**（`[download]`…）——必须与 `KEYS` 各条目节一致，改大小写会导致 `ini_write` 产生重复节（DEFAULT_INI 顶部注释有警告）；
- **每个键上方一行中文注释**说明作用与取值（配置说明唯一来源 = 本文件 + `CONFIG_HELP`，help 正文不列配置键）。

## 3. 机制（代码里的四张表 + 拦截链）

### 3.1 四表分工

| 表 | 位置 | 职责 |
|---|---|---|
| `TOP_GROUPS` / `TOP_OPTIONS` | `main.rs` | 顶层分组 help 的组定义与选项段 |
| `HELP_TAILS` | `main.rs` | 各子命令 help 底部追加示例段 |
| `FLAG_HELP` | `main.rs` | 逐 flag 详情 |
| `CONFIG_HELP` | `config.rs` | 逐配置键详情 |
| `CONVENTION/API_HEAD/API_TAIL/REQUESTS` | `helpfile.rs` | help.txt 的约定、api 格式、端点清单、命令→请求映射 |

#### 3.1.1 help.txt（helpfile.rs）

- `ensure_help_file()` 在 `Config::save_default()` 后调用：写 `~/.config/fqdt/help.txt`，首行标记 `fqdt <版本> -- 命令参考`，**版本变化才重写**（用户改动在版本升级时被覆盖）。
- **内容是解释性参考，不是 clap help 的复制**：每条命令给 `目的:`（about）+ `格式:`（usage 补全父命令路径）+ `参数:`/`选项:`（签名 + 作用说明，不带 example）+ `子命令:` 一览；末尾三节 = api 命令格式与各 flag 作用、端点清单（遍历 `fqapi::ENDPOINTS` 与 `--list` 同源）、**各命令底层 API 请求格式**（手写 `API_TAIL`：命令 → 实际发出的 GET/POST 路径与参数）。
- 渲染函数拆分遵守 ≤30 行：`render_cmd` / `render_args` / `render_subs` / `usage_of` / `arg_line` / `api_section`；输出统一 `strip_ansi` 后落盘。
- bool flag 不显示 value（`ArgAction::SetTrue` 等）；隐藏/自动 help/version flag 跳过。

## 3.2 拦截链（main() 中的顺序，不可乱）

```
args → try_flag_help（FLAG_HELP 逐 flag 详情，clap 解析前）
     → apply_help_tails + init_color
     → top_help_requested → render_top_help（顶层 help 手写渲染，exit 0）
     → clap 解析（子命令 help / flag 详情 / 运行）
```

- `top_help_requested`：args 空 / `["help"]` / 首个 `--help`-`-h` 前无所见子命令 → true；
  `fqdt search --help`、`fqdt help <cmd>` 走原 clap 路径。
- 改 help 时先确认落在哪一层，改对应表；**双份维护点**（doc 注释 vs TOP_GROUPS 摘要）靠内省消除，新增子命令只需：变体 + `display_order` + 归入某 `TOP_GROUPS` 行。

### 3.3 新增/修改子命令的 help checklist

- [ ] 变体 doc 注释 = 动词化摘要（会被顶层 help 内省）
- [ ] `display_order` 已设且与所在 `TOP_GROUPS` 组内顺序一致
- [ ] 归入 `TOP_GROUPS` 某一行（服务/查询/下载/管理/其他）
- [ ] flag help 短、无括号、取值语义明确
- [ ] 会取值的新 flag → `FLAG_HELP` 加详情行
- [ ] `HELP_TAILS` 示例段更新（真实语法、1–3 条、`ui.help_examples` 可关）
- [ ] 从属 flag（如 `--tone` 属 `--ai`）→ 多行 help 缩进 + `hide`
- [ ] 配置键变化 → `config.ini` 模板注释 + `CONFIG_HELP`（help 正文不写）
- [ ] 变体 doc 注释是否仍是**单行**（多行会破坏紧凑布局）？
- [ ] 新 flag/命令的格式与作用是否同步进 `helpfile.rs`（API 请求映射 `API_TAIL` 有变则更新）？
- [ ] config 键变化 → `DEFAULT_INI` 节标题与每键注释同步？
- [ ] 输出有变化 → 同步 `TUI.md` §2.1/§5
- [ ] 冒烟：`fqdt --help`、`fqdt <cmd> --help`、`fqdt <cmd> <flag> --help`、`fqdt help <cmd>` 四路

---

## 4. 优秀 CLI help 要点（调研摘录）

**clig.dev**
- `-h`/`--help` 全深度可用；无参运行显示**精简** help（一句话 + 一两个示例 + 指向 `--help`）。
- **Lead with examples**——用户先看示例；示例太多放别处（cheat sheet/文档），help 保持短。
- 常用命令/flag 排前面（git 式"常见命令"区）。
- 能猜到用户意图就给建议（`brew update jq` → 提示 `brew upgrade jq`），但别替他执行。
- 子命令一致性：同名 flag 同语义；名词-动词两级（`docker container create`）。
- stdout=数据、stderr=消息；非 TTY 关动画/颜色（NO_COLOR）。

**Vercel CLI Design Guidelines**
- help 走 stdout、exit 0、离线可用、**不鉴权不跳转**——用户在"出问题时"来读 help，此时它必须能用。
- 未知命令报错要**点名未知段并指向最近的有效帮助**。
- 顶层 help 按**任务分组**（deploying/domains/…），每个命令只出现一次（别名列别处、别名不列出）；无泛泛 overview、无"运行 help 查看更多"提示。
- 叶子 help 是**参考手册不是教程**：`Examples` 段 1–3 条覆盖不同作业，每条一个真实动作，绝不用 `[options]`。
- 默认值写 `(default: VALUE)`；段落间一空行；无色下 80 列仍可读。

**bettercli.org**
- help 三问：这是什么 / 从哪开始 / 怎么深入——新用户假设零背景。
- 无参运行可打印 help 作入门（jq 模式：精简版 + 指向 `--help` 全量）。
- man page 式段落：Name/Usage/Commands/Options/Examples。

**fqdt 落地差异**（有意为之）：
- 顶层 help 不放"运行 xxx --help"提示（不教 help，见 §2.2）；
- 配置键不进 help（唯一来源 config.ini 注释，见 §2.3）；
- 示例放 `HELP_TAILS` 而非 clap doc（集中管理 + `ui.help_examples` 开关）。
