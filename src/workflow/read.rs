//! # read — 终端 TUI 连贯阅读
//!
//! UI 布局抄 `bugzmanov/bookokrat`（1292★，ratatui）：普通模式 = 左目录面板 +
//! 右正文 + 底部 3 行帮助栏，焦点面板边框高亮；`z` 进 zen 模式（全屏正文，
//! 藏目录与帮助栏）。进度模型（章 item_id + 行偏移）抄 `rereader`；内容源接
//! fqdt `fetch_content`。
//!
//! 键位（正文焦点）：`j/k` 行滚 · `d/u` 半页 · `g/G` 章首尾 · `n/p` 切章 ·
//! `Tab` 进目录 · `z` zen · `q` 退出（存进度）。目录焦点：`j/k` 选择 ·
//! `Enter` 跳转 · `Tab/Esc` 回正文。滚到底再按 `j` 自动进下一章。

use crate::api::{self, Client};
use crate::store::{self, ReadPos};
use crate::types::{Chapter, ChapterRange, Config};
use crate::util::{self, CYAN, DIM, MAGENTA, YELLOW};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{IsTerminal, Stdout};

const HELP: &str = "j/k 滚动 · d/u 半页 · n/p 章节 · Tab 目录 · z zen · q 退出";
const HELP_NAV: &str = "j/k 选择 · Enter 跳转 · Tab/Esc 回正文 · q 退出";
/// 左目录面板宽度（bookokrat nav_width 同思路，定宽）
const NAV_W: u16 = 28;

/// 焦点面板（bookokrat FocusedPanel/MainPanel 简化版）。
#[derive(PartialEq, Clone, Copy)]
enum Focus {
    Content,
    Nav,
}

/// TUI 状态。
struct Reader {
    title: String,
    chs: Vec<Chapter>,
    cur: usize,
    /// 当前章原文（宽度变化时 rewrap，免重新请求）
    raw: String,
    lines: Vec<Line<'static>>,
    scroll: u16,
    focus: Focus,
    /// 目录面板选中项
    sel: usize,
    zen: bool,
    need_load: bool,
    msg: String,
}

/// 按键动作（由 on_key 判定，循环里执行）。
enum Action {
    Quit,
    PrevCh,
    NextCh,
    Scroll(i32),
    ToggleFocus,
    ToggleZen,
    /// 目录：跳到选中章
    GotoSel,
    /// 目录：焦点回到正文
    BackContent,
    /// 聚焦目录并定位当前章
    FocusToc,
    /// 目录选择移动
    SelMove(i32),
}

impl Reader {
    fn new(title: String, chs: Vec<Chapter>) -> Self {
        Reader {
            title,
            chs,
            cur: 0,
            raw: String::new(),
            lines: Vec::new(),
            scroll: 0,
            focus: Focus::Content,
            sel: 0,
            zen: false,
            need_load: false,
            msg: String::new(),
        }
    }

    /// 正文可用宽度：zen 全屏，普通模式扣掉目录面板与两侧边框。
    fn content_width(&self, total: u16) -> u16 {
        if self.zen {
            total.saturating_sub(2)
        } else {
            total.saturating_sub(NAV_W + 3)
        }
    }
}

