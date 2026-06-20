use crate::types::Chapter;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn stored_opts() -> FileOptions<'static, ()> {
    FileOptions::default().compression_method(CompressionMethod::Stored).unix_permissions(0o644)
}

fn deflated_opts() -> FileOptions<'static, ()> {
    FileOptions::default().compression_method(CompressionMethod::Deflated).unix_permissions(0o644)
}

pub fn generate(title: &str, chapters: &[Chapter], path: &Path) -> Result<(), String> {
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
    for ch in chapters {
        opf.push_str(&format!("\n<item id=\"ch{:04}\" href=\"ch{:04}.xhtml\" media-type=\"application/xhtml+xml\"/>", ch.index, ch.index));
    }
    opf.push_str("\n</manifest>\n<spine>\n<itemref idref=\"toc\"/>");
    for ch in chapters {
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
    for ch in chapters {
        toc.push_str(&format!("\n<li><a href=\"ch{:04}.xhtml\">{}</a></li>", ch.index, xml_esc(&ch_title(ch))));
    }
    toc.push_str("\n</ol></nav></body></html>");
    z.start_file("OEBPS/toc.xhtml", deflated_opts()).map_err(|e| e.to_string())?;
    z.write_all(toc.as_bytes()).map_err(|e| e.to_string())?;

    for ch in chapters {
        let html = format!(
"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\">
<head><title>{}</title></head>
<body><h1>{}</h1><p>待下载</p></body>
</html>", xml_esc(&ch_title(ch)), xml_esc(&ch_title(ch)));
        z.start_file(&format!("OEBPS/ch{:04}.xhtml", ch.index), deflated_opts()).map_err(|e| e.to_string())?;
        z.write_all(html.as_bytes()).map_err(|e| e.to_string())?;
    }

    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn update_chapter(path: &Path, ch: &Chapter, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("epub.tmp");
    let src_f = fs::File::open(path).map_err(|e| format!("打开 EPUB: {}", e))?;
    let mut src = ZipArchive::new(src_f).map_err(|e| format!("读 EPUB: {}", e))?;
    let dst_f = fs::File::create(&tmp).map_err(|e| format!("创建临时: {}", e))?;
    let mut dst = ZipWriter::new(dst_f);

    let html = format!(
"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\">
<head><title>{}</title></head>
<body><h1>{}</h1>
{}</body>
</html>", xml_esc(&ch_title(ch)), xml_esc(&ch_title(ch)), content_to_html(content));

    let target = format!("OEBPS/ch{:04}.xhtml", ch.index);

    for i in 0..src.len() {
        let mut entry = src.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let opts = if name == "mimetype" { stored_opts() } else { deflated_opts() };
        dst.start_file(&name, opts).map_err(|e| e.to_string())?;
        if name == target {
            dst.write_all(html.as_bytes()).map_err(|e| e.to_string())?;
        } else {
            let mut buf = vec![];
            std::io::copy(&mut entry, &mut buf).map_err(|e| e.to_string())?;
            dst.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    dst.finish().map_err(|e| e.to_string())?;
    drop(src);
    fs::rename(&tmp, path).map_err(|e| format!("重命名: {}", e))?;
    Ok(())
}

fn ch_title(ch: &Chapter) -> String {
    if ch.title.starts_with('第') || ch.title.starts_with(|c: char| c.is_ascii_digit()) {
        ch.title.clone()
    } else {
        format!("第{}章 {}", ch.index, ch.title)
    }
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
