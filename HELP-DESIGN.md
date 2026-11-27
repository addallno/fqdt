# fq help 设计与重写规范

> 面向修改 fq CLI 帮助文案的 agent。改任何 help/输出前必读；总输出规范见 `TUI.md`。
> 依据：clig.dev（Command Line Interface Guidelines）、Vercel CLI Design Guidelines、bettercli.org，
> 以及本项目 v0.16.0–v0.24.0 的迭代结论（去括号、不教 help、不列配置、分组顶层 help）。

---

## 1. 结构（四层 help，各管一段）

| 层 | 触发 | 载体 | 内容 |
|---|---|---|---|
| 顶层 help | `fq` / `fq --help` / `fq help` / `fq -h` | `main.rs::render_top_help`（手写）+ `TOP_GROUPS`/`TOP_OPTIONS` | 标题、用法行、**按任务分组**的子命令表、全局选项、优先级尾行 |
| 子命令 help | `fq <cmd> --help` / `fq help <cmd>` | clap derive doc + `HELP_TAILS` 追加示例段 | 一句话摘要、位置参数、flags（长 help）、示例 |
| flag 详情 | `fq <cmd> <flag> --help` | `main.rs::FLAG_HELP` / `config.rs::CONFIG_HELP` | 取值范围、语义、示例、关联查询/配置键 |
| config 键详情 | `fq config get <key>`（子命令式，m0162） | `CONFIG_HELP` | 同上（键专属） |
| help.txt 参考 | 启动自动生成 `~/.config/fq/help.txt` | `helpfile.rs` | 解释性参考：命令格式与目的、参数/端点作用、各命令底层 API 请求格式（**非** clap help 复制、**无** example） |

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
新增会取值的 flag 时**同步加详情行**，否则 `fq <cmd> <flag> --help` 退化为 clap 粗 help。

---

## 2. 文案规则（硬约束）

1. **去括号**——help 里禁止括号及括号内注释（v0.23.0 起，用户明确要求）；改用换行缩进或独立行表达补充信息。代码注释、运行输出不受此限。
2. **不教 help**——删除 `flag 详情: ...`、`用 fq xxx --help 查看` 类尾行（v0.24.0）。
3. **不列配置键**——help 不写 `配置: config.ini [x] y` 说明；配置说明唯一来源 = `config.ini` 分节模板注释（v0.24.0）+ `CONFIG_HELP`。
4. **不打迁移提示**——命令移除后直接未知命令 err，不打"已移除/请改用 xxx"（v0.24.0）。
5. **动词化、短**——摘要行 ≤20 字；flag help ≤1 行；占位符小写（m0108：不写 `<ID>`），**合法形式**（m0125，清单扩充 m0142）：单类型必填 `<id>`/`<key>`、联合必填 `<id|key>`/`<id|path>`、可选 `[path]`/`[id|path]`——位置参数支持多种类型时用 `|` 联合（如 download/update 的 `<id|path>` = id、书名或已下载目录；view/audio/info 支持书名关键词用 `<id|key>`，占位与描述类数必须一致 m0139/m0142）；对象多样用通名 `id` 不用 `book_id`。
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

### 1.4 子项缩进（SUBITEMS + 自定义渲染，v0.26.1）

- **子项** = 挂在某 flag 下的附属 flag（如 `--tone` 隶属 `--ai`、`--voice` 隶属 `--tts`），不是独立配置，help 中必须**缩进 +2 挂父下**，禁止当描述文本塞进父 flag 的多行 help（用户：作描述放那里=错误）。
- 实现：`SUBITEMS: &[(&str, &[&str])]`（main.rs）声明归属；`fq <子命令> --help` / `fq help <子命令>` 由 `render_sub_help()` 自定义渲染（about/Usage/Arguments/Commands/Options/尾部），`option_rows()` 让子项行 = 父行 +2 缩进、help 列按最长左列对齐；`Cli` 的 `global=true` flag（--timeout/-v）自动追加。
- `helpfile.rs`（help.txt）同步：`arg_left` 自含 2 空格前导，子项行缩进 +2，隐藏 flag 除非是子项否则跳过。
- **审查原则**：新增"父 flag + 附属 flag"结构必须登记进 `SUBITEMS`，两侧（自定义 help、help.txt）同步；`fq <cmd> <flag> --help` 详情拦截（FLAG_HELP）优先级更高，不受影响。

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

