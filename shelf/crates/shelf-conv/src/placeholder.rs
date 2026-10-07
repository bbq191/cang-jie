//! 大文件"占位 + 替换"投原生用的占位文档。
//!
//! xochitl 的网页上传接口有约 100MB 的硬上限（超了直接断连，2026-09-19 真机实测）。绕开办法（2026-09-20
//! 真机验证过 PDF 154MB/349 页、EPUB 153MB 都能打开）：先用网页上传一个几 KB 的**占位文档**让 xochitl
//! 建好条目，再由设备上的 book-serve 把磁盘上那个文件原子替换成真文件。
//!
//! 占位必须带**真书的书名和封面**：xochitl 用占位 EPUB 的 `dc:title` 当显示名、在导入时生成 `cover.png`，
//! 替换成大文件后它不会补生成封面、也不会改名（真机踩过：显示成"上传限制实验-EPUB"且没封面）。
//! PDF 的显示名取上传文件名，缩略图打开时才按页生成，占位不需要带内容。

use crate::epub::{self, Book};

/// 造占位 EPUB：显示名 = 真书自己的 `dc:title`（sheng-ren 优化时已写好规范书名），封面 = 真书的封面（找不到就没有
/// 封面页，只有标题）。体积通常几十到几百 KB。
pub fn epub_placeholder(book: &mut Book) -> Result<Vec<u8>, String> {
    book.opf()?; // 读不到 OPF 的书不造占位（调用方整本拒收）
    let cover = book.cover();
    // OPF 里读出的是转义过的 XML 文本：`dc_text` 还原字符引用后下面统一转义，否则 `A &amp; B` 会被写成 `A &amp;amp; B`（设备显示名带字面 `&amp;`）。
    // 作者/语言同样按"读出→还原→转义"处理：原书里若是 CDATA、嵌套标签或非法字符，原样拼进占位 OPF 就不是合法 XML
    // （xochitl 严格解析，占位导入失败）。
    let title = book.title().unwrap_or_else(|| "未命名".into());
    let creator = epub::xml_escape(&book.dc("creator").unwrap_or_default());
    let lang = epub::xml_escape(&book.dc("language").unwrap_or_else(|| "zh".into()));
    let t = epub::xml_escape(&title);
    let (cover_item, cover_meta, page_body) = match &cover {
        Some((ext, _)) => (
            format!(r#"<item id="cover-img" href="cover.{ext}" media-type="{}" properties="cover-image"/>"#, epub::image_media_type_of_ext(ext)),
            r#"<meta name="cover" content="cover-img"/>"#.to_string(),
            format!(r#"<div><img src="cover.{ext}" alt="cover"/></div>"#),
        ),
        None => (String::new(), String::new(), format!("<p>{t}</p>")),
    };
    let creator_xml = if creator.is_empty() { String::new() } else { format!("<dc:creator>{creator}</dc:creator>") };
    let opf_out = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><dc:title>{t}</dc:title>{creator_xml}<dc:language>{lang}</dc:language><dc:identifier id="id">urn:cangjie:placeholder:{}</dc:identifier>{cover_meta}</metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>{cover_item}</manifest><spine toc="ncx"><itemref idref="c1"/></spine></package>"#,
        title.len()
    );
    let ncx = format!(
        r#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head/><docTitle><text>{t}</text></docTitle><navMap><navPoint id="n1" playOrder="1"><navLabel><text>{t}</text></navLabel><content src="c1.xhtml"/></navPoint></navMap></ncx>"#
    );
    let page = format!(r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{t}</title></head><body>{page_body}</body></html>"#);

    let mut buf = Vec::new();
    {
        // mimetype 首个 STORED（`EpubWriter::new` 写）；封面图 STORED，其余 deflate（`EpubWriter::put` 按扩展名选）
        let mut z = epub::EpubWriter::new(std::io::Cursor::new(&mut buf))?;
        z.put("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)?;
        z.put("content.opf", opf_out.as_bytes())?;
        z.put("toc.ncx", ncx.as_bytes())?;
        z.put("c1.xhtml", page.as_bytes())?;
        if let Some((ext, data)) = &cover {
            z.put(&format!("cover.{ext}"), data)?;
        }
        z.finish()?;
    }
    Ok(buf)
}

/// 占位 PDF 的页面尺寸（点）：Move 屏幕 954×1696。
const PDF_PAGE_W: u32 = 954;
const PDF_PAGE_H: u32 = 1696;

/// 造占位 PDF：一页空白（设备页面尺寸）。显示名取上传文件名，缩略图打开时才按页生成，所以不用带内容。
/// 手写最小 PDF（目录 → 页树 → 一页，无内容流），交叉引用表按实际偏移生成。
pub fn pdf_placeholder() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PDF_PAGE_W} {PDF_PAGE_H}] /Resources << >> >>"),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::Path;
    use std::io::Write;

    fn real_epub(dir: &Path, cover_meta: bool) -> std::path::PathBuf {
        let p = dir.join("real.epub");
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("mimetype", o).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();
        let meta = if cover_meta { r#"<meta name="cover" content="cv"/>"# } else { "" };
        z.start_file("OEBPS/content.opf", o).unwrap();
        z.write_all(format!(r#"<package><metadata><dc:title>鏢人 - 02卷</dc:title><dc:creator>许先哲</dc:creator><dc:language>zh-CN</dc:language>{meta}</metadata><manifest><item id="cv" href="images/cv.jpg" media-type="image/jpeg"/><item id="c1" href="Text/p1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).as_bytes()).unwrap();
        z.start_file("OEBPS/Text/p1.xhtml", o).unwrap();
        z.write_all(br#"<html><body><img src="../images/cv.jpg"/></body></html>"#).unwrap();
        z.start_file("OEBPS/images/cv.jpg", o).unwrap();
        z.write_all(b"\xFF\xD8COVERBYTES\xFF\xD9").unwrap();
        z.finish().unwrap();
        p
    }

    /// 占位本身要是合法 EPUB：`mimetype` 第一个且不压缩、container 指向的 OPF 在、manifest 每项都有条目、有 NCX 目录、
    /// OPF 与 NCX 是良构 XML（标签配对）。
    fn assert_valid_epub(bytes: &[u8]) {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let first = z.by_index(0).unwrap();
        assert_eq!((first.name(), first.compression()), ("mimetype", zip::CompressionMethod::Stored));
        drop(first);
        let e = entries_of(bytes);
        assert_eq!(e["mimetype"], b"application/epub+zip");
        let opf = String::from_utf8(e["content.opf"].clone()).unwrap();
        assert!(String::from_utf8_lossy(&e["META-INF/container.xml"]).contains(r#"full-path="content.opf""#));
        for it in opf.split("<item ").skip(1) {
            let href = it.split("href=\"").nth(1).unwrap().split('"').next().unwrap();
            assert!(e.contains_key(href), "manifest 项 {href} 没有对应条目");
        }
        assert!(opf.contains(r#"media-type="application/x-dtbncx+xml""#) && e.contains_key("toc.ncx"), "要有目录");
        for doc in [&opf, &String::from_utf8(e["toc.ncx"].clone()).unwrap(), &String::from_utf8(e["c1.xhtml"].clone()).unwrap()] {
            assert_well_formed(doc);
        }
    }

    /// 极简良构检查：开闭标签按栈配对（自闭合、声明不入栈），文本里不出现裸 `<`/`&`（`&` 必须是实体）。
    fn assert_well_formed(doc: &str) {
        let tag = regex::Regex::new(r"<(/?)([A-Za-z_][-\w.:]*)[^>]*?(/?)>|<\?[^>]*\?>").unwrap();
        let ent = regex::Regex::new(r"&(?:[a-z]+|#[0-9]+|#x[0-9a-fA-F]+);").unwrap();
        let mut stack: Vec<String> = Vec::new();
        let mut last = 0;
        for c in tag.captures_iter(doc) {
            let m = c.get(0).unwrap();
            let text = &doc[last..m.start()];
            assert!(!text.contains('<'), "裸 < : {doc}");
            assert!(ent.replace_all(text, "").find('&').is_none(), "裸 & : {doc}");
            last = m.end();
            let Some(name) = c.get(2) else { continue };
            match (&c[1], &c[3]) {
                ("/", _) => assert_eq!(stack.pop().as_deref(), Some(name.as_str()), "标签不配对: {doc}"),
                (_, "/") => {}
                _ => stack.push(name.as_str().to_string()),
            }
        }
        assert!(stack.is_empty(), "未闭合 {stack:?}: {doc}");
    }

    fn entries_of(bytes: &[u8]) -> std::collections::HashMap<String, Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut v = Vec::new(); f.read_to_end(&mut v).unwrap(); (f.name().to_string(), v) }).collect()
    }

    #[test]
    fn placeholder_carries_real_title_author_and_cover_via_meta() {
        let d = tempfile::tempdir().unwrap();
        let out = epub_placeholder(&mut Book::open(&real_epub(d.path(), true)).unwrap()).unwrap();
        let e = entries_of(&out);
        let opf = String::from_utf8_lossy(&e["content.opf"]).to_string();
        assert!(opf.contains("<dc:title>鏢人 - 02卷</dc:title>") && opf.contains("<dc:creator>许先哲</dc:creator>") && opf.contains(r#"properties="cover-image""#));
        assert_eq!(e["cover.jpg"], b"\xFF\xD8COVERBYTES\xFF\xD9", "封面字节必须是真书的封面");
        assert!(out.len() < 5000, "占位应很小: {}", out.len());
        assert_valid_epub(&out);
    }

    #[test]
    fn cover_falls_back_to_first_image_of_first_spine_page() {
        let d = tempfile::tempdir().unwrap();
        let out = epub_placeholder(&mut Book::open(&real_epub(d.path(), false)).unwrap()).unwrap();
        assert_eq!(entries_of(&out)["cover.jpg"], b"\xFF\xD8COVERBYTES\xFF\xD9");
    }

    #[test]
    fn cover_declared_as_non_image_falls_back_to_first_page_image() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("calibre.epub");
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("mimetype", o).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#).unwrap();
        z.start_file("content.opf", o).unwrap();
        z.write_all(br#"<package><metadata><dc:title>t</dc:title><meta name="cover" content="cover.txt"/></metadata><manifest><item id="cover.txt" href="cover.txt" media-type="text/plain"/><item id="c1" href="p1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).unwrap();
        z.start_file("cover.txt", o).unwrap();
        z.write_all(b"not an image").unwrap();
        z.start_file("p1.xhtml", o).unwrap();
        z.write_all(br#"<html><body><img src="real.jpg"/></body></html>"#).unwrap();
        z.start_file("real.jpg", o).unwrap();
        z.write_all(b"\xFF\xD8REALCOVER\xFF\xD9").unwrap();
        z.finish().unwrap();
        let out = epub_placeholder(&mut Book::open(&p).unwrap()).unwrap();
        let e = entries_of(&out);
        assert_eq!(e["cover.jpg"], b"\xFF\xD8REALCOVER\xFF\xD9", "必须回退到第一页的真图片，而不是 txt");
        assert!(!e.contains_key("cover.txt"));
    }

    /// 书名/作者带 `&` 等：OPF 里是 `&amp;`，占位里必须还是 `&amp;`（此前书名被写成 `&amp;amp;`，设备显示名多出字面 `&amp;`）。
    #[test]
    fn placeholder_does_not_double_escape_title_or_creator() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("amp.epub");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#).unwrap();
        z.start_file("content.opf", o).unwrap();
        z.write_all(br#"<package><metadata><dc:title>Tom &amp; Jerry</dc:title><dc:creator>A &lt;B&gt; &amp; C</dc:creator></metadata><manifest><item id="c1" href="p1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).unwrap();
        z.start_file("p1.xhtml", o).unwrap();
        z.write_all(b"<html><body><p>x</p></body></html>").unwrap();
        z.finish().unwrap();
        let out = epub_placeholder(&mut Book::open(&p).unwrap()).unwrap();
        let opf = String::from_utf8(entries_of(&out)["content.opf"].clone()).unwrap();
        assert!(opf.contains("<dc:title>Tom &amp; Jerry</dc:title>"), "{opf}");
        assert!(opf.contains("<dc:creator>A &lt;B&gt; &amp; C</dc:creator>"), "{opf}");
        assert_valid_epub(&out);
    }

    #[test]
    fn pdf_placeholder_is_one_valid_page() {
        let pdf = pdf_placeholder();
        assert!(pdf.starts_with(b"%PDF"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.pdf");
        std::fs::write(&path, &pdf).unwrap();
        assert_eq!(crate::pdfmeta::page_count(&path).unwrap(), 1);
        assert!(pdf.len() < 4000);
    }
}
