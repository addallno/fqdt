//! TUI 主题（84）：预设主题集 + #RRGGBB 单项覆盖，颜色按部分拆槽。
//!
//! 预设：`default`（现版配色）/ `nord` / `dracula` / `gruvbox`；
//! config.ini `[theme]` 段 `preset` 选预设，六个槽键留空=用预设、
//! 填 `#RRGGBB` 即单项覆盖。未知预设回 default，非法色值忽略。

use ratatui::style::Color as TColor;

/// 主题槽（拍板六键）：边框 / 标题 / 正文 / 目录 / 选中 / 状态。
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeColors {
    pub border: TColor,
    pub title: TColor,
    pub content: TColor,
    pub toc: TColor,
    pub selected: TColor,
    pub status: TColor,
}

/// config 覆盖值（空串 = 用预设）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeOverride {
    pub border: String,
    pub title: String,
    pub content: String,
    pub toc: String,
    pub selected: String,
    pub status: String,
}

/// `#RRGGBB`（可省 #）→ TColor::Rgb；非法返回 None。
pub fn parse_hex(s: &str) -> Option<TColor> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some(TColor::Rgb(r, g, b))
}

/// 预设色板（大小写不敏感；未知回 default）。
pub fn preset_colors(preset: &str) -> ThemeColors {
    let rgb = |r: u8, g: u8, b: u8| TColor::Rgb(r, g, b);
    match preset.to_ascii_lowercase().as_str() {
        "nord" => ThemeColors {
            border: rgb(0x88, 0xC0, 0xD0),
            title: rgb(0x8F, 0xBC, 0xBB),
            content: rgb(0xD8, 0xDE, 0xE9),
            toc: rgb(0xD8, 0xDE, 0xE9),
            selected: rgb(0xB4, 0x8E, 0xAD),
            status: rgb(0xEB, 0xCB, 0x8B),
        },
        "dracula" => ThemeColors {
            border: rgb(0xFF, 0x79, 0xC6),
            title: rgb(0xBD, 0x93, 0xF9),
            content: rgb(0xF8, 0xF8, 0xF2),
            toc: rgb(0xF8, 0xF8, 0xF2),
            selected: rgb(0xFF, 0x79, 0xC6),
            status: rgb(0xF1, 0xFA, 0x8C),
        },
        "gruvbox" => ThemeColors {
            border: rgb(0x83, 0xA5, 0x98),
            title: rgb(0x8E, 0xC0, 0x7C),
            content: rgb(0xEB, 0xDB, 0xB2),
            toc: rgb(0xEB, 0xDB, 0xB2),
            selected: rgb(0xFE, 0x80, 0x19),
            status: rgb(0xFA, 0xBD, 0x2F),
        },
        // default：与现版视觉一致（边框/标题 Cyan、正文/目录 Reset、选中 Magenta、状态 Yellow）
        _ => ThemeColors {
            border: TColor::Cyan,
            title: TColor::Cyan,
            content: TColor::Reset,
            toc: TColor::Reset,
            selected: TColor::Magenta,
            status: TColor::Yellow,
        },
    }
}

/// 预设 + 单项覆盖 → 最终主题（非法覆盖值忽略）。
pub fn resolve(preset: &str, ov: &ThemeOverride) -> ThemeColors {
    let mut c = preset_colors(preset);
    if let Some(x) = parse_hex(&ov.border) {
        c.border = x;
    }
    if let Some(x) = parse_hex(&ov.title) {
        c.title = x;
    }
    if let Some(x) = parse_hex(&ov.content) {
        c.content = x;
    }
    if let Some(x) = parse_hex(&ov.toc) {
        c.toc = x;
    }
    if let Some(x) = parse_hex(&ov.selected) {
        c.selected = x;
    }
    if let Some(x) = parse_hex(&ov.status) {
        c.status = x;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_legacy_palette() {
        let c = resolve("default", &ThemeOverride::default());
        assert_eq!(c.border, TColor::Cyan);
        assert_eq!(c.title, TColor::Cyan);
        assert_eq!(c.content, TColor::Reset);
        assert_eq!(c.toc, TColor::Reset);
        assert_eq!(c.selected, TColor::Magenta);
        assert_eq!(c.status, TColor::Yellow);
        // 未知预设回 default
        assert_eq!(resolve("nope", &ThemeOverride::default()), c);
        assert_eq!(
            resolve("NORD", &ThemeOverride::default()),
            preset_colors("nord")
        );
    }

    #[test]
    fn hex_override_and_invalid() {
        assert_eq!(parse_hex("#5f87af"), Some(TColor::Rgb(0x5f, 0x87, 0xaf)));
        assert_eq!(parse_hex("5f87af"), Some(TColor::Rgb(0x5f, 0x87, 0xaf)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(parse_hex(""), None);
        // 覆盖生效：仅 border 改，其余随预设
        let ov = ThemeOverride {
            border: "#010203".into(),
            ..Default::default()
        };
        let c = resolve("nord", &ov);
        assert_eq!(c.border, TColor::Rgb(1, 2, 3));
        assert_eq!(c.title, preset_colors("nord").title);
        // 非法覆盖忽略
        let bad = ThemeOverride {
            border: "nope".into(),
            ..Default::default()
        };
        assert_eq!(resolve("default", &bad).border, TColor::Cyan);
    }
}
