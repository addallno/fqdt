# fqdt 终端输出设计规范（TUI Rule）

> 所有 CLI 输出改动必须遵守本文件。修改前先读，评审时对照检查。
> 原则来源：clig.dev（CLI Guidelines）+ tui-design skill（终端 UI 工艺），结合 fqdt 现状裁剪。

## 0. 一句话原则
**人看的信息要少而准，机器用的信息要纯而稳。** 每一行都应该"挣得"它占据的位置。

## 1. 流的分工（不可违反）
| 流 | 内容 | fqdt 现状 |
|----|------|-----------|
| stdout | 结果数据：搜索列表、章节正文、`--json`、`api` 透传 body | println / json 输出 |
| stderr | 日志、进度、错误、状态、交互提示 | eprintln + `util::progress` |
- core 日志默认**不打印**（仍落盘 `rust.log`），仅 `-v/--verbose` 时镜像到 stderr。
- 过程性进度一律走 `util::progress()` / `util::clear_progress()`（stderr、仅 TTY 打印、完成后原地清除再打收据）；绝不让动画混入 stdout。
- 管道场景（`| grep`、`| jq`）stdout 必须干净：无颜色污染数据行。

## 2. 信息分层（色彩语义，effective info 才上色）
1. **主体**：`CYAN` 标题/序号/书名，`MAGENTA` 次主体（作者/分类/评论用户）
2. **强调**：`YELLOW` 热度/评分/赞数（读者最关心的数字）
3. **状态**：`GREEN` 成功收据 `ok`，`RED` 错误 `err`，`BLUE` #id/链接类标识
4. **弱化**：`DIM` 提示/尾注/辅助信息；正文用默认色
**颜色常量只能从 `src/util.rs` 引**（RST/DIM/CYAN/YELLOW/MAGENTA/GREEN/BLUE/RED），禁止裸写 `\x1b[...]`。
**开关**：`util::init_color()`（main 里 Config 加载、`ui_color` 检查后调用；`ui.color=false` 等价 NO_COLOR）——`NO_COLOR` 非空 / `CLICOLOR=0` / stdout 非 TTY → 纯文本输出。

## 2.1 help 消息规范（细则见 `HELP-DESIGN.md`）
- 简短介绍只写一句用途，**不枚举子项/子 flag**（`--help` 会补全）；flag help 必须可操作（含义、单位、取值范围、默认值、与其他 flag 关系）。
- **四层结构**（v0.24.0）：顶层分组 help（`render_top_help` 手写：查询/下载/管理/服务/其他五组）→ 子命令 help（clap doc + `HELP_TAILS` 示例段）→ flag 详情（`FLAG_HELP`）→ config 键详情（`CONFIG_HELP`）。每层不越界。
- **文案硬约束**：去括号、不教 help（无 `flag 详情:` 尾行）、不列配置键（配置说明唯一来源 = `config.ini` 分节模板注释）、不打迁移提示（命令删除直接未知命令 err）。
- **flags 先于 configs**：顶部注明 `优先级: 命令行 flag > config.ini 同义项`。
- help 与行为同步：子命令语义变更**当次发版必须改 doc**，不许遗留旧描述。
- **逐 flag 详情**：四场景 `fqdt audio --tone --help`、`fqdt help audio --lrc`、`fqdt download -r --help`、`fqdt config tone --help` 由 `FLAG_HELP`（main.rs）/`CONFIG_HELP`（config.rs）单点表拦截，渲染 usage+brief+字段表（类型/取值/默认/说明/互斥）；带值的 `--tone 1 --help` 走 clap 原生子命令 help；`config <键> --help` 支持点号键（多候选列清单）。
- 示例段受 `ui.help_examples=false` 关闭（`HELP_TAILS` + `apply_help_tails` 单点定义）；示例必须真实可跑、1–3 条覆盖不同场景。

## 3. 每个命令的四问（Vercel CLI 四问裁剪）
任何输出都要回答：
1. **作用于什么** —— 目标（书名/ID/路径），在结果首行
2. **发生了什么** —— 结果计数与去向：`ok 2 章 · 跳过 0 章`
3. **结果在哪** —— 文件路径 / 下一页命令
4. **下一步** —— 仅交互场景给一条提示；非交互不给

