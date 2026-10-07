//! PDF 转出的 EPUB 的组装（`pdf_ingest::optimize_pdf_to_epub` 之后、book-serve 落地之前）。
//!
//! 书的数据结构（[`Book`] 等）用 sheng-ren `bookconv::epub` 的；组装本身留在这里，因为 PDF 转出的书有两点 cang-jie 独有：
//! 1. **外链共用样式表** `pdf-img.css`（图片 `max-width` + 颜色类），只挂在含 `<img`/颜色 span 的章节上——sheng-ren 的
//!    `epub::assemble` 没有这个选项（它的 `AssembleOpts` 是 crate 内私有）；
//! 2. **`dc:identifier` 前缀 `weread:pdf:`**：母版库靠它认出"PDF 转出的 EPUB"（[`crate::pdf_ingest::looks_like_pdf_derived_epub`]），
//!    设备上已有的这类书都是这个前缀，不能跟着 sheng-ren 改成 `urn:bookconv:`。
//!
//! 其余（OPF/nav/封面页的写法）照原 cang-jie `bookconv::epub` 原样搬来，产物与迁移前逐字节相同。mimetype 首个 STORED，全部条目 STORED。

use bookconv::epub::{chapter_filename, Book, BookMeta, Chapter, NavEntry};
use bookconv::util::xml_escape as xesc;
use bookconv::htmlproc::{break_footnote_cycles, fix_internal_links};

/// 组装出的 OPF 在 zip 里的路径（`container.xml` 指向它；PDF 来源识别等也按这个路径读）。
pub const OPF_PATH: &str = "OEBPS/content.opf";
/// OPF `dc:identifier` 的前缀：`weread:{book_id}`。`pdf_ingest::looks_like_pdf_derived_epub` 靠
/// 它加 `book_id` 的 `pdf:` 前缀识别"PDF 转出的 EPUB"，两边必须同源。
pub const ID_SCHEME: &str = "weread:";


/// `head_extra` 原样插在 `</head>` 前（外链样式表 `<link>` 等），普通章节传 `""`。
fn chapter_doc(ch: &Chapter, head_extra: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"zh\">\n<head><title>{}</title>{head_extra}</head>\n<body>{}</body>\n</html>\n",
        xesc(&ch.title),
        ch.html_body
    )
}

fn container_xml() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n  <rootfiles>\n    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n  </rootfiles>\n</container>\n"
}

fn cover_xhtml(m: &BookMeta) -> String {
    // 封面**水平+垂直双居中**、限高一屏：早先只 text-align:center（仅水平居中、height:auto 垂直贴顶），
    // 宽高比偏方/偏宽的封面宽度撑满后高度不足就贴页顶 → 书库缩略图「偏上」。table/table-cell +
    // vertical-align:middle 是老渲染器（xochitl epub 引擎）也吃的垂直居中法；max-height:100vh 防超高。
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"zh\">\n<head><title>封面</title>\n<style>html,body{{margin:0;padding:0;height:100%;}} .cv{{display:table;width:100%;height:100vh;}} .cv-c{{display:table-cell;vertical-align:middle;text-align:center;}} .cv-c img{{max-width:100%;max-height:100vh;}}</style></head>\n<body>\n  <div class=\"cv\"><div class=\"cv-c\"><img src=\"cover.{}\" alt=\"{}\"/></div></div>\n</body>\n</html>\n",
        m.cover_ext,
        xesc(&m.title)
    )
}

