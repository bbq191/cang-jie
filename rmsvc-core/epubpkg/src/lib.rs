//! epubpkg —— EPUB 容器 / OPF 的最小只读解析，笔记线（`notes/crates/epubmap`：找目录文件）与书架（`shelf/crates/shelf-conv`：
//! 书名、封面）共用。2026-10-09 从两边各自的实现合并而来，取两者里更严格/更正确的那份：
//!
//! - [`escape`]：href 两层解码（XML 实体 → 去 `#片段` → 百分号），只认 XML 预定义实体与数字引用；
//! - [`xml`]：正则标签扫描（去注释、三种引号、命名空间前缀、不分大小写），[`xml::plain_text`] 把元素内容变纯文本；
//! - 本模块：zip 内路径（[`resolve`]、[`posix_norm`]、[`dir_of`]）、有上限地读条目（[`read_entry`]、[`read_text`]）、
//!   `container.xml` → OPF（[`Package::read`]）、manifest / spine / Dublin Core（[`Package::manifest`]、[`Package::spine`]、[`dc_text`]）；
//! - [`epubindex`]：xochitl 的 `<uuid>.epubindex`（各 spine 文件起始页），2026-10-09 从 epubmap 下沉，书架找回阅读位置也用。
//!
//! 只读 container.xml、OPF 和调用方点名的少数几个条目，从不解压整本。写 EPUB、封面挑选、目录结构这些各自的业务留在各自 crate。
//! 放在 `rmsvc-core/` 目录下但**不依赖** rmsvc-core：解析库不该连带编进 HTTP/TLS/证书那一整套。
pub mod epubindex;
pub mod escape;
pub mod xml;

pub use escape::{href_path, percent_decode, xml_unescape};
pub use xml::{attr, plain_text, start_tags, strip_comments};

use regex::Regex;
use std::io::{Read, Seek};
use std::sync::OnceLock;
use zip::ZipArchive;

/// 文本条目（container.xml、OPF、nav/NCX、正文页）解压后的上限。几 KB 的压缩数据能解出几 GB（zip 炸弹），目录里声明的
/// 大小也可以造假，按实际解出的字节数截；超限当读不到（不截断着用：截出半份 OPF/目录只会解析出错的东西）。上限要远低于
/// 消费方服务的 MemoryMax（book-serve 192M、ink-serve 128M）：文本读进来后正则扫描还要再拷一两份。真实书里 OPF 几 KB 到
/// 一两 MB。
pub const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;

