# PLAN: info 子命令扩展 + 命令重组（v0.13.0）✅ 全部完成 2026-11-13

用户决策（question 确认）：
- 形式：`fqdt info <book_id> [SECTION]` 可选 section 位置参数；无 section = 现有概览
- hots / shelf：本轮不做
- **breaking**：`comments` / `author` 顶层命令删除，迁入 info section

| # | 动作 | 验收 |
|---|------|------|
| 1 | `info.rs` 加 section 分发：`chapters`（目录带 item_id）/ `intro`（完整简介）/ `author`（→ discover::author，id 取 detail `bind_author_ids` 或 `author_id` 去前缀）/ `comments`（→ discover::comments, count=20）/ `cover`（detail `thumb_url`）；未知 section 报错列合法值；概览尾部加 sections 提示 | `fqdt info <id> chapters` 显示 `NNNN title #item_id`；5 个 section 实测过 |
| 2 | `main.rs` Info 加 `section: Option<String>` 位置参数；删 `Cmd::Comments`/`Cmd::Author` 变体、clap 定义、分发 | `--help` 不再出现 comments/author；编译过 |
| 3 | build / clippy / test 全绿 + 实测 5 section + 删后旧命令报错提示合理 | 命令输出证据 |
| 4 | AGENTS（1.5.2 封装段、7.2 待办、版本、breaking 注记）+ 版本 v0.13.0 | 文档一致 |

实现要点：
- detail JSON 走 `fqapi::get_json("/api/v1/books/{id}", "")`（discover::fetch 同款，零新 API）
- discover.rs 的 comments/author **函数保留**（info 调用），只删顶层 CLI 入口
- section 分支与现有 `-r`/`-s` 互不干扰：chapters 尊重 `-r`，其余 section 忽略
