//! epubmap —— 设备 EPUB 的「页号 → 章 / 小节」映射（笔记线用来给每条勾画标章节、按章生成笔记本）。
//!
//! 两份数据源合一：
//! - **`<uuid>.epubindex`**（xochitl 导入时生成的二进制）：记录每个 spine 文件的**起始页**（[`index`]，格式事实剥离自早期
//!   设备端共享底座（已不在本仓库）的逆向结论，3.28 真机再核：两张表、第一张的第一个 u32 是起始页）；
//! - **EPUB 自身目录**（[`toc`]：优先 `nav.xhtml` 的嵌套 `<ol>`，退回 `toc.ncx` 的嵌套 `navPoint`），统一成带层级的
//!   [`toc::TocEntry`] 列表。
//!
//! [`BookMap`] 把两者合起来：`chapter_of(page)` 给出该页所属的**章**（1 级祖先）与**小节**（本条目若 ≥2 级）。
//! 页号 0-based（封面=0），与 `.rm` 页文件在 `.content` `pages` 里的位置一致。
pub mod index;
pub mod toc;

pub use index::{parse_epubindex, Section};
pub use toc::{Toc, TocEntry};

use std::io::{Cursor, Read, Seek};

/// 某页所属的章节标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter<'a> {
    /// 1 级条目在目录里的序号（0-based；note-serve 用它编「第 N 章」文件名与顺序）。
    pub index: usize,
    /// 章标题（本条目是 1 级就是自己，否则是它的 1 级祖先）。
    pub title: &'a str,
    /// 小节标题（本条目 ≥2 级才有）。
    pub subhead: Option<&'a str>,
}

#[derive(Debug, Default)]
pub struct BookMap {
    pub sections: Vec<Section>,
    pub toc: Toc,
}

impl BookMap {
    pub fn new(sections: Vec<Section>, toc: Toc) -> BookMap {
        BookMap { sections, toc }
    }

    /// 从设备上的 `.epub` 字节 + `.epubindex` 字节直接建表（找不到目录 → toc 为空，只剩 section 粒度）。
    pub fn from_epub(epub: &[u8], epubindex: &[u8]) -> BookMap {
        BookMap::from_epub_reader(Cursor::new(epub), epubindex)
    }

    /// 同 [`Self::from_epub`]，但直接吃可 seek 的读端（如打开的 `.epub` 文件）：zip 只读中央目录和目录那一两个条目，
    /// 不必把整本书读进内存——大书（上传绕过 100MB 限制的那类）整本 `fs::read` 会顶破服务的 MemoryMax（2026-09-24）。
    pub fn from_epub_reader<R: Read + Seek>(epub: R, epubindex: &[u8]) -> BookMap {
        let (nav, ncx) = read_toc_texts_from(epub);
        BookMap::new(parse_epubindex(epubindex), Toc::parse(nav.as_deref(), ncx.as_deref()))
    }

    /// 该页所在的 spine 文件（起始页 ≤ page 的最后一条）。
    pub fn section_of(&self, page: usize) -> Option<&Section> {
        self.sections.iter().take_while(|s| (s.start_page as usize) <= page).last()
    }

