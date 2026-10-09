//! 读 EPUB 的几样小事（[`Book`]）：找 OPF、书名/作者/语言、封面图、sheng-ren 的漫画页边距标记，外加写占位 EPUB 的 zip。
//!
//! 2026-10-07 起书架不优化书，只把书原样投进 xochitl，读书只剩这几样；以前借用 sheng-ren `bookconv` 的公开接口，
//! 它连带的图片处理、网页正文抽取、HTTP 客户端等依赖书架一样都用不上，同日改成这里的最小实现（只读 container.xml、
//! OPF 和封面要用的少数几个条目，从不解压整本）。2026-10-09 起 container.xml → OPF、manifest/spine/Dublin Core、href 解码、
//! 标签扫描和有上限地读条目都改用跟笔记线 epubmap 共用的 `epubpkg`（`rmsvc-core/epubpkg`），这里只剩封面挑选、页边距标记
//! 和写 EPUB。
use epubpkg::{attr, dir_of, read_capped, read_entry, read_text, resolve, start_tags, strip_comments, Item, Package};
use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// sheng-ren 优化器写进漫画的标记条目（内容是阅读器该设的页边距，xochitl 模式写 `1`）。
/// **必须与 sheng-ren `bookconv::optimize::READER_MARGINS_MARKER` 一致**——两边各写一份字符串，改名要同时改。
pub const READER_MARGINS_MARKER: &str = "META-INF/eink-reader-margins";

/// 封面图解压后的上限（文本条目的上限是共享的 [`epubpkg::MAX_TEXT_BYTES`] 16MB）。几 KB 的压缩数据能解出几 GB（zip 炸弹），
/// 目录里声明的大小也可以造假，按实际解出的字节数截。上限要远低于 book-serve 的 `MemoryMax=192M`：此前统一 256MB，
/// 一个解出几百 MB 的封面足以让整个服务被 OOM 杀掉、在途操作全丢。真实书里封面几百 KB 到几 MB。
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

type FileZip = ZipArchive<std::io::BufReader<std::fs::File>>;

/// XML 文本/属性转义（`& < > "`），并丢掉 XML 1.0 不允许的字符——转义救不了它们，留着整份文档就不是合法 XML
/// （真机踩过：PDF 标题按错误编码解出一串 `\0`，写进 OPF 后 xochitl 整本只渲染 1 页）。
pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// 图片扩展名（小写，不带点）→ media-type；认不出的当 JPEG。
pub(crate) fn image_media_type_of_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/jpeg",
    }
}

fn is_image_ext(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp"].iter().any(|e| l.ends_with(e))
}

/// 图片路径的扩展名（小写，只看文件名部分）；没有扩展名当 `jpg`。
fn image_ext_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() => e.to_ascii_lowercase(),
        _ => "jpg".into(),
    }
}

/// 打开着的一本 EPUB：zip 中央目录只解一次，书名、封面、页边距标记都从这里取（此前投一本书要把 zip 打开两三次，
/// 图多的漫画中央目录不小）。只读 container.xml、OPF 和需要的少数几个条目，从不解压整本。
pub struct Book {
    zip: FileZip,
    /// 缺 container.xml / OPF 时是 `Err(原因)`——页边距标记照样能读，书名为空，造不了占位。
    opf: Result<Package, String>,
}

impl Book {
    /// 打开并读出 OPF。不是 zip → `Err`；缺 container.xml / OPF 不算错（见 [`Book::opf`]）。
    pub fn open(epub: &Path) -> Result<Book, String> {
        let file = std::fs::File::open(epub).map_err(|e| format!("打开 {} 失败: {e}", epub.display()))?;
        let mut zip = ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("解 EPUB 失败: {e}"))?;
        let opf = Package::read(&mut zip);
        Ok(Book { zip, opf })
    }

    /// OPF 文本（已去注释）；缺 container.xml / OPF → `Err(原因)`。
    pub fn opf(&self) -> Result<&str, String> {
        self.opf.as_ref().map(|p| p.text.as_str()).map_err(String::clone)
    }

    /// OPF 里第一个非空的 Dublin Core 元素（`title`/`creator`/`language`…）的纯文本，见 [`epubpkg::dc_text`]。
    pub fn dc(&self, local: &str) -> Option<String> {
        self.opf.as_ref().ok()?.dc(local)
    }

    /// `dc:title`：xochitl 进库后的显示名取自它。
    pub fn title(&self) -> Option<String> {
        self.dc("title")
    }

    /// 封面图（扩展名, 字节）；找不到 → `None`。见 [`cover_path`]。
    pub fn cover(&mut self) -> Option<(String, Vec<u8>)> {
        let opf = self.opf.as_ref().ok()?;
        let zip = &mut self.zip;
        let path = cover_path(opf, |p| read_text(zip, p))?;
        Some((image_ext_of(&path), read_entry(&mut self.zip, &path, MAX_IMAGE_BYTES).ok()??))
    }

    /// sheng-ren 写在漫画里的页边距（[`READER_MARGINS_MARKER`] 条目的内容）。没有、读不出、不是数字 → `None`。
    pub fn reader_margins(&mut self) -> Option<u32> {
        let entry = self.zip.by_name(READER_MARGINS_MARKER).ok()?;
        let bytes = read_capped(entry, 16, 0).ok()??;
        std::str::from_utf8(&bytes).ok()?.trim().parse().ok()
    }
}