/// zip 内 posix 路径：去掉 `.`、空段，`..` 回退一级（越过根的 `..` 丢掉）。
pub fn posix_norm(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// 路径所在目录（不带结尾 `/`；根下的文件 → `""`）。
pub fn dir_of(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

/// 链接原文（相对 `base_dir`，可能带 `#锚点`、XML 实体、百分号编码、`./`、`../`）→ zip 条目路径。`base_dir` 结尾带不带 `/` 都行。
pub fn resolve(base_dir: &str, href: &str) -> String {
    let p = href_path(href);
    if base_dir.is_empty() {
        posix_norm(&p)
    } else {
        posix_norm(&format!("{base_dir}/{p}"))
    }
}

/// 读一个条目的全部字节，解出超过 `cap` 字节返回 `Ok(None)`（预分配按声明大小，封顶 32MB）。
pub fn read_capped(r: impl Read, cap: u64, declared: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::with_capacity(declared.min(cap).min(32 << 20) as usize);
    r.take(cap + 1).read_to_end(&mut buf)?;
    Ok((buf.len() as u64 <= cap).then_some(buf))
}

/// 按名读条目；不存在 → `Ok(None)`，解出超过 `cap` 字节或读失败 → `Err(原因)`。
pub fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str, cap: u64) -> Result<Option<Vec<u8>>, String> {
    let f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let size = f.size();
    read_capped(f, cap, size)
        .map_err(|e| format!("{name}: {e}"))?
        .map(Some)
        .ok_or_else(|| format!("{name}: 解压后超过单个条目上限 {} MB（损坏或恶意的压缩包？）", cap >> 20))
}

/// 按名读文本条目（上限 [`MAX_TEXT_BYTES`]）：不存在、超限、读失败 → `None`；不是合法 UTF-8 的字节按替换字符读（不整份丢）。
pub fn read_text<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Option<String> {
    read_entry(zip, name, MAX_TEXT_BYTES).ok().flatten().map(|b| String::from_utf8(b).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// OPF（package 文档）。
#[derive(Debug, Clone)]
pub struct Package {
    /// OPF 在 zip 里的路径（已解码、归一）。
    pub path: String,
    /// OPF 文本，**已去掉注释**（注释里的 item/meta 不算数）。
    pub text: String,
}

/// manifest 里的一项（属性都是原文，未解实体；路径用 [`Item::path`]）。没有 `href` 的项不收。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item<'a> {
    pub id: Option<&'a str>,
    pub href: &'a str,
    pub media_type: &'a str,
    pub properties: &'a str,
}

impl Item<'_> {
    /// `properties` 里有没有这一项（空白分隔，如 `nav`、`cover-image`）。
    pub fn has_property(&self, p: &str) -> bool {
        self.properties.split_whitespace().any(|x| x == p)
    }

    /// 是不是 NCX（`application/x-dtbncx+xml`，不分大小写）。
    pub fn is_ncx(&self) -> bool {
        self.media_type.eq_ignore_ascii_case("application/x-dtbncx+xml")
    }
}

impl Package {
    /// `META-INF/container.xml` 第一个带 `full-path` 的 `<rootfile>` → 读出 OPF。缺 container.xml、没有 full-path、
    /// 读不到 OPF（不存在 / 解压超过 [`MAX_TEXT_BYTES`]）→ `Err(原因)`。
    pub fn read<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<Package, String> {
        let container = read_text(zip, "META-INF/container.xml").ok_or("缺 META-INF/container.xml")?;
        let container = strip_comments(&container);
        let opf = start_tags(&container, "rootfile").into_iter().find_map(|t| attr(t, "full-path")).ok_or("container.xml 里没有 full-path")?;
        let path = resolve("", opf);
        let text = read_text(zip, &path).ok_or("读不到 OPF")?;
        Ok(Package { text: strip_comments(&text).into_owned(), path })
    }

    /// OPF 所在目录（manifest 的 href 相对它）。
    pub fn dir(&self) -> &str {
        dir_of(&self.path)
    }

    /// manifest 项的 zip 路径（href 相对 OPF 目录解码、归一）。
    pub fn item_path(&self, item: &Item) -> String {
        resolve(self.dir(), item.href)
    }

    /// manifest 全部项，文档序。
    pub fn manifest(&self) -> Vec<Item<'_>> {
        manifest_items(&self.text)
    }

    /// spine 的 `idref` 序列（原文），文档序。
    pub fn spine(&self) -> Vec<&str> {
        start_tags(&self.text, "itemref").into_iter().filter_map(|t| attr(t, "idref")).collect()
    }

    /// `<meta name="…" content="…">`（EPUB 2 写法，如 `name="cover"`）的 content 原文，文档序。
    pub fn meta_contents(&self, name: &str) -> Vec<&str> {
        start_tags(&self.text, "meta").into_iter().filter(|t| attr(t, "name") == Some(name)).filter_map(|t| attr(t, "content")).collect()
    }

    /// 第一个非空的 Dublin Core 元素的纯文本，见 [`dc_text`]。
    pub fn dc(&self, local: &str) -> Option<String> {
        dc_text(&self.text, local)
    }
}

/// OPF 文本里 manifest 的 `<item>`（认任意前缀；调用方先 [`strip_comments`]，[`Package::text`] 已去过）。
pub fn manifest_items(opf: &str) -> Vec<Item<'_>> {
    start_tags(opf, "item")
        .into_iter()
        .filter_map(|t| Some(Item { id: attr(t, "id"), href: attr(t, "href")?, media_type: attr(t, "media-type").unwrap_or(""), properties: attr(t, "properties").unwrap_or("") }))
        .collect()
}