    /// **章 = 没有父条目的顶层条目**（通常就是 1 级）。此前 `chapters()` 只收 1 级、`chapter_of` 却取顶层祖先：目录里
    /// 分组项是 `<span>`（没有链接）时，二级条目没有父条目，它的章号按"前面有几个 1 级"算、标题却是它自己——跟
    /// `chapters()` 对不上（下标越界或撞上别的章），这些条目在两处投影里找不到自己的章，永远生成/导出不出来
    /// （2026-09-25 第四轮审计）。两处现在用同一个判据；标准嵌套目录（每个二级都有 1 级父条目）结果不变。
    ///
    /// 本页所在的 spine 文件**不在目录里**时，往前找最近一个在目录里的 spine 文件，归到它那一章（2026-09-29）：书架优化的
    /// 章节分页把一章拆成多个文件，章标题独占一份、正文另起一份，正文那份没有自己的目录条目；Calibre 按大小切出来的
    /// `index_split_NNN` 同理。此前这些页一律没有章。封面等第一个目录条目之前的页仍是 `None`。
    pub fn chapter_of(&self, page: usize) -> Option<Chapter<'_>> {
        let upto = self.sections.iter().take_while(|s| (s.start_page as usize) <= page).count();
        let i = self.sections[..upto].iter().rev().find_map(|sec| self.toc.entries.iter().position(|e| e.file == sec.file))?;
        let top = self.toc.top_ancestor(i);
        let index = self.toc.entries.iter().take(top).filter(|e| e.parent.is_none()).count();
        Some(Chapter { index, title: &self.toc.entries[top].title, subhead: (i != top).then_some(self.toc.entries[i].title.as_str()) })
    }

    /// 全书章列表 [(index, title)]（顶层条目，见 [`Self::chapter_of`]）。
    pub fn chapters(&self) -> Vec<(usize, &str)> {
        self.toc.entries.iter().filter(|e| e.parent.is_none()).enumerate().map(|(i, e)| (i, e.title.as_str())).collect()
    }
}

/// 在 EPUB zip 里找 (nav.xhtml, toc.ncx) 文本；缺的为 None。
pub fn read_toc_texts(epub: &[u8]) -> (Option<String>, Option<String>) {
    read_toc_texts_from(Cursor::new(epub))
}

/// 同 [`read_toc_texts`]，吃可 seek 的读端。
///
/// 目录文件**先按 OPF 声明找**（`container.xml` → OPF → manifest 里 `properties` 含 `nav` 的条目 / NCX 媒体类型的条目），
/// 找不到再退回按文件名猜（2026-09-30 第五轮审计）：nav 文档不一定叫 `nav.xhtml`（书架洗书那边记过叫 `toc.xhtml`、
/// 又没有 NCX 的书——此前这类书整本没有章，条目两处投影都不收）；反过来"文件名含 nav 的 xhtml"也可能是正文
/// （`canvas.xhtml` 之类），按 zip 顺序先撞上它就会拿正文当目录。
pub fn read_toc_texts_from<R: Read + Seek>(epub: R) -> (Option<String>, Option<String>) {
    let Ok(mut ar) = zip::ZipArchive::new(epub) else { return (None, None) };
    let (mut nav_name, mut ncx_name) = opf_toc_names(&mut ar);
    let (mut guess_nav, mut guess_ncx) = (None, None);
    for i in 0..ar.len() {
        let Ok(f) = ar.by_index(i) else { continue };
        let n = f.name().to_string();
        let l = n.to_ascii_lowercase();
        if l.ends_with(".ncx") {
            guess_ncx.get_or_insert(n);
        } else if l.rsplit('/').next().is_some_and(|b| b == "nav.xhtml") {
            guess_nav = Some(n); // 正好叫 nav.xhtml 的优先于"名字里含 nav"的
        } else if l.contains("nav") && l.ends_with(".xhtml") {
            guess_nav.get_or_insert(n);
        }
    }
    let exists = |ar: &mut zip::ZipArchive<R>, n: &Option<String>| n.as_deref().is_some_and(|n| ar.by_name(n).is_ok());
    if !exists(&mut ar, &nav_name) {
        nav_name = guess_nav;
    }
    if !exists(&mut ar, &ncx_name) {
        ncx_name = guess_ncx;
    }
    let nav = read_capped(&mut ar, nav_name);
    let ncx = read_capped(&mut ar, ncx_name);
    (nav, ncx)
}

/// 读 zip 里一个文本条目。设读取上限：解压后的大小由 zip 自己声明，坏书/恶意书可以声称极大（笔记服务 MemoryMax=128M）。
fn read_capped<R: Read + Seek>(ar: &mut zip::ZipArchive<R>, name: Option<String>) -> Option<String> {
    const TOC_MAX: u64 = 16 << 20;
    let mut s = String::new();
    ar.by_name(&name?).ok()?.take(TOC_MAX).read_to_string(&mut s).ok()?;
    Some(s)
}