fn content_opf(book: &Book) -> String {
    let m = &book.meta;
    let has_cover = m.cover.is_some();
    let mut manifest: Vec<String> = vec![
        "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
    ];
    let mut spine: Vec<String> = Vec::new();
    if has_cover {
        manifest.push(format!(
            "    <item id=\"cover-image\" href=\"cover.{}\" media-type=\"{}\" properties=\"cover-image\"/>",
            m.cover_ext, m.cover_media_type
        ));
        manifest.push("    <item id=\"cover\" href=\"cover.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string());
        spine.push("    <itemref idref=\"cover\"/>".to_string());
    }
    for i in 0..book.chapters.len() {
        let cid = format!("c{}", i + 1);
        let href = chapter_filename(i);
        manifest.push(format!(
            "    <item id=\"{cid}\" href=\"{href}\" media-type=\"application/xhtml+xml\"/>"
        ));
        spine.push(format!("    <itemref idref=\"{cid}\"/>"));
    }
    // 嵌入资源（插图等），只进 manifest、不进 spine
    for (i, r) in book.resources.iter().enumerate() {
        manifest.push(format!(
            "    <item id=\"res{}\" href=\"{}\" media-type=\"{}\"/>",
            i + 1,
            xesc(&r.path),
            xesc(&r.media_type)
        ));
    }
    let author = if m.author.is_empty() {
        String::new()
    } else {
        format!("\n    <dc:creator>{}</dc:creator>", xesc(&m.author))
    };
    let publisher = if m.publisher.is_empty() {
        String::new()
    } else {
        format!("\n    <dc:publisher>{}</dc:publisher>", xesc(&m.publisher))
    };
    let cover_meta = if has_cover {
        "\n    <meta name=\"cover\" content=\"cover-image\"/>"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"pub-id\">\n  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n    <dc:identifier id=\"pub-id\">{ID_SCHEME}{}</dc:identifier>\n    <dc:title>{}</dc:title>\n    <dc:language>{}</dc:language>{}{}{}\n  </metadata>\n  <manifest>\n{}\n  </manifest>\n  <spine>\n{}\n  </spine>\n</package>\n",
        xesc(&m.book_id),
        xesc(&m.title),
        xesc(&m.language),
        author,
        publisher,
        cover_meta,
        manifest.join("\n"),
        spine.join("\n")
    )
}

/// 生成嵌套 nav：level<=1 为顶层 <li>，level>=2 收进上一个顶层项的子 <ol>。
/// 只收非空标题章（整章一页后每章都带标题）。仅两层——微信读书目录最多部/章两级。
fn nav_body(book: &Book) -> String {
    let visible: Vec<NavEntry> = if book.nav.is_empty() {
        book.chapters
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.title.is_empty())
            .map(|(i, c)| NavEntry { title: c.title.clone(), level: c.level, href: chapter_filename(i) })
            .collect()
    } else {
        book.nav.iter().filter(|n| !n.title.is_empty()).cloned().collect()
    };
    let mut out = String::new();
    let mut sub_open = false; // 是否有未闭合的子 <ol>
    for (idx, ch) in visible.iter().enumerate() {
        let link = format!("<a href=\"{}\">{}</a>", xesc(&ch.href), xesc(&ch.title));
        if ch.level <= 1 {
            if sub_open {
                out.push_str("        </ol>\n      </li>\n");
                sub_open = false;
            }
            let has_child = visible.get(idx + 1).is_some_and(|n| n.level >= 2);
            if has_child {
                out.push_str(&format!("      <li>{link}\n        <ol>\n"));
                sub_open = true;
            } else {
                out.push_str(&format!("      <li>{link}</li>\n"));
            }
        } else if sub_open {
            out.push_str(&format!("          <li>{link}</li>\n"));
        } else {
            // 容错：level>=2 却无顶层父（书首即子节），平铺为顶层
            out.push_str(&format!("      <li>{link}</li>\n"));
        }
    }
    if sub_open {
        out.push_str("        </ol>\n      </li>\n");
    }
    out
}

fn nav_xhtml(book: &Book) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"zh\">\n<head><title>目录</title></head>\n<body>\n  <nav epub:type=\"toc\" id=\"toc\">\n    <ol>\n{}    </ol>\n  </nav>\n</body>\n</html>\n",
        nav_body(book)
    )
}

/// 不区分大小写的 `<img` 探测（不为此分配整章小写副本）。
fn has_img_tag(html: &str) -> bool {
    html.as_bytes().windows(4).any(|w| w.eq_ignore_ascii_case(b"<img"))
}

/// PDF→EPUB 颜色 span 探测（`optimize_pdf_to_epub` 生成的 `class="cj-cN"`）——共享样式表除了图片尺寸规则还带颜色规则，
/// 纯文字页也可能用到颜色，不能只按"有没有 `<img`"决定要不要挂 `<link>`（2026-09-23）。
fn has_color_span(html: &str) -> bool {
    html.contains("class=\"cj-c")
}

/// 共用样式表在 `OEBPS/` 下的文件名（也是章节 `<link href>` 的值）与 manifest id。
const SHARED_CSS_FILE: &str = "pdf-img.css";
const SHARED_CSS_ID: &str = "pdf-img-css";
const PDF_IMG_CSS: &str = "img{max-width:100%;height:auto;}\n";