/// OPF 里第一个非空的 Dublin Core 元素（`title`/`creator`/`language`…，认任意前缀）的纯文本（[`plain_text`]）。
pub fn dc_text(opf: &str, local: &str) -> Option<String> {
    static DC: OnceLock<Regex> = OnceLock::new();
    let opf = strip_comments(opf);
    xml::re(&DC, r"(?s)<([A-Za-z_][-\w.]*):([A-Za-z_][-\w.]*)\b[^>]*?(?:/>|>(.*?)</[A-Za-z_][-\w.]*:[A-Za-z_][-\w.]*\s*>)")
        .captures_iter(&opf)
        .filter(|c| c[2].eq_ignore_ascii_case(local))
        .filter_map(|c| c.get(3).map(|m| plain_text(m.as_str())))
        .find(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn zip_of(files: &[(&str, &[u8])]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = ZipWriter::new(&mut buf);
            let o = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, body) in files {
                w.start_file(*name, o).unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        ZipArchive::new(buf).unwrap()
    }

    #[test]
    fn resolve_paths() {
        assert_eq!(resolve("OEBPS/Text", "../images/a%20b.jpg#x"), "OEBPS/images/a b.jpg");
        assert_eq!(resolve("OEBPS/Text/", "../images/a%20b.jpg"), "OEBPS/images/a b.jpg", "结尾带 / 的目录一样");
        assert_eq!(resolve("", "./content.opf"), "content.opf");
        assert_eq!(resolve("OEBPS", "a&amp;b.xhtml"), "OEBPS/a&b.xhtml");
        assert_eq!(resolve("O&P", "t&#x2E;ncx"), "O&P/t.ncx");
        assert_eq!(resolve("a", "../../x.xhtml"), "x.xhtml", "越过根的 .. 丢掉");
        assert_eq!(dir_of("OEBPS/content.opf"), "OEBPS");
        assert_eq!(dir_of("content.opf"), "");
    }

    #[test]
    fn package_in_subdir_with_encoded_paths() {
        let container = br#"<?xml version="1.0"?><container><rootfiles><!-- <rootfile full-path="old.opf"/> --><rootfile full-path='O%20E/P&amp;Q/content.opf' media-type=application/oebps-package+xml/></rootfiles></container>"#;
        let opf = br#"<package><metadata><dc:title>Tom &amp; Jerry</dc:title><meta name="cover" content="img"/></metadata>
            <manifest><!-- <item id="dead" href="dead.xhtml"/> --><opf:item id="c1" href="a&amp;b.xhtml" media-type="application/xhtml+xml"/>
            <item id="img" href="../img/c%20v.jpg" media-type="image/jpeg" properties="cover-image"/><item href="nav.xhtml" properties="nav scripted"/>
            <item id="n" href="toc.ncx" media-type="Application/X-DTBNCX+XML"/></manifest><spine toc="n"><itemref idref="c1"/><opf:itemref idref='img' linear="no"/></spine></package>"#;
        let mut zip = zip_of(&[("META-INF/container.xml", container), ("O E/P&Q/content.opf", opf)]);
        let p = Package::read(&mut zip).unwrap();
        assert_eq!(p.path, "O E/P&Q/content.opf");
        assert_eq!(p.dir(), "O E/P&Q");
        assert!(!p.text.contains("dead"), "OPF 已去注释");
        let items = p.manifest();
        assert_eq!(items.iter().map(|i| p.item_path(i)).collect::<Vec<_>>(), ["O E/P&Q/a&b.xhtml", "O E/img/c v.jpg", "O E/P&Q/nav.xhtml", "O E/P&Q/toc.ncx"]);
        assert_eq!(items[2].id, None, "没有 id 的项照收");
        assert!(items[2].has_property("nav") && !items[2].has_property("nav-x"));
        assert!(items[3].is_ncx());
        assert_eq!(p.spine(), ["c1", "img"]);
        assert_eq!(p.meta_contents("cover"), ["img"]);
        assert_eq!(p.dc("title").as_deref(), Some("Tom & Jerry"));
    }

    #[test]
    fn missing_container_or_opf_is_err() {
        let mut zip = zip_of(&[("OEBPS/content.opf", b"<package/>")]);
        assert!(Package::read(&mut zip).unwrap_err().contains("container.xml"));
        let mut zip = zip_of(&[("META-INF/container.xml", b"<container><rootfile/></container>")]);
        assert!(Package::read(&mut zip).unwrap_err().contains("full-path"));
        let mut zip = zip_of(&[("META-INF/container.xml", br#"<container><rootfile full-path="x.opf"/></container>"#)]);
        assert_eq!(Package::read(&mut zip).unwrap_err(), "读不到 OPF");
    }

    #[test]
    fn dc_text_decodes_and_skips_empty() {
        let opf = r#"<metadata><dc:title/><dc:title>  </dc:title><dc:title>Tom &amp; <i>Jerry</i> &#20013;</dc:title><dc11:creator><![CDATA[A & B]]></dc11:creator></metadata>"#;
        assert_eq!(dc_text(opf, "title").as_deref(), Some("Tom & Jerry 中"));
        assert_eq!(dc_text(opf, "creator").as_deref(), Some("A & B"));
        assert_eq!(dc_text(opf, "language"), None);
    }

    /// 解压后超过上限的条目不整份读进内存，也不截断着用：OPF 解出超过 [`MAX_TEXT_BYTES`] → 当成读不到 OPF。
    #[test]
    fn oversized_entries_are_refused_not_truncated() {
        let mut big = b"<package><metadata><dc:title>t</dc:title></metadata>".to_vec();
        big.resize(MAX_TEXT_BYTES as usize + 1, b' ');
        let mut zip = zip_of(&[("META-INF/container.xml", br#"<container><rootfile full-path="content.opf"/></container>"#), ("content.opf", &big)]);
        assert_eq!(Package::read(&mut zip).unwrap_err(), "读不到 OPF");
        assert!(read_text(&mut zip, "content.opf").is_none());
        assert!(read_entry(&mut zip, "content.opf", MAX_TEXT_BYTES).unwrap_err().contains("上限"));
        assert!(read_entry(&mut zip, "content.opf", MAX_TEXT_BYTES + 1).unwrap().is_some(), "上限按调用方给的算");
        assert_eq!(read_entry(&mut zip, "nope", 1).unwrap(), None);
    }

    #[test]
    fn non_utf8_text_is_read_lossily() {
        let mut zip = zip_of(&[("a.xhtml", b"ok\xFFok")]);
        assert_eq!(read_text(&mut zip, "a.xhtml").as_deref(), Some("ok\u{FFFD}ok"));
    }
}
