use super::*;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

type Ent = (String, Vec<u8>, CompressionMethod, u32);

#[test]
fn wash_css_name_matches_sheng_ren() {
    assert!(bookconv::wash::is_wash_css_name(&format!("OEBPS/{WASH_CSS}")), "sheng-ren 清洗层的样式表改名了，这里要跟着改");
}

#[test]
fn class_words_map_whole_words_only() {
    assert_eq!(rename_classes("calibre cj-flush  cj-tx"), ("calibre eink-flush  eink-tx".to_string(), 2));
    assert_eq!(rename_classes("cj-tp"), ("eink-textpage".to_string(), 1));
    assert_eq!(rename_classes("cj-tpx cj-c3 cj-w40 cj-imgwrap"), ("cj-tpx cj-c3 cj-w40 cj-imgwrap".to_string(), 0), "不在表里的不动");
    assert_eq!(rename_classes(""), (String::new(), 0));
}

#[test]
fn html_classes_links_and_counters() {
    let src = r##"<html><head><link href="../cangjie-wash.css" rel="stylesheet" type="text/css"/><style class="cj-wash">p{}</style></head><body class='x cj-tp'><div class="cj-flush" id="a">首段</div><p class="cj-center">诗</p><p>正文<a href="#n1"><sup>1</sup></a> <a href="#n1">[1]</a>，<a href="#n2">[2]</a></p><p>说cangjie-wash.css和 class="cj-tx" 这几个字是正文</p><span class="cj-fnote">〔注〕</span><div id="n1" class="cj-note">注一</div></body></html>"##;
    let (out, classes, counters) = convert_html(src);
    assert_eq!((classes, counters), (6, 1), "{out}");
    assert!(out.contains(r#"<link href="../eink-wash.css""#), "{out}");
    assert!(out.contains(r#"<style class="eink-wash">"#) && out.contains("<body class='x eink-textpage'>"), "{out}");
    assert!(out.contains(r#"<div class="eink-flush" id="a">"#) && out.contains(r#"class="eink-center""#) && out.contains(r#"class="eink-fnote""#) && out.contains(r#"class="eink-note""#), "{out}");
    assert!(out.contains(r##"<a href="#n1"><sup>1</sup></a>，<a href="#n2">[2]</a>"##), "只去紧跟同目标原标号的 [N]: {out}");
    assert!(out.contains(r#"<p>说cangjie-wash.css和 class="cj-tx" 这几个字是正文</p>"#), "正文里的字不动: {out}");
    // 可见文字：除了去掉的那个 [1]，一字不差
    assert_eq!(html::plain_text(&out), html::plain_text(src).replacen(" [1]", "", 1));
}

/// v15 在注释标号后追加的 `[N]`：前面紧挨着指向同一锚点的原标号时去掉，其余写法不动（原 cang-jie 测试原样搬来）。
#[test]
fn strip_legacy_note_counters_only_removes_duplicate_links() {
    let t = r##"<p>正文<a href="#n1"><sup>1</sup></a> <a href="#n1">[1]</a>，又<sup>2</sup><a href="#n2">[2]</a>，还有<a href="#n3">[3]</a>和<a href="#x">甲</a> <a href="#y">[4]</a></p>"##;
    assert_eq!(strip_legacy_note_counters(t), r##"<p>正文<a href="#n1"><sup>1</sup></a>，又<sup>2</sup><a href="#n2">[2]</a>，还有<a href="#n3">[3]</a>和<a href="#x">甲</a> <a href="#y">[4]</a></p>"##);
}

#[test]
fn opf_manifest_item_renamed() {
    let opf = r#"<manifest><item id="cangjie-wash-css" href="cangjie-wash.css" media-type="text/css"/><item id="c1" href="Text/cangjie-wash.css.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine>"#;
    let out = convert_opf(opf);
    assert!(out.contains(r#"<item id="eink-wash-css" href="eink-wash.css" media-type="text/css"/>"#), "{out}");
    assert!(out.contains(r#"href="Text/cangjie-wash.css.xhtml""#), "只认文件名完全相同的: {out}");
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 3) as u8, (y * 2) as u8, 90])));
    let mut b = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Jpeg).unwrap();
    b
}

fn write_zip(p: &Path, files: &[(&str, Vec<u8>)]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(p).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (n, d) in files {
        z.start_file(*n, if *n == "mimetype" { stored } else { deflated }).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap();
}

/// 造一本旧版（v16）产物：旧标记、cangjie-wash.css、cj- 类、重复 [N]、一张 deflate 压缩的图。
fn legacy_book(dir: &Path, marker: bool) -> std::path::PathBuf {
    let p = dir.join("old.epub");
    let mut files = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#.to_vec()),
        ("OEBPS/content.opf", br#"<package version="3.0" unique-identifier="id"><metadata><dc:identifier id="id">x</dc:identifier><dc:title>t</dc:title><dc:language>zh</dc:language></metadata><manifest><item id="c1" href="Text/c1.xhtml" media-type="application/xhtml+xml"/><item id="i1" href="Images/a.jpg" media-type="image/jpeg"/><item id="cangjie-wash-css" href="cangjie-wash.css" media-type="text/css"/></manifest><spine><itemref idref="c1"/></spine></package>"#.to_vec()),
        ("OEBPS/Text/c1.xhtml", r##"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title><link href="../cangjie-wash.css" rel="stylesheet" type="text/css"/></head><body><h1>第一章</h1><div class="cj-flush">第一段</div><p class="cj-center">中</p><p>注<a href="#n1">1</a> <a href="#n1">[1]</a></p><p><img src="../Images/a.jpg" alt=""/></p><div id="n1" class="cj-note">注文</div></body></html>"##.as_bytes().to_vec()),
        ("OEBPS/Images/a.jpg", jpeg(64, 96)),
        ("OEBPS/cangjie-wash.css", b"p{text-indent:2em;}\n.cj-center{text-align:center;}\n.cj-flush{text-indent:0.01em;}\n.cj-tpx{}\n".to_vec()),
    ];
    if marker {
        files.push((LEGACY_MARKER, b"16".to_vec()));
    }
    write_zip(&p, &files);
    p
}

fn entries(p: &Path) -> Vec<Ent> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let (m, crc) = (f.compression(), f.crc32());
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.name().to_string(), v, m, crc)
        })
        .collect()
}

fn get(v: &[Ent], n: &str) -> Option<Ent> {
    v.iter().find(|e| e.0 == n).cloned()
}

fn text_of(v: &[Ent], n: &str) -> String {
    String::from_utf8(get(v, n).unwrap().1).unwrap()
}

#[test]
fn preprocess_translates_legacy_book_and_copies_images_raw() {
    let t = tempfile::tempdir().unwrap();
    let src = legacy_book(t.path(), true);
    assert_eq!(legacy_version_file(&src).as_deref(), Some("16"));
    let out = t.path().join("new.epub");
    let rep = preprocess_file(&src, &out).unwrap().expect("有旧标记要处理");
    assert_eq!(rep, Report { html_files: 1, classes_renamed: 3, counters_stripped: 1, wash_css_renamed: true });
    assert_eq!(legacy_version_file(&out), None, "旧标记删掉");
    let (old, new) = (entries(&src), entries(&out));
    assert_eq!(new[0].0, "mimetype");
    assert!(get(&new, "OEBPS/cangjie-wash.css").is_none());
    let css = text_of(&new, "OEBPS/eink-wash.css");
    assert!(css.contains(".eink-center{") && css.contains(".eink-flush{") && css.contains(".cj-tpx{}"), "{css}");
    let opf = text_of(&new, "OEBPS/content.opf");
    assert!(opf.contains(r#"<item id="eink-wash-css" href="eink-wash.css" media-type="text/css"/>"#), "{opf}");
    let c1 = text_of(&new, "OEBPS/Text/c1.xhtml");
    assert!(c1.contains(r#"href="../eink-wash.css""#) && !c1.contains("cj-") && !c1.contains("[1]"), "{c1}");
    assert_eq!(html::plain_text(&c1), html::plain_text(&text_of(&old, "OEBPS/Text/c1.xhtml")).replacen(" [1]", "", 1), "可见文字只少重复的 [1]");
    let (oi, ni) = (get(&old, "OEBPS/Images/a.jpg").unwrap(), get(&new, "OEBPS/Images/a.jpg").unwrap());
    assert_eq!((oi.1, oi.2, oi.3), (ni.1, ni.2, ni.3), "图片原样拷贝（压缩方式、CRC 都不变）");
}

#[test]
fn preprocess_skips_books_without_legacy_marker() {
    let t = tempfile::tempdir().unwrap();
    let src = legacy_book(t.path(), false);
    let out = t.path().join("new.epub");
    assert_eq!(preprocess_file(&src, &out).unwrap(), None);
    assert!(!out.exists(), "没有旧标记不碰输出");
}

/// 端到端：预处理后交给 sheng-ren 优化器（xochitl 阅读模式），只有一份洗书样式表、没有旧类名、可见文字还在、过质量门。
#[test]
fn preprocessed_book_optimizes_cleanly_with_sheng_ren() {
    let t = tempfile::tempdir().unwrap();
    let src = legacy_book(t.path(), true);
    let mid = t.path().join("mid.epub");
    preprocess_file(&src, &mid).unwrap().unwrap();
    let out = t.path().join("out.epub");
    let opts = bookconv::optimize::OptimizeOpts::for_profile(profile::get("xochitl").unwrap());
    bookconv::optimize::optimize_epub_file_streaming(&mid, &out, &opts, |_, _| {}).unwrap();
    let new = entries(&out);
    let css: Vec<_> = new.iter().filter(|e| e.0.ends_with(".css")).map(|e| e.0.clone()).collect();
    assert_eq!(css, vec!["OEBPS/eink-wash.css".to_string()], "只有一份洗书样式表");
    assert!(new.iter().all(|e| !e.0.contains("cangjie") && e.0 != LEGACY_MARKER));
    let c1 = text_of(&new, "OEBPS/Text/c1.xhtml");
    assert!(!c1.contains("cj-") && c1.matches("eink-wash.css").count() == 1, "{c1}");
    let txt = html::plain_text(&c1);
    assert!(txt.contains("第一段") && txt.contains("注文") && !txt.contains("[1]"), "{txt}");
    assert_eq!(bookconv::optimize::optimized_version_file(&out).as_deref(), Some(bookconv::optimize::OPTIMIZE_VERSION));
    let check = bookconv::check::check_epub_file(&out).unwrap();
    assert!(check.ok, "{:?}", check.errors);
}

/// 旧版最小页边距漫画：v16 + 文字页带 cj-tp + 页框 952×1457 才认；旧页框（屏幕比例）、v14、文字页没留边的都不认。
#[test]
fn legacy_min_margin_comic_detection() {
    let t = tempfile::tempdir().unwrap();
    let book = |name: &str, version: &str, frame: (u32, u32), text_class: &str| {
        let p = t.path().join(name);
        let n = 22;
        let items: String = (1..=n).map(|i| format!(r#"<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/><item id="p{i}" href="p{i}.jpg" media-type="image/jpeg"/>"#)).collect();
        let spine: String = std::iter::once(r#"<itemref idref="t"/>"#.to_string()).chain((1..=n).map(|i| format!(r#"<itemref idref="c{i}"/>"#))).collect();
        let mut files = vec![
            ("mimetype".to_string(), b"application/epub+zip".to_vec()),
            ("META-INF/container.xml".to_string(), br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#.to_vec()),
            ("content.opf".to_string(), format!(r#"<package version="3.0"><metadata><dc:title>漫画</dc:title></metadata><manifest><item id="t" href="t.xhtml" media-type="application/xhtml+xml"/>{items}</manifest><spine>{spine}</spine></package>"#).into_bytes()),
            ("t.xhtml".to_string(), format!(r#"<html><body{text_class}><p>版权页</p></body></html>"#).into_bytes()),
            (LEGACY_MARKER.to_string(), version.as_bytes().to_vec()),
        ];
        let page = jpeg(frame.0, frame.1);
        for i in 1..=n {
            files.push((format!("c{i}.xhtml"), format!(r#"<html><body><img src="p{i}.jpg"/></body></html>"#).into_bytes()));
            files.push((format!("p{i}.jpg"), page.clone()));
        }
        let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        write_zip(&p, &refs);
        p
    };
    assert!(is_legacy_min_margin_comic_file(&book("ok.epub", "16", (952, 1457), r#" class="cj-tp""#)));
    assert!(is_legacy_min_margin_comic_file(&book("old-canvas.epub", "15", (954, 1458), r#" class="cj-tp""#)));
    assert!(!is_legacy_min_margin_comic_file(&book("screen.epub", "16", (954, 1696), r#" class="cj-tp""#)), "屏幕比例页框不认");
    assert!(!is_legacy_min_margin_comic_file(&book("v14.epub", "14", (952, 1457), r#" class="cj-tp""#)), "v15 之前没有这个页框");
    assert!(!is_legacy_min_margin_comic_file(&book("core.epub", "16-core", (952, 1457), r#" class="cj-tp""#)), "没清洗的不认");
    assert!(!is_legacy_min_margin_comic_file(&book("bare.epub", "16", (952, 1457), "")), "文字页没留边不认");
}