fn is_image_item(it: &Item) -> bool {
    it.media_type.starts_with("image/") || is_image_ext(it.href.split('#').next().unwrap_or(""))
}

/// 封面图的 zip 路径：OPF 声明的封面（`<meta name="cover" content="id">` 指向的图片项，其次 `properties="cover-image"`；
/// 指向 txt 之类的坏声明不算）；没有就取前 12 个 spine 页（跳过导航页）里第一张图。
fn cover_path(opf: &Package, mut read: impl FnMut(&str) -> Option<String>) -> Option<String> {
    let items = opf.manifest();
    let declared = opf
        .meta_contents("cover")
        .iter()
        .find_map(|id| items.iter().find(|i| i.id == Some(*id) && is_image_item(i)))
        .or_else(|| items.iter().find(|i| is_image_item(i) && i.has_property("cover-image")));
    if let Some(it) = declared {
        return Some(opf.item_path(it));
    }
    let by_id: HashMap<&str, &Item> = items.iter().filter_map(|i| Some((i.id?, i))).collect();
    let pages = opf.spine().into_iter().filter_map(|idref| by_id.get(idref).copied()).filter(|it| !it.has_property("nav")).take(12);
    for it in pages {
        let page = opf.item_path(it);
        let Some(text) = read(&page) else { continue };
        let text = strip_comments(&text);
        for tag in start_tags(&text, "img").into_iter().chain(start_tags(&text, "image")) {
            let Some(v) = ["src", "xlink:href", "href"].iter().find_map(|a| attr(tag, a)) else { continue };
            if v.is_empty() || v.starts_with('#') || v.contains("://") || v.starts_with("data:") {
                continue;
            }
            let path = resolve(dir_of(&page), v);
            if is_image_ext(&path) {
                return Some(path);
            }
        }
    }
    None
}

/// 写 EPUB：先写 `mimetype`（第一个条目、STORED，OCF 规范要求），之后图片 STORED、其余 deflate。
pub(crate) struct EpubWriter<W: Write + Seek> {
    zw: ZipWriter<W>,
}

impl<W: Write + Seek> EpubWriter<W> {
    pub(crate) fn new(w: W) -> Result<Self, String> {
        let mut me = EpubWriter { zw: ZipWriter::new(w) };
        me.put_with("mimetype", CompressionMethod::Stored, b"application/epub+zip")?;
        Ok(me)
    }

    pub(crate) fn put(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let m = if is_image_ext(name) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        self.put_with(name, m, data)
    }

    fn put_with(&mut self, name: &str, m: CompressionMethod, data: &[u8]) -> Result<(), String> {
        self.zw.start_file(name, SimpleFileOptions::default().compression_method(m)).map_err(|e| e.to_string())?;
        self.zw.write_all(data).map_err(|e| e.to_string())
    }

    pub(crate) fn finish(self) -> Result<W, String> {
        let mut w = self.zw.finish().map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape() {
        assert_eq!(xml_escape("A & <B> \"q\"\u{0}\u{1}"), "A &amp; &lt;B&gt; &quot;q&quot;");
    }

    /// 解压后超过上限的条目不整份读进内存：OPF 解出超过 [`epubpkg::MAX_TEXT_BYTES`] → 当成读不到 OPF（书名为空、造不了占位），不 OOM。
    #[test]
    fn oversized_entries_are_refused_not_buffered() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bomb.epub");
        let mut z = ZipWriter::new(std::fs::File::create(&p).unwrap());
        let o = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#).unwrap();
        z.start_file("content.opf", o).unwrap();
        z.write_all(b"<package><metadata><dc:title>t</dc:title></metadata>").unwrap();
        let pad = vec![b' '; 1 << 20];
        for _ in 0..=(epubpkg::MAX_TEXT_BYTES >> 20) {
            z.write_all(&pad).unwrap();
        }
        z.write_all(b"</package>").unwrap();
        z.finish().unwrap();
        let book = Book::open(&p).unwrap();
        assert!(book.opf().is_err(), "超限 OPF 不读");
        assert_eq!(book.title(), None);
        let mut zip = ZipArchive::new(std::fs::File::open(&p).unwrap()).unwrap();
        assert!(read_entry(&mut zip, "content.opf", epubpkg::MAX_TEXT_BYTES).unwrap_err().contains("上限"));
        assert!(read_entry(&mut zip, "content.opf", MAX_IMAGE_BYTES).unwrap().is_some(), "上限按调用方给的算");
    }

    /// 封面从 spine 页里找：OPF 在子目录、页面与图片的 href 带实体和百分号编码、导航页跳过。
    #[test]
    fn cover_from_spine_page_with_encoded_hrefs() {
        let opf = Package {
            path: "OEBPS/content.opf".into(),
            text: r#"<manifest><item id="nav" href="nav.xhtml" properties="nav"/><item id="p" href="Text/p&amp;1.xhtml"/></manifest><spine><itemref idref="nav"/><itemref idref="p"/></spine>"#.into(),
        };
        let mut asked = Vec::new();
        let got = cover_path(&opf, |p| {
            asked.push(p.to_string());
            (p == "OEBPS/Text/p&1.xhtml").then(|| r#"<!-- <img src="no.jpg"/> --><img src="data:x"/><svg><image xlink:href="../Images/c%20v.JPG"/></svg>"#.to_string())
        });
        assert_eq!(got.as_deref(), Some("OEBPS/Images/c v.JPG"));
        assert_eq!(asked, ["OEBPS/Text/p&1.xhtml"], "导航页不读");
    }
}
