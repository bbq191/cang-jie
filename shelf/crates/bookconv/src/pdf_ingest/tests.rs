//! PDF 入库单测（原 `pdf_ingest.rs` 内联的 `mod tests`）。
use super::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../tests/fixtures/sample.pdf");

fn char_at(ch: char, x: f64, y: f64, font_size: f64, line: usize) -> PositionedChar {
    PositionedChar { ch, x, y, font_size, line }
}

// ---- 公式区域探测 ----

#[test]
fn formula_detection_single_inline_run() {
    let chars = vec![
        char_at('x', 0.0, 100.0, 10.0, 0),
        char_at('=', 10.0, 100.0, 10.0, 0),
        char_at('π', 20.0, 100.0, 10.0, 0), // 命中数学符号区块
    ];
    let blocks = detect_formula_regions(&chars);
    assert_eq!(blocks.len(), 1, "单行内联公式应该产出一个公式块");
}

#[test]
fn formula_detection_multiline_merges_into_one_block() {
    // 模拟分数/矩阵：多行都出现数学符号，行号相邻。
    let chars = vec![
        char_at('∫', 0.0, 200.0, 10.0, 0),
        char_at('0', 5.0, 200.0, 10.0, 0),
        char_at('√', 0.0, 190.0, 10.0, 1),
        char_at('π', 5.0, 190.0, 10.0, 1),
    ];
    let blocks = detect_formula_regions(&chars);
    assert_eq!(blocks.len(), 1, "垂直相邻的公式行应该合并成一个块，不是两个");
}

#[test]
fn formula_detection_plain_prose_has_zero_blocks() {
    let chars = vec![
        char_at('h', 0.0, 100.0, 10.0, 0),
        char_at('e', 5.0, 100.0, 10.0, 0),
        char_at('l', 10.0, 100.0, 10.0, 0),
        char_at('l', 15.0, 100.0, 10.0, 0),
        char_at('o', 20.0, 100.0, 10.0, 0),
    ];
    assert!(detect_formula_regions(&chars).is_empty(), "纯正文字符不该产出任何公式块");
}

#[test]
fn formula_detection_empty_input() {
    assert!(detect_formula_regions(&[]).is_empty());
}

#[test]
fn formula_detection_far_lines_not_merged() {
    let chars = vec![
        char_at('π', 0.0, 500.0, 10.0, 0),
        char_at('α', 0.0, 100.0, 10.0, 10), // 行号差很远，不该合并
    ];
    let blocks = detect_formula_regions(&chars);
    assert_eq!(blocks.len(), 2, "相隔很远的两处公式不该被误合并成一个块");
}

// ---- 字号识别标题 ----

#[test]
fn heading_detection_picks_larger_font_lines() {
    let mut page = Vec::new();
    // 正文：字号 10，多次出现占多数。
    for i in 0..20 {
        page.push(char_at('a', i as f64, 100.0, 10.0, 1));
    }
    // 标题行：字号 16（1.6 倍正文），字号更大。
    for i in 0..5 {
        page.push(char_at('T', i as f64, 200.0, 16.0, 0));
    }
    let headings = detect_headings_by_font_size(&[page]);
    assert_eq!(headings.len(), 1);
    assert_eq!(headings[0].level, 1);
}

#[test]
fn heading_detection_zero_candidates_when_uniform_size() {
    let page: Vec<PositionedChar> = (0..20).map(|i| char_at('a', i as f64, 100.0, 10.0, 0)).collect();
    assert!(detect_headings_by_font_size(&[page]).is_empty(), "字号完全统一时不该识别出标题");
}

#[test]
fn heading_detection_ranks_levels_by_size() {
    let mut p1 = Vec::new();
    for i in 0..10 {
        p1.push(char_at('a', i as f64, 100.0, 10.0, 1));
    }
    for i in 0..3 {
        p1.push(char_at('T', i as f64, 200.0, 20.0, 0)); // 最大字号 → level 1
    }
    let mut p2 = Vec::new();
    for i in 0..10 {
        p2.push(char_at('a', i as f64, 100.0, 10.0, 1));
    }
    for i in 0..3 {
        p2.push(char_at('S', i as f64, 200.0, 14.0, 0)); // 次大字号 → level 2
    }
    let headings = detect_headings_by_font_size(&[p1, p2]);
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[0].level, 1);
    assert_eq!(headings[1].level, 2);
}

