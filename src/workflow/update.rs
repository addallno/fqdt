use crate::types::{Config, DownloadParams};
use crate::workflow::download;

/// # update — 增量更新（已静默并入 download，m0059 拍板选二）
///
/// 本命令为 download 的薄封装，行为完全转调、不打提示（保持肌肉记忆与别名 u）：
/// - `<目录>` → `download::run_dir`：正文增量 + 目录含 Audio/ 时**一次同时**音频增量
/// - `<id>`   → `download::run`：Downloader 按文件名跳过已有文件，**中间缺章也补**
/// - `-T/--type content|audio[,…]`（m0064）：分类增量——只做指定类别，默认全部；
///   id 模式 `-T audio` 转 `audio::run`（download id 只会正文+附加音频，无法只音频）
///
/// # 调用的原子函数
/// - `api.fetch_catalog()`、`download::Downloader::run()`、`audio` 工作流
///
/// # CLI 调用
/// - `fqdt update <id|path> [-T content,audio] [-r 范围] [-o 目录] [-j 并发] [-f]`
pub async fn run(target: &str, output: Option<&str>, range: Option<&str>,
                 only: Option<&str>, concurrent: Option<usize>, force: bool,
                 verbose: bool, cfg: &Config) {
    // 类别归一：仅接受 content/audio（大小写不敏感、去重）
    let only_norm = match only {
        None => None,
        Some(s) => {
            let mut kinds: Vec<String> = Vec::new();
            for p in s.split(',') {
                let p = p.trim().to_ascii_lowercase();
                if p.is_empty() { continue; }
                if p != "content" && p != "audio" {
                    eprintln!("  err 未知增量类别 {}（可用 content/audio）", p);
                    return;
                }
                if !kinds.contains(&p) { kinds.push(p); }
            }
            if kinds.is_empty() {
                eprintln!("  err -T 需要 content 或 audio");
                return;
            }
            Some(kinds.join(","))
        }
    };

    let p = DownloadParams {
        output: output.map(|s| s.to_string()),
        range: range.map(|s| s.to_string()),
        format: None,
        concurrent,
        audio: false,
        tone: cfg.audio_tone,
        abr: cfg.abr,
        lrc: String::new(),
        force,
        verbose,
        book_title: None,
        only: only_norm.clone(),
    };

    let path = std::path::Path::new(target);
    // id + 只音频 → audio 工作流（download id 模式无法只下音频）
    if !path.is_dir() && only_norm.as_deref() == Some("audio") {
        return crate::workflow::audio::run(
            Some(target), output, range, cfg.audio_tone, verbose,
            None, "zh-CN-XiaoxiaoNeural", None, None, None, None, None, false, None,
            "external", force, concurrent, 0, cfg).await;
    }
    download::run(target, &p, cfg).await;
}
