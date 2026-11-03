# fqdt 项目进度

## 当前版本

v0.5.1（GitHub Actions CI 自动构建）

## 已完成功能

### 核心功能
- **搜索**: 关键词搜索 + 分页 + 交互下载
- **下载**: 正文并发下载（线程池 + round-robin URL 轮换）
- **音频**: 语音下载 + 音色回退 + ABR 流式压缩
- **TTS**: edge-tts 文本转语音（文件/目录批量）
- **更新**: 增量更新（正文/音频）+ 哈希比对跳过
- **书架**: 收藏管理 + 一键更新
- **info**: 目录查看 + 正文预览

### 格式与后处理
- **EPUB**: 零系统依赖生成（zip 库）
- **LRC**: 自动生成 + ID3v2 USLT 嵌入
- **封面**: APIC 嵌入（jpg/png）
- **压缩**: lame subprocess ABR 压缩
- **后处理**: 用户自定义命令模板

### 架构
- **CLI**: 6 个子命令 + function 子命令
- **配置**: INI 解析 + 书架持久化
- **CI**: GitHub Actions musl 静态编译（x86_64 + aarch64-unknown-linux-musl，静态链接）
- **智能源切换**: URL 按成功率+延迟排序，失败自动降级

## API 现状（2026-08-30 验证）

| 功能 | 状态 | 说明 |
|------|------|------|
| 搜索 | ✅ 可用 | `novel.snssdk.com` 官方 API |
| 目录 | ✅ 可用 | `fanqienovel.com/api/reader/directory/detail` |
| 正文 | ⚠️ 需代理 | 官方 `/api/reader/full` 返回空 body + `bdturing-verify` 验证码 |
| 音频 | ⚠️ 需代理 | 依赖第三方服务器 |
| 书籍详情 | ⚠️ 需代理 | `i.snssdk.com` 返回空 |

### 字体混淆机制

fanqienovel.com 对正文内容实施字体混淆加密：
- API 返回的文本中，部分 Unicode 字符被替换为其他码位
- 响应头 `x-tt-zhal` 包含字体映射配置
- 浏览器通过 `addFromConfigString()` 加载自定义 CSS @font-face 视觉还原
- 需要逆向字体映射才能在 CLI 中还原真实文本

## 待完成事项

### 高优先级
1. **正文内容获取方案** — 解决 ByteDance 图灵验证拦截问题
   - 方案 A: 住宅 IP 代理
   - 方案 B: Headless 浏览器（Puppeteer/Playwright）
   - 方案 C: 字体映射逆向 + API 绕过
2. **字体解混淆** — 实现 `x-tt-zhal` 字体映射逆向还原
3. **API 代理层** — 可选的本地/远程代理服务

### 中优先级
4. **代码重构** — 大函数拆分为基本函数
5. **高阶函数** — `with_retry`、`with_cache`、`with_progress` 封装
6. **workflow 模块** — 预编译工作流从 main.rs 移出

### 低优先级
7. **配置驱动扩展** — `[function]`、`[workflow]` 节
8. **Rust 原生 lame** — 消除外部 lame 依赖
9. **clippy 清理** — 解决现有 warnings

## 已知限制

1. **正文获取依赖第三方代理** — 官方 API 有反爬机制
2. **音频依赖外部服务** — 第三方服务器可能不稳定
3. **TTS 需要 Python 环境** — edge-tts 是 Python 包
4. **交叉编译** — 本地只能 debug 模式，release 需 GitHub Actions

## 未来可能添加的功能

### API 增强
- 批量内容 API（一次请求多章）
- 书籍元数据 API（封面、分类、标签）
- 用户书架 API（收藏列表同步）
- 章节推荐 API

### 功能扩展
- 批量下载（书单/合集）
- 断点续传
- 自动更新检查
- 多格式输出（mobi、azw3）
- 书籍封面下载
- 阅读进度同步

### 用户体验
- 交互式选择界面优化
- 下载历史记录
- 书签/笔记导出
- 自定义主题/配色

### 技术优化
- 异步 HTTP（reqwest 异步模式）
- 连接池复用
- 增量更新优化
- 内存映射大文件处理