// ---- 分类（拿真实 pdflatex 样本核对，不是拍脑袋） ----

#[test]
fn classify_real_pdflatex_sample_is_text_layer() {
    assert_eq!(classify_pdf_bytes(SAMPLE_PDF), PdfKind::TextLayer, "真实 pdflatex 文字样本应该判定为有文字层");
}

#[test]
fn classify_one_image_per_page_pdf_is_comic() {
    const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    // `images_to_pdf` 的 MediaBox 直接等于图片像素尺寸（1px=1pt，见其文档注释），宽高比
    // 天然跟页面一致——不需要专门构造特定尺寸的图片，这条判据本来就该命中。
    let img = pdfwrite::image_from_bytes(RED_PNG).unwrap();
    let comic_pdf = pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
    assert_eq!(classify_pdf_bytes(&comic_pdf), PdfKind::Comic);
}

#[test]
fn classify_unparseable_bytes_defaults_to_no_text_layer() {
    assert_eq!(classify_pdf_bytes(b"not a pdf"), PdfKind::NoTextLayer, "解析失败要退到保守默认（只裁边），不能默认转换");
}

// ---- 端到端：真实样本转 EPUB ----

#[test]
fn optimize_pdf_to_epub_real_sample_produces_chapters_and_resources() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("sample.pdf");
    std::fs::write(&src, SAMPLE_PDF).unwrap();
    let (book, report) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
    assert_eq!(report.pages, 3, "样本是 3 页");
    assert!(report.chapters >= 2, "没有书签，应该靠字号识别出至少 2 个标题层级的章节，实际 {}", report.chapters);
    assert!(report.formula_blocks >= 1, "样本含多处公式，应该探测到至少一个公式块");
    assert!(!book.resources.is_empty(), "样本含嵌入图片+公式块渲染图，resources 不该是空的");
    let bytes = crate::epub::assemble(&mut { book }).unwrap();
    assert!(!bytes.is_empty());
}

/// 章节 HTML 的可见文字（去标签、去空白、还原 `&amp;` 等）。
fn visible(html: &str) -> String {
    let no_tags = regex::Regex::new(r"(?s)<[^>]*>").unwrap().replace_all(html, "");
    let t = no_tags.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    t.chars().filter(|c| !c.is_whitespace()).collect()
}

/// **不允许变动书籍内容**的核心不变量：PDF 文字层提取出的每个字符，按顺序原样出现在 EPUB 文字流里，
/// 不多不少（此前公式外接框内字符被整体丢弃，吞掉了 identity / 整半句正文；标题还被重复输出一遍）。
#[test]
fn epub_text_flow_equals_pdf_text_layer_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("sample.pdf");
    std::fs::write(&src, SAMPLE_PDF).unwrap();
    let expected: String = extract_positioned_text(SAMPLE_PDF)
        .unwrap()
        .iter()
        .flatten()
        .filter(|c| c.ch != '\u{0}')
        .map(|c| c.ch)
        .filter(|c| !c.is_whitespace())
        .collect();
    let (book, _) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
    let got: String = book.chapters.iter().map(|c| visible(&c.html_body)).collect();
    assert_eq!(got, expected, "EPUB 文字流必须与 PDF 文字层逐字一致");
    // 具体回归点：紧贴公式的正文单词与整半句不能丢；标题只出现一次。
    assert!(got.contains("identity") && got.contains("quadraticformula"), "行内公式旁的单词被吞了");
    assert!(got.contains("Afinalshortsection") && got.contains("onemoreinlinesymbol"), "整半句正文被吞了");
    assert_eq!(got.matches("1Introduction").count(), 1, "章节标题不该重复输出");
}

