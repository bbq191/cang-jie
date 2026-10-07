//! PDF 转出的 EPUB 的组装（`pdf_ingest::optimize_pdf_to_epub` 之后、book-serve 落地之前）。
//!
//! 组装本身用 sheng-ren `bookconv::epub::assemble_with`（OPF/nav/封面页/章节的写法、组装前的 `fix_internal_links`
//! 与 `break_footnote_cycles` 都在那边）；这里只给它 PDF 转出的书独有的两项选项：
//! 1. **外链共用样式表** `pdf-img.css`（图片 `max-width` + 颜色类），只挂在含 `<img`/颜色 span 的章节上（[`SharedCss`]）；
//! 2. **`dc:identifier` 前缀 `weread:`**：母版库靠 `weread:pdf:` 认出"PDF 转出的 EPUB"（[`crate::pdf_ingest::looks_like_pdf_derived_epub`]），
//!    设备上已有的这类书都是这个前缀，不能跟着 sheng-ren 改成 `urn:bookconv:`。
//!
//! 2026-10-07 起改用 sheng-ren 的组装器（此前是旧 cang-jie 组装器的一份拷贝），产物和以前的差别：`<html>` 上多写 `lang="zh"`
//! （以前只写 `xml:lang="zh"`）、样式表的 manifest 条目换成单独一行，目录超过两级时按实际层级嵌套（以前三级及以下都压成第二级）。

use bookconv::epub::{assemble_with, AssembleOpts, Book, SharedCss};

/// 组装出的 OPF 在 zip 里的路径（`container.xml` 指向它；PDF 来源识别等也按这个路径读）。
pub const OPF_PATH: &str = bookconv::epub::OPF_PATH;
/// OPF `dc:identifier` 的前缀：`weread:{book_id}`。`pdf_ingest::looks_like_pdf_derived_epub` 靠
/// 它加 `book_id` 的 `pdf:` 前缀识别"PDF 转出的 EPUB"，两边必须同源。
pub const ID_SCHEME: &str = "weread:";

/// 不区分大小写的 `<img` 探测（不为此分配整章小写副本）。
fn has_img_tag(html: &str) -> bool {
    html.as_bytes().windows(4).any(|w| w.eq_ignore_ascii_case(b"<img"))
}

/// PDF→EPUB 颜色 span 探测（`optimize_pdf_to_epub` 生成的 `class="cj-cN"`）——共享样式表除了图片尺寸规则还带颜色规则，
/// 纯文字页也可能用到颜色，不能只按"有没有 `<img`"决定要不要挂 `<link>`（2026-09-23）。
fn has_color_span(html: &str) -> bool {
    html.contains("class=\"cj-c")
}

/// 章节要不要挂共用样式表。
fn needs_shared_css(html: &str) -> bool {
    has_img_tag(html) || has_color_span(html)
}

/// 共用样式表在 `OEBPS/` 下的文件名（也是章节 `<link href>` 的值）与 manifest id。
const SHARED_CSS_FILE: &str = "pdf-img.css";
const SHARED_CSS_ID: &str = "pdf-img-css";
const PDF_IMG_CSS: &str = "img{max-width:100%;height:auto;}\n";

/// PDF→EPUB 转出的书专用：`<img>` 没有 `width`/`height`（源自 PDF 页内嵌图），也没有任何外链 CSS 撑住布局时，xochitl
/// 原生阅读器里图片整个不出现（2026-09-23 真机投一本真实 PDF 手册核实：24 张图一张没有）。所以写一份外链 `pdf-img.css`
/// （`max-width:100%` 只封顶超宽图、不撑大小图，也不清零正文页边距），只挂在含 `<img`/颜色 span 的章节上。
/// `extra_css`：`optimize_pdf_to_epub` 返回的颜色 CSS（`.cj-cN{color:#rrggbb;}` 逐条，可能是空串）。
/// 组装前对每章：`fix_internal_links`（同文件锚点规整）→ `break_footnote_cycles`（拆双向互指对，xochitl 会整对丢掉），由 sheng-ren 组装器做。
/// **组装后 `book.resources` 被清空**（写一张释放一张）：PDF 转出的书图片可达上百 MB，不让"资源表 + zip 缓冲"同时各占一整份。
pub fn assemble_pdf_derived(book: &mut Book, extra_css: &str) -> Result<Vec<u8>, String> {
    assemble_with(
        book,
        AssembleOpts {
            consume_resources: true,
            id_scheme: Some(ID_SCHEME.to_string()),
            shared_css: Some(SharedCss {
                file: SHARED_CSS_FILE.to_string(),
                id: SHARED_CSS_ID.to_string(),
                content: format!("{PDF_IMG_CSS}{extra_css}"),
                link_if: Some(needs_shared_css),
            }),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookconv::epub::{BookMeta, Chapter, Resource};

    fn meta() -> BookMeta {
        BookMeta { book_id: "b".into(), title: "t".into(), author: "".into(), language: "zh".into(), publisher: "".into(), cover: None, cover_ext: "jpg".into(), cover_media_type: "image/jpeg".into(), subjects: Vec::new() }
    }

    fn two_chapter_book() -> Book {
        Book {
            meta: meta(),
            chapters: vec![
                Chapter { title: "图页".into(), html_body: "<div><IMG src=\"images/a.png\"/></div>".into(), level: 1 },
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
        let mut b = two_chapter_book();
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
        let opf = get(OPF_PATH);
        assert!(opf.contains("<item id=\"pdf-img-css\" href=\"pdf-img.css\" media-type=\"text/css\"/>"), "{opf}");
        assert!(opf.contains("weread:b</dc:identifier>"), "标识符前缀保持 weread:（母版库靠它认 PDF 来源）: {opf}");
        assert!(opf.contains("id=\"res1\" href=\"images/a.png\" media-type=\"image/png\""), "manifest 缺资源项: {opf}");
    }
}