/// `fqdt read` 入口：TTY 检查 → 目录/书名 → 恢复进度 → 进入 TUI 循环。
pub async fn run(book_id: &str, range: Option<&str>, cfg: &Config) {
    if !std::io::stdout().is_terminal() {
        eprintln!("  err read 需要交互式终端；可先 download 下载后用其他工具阅读");
        return;
    }
    let api = Client::from_config(cfg, false);
    util::progress("获取目录");
    let all = match api.fetch_catalog(book_id).await {
        Ok(c) => c,
        Err(e) => {
            util::clear_progress();
            eprintln!("  err {}", e);
            return;
        }
    };
    util::clear_progress();
    let r = range.and_then(ChapterRange::parse);
    let chs: Vec<Chapter> = util::filter_by_range(&all, r.as_ref()).into_iter().cloned().collect();
    if chs.is_empty() {
        eprintln!("  err 空范围");
        return;
    }
    let title = match api.fetch_detail(book_id).await {
        Ok(d) => d.split('|').next().unwrap_or("小说").to_string(),
        Err(_) => book_id.to_string(),
    };
    let mut rd = Reader::new(title, chs);
    if let Some(p) = store::read_get(book_id) {
        if let Some(i) = rd.chs.iter().position(|c| c.item_id == p.item_id) {
            rd.cur = i;
            rd.scroll = p.scroll.min(u16::MAX as usize) as u16;
        }
    }
    rd.sel = rd.cur;
    rd.need_load = true;
    if let Err(e) = event_loop(&mut rd, &api).await {
        eprintln!("  err {}", e);
        return;
    }
    let pos = ReadPos {
        item_id: rd.chs.get(rd.cur).map(|c| c.item_id.clone()).unwrap_or_default(),
        scroll: rd.scroll as usize,
    };
    let _ = store::read_save(book_id, pos);
}

/// 终端 setup → 事件循环 → teardown（进度由调用方保存）。
async fn event_loop(rd: &mut Reader, api: &Client) -> Result<(), String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut out = std::io::stdout();
    let _ = execute!(out, EnterAlternateScreen);
    let res = loop_with(rd, api, &mut out).await;
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    res
}

/// 同步事件循环：draw → 取事件 → 处理；切章时 block_in_place 加载正文。
async fn loop_with(rd: &mut Reader, api: &Client, out: &mut Stdout) -> Result<(), String> {
    let mut term = Terminal::new(CrosstermBackend::new(&mut *out)).map_err(|e| e.to_string())?;
    let mut width: u16 = term.size().map(|s| s.width).unwrap_or(80);
    let mut height: u16 = term.size().map(|s| s.height).unwrap_or(24);
    loop {
        if rd.need_load {
            reload(rd, api, width).await?;
            rd.need_load = false;
        }
        term.draw(|f| draw(f, rd)).map_err(|e| e.to_string())?;
        let ev = event::read().map_err(|e| e.to_string())?;
        match ev {
            Event::Resize(w, h) => {
                width = w;
                height = h;
                rewrap(rd, rd.content_width(w));
            }
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                let act = on_key(k.code, rd, height);
                if apply_action(act, rd, height) {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
}

/// 执行按键动作；返回 true 表示退出。
fn apply_action(act: Action, rd: &mut Reader, height: u16) -> bool {
    match act {
        Action::Quit => return true,
        Action::PrevCh if rd.cur > 0 => {
            rd.cur -= 1;
            rd.scroll = 0;
            rd.need_load = true;
        }
        Action::NextCh if rd.cur + 1 < rd.chs.len() => {
            rd.cur += 1;
            rd.scroll = 0;
            rd.need_load = true;
        }
        Action::Scroll(d) => scroll_by(rd, d, height),
        Action::ToggleFocus => {
            rd.focus = match rd.focus {
                Focus::Content => Focus::Nav,
                Focus::Nav => Focus::Content,
            };
            if rd.focus == Focus::Nav {
                rd.sel = rd.cur;
            }
        }
        Action::ToggleZen => {
            rd.zen = !rd.zen;
            rd.focus = Focus::Content;
            rewrap(rd, rd.content_width(terminal_width()));
        }
        Action::GotoSel => {
            if rd.sel < rd.chs.len() && rd.sel != rd.cur {
                rd.cur = rd.sel;
                rd.scroll = 0;
                rd.need_load = true;
            }
            rd.focus = Focus::Content;
        }
        Action::BackContent => rd.focus = Focus::Content,
        Action::FocusToc => {
            rd.sel = rd.cur;
            rd.focus = Focus::Nav;
        }
        Action::SelMove(d) => {
            let n = rd.chs.len() as i64;
            rd.sel = ((rd.sel as i64 + d as i64).clamp(0, n - 1)) as usize;
        }
        // 带 guard 的臂不满足时落到此（首章 p / 末章 n）
        Action::PrevCh | Action::NextCh => {}
    }
    false
}

/// 当前终端宽度（zen 切换 rewrap 用；失败回退 80）。
fn terminal_width() -> u16 {
    crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80)
}

/// 加载当前章：取正文 → 剥 HTML → 按宽度折行（同步上下文里 block_on）。
async fn reload(rd: &mut Reader, api: &Client, width: u16) -> Result<(), String> {
    let item = rd.chs[rd.cur].item_id.clone();
    let res = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(api.fetch_content(&item))
    });
    match res {
        Ok(html) => {
            rd.raw = api::strip_html_imgs(&html);
            rd.lines = wrap_with_imgs(&rd.raw, rd.content_width(width));
            rd.msg.clear();
            Ok(())
        }
        Err(e) => {
            rd.raw.clear();
            rd.lines = Vec::new();
            rd.msg = format!("加载失败: {}", e);
            Ok(())
        }
    }
}

