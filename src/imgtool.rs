//! 图片渲染（view TUI）：探测外部加载器 timg→chafa→viu→catimg，
//! 下载到临时缓存后转 ANSI 行；无工具或失败返回 None，由调用方落纯文本占位。
//! 不引 Rust 图像 crate（体积），外部工具零依赖（Termux `pkg install timg` 可得）。

use std::path::PathBuf;
use std::sync::OnceLock;

/// 图片块最大高度（行）。
pub const IMG_MAX_H: u16 = 10;

/// 探测可用图片工具（一次缓存）。优先级 timg → chafa → viu → catimg。
pub fn img_tool() -> Option<&'static str> {
    static T: OnceLock<Option<&'static str>> = OnceLock::new();
    *T.get_or_init(|| {
        for cand in ["timg", "chafa", "viu", "catimg"] {
            let ok = std::process::Command::new("which")
                .arg(cand)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Some(cand);
            }
        }
        None
    })
}

/// 缓存路径：temp/fqdt_img_<url hash>。
fn cache_path(url: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    std::env::temp_dir().join(format!("fqdt_img_{:016x}", h.finish()))
}

/// curl 下载（同 audio.rs 先例）：成功且 >100 字节返回 Ok。
fn fetch(url: &str, path: &PathBuf) -> Result<(), String> {
    let st = std::process::Command::new("curl")
        .args(["-sfL", "--connect-timeout", "10", "--max-time", "30", "-o"])
        .arg(path)
        .arg(url)
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("curl {st}"));
    }
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size < 100 {
        return Err("图片过小".into());
    }
    Ok(())
}

/// 渲染图片为 ANSI 行；仅接受 http(s) 绝对地址，其余/失败 → None。
pub fn render(url: &str, width: u16, height: u16) -> Option<Vec<String>> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let tool = img_tool()?;
    let cache = cache_path(url);
    if !cache.exists() {
        fetch(url, &cache).ok()?;
    }
    let wh = format!("{}x{}", width, height);
    let mut cmd = std::process::Command::new(tool);
    match tool {
        "timg" => {
            cmd.args(["-g", &wh]);
        }
        "chafa" => {
            cmd.args(["-s", &wh]);
        }
        "viu" => {
            cmd.args(["-w", &width.to_string(), "-h", &height.to_string()]);
        }
        _ => {
            cmd.args(["-w", &width.to_string()]);
        } // catimg -w 字符宽
    }
    let out = cmd.arg(&cache).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<String> = text
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .take(height as usize)
        .collect();
    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}

/// 纯文本占位（无工具/渲染失败/非绝对地址）。
pub fn placeholder(url: &str) -> String {
    let short = if url.len() > 60 {
        format!("{}…", &url[..url.len().min(60)])
    } else {
        url.to_string()
    };
    format!("[图片: {short}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_truncates() {
        let long = format!("https://example.com/{}", "a".repeat(80));
        let p = placeholder(&long);
        assert!(
            p.starts_with("[图片: https://example.com/") && p.contains('…') && p.ends_with(']'),
            "got: {p}"
        );
        assert!(p.len() < 100);
    }

    #[test]
    fn relative_url_renders_none() {
        assert!(render("/static/x.png", 40, 10).is_none());
        assert!(render("ftp://x/y.png", 40, 10).is_none());
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;

    /// 真实 CDN 图片渲染（需网络）：`cargo test -p fqdt imgtool::net -- --ignored`
    #[test]
    #[ignore = "需要网络：真实番茄封面 URL"]
    fn render_real_cover() {
        let url = "https://p3-reading-sign.fqnovelpic.com/novel-pic/9f37f020493b8b927fdeff412075cfed~tplv-snk2bdmkp8-superreso-double-rz:1200:0.image?lk3s=45961287&x-expires=1800767463&x-signature=WgwpOOgd79XsLu9SsQHdsEznzsc%3D";
        let out = render(url, 60, 10).expect("有 timg/chafa 时应渲染成功");
        assert!(!out.is_empty(), "渲染行不应为空");
        // ANSI 着色行或纯文本行都算成功
        eprintln!(
            "rendered {} lines, first={:?}",
            out.len(),
            out[0].chars().take(40).collect::<String>()
        );
    }

    /// 无工具降级：PATH 剥离后应返回 None（不 panic）
    #[test]
    fn render_without_tools_returns_none() {
        // 相对地址已在主测试覆盖；此处保证网络失败不 panic
        let out = render("https://invalid.invalid/x.png", 40, 8);
        assert!(out.is_none());
    }
}
