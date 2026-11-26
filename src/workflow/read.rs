//! # read — 终端 TUI 连贯阅读
//!
//! UI 布局抄 `bugzmanov/bookokrat`（1292★，ratatui）：普通模式 = 左目录面板 +
//! 右正文 + 底部 3 行帮助栏，焦点面板边框高亮；`z` 进 zen 模式（全屏正文，
//! 藏目录与帮助栏）。进度模型（章 item_id + 行偏移）抄 `rereader`；内容源接
//! fqdt `fetch_content`。
//!
//! 键位（91 大重设计，m0130）：正文焦点 `j/k/↑↓` 段落光标 · `,`.` 翻 2/3、1/2 页 ·
//! `<>`/`n/p` 切章 · `space` 查看光标处图片 · `d/u/g/G` 行滚 · `Tab` 目录 ·
//! `z` zen · `?` 帮助 · `q` 退出。目录焦点：`j/k` 选择 · `Enter` 跳转 ·
//! `Tab/Esc` 回正文。翻页方式由 `ui.page_mode` 决定（左右/上下渲染，90）。

use crate::api::{self, Client};
use crate::store::{self, ReadPos};
use crate::types::{Chapter, ChapterRange, Config};
use crate::util::{self, DIM};
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

#[allow(dead_code)] // 批C③ ?/h 帮助框将使用
const HELP: &str =
    "j/k 段光标 · ,. 翻页 · <> 章节 · space 看图 · Tab 目录 · z zen · ? 帮助 · q 退出";
#[allow(dead_code)] // 批C③ 将使用
const HELP_NAV: &str = "j/k 选择 · Enter 跳转 · Tab/Esc 回正文 · q 退出";
/// 左目录面板宽度（bookokrat nav_width 同思路，定宽）
const NAV_W: u16 = 28;

/// 翻页渲染模式（90/91）：左右=平行翻页光标保持相对段；上下=1/4 屏平移。
#[derive(PartialEq, Clone, Copy, Debug)]
enum PageMode {
    RightLeft,
    UpDown,
}

/// 段落种类（91）：文本段可高亮，图片段不高亮（space 查看）。
#[derive(PartialEq, Clone, Debug)]
enum ParaKind {
    Text,
    Image(String),
}

/// 段落：行区间 [start, end)，供段落光标导航（91）。
#[derive(PartialEq, Clone, Debug)]
struct Para {
    start: usize,
    end: usize,
    kind: ParaKind,
}

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
    /// 图片聚焦态（77）：放大查看的 url，None=正文
    img_focus: Option<String>,
    /// 图块行区间 (start, end, url)（wrap 时记录，供 i 键聚焦）
    imgs: Vec<(usize, usize, String)>,
    /// 段落列表（91，rebuild_paras 重建）
    paras: Vec<Para>,
    /// 段落光标（paras 索引，91）
    cursor: usize,
    /// ?/h 帮助框（批C③）
    help_shown: bool,
    /// TUI 主题（84）：预设+覆盖已解析
    theme: crate::theme::ThemeColors,
    /// 帮助框滚动偏移（103F）
    help_scroll: u16,
    /// 正文/zen/help 内边距空格数（103 ui.gutter）
    gutter: u16,
    /// 滚动边缘区百分比（103 ui.scroll_edge）
    scroll_edge: u32,
    /// 页尾空行数（103 ui.tail_blank）
    tail_blank: u16,
    /// 过渡动画帧数（103 ui.anim_frames，0=关）
    anim_frames: u16,
    /// 跨上章后落章尾（103⑥，reload 完成后消费）
    pending_end: bool,
    /// 翻页模式（90/91，ui.page_mode）
    page_mode: PageMode,
    /// 外部编辑器命令（87：e 挂起 TUI 只读打开当前章，run 启动时解析）
    editor: Option<String>,
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
    /// 段落光标移动（91）：±1 段，端点钳制；图片段可停留
    CursorMove(i32),
    /// space 查看光标处图片（91）：光标段为图 → 聚焦，否则无操作
    SpaceImg,
    /// 打开 ?/h 帮助框（批C③）
    Help,
    /// 关闭帮助框（批C③）
    CloseHelp,
    /// 帮助框滚动（103F 超屏）：±行
    HelpScroll(i16),
    /// 外部编辑器只读打开当前章（87）
    OpenEditor,
    /// 整页切换（91）：±1 页，光标段行相对偏移保持（左右平行/上下整屏）
    PageFlip(i32),
    /// 目录：焦点回到正文
    BackContent,
    /// 放大查看图片（77）
    FocusImg(String),
    /// 关闭图片聚焦
    CloseImg,
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
            img_focus: None,
            imgs: Vec::new(),
            paras: Vec::new(),
            cursor: 0,
            page_mode: PageMode::RightLeft,
            editor: None,
            help_shown: false,
            theme: crate::theme::resolve("default", &crate::theme::ThemeOverride::default()),
            help_scroll: 0,
            gutter: 1,
            scroll_edge: 25,
            tail_blank: 1,
            anim_frames: 3,
            pending_end: false,
        }
    }

    /// 正文可用宽度：zen 全屏，普通模式扣掉目录面板与两侧边框。
    /// 正文视窗高（103④⑦）：与渲染区逐像素一致——翻页/滚动/光标跟随共用，
    /// 杜绝 view 散算（-2/-3/-4）与渲染差造成的页边界跳行。
    fn view_h(&self, screen: u16) -> u16 {
        if self.zen {
            // 框 2 + 上 1 + 下 1
            screen.saturating_sub(4).max(1)
        } else {
            // 框 2 + 状态 1 + 空行 1 + 页尾 tail_blank
            screen.saturating_sub(4 + self.tail_blank).max(1)
        }
    }

    fn content_width(&self, total: u16) -> u16 {
        // 对称边距（103⑩⑬）：框线 2 + 左右各 gutter；normal 另扣目录栏 NAV_W（含其框线）
        let pad = 2 + self.gutter * 2;
        if self.zen {
            total.saturating_sub(pad)
        } else {
            total.saturating_sub(NAV_W + pad)
        }
    }
}

/// `fqdt read` 入口：TTY 检查 → 目录/书名 → 恢复进度 → 进入 TUI 循环。
/// 启动模式应用（89）：ui.page_mode 翻页 + ui.view_mode 默认视图（zen 启动）。
fn apply_startup_modes(rd: &mut Reader, cfg: &Config) {
    rd.page_mode = if cfg.ui_page_mode.eq_ignore_ascii_case("up-down")
        || cfg.ui_page_mode.eq_ignore_ascii_case("ud")
    {
        PageMode::UpDown
    } else {
        PageMode::RightLeft
    };
    rd.zen = cfg.ui_view_mode.eq_ignore_ascii_case("zen");
}

/// view 标题选择（103⑰）：详情首段可用且非纯数字 → 用详情；否则关键词 hint；再否则 book_id。
/// 搜索进 view 时详情接口偶发失败，hint 来自 search::pick 命中书名，避免框标题退化成数字串。
fn resolve_title(detail: Option<&str>, hint: Option<&str>, id: &str) -> String {
    let d = detail
        .map(|s| s.split('|').next().unwrap_or("").trim())
        .unwrap_or("");
    let usable = !d.is_empty() && !d.chars().all(|c| c.is_ascii_digit());
    if usable {
        return d.to_string();
    }
    let h = hint.map(str::trim).unwrap_or("");
    if !h.is_empty() {
        return h.to_string();
    }
    id.to_string()
}

