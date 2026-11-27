//! HTML cleanup shared by the content / full / manga / toutiao endpoints.

use once_cell::sync::Lazy;
use regex::Regex;

static CONTENT_STRIP_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r#"<p class="pictureDesc" group-id="\d+" idx="\d+">"#,
        r"</body>|</html>|</div>",
        r#"<p class="picture" group-id="\d+">"#,
        r#"<div data-fanqie-type="image" source="user">"#,
        r"(?s)<head>.*</h1>",
        r"(?s)<!DOCTYPE.*<html>",
        r"(?s)<\?xml.*\?>",
        r#"<p idx="\d+">"#,
        r"(?s)<header>.*</header>",
        r"<article>|</article>",
        r"<footer>|</footer>",
        r"(?s)<tt_keyword.*keyword_ad>",
        r"<p>",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("strip pattern"))
    .collect()
});

static AMP_X_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"&amp;x").unwrap());
static CLOSE_P_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"</p>").unwrap());

static FULL_BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<\s*br\s*/?>").unwrap());
static FULL_OPEN_P: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<\s*p[^>]*>").unwrap());
static FULL_CLOSE_P: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<\s*/p\s*>").unwrap());
static FULL_ANY_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());
static FULL_MULTI_NL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{2,}").unwrap());
/// img 标签（107⑩）：process 中暂存占位、剥完其他标签后还原——view 图片渲染依赖。
static IMG_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<img\b[^>]*>").unwrap());
/// img 占位符：NUL+序号+NUL，不会出现在正文，且不被任何 strip 模式命中。
fn img_placeholder(n: usize) -> String {
    format!("\u{0}{n}\u{0}")
}

#[derive(Clone, Copy)]
pub struct FullContentProcessor {
    br: &'static Regex,
    open_p: &'static Regex,
    close_p: &'static Regex,
    any_tag: &'static Regex,
    multi_nl: &'static Regex,
}

impl Default for FullContentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FullContentProcessor {
    pub fn new() -> Self {
        FullContentProcessor {
            br: &FULL_BR,
            open_p: &FULL_OPEN_P,
            close_p: &FULL_CLOSE_P,
            any_tag: &FULL_ANY_TAG,
            multi_nl: &FULL_MULTI_NL,
        }
    }

    /// Converts br/p tags to newlines, strips remaining tags, applies the
    /// shared strip patterns, collapses repeated newlines, and trims.
    /// img 标签保留原样（107⑩：先占位、剥完还原；view 需要 img 渲染图片）。
    pub fn process(&self, content: &str) -> String {
        let mut imgs: Vec<String> = Vec::new();
        let mut src = content.to_string();
        let mut n = 0usize;
        while let Some(m) = IMG_TAG.find(&src) {
            let tag = src[m.start()..m.end()].to_string();
            imgs.push(tag);
            let ph = img_placeholder(n);
            src.replace_range(m.start()..m.end(), &ph);
            n += 1;
        }
        let c = self.br.replace_all(&src, "\n");
        let c = self.open_p.replace_all(&c, "\n");
        let c = self.close_p.replace_all(&c, "\n");
        let c = self.any_tag.replace_all(&c, "");
        let mut c = c.into_owned();
        for re in CONTENT_STRIP_PATTERNS.iter() {
            c = re.replace_all(&c, "").into_owned();
        }
        c = AMP_X_RE.replace_all(&c, "&x").into_owned();
        c = self.multi_nl.replace_all(&c, "\n").into_owned();
        let mut c = c.trim().to_string();
        for (i, tag) in imgs.iter().enumerate() {
            c = c.replace(&img_placeholder(i), tag);
        }
        c
    }
}

/// `processFullContent` as a free function.
pub fn process_full_content(content: &str) -> String {
    FullContentProcessor::new().process(content)
}

/// `processContentPatterns`.
pub fn process_content_patterns(content: &str) -> String {
    let mut c = content.to_string();
    for re in CONTENT_STRIP_PATTERNS.iter() {
        c = re.replace_all(&c, "").into_owned();
    }
    c = AMP_X_RE.replace_all(&c, "&x").into_owned();
    c = CLOSE_P_RE.replace_all(&c, "\n").into_owned();
    c.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_keeps_img_strips_other_tags() {
        // 107⑩：img 原样保留，其他标签照剥
        let html = "<p>第一段<img src=\"https://a/1.jpg\" alt=\"x\"/></p><p>第二段</p>";
        let out = FullContentProcessor::new().process(html);
        assert!(
            out.contains("<img src=\"https://a/1.jpg\" alt=\"x\"/>"),
            "img 应保留：{out:?}"
        );
        assert!(!out.contains("<p>"), "p 应剥掉：{out:?}");
        assert!(out.contains("第一段") && out.contains("第二段"));
        // 占位符不残留
        assert!(!out.contains('\u{0}'), "占位应还原干净：{out:?}");
    }

    #[test]
    fn process_img_with_empty_content() {
        let out = FullContentProcessor::new().process("<img src=\"a.jpg\">");
        assert_eq!(out, "<img src=\"a.jpg\">");
    }
}