/// 正文按显示宽度折行：逐段（\n 分隔）逐字符累加，全角按 2 列计。
fn wrap_text(text: &str, width: u16) -> Vec<Line<'static>> {
    let w = width.max(20) as usize;
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.trim().is_empty() {
            out.push(Line::default());
            continue;
        }
        let mut cur = String::new();
        let mut cw = 0usize;
        for ch in para.chars() {
            let cwid = Span::raw(ch.to_string()).width();
            if cw + cwid > w && !cur.is_empty() {
                out.push(Line::from(std::mem::take(&mut cur)));
                cw = 0;
            }
            cur.push(ch);
            cw += cwid;
        }
        if !cur.is_empty() {
            out.push(Line::from(cur));
        }
        out.push(Line::default());
    }
    out
}

/// 宽度/zen 变化后按原文重折（不重新请求）。
fn rewrap(rd: &mut Reader, width: u16) {
    rd.lines = wrap_with_imgs(&rd.raw, width);
}

/// 含图片标记的折行：`{{IMG:url}}` 独占行展开为图片块/占位，其余行走 wrap_text。
fn wrap_with_imgs(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Line<'static>>| {
        if !buf.is_empty() {
            out.extend(wrap_text(buf, width));
            buf.clear();
        }
    };
    for line in text.split('\n') {
        if let Some(url) = line.strip_prefix("{{IMG:").and_then(|s| s.strip_suffix("}}")) {
            flush(&mut buf, &mut out);
            push_img_block(&mut out, url, width);
        } else {
            if !buf.is_empty() { buf.push('\n'); }
            buf.push_str(line);
        }
    }
    flush(&mut buf, &mut out);
    out
}

/// 图片块：有外部加载器→下载转 ANSI 行；否则/失败→纯文本占位行。
fn push_img_block(out: &mut Vec<Line<'static>>, url: &str, width: u16) {
    let h = crate::imgtool::IMG_MAX_H;
    match crate::imgtool::render(url, width, h) {
        Some(rows) => {
            out.push(Line::default());
            for row in rows { out.push(Line::from(row)); }
            out.push(Line::default());
        }
        None => {
            out.push(Line::from(crate::imgtool::placeholder(url)));
        }
    }
}

/// 滚动：上/下界钳制；滚到底再向下 → 自动进下一章。
fn scroll_by(rd: &mut Reader, delta: i32, height: u16) {
    let view = height.saturating_sub(2).max(1);
    let max = rd.lines.len().saturating_sub(view as usize) as i64;
    let next = (rd.scroll as i64 + delta as i64).clamp(0, max);
    // 仅逐行/半页级滚动到底（含已停在底部）自动进下一章；G 等大跳停在章尾
    let small = delta > 0 && delta as i64 <= view as i64;
    if small && next == max && rd.cur + 1 < rd.chs.len() {
        rd.cur += 1;
        rd.scroll = 0;
        rd.need_load = true;
        return;
    }
    rd.scroll = next as u16;
}