pub async fn run(book_id: &str, range: Option<&str>, cfg: &Config, title_hint: Option<&str>) {
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
    let chs: Vec<Chapter> = util::filter_by_range(&all, r.as_ref())
        .into_iter()
        .cloned()
        .collect();
    if chs.is_empty() {
        eprintln!("  err 空范围");
        return;
    }
    let title = resolve_title(
        api.fetch_detail(book_id).await.ok().as_deref(),
        title_hint,
        book_id,
    );
    let mut rd = Reader::new(title, chs);
    apply_startup_modes(&mut rd, cfg);
    rd.theme = crate::theme::resolve(&cfg.ui_theme, &cfg.theme_override());
    rd.gutter = cfg.ui_gutter.min(4) as u16;
    rd.scroll_edge = cfg.ui_scroll_edge.clamp(5, 45);
    rd.tail_blank = cfg.ui_tail_blank.min(4) as u16;
    rd.anim_frames = cfg.ui_anim_frames.min(12) as u16;
    rd.editor = resolve_editor(cfg);
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
        item_id: rd
            .chs
            .get(rd.cur)
            .map(|c| c.item_id.clone())
            .unwrap_or_default(),
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

/// 过渡动画判定（103⑮ 纯函数）：|new-old| ≤ 2 屏才滑动（章跳/大跳瞬切，防千行插值乱跳）。
fn scroll_animateable(old: u16, new: u16, view: usize) -> bool {
    let v = (view as i64).max(1) * 2;
    ((new as i64) - (old as i64)).abs() <= v
}

/// 过渡插值（103⑮ 纯函数）：第 k/frames 帧的窗口起点（帧 0=old，frames=new）。
fn scroll_lerp(old: u16, new: u16, k: u32, frames: u32) -> u16 {
    let (o, n) = (old as i64, new as i64);
    let v = o + (n - o) * k as i64 / frames.max(1) as i64;
    v.clamp(0, u16::MAX as i64) as u16
}

/// 翻页/滚动过渡帧循环（103⑮，克制）：anim_frames 帧 × 16ms，末帧落 new。
/// 超过 2 屏的大跳（切章、G 等）直接单帧；anim_frames ≤1 由调用方跳过。
fn animate_scroll(
    term: &mut Terminal<CrosstermBackend<&mut Stdout>>,
    rd: &mut Reader,
    old: u16,
    new: u16,
    height: u16,
) -> Result<(), String> {
    let view = rd.view_h(height) as usize;
    if !scroll_animateable(old, new, view) {
        term.draw(|f| draw(f, rd)).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let frames = rd.anim_frames.max(2);
    for k in 1..frames {
        rd.scroll = scroll_lerp(old, new, k as u32, frames as u32);
        term.draw(|f| draw(f, rd)).map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    rd.scroll = new;
    term.draw(|f| draw(f, rd)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 同步事件循环：draw → 取事件 → 处理；切章时 block_in_place 加载正文。
async fn loop_with(rd: &mut Reader, api: &Client, out: &mut Stdout) -> Result<(), String> {
    let mut term = Terminal::new(CrosstermBackend::new(&mut *out)).map_err(|e| e.to_string())?;
    let mut width: u16 = term.size().map(|s| s.width).unwrap_or(80);
    let mut height: u16 = term.size().map(|s| s.height).unwrap_or(24);
    let mut shown: u16 = rd.scroll; // 上次绘制的窗口起点（103⑮ 过渡动画基准）
    loop {
        if rd.need_load {
            reload(rd, api, width).await?;
            rd.need_load = false;
            settle_pending(rd, height);
        }
        // 103⑮ 翻页/滚动过渡：scroll 变化 → 小步逐帧滑动（大跳瞬切）
        if rd.scroll != shown && rd.anim_frames > 1 {
            animate_scroll(&mut term, rd, shown, rd.scroll, height)?;
        } else {
            term.draw(|f| draw(f, rd)).map_err(|e| e.to_string())?;
        }
        shown = rd.scroll;
        let ev = event::read().map_err(|e| e.to_string())?;
        match ev {
            Event::Resize(w, h) => {
                width = w;
                height = h;
                rewrap(rd, rd.content_width(w));
            }
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                let act = on_key(k.code, rd, height);
                if matches!(act, Action::OpenEditor) {
                    open_editor(rd, &mut term)?;
                    continue;
                }
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
        Action::FocusImg(url) => {
            rd.img_focus = Some(url);
        }
        Action::CloseImg => {
            rd.img_focus = None;
        }
        Action::PrevCh if rd.cur > 0 => {
            rd.cur -= 1;
            rd.scroll = 0;
            rd.cursor = 0;
            rd.paras.clear();
            rd.need_load = true;
        }
        Action::NextCh if rd.cur + 1 < rd.chs.len() => {
            rd.cur += 1;
            rd.scroll = 0;
            rd.cursor = 0;
            rd.paras.clear();
            rd.need_load = true;
        }
        Action::Scroll(d) => {
            scroll_by(rd, d, height);
            sync_cursor_from_view(rd, height);
        }
        Action::CursorMove(d) => {
            if !rd.paras.is_empty() {
                let n = rd.paras.len() as i64;
                rd.cursor = ((rd.cursor as i64 + d as i64).clamp(0, n - 1)) as usize;
                ensure_cursor_visible(rd, height);
            }
        }
        Action::PageFlip(d) => {
            let view = rd.view_h(height) as usize;
            let max = rd.lines.len().saturating_sub(view) as i64;
            // 光标段相对视窗顶的行偏移（91：翻页后落大致相同段）
            let rel = rd
                .paras
                .get(rd.cursor)
                .map(|p| p.start as i64 - rd.scroll as i64)
                .unwrap_or(0);
            // ⑥ 页边界跨章：第一页←上章（落章尾）、最后一页→下章（落章首）
            if d < 0 && rd.scroll == 0 && rd.cur > 0 {
                rd.cur -= 1;
                rd.scroll = 0;
                rd.pending_end = true;
                rd.need_load = true;
                return false;
            }
            if d > 0 && rd.scroll as i64 >= max && rd.cur + 1 < rd.chs.len() {
                rd.cur += 1;
                rd.scroll = 0;
                rd.cursor = 0;
                rd.paras.clear();
                rd.need_load = true;
                return false;
            }
            let next = (rd.scroll as i64 + d as i64 * view as i64).clamp(0, max);
            rd.scroll = next as u16;
            // 新视窗中包含 rel 行的段 → cursor
            let target = (next + rel).max(0) as usize;
            if let Some(i) = rd
                .paras
                .iter()
                .position(|p| p.start <= target && target < p.end)
            {
                rd.cursor = i;
            } else if !rd.paras.is_empty() {
                // 落在空行/图外 → 最近段
                rd.cursor = rd
                    .paras
                    .iter()
                    .position(|p| p.start > target)
                    .unwrap_or(rd.paras.len() - 1);
            }
        }
        Action::SpaceImg => {
            // 光标段是图片才聚焦（91：space 专用于查看/放大）
            if let Some(p) = rd.paras.get(rd.cursor) {
                if let ParaKind::Image(url) = &p.kind {
                    rd.img_focus = Some(url.clone());
                }
            }
        }
        Action::Help => {
            rd.help_shown = true;
            rd.help_scroll = 0;
        }
        Action::CloseHelp => rd.help_shown = false,
        Action::HelpScroll(d) => {
            if d < 0 {
                rd.help_scroll = rd.help_scroll.saturating_sub(d.unsigned_abs());
            } else {
                rd.help_scroll = rd.help_scroll.saturating_add(d as u16);
            }
        }
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
                rd.cursor = 0;
                rd.paras.clear();
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
        // 编辑器在 loop_with 拦截执行（需挂起终端），此处仅兜底
        Action::OpenEditor => {}
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
            let (lines, imgs) = wrap_with_imgs(&rd.raw, rd.content_width(width));
            rd.lines = lines;
            rd.imgs = imgs;
            rd.cursor = 0;
            rebuild_paras(rd);
            rd.msg.clear();
            Ok(())
        }
        Err(e) => {
            rd.raw.clear();
            rd.lines = Vec::new();
            rd.imgs.clear();
            rd.paras.clear();
            rd.cursor = 0;
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
        // 段首缩进 2 空格（m2994；段间空行保持恰好 1 行）
        let para = format!("  {para}");
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
    let (lines, imgs) = wrap_with_imgs(&rd.raw, width);
    rd.lines = lines;
    rd.imgs = imgs;
    rebuild_paras(rd);
}

/// 含图片标记的折行：`{{IMG:url}}` 独占行展开为图片块/占位，其余行走 wrap_text。
fn wrap_with_imgs(text: &str, width: u16) -> (Vec<Line<'static>>, Vec<(usize, usize, String)>) {
    let mut out = Vec::new();
    let mut imgs = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Line<'static>>| {
        if !buf.is_empty() {
            out.extend(wrap_text(buf, width));
            buf.clear();
        }
    };
    for line in text.split('\n') {
        if let Some(url) = line
            .strip_prefix("{{IMG:")
            .and_then(|s| s.strip_suffix("}}"))
        {
            flush(&mut buf, &mut out);
            let start = out.len();
            push_img_block(&mut out, url, width);
            imgs.push((start, out.len(), url.to_string()));
        } else {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(line);
        }
    }
    flush(&mut buf, &mut out);
    (out, imgs)
}

/// 图片块：有外部加载器→下载转 ANSI 行；否则/失败→纯文本占位行。
fn push_img_block(out: &mut Vec<Line<'static>>, url: &str, width: u16) {
    let h = crate::imgtool::IMG_MAX_H;
    match crate::imgtool::render(url, width, h) {
        Some(rows) => {
            out.push(Line::default());
            for row in rows {
                out.push(Line::from(row));
            }
            out.push(Line::default());
        }
        None => {
            out.push(Line::from(crate::imgtool::placeholder(url)));
        }
    }
}

/// 滚动：上/下界钳制；滚到底再向下 → 自动进下一章。
/// 由行+图区间重建段落列表（91）。文本段=非空行连续块；图区间独立成段。
fn build_paras(lines: &[Line<'static>], imgs: &[(usize, usize, String)]) -> Vec<Para> {
    let n = lines.len();
    let mut img_at = vec![false; n];
    let mut img_url: Vec<Option<String>> = vec![None; n];
    for (st, en, url) in imgs {
        for i in *st..(*en).min(n) {
            img_at[i] = true;
            img_url[i] = Some(url.clone());
        }
    }
    let blank = |l: &Line<'static>| l.spans.iter().all(|s| s.content.trim().is_empty());
    let mut paras = Vec::new();
    let mut i = 0usize;
    while i < n {
        if img_at[i] {
            let start = i;
            let url = img_url[i].clone().unwrap_or_default();
            while i < n && img_at[i] {
                i += 1;
            }
            paras.push(Para {
                start,
                end: i,
                kind: ParaKind::Image(url),
            });
        } else if !blank(&lines[i]) {
            let start = i;
            while i < n && !img_at[i] && !blank(&lines[i]) {
                i += 1;
            }
            paras.push(Para {
                start,
                end: i,
                kind: ParaKind::Text,
            });
        } else {
            i += 1;
        }
    }
    paras
}

/// 重折/换章后重建段落并保持光标在原行附近（91）。
fn rebuild_paras(rd: &mut Reader) {
    let old_line = rd.paras.get(rd.cursor).map(|p| p.start);
    rd.paras = build_paras(&rd.lines, &rd.imgs);
    if rd.paras.is_empty() {
        rd.cursor = 0;
        return;
    }
    rd.cursor = match old_line {
        Some(l) => rd
            .paras
            .iter()
            .position(|p| p.start >= l)
            .unwrap_or(rd.paras.len() - 1),
        None => 0,
    };
}

/// 光标段保持可见（91）：左右模式出视窗→整页跳（段顶对齐=自动翻页）；
/// 上下模式→1/4 屏平移（段保持在屏内 1/4~3/4 区间，不许超出）。
fn ensure_cursor_visible(rd: &mut Reader, height: u16) {
    let Some(p) = rd.paras.get(rd.cursor) else {
        return;
    };
    // yazi 式边缘滚动（103②）：光标进边缘带（ui.scroll_edge% 屏高）→
    // 内容平移把光标贴回带沿，光标相对屏幕停在带边不动；两种翻页模式统一。
    let view = rd.view_h(height) as usize;
    let edge = (view * rd.scroll_edge as usize / 100).max(1);
    let (top, bot) = (p.start, p.end.max(p.start + 1));
    let scroll = rd.scroll as usize;
    if top < scroll + edge {
        rd.scroll = top.saturating_sub(edge) as u16;
    } else if bot > scroll + view - edge {
        rd.scroll = (bot + edge).saturating_sub(view) as u16;
    }
}

/// 行滚动后光标跟随视窗（仍可见则不动，91）。
fn sync_cursor_from_view(rd: &mut Reader, height: u16) {
    if rd.paras.is_empty() {
        return;
    }
    let view = rd.view_h(height) as usize;
    let lo = rd.scroll as usize;
    let hi = lo + view;
    if let Some(p) = rd.paras.get(rd.cursor) {
        if p.end > lo && p.start < hi {
            return;
        }
    }
    rd.cursor = rd
        .paras
        .iter()
        .position(|p| p.end > lo && p.start < hi)
        .unwrap_or(rd.paras.len() - 1);
}

fn scroll_by(rd: &mut Reader, delta: i32, height: u16) {
    let view = rd.view_h(height);
    let max = rd.lines.len().saturating_sub(view as usize) as i64;
    let next = (rd.scroll as i64 + delta as i64).clamp(0, max);
    // 仅逐行/半页级滚动到底（含已停在底部）自动进下一章；G 等大跳停在章尾
    // ⑥ 首行↑：上滚到顶且小幅 → 上一章（落章尾）
    if delta < 0 && (-delta) as u16 <= view && next == 0 && rd.cur > 0 {
        rd.cur -= 1;
        rd.scroll = 0;
        rd.pending_end = true;
        rd.need_load = true;
        return;
    }
    let small = delta > 0 && delta as i64 <= view as i64;
    if small && next == max && rd.cur + 1 < rd.chs.len() {
        rd.cur += 1;
        rd.scroll = 0;
        rd.need_load = true;
        return;
    }
    rd.scroll = next as u16;
}

/// 103⑥：跨上章挂起 → 加载完落章尾（末页），实现"第一页 ← 上章"连续回翻。
fn settle_pending(rd: &mut Reader, height: u16) {
    if rd.pending_end {
        rd.pending_end = false;
        rd.scroll = rd.lines.len().saturating_sub(rd.view_h(height) as usize) as u16;
    }
}

/// 编辑器命令行拆分（87 纯函数）；vim 系自动插 -R（已有 -R 不重复）。
fn editor_argv(cmd: &str) -> (String, Vec<String>) {
    let mut it = cmd.split_whitespace().map(str::to_string);
    let bin = it.next().unwrap_or_default();
    let mut args: Vec<String> = it.collect();
    let base = bin.rsplit(['/', '\\']).next().unwrap_or("");
    if matches!(base, "vi" | "vim" | "nvim" | "gvim" | "view") && !args.iter().any(|a| a == "-R") {
        args.insert(0, "-R".into());
    }
    (bin, args)
}

/// 编辑器来源优先级（87 纯函数）：config/flag 值 > VISUAL > EDITOR > PATH 探测结果。
fn pick_editor(
    cfg_val: &str,
    visual: Option<&str>,
    editor: Option<&str>,
    probed: Option<&str>,
) -> Option<String> {
    for cand in [
        cfg_val,
        visual.unwrap_or(""),
        editor.unwrap_or(""),
        probed.unwrap_or(""),
    ] {
        if !cand.trim().is_empty() {
            return Some(cand.trim().to_string());
        }
    }
    None
}

/// PATH 探测（87）：按 nvim→vim→micro→leaf→glow 顺序取第一个存在的。
fn probe_editor() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for bin in ["nvim", "vim", "micro", "leaf", "glow"] {
        if std::env::split_paths(&path).any(|d| d.join(bin).is_file()) {
            return Some(bin.to_string());
        }
    }
    None
}

/// 解析外部编辑器（87；run 启动一次）。
fn resolve_editor(cfg: &Config) -> Option<String> {
    let visual = std::env::var("VISUAL").ok();
    let editor = std::env::var("EDITOR").ok();
    pick_editor(
        &cfg.ui_editor,
        visual.as_deref(),
        editor.as_deref(),
        probe_editor().as_deref(),
    )
}

/// 挂起 TUI → 外部编辑器只读打开当前章 → 恢复（87）。
/// raw/备用屏的进出行用新 stdout 句柄（同 event_loop teardown 先例）。
fn open_editor(
    rd: &mut Reader,
    term: &mut Terminal<CrosstermBackend<&mut Stdout>>,
) -> Result<(), String> {
    let Some(cmd) = rd.editor.clone() else {
        rd.msg = "未找到编辑器；可用 -e/--editor、config ui.editor 或 EDITOR 指定".into();
        return Ok(());
    };
    let path = std::env::temp_dir().join(format!("fqdt-chapter-{}.txt", rd.cur));
    std::fs::write(&path, &rd.raw).map_err(|e| format!("写临时文件失败: {e}"))?;
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    let (bin, args) = editor_argv(&cmd);
    let res = std::process::Command::new(&bin)
        .args(&args)
        .arg(&path)
        .status();
    let _ = enable_raw_mode();
    let _ = execute!(std::io::stdout(), EnterAlternateScreen);
    term.clear().map_err(|e| e.to_string())?;
    match res {
        Ok(st) if st.success() => rd.msg = format!("已用 {bin} 打开 {}", path.display()),
        Ok(st) => rd.msg = format!("编辑器退出码 {}", st.code().unwrap_or(-1)),
        Err(e) => rd.msg = format!("启动 {bin} 失败: {e}"),
    }
    Ok(())
}

/// 按键 → 动作（按焦点分发；键位习惯同 baca/epubreader-cli：j/k、n/p、g/G、Tab）。
fn on_key(code: KeyCode, rd: &Reader, height: u16) -> Action {
    // 帮助框开着：Esc/q/?/h 关闭；k/j/↑↓/PgUp/PgDn 滚动（103F），其余不穿透
    if rd.help_shown {
        return match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Char('h') => {
                Action::CloseHelp
            }
            KeyCode::Up | KeyCode::Char('k') => Action::HelpScroll(-1),
            KeyCode::Down | KeyCode::Char('j') => Action::HelpScroll(1),
            KeyCode::PageUp => Action::HelpScroll(-10),
            KeyCode::PageDown => Action::HelpScroll(10),
            _ => Action::Scroll(0),
        };
    }
    // 全局键（两种焦点都响应）
    match code {
        KeyCode::Char('?') | KeyCode::Char('h') => return Action::Help,
        KeyCode::Char('q') => {
            if rd.img_focus.is_some() {
                return Action::CloseImg;
            }
            return Action::Quit;
        }
        KeyCode::Char('z') => return Action::ToggleZen,
        KeyCode::Char('e') => return Action::OpenEditor,
        KeyCode::Tab => return Action::ToggleFocus,
        KeyCode::Char('i') => {
            if rd.img_focus.is_some() {
                return Action::CloseImg;
            }
            // 视窗内第一张图放大（77：查看位置太少 → 聚焦态）
            let view = rd.view_h(height) as usize;
            let lo = rd.scroll as usize;
            let hi = lo + view;
            if let Some((_, _, url)) = rd.imgs.iter().find(|(s, e, _)| *e > lo && *s < hi) {
                return Action::FocusImg(url.clone());
            }
            return Action::Scroll(0); // 无视窗内图：无操作
        }
        _ => {}
    }
    match rd.focus {
        Focus::Content => {
            let view = rd.view_h(height) as i32;
            let half = (view / 2).max(1);
            let two_third = (view * 2 / 3).max(1);
            match code {
                KeyCode::Esc => Action::FocusToc,
                // 段落光标（91）：j/k/↑↓ 移动选中段
                KeyCode::Char('j') | KeyCode::Down => Action::CursorMove(1),
                KeyCode::Char('k') | KeyCode::Up => Action::CursorMove(-1),
                // 翻页：`.` 半页下、`,` 2/3 页上；方向键左右整页（91/90）
                KeyCode::Char('.') | KeyCode::PageDown => Action::Scroll(half),
                KeyCode::Char(',') | KeyCode::PageUp => Action::Scroll(-two_third),
                KeyCode::Right => Action::PageFlip(1),
                KeyCode::Left => Action::PageFlip(-1),
                KeyCode::Char('d') => Action::Scroll(half),
                KeyCode::Char('u') => Action::Scroll(-half),
                KeyCode::Char('g') | KeyCode::Home => Action::Scroll(-(i32::MAX / 2)),
                KeyCode::Char('G') | KeyCode::End => Action::Scroll(i32::MAX / 2),
                // 切章（91）：<> 为主，n/p 保留兼容
                KeyCode::Char('>') | KeyCode::Char('n') => Action::NextCh,
                KeyCode::Char('<') | KeyCode::Char('p') => Action::PrevCh,
                // space 查看光标处图片（91）
                KeyCode::Char(' ') => Action::SpaceImg,
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
            KeyCode::Char('>') | KeyCode::Char('n') => Action::NextCh,
            KeyCode::Char('<') | KeyCode::Char('p') => Action::PrevCh,
            _ => Action::SelMove(0),
        },
    }
}

/// 渲染主入口：zen 全屏 / 普通分栏（bookokrat draw 同构）。
fn draw(f: &mut Frame, rd: &Reader) {
    if rd.help_shown {
        draw_help_box(f, rd);
        return;
    }
    if let Some(url) = &rd.img_focus {
        draw_img_focus(f, url, &rd.theme);
        return;
    }
    if rd.zen {
        draw_zen(f, rd);
    } else {
        draw_normal(f, rd);
    }
}

/// 图片聚焦态（77）：全屏放大单图，顶=url、底=返回提示；加载失败→占位行。
fn draw_img_focus(f: &mut Frame, url: &str, theme: &crate::theme::ThemeColors) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());
    f.render_widget(
        Paragraph::new(format!(" 图片 · {}", url))
            .style(util::fg_t(theme.status).add_modifier(ratatui::style::Modifier::BOLD)),
        chunks[0],
    );
    let inner_w = chunks[1].width.saturating_sub(2);
    let inner_h = chunks[1].height.saturating_sub(2);
    let block = Block::default().borders(Borders::ALL).title(" 放大 ");
    if let Some(rows) = crate::imgtool::render(url, inner_w, inner_h) {
        let lines: Vec<Line> = rows.into_iter().map(Line::from).collect();
        f.render_widget(Paragraph::new(lines), block.inner(chunks[1]));
    } else {
        f.render_widget(
            Paragraph::new(crate::imgtool::placeholder(url)),
            block.inner(chunks[1]),
        );
    }
    f.render_widget(block, chunks[1]);
    f.render_widget(
        Paragraph::new(" i/q/ESC 返回正文 · 图片由 timg/chafa/viu/catimg 渲染（未装则占位）")
            .style(util::fg(DIM)),
        chunks[2],
    );
}

/// 普通模式：上=分栏（左目录+右正文），下=3 行帮助栏。
fn draw_normal(f: &mut Frame, rd: &Reader) {
    // 85：非 zen 去底部 3 行帮助栏，两栏铺满
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(NAV_W), Constraint::Min(0)])
        .split(f.area());
    draw_toc(f, rd, main[0]);
    draw_content(f, rd, main[1]);
}

/// zen 模式：全屏四边框（m2998）——title=章题、底框=---- 状态 ----，
/// 正文四边留隔（上 1 行、下 1 行、左靠段首缩进、右 2 格，m3000）。
fn draw_zen(f: &mut Frame, rd: &Reader) {
    let area = f.area();
    let ch = &rd.chs[rd.cur];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(util::fg_t(rd.theme.border).add_modifier(ratatui::style::Modifier::BOLD))
        .title(ratatui::text::Line::from(vec![
            Span::styled(format!(" {}", rd.title), util::rev(rd.theme.selected)),
            Span::styled(format!(" {} ", ch.title), util::rev(rd.theme.title)),
        ]))
        .title_bottom(Span::styled(
            format!(" {} ", zen_status_fit(rd, area.width.saturating_sub(2))),
            util::fg_t(rd.theme.status),
        ));
    let inner = block.inner(area);
    // 上 1 行、下 1 行；宽 = 内宽 - 2（右留 2，与 content_width zen 一致）
    let body = Rect {
        x: inner.x.saturating_add(rd.gutter),
        y: inner.y.saturating_add(1),
        width: inner.width.saturating_sub(rd.gutter * 2),
        height: inner.height.saturating_sub(2).max(1),
    };
    f.render_widget(
        Paragraph::new(highlight_lines(rd)).scroll((rd.scroll, 0)),
        body,
    );
    f.render_widget(block, area);
}

/// zen 底框状态（m2999）：`---- x/y 章 · 行 a/b ----`，宽度不够逐级砍不卡半截。
fn zen_status_fit(rd: &Reader, width: u16) -> String {
    let ch_n = rd.cur + 1;
    let ch_t = rd.chs.len();
    let pos = (rd.scroll as usize + 1).min(rd.lines.len().max(1));
    let line_t = rd.lines.len();
    let w = |s: &str| Span::raw(s.to_string()).width() as u16;
    let cands = [
        format!("---- {ch_n}/{ch_t}章 · {pos}/{line_t}行 ----"),
        format!("{ch_n}/{ch_t}章 · {pos}/{line_t}行"),
        format!("{ch_n}/{ch_t}章"),
        format!("{ch_n}/{ch_t}"),
    ];
    cands
        .into_iter()
        .find(|c| w(c) <= width)
        .unwrap_or_default()
}

/// 左目录面板：边框按焦点高亮，当前章高亮，选中反白。
/// 目录视窗起点（103⑯）：sel 进下带（屏位 ≥ view-edge）→ 平移贴带沿；上带贴顶 start=0。
fn toc_start(sel: usize, view: usize, scroll_edge: u32) -> usize {
    if view == 0 {
        return 0;
    }
    let edge = (view * scroll_edge as usize / 100).max(1);
    let bottom = view.saturating_sub(edge).max(1); // 光标最大屏位 + 1
    (sel + 1).saturating_sub(bottom)
}

fn draw_toc(f: &mut Frame, rd: &Reader, area: Rect) {
    let focused = rd.focus == Focus::Nav;
    // 焦点：粗体青边 + 标题反白底块；非焦点：灰边（m2816 视觉表达打磨）
    let border = if focused {
        util::fg_t(rd.theme.border).add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        util::fg(DIM)
    };
    let title_style = if focused {
        util::rev(rd.theme.title)
    } else {
        util::fg(DIM)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(" 目录 ", title_style));
    let inner_h = area.height.saturating_sub(2) as usize;
    // 视窗（103⑯）：yazi 式边缘带——与正文 ensure_cursor_visible 同一 scroll_edge 语义
    let start = toc_start(rd.sel, inner_h, rd.scroll_edge);
    let lines: Vec<Line> = rd
        .chs
        .iter()
        .enumerate()
        .skip(start)
        .take(inner_h.max(1))
        .map(|(i, c)| {
            // 83：选中行恒有箭头——cur=▶（阅读位）、sel≠cur=▸（选择箭头）
            let mark = if i == rd.cur {
                "▶ "
            } else if i == rd.sel {
                "▸ "
            } else {
                "  "
            };
            let title = format!("{}{}", mark, c.title);
            if i == rd.sel {
                Line::from(Span::styled(title, util::rev(rd.theme.selected)))
            } else if i == rd.cur {
                Line::from(Span::styled(title, util::fg_t(rd.theme.title)))
            } else if focused {
                // 焦点面板普通行提亮：与非焦点面板（全 DIM）拉开色阶
                Line::from(Span::styled(title, util::fg_t(rd.theme.content)))
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
    let border = if focused {
        util::fg_t(rd.theme.border).add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        util::fg(DIM)
    };
    let title_style = if focused {
        util::rev(rd.theme.title)
    } else {
        util::fg(DIM)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(format!(" {} ", rd.title), title_style));
    // 非焦点面板整体降权（正文行全 DIM），切换时色阶反转即时可见
    let body_style = if focused {
        util::fg_t(rd.theme.content)
    } else {
        util::fg(DIM)
    };
    let inner = {
        let mut r = area;
        r.x += 1 + rd.gutter;
        r.y += 1;
        r.width = r.width.saturating_sub(2 + rd.gutter * 2);
        r.height = r.height.saturating_sub(2);
        r
    };
    // 顶行进度 + 正文
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);
    f.render_widget(
        Paragraph::new(top_line_fit(rd, body[0].width)).style(body_style),
        body[0],
    );
    // 页尾空行走 ui.tail_blank（103⑪：末行距下框 N 行）
    let mut para_area = body[2];
    para_area.height = para_area.height.saturating_sub(rd.tail_blank).max(1);
    f.render_widget(
        Paragraph::new(highlight_lines(rd))
            .style(body_style)
            .scroll((rd.scroll, 0)),
        para_area,
    );
    f.render_widget(block, area);
}

/// ?/h 全屏帮助框（批C③，m0137）：一个大框叙述 UI；键位明细只在
/// README/help.txt，此处不列 ?/h 键位（Esc 返回提示保留）。
fn draw_help_box(f: &mut Frame, rd: &Reader) {
    let theme = &rd.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(util::fg_t(theme.border).add_modifier(ratatui::style::Modifier::BOLD))
        .title(Span::styled(" --Help-- ", util::rev(theme.title)));
    let inner = block.inner(f.area());
    let body = Rect {
        x: inner.x.saturating_add(rd.gutter),
        y: inner.y.saturating_add(1),
        width: inner.width.saturating_sub(rd.gutter * 2),
        height: inner.height.saturating_sub(2).max(1),
    };
    let t = |s: &str| Span::styled(s.to_string(), util::fg_t(theme.content));
    let head = |s: &str| Span::styled(s.to_string(), util::fg_t(theme.title));
    let lines = vec![
        Line::from(vec![t("阅读视图（view）")]),
        Line::from(vec![t("  连贯阅读一章内容：左侧目录面板，右侧正文面板。")]),
        Line::from(vec![t(
            "  正文按段落排布——段首缩进两格，段与段之间恰好空一行。",
        )]),
        Line::from(vec![t(
            "  上下移动会选中一个段落并整段高亮；图片段不高亮。",
        )]),
        Line::default(),
        Line::from(vec![head("翻页")]),
        Line::from(vec![t("  翻页方式由配置 ui.page_mode 决定：")]),
        Line::from(vec![t(
            "  right-left（默认）为左右渲染——左右键平行翻页，上下滚到屏底自动翻页；",
        )]),
        Line::from(vec![t(
            "  up-down 为上下渲染——选中段保持在屏幕范围内平移，左右键整屏切换。",
        )]),
        Line::default(),
        Line::from(vec![head("zen 模式")]),
        Line::from(vec![t(
            "  z 切换全屏四边框：标题为章节名，底框显示章节与行进度，",
        )]),
        Line::from(vec![t("  正文与四边框保持一行/一格距离。")]),
        Line::default(),
        Line::from(vec![head("进度与图片")]),
        Line::from(vec![t("  退出时自动记忆阅读位置，下次进入回到原处。")]),
        Line::from(vec![t("  光标停在图片段时可放大查看。")]),
        Line::default(),
        Line::from(vec![t("完整快捷键见 ~/.config/fqdt/help.txt")]),
        Line::from(vec![t("<Esc>/? 关闭返回；k/j/↑↓ 翻动")]),
    ];
    // 103F：超一屏可滚动，偏移钳在总行数-屏高内
    let max = lines.len().saturating_sub(body.height as usize) as u16;
    let off = rd.help_scroll.min(max);
    f.render_widget(Paragraph::new(lines).scroll((off, 0)), body);
    f.render_widget(block, f.area());
}

/// 段落光标高亮（91）：文本段整段换背景色（不同色）；图片段不高亮。
fn highlight_lines(rd: &Reader) -> Vec<Line<'static>> {
    let mut lines = rd.lines.clone();
    if let Some(p) = rd.paras.get(rd.cursor) {
        if p.kind == ParaKind::Text {
            for l in lines
                .iter_mut()
                .skip(p.start)
                .take(p.end.saturating_sub(p.start))
            {
                *l = l.clone().style(util::bg(DIM));
            }
        }
    }
    lines
}

/// 状态行宽度自适应（批C①，m0130）：全量 → 去书名 → 仅进度 → 仅章进度 → 空行。
/// 逐级砍到总宽 ≤ width，绝不卡半截。
fn top_line_fit(rd: &Reader, width: u16) -> Line<'static> {
    let ch = &rd.chs[rd.cur];
    let total = rd.lines.len();
    let pos = (rd.scroll as usize + 1).min(total.max(1));
    let progress = format!("  {}/{}章 · {}/{}行", rd.cur + 1, rd.chs.len(), pos, total);
    let ch_only = format!("  {}/{}章", rd.cur + 1, rd.chs.len());
    // 候选层级（由全到简）；⑧ 顶行统一去书名（书名进框标题）
    let mk_no_title = |_rd: &Reader, ch: &Chapter| {
        Line::from(vec![
            Span::styled(ch.title.clone(), util::fg_t(rd.theme.title)),
            Span::styled(progress.clone(), util::fg_t(rd.theme.status)),
        ])
    };
    let mk_progress = || {
        Line::from(Span::styled(
            progress.trim_start().to_string(),
            util::fg_t(rd.theme.status),
        ))
    };
    let mk_ch = || {
        Line::from(Span::styled(
            ch_only.trim_start().to_string(),
            util::fg_t(rd.theme.status),
        ))
    };
    let w = width as usize;
    let fits = |l: &Line| l.spans.iter().map(|s| s.width()).sum::<usize>() <= w;
    let cands = [mk_no_title(rd, ch), mk_progress(), mk_ch(), Line::default()];
    cands.into_iter().find(fits).unwrap_or_default()
}

// draw_helpbar 已删（85 非 zen 去帮助栏）；HELP/HELP_NAV 供批C③ ?/h 帮助框使用。

#[cfg(test)]
mod tests {
    /// 103⑥：页边界/首行末行四触发跨章。
    #[test]
    fn chapter_boundary_triggers() {
        let chs = vec![
            Chapter {
                index: 1,
                item_id: "a".into(),
                title: "一".into(),
            },
            Chapter {
                index: 2,
                item_id: "b".into(),
                title: "二".into(),
            },
        ];
        let mut rd = Reader::new("b".into(), chs);
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        rd.zen = true;
        let h = 24u16;
        let view = rd.view_h(h);
        // 最后一页 → 下章（scroll=0）
        rd.scroll = 100 - view;
        apply_action(Action::PageFlip(1), &mut rd, h);
        assert_eq!(rd.cur, 1, "最后一页→下章");
        assert_eq!(rd.scroll, 0);
        assert!(rd.need_load);
        // 第一页 ← 上章（pending_end → settle 落章尾）
        rd.need_load = false;
        rd.cur = 1;
        rd.scroll = 0;
        apply_action(Action::PageFlip(-1), &mut rd, h);
        assert_eq!(rd.cur, 0, "第一页←上章");
        assert!(rd.pending_end);
        rd.lines = (0..80).map(|i| Line::from(format!("M{i}"))).collect();
        settle_pending(&mut rd, h);
        assert_eq!(rd.scroll as usize, 80 - view as usize, "落章尾末页");
        assert!(!rd.pending_end);
        // 首行↑ 触发上章
        rd.cur = 1;
        rd.scroll = 0;
        rd.pending_end = false;
        rd.need_load = false;
        scroll_by(&mut rd, -1, h);
        assert_eq!(rd.cur, 0, "首行↑上章");
        assert!(rd.pending_end);
        settle_pending(&mut rd, h);
        // 末行↓：滚到本章底自动下章（small）
        rd.cur = 0;
        rd.scroll = 0;
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        scroll_by(&mut rd, i32::MAX / 2, h); // 到底
        scroll_by(&mut rd, 1, h); // small 到底 → 下章
        assert_eq!(rd.cur, 1, "末行↓下章");
        assert!(rd.need_load);
    }

    /// 103 步骤4：view_h 与渲染区一致（zen=屏-4；normal=屏-4-tail）。
    #[test]
    fn view_h_matches_render() {
        let mut rd = Reader::new("t".into(), vec![]);
        rd.zen = true;
        assert_eq!(rd.view_h(24), 20);
        rd.zen = false;
        rd.tail_blank = 1;
        assert_eq!(rd.view_h(24), 19);
        rd.tail_blank = 0;
        assert_eq!(rd.view_h(24), 20);
        assert_eq!(rd.view_h(1), 1); // 不下溢
    }

    /// 103 步骤4：PageFlip 整页步长=视窗高（页边界不跳行）。
    #[test]
    fn page_flip_steps_by_view() {
        let chs = vec![Chapter {
            index: 1,
            item_id: "a".into(),
            title: "t".into(),
        }];
        let mut rd = Reader::new("b".into(), chs);
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        rd.zen = true;
        let h = 24u16;
        let view = rd.view_h(h) as usize;
        apply_action(Action::PageFlip(1), &mut rd, h);
        assert_eq!(rd.scroll as usize, view, "正翻一步=视窗高");
        apply_action(Action::PageFlip(-1), &mut rd, h);
        assert_eq!(rd.scroll, 0);
        // 末页 clamp 不越界
        apply_action(Action::PageFlip(99), &mut rd, h);
        assert_eq!(rd.scroll as usize, 100 - view);
    }

    /// 103 步骤2：content_width 对称吃 gutter（框线 2 + 左右各 g；normal 另扣 NAV_W）。
    #[test]
    fn content_width_gutter_pad() {
        let mut rd = Reader::new("t".into(), vec![]);
        rd.gutter = 1;
        rd.zen = false;
        assert_eq!(rd.content_width(80), 80 - (NAV_W + 4));
        rd.zen = true;
        assert_eq!(rd.content_width(80), 80 - 4);
        rd.gutter = 2;
        assert_eq!(rd.content_width(80), 80 - 6);
    }

    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn wrap_with_imgs_marker_and_text() {
        // 无工具环境（CI）：图片行→占位；普通行照常折行
        let raw = "前文\n{{IMG:/rel/x.png}}\n后文";
        let (lines, imgs) = wrap_with_imgs(raw, 40);
        let joined: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("[图片: /rel/x.png]"), "got: {joined:?}");
        assert!(
            joined.contains("前文") && joined.contains("后文"),
            "got: {joined:?}"
        );
        // 图块区间记录（77：供 i 键聚焦）
        assert_eq!(imgs.len(), 1, "got: {imgs:?}");
        let (s, e, url) = &imgs[0];
        assert_eq!(url, "/rel/x.png");
        assert!(*s < *e && *e <= lines.len());
        assert_eq!(lines[*s].to_string(), "[图片: /rel/x.png]");
        // 无标记时与 wrap_text 行数一致
        let (a, ia) = wrap_with_imgs("甲乙丙\n丁戊", 40);
        let b = wrap_text("甲乙丙\n丁戊", 40);
        assert_eq!(a.len(), b.len());
        assert!(ia.is_empty());
    }

    #[test]
    fn img_focus_key_and_draw() {
        let mut rd = Reader::new("书名".into(), Vec::new());
        rd.raw = "前文\n{{IMG:/rel/x.png}}\n后文".to_string();
        let (lines, imgs) = wrap_with_imgs(&rd.raw, 60);
        rd.lines = lines;
        rd.imgs = imgs;
        rd.scroll = 0;
        rd.focus = Focus::Content;
        // i → 聚焦第一张图
        let act = on_key(KeyCode::Char('i'), &rd, 24);
        let Action::FocusImg(url) = act else {
            panic!("expected FocusImg")
        };
        assert_eq!(url, "/rel/x.png");
        // 聚焦态 q → 返回正文而非退出
        rd.img_focus = Some(url);
        assert!(matches!(
            on_key(KeyCode::Char('q'), &rd, 24),
            Action::CloseImg
        ));
        assert!(matches!(
            on_key(KeyCode::Char('i'), &rd, 24),
            Action::CloseImg
        ));
        // 应用后 img_focus 生效
        let quit = apply_action(Action::FocusImg("/rel/x.png".into()), &mut rd, 24);
        assert!(!quit);
        assert_eq!(rd.img_focus.as_deref(), Some("/rel/x.png"));
        // 聚焦态全屏渲染（TestBackend）
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                if let Some(u) = rd.img_focus.clone() {
                    draw_img_focus(f, &u, &rd.theme);
                }
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<String>();
        let content = content.replace(' ', "");
        assert!(
            content.contains("放大"),
            "聚焦面板标题缺失: got {content:?}"
        );
        assert!(
            content.contains("图片") || content.contains("返回"),
            "got {content:?}"
        );
        // 关闭
        apply_action(Action::CloseImg, &mut rd, 24);
        assert!(rd.img_focus.is_none());
    }

    #[test]
    fn wrap_chinese_by_width() {
        // 宽度下限 20：宽 10 被抬到 20 列；段首缩进 2 → 2+9 全角=20 列
        let lines = wrap_text("一二三四五六七八九十", 10);
        assert_eq!(lines[0].to_string(), "  一二三四五六七八九");
        assert_eq!(lines[1].to_string(), "十");
        // 真实宽 24：首行 2 缩进+11 字=24；续行 12 字；30 字 = 11+12+7 + 段尾空行
        let lines = wrap_text(&"一".repeat(30), 24);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].to_string(), format!("  {}", "一".repeat(11)));
        assert_eq!(lines[1].to_string(), "一".repeat(12));
        assert_eq!(lines[2].to_string(), "一".repeat(7));
        assert_eq!(lines[3].to_string(), "");
    }

    /// 83：目录选中行恒显选择箭头 + 章题头部完整（m0085-1）。
    #[test]
    fn startup_modes_from_config() {
        let mut rd = Reader::new("t".into(), Vec::new());
        let mut cfg = Config::default();
        apply_startup_modes(&mut rd, &cfg);
        assert!(!rd.zen, "默认 normal 不应 zen");
        assert_eq!(rd.page_mode, PageMode::RightLeft);
        cfg.ui_view_mode = "zen".into();
        cfg.ui_page_mode = "up-down".into();
        apply_startup_modes(&mut rd, &cfg);
        assert!(rd.zen, "zen 启动");
        assert_eq!(rd.page_mode, PageMode::UpDown);
    }

    #[test]
    fn toc_selected_row_shows_arrow_and_full_title() {
        use ratatui::{backend::TestBackend, Terminal};
        let chs = vec![
            Chapter {
                index: 1,
                item_id: "a".into(),
                title: "第171章 前章标题".into(),
            },
            Chapter {
                index: 2,
                item_id: "b".into(),
                title: "第172章 编外也是编".into(),
            },
            Chapter {
                index: 3,
                item_id: "c".into(),
                title: "第173章 很长很长的章题后面还有更多".into(),
            },
        ];
        let mut rd = Reader::new("书".into(), chs);
        rd.focus = Focus::Nav;
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: 28,
            height: 10,
        };
        // sel≠cur：选中行带 ▸ 箭头 + 章题头完整
        rd.sel = 1;
        rd.cur = 0;
        let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term.draw(|f| draw_toc(f, &rd, area)).unwrap();
        let buf = term.backend().buffer().clone();
        // 取 inner（去左右边框），全角归一化去空格
        let row: String = (1..27u16)
            .map(|x| buf[(x, 2u16)].symbol().to_string())
            .collect();
        let row: String = row.chars().filter(|c| *c != ' ').collect();
        assert!(row.starts_with('▸'), "选中行缺选择箭头: {row:?}");
        assert!(
            row.contains("第172章编外也是编"),
            "选中行章题不完整: {row:?}"
        );
        // sel==cur：▶ 箭头保留
        rd.sel = 2;
        rd.cur = 2;
        let mut term2 = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term2.draw(|f| draw_toc(f, &rd, area)).unwrap();
        let buf2 = term2.backend().buffer().clone();
        let row2: String = (1..27u16)
            .map(|x| buf2[(x, 3u16)].symbol().to_string())
            .collect();
        let row2: String = row2.chars().filter(|c| *c != ' ').collect();
        assert!(row2.starts_with('▶'), "sel==cur 应显 ▶: {row2:?}");
        assert!(row2.contains("第173章"), "章题头缺失: {row2:?}");
    }

    /// 分栏面板标题与焦点切换渲染（m2816 视觉打磨回归）。
    #[test]
    fn draw_normal_renders_panel_titles() {
        use ratatui::{backend::TestBackend, Terminal};
        let chs = vec![
            Chapter {
                index: 1,
                item_id: "a".into(),
                title: "一".into(),
            },
            Chapter {
                index: 2,
                item_id: "b".into(),
                title: "二".into(),
            },
        ];
        let mut rd = Reader::new("书名".into(), chs);
        rd.lines = wrap_text("正文测试内容", 40);
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        fn grab(term: &mut Terminal<TestBackend>, rd: &Reader) -> String {
            term.draw(|f| draw(f, rd)).unwrap();
            // TestBackend 全角字占双 cell（字+空格），归一化去空格再断言
            term.backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .chars()
                .filter(|c| *c != ' ')
                .collect()
        }
        // 焦点在正文：两面板标题都在
        let t1 = grab(&mut term, &rd);
        assert!(t1.contains("目录"), "缺目录标题: {t1:?}");
        assert!(t1.contains("正文"), "缺正文标题: {t1:?}");
        // 焦点切目录：标题保持（差分渲染下整 buffer 仍含）
        rd.focus = Focus::Nav;
        let t2 = grab(&mut term, &rd);
        assert!(t2.contains("目录") && t2.contains("正文"));
        // zen 模式藏面板
        rd.zen = true;
        let t3 = grab(&mut term, &rd);
        assert!(!t3.contains("目录"), "zen 不应有目录面板");
    }

    /// 批C①：状态行宽度自适应——每级总宽 ≤ 给定宽，绝不卡半截。
    #[test]
    fn top_line_fit_never_truncates() {
        let chs = vec![Chapter {
            index: 1,
            item_id: "a".into(),
            title: "一个非常长的章节标题用来测试压缩".into(),
        }];
        let mut rd = Reader::new("超长书名测试用例：某某某小说的完整标题".into(), chs);
        rd.lines = (0..50)
            .map(|i| Line::from(format!("第{i}行内容")))
            .collect();
        // 各宽度档：输出显示宽 ≤ 宽度
        for w in [0u16, 5, 10, 16, 24, 40, 80, 200] {
            let l = top_line_fit(&rd, w);
            let got: usize = l.spans.iter().map(|s| s.width()).sum();
            assert!(
                got <= w as usize,
                "width={w} 时输出宽 {got} 超限: {:?}",
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<Vec<_>>()
            );
        }
        // 极宽 → 全量：⑧ 顶行统一去书名（书名进框标题），状态 `138/173章 · 1/199行`
        let full = top_line_fit(&rd, 200);
        let text: String = full.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(!text.contains("超长书名"), "⑧ 顶行不应含书名: {text}");
        assert!(text.contains("1/1章"), "全量应含章进度: {text}");
        assert!(text.contains("1/50行"), "全量应含行进度: {text}");
        // 中宽 → 砍掉书名仍含章/行进度
        let mid = top_line_fit(&rd, 30);
        let mtext: String = mid.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(!mtext.contains("超长书名"), "30 宽应砍书名: {mtext}");
        assert!(mtext.contains("章"), "30 宽应保留章进度: {mtext}");
        // 极窄 → 空行或极简，不 panic 不超宽
        let tiny = top_line_fit(&rd, 3);
        let tw: usize = tiny.spans.iter().map(|s| s.width()).sum();
        assert!(tw <= 3, "极窄仍不超: {tw}");
    }

    #[test]
    fn key_bindings_content() {
        let rd = Reader::new("t".into(), vec![]);
        let h = 24;
        assert!(matches!(on_key(KeyCode::Char('q'), &rd, h), Action::Quit));
        // 91 新键位：j/k/↑↓ = 段落光标
        assert!(matches!(
            on_key(KeyCode::Char('j'), &rd, h),
            Action::CursorMove(1)
        ));
        assert!(matches!(
            on_key(KeyCode::Char('k'), &rd, h),
            Action::CursorMove(-1)
        ));
        assert!(matches!(
            on_key(KeyCode::Down, &rd, h),
            Action::CursorMove(1)
        ));
        assert!(matches!(
            on_key(KeyCode::Up, &rd, h),
            Action::CursorMove(-1)
        ));
        // <> 切章（n/p 兼容保留）
        assert!(matches!(on_key(KeyCode::Char('>'), &rd, h), Action::NextCh));
        assert!(matches!(on_key(KeyCode::Char('<'), &rd, h), Action::PrevCh));
        assert!(matches!(on_key(KeyCode::Char('n'), &rd, h), Action::NextCh));
        assert!(matches!(on_key(KeyCode::Char('p'), &rd, h), Action::PrevCh));
        // ,. 翻 2/3、1/2 页；左右整页
        assert!(matches!(on_key(KeyCode::Char(','), &rd, h), Action::Scroll(d) if d < 0));
        assert!(matches!(on_key(KeyCode::Char('.'), &rd, h), Action::Scroll(d) if d > 0));
        assert!(matches!(
            on_key(KeyCode::Left, &rd, h),
            Action::PageFlip(-1)
        ));
        assert!(matches!(
            on_key(KeyCode::Right, &rd, h),
            Action::PageFlip(1)
        ));
        // space = 查看光标处图片（非切章）
        assert!(matches!(
            on_key(KeyCode::Char(' '), &rd, h),
            Action::SpaceImg
        ));
        assert!(matches!(
            on_key(KeyCode::Char('z'), &rd, h),
            Action::ToggleZen
        ));
        assert!(matches!(on_key(KeyCode::Tab, &rd, h), Action::ToggleFocus));
        assert!(matches!(on_key(KeyCode::Esc, &rd, h), Action::FocusToc));
    }

    /// 91：段落切分——文本段按空行分，图区间独立成段。
    #[test]
    fn paras_split_text_and_image() {
        let lines = vec![
            Line::from("段一行"),
            Line::from("段一行续"),
            Line::default(), // 空行分隔
            Line::from("段二行"),
            Line::default(),
            Line::from("[图占位]"), // 图块行
            Line::default(),
            Line::from("段三行"),
        ];
        let imgs = vec![(5, 6, "http://x/y.png".to_string())];
        let paras = build_paras(&lines, &imgs);
        assert_eq!(paras.len(), 4, "文本三段+图一段: {:?}", paras);
        assert_eq!(
            paras[0],
            (Para {
                start: 0,
                end: 2,
                kind: ParaKind::Text
            })
        );
        assert_eq!(paras[1].start, 3);
        assert!(matches!(paras[2].kind, ParaKind::Image(ref u) if u == "http://x/y.png"));
        assert_eq!(paras[2].start, 5);
        assert_eq!(paras[3].start, 7);
    }

    /// 91：space 光标在图段才聚焦；文本段无操作。
    #[test]
    fn space_focuses_cursor_image_only() {
        let mut rd = Reader::new("t".into(), vec![]);
        rd.lines = vec![Line::from("a"), Line::from("[图]"), Line::from("b")];
        rd.imgs = vec![(1, 2, "u://img".to_string())];
        rebuild_paras(&mut rd);
        assert_eq!(rd.paras.len(), 3);
        rd.cursor = 0;
        // 文本段 space → 无聚焦
        apply_action(Action::SpaceImg, &mut rd, 24);
        assert!(rd.img_focus.is_none());
        // 图段 space → 聚焦
        rd.cursor = 1;
        apply_action(Action::SpaceImg, &mut rd, 24);
        assert_eq!(rd.img_focus.as_deref(), Some("u://img"));
        // 关图
        apply_action(Action::CloseImg, &mut rd, 24);
        assert!(rd.img_focus.is_none());
    }

    /// 91 批B：PageFlip 整页切换且光标段行相对偏移保持。
    #[test]
    fn page_flip_keeps_cursor_offset() {
        let mut rd = Reader::new("t".into(), vec![]);
        // 40 行，每 10 行一段（插空行分段）
        let mut lines = Vec::new();
        for seg in 0..4 {
            for i in 0..10 {
                lines.push(Line::from(format!("s{seg}l{i}")));
            }
            lines.push(Line::default());
        }
        rd.lines = lines;
        rebuild_paras(&mut rd);
        assert_eq!(rd.paras.len(), 4);
        let h = 24;
        let view = rd.view_h(h); // 103④：整页=渲染视窗（normal=24-4-1=19）
        rd.cursor = 1; // start=11，scroll=0 → rel=11
        apply_action(Action::PageFlip(1), &mut rd, h);
        assert_eq!(rd.scroll, view, "整页=view_h");
        // target = 19+11 = 30 → 段界 s0:0-9, s1:11-20, s2:22-31 → 落 s2
        assert_eq!(rd.cursor, 2, "target=30 落 s2 段: {:?}", rd.paras);
        // 回翻：rel=22-19=3 → target=3 落 s0
        apply_action(Action::PageFlip(-1), &mut rd, h);
        assert_eq!(rd.scroll, 0);
        assert_eq!(rd.cursor, 0, "rel=3 回到 s0");
    }

    /// 91 批B：左右模式光标出视窗→段顶对齐（自动翻页）；上下模式 1/4 平移不许超出。
    #[test]
    fn ensure_cursor_edge_scroll() {
        let mut rd = Reader::new("t".into(), vec![]);
        let mut lines = Vec::new();
        for seg in 0..10 {
            for i in 0..10 {
                lines.push(Line::from(format!("s{seg}l{i}")));
            }
            lines.push(Line::default());
        }
        rd.lines = lines;
        rebuild_paras(&mut rd);
        let h = 24;
        let view = rd.view_h(h) as usize; // 103④：19
        let q = view * rd.scroll_edge as usize / 100; // 25% → 4
                                                      // --- 屏外段（原 RightLeft 跳页）→ 边缘滚动贴底带 ---
        rd.scroll = 0;
        rd.cursor = 3; // start=33, bot=43 > view-q → 下移
        ensure_cursor_visible(&mut rd, h);
        assert_eq!(
            rd.scroll as usize,
            43 + q - view,
            "屏外段 bot+edge-view 贴底带"
        );
        // --- 光标进下边缘带 → 平移 ---
        rd.scroll = 0;
        rd.cursor = 1; // start=11, bot=21；bot(21) > view-q(15)
        ensure_cursor_visible(&mut rd, h);
        assert_eq!(rd.scroll as usize, 21 + q - view, "bot+edge-view");
        // 段在屏内边缘带上 → 上移
        rd.scroll = 20;
        rd.cursor = 1; // top=11 < 20+4 → scroll = 11-4
        ensure_cursor_visible(&mut rd, h);
        assert_eq!(rd.scroll as usize, 11 - q, "top-edge 平移");
        // 不越界：首段
        rd.scroll = 10;
        rd.cursor = 0;
        ensure_cursor_visible(&mut rd, h);
        assert_eq!(rd.scroll, 0, "top-edge 钳 0");
        // --- scroll_edge 配置生效：带宽随比例变 ---
        rd.scroll_edge = 40; // edge = 19*40/100 = 7
        rd.scroll = 20;
        rd.cursor = 1; // top=11 < 20+7 → 11-7
        ensure_cursor_visible(&mut rd, h);
        assert_eq!(rd.scroll as usize, 11 - 7, "scroll_edge=40 吃配置");
    }

    /// 91 光标移动钳制端点且保持可见。
    #[test]
    fn cursor_move_clamps_and_visible() {
        let mut rd = Reader::new("t".into(), vec![]);
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        rebuild_paras(&mut rd);
        // 单行连续非空 → 全部一行一段？空行无 → 1 段
        assert_eq!(rd.paras.len(), 1);
        apply_action(Action::CursorMove(5), &mut rd, 24);
        assert_eq!(rd.cursor, 0, "只有一段，钳制在 0");
        apply_action(Action::CursorMove(-5), &mut rd, 24);
        assert_eq!(rd.cursor, 0);

        // 多段：插入空行
        rd.lines.insert(50, Line::default());
        rebuild_paras(&mut rd);
        assert_eq!(rd.paras.len(), 2);
        apply_action(Action::CursorMove(1), &mut rd, 24);
        assert_eq!(rd.cursor, 1);
        // 第二段 start=51 远超视窗 → ensure 拉回 scroll
        assert!(rd.scroll >= 51 - 24, "scroll={}", rd.scroll);
        apply_action(Action::CursorMove(-1), &mut rd, 24);
        assert_eq!(rd.cursor, 0);
    }

    #[test]
    fn key_bindings_nav() {
        let mut rd = Reader::new(
            "t".into(),
            vec![
                Chapter {
                    index: 1,
                    item_id: "a".into(),
                    title: "一".into(),
                },
                Chapter {
                    index: 2,
                    item_id: "b".into(),
                    title: "二".into(),
                },
            ],
        );
        rd.focus = Focus::Nav;
        let h = 24;
        assert!(matches!(
            on_key(KeyCode::Char('j'), &rd, h),
            Action::SelMove(1)
        ));
        assert!(matches!(on_key(KeyCode::Enter, &rd, h), Action::GotoSel));
        assert!(matches!(on_key(KeyCode::Esc, &rd, h), Action::BackContent));
        // 全局键在 Nav 下同样生效
        assert!(matches!(on_key(KeyCode::Char('q'), &rd, h), Action::Quit));
        assert!(matches!(
            on_key(KeyCode::Char('z'), &rd, h),
            Action::ToggleZen
        ));
    }

    #[test]
    fn scroll_clamps_and_auto_next_chapter() {
        let mut rd = Reader::new(
            "t".into(),
            vec![
                Chapter {
                    index: 1,
                    item_id: "a".into(),
                    title: "一".into(),
                },
                Chapter {
                    index: 2,
                    item_id: "b".into(),
                    title: "二".into(),
                },
            ],
        );
        rd.lines = (0..100).map(|i| Line::from(format!("L{i}"))).collect();
        scroll_by(&mut rd, -5, 10);
        assert_eq!(rd.scroll, 0, "上界钳制");
        scroll_by(&mut rd, i32::MAX / 2, 10);
        // 103④：下界=lines-view_h（h=10 → view=10-4-1=5 → 95）
        assert_eq!(rd.scroll as usize, 100 - rd.view_h(10) as usize, "下界钳制");
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
        let mut rd = Reader::new(
            "t".into(),
            vec![
                Chapter {
                    index: 1,
                    item_id: "a".into(),
                    title: "一".into(),
                },
                Chapter {
                    index: 2,
                    item_id: "b".into(),
                    title: "二".into(),
                },
            ],
        );
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
        let chs = [Chapter {
            index: 1,
            item_id: "x".into(),
            title: "一".into(),
        }];
        assert!(chs.iter().position(|c| c.item_id == "missing").is_none());
        assert_eq!(chs.iter().position(|c| c.item_id == "x"), Some(0));
    }

    /// 103F+⑫：帮助框超屏滚动、键位、底部提示提亮与段首缩进。
    #[test]
    fn help_scroll_and_indent() {
        use ratatui::backend::TestBackend;
        let mut rd = Reader::new("t".into(), vec![]);
        rd.help_shown = true;
        // 键位 → 滚动动作
        assert!(matches!(
            on_key(KeyCode::Char('j'), &rd, 24),
            Action::HelpScroll(1)
        ));
        assert!(matches!(
            on_key(KeyCode::Up, &rd, 24),
            Action::HelpScroll(-1)
        ));
        assert!(matches!(
            on_key(KeyCode::PageDown, &rd, 24),
            Action::HelpScroll(10)
        ));
        // 关闭键仍有效
        assert!(matches!(on_key(KeyCode::Esc, &rd, 24), Action::CloseHelp));
        // apply 累加与负向饱和
        apply_action(Action::HelpScroll(3), &mut rd, 24);
        apply_action(Action::HelpScroll(-5), &mut rd, 24);
        assert_eq!(rd.help_scroll, 0, "负向饱和到 0");
        // 矮屏 60x14：未滚到底部提示在屏外，超量偏移被钳到底可见
        let w = 60usize;
        let draw_buf = |rd: &Reader| -> Vec<String> {
            let backend = TestBackend::new(60, 14);
            let mut t = ratatui::Terminal::new(backend).unwrap();
            t.draw(|f| draw_help_box(f, rd)).unwrap();
            t.backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol().to_string())
                .collect()
        };
        let row_of = |buf: &[String], needle: &str| -> Option<usize> {
            (0..buf.len() / w).find(|&r| {
                buf[r * w..(r + 1) * w]
                    .concat()
                    .replace(' ', "")
                    .contains(needle)
            })
        };
        rd.help_scroll = 0;
        let top = draw_buf(&rd);
        assert_eq!(row_of(&top, "关闭返回"), None, "未滚动底部提示在屏外");
        // ⑫③ 段首缩进两格（未滚状态正文行在屏内）
        let r0 = row_of(&top, "正文按段落").expect("正文行在屏内");
        let line0 = top[r0 * w..(r0 + 1) * w].concat();
        let col0 = line0.find('正').unwrap();
        assert!(col0 >= 2, "段首缩进两格，实际列 {col0}");
        rd.help_scroll = 99; // 超量由 draw 钳到 max
        let bot = draw_buf(&rd);
        assert!(row_of(&bot, "关闭返回").is_some(), "滚到底底部提示可见");
    }

    #[test]
    fn toc_start_edge_band() {
        // 20 行视窗、edge=25% → bottom=15：sel≤14 不滚（屏内），15 起平移贴带
        assert_eq!(toc_start(0, 20, 25), 0);
        assert_eq!(toc_start(14, 20, 25), 0);
        assert_eq!(toc_start(15, 20, 25), 1);
        assert_eq!(toc_start(30, 20, 25), 16);
        assert_eq!(toc_start(0, 0, 25), 0);
        // 极小视窗 edge 至少 1，bottom 至少 1
        assert_eq!(toc_start(2, 3, 5), 1);
        // edge 更大（45%）→ 更早开始滚动
        assert_eq!(toc_start(11, 20, 45), 1); // bottom=20-9=11，12>11 → 12-11
        assert_eq!(toc_start(9, 20, 45), 0); // 刚出带上沿不滚
    }

    /// 全屏渲染后从正文列提取本测试的行号（"行NN"），供页边界行集断言。
    fn screen_row_ids(rd: &Reader) -> Vec<u32> {
        use ratatui::backend::TestBackend;
        let mut term = Terminal::new(TestBackend::new(80, 16)).unwrap();
        term.draw(|f| draw(f, rd)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut out = Vec::new();
        for y in 0..16u16 {
            let row: String = (29..78u16)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            let row: String = row.chars().filter(|c| !c.is_whitespace()).collect();
            if let Some(pos) = row.find('行') {
                let rest = &row[pos + '行'.len_utf8()..];
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.len() == 2 {
                    if let Ok(n) = digits.parse::<u32>() {
                        out.push(n);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn page_boundary_row_set() {
        // 103④：翻页交界行集连续——两页之间不许跳行/重复
        let chs = vec![Chapter {
            index: 1,
            item_id: "1".into(),
            title: "第1章".into(),
        }];
        let mut rd = Reader::new("t".into(), chs);
        rd.lines = (0..50).map(|i| Line::from(format!("行{i:02}"))).collect();
        rd.need_load = false;
        let h = 16u16; // view = 16-4-tail(1) = 11
        let page1 = screen_row_ids(&rd);
        assert_eq!(*page1.first().unwrap(), 0, "第一页应从行00起");
        let last1 = *page1.last().unwrap();
        assert_eq!(last1, 10, "第一页末=行10（view=11 行），实际 {last1}");
        apply_action(Action::PageFlip(1), &mut rd, h);
        assert_eq!(rd.scroll, 11, "翻页步长=view");
        let page2 = screen_row_ids(&rd);
        assert_eq!(*page2.first().unwrap(), 11, "第二页首应紧接行11（不跳行）");
        let expect_end = (*page1.last().unwrap() + 11).min(49);
        assert_eq!(*page2.last().unwrap(), expect_end, "第二页满 view 行");
        // 回翻交界：再翻回第一页，行集回到 0..10
        apply_action(Action::PageFlip(-1), &mut rd, h);
        assert_eq!(rd.scroll, 0);
        let back = screen_row_ids(&rd);
        assert_eq!(*back.last().unwrap(), 10, "回翻第一页末行不丢");
    }

    #[test]
    fn scroll_animate_and_lerp() {
        // 小步（翻页/滚动）→ 滑动；大跳（切章）→ 瞬切
        assert!(scroll_animateable(10, 30, 20)); // 半屏
        assert!(scroll_animateable(0, 40, 20)); // 恰 2 屏
        assert!(!scroll_animateable(0, 41, 20)); // 超 2 屏
        assert!(!scroll_animateable(900, 0, 20)); // 跨章大跳
                                                  // 插值：帧 0=old、frames=new、单调、钳 u16
        assert_eq!(scroll_lerp(10, 30, 0, 3), 10);
        assert_eq!(scroll_lerp(10, 30, 3, 3), 30);
        assert_eq!(scroll_lerp(10, 30, 1, 3), 16); // 10 + 20/3
        assert_eq!(scroll_lerp(30, 10, 1, 3), 24); // 反向：-20/3 向零取整 -6
        assert_eq!(scroll_lerp(0, u16::MAX, 4, 4), u16::MAX);
        // frames=0 防除零 → 视作 1
        assert_eq!(scroll_lerp(0, 9, 0, 0), 0);
    }

    #[test]
    fn resolve_title_fallback() {
        // 详情可用（非纯数字）→ 详情优先
        assert_eq!(
            resolve_title(Some("十日终焉|作者"), Some("别的"), "123"),
            "十日终焉"
        );
        // 详情失败 → 关键词 hint 兜底（搜索进 view 的书名保障）
        assert_eq!(
            resolve_title(None, Some("雪中悍刀行"), "7185478854738185271"),
            "雪中悍刀行"
        );
        // 详情返回纯数字串（脏数据）→ hint
        assert_eq!(
            resolve_title(Some("123456"), Some("夜的命名术"), "999"),
            "夜的命名术"
        );
        // 都没有 → book_id
        assert_eq!(
            resolve_title(None, None, "7185478854738185271"),
            "7185478854738185271"
        );
        assert_eq!(resolve_title(Some("  "), Some("  "), "42"), "42");
    }

    /// 87：e 键、编辑器命令行拆分与来源优先级。
    #[test]
    fn editor_key_argv_and_priority() {
        let rd = Reader::new("t".into(), vec![]);
        assert!(matches!(
            on_key(KeyCode::Char('e'), &rd, 24),
            Action::OpenEditor
        ));
        // vim 系自动插 -R；已有 -R 不重复；非 vim 系不加
        let (b, a) = editor_argv("nvim");
        assert_eq!(b, "nvim");
        assert_eq!(a, vec!["-R"]);
        let (b, a) = editor_argv("vim -u NONE");
        assert_eq!(b, "vim");
        assert_eq!(a, vec!["-R", "-u", "NONE"]);
        let (_, a) = editor_argv("nvim -R");
        assert_eq!(a, vec!["-R"]);
        let (b, a) = editor_argv("micro");
        assert_eq!(b, "micro");
        assert!(a.is_empty());
        let (_, a) = editor_argv("glow");
        assert!(a.is_empty());
        // 优先级：config/flag > VISUAL > EDITOR > 探测；全空 → None
        assert_eq!(
            pick_editor("nvim", Some("vim"), Some("micro"), Some("glow")).as_deref(),
            Some("nvim")
        );
        assert_eq!(
            pick_editor("", Some("vim"), Some("micro"), Some("glow")).as_deref(),
            Some("vim")
        );
        assert_eq!(
            pick_editor("", None, Some("micro"), Some("glow")).as_deref(),
            Some("micro")
        );
        assert_eq!(
            pick_editor("", None, None, Some("glow")).as_deref(),
            Some("glow")
        );
        assert_eq!(pick_editor("", None, None, None), None);
        assert_eq!(pick_editor("  ", Some(" "), None, None), None);
    }
}
