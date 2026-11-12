use crate::types::{sanitize_filename, Chapter};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// 终端样式常量：所有 CLI 输出的统一配色入口
/// 颜色总开关：`main()` 启动时由 [`init_color`] 按 NO_COLOR/CLICOLOR/TTY 设置。
static COLOR_ON: AtomicBool = AtomicBool::new(true);

/// 初始化颜色开关：NO_COLOR / CLICOLOR=0 / stdout 非 TTY → 纯文本（TUI.md §6）。
pub fn init_color() {
    use std::io::IsTerminal;
    let off = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
        || std::env::var("CLICOLOR").is_ok_and(|v| v == "0")
        || !std::io::stdout().is_terminal();
    COLOR_ON.store(!off, Ordering::Relaxed);
}

/// 语义色（Display：开关关闭时输出空串），format 里按原写法 `{CYAN}` 使用。
pub struct Color(&'static str);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if COLOR_ON.load(Ordering::Relaxed) {
            f.write_str("\x1b[")?;
            f.write_str(self.0)?;
            f.write_str("m")
        } else {
            Ok(())
        }
    }
}

pub static RST: Color = Color("0");
pub static DIM: Color = Color("2");
pub static CYAN: Color = Color("1;36");
pub static YELLOW: Color = Color("33");
pub static MAGENTA: Color = Color("35");

pub fn has_chapter_prefix(title: &str) -> bool {
    title.starts_with('第') || title.starts_with(|c: char| c.is_ascii_digit())
}

pub fn chapter_heading(ch: &Chapter) -> String {
    if has_chapter_prefix(&ch.title) {
        ch.title.clone()
    } else {
        format!("第{}章 {}", ch.index, ch.title)
    }
}

pub fn format_filename(template: &str, ch: &Chapter) -> String {
    template
        .replace("{idx04}", &format!("{:04}", ch.index))
        .replace("{idx}", &ch.index.to_string())
        .replace("{title}", &sanitize_filename(&ch.title))
}

pub fn filter_by_range<'a>(
    chapters: &'a [Chapter],
    range: Option<&crate::types::ChapterRange>,
) -> Vec<&'a Chapter> {
    chapters
        .iter()
        .filter(|c| range.is_none_or(|r| r.contains(c.index)))
        .collect()
}

pub fn bar_style(colors: &str, _concurrent: usize) -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(&format!(
            "[{{elapsed_precise}}] {{bar:24.{colors}}} {{pos}}/{{len}} {{msg}}"
        ))
        .unwrap()
        .progress_chars("━▶")
}

/// stderr 进度行（`  msg …`）。仅 TTY 打印，完成/失败后用 [`clear_progress`] 消除，
/// 保证 stdout 只剩结果数据、stderr 不留过程残行。
pub fn progress(msg: &str) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprint!("  {DIM}{msg} …{RST}");
        use std::io::Write;
        std::io::stderr().flush().ok();
    }
}

/// 擦除 [`progress`] 留下的当前行。
pub fn clear_progress() {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[2K\r");
    }
}

pub fn json_esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub async fn with_retry_async<F, Fut, T>(f: F, max: u32) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut last_err = String::new();
    for i in 0..max {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e;
                if i + 1 < max {
                    tokio::time::sleep(Duration::from_secs(1 << i)).await;
                }
            }
        }
    }
    Err(format!("重试{}次后失败: {}", max, last_err))
}

pub async fn with_verbose_async<F, Fut, T>(f: F, label: &str, enabled: bool) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    if enabled {
        eprintln!("  [verbose] {} 开始", label);
    }
    let result = f().await;
    if enabled {
        eprintln!("  [verbose] {} 完成", label);
    }
    result
}

