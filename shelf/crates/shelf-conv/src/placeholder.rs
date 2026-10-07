//! 大文件"占位 + 替换"投原生用的占位文档，以及 OPF 书名改写。
//!
//! xochitl 的网页上传接口有约 100MB 的硬上限（超了直接断连，2026-09-19 真机实测）。绕开办法（2026-09-20
//! 真机验证过 PDF 154MB/349 页、EPUB 153MB 都能打开）：先用网页上传一个几 KB 的**占位文档**让 xochitl
//! 建好条目，再由设备上的 book-serve 把磁盘上那个文件原子替换成真文件。
//!
//! 占位必须带**真书的书名和封面**：xochitl 用占位 EPUB 的 `dc:title` 当显示名、在导入时生成 `cover.png`，
//! 替换成大文件后它不会补生成封面、也不会改名（真机踩过：显示成"上传限制实验-EPUB"且没封面）。
//! PDF 的显示名取上传文件名，缩略图打开时才按页生成，占位不需要带内容。

use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// 这本 EPUB 是不是"从右往左"翻页：OPF `<spine page-progression-direction="rtl">`（日漫常见）。只读
/// container.xml 和 OPF 两个条目，不解压整本（漫画一卷可达数百 MB）。读不到/不是 EPUB 一律 `false`。
/// 给 xochitl 阅读器的"日漫从右往左翻页"用（book-serve `GET /reading-direction/{uuid}`，2026-09-24）。
/// 判据见 [`bookconv::direction::spine_direction`]（只读原书自带的方向）。
pub fn epub_is_rtl(epub: &Path) -> bool {
    bookconv::epubzip::open_opf(epub).ok().and_then(|(_, _, opf)| bookconv::direction::spine_direction(&opf)) == Some(bookconv::direction::PageDirection::Rtl)
}