/// 按键 → 动作（按焦点分发；键位习惯同 baca/epubreader-cli：j/k、n/p、g/G、Tab）。
fn on_key(code: KeyCode, rd: &Reader, height: u16) -> Action {
    // 全局键（两种焦点都响应）
    match code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('z') => return Action::ToggleZen,
        KeyCode::Tab => return Action::ToggleFocus,
        _ => {}
    }
    match rd.focus {
        Focus::Content => {
            let half = (height.saturating_sub(4) / 2).max(1) as i32;
            match code {
                KeyCode::Esc => Action::FocusToc,
                KeyCode::Char('j') | KeyCode::Down => Action::Scroll(1),
                KeyCode::Char('k') | KeyCode::Up => Action::Scroll(-1),
                KeyCode::Char('d') | KeyCode::PageDown => Action::Scroll(half),
                KeyCode::Char('u') | KeyCode::PageUp => Action::Scroll(-half),
                KeyCode::Char('g') | KeyCode::Home => Action::Scroll(-(i32::MAX / 2)),
                KeyCode::Char('G') | KeyCode::End => Action::Scroll(i32::MAX / 2),
                KeyCode::Char('n') | KeyCode::Right | KeyCode::Char(' ') => Action::NextCh,
                KeyCode::Char('p') | KeyCode::Left => Action::PrevCh,
                KeyCode::Char('t') => Action::FocusToc,
                _ => Action::Scroll(0),
            }
        }
        Focus::Nav => match code {
            KeyCode::Char('j') | KeyCode::Down => Action::SelMove(1),
            KeyCode::Char('k') | KeyCode::Up => Action::SelMove(-1),
            KeyCode::Char('g') => Action::SelMove(-(i32::MAX / 2)),
            KeyCode::Char('G') => Action::SelMove(i32::MAX / 2),
            KeyCode::Enter => Action::GotoSel,
            KeyCode::Esc => Action::BackContent,
            KeyCode::Char('n') | KeyCode::Right => Action::NextCh,
            KeyCode::Char('p') | KeyCode::Left => Action::PrevCh,
            _ => Action::SelMove(0),
        },
    }
}

/// 渲染主入口：zen 全屏 / 普通分栏（bookokrat draw 同构）。
fn draw(f: &mut Frame, rd: &Reader) {
    if rd.zen {
        draw_zen(f, rd);
    } else {
        draw_normal(f, rd);
    }
}

/// 普通模式：上=分栏（左目录+右正文），下=3 行帮助栏。
fn draw_normal(f: &mut Frame, rd: &Reader) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(f.area());
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(NAV_W), Constraint::Min(0)])
        .split(chunks[0]);
    draw_toc(f, rd, main[0]);
    draw_content(f, rd, main[1]);
    draw_helpbar(f, rd, chunks[1]);
}

/// zen 模式：全屏正文（藏目录与帮助栏），顶行保留极简进度。
fn draw_zen(f: &mut Frame, rd: &Reader) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(f.area());
    f.render_widget(Paragraph::new(top_line(rd)), chunks[0]);
    f.render_widget(
        Paragraph::new(rd.lines.clone()).scroll((rd.scroll, 0)),
        chunks[1],
    );
}