- `ensure_help_file()` 在 `Config::save_default()` 后调用：写 `~/.config/fq/help.txt`，首行标记 `fq <版本> -- 命令参考`，**版本变化才重写**（用户改动在版本升级时被覆盖）。
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
  `fq search --help`、`fq help <cmd>` 走原 clap 路径。
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
- [ ] 冒烟：`fq --help`、`fq <cmd> --help`、`fq <cmd> <flag> --help`、`fq help <cmd>` 四路

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

**fq 落地差异**（有意为之）：
- 顶层 help 不放"运行 xxx --help"提示（不教 help，见 §2.2）；
- 配置键不进 help（唯一来源 config.ini 注释，见 §2.3）；
- 示例放 `HELP_TAILS` 而非 clap doc（集中管理 + `ui.help_examples` 开关）。

---

## 5. 教程：如何写 help 与输出（面向 agent，m0152/m0153）

> 本章是"怎么做"的教学，规则出处见 §2 硬约束、§3 机制；改动前按 §3.3 checklist 走一遍。

### 5.1 写一条 help 的完整思考顺序

1. **它是谁**：这个子命令/flag 属于哪一层（顶层 / 父命令 / 父 flag）→ 决定写在哪个结构上（变体 doc / arg help / SUBITEMS / FLAG_HELP）。
2. **一句话它是干什么的**：动词开头、≤20 字、无括号（见 5.2）→ 写变体 doc 注释（顶层 help 靠内省取这行，双份维护靠内省消除）。
3. **位置参数怎么称呼**：通名小写 `<id>`（对象多义时）、具体对象才用专名；取值规则（"id 或书名关键词"）写 arg help 一个短语，深规则进 FLAG_HELP/详情。
4. **示例要不要**：默认关（`ui.help_examples`），开启时 1–3 条真实可跑、覆盖不同场景、绝不写 `[options]` 占位 → HELP_TAILS 登记。
5. **四路冒烟**：`fq --help` / `fq <cmd> --help` / `fq <cmd> <flag> --help` / `fq help <cmd>`。

### 5.2 括号内信息的归属（用户多次纠正的高频坑）

help 正文**禁止括号**（v0.23.0 起，§2.1）。发现想写 `目录（含 item_id）` 时，先分类再去该去的地方：

| 括号内容是什么 | 去处 | 反例 |
|---|---|---|
| 数据特征/产物形态（含 item_id、Opus 编码、需登录） | 参数/命令的 **arg help 短语**或 **FLAG_HELP 详情**独立成句 | 塞进 doc 摘要行括号里 |
| 操作指引（"按回车确认"、"再跑一次修复"） | **运行时输出**（stderr 单行尾注），不进 help | help 里教操作 |
| 同义配置键、默认值来源 | `CONFIG_HELP` + config.ini 模注（help 不列配置键，§2.3） | `--abr (config: audio.abr)` |
| 代码层面的兼容说明（旧格式、迁移） | 代码注释 / 运行时 verbose，不进 help | help 讲历史包袱 |

判据一句话：**help 是给"第一次用的人"的速查，不是给"维护者"的备注**——凡是解释"为什么/内部怎么"的括号，都不属于 help。

### 5.3 flag 层级与帮助项放哪一层

```
顶层 Cli        → global=true 的通用 flag（--timeout / -v）：各子命令 help 自动追加
Cmd 变体        → 该命令专属 flag（download 的 -t、audio 的 --lrc）
父 flag 子项    → SUBITEMS 登记（--tone 属 --ai、--voice 属 --tts），渲染缩进 +2 挂父下，
                  禁止把子项当描述文本写进父 flag 的多行 help（v0.26.1 教训）
互斥关系        → conflicts_with 在 arg 定义（--mp3 ↔ --keep-m4a），不写进 help 文案
```

**help 文本四写处，按读者深度选**：

| 读者动作 | 写处 | 形态 |
|---|---|---|
| 顶层扫一眼有哪些命令 | 变体 doc 注释 | 单行动词化摘要（多行破坏紧凑布局） |
| `--help` 看参数列表 | `arg(help=...)` | ≤1 行短语，无括号 |
| `<flag> --help` 查深规则 | `FLAG_HELP` 表（brief + fields） | brief 一句 + fields 分行 |
| config.ini 配键 | `CONFIG_HELP` + DEFAULT_INI 注 | help 正文永远不写 |

### 5.4 输出信息三类文案（详见 TUI.md，此处给模式）