/// 造占位 EPUB：显示名 = `title`（`None` 取真书自己的 `dc:title`），封面 = 真书的封面（找不到就没有封面页，
/// 只有标题）。体积通常几十到几百 KB。
pub fn epub_placeholder(real_epub: &Path, title: Option<&str>) -> Result<Vec<u8>, String> {
    let (_, _, opf) = bookconv::epubzip::open_opf(real_epub)?;
    let cover = bookconv::epubzip::cover_image_of(real_epub);
    static TITLE: OnceLock<Regex> = OnceLock::new();
    // OPF 里读出的是转义过的 XML 文本：`plain_text` 还原字符引用后下面统一转义，否则 `A &amp; B` 会被写成 `A &amp;amp; B`（设备显示名带字面 `&amp;`）。
    let from_opf = |c: &regex::Captures| bookconv::html::plain_text(&c[1]).trim().to_string();
    let real_title = re(&TITLE, r#"(?s)<dc:title\b[^>]*>(.*?)</dc:title>"#).captures(&opf).map(|c| from_opf(&c)).filter(|t| !t.is_empty());
    let title: String = title.map(str::to_string).or(real_title).unwrap_or_else(|| "未命名".into());
    let title = title.as_str();
    static CREATOR: OnceLock<Regex> = OnceLock::new();
    // 作者/语言同样按"读出→还原→转义"处理：此前原样拼进占位 OPF，原书里若是 CDATA、嵌套标签或非法字符，占位 OPF 就不是
    // 合法 XML（xochitl 严格解析，占位导入失败）。
    let creator = re(&CREATOR, r#"(?s)<dc:creator\b[^>]*>(.*?)</dc:creator>"#).captures(&opf).map(|c| from_opf(&c)).unwrap_or_default();
    static LANG: OnceLock<Regex> = OnceLock::new();
    let lang = re(&LANG, r#"(?s)<dc:language\b[^>]*>(.*?)</dc:language>"#).captures(&opf).map(|c| from_opf(&c)).filter(|l| !l.is_empty()).unwrap_or_else(|| "zh".into());
    let (creator, lang) = (bookconv::util::xml_escape(&creator), bookconv::util::xml_escape(&lang));

    let t = bookconv::util::xml_escape(title);
    let (cover_item, cover_meta, page_body) = match &cover {
        Some((ext, _)) => (
            format!(r#"<item id="cover-img" href="cover.{ext}" media-type="{}" properties="cover-image"/>"#, bookconv::util::image_media_type_of_ext(ext)),
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
        let mut z = bookconv::epubzip::EpubWriter::new(std::io::Cursor::new(&mut buf))?;
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

/// 造占位 PDF：一页空白（设备页面尺寸）。显示名取上传文件名，缩略图打开时才按页生成，所以不用带内容。
pub fn pdf_placeholder() -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 14, image::Luma([255])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let img = crate::pdfwrite::image_from_bytes(&png)?;
    crate::pdfwrite::images_to_pdf_with_toc(&[img], &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
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
        z.write_all(format!(r#"<package><metadata><dc:title>Unknown</dc:title><dc:creator>许先哲</dc:creator><dc:language>zh-CN</dc:language>{meta}</metadata><manifest><item id="cv" href="images/cv.jpg" media-type="image/jpeg"/><item id="c1" href="Text/p1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).as_bytes()).unwrap();
        z.start_file("OEBPS/Text/p1.xhtml", o).unwrap();
        z.write_all(br#"<html><body><img src="../images/cv.jpg"/></body></html>"#).unwrap();
        z.start_file("OEBPS/images/cv.jpg", o).unwrap();
        z.write_all(b"\xFF\xD8COVERBYTES\xFF\xD9").unwrap();
        z.finish().unwrap();
        p
    }

    fn entries_of(bytes: &[u8]) -> std::collections::HashMap<String, Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut v = Vec::new(); f.read_to_end(&mut v).unwrap(); (f.name().to_string(), v) }).collect()
    }

    #[test]
    fn placeholder_carries_real_title_author_and_cover_via_meta() {
        let d = tempfile::tempdir().unwrap();
        let out = epub_placeholder(&real_epub(d.path(), true), Some("鏢人 - 02卷")).unwrap();
        let e = entries_of(&out);
        let opf = String::from_utf8_lossy(&e["content.opf"]).to_string();
        assert!(opf.contains("<dc:title>鏢人 - 02卷</dc:title>") && opf.contains("<dc:creator>许先哲</dc:creator>") && opf.contains(r#"properties="cover-image""#));
        assert_eq!(e["cover.jpg"], b"\xFF\xD8COVERBYTES\xFF\xD9", "封面字节必须是真书的封面");
        assert!(out.len() < 5000, "占位应很小: {}", out.len());
        assert!(bookconv::check::check_entries(&bookconv::epubzip::read_entries(&out).unwrap(), true).ok, "占位本身要是合法 EPUB（有目录）");
    }

    #[test]
    fn cover_falls_back_to_first_image_of_first_spine_page() {
        let d = tempfile::tempdir().unwrap();
        let out = epub_placeholder(&real_epub(d.path(), false), Some("书")).unwrap();
        assert_eq!(entries_of(&out)["cover.jpg"], b"\xFF\xD8COVERBYTES\xFF\xD9");
    }

    #[test]
    fn placeholder_defaults_to_real_dc_title_when_none_given() {
        let d = tempfile::tempdir().unwrap();
        let out = epub_placeholder(&real_epub(d.path(), true), None).unwrap();
        assert!(String::from_utf8_lossy(&entries_of(&out)["content.opf"]).contains("<dc:title>Unknown</dc:title>"));
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
        let out = epub_placeholder(&p, Some("t")).unwrap();
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
        let out = epub_placeholder(&p, None).unwrap();
        let opf = String::from_utf8(entries_of(&out)["content.opf"].clone()).unwrap();
        assert!(opf.contains("<dc:title>Tom &amp; Jerry</dc:title>"), "{opf}");
        assert!(opf.contains("<dc:creator>A &lt;B&gt; &amp; C</dc:creator>"), "{opf}");
        assert!(bookconv::check::check_entries(&bookconv::epubzip::read_entries(&out).unwrap(), true).ok);
    }

    #[test]
    fn pdf_placeholder_is_one_valid_page() {
        let pdf = pdf_placeholder().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.pdf");
        std::fs::write(&path, &pdf).unwrap();
        assert_eq!(crate::pdfwrite::PdfFileReader::open(&path).unwrap().page_count().unwrap(), 1);
        assert!(pdf.len() < 4000);
    }

    #[test]
    fn epub_is_rtl_reads_spine_direction_only() {
        let d = tempfile::tempdir().unwrap();
        let mk = |name: &str, spine: &str| {
            let p = d.path().join(name);
            let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            let o: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            z.start_file("META-INF/container.xml", o).unwrap();
            z.write_all(br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();
            z.start_file("OEBPS/content.opf", o).unwrap();
            z.write_all(format!(r#"<package><manifest/>{spine}</package>"#).as_bytes()).unwrap();
            z.finish().unwrap();
            p
        };
        assert!(epub_is_rtl(&mk("r.epub", r#"<spine toc="ncx" page-progression-direction="rtl"><itemref idref="a"/></spine>"#)));
        assert!(!epub_is_rtl(&mk("l.epub", r#"<spine page-progression-direction="ltr"><itemref idref="a"/></spine>"#)));
        assert!(!epub_is_rtl(&mk("n.epub", r#"<spine toc="ncx"><itemref idref="a"/></spine>"#)));
        assert!(!epub_is_rtl(&d.path().join("missing.epub")));
        std::fs::write(d.path().join("bad.epub"), b"not a zip").unwrap();
        assert!(!epub_is_rtl(&d.path().join("bad.epub")));
    }
}