/// OPF 声明的 (nav 文档, NCX) 在 zip 里的路径；没有 `container.xml`/OPF、或没声明的一项为 None。
fn opf_toc_names<R: Read + Seek>(ar: &mut zip::ZipArchive<R>) -> (Option<String>, Option<String>) {
    use regex::Regex;
    use std::sync::OnceLock;
    static ROOT: OnceLock<Regex> = OnceLock::new();
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static ATTR: OnceLock<Regex> = OnceLock::new();
    let root = ROOT.get_or_init(|| Regex::new(r#"(?i)<rootfile\b[^>]*\bfull-path\s*=\s*["']([^"']+)["']"#).unwrap());
    let item = ITEM.get_or_init(|| Regex::new(r"(?is)<(?:opf:)?item\b[^>]*>").unwrap());
    let attr = ATTR.get_or_init(|| Regex::new(r#"(?s)([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());
    let Some(container) = read_capped(ar, Some("META-INF/container.xml".into())) else { return (None, None) };
    let Some(opf_path) = root.captures(&container).map(|c| c[1].to_string()) else { return (None, None) };
    let Some(opf) = read_capped(ar, Some(opf_path.clone())) else { return (None, None) };
    let dir = opf_path.rfind('/').map(|i| &opf_path[..=i]).unwrap_or("");
    let (mut nav, mut ncx) = (None, None);
    for m in item.find_iter(&opf) {
        let (mut href, mut props, mut media) = (None, "", "");
        for c in attr.captures_iter(m.as_str()) {
            let v = c.get(2).or_else(|| c.get(3)).map_or("", |v| v.as_str());
            match c[1].to_ascii_lowercase().as_str() {
                "href" => href = Some(v),
                "properties" => props = v,
                "media-type" => media = v,
                _ => {}
            }
        }
        let Some(href) = href else { continue };
        let path = || resolve_href(dir, href);
        if nav.is_none() && props.split_whitespace().any(|p| p == "nav") {
            nav = Some(path());
        } else if ncx.is_none() && media.eq_ignore_ascii_case("application/x-dtbncx+xml") {
            ncx = Some(path());
        }
    }
    (nav, ncx)
}

/// manifest 的 href（相对 OPF 所在目录、可能百分号编码、可能带 `./`/`../`）→ zip 条目路径。
fn resolve_href(dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let decoded = percent_decode(href);
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in decoded.split('/') {
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

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAV_NESTED: &str = r#"<nav epub:type="toc"><ol>
      <li><a href="chap_0002.xhtml">Dedication</a></li>
      <li><a href="chap_0003.xhtml">Book One</a>
        <ol><li><a href="chap_0004.xhtml">Chapter One</a></li><li><a href="chap_0005.xhtml">Chapter Two</a></li></ol>
      </li>
      <li><a href="chap_0014.xhtml">Book Two</a>
        <ol><li><a href="chap_0015.xhtml">Chapter Eleven</a></li></ol>
      </li>
    </ol></nav>"#;

    #[test]
    fn nested_nav_gives_chapter_and_subhead() {
        let secs = vec![
            Section { file: "chap_0002.xhtml".into(), start_page: 1 },
            Section { file: "chap_0003.xhtml".into(), start_page: 5 },
            Section { file: "chap_0004.xhtml".into(), start_page: 6 },
            Section { file: "chap_0005.xhtml".into(), start_page: 20 },
            Section { file: "chap_0014.xhtml".into(), start_page: 40 },
            Section { file: "chap_0015.xhtml".into(), start_page: 41 },
        ];
        let m = BookMap::new(secs, Toc::parse(Some(NAV_NESTED), None));
        assert_eq!(m.chapter_of(0), None, "封面页无 section");
        assert_eq!(m.chapter_of(2), Some(Chapter { index: 0, title: "Dedication", subhead: None }));
        assert_eq!(m.chapter_of(5), Some(Chapter { index: 1, title: "Book One", subhead: None }));
        assert_eq!(m.chapter_of(10), Some(Chapter { index: 1, title: "Book One", subhead: Some("Chapter One") }));
        assert_eq!(m.chapter_of(25), Some(Chapter { index: 1, title: "Book One", subhead: Some("Chapter Two") }));
        assert_eq!(m.chapter_of(99), Some(Chapter { index: 2, title: "Book Two", subhead: Some("Chapter Eleven") }));
        assert_eq!(m.chapters(), vec![(0, "Dedication"), (1, "Book One"), (2, "Book Two")]);
    }

    /// 不在目录里的 spine 文件（章节分页拆出来的正文份、Calibre 按大小切的份）归到前面最近一个在目录里的文件那一章。
    #[test]
    fn untocced_split_files_belong_to_preceding_chapter() {
        let nav = r#"<ol><li><a href="c1.xhtml">第一章</a><ol><li><a href="c1_2.xhtml#s2">二</a></li></ol></li><li><a href="c2.xhtml">第二章</a></li></ol>"#;
        let secs = vec![
            Section { file: "cover.xhtml".into(), start_page: 0 },
            Section { file: "c1.xhtml".into(), start_page: 1 },
            Section { file: "c1_1.xhtml".into(), start_page: 2 },
            Section { file: "c1_2.xhtml".into(), start_page: 6 },
            Section { file: "c1_3.xhtml".into(), start_page: 9 },
            Section { file: "c2.xhtml".into(), start_page: 12 },
            Section { file: "c2_1.xhtml".into(), start_page: 13 },
        ];
        let m = BookMap::new(secs, Toc::parse(Some(nav), None));
        assert_eq!(m.chapter_of(0), None, "第一个目录条目之前的页没有章");
        assert_eq!(m.chapter_of(1), Some(Chapter { index: 0, title: "第一章", subhead: None }));
        assert_eq!(m.chapter_of(3), Some(Chapter { index: 0, title: "第一章", subhead: None }), "章标题页后面的正文份");
        assert_eq!(m.chapter_of(7), Some(Chapter { index: 0, title: "第一章", subhead: Some("二") }));
        assert_eq!(m.chapter_of(10), Some(Chapter { index: 0, title: "第一章", subhead: Some("二") }), "节后面没进目录的份跟着这一节");
        assert_eq!(m.chapter_of(14), Some(Chapter { index: 1, title: "第二章", subhead: None }));
    }

    /// 回归：分组项是 `<span>` 的目录，章号与 `chapters()` 必须对得上（此前越界，条目永远投影不出去）。
    #[test]
    fn chapter_index_matches_chapters_when_groups_have_no_link() {
        let nav = r#"<ol><li><a href="pre.xhtml">前言</a></li><li><span>第一部</span><ol><li><a href="c1.xhtml">第一章</a></li><li><a href="c2.xhtml">第二章</a></li></ol></li></ol>"#;
        let secs = vec![Section { file: "pre.xhtml".into(), start_page: 1 }, Section { file: "c1.xhtml".into(), start_page: 3 }, Section { file: "c2.xhtml".into(), start_page: 9 }];
        let m = BookMap::new(secs, Toc::parse(Some(nav), None));
        assert_eq!(m.chapters(), vec![(0, "前言"), (1, "第一章"), (2, "第二章")]);
        for page in [1, 4, 10] {
            let c = m.chapter_of(page).unwrap();
            assert_eq!(m.chapters()[c.index].1, c.title, "第 {page} 页：章号与章表一致");
            assert_eq!(c.subhead, None);
        }
    }

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            for (name, body) in files {
                w.start_file(*name, o).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    /// 回归（2026-09-30）：nav 文档不叫 nav.xhtml、又没有 NCX 时按 OPF 声明找到它；名字里恰好带 nav 的正文不会被当成目录。
    #[test]
    fn toc_files_are_found_via_opf_declaration() {
        let container = r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let opf = r#"<package><manifest><item id="c" href="canvas.xhtml" media-type="application/xhtml+xml"/><item media-type="application/xhtml+xml" properties="nav" href='text/my%20toc.xhtml' id="t"/></manifest></package>"#;
        let decoy = r#"<ol><li><a href="canvas.xhtml">假目录</a></li></ol>"#;
        let toc = r#"<nav><ol><li><a href="c1.xhtml">第一章</a></li><li><a href="c2.xhtml">第二章</a></li></ol></nav>"#;
        let bytes = zip_of(&[("mimetype", "application/epub+zip"), ("META-INF/container.xml", container), ("OEBPS/content.opf", opf), ("OEBPS/canvas.xhtml", decoy), ("OEBPS/text/my toc.xhtml", toc)]);
        let (nav, ncx) = read_toc_texts(&bytes);
        assert!(nav.as_deref().is_some_and(|n| n.contains("第一章")), "{nav:?}");
        assert!(ncx.is_none());
        // 没有 container.xml 的旧式包：退回按文件名猜，正好叫 nav.xhtml 的优先
        let bytes = zip_of(&[("OEBPS/canvas.xhtml", decoy), ("OEBPS/nav.xhtml", toc)]);
        assert!(read_toc_texts(&bytes).0.is_some_and(|n| n.contains("第二章")));
        assert_eq!(resolve_href("OEBPS/text/", "../img/a%20b.xhtml#x"), "OEBPS/img/a b.xhtml");
        assert_eq!(resolve_href("", "./nav.xhtml"), "nav.xhtml");
        assert_eq!(percent_decode("%E7%AB%A0%zz%"), "章%zz%");
    }

    /// 从文件读端建表与整本字节建表结果一致（ingest 改走文件读端，不再整本读进内存）。
    #[test]
    fn reader_and_bytes_give_same_map() {
        use std::io::Write;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("mimetype", o).unwrap();
            w.write_all(b"application/epub+zip").unwrap();
            w.start_file("OEBPS/big.bin", o).unwrap();
            w.write_all(&vec![7u8; 1 << 20]).unwrap();
            w.start_file("OEBPS/toc.ncx", o).unwrap();
            w.write_all(include_bytes!("../../../testdata/renggu/toc.ncx")).unwrap();
            w.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let idx = include_bytes!("../../../testdata/renggu/book.epubindex");
        let dir = std::env::temp_dir().join(format!("epubmap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("b.epub");
        std::fs::write(&p, &bytes).unwrap();
        let a = BookMap::from_epub(&bytes, idx);
        let b = BookMap::from_epub_reader(std::io::BufReader::new(std::fs::File::open(&p).unwrap()), idx);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(a.chapters().len(), 38);
        assert_eq!(a.chapters(), b.chapters());
        assert_eq!(a.chapter_of(100), b.chapter_of(100));
    }

    #[test]
    fn real_fixture_renggu_flat_ncx() {
        // 真机《人骨拼圖》：.epubindex 38 个 spine 文件 + calibre 的扁平 toc.ncx（38 navPoint）
        let m = BookMap::new(parse_epubindex(include_bytes!("../../../testdata/renggu/book.epubindex")), Toc::parse(None, Some(include_str!("../../../testdata/renggu/toc.ncx"))));
        assert_eq!(m.sections.len(), 39, "38 章 + titlepage");
        assert_eq!(m.section_of(100).map(|s| s.file.as_str()), Some("7.xhtml"), "7.xhtml 起 97、8.xhtml 起 112");
        let c = m.chapter_of(100).unwrap();
        assert_eq!((c.index, c.title, c.subhead), (7, "7", None));
        assert_eq!(m.chapter_of(3).unwrap().title, "第一部 一天的國王 1");
        assert_eq!(m.chapter_of(0), None, "titlepage 不在目录里");
        assert_eq!(m.chapters().len(), 38);
    }
}