/// PDF→EPUB 转出的书专用：`<img>` 没有 `width`/`height`（源自 PDF 页内嵌图），也没有任何外链 CSS 撑住布局时，xochitl
/// 原生阅读器里图片整个不出现（2026-09-23 真机投一本真实 PDF 手册核实：24 张图一张没有）。所以写一份外链 `pdf-img.css`
/// （`max-width:100%` 只封顶超宽图、不撑大小图，也不清零正文页边距），只挂在含 `<img`/颜色 span 的章节上。
/// `extra_css`：`optimize_pdf_to_epub` 返回的颜色 CSS（`.cj-cN{color:#rrggbb;}` 逐条，可能是空串）。
/// 组装前对每章：`fix_internal_links`（同文件锚点规整）→ `break_footnote_cycles`（拆双向互指对，xochitl 会整对丢掉）。
/// **组装后 `book.resources` 被清空**（写一张释放一张）：PDF 转出的书图片可达上百 MB，不让"资源表 + zip 缓冲"同时各占一整份。
pub fn assemble_pdf_derived(book: &mut Book, extra_css: &str) -> Result<Vec<u8>, String> {
    if book.chapters.is_empty() {
        return Err("EPUB 至少要有一章".into());
    }
    for ch in book.chapters.iter_mut() {
        ch.html_body = fix_internal_links(&ch.html_body);
        ch.html_body = break_footnote_cycles(&ch.html_body);
    }
    let css = format!("{PDF_IMG_CSS}{extra_css}");
    let opf = content_opf(book).replacen("</manifest>", &format!("<item id=\"{SHARED_CSS_ID}\" href=\"{SHARED_CSS_FILE}\" media-type=\"text/css\"/></manifest>"), 1);
    // 预留足够容量：全部 STORED，产物 ≈ 资源 + 章节文本 + 少量固定条目。Vec 倍增扩容会在峰值瞬间同时持有新旧两块。
    let cap = book.resources.iter().map(|r| r.bytes.len()).sum::<usize>() + book.chapters.iter().map(|c| c.html_body.len() + 512).sum::<usize>() + opf.len() + 16 * 1024;
    let mut buf: Vec<u8> = Vec::with_capacity(cap);
    {
        // mimetype 首个、STORED（`EpubWriter::new` 写），其余也全部 STORED
        let mut z = bookconv::epubzip::EpubWriter::new(std::io::Cursor::new(&mut buf))?;
        z.put_stored("META-INF/container.xml", container_xml().as_bytes())?;
        z.put_stored(OPF_PATH, opf.as_bytes())?;
        z.put_stored("OEBPS/nav.xhtml", nav_xhtml(book).as_bytes())?;
        if let Some(cover) = &book.meta.cover {
            z.put_stored(&format!("OEBPS/cover.{}", book.meta.cover_ext), cover)?;
            z.put_stored("OEBPS/cover.xhtml", cover_xhtml(&book.meta).as_bytes())?;
        }
        for (i, ch) in book.chapters.iter().enumerate() {
            let link = if has_img_tag(&ch.html_body) || has_color_span(&ch.html_body) { format!("<link rel=\"stylesheet\" type=\"text/css\" href=\"{SHARED_CSS_FILE}\"/>") } else { String::new() };
            z.put_stored(&format!("OEBPS/{}", chapter_filename(i)), chapter_doc(ch, &link).as_bytes())?;
        }
        for r in std::mem::take(&mut book.resources) {
            z.put_stored(&format!("OEBPS/{}", r.path), &r.bytes)?; // r 在本次迭代结束即释放
        }
        z.put_stored(&format!("OEBPS/{SHARED_CSS_FILE}"), css.as_bytes())?;
        z.finish()?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookconv::epub::Resource;

    fn meta() -> BookMeta {
        BookMeta { book_id: "b".into(), title: "t".into(), author: "".into(), language: "zh".into(), publisher: "".into(), cover: None, cover_ext: "jpg".into(), cover_media_type: "image/jpeg".into(), subjects: Vec::new() }
    }
    fn ch(title: &str, level: i64) -> Chapter {
        Chapter { title: title.into(), html_body: "<p>x</p>".into(), level }
    }

    #[test]
    fn nests_level2_under_level1() {
        let book = Book { meta: meta(), chapters: vec![ch("第一部", 1), ch("第一章", 2), ch("第二章", 2), ch("第二部", 1)], resources: vec![], nav: Vec::new() };
        let nav = nav_body(&book);
        assert!(nav.matches("<ol>").count() == 1 && nav.matches("</ol>").count() == 1, "应恰有一个子 ol: {nav}");
        assert!(nav.find("<ol>").unwrap() < nav.find("第一章").unwrap(), "第一章应在子 ol 内: {nav}");
        let flat = Book { meta: meta(), chapters: vec![ch("一", 1), ch("二", 1)], resources: vec![], nav: Vec::new() };
        assert!(!nav_body(&flat).contains("<ol>"), "全顶层不应有子 ol");
    }

    fn two_chapter_book(with_img: bool) -> Book {
        Book {
            meta: meta(),
            chapters: vec![
                Chapter { title: "图页".into(), html_body: if with_img { "<div><IMG src=\"images/a.png\"/></div>".into() } else { "<p>字</p>".into() }, level: 1 },
                Chapter { title: "字页".into(), html_body: "<p>纯文字</p>".into(), level: 1 },
            ],
            resources: vec![Resource { path: "images/a.png".into(), media_type: "image/png".into(), bytes: vec![9; 64] }],
            nav: Vec::new(),
        }
    }

    fn zip_names_and_text(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut v = Vec::new(); f.read_to_end(&mut v).unwrap(); (f.name().to_string(), v) }).collect()
    }

    /// 2026-09-23 真机投一本真实 PDF 手册发现：没有这条 CSS 时图片在 xochitl 原生阅读器里完全不出现。
    #[test]
    fn links_max_width_css_to_image_and_color_chapters_only() {
        let mut b = two_chapter_book(true);
        b.chapters[1].html_body = "<p><span class=\"cj-c0\">红字</span></p>".into();
        b.chapters.push(Chapter { title: "纯字".into(), html_body: "<p>字</p>".into(), level: 1 });
        let out = assemble_pdf_derived(&mut b, ".cj-c0{color:#ff0000;}\n").unwrap();
        assert!(b.resources.is_empty(), "资源写完即释放");
        let entries = zip_names_and_text(out);
        assert_eq!(entries[0].0, "mimetype");
        assert_eq!(entries.last().unwrap().0, "OEBPS/pdf-img.css", "样式表条目排在资源之后");
        let get = |n: &str| String::from_utf8(entries.iter().find(|(k, _)| k == n).unwrap().1.clone()).unwrap();
        assert_eq!(get("OEBPS/pdf-img.css"), "img{max-width:100%;height:auto;}\n.cj-c0{color:#ff0000;}\n");
        assert!(get("OEBPS/chap_0001.xhtml").contains("<link rel=\"stylesheet\" type=\"text/css\" href=\"pdf-img.css\"/></head>"), "含 <IMG（大小写不敏感）的章要挂 link");
        assert!(get("OEBPS/chap_0002.xhtml").contains("href=\"pdf-img.css\""), "有颜色 span 的章也挂");
        assert!(!get("OEBPS/chap_0003.xhtml").contains("<link"), "纯文字章不挂");
        let opf = get("OEBPS/content.opf");
        assert!(opf.contains("<item id=\"pdf-img-css\" href=\"pdf-img.css\" media-type=\"text/css\"/></manifest>"), "{opf}");
        assert!(opf.contains("weread:b</dc:identifier>"), "标识符前缀保持 weread:（母版库靠它认 PDF 来源）: {opf}");
        assert!(opf.contains("id=\"res1\" href=\"images/a.png\" media-type=\"image/png\""), "manifest 缺资源项: {opf}");
    }

    #[test]
    fn explicit_nav_entries_can_share_a_file_and_carry_fragments() {
        let mut b = two_chapter_book(false);
        b.nav = vec![
            NavEntry { title: "栏目".into(), level: 1, href: "chap_0001.xhtml".into() },
            NavEntry { title: "文章甲".into(), level: 2, href: "chap_0001.xhtml#a".into() },
            NavEntry { title: "文章乙".into(), level: 2, href: "chap_0002.xhtml#b".into() },
        ];
        let nav = nav_body(&b);
        assert!(nav.contains("<li><a href=\"chap_0001.xhtml\">栏目</a>\n        <ol>"), "{nav}");
        assert!(nav.contains("<li><a href=\"chap_0001.xhtml#a\">文章甲</a></li>") && nav.contains("<li><a href=\"chap_0002.xhtml#b\">文章乙</a></li>"), "{nav}");
        assert!(!nav.contains("图页") && !nav.contains("字页"), "有显式目录时不再按章节生成: {nav}");
    }
}
