//! ★ 检测真机 fixture 回归：与 Python 原型逐结果对拍的锚点。
//!
//! fixture=《缺失功能》第2页（pkm-semantic/proto/testdata/，用户红笔画的星+红干扰+黑笔记）。
//! 锁死两件事：① vendored remarkable_lines 的新格式补丁（ParagraphStyle::Unknown + 块少读跳过），
//! ② RED 门控 + 星形几何检出 6 个红星（与 star_scan.py 全等）。

use std::path::PathBuf;

use pkm_device::stardetect::{
    read_strokes, render_todo_html, results_fingerprint, scan_library, scan_page, StarConfig,
};

fn star_page() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pkm-semantic/proto/testdata")
        .join("9f71a39c-ca3f-49fb-828a-ef99f62596e5")
        .join("52638c34-7825-4039-a79b-c0a3ed5ffb46.rm")
}

#[test]
fn newer_format_parses_all_strokes() {
    let p = star_page();
    if !p.exists() {
        eprintln!("跳过：缺 fixture {p:?}");
        return;
    }
    let bytes = std::fs::read(&p).unwrap();
    let strokes = read_strokes(&bytes).expect("新格式 .rm 应能解析（补丁生效）");
    // 与 rmscene 一致：200 笔，155 黑 45 红
    assert_eq!(strokes.len(), 200, "笔划总数应为 200");
    let red = strokes.iter().filter(|s| s.color == "RED").count();
    assert_eq!(red, 45, "红色笔划应为 45");
}

#[test]
fn red_gate_detects_six_stars() {
    let p = star_page();
    if !p.exists() {
        return;
    }
    let bytes = std::fs::read(&p).unwrap();
    let cfg = StarConfig {
        todo_colors: Some(vec!["RED".into()]),
        ..Default::default()
    };
    // 与 Python star_scan（gap=25）全等：6 个红星
    assert_eq!(scan_page(&bytes, &cfg, 25.0), 6);
    // 未使用的笔色 → 0
    let green = StarConfig {
        todo_colors: Some(vec!["GREEN".into()]),
        ..Default::default()
    };
    assert_eq!(scan_page(&bytes, &green, 25.0), 0);
}

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pkm-semantic/proto/testdata")
}

#[test]
fn scan_library_finds_one_doc() {
    let dir = testdata_dir();
    if !dir.exists() {
        return;
    }
    let cfg = StarConfig {
        todo_colors: Some(vec!["RED".into()]),
        ..Default::default()
    };
    let docs = scan_library(&dir, &cfg, 25.0);
    assert_eq!(docs.len(), 1, "应扫到 1 本有星文档");
    assert_eq!(docs[0].title, "缺失功能");
    let total: u32 = docs[0].hits.iter().map(|h| h.count).sum();
    assert_eq!(total, 6, "红星总数应为 6");
}

#[test]
fn fingerprint_stable_and_sensitive() {
    let dir = testdata_dir();
    if !dir.exists() {
        return;
    }
    let cfg = StarConfig {
        todo_colors: Some(vec!["RED".into()]),
        ..Default::default()
    };
    let a = scan_library(&dir, &cfg, 25.0);
    let b = scan_library(&dir, &cfg, 25.0);
    assert_eq!(results_fingerprint(&a), results_fingerprint(&b), "同输入指纹应稳定");
    // 空结果的指纹与非空不同
    assert_ne!(results_fingerprint(&a), results_fingerprint(&[]));
}

#[test]
fn todo_epub_assembles_to_valid_zip() {
    use weread_device::epub::{assemble, Book, BookMeta, Chapter};
    let dir = testdata_dir();
    let docs = if dir.exists() {
        let cfg = StarConfig {
            todo_colors: Some(vec!["RED".into()]),
            ..Default::default()
        };
        scan_library(&dir, &cfg, 25.0)
    } else {
        vec![]
    };
    let mut book = Book {
        meta: BookMeta {
            book_id: "cangjie-star-todo".into(),
            title: "★ 全局待办".into(),
            author: "墨香".into(),
            language: "zh".into(),
            publisher: "cangjie".into(),
            cover: None,
            cover_ext: String::new(),
            cover_media_type: String::new(),
        },
        chapters: vec![Chapter {
            title: "★ 全局待办".into(),
            html_body: render_todo_html(&docs),
            level: 1,
        }],
    };
    let bytes = assemble(&mut book).expect("EPUB 应组装成功");
    assert!(bytes.len() > 100, "EPUB 非空");
    assert_eq!(&bytes[0..2], b"PK", "zip 魔数");
    // mimetype 必须首条且为 application/epub+zip
    let head = String::from_utf8_lossy(&bytes[..60]);
    assert!(head.contains("mimetypeapplication/epub+zip"), "mimetype 首条 STORED");
}
