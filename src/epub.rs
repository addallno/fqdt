use crate::types::Chapter;
use crate::util;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

fn stored_opts() -> FileOptions<'static, ()> {
    FileOptions::default().compression_method(CompressionMethod::Stored).unix_permissions(0o644)
}

fn deflated_opts() -> FileOptions<'static, ()> {
    FileOptions::default().compression_method(CompressionMethod::Deflated).unix_permissions(0o644)
}

/// 一次性写出完整 EPUB（O(n) 单遍）
///
/// 取代旧的 generate 骨架 + update_chapter 逐章全量重写（O(n²) 写放大）：
/// 章节目录、OPF、TOC 与全部章节正文按顺序写入同一个 ZipWriter，一次落盘。
///
/// # 参数
/// - `items`: (章, 正文) 列表；正文为 `None`（获取失败）时写占位段落
///
/// # 被工作流调用
/// - `download::Downloader::do_epub` — EPUB 模式下载
///
/// # CLI 调用
/// - `fqdt download <book_id> -f epub`
pub fn write_book(title: &str, items: &[(Chapter, Option<String>)], path: &Path) -> Result<(), String> {
    let f = fs::File::create(path).map_err(|e| format!("创建 EPUB: {}", e))?;
    let mut z = ZipWriter::new(f);

    z.start_file("mimetype", stored_opts()).map_err(|e| e.to_string())?;
    z.write_all(b"application/epub+zip").map_err(|e| e.to_string())?;

    z.start_file("META-INF/container.xml", deflated_opts()).map_err(|e| e.to_string())?;
    z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
<rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
</rootfiles></container>").map_err(|e| e.to_string())?;

    let uuid = format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos());
    let now = now_iso();
    let ts = xml_esc(title);
    let mut opf = format!(
"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bid\">
<metadata>
<dc:identifier id=\"bid\">urn:uuid:{uuid}</dc:identifier>
<dc:title>{ts}</dc:title>
<dc:language>zh-CN</dc:language>
<meta property=\"dcterms:modified\">{now}</meta>
</metadata>
<manifest>
<item id=\"toc\" href=\"toc.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>");
    for (ch, _) in items {
        opf.push_str(&format!("\n<item id=\"ch{:04}\" href=\"ch{:04}.xhtml\" media-type=\"application/xhtml+xml\"/>", ch.index, ch.index));
    }
    opf.push_str("\n</manifest>\n<spine>\n<itemref idref=\"toc\"/>");
    for (ch, _) in items {
        opf.push_str(&format!("\n<itemref idref=\"ch{:04}\"/>", ch.index));
    }
    opf.push_str("\n</spine>\n</package>");
    z.start_file("OEBPS/content.opf", deflated_opts()).map_err(|e| e.to_string())?;
    z.write_all(opf.as_bytes()).map_err(|e| e.to_string())?;

    let mut toc = format!(
"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">
<head><title>{ts}</title></head>
<body><nav epub:type=\"toc\"><h1>{ts}</h1><ol>");
    for (ch, _) in items {
        toc.push_str(&format!("\n<li><a href=\"ch{:04}.xhtml\">{}</a></li>", ch.index, xml_esc(&util::chapter_heading(ch))));
    }
    toc.push_str("\n</ol></nav></body></html>");
    z.start_file("OEBPS/toc.xhtml", deflated_opts()).map_err(|e| e.to_string())?;
    z.write_all(toc.as_bytes()).map_err(|e| e.to_string())?;

    for (ch, content) in items {
        let heading = xml_esc(&util::chapter_heading(ch));
        let body = match content {
            Some(text) => content_to_html(text),
            None => String::from("<p>获取失败</p>\n"),
        };
        let html = format!(
"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\">
<head><title>{h}</title></head>
<body><h1>{h}</h1>
{body}</body>
</html>", h = heading, body = body);
        z.start_file(format!("OEBPS/ch{:04}.xhtml", ch.index), deflated_opts()).map_err(|e| e.to_string())?;
        z.write_all(html.as_bytes()).map_err(|e| e.to_string())?;
    }

    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn content_to_html(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() { out.push_str("<br/>\n"); }
        else { out.push_str(&format!("<p>{}</p>\n", xml_esc(t))); }
    }
    out
}

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn now_iso() -> String {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let s = dur.as_secs();
    let days = s / 86400;
    let ts = s % 86400;
    let h = ts / 3600;
    let m = (ts % 3600) / 60;
    let sec = ts % 60;

    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let dy = if lp(y) { 366 } else { 365 };
        if d < dy { break; }
        d -= dy; y += 1;
    }
    let mo = [31, if lp(y){29}else{28}, 31,30,31,30,31,31,30,31,30,31];
    let mut mi = 0;
    for &md in &mo {
        if d < md { break; }
        d -= md; mi += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mi+1, d+1, h, m, sec)
}

fn lp(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }
