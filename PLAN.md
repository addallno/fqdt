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
