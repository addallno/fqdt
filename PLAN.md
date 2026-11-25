# PLAN — help 输出重构 + info 子命令化 + help.txt + config 模板（v0.26.0）

目标（用户 m0049）：紧凑 help（短介绍同行、不逐条回车、去板块标题、Options 英文、examples 默认关）；
info 的 section 变真子命令且 flag 按归属分布；配置目录生成 help 参考文件（API 详解）；
config.ini 英文节标题 + 每键注释；同步 HELP-DESIGN.md。

## 步骤与验收
1. [x] AGENTS 7.2 新增待办 52（先更新待办再实现，AGENTS 8.1）
2. [x] 顶层 help：删 TOP_GROUPS 板块标题行与组间空行，"选项:"→"Options"；验收 `fqdt --help`
3. [x] 全命令 doc 注释压成单行，消除 clap long_about 长格式；验收 `fqdt info --help`/`fqdt login --help` 每条 flag 单行、无空行
4. [x] examples 默认关：types.rs default false + DEFAULT_INI 注释/false + CONFIG_HELP；验收新模板下 help 无示例段
5. [x] info 子命令化：InfoCmd（overview/chapters/intro/author/comments/cover/relative/audio/recommend/category/folders/history/progress/user/highlights），flag 归各子命令；验收 6 条真实命令 + 裸 info 报用法
6. [x] help.txt：src/helpfile.rs 启动 ensure（版本变更才重写），含完整命令树与 flags、API 请求专章；验收删文件后生成且含 API 章
7. [x] config.ini 模板：`## Download` 式英文标题 + 每键中文注释补全；验收临时 HOME 生成内容
8. [x] HELP-DESIGN.md 更新（紧凑规则、无板块标题、Options、examples 默认关、info 子命令、help.txt、config 模板）
9. [x] 质量门：build / clippy 0 / test 全过；四路 help 冒烟；`fqdt api --help` 等

---

# PLAN — 91 view 交互大重设计（含 83/85/88/89/90/84阅读器，m0130/m0132/m0133/m0142）

目标：read.rs 交互全面翻新——新键位、段落光标、左右/上下两渲染模式、状态行自适应、
zen 四边框（与正文隔一点点距离）、?/h 全屏帮助框、目录高亮截断、config 默认启动模式、
阅读器主题。改动期只 `cargo check`，全部完成后三绿 + 攒批发版（含 94 延伸）。

| # | 步骤 | 动作 | 验收 |
|---|------|------|------|
| 1 | 键位重设计 | `on_key`：`<>` 切章；方向键=翻页（左右渲染模式下）；`,`/`.` 翻 2/3、1/2 页；`space` 改查图（光标在图片上才触发）；q/z/Tab/t/i/Esc 保留 | `cargo check` 0；`on_key` 单测映射表 |
| 2 | 段落光标 | `Reader` 加段落模型（段类型 Text/Image + 行区间）与光标段索引；上下键移光标（**不同高亮色**）；选中图片不高亮且自动跳下一行→无则上一行→无则停留 | `cargo check` 0；光标移动/图片跳行单测 |
| 3 | 左右渲染模式 | `ui.page_mode` 默认 RightLeft（90）：上下滚到屏底再按自动翻页；左右键平行翻页、光标落大致相同段 | `cargo check` 0；翻页/光标对齐单测 |
| 4 | 上下渲染模式 | UpDown：选中段保持屏内（上/下 1/4 触发整体平移不许超出）；左右键整屏切换并平移当前行 | `cargo check` 0；1/4 平移边界单测 |
| 5 | 状态行自适应 | 顶行 `138/172 章 · 行 1/224` 宽度不足→紧凑→消失，**绝不卡半截** | 不同宽度渲染单测（无半截断言） |
| 6 | zen 四边框 | 沿屏幕四边渲染框；标题作框标题（「目录」「正文」风格，同 normal）；**正文与框四边隔一点点距离**（m0133）；状态下方 | `cargo check` 0；zen 渲染单测（框/留边断言） |
| 7 | ?/h 帮助框 | 全屏一个大框（标题 `--Help--`），分部分叙述；快捷键说明只在 readme/help.txt，view 内不写 | pty/expect 按 `?` 见大框；help.txt 含快捷键 |
| 8 | 83/85/88 杂项 | 83 目录选中高亮截断显示完整章题+箭头；85 非 zen 去底部 helpbar；88 key tip 怪字符 "5" 修复 | 各单测/渲染断言 |
| 9 | 89/84 config | 89 config 默认启动模式（zen/normal）；84 阅读器主题（仅阅读器：正文字/高亮/边框/状态色，`ui.theme` 预设） | config 单测 + 默认值断言 |
| 10 | 攒批发版 | 全部完成→三绿→发版链（含 94 延伸 render_history/render_folders） | 远端 `--version` 证据 |

依赖：1→2→3/4（光标模型是翻页基础）；5/6/7 可并行于渲染层；8/9 独立；10 收尾。