- **过程**：`util::progress("获取目录")` → stderr，TTY 才显示，完成 `clear_progress`；一阶段一短语，不打 URL/参数明细（进 `-v` verbose）。
- **结果**：一行收据，量词先行——`  ok 1 章 · 跳过 0 章`、`  新增 44 章音频 · 已有 482/526 章`、`  已是最新 · 共 526 章`。成功 `ok`、事实陈述无前缀。
- **失败**：`  err ` 前缀 + **可执行的下一步**（"请手动在 info.list 补 book_id 后重试"），不是自哀（"出错了抱歉"）；多失败聚合 `util::print_failures`（≤3 条+计数+重跑提示）。
- **警告**：`  warn ` + 降级后果，一章一次（`warn_fallback` 去重），不逐条刷屏。
- 三类全部走 stderr 或收据行规则（TUI.md 一行一记录）；stdout 只有数据。

### 5.5 哪些 subcommands 该合并/简化（判定标准 + fq 案例）

**判定标准**（按序问，命中即动）：

1. **同一对象？** 两条命令操作同一对象的不同属性 → 合并为父+子（`comments`/`author` 是"查书"的一部分 → `info <id> comments|author`，v0.13.0）。
2. **只读查询？** 顶层只读查询流 → 并入最贴近的聚合命令（`recommend`/`category` 并入 `info`，v0.24.0；`folders`/`history` 等 account 只读 → `info` sections，v0.24.0）。
3. **一层壳？** 子命令无自身逻辑、只是转发参数 → 删掉或并入（`function` 整个删除，v0.24.0——管道需求 `api` 已覆盖）。
4. **顶层膨胀？** 顶层命令 >7 个可扫读单位 → 按任务分组进 `TOP_GROUPS`，能归对象的归对象（服务类留顶层：serve/api/config/login）。
5. **总是一起用？** A 的输出恒为 B 的输入 → B 做 A 的 flag（`search -D` 自动续下载，而非独立 `dl` 命令）。

**反向（不该合）**：不同对象（download/update 都是"书"但语义相反，保留）；破坏肌肉记忆无收益（fq 别名体系 s/d/a/t/u 稳定）；合并后 arg 冲突（位置参数语义不同）。

**简化输出侧**：每加一个子命令先想"结果行能否一行说完"——说不完的交互（选书序号）非 TTY 必须有报错指引而非挂死。

### 5.6 措辞统一性（m0153 子项）

- **动词/名词统一**：摘要行一律动词开头（`下载正文`、`搜索书籍`），对象名词只做宾语；Usage 行里子命令名本身是名词（`download`/`search`），不因摘要动词而改名——动词化约束只作用于 doc/help 文案，不作用于命令名。
- **Usage 行格式**：`Usage: fq <cmd> [选项] <参数>`——子命令路径全写（help.txt 的 `usage_of` 补全父路径）；选项段 `Options:`、参数段 `Arguments:`、子命令段 `Subcommands:`（**一个词**、带冒号，m0115 纠正 m0151 的 `Sub commands:`）；bool flag 不占 value 位；占位符按 §2.5 规则 5 的合法形式。
- **顶层 help 不写"优先级"尾行**（m0117）：flag > config 优先级说明不进顶层输出，需要时只放子命令 help text / FLAG_HELP 详情 / config.ini 模注。
- **子命令排序**：顶层顺序 = `display_order`（与 `TOP_GROUPS` 组内顺序一致，§3.3）；组内常用在前；**同一命令在任何视图只出现一次**（别名列在定义处、不进帮助列表）。子命令 help 的 Options 段同理：专属 flag 在前、global flag（-v/--timeout）自动追加在后。
- **错误文案与 help 一致性**：同名 flag 同语义（跨命令）；报错里提到的 flag/子命令必须与 help 中拼写完全一致（含大小写与占位符），用户照抄报错应能直接跑通——改名时 grep 报错文案与 help 同步（`grep -rn "旧名" src/*.rs` 含 err! 宏）。

### 5.7 新写 help 时的自检（并入 §3.3）

- 括号清零：`grep -n '(' 对应 help 文案` 逐条过 5.2 表
- 占位符全小写、通名 `<id>`；段标题 `Subcommands:`（非 `Commands:`，m0151）
- 同名 flag 同语义（跨命令一致性）；别名只在变体声明、不进列表文案
- 改完四路冒烟 + `strip_ansi` 后 help.txt 版本变化才重写