## 4. 行文风格
- 成功输出**短**：一行能说清不用两行；`·` 做分隔符（`ok 2 章 · 跳过 0 章`），不要逗号+多行。
- 耗时信息默认不打印（用户需要时用 `time fqdt ...`）；搜索场景的秒数因代表体验可保留在头部行尾。
- 中文 `  err ...` / `  ok ...` 前缀（两空格缩进），错误信息含**原因 + 建议**（如 `err 无效序号`+重新输入提示）。
- 噪音禁入 stdout/stderr：`[I] core init` 这类内部日志只在 `-v` 出现。
- 过程进度用 `util::progress("获取目录")`（stderr TTY），完成后 `util::clear_progress()` 再打结果收据；不要往 stdout 写进度。

## 5. 列表与表格
- 一条记录默认**一行**（多行记录会破坏 grep 逐行语义；确需展开时提供 `--json`）。
- 对齐用固定宽度列（`{:>2}` 序号），中文列宽用 `pad`/`clip` 辅助（discover.rs 已有）。
- 分隔符首选 `·`，其次 `|`；空状态必须给一句话（`无结果`、`（无推荐数据）`），不留白屏。
- 分区标题：`【标题】` 或 `{CYAN}`浓色，用一个风格并全项目统一（现用 `【推荐】`）。
- **云端书架一层视图**（v0.24.0，`shelf_top`）：顶层只列**未分组书行** + `【组名】N 本` 分组条目行（已分组的书不直接显示）；`shelf list <组名>` 展开该组书行；`--search`/`--author` 展平过滤。头行 `ok 书架 N 本` 或 `ok 书架 N 本 · M 组`。
- **指引尾行纪律**（v0.24.0）：运行输出不打 tips 尾行（能力:/查询:/查看: 等 DIM 行全删）；仅未登录 / 未设置 Cookie 保留**单行**指引。

## 6. 符号与进度
- 状态符号：`✓` 成功 `✗` 失败（进度条消息内已用）；主输出别滥用符号。
- 进程操作（>200ms）必给进度：`util::progress()`；批量失败用 `util::print_failures(errs, hint)`（最多列 3 条 + `失败 N 项` 计数 + 一条重跑提示），不逐条刷屏。
- **阶段文案**：每个 >200ms 的阶段先 `util::progress("阶段名")` 再干活、完了 `clear_progress()`——阶段名要说清到哪一步了（如 `启动 core`、`获取目录`、`批量获取正文（N 章）`、`批量获取音频信息（N 章）`、`生成 EPUB（N 章）`、`搜索 "kw" · 第N页`）。
- **动画形态**：`util::progress()` 是 Braille spinner 动画（帧 `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`，120ms，原地 `\\r` 覆盖，完成后 `clear_progress()` 擦行）；带总量的循环用 `with_progress_async` / `ProgressBar`，bar 模板含 `{spinner}` + `{percent}`（百分比）+ `{pos}/{len}` + `{msg}`（章号 `✓0001`/`✗0002`）。
- 非交互（stdout 非 TTY）时：颜色关闭（NO_COLOR/CLICOLOR≈221）、进度动画静默——管道里不产生动画。

## 7. 美化的边界
- 装饰要有目的：一个醒目的签名元素（如 `【推荐】` 分区头）胜过处处点缀。
- 禁止：满屏分隔线、每行一个颜色、emoji 滥用、ASCII 大图。
- 对齐优先于着色：列不齐时先修列宽，再考虑颜色。

## 8. 检查清单（提交输出相关改动前过一遍）
- [ ] stdout 里有没有日志/进度/错误混入？
- [ ] `| grep` 和 `| jq` 跑一遍，数据行是否干净可解析？
- [ ] 颜色是否全部引自 util 常量？是否按语义分层（主体/强调/状态/弱化）？
- [ ] 默认输出是否 < 3 行"非数据"文本？
- [ ] `-v` 是否能看到完整日志？非 `-v` 是否一行日志都看不到？
- [ ] 错误信息是否包含原因与下一步建议？
- [ ] 空结果/无数据是否有明确一句话？