pub async fn with_progress_async<T, U, F, Fut>(
    items: Vec<T>,
    total: usize,
    concurrent: usize,
    colors: &str,
    work: F,
) -> (usize, usize, Vec<String>)
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T, ProgressBar) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<U, String>> + Send,
{
    if items.is_empty() {
        return (0, 0, Vec::new());
    }

    let pb = ProgressBar::new(total as u64);
    pb.set_style(bar_style(colors, concurrent));

    let skipped = total - items.len();
    pb.inc(skipped as u64);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrent.max(1)));
    let errors = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let work = Arc::new(work);
    let mut handles = Vec::with_capacity(items.len());

    for item in items {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let pb = pb.clone();
        let errs = errors.clone();
        let work = work.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let result = work(item, pb.clone()).await;
            if let Err(e) = &result {
                if let Ok(mut v) = errs.lock() {
                    v.push(e.clone());
                }
            }
            pb.inc(1);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    pb.finish_and_clear();
    let errs = errors.lock().map(|v| v.clone()).unwrap_or_default();
    let fail = errs.len();
    let ok = total - skipped - fail;
    (ok, fail, errs)
}

/// 失败分组收据：同类错误最多展示 3 条 + 计数，其余收敛（TUI.md §6 错误分组）。
pub fn print_failures(errs: &[String], retry_hint: &str) {
    if errs.is_empty() { return; }
    let show: Vec<&str> = errs.iter().take(3).map(|s| s.as_str()).collect();
    let more = if errs.len() > 3 { " …" } else { "" };
    eprintln!("  err 失败 {} 项 · {}{}{}", errs.len(), show.join(" · "), more, retry_hint);
}

/// 按 `.a.b[0].c` 形式提取 JSON 字段（jq 子集，供 `fqdt api --jq` 与封装子命令共用）
///
/// 段语法：`键名`、`键名[n]`（取下标）、`键名[]`（数组展开，多结果合并为数组）。
/// 前导点可省略。
///
/// # 返回
/// - 命中返回对应值（展开产生多个结果时为数组），任一段无匹配返回 `None`
pub fn jq_extract(v: &serde_json::Value, filter: &str) -> Option<serde_json::Value> {
    let mut cur = vec![v.clone()];
    for seg in filter.split('.').filter(|s| !s.trim().is_empty()) {
        let mut next = Vec::new();
        for item in &cur {
            jq_step(item, seg.trim(), &mut next)?;
        }
        if next.is_empty() {
            return None;
        }
        cur = next;
    }
    if cur.len() == 1 {
        cur.pop()
    } else {
        Some(serde_json::Value::Array(cur))
    }
}

/// 单段前进：处理 `键名` / `键名[n]` / `键名[]`，结果追加到 `out`
fn jq_step(item: &serde_json::Value, seg: &str, out: &mut Vec<serde_json::Value>) -> Option<()> {
    if let Some(key) = seg.strip_suffix("[]") {
        let base = if key.is_empty() { Some(item) } else { item.get(key) };
        for x in base?.as_array()? {
            out.push(x.clone());
        }
    } else if let Some(i) = seg.find('[') {
        let key = &seg[..i];
        let idx = seg[i..]
            .trim_start_matches('[')
            .split(']')
            .next()?
            .parse::<usize>()
            .ok()?;
        let base = if key.is_empty() { item } else { item.get(key)? };
        out.push(base.get(idx)?.clone());
    } else {
        out.push(item.get(seg)?.clone());
    }
    Some(())
}

/// jq 提取结果的展示形式：字符串去引号直出，Null 空行，其余紧凑 JSON
pub fn jq_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::jq_extract;
    use serde_json::json;

    #[test]
    fn test_jq_simple_key() {
        let v = json!({"data": {"book_name": "十日终焉"}});
        assert_eq!(jq_extract(&v, "data.book_name"), Some(json!("十日终焉")));
    }

    #[test]
    fn test_jq_index() {
        let v = json!({"list": [{"t": "甲"}, {"t": "乙"}]});
        assert_eq!(jq_extract(&v, "list[1].t"), Some(json!("乙")));
    }

    #[test]
    fn test_jq_spread() {
        let v = json!({"cells": [{"books": [{"n": 1}, {"n": 2}]}, {"books": [{"n": 3}]}]});
        assert_eq!(
            jq_extract(&v, "cells[].books[].n"),
            Some(json!([1, 2, 3]))
        );
    }

    #[test]
    fn test_jq_miss() {
        let v = json!({"a": 1});
        assert_eq!(jq_extract(&v, "a.b"), None);
    }
}
