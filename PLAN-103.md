# PLAN-103 view 渲染与交互批量（TODO 103 + 102）

一步一验，每步：动作 + 验收命令，三绿后 commit。

1. **config 四键**：ui.gutter(1)/ui.scroll_edge(25)/ui.tail_blank(1)/ui.anim_frames(3)
   验收：`cargo test --bin fqdt` 含新键 get/set 测试绿
2. **gutter 应用**：normal 正文框、zen 四边、help 框内边距 + 多行段落续行距框（3⑩）
   验收：TestBackend 断言行 x 偏移；pty 目视
3. **tail_blank**：页尾空行数走 config（11）
   验收：单测断言页尾行数
4. **状态栏+标题**：5 格式 `138/173章 · 1/199行`；8 顶行去书名统一；框标题 normal 书名替"正文"、zen 书名+章题（14）；9 标题栏下空一行
   验收：TestBackend 断言 title 文本
5. **翻页修复**：1 虚空行、4 跳行、7 衔接、E 续行跨页
   验收：单测页边界行集
6. **跨章触发**：6 四触发（首末页←→+首末行滚动）
   验收：单测 state 转移
7. **yazi 滚动**：2 edge_ratio 边缘区内容平移光标不动
   验收：单测光标停驻+行偏移
8. **help**：12 三件（<Esc>/? 写入、提亮、gutter+缩进）+ F 超屏滚动
   验收：TestBackend 断言
9. **动画**：15 过渡动画（anim_frames 帧，翻页滑动+滚动平滑，克制）
   验收：帧渲染不崩+可关（0）；pty 目视
10. **102 RTL**：bidi 缓解评估（修不了则 help/README 标注）→ 三绿 → 发版链 v0.37.1 或 0.38.0