/// 左目录面板：边框按焦点高亮，当前章高亮，选中反白。
fn draw_toc(f: &mut Frame, rd: &Reader, area: Rect) {
    let focused = rd.focus == Focus::Nav;
    let border = if focused { util::fg(CYAN) } else { util::fg(DIM) };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(" 目录 ", if focused { util::fg(CYAN) } else { util::fg(DIM) }));
    let inner_h = area.height.saturating_sub(2) as usize;
    // 视窗：让选中项可见
    let mut start = 0usize;
    if rd.sel >= inner_h && inner_h > 0 {
        start = rd.sel + 1 - inner_h;
    }
    let lines: Vec<Line> = rd
        .chs
        .iter()
        .enumerate()
        .skip(start)
        .take(inner_h.max(1))
        .map(|(i, c)| {
            let mark = if i == rd.cur { "▶ " } else { "  " };
            let title = format!("{}{}", mark, c.title);
            if i == rd.sel {
                Line::from(Span::styled(
                    title,
                    util::fg(MAGENTA).add_modifier(ratatui::style::Modifier::REVERSED),
                ))
            } else if i == rd.cur {
                Line::from(Span::styled(title, util::fg(CYAN)))
            } else {
                Line::from(Span::styled(title, util::fg(DIM)))
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// 右正文面板：边框按焦点高亮，顶行书名·章节·进度。
fn draw_content(f: &mut Frame, rd: &Reader, area: Rect) {
    let focused = rd.focus == Focus::Content;
    let border = if focused { util::fg(CYAN) } else { util::fg(DIM) };
    let block = Block::default().borders(Borders::ALL).border_style(border);
    let inner = {
        let mut r = area;
        r.x += 1;
        r.y += 1;
        r.width = r.width.saturating_sub(2);
        r.height = r.height.saturating_sub(2);
        r
    };
    // 顶行进度 + 正文
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    f.render_widget(Paragraph::new(top_line(rd)), body[0]);
    f.render_widget(
        Paragraph::new(rd.lines.clone()).scroll((rd.scroll, 0)),
        body[1],
    );
    f.render_widget(block, area);
}

/// 顶行：书名 · 章节 · 行进度（zen 与正文共用）。
fn top_line(rd: &Reader) -> Line<'static> {
    let ch = &rd.chs[rd.cur];
    let total = rd.lines.len();
    let pos = (rd.scroll as usize + 1).min(total.max(1));
    Line::from(vec![
        Span::styled(rd.title.clone(), util::fg(MAGENTA)),
        Span::styled(" · ", util::fg(DIM)),
        Span::styled(ch.title.clone(), util::fg(CYAN)),
        Span::styled(
            format!("  {}/{} 章 · 行 {}/{}", rd.cur + 1, rd.chs.len(), pos, total),
            util::fg(YELLOW),
        ),
    ])
}

/// 底部 3 行帮助栏（bookokrat render_help_bar 同构：通知行 + 键位行 + 边框）。
fn draw_helpbar(f: &mut Frame, rd: &Reader, area: Rect) {
    let block = Block::default().borders(Borders::TOP).border_style(util::fg(DIM));
    let lines = vec![
        Line::from(Span::styled(
            match rd.focus {
                Focus::Content => HELP,
                Focus::Nav => HELP_NAV,
            },
            util::fg(DIM),
        )),
        Line::from(if rd.msg.is_empty() {
            Span::styled(
                format!(
                    "{} 章 / 共 {} · {}",
                    rd.cur + 1,
                    rd.chs.len(),
                    if rd.zen { "zen" } else { "normal" }
                ),
                util::fg(DIM),
            )
        } else {
            Span::styled(rd.msg.clone(), util::fg(util::RED))
        }),
    ];
    f.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn wrap_with_imgs_marker_and_text() {
        // 无工具环境（CI）：图片行→占位；普通行照常折行
        let raw = "前文\n{{IMG:/rel/x.png}}\n后文";
        let lines = wrap_with_imgs(raw, 40);
        let joined: String = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("[图片: /rel/x.png]"), "got: {joined:?}");
        assert!(joined.contains("前文") && joined.contains("后文"), "got: {joined:?}");
        // 无标记时与 wrap_text 行数一致
        let a = wrap_with_imgs("甲乙丙\n丁戊", 40);
        let b = wrap_text("甲乙丙\n丁戊", 40);
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn wrap_chinese_by_width() {
        // 宽度下限 20：宽 10 被抬到 20 列 → 10 个全角字恰好一行
        let lines = wrap_text("一二三四五六七八九十", 10);
        assert_eq!(lines[0].to_string(), "一二三四五六七八九十");
        // 真实宽 24 → 每行 12 全角字；30 字 = 12+12+6 + 段尾空行
        let lines = wrap_text(&"一".repeat(30), 24);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].to_string(), "一".repeat(12));
        assert_eq!(lines[2].to_string(), "一".repeat(6));
        assert_eq!(lines[3].to_string(), "");
    }

    #[test]
    fn key_bindings_content() {
        let rd = Reader::new("t".into(), vec![]);
        let h = 24;
        assert!(matches!(on_key(KeyCode::Char('q'), &rd, h), Action::Quit));
        assert!(matches!(on_key(KeyCode::Char('j'), &rd, h), Action::Scroll(1)));
        assert!(matches!(on_key(KeyCode::Char('k'), &rd, h), Action::Scroll(-1)));
        assert!(matches!(on_key(KeyCode::Char('n'), &rd, h), Action::NextCh));
        assert!(matches!(on_key(KeyCode::Char('p'), &rd, h), Action::PrevCh));
        assert!(matches!(on_key(KeyCode::Char('z'), &rd, h), Action::ToggleZen));
        assert!(matches!(on_key(KeyCode::Tab, &rd, h), Action::ToggleFocus));
        assert!(matches!(on_key(KeyCode::Esc, &rd, h), Action::FocusToc));
    }

    #[test]
    fn key_bindings_nav() {
        let mut rd = Reader::new("t".into(), vec![
            Chapter { index: 1, item_id: "a".into(), title: "一".into() },
            Chapter { index: 2, item_id: "b".into(), title: "二".into() },
        ]);
        rd.focus = Focus::Nav;
        let h = 24;
        assert!(matches!(on_key(KeyCode::Char('j'), &rd, h), Action::SelMove(1)));
        assert!(matches!(on_key(KeyCode::Enter, &rd, h), Action::GotoSel));
        assert!(matches!(on_key(KeyCode::Esc, &rd, h), Action::BackContent));
        // 全局键在 Nav 下同样生效
        assert!(matches!(on_key(KeyCode::Char('q'), &rd, h), Action::Quit));
        assert!(matches!(on_key(KeyCode::Char('z'), &rd, h), Action::ToggleZen));
    }

    #[test]
    fn scroll_clamps_and_auto_next_chapter() {
        let mut rd = Reader::new("t".into(), vec![
            Chapter { index: 1, item_id: "a".into(), title: "一".into() },
            Chapter { index: 2, item_id: "b".into(), title: "二".into() },
        ]);
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        scroll_by(&mut rd, -5, 10);
        assert_eq!(rd.scroll, 0, "上界钳制");
        scroll_by(&mut rd, i32::MAX / 2, 10);
        assert_eq!(rd.scroll, 92, "下界钳制");
        scroll_by(&mut rd, 1, 10);
        assert_eq!(rd.cur, 1, "滚到底自动下一章");
        assert_eq!(rd.scroll, 0);
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        scroll_by(&mut rd, i32::MAX / 2, 10);
        scroll_by(&mut rd, 1, 10);
        assert_eq!(rd.cur, 1, "末章不越界");
    }

    #[test]
    fn focus_toggle_and_zen() {
        let mut rd = Reader::new("t".into(), vec![
            Chapter { index: 1, item_id: "a".into(), title: "一".into() },
            Chapter { index: 2, item_id: "b".into(), title: "二".into() },
        ]);
        rd.cur = 1;
        assert!(!apply_action(Action::ToggleFocus, &mut rd, 24));
        assert!(rd.focus == Focus::Nav);
        assert_eq!(rd.sel, 1, "进目录定位当前章");
        assert!(!apply_action(Action::ToggleFocus, &mut rd, 24));
        assert!(rd.focus == Focus::Content);
        assert!(!apply_action(Action::ToggleZen, &mut rd, 24));
        assert!(rd.zen);
        // 目录跳转：sel=0 → cur=0
        rd.focus = Focus::Nav;
        rd.sel = 0;
        apply_action(Action::GotoSel, &mut rd, 24);
        assert_eq!(rd.cur, 0);
        assert!(rd.need_load, "跳章需重载");
        assert!(rd.focus == Focus::Content, "跳转后回正文");
        // Quit 返回 true
        assert!(apply_action(Action::Quit, &mut rd, 24));
    }

    #[test]
    fn restore_position_by_item_id() {
        let chs = [Chapter { index: 1, item_id: "x".into(), title: "一".into() }];
        assert!(chs.iter().position(|c| c.item_id == "missing").is_none());
        assert_eq!(chs.iter().position(|c| c.item_id == "x"), Some(0));
    }
}

