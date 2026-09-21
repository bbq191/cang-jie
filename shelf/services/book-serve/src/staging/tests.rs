//! 母版库单测（原 `staging.rs` 内联的 `mod tests`；夹具（假 xochitl、漫画 EPUB 等）跨入库/优化/落库共用，集中放这里）。
use super::*;
use rmsvc_core::asset::AssetUploadFlow;

fn staging(t: &tempfile::TempDir) -> Staging {
    let x = Arc::new(Xochitl::new("127.0.0.1:1", Path::new("/nonexistent"), 1));
    let s = Staging::new(t.path().join("staging"), x, 1024 * 1024);
    s.ensure().unwrap();
    s
}

/// 测试用的 mkdir 队列——这些 deliver 测试全部传空 folder（`ensure_folder` 见到空串直接短路
/// 返回，压根不会碰 mkdir），队列本身指哪个临时目录不重要，只要类型对得上。
fn empty_mkdir(t: &tempfile::TempDir) -> MkdirQueue {
    MkdirQueue::new(&t.path().join("state"), &t.path().join("xochitl"))
}

fn mini_epub(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("mimetype", o).unwrap();
        zw.write_all(b"application/epub+zip").unwrap();
        for (n, d) in files {
            zw.start_file(*n, o).unwrap();
            zw.write_all(d.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

#[test]
fn put_list_read_remove() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    assert_eq!(s.stage_new("a.epub", b"not-a-zip").unwrap(), "a.epub");
    assert_eq!(s.stage_new("a.epub", b"xx").unwrap(), "1_a.epub", "同名不覆盖");
    s.stage_new("doc.pdf", b"%PDF").unwrap();
    let list = s.list();
    assert_eq!(list.len(), 3);
    let epub = list.iter().find(|e| e.name == "a.epub").unwrap();
    assert_eq!((epub.format, epub.level, epub.optimized), ("epub", "none", false), "非 zip 不该判已优化");
    assert_eq!(list.iter().find(|e| e.name == "doc.pdf").unwrap().format, "pdf");
    s.remove("a.epub").unwrap();
    assert_eq!(s.list().len(), 2);
    for bad in ["../x", ".hidden", "a/b"] {
        assert!(s.stage_new(bad, b"y").is_err(), "{bad:?} 非法名拒绝");
    }
}

/// `free_bytes` 走 statvfs 而不是 fork `df`：与 host 的 `df -k` 对拍（两次取样之间别的进程会写盘，给 64MB 容差）。
#[test]
fn free_bytes_matches_df_and_is_none_for_missing_dir() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let got = s.free_bytes().expect("临时目录所在分区应可查");
    assert!(got > 0);
    if let Ok(out) = std::process::Command::new("df").arg("-Pk").arg(s.dir()).output() {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        // POSIX 格式：表头后一行，第 4 列 = Available (KB)
        if let Some(kb) = text.lines().nth(1).and_then(|l| l.split_whitespace().nth(3)).and_then(|v| v.parse::<u64>().ok()) {
            assert!(got.abs_diff(kb * 1024) < 64 * 1024 * 1024, "statvfs {got} vs df {}", kb * 1024);
        }
    }
    let gone = Staging::new(t.path().join("no/such/dir"), Arc::new(Xochitl::new("127.0.0.1:1", Path::new("/nonexistent"), 1)), 0);
    assert_eq!(gone.free_bytes(), None);
}

#[test]
fn delivered_record_roundtrip_and_reader_parse() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("b.epub", b"x").unwrap();
    assert!(s.list()[0].delivered.is_none(), "未落库无记录");
    s.mark_delivered("b.epub", Reader::Native).unwrap();
    s.mark_delivered("b.epub", Reader::Koreader).unwrap();
    let d = s.list()[0].delivered.clone().unwrap();
    assert!(d.native.is_some() && d.koreader.is_some());
    assert_eq!(s.list().len(), 1, "sidecar 不当条目列出");
    assert!(Reader::parse("nowhere").is_err() && Reader::parse("native").is_ok());
    assert!(s.mark_delivered("nope.epub", Reader::Native).is_err(), "不存在的书拒绝");
    s.remove("b.epub").unwrap();
    assert!(!sidecar::path_for(&s.dir().join("b.epub")).exists(), "删书连带删 sidecar");
}

#[test]
fn optimize_gates_and_runs_single_full_pass() {
    // 2026-09-19 用户明确要求去掉"优化分档位"——不再有 Plain/KeepSpacing 两档，`optimize()`
    // 永远跑完整的清洗+优化（原来标"推荐"的那档，也是绝大多数书要的那档）。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", r#"<html><head></head><body><h1>第一章</h1><p style="font-size:9px;margin:1em">正文</p></body></html>"#)]);
    s.stage_new("x.epub", &epub).unwrap();
    // 2026-09-19 起 PDF 也能「优化」（入库 PDF 线，见 `optimize_pdf`）——这份 `%PDF` 字面量
    // 不是真实可解析的 PDF 结构，走到 `pdf_ingest::classify_pdf` 会解析失败，这里只断言
    // "确实报错、不是静默成功"，不再断言旧版"PDF 一律不支持"那句文案。
    s.stage_new("p.pdf", b"%PDF").unwrap();
    assert!(s.optimize("p.pdf", |_, _| {}).is_err());
    assert!(s.optimize("none.epub", |_, _| {}).is_err());
    assert!(s.optimize("x.cbz", |_, _| {}).is_err()); // 格式白名单之外的仍然拒绝

    let mut progresses = Vec::new();
    let msg = s.optimize("x.epub", |done, total| progresses.push((done, total))).unwrap();
    assert!(msg.contains("清洗+优化") && msg.contains("自动目录 1 条"), "{msg}");
    assert!(!progresses.is_empty(), "on_progress 应该原样转发自 optimize_epub_file_streaming");
    assert_eq!(progresses.last().unwrap().0, progresses.last().unwrap().1, "最后一次回调应该是 done==total");
    let e = s.list().into_iter().find(|e| e.name == "x.epub").unwrap();
    assert!(e.optimized && e.level == "full");
}