#[test]
fn promote_heading_upgrades_existing_paragraph_without_adding_text() {
    assert_eq!(promote_heading("1 Intro", "<p>1 Intro</p><p>body</p>".into()), "<h2>1 Intro</h2><p>body</p>");
    assert_eq!(promote_heading("1 Intro", "<p>1 Intro Some text</p>".into()), "<h2>1 Intro</h2><p>Some text</p>");
    // 章内找不到标题段落：不往正文里塞原书没有的字。
    assert_eq!(promote_heading("Missing", "<p>body</p>".into()), "<p>body</p>");
}

#[test]
fn looks_like_pdf_derived_epub_detects_marker() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("sample.pdf");
    std::fs::write(&src, SAMPLE_PDF).unwrap();
    let (mut book, _) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
    let bytes = crate::epub::assemble(&mut book).unwrap();
    let out = dir.path().join("out.epub");
    std::fs::write(&out, &bytes).unwrap();
    assert!(looks_like_pdf_derived_epub(&out));
}

#[test]
fn looks_like_pdf_derived_epub_false_for_unrelated_zip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("not_epub.zip");
    let file = std::fs::File::create(&p).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file::<_, ()>("hello.txt", Default::default()).unwrap();
    std::io::Write::write_all(&mut zip, b"hi").unwrap();
    zip.finish().unwrap();
    assert!(!looks_like_pdf_derived_epub(&p));
}

// ---- 裁边路径 ----

#[test]
fn optimize_pdf_trim_only_comic_shaped_fixture_roundtrips() {
    // sample.pdf 是文字样本，不代表"裁边"路径的真实输入形状（裁边只服务一页一图的扫描件/
    // 漫画 PDF）——这里用既有的 `images_to_pdf`（漫画 EPUB→PDF 那条产线复用的写手，已经
    // 有自己的测试覆盖）现造一份"每页一张图"的合成 PDF，形状上才贴近这条路径真正会遇到的
    // 输入。
    const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let img = pdfwrite::image_from_bytes(RED_PNG).unwrap();
    let comic_pdf = pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("comic.pdf");
    std::fs::write(&src, &comic_pdf).unwrap();
    let dst = dir.path().join("out.pdf");
    let report = optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap();
    assert_eq!(report.pages, 3);
    assert!(pdfwrite::looks_like_own_bookconv_pdf(&dst), "裁边输出应该能被识别成自产 PDF");
    // 要有目录：源 PDF 没有书签 → 按页分段兜底，不是空的。
    let titles = pdfwrite::PdfFileReader::open(&dst).unwrap().outline_titles().unwrap();
    assert_eq!(titles, vec![(0, "第 1–3 页".to_string())]);
}

fn red_png_pdf(pages: usize, titles: &[(usize, String)]) -> Vec<u8> {
    // 与设备页面(954×1696)同宽高比的整页图，才满足"单张整页图"判据。
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(95, 169, image::Rgb([200, 30, 30])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let img = pdfwrite::image_from_bytes(&png).unwrap();
    let imgs: Vec<_> = (0..pages).map(|_| img.clone()).collect();
    pdfwrite::images_to_pdf_with_toc(&imgs, titles).unwrap()
}

#[test]
fn trim_only_preserves_original_bookmarks() {
    // 原书签必须保留（不许变动书籍内容 + 要有目录），页码原样。
    let titles = vec![(0, "第一卷".to_string()), (2, "第二卷".to_string())];
    let pdf = red_png_pdf(4, &titles);
    let dir = tempfile::tempdir().unwrap();
    let (src, dst) = (dir.path().join("a.pdf"), dir.path().join("o.pdf"));
    std::fs::write(&src, &pdf).unwrap();
    optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap();
    assert_eq!(pdfwrite::PdfFileReader::open(&dst).unwrap().outline_titles().unwrap(), titles);
}

#[test]
fn trim_only_refuses_pdf_with_text_layer_and_leaves_it_untouched() {
    // 文字样本含大量文字：重写页面会丢文字层——必须拒绝，且不产出任何文件。
    let dir = tempfile::tempdir().unwrap();
    let (src, dst) = (dir.path().join("t.pdf"), dir.path().join("o.pdf"));
    std::fs::write(&src, SAMPLE_PDF).unwrap();
    let err = optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap_err();
    assert!(err.contains("文字"), "应说明是因为文字层: {err}");
    assert!(!dst.exists());
}
