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
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// 改写 OPF 里的 `<dc:title>`（没有就原样返回）。
pub fn set_opf_title(opf: &str, title: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = re(&R, r#"(?s)(<dc:title\b[^>]*>).*?(</dc:title>)"#);
    let esc = crate::util::xml_escape(title);
    r.replace(opf, |c: &regex::Captures| format!("{}{}{}", &c[1], esc, &c[2])).into_owned()
}

fn attr_of(tag: &str, name: &str) -> Option<String> {
    let r = Regex::new(&format!(r#"\b{name}\s*=\s*"([^"]*)""#)).ok()?;
    r.captures(tag).map(|c| c[1].to_string())
}

fn read_entry(zip: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut v = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

fn join(dir: &str, href: &str) -> String {
    let href = crate::wash::percent_decode(href);
    let mut parts: Vec<&str> = if dir.is_empty() { vec![] } else { dir.split('/').collect() };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// 从真 EPUB 里找封面图：OPF `<meta name="cover">` → manifest；`properties="cover-image"`；都没有就取
/// 第一个 spine 页里的第一张 `<img>`。只读需要的几个条目，不解压整本。
fn find_cover(zip: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>, opf_path: &str, opf: &str) -> Option<(String, Vec<u8>)> {
    let dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let items: Vec<(String, String, String)> = re(&ITEM, r#"(?s)<item\b[^>]*>"#)
        .find_iter(opf)
        .filter_map(|m| {
            let t = m.as_str();
            Some((attr_of(t, "id")?, attr_of(t, "href")?, format!("{} {}", attr_of(t, "properties").unwrap_or_default(), attr_of(t, "media-type").unwrap_or_default())))
        })
        .collect();
    let mut candidate: Option<String> = None;
    static META: OnceLock<Regex> = OnceLock::new();
    if let Some(id) = re(&META, r#"(?s)<meta\b[^>]*\bname\s*=\s*"cover"[^>]*>"#).find(opf).and_then(|m| attr_of(m.as_str(), "content")) {
        candidate = items.iter().find(|(i, _, _)| *i == id).map(|(_, h, _)| h.clone());
    }
    if candidate.is_none() {
        candidate = items.iter().find(|(_, _, p)| p.contains("cover-image")).map(|(_, h, _)| h.clone());
    }
    // 声明必须真指向图片：Calibre 产物常见 `<meta name="cover" content="cover.txt"/>` 指向 txt，直接拿来当封面
    // 会得到一个不是图片的"封面"，xochitl 取不到封面缩略图（2026-09-20 真机日志 `null cover image`）。
    let candidate = candidate.filter(|h| crate::util::is_image_ext(h));
    if candidate.is_none() {
        // 第一个 spine 页里的第一张图。
        static SPINE: OnceLock<Regex> = OnceLock::new();
        let first_ref = re(&SPINE, r#"<itemref\b[^>]*\bidref="([^"]+)""#).captures(opf).map(|c| c[1].to_string());
        if let Some(rid) = first_ref {
            if let Some((_, h, _)) = items.iter().find(|(i, _, _)| *i == rid) {
                let page = join(dir, h);
                if let Some(bytes) = read_entry(zip, &page) {
                    let html = String::from_utf8_lossy(&bytes).to_string();
                    static IMG: OnceLock<Regex> = OnceLock::new();
                    if let Some(c) = re(&IMG, r#"(?is)<(?:img|image)\b[^>]*?(?:src|xlink:href|href)\s*=\s*"([^"]+)""#).captures(&html) {
                        let pdir = page.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();
                        let path = join(&pdir, &c[1]);
                        let ext = path.rsplit('.').next().unwrap_or("jpg").to_lowercase();
                        let data = read_entry(zip, &path)?;
                        return Some((ext, data));
                    }
                }
            }
        }
        return None;
    }
    let path = join(dir, &candidate?);
    let ext = path.rsplit('.').next().unwrap_or("jpg").to_lowercase();
    Some((ext, read_entry(zip, &path)?))
}

/// 读出一本 EPUB 的封面图（扩展名, 字节）：OPF 声明的有效封面，否则第一个 spine 页里的第一张图（同占位构造的规则）。
/// 给"给已有文档补封面缩略图"的小工具用；找不到返回 `None`。
pub fn cover_image_of(epub: &Path) -> Option<(String, Vec<u8>)> {
    let file = std::fs::File::open(epub).ok()?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let container = String::from_utf8_lossy(&read_entry(&mut zip, "META-INF/container.xml")?).to_string();
    let opf_path = attr_of(&container, "full-path")?;
    let opf = String::from_utf8_lossy(&read_entry(&mut zip, &opf_path)?).to_string();
    find_cover(&mut zip, &opf_path, &opf)
}

/// 造占位 EPUB：显示名 = `title`（`None` 取真书自己的 `dc:title`），封面 = 真书的封面（找不到就没有封面页，
/// 只有标题）。体积通常几十到几百 KB。
pub fn epub_placeholder(real_epub: &Path, title: Option<&str>) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(real_epub).map_err(|e| format!("打开 {} 失败: {e}", real_epub.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("解 EPUB 失败: {e}"))?;
    let container = read_entry(&mut zip, "META-INF/container.xml").ok_or("缺 META-INF/container.xml")?;
    let container = String::from_utf8_lossy(&container).to_string();
    let opf_path = attr_of(&container, "full-path").ok_or("container.xml 里没有 full-path")?;
    let opf = String::from_utf8_lossy(&read_entry(&mut zip, &opf_path).ok_or("读不到 OPF")?).to_string();
    let cover = find_cover(&mut zip, &opf_path, &opf);
    static TITLE: OnceLock<Regex> = OnceLock::new();
    let real_title = re(&TITLE, r#"(?s)<dc:title\b[^>]*>(.*?)</dc:title>"#).captures(&opf).map(|c| crate::wash::plain_text(&c[1]).trim().to_string()).filter(|t| !t.is_empty());
    let title: String = title.map(str::to_string).or(real_title).unwrap_or_else(|| "未命名".into());
    let title = title.as_str();
    static CREATOR: OnceLock<Regex> = OnceLock::new();
    let creator = re(&CREATOR, r#"(?s)<dc:creator\b[^>]*>(.*?)</dc:creator>"#).captures(&opf).map(|c| c[1].trim().to_string()).unwrap_or_default();
    static LANG: OnceLock<Regex> = OnceLock::new();
    let lang = re(&LANG, r#"(?s)<dc:language\b[^>]*>(.*?)</dc:language>"#).captures(&opf).map(|c| c[1].trim().to_string()).unwrap_or_else(|| "zh".into());

    let t = crate::util::xml_escape(title);
    let (cover_item, cover_meta, page_body) = match &cover {
        Some((ext, _)) => (
            format!(r#"<item id="cover-img" href="cover.{ext}" media-type="{}" properties="cover-image"/>"#, crate::util::image_media_type_of_ext(ext)),
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
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, data: &[u8], o| -> Result<(), String> {
            z.start_file(name, o).map_err(|e| e.to_string())?;
            z.write_all(data).map_err(|e| e.to_string())
        };
        put("mimetype", b"application/epub+zip", stored)?;
        put("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#, deflated)?;
        put("content.opf", opf_out.as_bytes(), deflated)?;
        put("toc.ncx", ncx.as_bytes(), deflated)?;
        put("c1.xhtml", page.as_bytes(), deflated)?;
        if let Some((ext, data)) = &cover {
            put(&format!("cover.{ext}"), data, stored)?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

/// 造占位 PDF：一页空白（设备页面尺寸）。显示名取上传文件名，缩略图打开时才按页生成，所以不用带内容。
pub fn pdf_placeholder() -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 14, image::Luma([255])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let img = crate::convert::pdfwrite::image_from_bytes(&png)?;
    crate::convert::pdfwrite::images_to_pdf_with_toc(&[img], &[])
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(crate::check::check_epub(&out, true).unwrap().ok, "占位本身要是合法 EPUB（有目录）");
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

    #[test]
    fn set_opf_title_rewrites_and_escapes() {
        let opf = r#"<metadata><dc:title id="t">Unknown</dc:title></metadata>"#;
        assert_eq!(set_opf_title(opf, "A & B - 2卷"), r#"<metadata><dc:title id="t">A &amp; B - 2卷</dc:title></metadata>"#);
        assert_eq!(set_opf_title("<metadata/>", "x"), "<metadata/>");
    }

    #[test]
    fn pdf_placeholder_is_one_valid_page() {
        let pdf = pdf_placeholder().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(crate::convert::pdfwrite::page_count(&pdf).unwrap(), 1);
        assert!(pdf.len() < 4000);
    }
}