#[test]
fn optimize_temp_file_dot_prefixed_so_list_does_not_surface_mid_flight_product() {
    // 真机回归（2026-09-19，《镖人》552MB 全集）：流式优化耗时到分钟级，临时产物在目录里存在
    // 的时间不再是"同步写一次内存 buffer"那种毫秒级窗口——之前用不带点前缀的命名，真机
    // `GET /staging` 撞见过一条 `format:"other"` 的 `....epub.optimizing.tmp` 离谱条目。
    // 这里不模拟并发时序（太脆），直接断言临时产物命名遵循 `list()` 已有的点前缀过滤规则。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("x.epub", b"PK").unwrap();
    std::fs::write(s.dir().join(".x.epub.optimizing.tmp"), b"partial").unwrap();
    let names: Vec<String> = s.list().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["x.epub".to_string()], "优化中途产物不该出现在列表里: {names:?}");
}

#[test]
fn busy_lock_blocks_second_start_and_conflicting_delete_deliver() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("x.epub", b"PK").unwrap();
    assert!(s.try_start_busy("x.epub"), "第一次加锁应该成功");
    assert!(!s.try_start_busy("x.epub"), "已经忙着，第二次应该失败");
    assert!(s.is_busy("x.epub"));
    assert!(s.remove("x.epub").unwrap_err().contains("正在处理中"), "忙的时候不该能删");
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    assert!(s.spawn_deliver("x.epub", "", Arc::new(empty_mkdir(&t)), bus).unwrap_err().contains("正在处理中"), "忙的时候不该能起第二个落库");
    assert_eq!(s.list().iter().find(|e| e.name == "x.epub").unwrap().busy, true, "GET /staging 列表应体现 busy");
    s.end_busy("x.epub");
    assert!(!s.is_busy("x.epub"));
    assert!(s.remove("x.epub").is_ok(), "解锁后恢复正常");
}

#[test]
fn spawn_deliver_runs_in_background_records_result_then_clears_busy() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t); // xochitl 指向不可达地址（见 staging() 测试 helper），deliver 必然失败——够测异步管线本身
    s.stage_new("d.pdf", &vec![b'%'; 10]).unwrap();
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    s.spawn_deliver("d.pdf", "", Arc::new(empty_mkdir(&t)), bus).unwrap();
    assert!(s.is_busy("d.pdf"), "spawn 返回时忙锁应已生效");
    let bus2 = Arc::new(rmsvc_core::events::EventBus::new());
    assert!(s.spawn_deliver("d.pdf", "", Arc::new(empty_mkdir(&t)), bus2).unwrap_err().contains("正在处理中"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while s.is_busy("d.pdf") && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!s.is_busy("d.pdf"), "后台线程应该在超时前跑完并清忙锁");
    let e = s.list().into_iter().find(|e| e.name == "d.pdf").unwrap();
    assert!(!e.busy);
    let dc = e.delivered.and_then(|d| d.deliver).expect("应该写了异步落库结果");
    assert_eq!(dc.status, "failed", "测试环境 xochitl 不可达，落库必然失败");
}

#[test]
fn spawn_deliver_rejects_bad_format_synchronously_without_busy_lock() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("c.cbz", b"PK").unwrap();
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    assert!(s.spawn_deliver("c.cbz", "", Arc::new(empty_mkdir(&t)), bus.clone()).unwrap_err().contains("只读 EPUB / PDF"));
    assert!(!s.is_busy("c.cbz"), "校验失败不该留下忙锁");
    assert!(s.spawn_deliver("none.epub", "", Arc::new(empty_mkdir(&t)), bus).is_err());
}

#[test]
fn spawn_optimize_runs_in_background_and_records_result_then_clears_busy() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", "<html><head></head><body><h1>第一章</h1><p>正文</p></body></html>")]);
    s.stage_new("x.epub", &epub).unwrap();
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    s.spawn_optimize("x.epub", bus).unwrap();
    // 起线程那一刻就该忙（同步部分：格式校验+加锁，不依赖线程调度时机）
    assert!(s.is_busy("x.epub"), "spawn 返回时忙锁应已生效");
    // 忙着的时候第二次调用应该被拒绝，不会排队/覆盖
    let bus2 = Arc::new(rmsvc_core::events::EventBus::new());
    assert!(s.spawn_optimize("x.epub", bus2).unwrap_err().contains("正在处理中"));
    // 等后台线程跑完（真实测试书毫秒级，给足超时兜底 CI 慢机器）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while s.is_busy("x.epub") && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!s.is_busy("x.epub"), "后台线程应该在超时前跑完并清忙锁");
    let e = s.list().into_iter().find(|e| e.name == "x.epub").unwrap();
    assert!(!e.busy);
    let oc = e.delivered.and_then(|d| d.optimize).expect("应该写了异步优化结果");
    assert_eq!(oc.status, "ok");
    assert!(oc.message.contains("已优化"), "{}", oc.message);
}

#[test]
fn spawn_optimize_rejects_non_epub_and_missing_file_synchronously() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("x.cbz", b"not a real cbz").unwrap();
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    // 格式白名单之外（CBZ 等）依然同步拒绝，不占忙锁——PDF 2026-09-19 起已经在白名单内，
    // 不能再拿它当"非法格式"的例子（见 `optimize_gates_and_runs_single_full_pass`）。
    assert!(s.spawn_optimize("x.cbz", bus.clone()).unwrap_err().contains("只有 EPUB/PDF"));
    assert!(!s.is_busy("x.cbz"), "校验失败不该留下忙锁");
    assert!(s.spawn_optimize("none.epub", bus).is_err());
}

/// 造一本 2 卷合集漫画（跟 bookconv::comic_split 测试用例同一套结构），塞进 mini_epub 装不了的
/// 二进制字节所以这里直接手搓 zip——is_comic/comic_split 只看扩展名和 NCX 结构，不校验图片
/// 内容本身是不是合法 JPEG，够测这条集成路径。
fn multivol_comic_epub(pages_per_vol: &[usize]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut navpoints = String::new();
    let mut page_no = 0usize;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (vi, &n) in pages_per_vol.iter().enumerate() {
        let vol_start = page_no;
        for _ in 0..n {
            files.push((format!("text/p{page_no:04}.html"), format!(r#"<html><body><img src="../images/{page_no:04}.jpg"/></body></html>"#).into_bytes()));
            files.push((format!("images/{page_no:04}.jpg"), vec![7u8; 200]));
            manifest += &format!(r#"<item id="h{page_no}" href="text/p{page_no:04}.html" media-type="application/xhtml+xml"/><item id="i{page_no}" href="images/{page_no:04}.jpg" media-type="image/jpeg"/>"#);
            spine += &format!(r#"<itemref idref="h{page_no}"/>"#);
            page_no += 1;
        }
        navpoints += &format!(r#"<navPoint id="nv{vi}"><navLabel><text>卷{vi}</text></navLabel><content src="text/p{vol_start:04}.html"/></navPoint>"#);
    }
    files.push(("content.opf".into(), format!(r#"<package version="3.0"><metadata><dc:title>t</dc:title></metadata><manifest>{manifest}<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx">{spine}</spine></package>"#).into_bytes()));
    files.push(("toc.ncx".into(), format!(r#"<ncx><navMap>{navpoints}</navMap></ncx>"#).into_bytes()));
    files.push(("META-INF/container.xml".into(), br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#.to_vec()));
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zw.start_file("mimetype", o).unwrap();
    zw.write_all(b"application/epub+zip").unwrap();
    for (n, d) in files {
        zw.start_file(n, o).unwrap();
        zw.write_all(&d).unwrap();
    }
    zw.finish().unwrap();
    buf
}

#[test]
fn deliver_oversized_epub_comic_attempts_split_instead_of_flat_reject() {
    // 25+ 张图满足 is_comic 阈值（2 卷各 15 张，每页 html+img 约 200B）；native_limit 给 1KB
    // 逼近强制超限，触发拆分路径。测试环境 Xochitl 指向不可达地址，upload() 必然失败，验证不了
    // 真正投递成功——那部分已经在真机+bookconv 单测（`comic_split::tests::against_real_naruto_book`）
    // 分别验证过；这里只验证 deliver() 确实走了"按卷拆分尝试"这条新路径，不是笼统整本拒绝。
    let t = tempfile::tempdir().unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", Path::new("/nonexistent"), 1));
    let s = Staging::new(t.path().join("staging"), x, 1024);
    s.ensure().unwrap();
    let epub = multivol_comic_epub(&[15, 15]);
    s.stage_new("manga.epub", &epub).unwrap();
    let err = s.deliver("manga.epub", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err();
    assert!(err.contains("按卷拆分") || err.contains("上传失败") || err.contains("拆分后一份都没能投上"), "应该走拆分路径而不是整本拒绝: {err}");
    assert!(err.contains("卷0") || err.contains("卷1"), "错误信息应该点出具体是哪一卷: {err}");
}

/// SOI+SOF0(16x16,3分量)+EOI 最小 JPEG 骨架——`imgopt::trim_margins`/`downscale_for_epub_comic`
/// 真解码会失败（没有 SOS/熵编码数据），两者都优雅降级回原字节（`unwrap_or`），不 panic 不报错；
/// `pdfwrite::image_from_bytes` 只解析 SOF 段拿宽高、原字节直嵌，不需要真解码，够用。
fn fake_jpeg() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03, 0x01, 0x11,
        0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xD9,
    ]
}

/// 带 NCX 目录、真图片字节的漫画 EPUB——给"优化改产出 PDF"这条新路径用的测试夹具，跟
/// `multivol_comic_epub`（图片纯占位、拆分不解码）区别在于图片是真能被 `pdfwrite::
/// image_from_bytes` 编进去的字节。
fn comic_epub_with_real_images(pages_per_vol: &[usize]) -> Vec<u8> {
    use std::io::Write;
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut navpoints = String::new();
    let mut page_no = 0usize;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let jpeg = fake_jpeg();
    for (vi, &n) in pages_per_vol.iter().enumerate() {
        let vol_start = page_no;
        for _ in 0..n {
            files.push((format!("text/p{page_no:04}.html"), format!(r#"<html><body><img src="../images/{page_no:04}.jpg"/></body></html>"#).into_bytes()));
            files.push((format!("images/{page_no:04}.jpg"), jpeg.clone()));
            manifest += &format!(r#"<item id="h{page_no}" href="text/p{page_no:04}.html" media-type="application/xhtml+xml"/><item id="i{page_no}" href="images/{page_no:04}.jpg" media-type="image/jpeg"/>"#);
            spine += &format!(r#"<itemref idref="h{page_no}"/>"#);
            page_no += 1;
        }
        navpoints += &format!(r#"<navPoint id="nv{vi}"><navLabel><text>卷{vi}</text></navLabel><content src="text/p{vol_start:04}.html"/></navPoint>"#);
    }
    files.push(("content.opf".into(), format!(r#"<package version="3.0"><metadata><dc:title>t</dc:title></metadata><manifest>{manifest}<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx">{spine}</spine></package>"#).into_bytes()));
    files.push(("toc.ncx".into(), format!(r#"<ncx><navMap>{navpoints}</navMap></ncx>"#).into_bytes()));
    files.push(("META-INF/container.xml".into(), br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#.to_vec()));
    let mut buf = Vec::new();
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zw.start_file("mimetype", o).unwrap();
    zw.write_all(b"application/epub+zip").unwrap();
    for (n, d) in files {
        zw.start_file(n, o).unwrap();
        zw.write_all(&d).unwrap();
    }
    zw.finish().unwrap();
    buf
}

#[test]
fn stage_new_names_epub_as_title_dash_volume_number_first() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let epub = comic_epub_with_real_images(&[12, 13]);
    let landed = s.stage_new("鏢人 - 卷02 -- 許先哲 -- 鏢人 - 卷02, 2022 -- Mox_moe -- df4a0842 -- Anna’s Archive.epub", &epub).unwrap();
    assert_eq!(landed, "鏢人 - 02卷.epub");
    // 非 EPUB 不动名字。
    assert_eq!(s.stage_new("x -- y.pdf", b"%PDF-1.4").unwrap(), "x -- y.pdf");
}

#[test]
fn optimize_renames_existing_long_epub_and_keeps_sidecar() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let long = "亂馬1⁄2 典藏版 - 19卷 -- 高橋留美子 -- 19, 2019 -- 尖端 -- 03220cf1 -- Anna’s Archive.epub";
    std::fs::write(t.path().join("staging").join(long), comic_epub_with_real_images(&[12, 13])).unwrap();
    s.mark_delivered(long, Reader::Koreader).unwrap();
    let msg = s.optimize(long, |_, _| {}).unwrap();
    assert!(msg.contains("亂馬1⁄2 典藏版 - 19卷"), "{msg}");
    let list = s.list();
    assert_eq!(list.len(), 1, "只该有一条: {:?}", list.iter().map(|e| &e.name).collect::<Vec<_>>());
    assert_eq!(list[0].name, "亂馬1⁄2 典藏版 - 19卷.epub");
    assert!(list[0].delivered.is_some(), "落库记录（边车）必须跟着改名，不能丢");
    assert!(list[0].optimized);
}

#[test]
fn optimize_sets_dc_title_to_canonical_name_for_volume_books() {
    // 设备显示名取 EPUB 的 dc:title：有卷标记的书，优化后 dc:title 必须是规范名，跟文件名一致。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let long = "鏢人 - 卷02 -- 許先哲 -- Anna’s Archive.epub";
    std::fs::write(t.path().join("staging").join(long), comic_epub_with_real_images(&[12, 13])).unwrap();
    s.optimize(long, |_, _| {}).unwrap();
    let bytes = std::fs::read(t.path().join("staging").join("鏢人 - 02卷.epub")).unwrap();
    let entries = bookconv::check::read_entries(&bytes).unwrap();
    let opf = entries.iter().find(|e| e.name.ends_with(".opf")).unwrap();
    assert!(String::from_utf8_lossy(&opf.data).contains("<dc:title>鏢人 - 02卷</dc:title>"), "dc:title 应为规范名");
}

#[test]
fn optimize_keeps_original_name_when_canonical_target_exists() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let dir = t.path().join("staging");
    let long = "书 - 01卷 -- 作者 -- Anna’s Archive.epub";
    std::fs::write(dir.join(long), comic_epub_with_real_images(&[12, 13])).unwrap();
    std::fs::write(dir.join("书 - 01卷.epub"), comic_epub_with_real_images(&[12, 13])).unwrap();
    s.optimize(long, |_, _| {}).unwrap();
    let names: Vec<String> = s.list().into_iter().map(|e| e.name).collect();
    assert!(names.contains(&long.to_string()) && names.contains(&"书 - 01卷.epub".to_string()), "重复的同一卷不能互相覆盖: {names:?}");
}

#[test]
fn onopen_render_record_upgrades_to_ok_once_xochitl_rewrites_page_count() {
    // 直接投入的 EPUB：记 onopen + 占位页数(2)；用户打开后 xochitl 把 .content 的 pageCount 改成 351 → 列表自动显示 ok/351。
    let t = tempfile::tempdir().unwrap();
    let lib = t.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1));
    let s = Staging::new(t.path().join("staging"), x, 1024 * 1024);
    s.ensure().unwrap();
    s.stage_new("big.epub", &comic_epub_with_real_images(&[12, 13])).unwrap();
    std::fs::write(lib.join("u1.content"), r#"{"pageCount":2}"#).unwrap();
    s.set_render("big.epub", sidecar::RenderCheck { uuid: "u1".into(), pages: 2, expected: 0, status: "onopen".into(), at: 1 }).unwrap();
    let rc = |s: &Staging| s.list()[0].delivered.clone().unwrap().render.unwrap();
    assert_eq!((rc(&s).status.as_str(), rc(&s).pages), ("onopen", 2), "没打开过：保持 onopen");
    std::fs::write(lib.join("u1.content"), r#"{"pageCount":351}"#).unwrap();
    assert_eq!((rc(&s).status.as_str(), rc(&s).pages), ("ok", 351), "打开过（页数变了）：升级成真页数");
}

#[test]
fn new_book_does_not_inherit_orphan_sidecar_and_gc_removes_orphans() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    // 旧书被外部删掉（没走 remove）→ 留下孤儿边车
    let name = s.stage_new("a.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>x</p>")])).unwrap();
    s.mark_delivered(&name, Reader::Native).unwrap();
    std::fs::remove_file(s.dir.join(&name)).unwrap();
    assert!(sidecar::path_for(&s.dir.join(&name)).exists());
    // 同名新书入库：不能带着旧的"已加入"
    let name2 = s.stage_new("a.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>y</p>")])).unwrap();
    assert_eq!(name2, name);
    assert!(s.list()[0].delivered.is_none(), "新书不继承孤儿边车");
    // 启动清理：只清孤儿，不动有书的边车
    s.mark_delivered(&name2, Reader::Native).unwrap();
    std::fs::write(s.dir.join(".gone.epub.delivered"), b"{}").unwrap();
    assert_eq!(s.gc_orphan_sidecars(), 1);
    assert!(sidecar::path_for(&s.dir.join(&name2)).exists());
    assert!(!s.dir.join(".gone.epub.delivered").exists());
}

#[test]
fn recover_interrupted_fixes_stale_pending_and_removes_tmp() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let name = s.stage_new("a.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>x</p>")])).unwrap();
    s.set_optimize_check(&name, sidecar::OptimizeCheck { status: "pending".into(), ..Default::default() }).unwrap();
    s.set_deliver_check(&name, sidecar::DeliverCheck { status: "pending".into(), ..Default::default() }).unwrap();
    s.set_render(&name, sidecar::RenderCheck { status: "pending".into(), ..Default::default() }).unwrap();
    let ok = s.stage_new("ok.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>y</p>")])).unwrap();
    s.set_optimize_check(&ok, sidecar::OptimizeCheck { status: "ok".into(), message: "已优化".into(), ..Default::default() }).unwrap();
    std::fs::write(s.dir.join(".a.epub.optimizing.tmp"), vec![0u8; 1000]).unwrap();
    assert_eq!(s.recover_interrupted(), (1, 1), "只修 pending 的那本，清 1 个半成品");
    let d = sidecar::read(&s.dir.join(&name)).unwrap();
    assert_eq!(d.optimize.unwrap().status, "failed");
    assert_eq!(d.deliver.unwrap().status, "failed");
    assert_eq!(d.render.unwrap().status, "timeout");
    assert_eq!(sidecar::read(&s.dir.join(&ok)).unwrap().optimize.unwrap().message, "已优化", "已完成的记录不动");
    assert!(!s.dir.join(".a.epub.optimizing.tmp").exists());
    assert_eq!(s.recover_interrupted(), (0, 0), "幂等");
}

#[test]
fn list_caches_level_probe_and_invalidates_on_rewrite_or_delete() {
    // 判定要开 zip（吃 CPU/电），按（大小,mtime）缓存；文件改写后必须重判，删除后清缓存。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let plain = mini_epub(&[("OEBPS/a.xhtml", "<p>x</p>")]);
    s.stage_new("b.epub", &plain).unwrap();
    assert_eq!(s.list()[0].level, "none");
    assert_eq!(crate::ops::lock(&s.probes).len(), 1, "首次列表写入缓存");
    // 篡改缓存里的结论：若第二次列表仍用它，说明命中缓存没有重开文件
    crate::ops::lock(&s.probes).get_mut("b.epub").unwrap().level = "full";
    assert_eq!(s.list()[0].level, "full", "文件没变 → 命中缓存");
    // 文件改写（内容长度变了）→ 缓存失效重判
    let marked = mini_epub(&[("OEBPS/a.xhtml", "<p>x</p>"), (optimize::OPTIMIZE_MARKER, optimize::OPTIMIZE_VERSION)]);
    std::fs::write(s.dir.join("b.epub"), &marked).unwrap();
    assert_eq!(s.list()[0].level, "full");
    std::fs::write(s.dir.join("b.epub"), &plain).unwrap();
    assert_eq!(s.list()[0].level, "none", "改写回未优化 → 重判");
    std::fs::remove_file(s.dir.join("b.epub")).unwrap();
    assert!(s.list().is_empty());
    assert!(crate::ops::lock(&s.probes).is_empty(), "条目消失 → 清缓存");
}

#[test]
fn backfill_claims_delivered_books_without_render_record_by_name_and_size() {
    let t = tempfile::tempdir().unwrap();
    let lib = t.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1));
    let s = Staging::new(t.path().join("staging"), x, 1024 * 1024);
    s.ensure().unwrap();
    let epub = comic_epub_with_real_images(&[12, 13]);
    for n in ["镖人 - 二卷.epub", "镖人 - 三卷.epub"] {
        s.stage_new(n, &epub).unwrap();
        s.mark_delivered(n, Reader::Native).unwrap();
    }
    let mk = |uuid: &str, name: &str, opened: bool| {
        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{name}","parent":"","createdTime":"{}"}}"#, rmsvc_core::clock::now_ms())).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), &epub).unwrap();
        std::fs::write(lib.join(format!("{uuid}.content")), if opened { r#"{"pageCount":264}"# } else { r#"{"pageCount":2}"# }).unwrap();
        if opened {
            std::fs::write(lib.join(format!("{uuid}.pdf")), b"render").unwrap();
        }
    };
    mk("u-unopened", "镖人 - 二卷", false);
    mk("u-opened", "镖人 - 三卷", true);
    assert_eq!(s.backfill_render_records(), 2);
    let get = |name: &str| s.list().into_iter().find(|e| e.name == name).unwrap().delivered.unwrap().render.unwrap();
    assert_eq!((get("镖人 - 二卷.epub").status.as_str(), get("镖人 - 二卷.epub").pages), ("onopen", 2));
    assert_eq!((get("镖人 - 三卷.epub").status.as_str(), get("镖人 - 三卷.epub").pages), ("ok", 264));
    assert_eq!(s.backfill_render_records(), 0, "幂等：已有记录的不再补");
}

#[test]
fn request_cancel_requires_busy_and_cancellable_step() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    assert!(s.request_cancel("x.epub").unwrap_err().contains("没有在处理"), "没在处理的书不能取消");
    assert!(s.try_start_busy("x.epub"));
    assert_eq!(s.request_cancel("x.epub"), Ok(false), "没声明可中断的步骤（如单文件上传）如实回 false");
    assert!(!s.is_cancelled("x.epub"));
    s.mark_cancellable("x.epub");
    assert_eq!(s.request_cancel("x.epub"), Ok(true));
    assert!(s.is_cancelled("x.epub"));
    s.end_busy("x.epub");
    assert!(!s.is_cancelled("x.epub"), "操作结束后取消标记必须清掉，不能带进下一次");
    assert!(s.try_start_busy("x.epub"));
    assert!(!s.is_cancelled("x.epub"));
}

#[test]
fn optimize_cancelled_midway_leaves_book_and_no_temp_file() {
    // 取消标记已设：优化一开始就停，母版原样保留，不留 .optimizing.tmp 半成品，状态是 cancelled 而不是 failed。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let epub = comic_epub_with_real_images(&[12, 13]);
    s.stage_new("manga.epub", &epub).unwrap();
    assert!(s.try_start_busy("manga.epub"));
    s.mark_cancellable("manga.epub");
    s.request_cancel("manga.epub").unwrap();
    let err = s.optimize("manga.epub", |_, _| {}).unwrap_err();
    assert!(err.contains("已取消"), "{err}");
    let dir = t.path().join("staging");
    assert_eq!(std::fs::read(dir.join("manga.epub")).unwrap(), epub, "母版原样保留");
    assert!(!std::fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("optimizing.tmp")), "不留半成品");
}

#[test]
fn optimize_comic_epub_stays_epub() {
    // 统一规则（2026-09-20 用户拍板）：漫画「优化」也不改格式，产物仍是同名 EPUB，内容/目录原样保留。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let epub = comic_epub_with_real_images(&[12, 13]); // 25 张图，够 is_comic 阈值
    s.stage_new("manga.epub", &epub).unwrap();

    let msg = s.optimize("manga.epub", |_, _| {}).unwrap();
    assert!(!msg.contains("PDF"), "漫画不该再转 PDF: {msg}");

    let list = s.list();
    assert!(list.iter().all(|e| e.name != "manga.pdf"), "不该产出 PDF");
    let e = list.iter().find(|e| e.name == "manga.epub").expect("仍是 manga.epub");
    assert_eq!(e.format, "epub");
    assert!(e.optimized && e.level == "full", "应报已优化: {e:?}");
}

/// 拿真实 pdflatex 编译的样本（bookconv 那条线的测试夹具，两个 crate 同一个仓库共享一份
/// 真实样本，不在 book-serve 这边另造一份假数据）核对：入库有文字层的 PDF「优化」真的会
/// 转成 EPUB、原 PDF 被删、新 EPUB 在 `list()` 里报 `pdfSource: true` 且不再显示优化档位
/// 的 none（视为已完成）。
#[test]
fn optimize_text_layer_pdf_produces_epub_output() {
    const SAMPLE_PDF: &[u8] = include_bytes!("../../../../crates/bookconv/tests/fixtures/sample.pdf");
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("paper.pdf", SAMPLE_PDF).unwrap();

    let msg = s.optimize("paper.pdf", |_, _| {}).unwrap();
    assert!(msg.contains("PDF→EPUB"), "{msg}");

    let list = s.list();
    assert!(list.iter().all(|e| e.name != "paper.pdf"), "原 PDF 条目应该被替换掉");
    let epub_entry = list.iter().find(|e| e.name == "paper.epub").expect("应该产出 paper.epub");
    assert_eq!(epub_entry.format, "epub");
    assert!(epub_entry.pdf_source, "应该标记来源是 PDF 转换: {epub_entry:?}");
    assert!(epub_entry.optimized && epub_entry.level == "full", "PDF 转出的 EPUB 应该直接报已完成: {epub_entry:?}");

    let epub_bytes = std::fs::read(t.path().join("staging").join("paper.epub")).unwrap();
    assert!(!epub_bytes.is_empty());
}

/// 漫画/无文字层 PDF 走裁边分支，格式不变仍是 PDF，且能被识别成"自己优化过的"。
#[test]
fn optimize_comic_shaped_pdf_stays_pdf_and_gets_trimmed() {
    let img = bookconv::convert::pdfwrite::image_from_bytes(&fake_jpeg()).unwrap();
    let comic_pdf = bookconv::convert::pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("scan.pdf", &comic_pdf).unwrap();

    let msg = s.optimize("scan.pdf", |_, _| {}).unwrap();
    assert!(msg.contains("裁边"), "{msg}");

    let list = s.list();
    let entry = list.iter().find(|e| e.name == "scan.pdf").expect("格式不该变，还是 scan.pdf");
    assert_eq!(entry.format, "pdf");
    assert!(entry.optimized && entry.level == "full", "裁边后应该报已优化: {entry:?}");
}

#[test]
fn optimize_comic_epub_rejects_when_no_images_found() {
    // is_comic_epub_file 判定要图够多；混进正好 20+ 张图但真正 spine 引用为空的极端情况这里不测，
    // 只覆盖最直接的"根本没图"分支走不到漫画判定，仍归普通 EPUB 分支（现状行为不变）。
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", "<html><body><p>正文</p></body></html>")]);
    s.stage_new("plain.epub", &epub).unwrap();
    let msg = s.optimize("plain.epub", |_, _| {}).unwrap();
    assert!(!msg.contains("PDF"), "没有图片不该走漫画→PDF 分支: {msg}");
}

#[test]
fn deliver_oversized_comic_pdf_attempts_split_instead_of_flat_reject() {
    let t = tempfile::tempdir().unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", Path::new("/nonexistent"), 1));
    let s = Staging::new(t.path().join("staging"), x, 1024); // 1KB 预算，逼近强制超限
    s.ensure().unwrap();
    let epub = comic_epub_with_real_images(&[12, 13]);
    s.stage_new("manga.epub", &epub).unwrap();
    // 造一份"自己产出的漫画 PDF"（带书签）：用户上传的原生 PDF 没有书签、不走分卷；漫画 EPUB 的「优化」
    // 已不再转 PDF，这里直接调转换函数造夹具。
    let dir = t.path().join("staging");
    bookconv::comic_pdf::optimize_comic_epub_to_pdf_streaming(&dir.join("manga.epub"), &dir.join("manga.pdf"), |_, _| {}).unwrap();
    std::fs::remove_file(dir.join("manga.epub")).unwrap();

    let err = s.deliver("manga.pdf", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err();
    assert!(err.contains("按卷拆分") || err.contains("上传失败"), "应该走 PDF 拆分路径而不是整本拒绝: {err}");
}

#[test]
fn deliver_oversized_non_comic_epub_keeps_flat_reject() {
    let t = tempfile::tempdir().unwrap();
    let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let long_text = "正".repeat(600_000); // 600_000 * 3 字节(UTF-8) ≈ 1.7MB，确保超过测试用 1MB native_limit
    let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", &format!("<html><body><p>{long_text}</p></body></html>"))]);
    let s = staging(&t);
    s.stage_new("novel.epub", &epub).unwrap();
    let err = s.deliver("novel.epub", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err();
    assert!(err.contains("超过 xochitl 上传上限") && err.contains("分卷"), "非漫画超限应该保持改动前的整本拒绝: {err}");
}

#[test]
fn deliver_gates_format_before_touching_xochitl() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("c.cbz", b"PK").unwrap();
    s.stage_new("d.pdf", b"%PDF").unwrap();
    assert_eq!(s.list().iter().find(|e| e.name == "c.cbz").unwrap().format, "cbz");
    assert!(s.deliver("c.cbz", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err().contains("只读 EPUB / PDF"));
    // 体积门：超过 native_limit（测试设 1MB）不碰 xochitl，回执指引分卷
    s.stage_new("huge.pdf", &vec![b'%'; 2 * 1024 * 1024]).unwrap();
    let e = s.deliver("huge.pdf", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err();
    assert!(e.contains("超过 xochitl 上传上限") && e.contains("分卷"), "{e}");
    // PDF 走到 xochitl 才失败（不可达），母版仍在、无落库记录
    assert!(s.deliver("d.pdf", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).is_err());
    assert!(s.list().iter().any(|e| e.name == "d.pdf" && e.delivered.is_none()));
}

#[test]
fn deliver_ensures_folder_enqueues_and_waits_for_agent_to_create_it() {
    // 2026-09-19 用户反馈"文件夹里写了名字依然不会创建文件夹"：deliver() 落库前要先经
    // ensure_folder 确认目标文件夹真实存在，不存在就入队等 shelf-mkdir-agent.qmd 建出来。
    // 这里模拟"代理真的建出来了"（另起一个线程，短延迟后往 lib_dir 写一份 CollectionType
    // .metadata，等价于 Library.createCollection 真机执行后落盘的结果），验证 ensure_folder
    // 真的会在代理建好之后很快继续（而不是傻等满 20s 超时）。
    let t = tempfile::tempdir().unwrap();
    let lib_dir = t.path().join("xochitl");
    std::fs::create_dir_all(&lib_dir).unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", &lib_dir, 1)); // 端口 1 必然连不上，只测 ensure_folder 本身
    let s = Staging::new(t.path().join("staging"), x, 1024 * 1024);
    s.ensure().unwrap();
    s.stage_new("x.epub", b"PK").unwrap();
    let mkdir = MkdirQueue::new(&t.path().join("state"), &lib_dir);
    assert!(mkdir.list().is_empty());

    let lib_dir2 = lib_dir.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::fs::write(lib_dir2.join("f1.metadata"), r#"{"type":"CollectionType","visibleName":"新文件夹","parent":""}"#).unwrap();
    });

    let started = std::time::Instant::now();
    let _ = s.deliver("x.epub", "新文件夹", &mkdir, &rmsvc_core::events::EventBus::new());
    // 20s 超时是"等不到才放弃"的兜底上限；代理已经在 300ms+3s 防抖内把文件夹建出来了，
    // ensure_folder 应该在远小于超时的时间内就继续往下走（这里用 15s 卡一个宽松上限，
    // 只为区分"真的检测到了"和"傻等满超时"两种情况，不是卡精确耗时）。
    assert!(started.elapsed() < std::time::Duration::from_secs(15), "elapsed={:?}，看起来是等满了超时而不是检测到文件夹已建出来", started.elapsed());
    assert_eq!(mkdir.list().len(), 1, "ensure_folder 应该把这个文件夹名入队过");
    assert_eq!(mkdir.list()[0].name, "新文件夹");
}

/// 假 xochitl：`POST /upload` 把文件部分落成 `<uuid>.{ext}` + `.metadata`（+ EPUB 的渲染缓存 `.pdf`、PDF 的 `.content`）
/// 回 201，其余请求回 200——够 `Xochitl::upload_large_file` 走通"占位→替换成真文件"这条大文件通道。
fn fake_xochitl(lib: std::path::PathBuf) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap().to_string();
    std::thread::spawn(move || {
        let mut n = 0u32;
        for mut req in server.incoming_requests() {
            if req.method() != &tiny_http::Method::Post {
                let _ = req.respond(tiny_http::Response::from_string("[]"));
                continue;
            }
            let mut body = Vec::new();
            std::io::Read::read_to_end(req.as_reader(), &mut body).unwrap();
            let start = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let head = String::from_utf8_lossy(&body[..start]).to_string();
            let fname = head.split("filename=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
            let tail = body.windows(4).rposition(|w| w == b"\r\n--").unwrap_or(body.len());
            n += 1;
            let uuid = format!("0000000{n}-0000-4000-8000-000000000000");
            let ext = fname.rsplit('.').next().unwrap();
            std::fs::write(lib.join(format!("{uuid}.{ext}")), &body[start..tail]).unwrap();
            std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{fname}","parent":"","createdTime":"{}"}}"#, rmsvc_core::clock::now_ms())).unwrap();
            if ext == "epub" {
                std::fs::write(lib.join(format!("{uuid}.pdf")), b"render-cache").unwrap();
            } else {
                std::fs::write(lib.join(format!("{uuid}.content")), r#"{"fileType":"pdf","pageCount":1,"pages":["x"],"redirectionPageMap":[0],"sizeInBytes":"5"}"#).unwrap();
            }
            let _ = req.respond(tiny_http::Response::from_string(r#"{"status":"Upload successful"}"#).with_status_code(201));
        }
    });
    addr
}

/// 体积门压到 100 字节，逼所有书都走"超限"分支；书库目录是真目录 + 假 xochitl 服务。
fn oversized_staging(t: &tempfile::TempDir) -> (Staging, std::path::PathBuf) {
    let lib = t.path().join("xochitl");
    std::fs::create_dir_all(&lib).unwrap();
    let x = Arc::new(Xochitl::new(&fake_xochitl(lib.clone()), &lib, 10));
    let s = Staging::new(t.path().join("staging"), x, 100);
    s.ensure().unwrap();
    (s, lib)
}

#[test]
fn deliver_oversized_epub_uses_direct_channel_placeholder_then_real_file() {
    // 大文件通道（`try_deliver_direct`）：超网页上传上限的 EPUB 不分卷，先传占位再把磁盘上的文件替换成真书。
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    let opf = r#"<package version="2.0"><metadata><dc:title>大书</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let container = r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
    let epub = mini_epub(&[("META-INF/container.xml", container), ("content.opf", opf), ("c1.xhtml", &format!("<html><body><p>{}</p></body></html>", "字".repeat(500)))]);
    s.stage_new("big.epub", &epub).unwrap();
    assert!(epub.len() > 100);

    let out = s.deliver("big.epub", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap();
    assert!(out.message.contains("已直接写入 xochitl 书库") && out.message.contains("未分卷"), "{}", out.message);
    assert!(out.render.is_none(), "大文件通道不走渲染自检线程，靠 onopen 记录升级");
    let uuid_epub = std::fs::read_dir(&lib).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".epub")).expect("书库里应有文档").path();
    assert_eq!(std::fs::read(&uuid_epub).unwrap(), epub, "占位必须被真书替换");
    let d = sidecar::read(&s.dir().join("big.epub")).unwrap();
    assert!(d.native.is_some(), "应记一笔已加入原生");
    let rc = d.render.unwrap();
    assert_eq!(rc.status, "onopen", "EPUB 首次打开才渲染，先记 onopen");
    assert!(uuid_epub.file_name().unwrap().to_string_lossy().starts_with(&rc.uuid));
}

#[test]
fn deliver_oversized_pdf_uses_direct_channel_and_records_ok_pages() {
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    let img = bookconv::convert::pdfwrite::image_from_bytes(&fake_jpeg()).unwrap();
    let pdf = bookconv::convert::pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
    s.stage_new("big.pdf", &pdf).unwrap();
    let out = s.deliver("big.pdf", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap();
    assert!(out.message.contains("已直接写入 xochitl 书库"), "{}", out.message);
    let rc = sidecar::read(&s.dir().join("big.pdf")).unwrap().render.unwrap();
    assert_eq!((rc.status.as_str(), rc.pages), ("ok", 3), "PDF 页数就是真页数，直接 ok");
    let uuid_pdf = std::fs::read_dir(&lib).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".pdf")).unwrap().path();
    assert_eq!(std::fs::read(uuid_pdf).unwrap(), pdf);
}

#[test]
fn deliver_direct_channel_falls_back_when_placeholder_cannot_be_built() {
    // 是 zip 但没有 container.xml/OPF（造不出占位）→ 不走大文件通道，落到分卷/整本拒绝老路径，且没有往 xochitl 传任何东西。
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    s.stage_new("bad.epub", &mini_epub(&[("c1.xhtml", &format!("<html><body>{}</body></html>", "x".repeat(500)))])).unwrap();
    let err = s.deliver("bad.epub", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap_err();
    assert!(err.contains("超过 xochitl 上传上限"), "{err}");
    assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 0, "没造出占位就不该上传任何东西");
}

/// 书库目录不存在（大文件通道不可用）+ 假 xochitl 收上传的分卷投递夹具；返回 (staging, 假 xochitl 收件目录)。
fn splitting_staging(t: &tempfile::TempDir, budget: u64) -> (Staging, std::path::PathBuf) {
    let inbox = t.path().join("fake-xochitl-docs");
    std::fs::create_dir_all(&inbox).unwrap();
    let x = Arc::new(Xochitl::new(&fake_xochitl(inbox.clone()), Path::new("/nonexistent-lib"), 10));
    let s = Staging::new(t.path().join("staging"), x, budget);
    s.ensure().unwrap();
    (s, inbox)
}

fn uploaded_names(inbox: &Path, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(inbox)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".metadata"))
        .filter_map(|e| serde_json::from_slice::<serde_json::Value>(&std::fs::read(e.path()).unwrap()).ok())
        .filter_map(|m| m["visibleName"].as_str().map(str::to_string))
        .filter(|n| n.ends_with(ext))
        .collect();
    v.sort();
    v
}

#[test]
fn deliver_oversized_comic_epub_splits_by_volume_and_uploads_each_piece() {
    // `deliver_pieces` 的成功路径（EPUB 版）：整本超预算、大文件通道不可用 → 按 NCX 卷逐份上传，进度/回执/落库记录齐全。
    let t = tempfile::tempdir().unwrap();
    let (s, inbox) = splitting_staging(&t, 1200);
    let epub = comic_epub_with_real_images(&[12, 13]);
    assert!(epub.len() > 1200, "夹具本身要超预算才会拆: {}", epub.len());
    s.stage_new("manga.epub", &epub).unwrap();
    let out = s.deliver("manga.epub", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap();
    assert!(out.message.contains("已按卷拆分加入 xochitl") && out.message.contains("卷0") && out.message.contains("卷1"), "{}", out.message);
    assert!(out.render.is_none());
    assert_eq!(uploaded_names(&inbox, ".epub").len(), 2, "两卷各上传一份: {:?}", uploaded_names(&inbox, ".epub"));
    assert!(sidecar::read(&s.dir().join("manga.epub")).unwrap().native.is_some(), "落库记录已写");
}

#[test]
fn deliver_oversized_comic_pdf_splits_and_uploads_pdf_pieces() {
    // `deliver_pieces` 的成功路径（PDF 版，mime 走 application/pdf、stem 去 .pdf）。
    let t = tempfile::tempdir().unwrap();
    let (s, inbox) = splitting_staging(&t, 1 << 30);
    let epub = comic_epub_with_real_images(&[12, 13]);
    s.stage_new("manga.epub", &epub).unwrap();
    let dir = s.dir().to_path_buf();
    bookconv::comic_pdf::optimize_comic_epub_to_pdf_streaming(&dir.join("manga.epub"), &dir.join("manga.pdf"), |_, _| {}).unwrap();
    std::fs::remove_file(dir.join("manga.epub")).unwrap();
    let pdf_len = std::fs::metadata(dir.join("manga.pdf")).unwrap().len();
    // 预算取整本的 60%：整本超限、每卷（约一半）能放进去
    let s = Staging::new(dir.clone(), s.xochitl.clone(), pdf_len * 6 / 10);
    let out = s.deliver("manga.pdf", "", &empty_mkdir(&t), &rmsvc_core::events::EventBus::new()).unwrap();
    assert!(out.message.contains("已按卷拆分加入 xochitl"), "{}", out.message);
    assert_eq!(uploaded_names(&inbox, ".pdf").len(), 2, "{:?}", uploaded_names(&inbox, ".pdf"));
}

#[test]
fn upload_flow_lands_books_and_rejects_non_books() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let work = t.path().join(".work");
    let mut body = Vec::new();
    // 2026-09-18 起母版库只收 EPUB/PDF（cbz 已随"仅 KOReader"档退役）：接受项/重名项都改用
    // .epub 夹具，pic.jpg 仍测"非书籍格式拒收"，e.pdf 空内容仍测"空文件拒收"（合法格式但空）。
    for (f, d) in [("../中文 名.epub", "内容"), ("pic.jpg", "x"), ("e.pdf", ""), ("中文 名.epub", "again")] {
        body.extend_from_slice(format!("--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{f}\"\r\n\r\n{d}\r\n").as_bytes());
    }
    body.extend_from_slice(b"--B--\r\n");
    let out = AssetUploadFlow::in_dir(work.clone()).run(&StagingStore(&s), &body[..], "B").unwrap();
    assert!(out[0].ok && out[0].name == "中文 名.epub" && out[0].message == "已入母版库");
    assert!(!out[1].ok && out[1].message.contains("不是书籍格式") && out[1].message.contains(".epub"));
    assert!(!out[2].ok && out[2].message == "空文件");
    assert!(out[3].ok && out[3].name == "1_中文 名.epub" && out[3].message.contains("存为 1_中文 名.epub"));
    assert_eq!(std::fs::read(s.dir().join("中文 名.epub")).unwrap(), "内容".as_bytes());
    assert!(std::fs::read_dir(&work).unwrap().next().is_none(), "暂存 .work 应清空");
}
