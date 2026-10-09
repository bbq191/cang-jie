//! 母版库单测（原 `staging.rs` 内联的 `mod tests`；夹具（假 xochitl、漫画 EPUB 等）跨入库/落库共用，集中放这里）。
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

pub(crate) fn mini_epub(files: &[(&str, &str)]) -> Vec<u8> {
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
    assert_eq!(epub.format, "epub");
    assert_eq!(list.iter().find(|e| e.name == "doc.pdf").unwrap().format, "pdf");
    s.remove("a.epub").unwrap();
    assert_eq!(s.list().len(), 2);
    for bad in ["../x", ".hidden", "a/b"] {
        assert!(s.stage_new(bad, b"y").is_err(), "{bad:?} 非法名拒绝");
    }
}

/// 同一本书（内容逐字节相同）再传一次：认已有那本，不再多出 `1_书名`（2026-09-28 真机：白夜行被加进 KOReader 两份）。
/// 字节入库和"已落盘暂存文件"入库两条路都一样；暂存文件被消费掉（删除），不留垃圾。同名不同内容仍加前缀。
#[test]
fn identical_reupload_reuses_existing_book() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    assert_eq!(s.stage_new("白夜行.epub", b"same-bytes").unwrap(), "白夜行.epub");
    assert_eq!(s.stage_new("白夜行.epub", b"same-bytes").unwrap(), "白夜行.epub", "字节入库：内容相同认已有");
    let src = t.path().join("upload.part");
    std::fs::write(&src, b"same-bytes").unwrap();
    assert_eq!(s.stage_from_path("白夜行.epub", &src).unwrap(), "白夜行.epub", "文件入库：内容相同认已有");
    assert!(!src.exists(), "暂存文件已消费");
    assert_eq!(s.list().len(), 1);
    assert_eq!(s.stage_new("白夜行.epub", b"other-bytes").unwrap(), "1_白夜行.epub", "同名不同内容仍不覆盖");
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
fn delivered_record_roundtrip() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("b.epub", b"x").unwrap();
    assert!(s.list()[0].delivered.is_none(), "未落库无记录");
    s.mark_delivered("b.epub").unwrap();
    let d = s.list()[0].delivered.clone().unwrap();
    assert!(d.native.is_some(), "记原生落库（KOReader 去向 2026-09-30 已删）");
    assert_eq!(s.list().len(), 1, "sidecar 不当条目列出");
    assert!(s.mark_delivered("nope.epub").is_err(), "不存在的书拒绝");
    s.remove("b.epub").unwrap();
    assert!(!sidecar::path_for(&s.dir().join("b.epub")).exists(), "删书连带删 sidecar");
}

#[test]
fn busy_lock_blocks_second_start_and_conflicting_delete_deliver() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("x.epub", b"PK").unwrap();
    let busy = s.busy_guard("x.epub", "").expect("第一次加锁应该成功");
    assert!(s.busy_guard("x.epub", "").is_err(), "已经忙着，第二次应该失败");
    assert!(s.is_busy("x.epub"));
    assert!(s.remove("x.epub").unwrap_err().contains("正在处理中"), "忙的时候不该能删");
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    assert!(s.spawn_deliver("x.epub", "", Arc::new(empty_mkdir(&t)), bus).unwrap_err().contains("正在处理中"), "忙的时候不该能起第二个落库");
    assert!(s.list().iter().find(|e| e.name == "x.epub").unwrap().busy, "GET /staging 列表应体现 busy");
    drop(busy);
    assert!(!s.is_busy("x.epub"));
    assert!(s.remove("x.epub").is_ok(), "解锁后恢复正常");
}

#[test]
fn spawn_deliver_runs_in_background_records_result_then_clears_busy() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t); // xochitl 指向不可达地址（见 staging() 测试 helper），deliver 必然失败——够测异步管线本身
    s.stage_new("d.pdf", &[b'%'; 10]).unwrap();
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

/// 造一本多卷合集漫画（带 NCX 分卷目录），塞进 mini_epub 装不了的二进制字节所以这里直接手搓 zip——
/// 图片内容不是合法 JPEG 也无妨，书架从不解码图片。
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

/// 分卷投递已移除（2026-09-30）：超限漫画走不了大文件通道（测试里没有 xochitl 书库目录）就整本拒绝，不再拆成几份上传。
#[test]
fn deliver_oversized_comic_no_longer_splits() {
    let t = tempfile::tempdir().unwrap();
    let x = Arc::new(Xochitl::new("127.0.0.1:1", Path::new("/nonexistent"), 1));
    let s = Staging::new(t.path().join("staging"), x, 1024);
    s.ensure().unwrap();
    s.stage_new("manga.epub", &multivol_comic_epub(&[15, 15])).unwrap();
    let err = s.deliver("manga.epub", "", &empty_mkdir(&t)).unwrap_err();
    assert!(err.contains("超过 xochitl 上传上限") && err.contains("没有加入") && !err.contains("卷0"), "{err}");
}

/// SOI+SOF0(16x16,3分量)+EOI 最小 JPEG 骨架：书架只把封面字节原样拷进占位文档、从不解码，够用。
fn fake_jpeg() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03, 0x01, 0x11,
        0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xD9,
    ]
}

/// 带 NCX 目录、真图片字节（最小 JPEG 头）的漫画 EPUB，大文件通道的占位要从里面取封面。
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
    s.set_render("big.epub", sidecar::RenderCheck { uuid: "u1".into(), pages: 2, status: "onopen".into(), at: 1 }).unwrap();
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
    s.mark_delivered(&name).unwrap();
    std::fs::remove_file(s.dir.join(&name)).unwrap();
    assert!(sidecar::path_for(&s.dir.join(&name)).exists());
    // 同名新书入库：不能带着旧的"已加入"
    let name2 = s.stage_new("a.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>y</p>")])).unwrap();
    assert_eq!(name2, name);
    assert!(s.list()[0].delivered.is_none(), "新书不继承孤儿边车");
    // 启动清理：只清孤儿，不动有书的边车
    s.mark_delivered(&name2).unwrap();
    std::fs::write(s.dir.join(".gone.epub.delivered"), b"{}").unwrap();
    assert_eq!(s.gc_orphan_sidecars(), 1);
    assert!(sidecar::path_for(&s.dir.join(&name2)).exists());
    assert!(!s.dir.join(".gone.epub.delivered").exists());
}

/// 书名很长（250 字节中文，老式边车名要 261 字节、写不进去）：落库记录能写能读、网页列表看得到；改名、删除时边车跟着走 / 被删；
/// 启动修复认得它；孤儿清理只清真孤儿（短名形式的孤儿也清），不误删有书的边车（2026-09-30）。
#[test]
fn long_book_name_sidecar_full_lifecycle() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let long = format!("{}abc.pdf", "书".repeat(81));
    assert_eq!(long.len(), 250);
    let name = s.stage_new(&long, b"%PDF-1.4").unwrap();
    assert_eq!(name, long);
    s.mark_delivered(&name).unwrap();
    assert!(s.list()[0].delivered.as_ref().is_some_and(|d| d.native.is_some()), "列表里看得到落库状态");
    let car = sidecar::path_for(&s.dir.join(&name));
    assert!(car.is_file() && car.file_name().unwrap().len() <= 255);
    // 启动修复：pending → failed
    s.set_deliver_check(&name, sidecar::DeliverCheck { status: "pending".into(), ..Default::default() }).unwrap();
    assert_eq!(s.recover_interrupted(), (1, 0));
    assert_eq!(sidecar::read(&s.dir.join(&name)).unwrap().deliver.unwrap().status, "failed");
    // 孤儿清理：有书的短名边车不动；再放一个普通孤儿 + 一个短名孤儿，都清掉
    let orphan_long = sidecar::file_name_for(&format!("{}zz.pdf", "书".repeat(81)));
    std::fs::write(s.dir.join(&orphan_long), b"{}").unwrap();
    std::fs::write(s.dir.join(".gone.pdf.delivered"), b"{}").unwrap();
    assert_eq!(s.gc_orphan_sidecars(), 2);
    assert!(car.is_file(), "有书的边车不误删");
    assert!(!s.dir.join(&orphan_long).exists() && !s.dir.join(".gone.pdf.delivered").exists());
    // 改名（新名同样很长）：边车跟着走
    let renamed = s.rename(&name, &format!("{}cd", "书".repeat(81))).unwrap();
    assert!(!car.exists(), "旧边车挪走了");
    assert!(sidecar::read(&s.dir.join(&renamed)).is_some_and(|d| d.native.is_some()), "新名下读得到原记录");
    assert!(s.list()[0].delivered.is_some());
    // 改成短名：边车回到老格式
    let plain = s.rename(&renamed, "短名").unwrap();
    assert!(s.dir.join(".短名.pdf.delivered").is_file());
    let back = s.rename(&plain, &format!("{}ef", "书".repeat(81))).unwrap();
    // 删书连带删边车
    let back_car = sidecar::path_for(&s.dir.join(&back));
    assert!(back_car.is_file());
    s.remove(&back).unwrap();
    assert!(!back_car.exists());
    assert_eq!(std::fs::read_dir(&s.dir).unwrap().count(), 0, "目录里什么都不剩");
}

/// 跨分区入库（rename 失败走拷贝）：落地是完整文件、源删掉、目录里不留临时文件；源读不了时报错且母版库里不出现半截书。
/// 需要一个与临时目录不同分区的可写目录（/dev/shm），没有就跳过。
#[test]
fn stage_from_path_across_filesystems_lands_atomically() {
    use std::os::unix::fs::MetadataExt;
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let Ok(other) = tempfile::tempdir_in("/dev/shm") else {
        eprintln!("跳过：没有 /dev/shm");
        return;
    };
    if std::fs::metadata(other.path()).unwrap().dev() == std::fs::metadata(t.path()).unwrap().dev() {
        eprintln!("跳过：/dev/shm 与临时目录同分区");
        return;
    }
    let src = other.path().join("up.epub");
    let bytes = vec![7u8; 300_000];
    std::fs::write(&src, &bytes).unwrap();
    assert_eq!(s.stage_from_path("书.epub", &src).unwrap(), "书.epub");
    assert_eq!(std::fs::read(s.dir.join("书.epub")).unwrap(), bytes);
    assert!(!src.exists(), "源已删");
    let names: Vec<String> = std::fs::read_dir(&s.dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert_eq!(names, vec!["书.epub".to_string()], "不留临时文件");
    assert!(s.stage_from_path("缺.epub", &other.path().join("nope")).is_err());
    assert!(!s.has("缺.epub"));
}

#[test]
fn remove_holds_busy_lock_and_releases_it() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("r.epub", b"x").unwrap();
    s.remove("r.epub").unwrap();
    assert!(!s.is_busy("r.epub") && !s.has("r.epub"), "删完释放忙锁");
    assert!(s.remove("r.epub").is_err(), "已删再删报错");
    assert!(!s.is_busy("r.epub"), "失败也释放忙锁");
    assert!(s.remove("../x").is_err() && !s.is_busy("../x"), "非法名不占锁");
}

#[test]
fn recover_interrupted_fixes_stale_pending_and_removes_tmp() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let name = s.stage_new("a.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>x</p>")])).unwrap();
    s.set_deliver_check(&name, sidecar::DeliverCheck { status: "pending".into(), ..Default::default() }).unwrap();
    s.set_render(&name, sidecar::RenderCheck { status: "pending".into(), ..Default::default() }).unwrap();
    let ok = s.stage_new("ok.epub", &mini_epub(&[("OEBPS/a.xhtml", "<p>y</p>")])).unwrap();
    s.set_deliver_check(&ok, sidecar::DeliverCheck { status: "ok".into(), message: "已加入".into(), ..Default::default() }).unwrap();
    std::fs::write(s.dir.join(".a.epub.optimizing.tmp"), vec![0u8; 1000]).unwrap();
    std::fs::write(s.dir.join(".123.0.landing.tmp"), vec![0u8; 1000]).unwrap();
    assert_eq!(s.recover_interrupted(), (1, 2), "只修 pending 的那本，清 2 个半成品（旧版优化 + 跨分区入库）");
    let d = sidecar::read(&s.dir.join(&name)).unwrap();
    assert_eq!(d.deliver.unwrap().status, "failed");
    assert_eq!(d.render.unwrap().status, "timeout");
    assert_eq!(sidecar::read(&s.dir.join(&ok)).unwrap().deliver.unwrap().message, "已加入", "已完成的记录不动");
    assert!(!s.dir.join(".a.epub.optimizing.tmp").exists());
    assert_eq!(s.recover_interrupted(), (0, 0), "幂等");
}

/// 回归：补封面的临时副本（整本 EPUB 的拷贝）、边车原子写的临时文件也按半成品清；新旧两种临时文件命名都认。
#[test]
fn recover_interrupted_removes_old_and_new_style_scratch_files() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("a.epub", b"PK").unwrap();
    for n in [".a.epub.cover.tmp", ".9.0.cover.tmp", ".9.1.optimizing.tmp", ".9.2.landing.tmp", ".a.epub.delivered.9.3.tmp"] {
        std::fs::write(s.dir.join(n), vec![0u8; 100]).unwrap();
    }
    std::fs::write(s.dir.join(".a.epub.delivered.tmp-not-ours"), b"x").unwrap();
    std::fs::create_dir_all(s.dir.join(".dir.tmp")).unwrap();
    assert_eq!(s.recover_interrupted(), (0, 5), "含边车原子写没改名的临时文件");
    assert!(s.dir.join(".dir.tmp").is_dir(), "目录不动");
    assert!(s.dir.join(".a.epub.delivered.tmp-not-ours").exists(), "别的点前缀文件不动");
    assert!(s.has("a.epub"));
}

/// 回归：临时文件出错/panic 时由 Drop 删掉，不留到下次重启（后台线程 `catch_unwind` 兜住 panic，进程照常跑）。
#[test]
fn scratch_file_is_removed_on_panic_and_names_are_unique() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    let (a, b) = (s.scratch(), s.scratch());
    assert_ne!(a.path(), b.path());
    drop((a, b));
    let seen = std::sync::Mutex::new(None);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tmp = s.scratch();
        std::fs::write(tmp.path(), vec![0u8; 1000]).unwrap();
        *seen.lock().unwrap() = Some(tmp.path().to_path_buf());
        panic!("入库中途 panic");
    }));
    assert!(r.is_err());
    let p = seen.lock().unwrap().clone().unwrap();
    assert!(!p.exists(), "panic 展开时临时文件被删");
    assert!(std::fs::read_dir(s.dir()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

/// 列表的边车 / xochitl 页数也按文件戳缓存：没变就不再开文件（篡改缓存证明命中），边车一改写（原子写换 inode）立刻看到新内容。
#[test]
fn list_caches_sidecar_and_onopen_page_count_until_files_change() {
    const U: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
    let t = tempfile::tempdir().unwrap();
    let lib = t.path().join("xochitl");
    std::fs::create_dir_all(&lib).unwrap();
    let s = Staging::new(t.path().join("staging"), Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1)), 1024 * 1024);
    s.ensure().unwrap();
    s.stage_new("a.epub", b"PK").unwrap();
    s.set_render("a.epub", RenderCheck { uuid: U.into(), pages: 2, status: "onopen".into(), at: 1 }).unwrap();
    std::fs::write(lib.join(format!("{U}.content")), r#"{"pageCount":2}"#).unwrap();
    let rc = |s: &Staging| s.list()[0].delivered.clone().unwrap().render.unwrap();
    assert_eq!((rc(&s).status.as_str(), rc(&s).pages), ("onopen", 2));
    assert_eq!((s.caches.sidecars.len(), s.caches.pages.len()), (1, 1));
    // 篡改两份缓存：文件没变时列表用的就是缓存里的值
    let content = lib.join(format!("{U}.content"));
    s.caches.pages.put(U, rmsvc_core::cache::FileStamp::read(&content).unwrap(), Some(2));
    let car = sidecar::path_for(&s.dir.join("a.epub"));
    let mut fake = sidecar::read(&s.dir.join("a.epub")).unwrap();
    fake.native = Some(42);
    s.caches.sidecars.put("a.epub", rmsvc_core::cache::FileStamp::read(&car).unwrap(), Some(fake));
    assert_eq!(s.list()[0].delivered.clone().unwrap().native, Some(42), "边车没变 → 命中缓存");
    // 边车改写（原子写）→ 立刻重读
    s.mark_delivered("a.epub").unwrap();
    assert_ne!(s.list()[0].delivered.clone().unwrap().native, Some(42), "边车一改写就重读");
    // xochitl 渲染完改写 .content → 页数变了，升级成 ok 并写回边车；之后不再是 onopen、不再查 .content
    std::fs::write(&content, r#"{"pageCount":351}"#).unwrap();
    assert_eq!((rc(&s).status.as_str(), rc(&s).pages), ("ok", 351));
    assert!(s.caches.pages.is_empty(), "不再是 onopen 的文档从页数缓存里清掉");
}

#[test]
fn deliver_oversized_non_comic_epub_keeps_flat_reject() {
    let t = tempfile::tempdir().unwrap();
    let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let long_text = "正".repeat(600_000); // 600_000 * 3 字节(UTF-8) ≈ 1.7MB，确保超过测试用 1MB native_limit
    let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", &format!("<html><body><p>{long_text}</p></body></html>"))]);
    let s = staging(&t);
    s.stage_new("novel.epub", &epub).unwrap();
    let err = s.deliver("novel.epub", "", &empty_mkdir(&t)).unwrap_err();
    assert!(err.contains("超过 xochitl 上传上限") && err.contains("没有加入"), "超限又走不了大文件通道：整本拒绝: {err}");
}

#[test]
fn deliver_gates_format_before_touching_xochitl() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("c.cbz", b"PK").unwrap();
    s.stage_new("d.pdf", b"%PDF").unwrap();
    assert_eq!(s.list().iter().find(|e| e.name == "c.cbz").unwrap().format, "other");
    assert!(s.deliver("c.cbz", "", &empty_mkdir(&t)).unwrap_err().contains("只读 EPUB / PDF"));
    // 体积门：超过 native_limit（测试设 1MB）又走不了大文件通道：不碰 xochitl，整本拒绝
    s.stage_new("huge.pdf", &vec![b'%'; 2 * 1024 * 1024]).unwrap();
    let e = s.deliver("huge.pdf", "", &empty_mkdir(&t)).unwrap_err();
    assert!(e.contains("超过 xochitl 上传上限") && e.contains("没有加入"), "{e}");
    // PDF 走到 xochitl 才失败（不可达），母版仍在、无落库记录
    assert!(s.deliver("d.pdf", "", &empty_mkdir(&t)).is_err());
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
    let _ = s.deliver("x.epub", "新文件夹", &mkdir);
    // 20s 超时是"等不到才放弃"的兜底上限；代理已经在 300ms+3s 防抖内把文件夹建出来了，
    // ensure_folder 应该在远小于超时的时间内就继续往下走（这里用 15s 卡一个宽松上限，
    // 只为区分"真的检测到了"和"傻等满超时"两种情况，不是卡精确耗时）。
    assert!(started.elapsed() < std::time::Duration::from_secs(15), "elapsed={:?}，看起来是等满了超时而不是检测到文件夹已建出来", started.elapsed());
    assert_eq!(mkdir.list().len(), 1, "ensure_folder 应该把这个文件夹名入队过");
    assert_eq!(mkdir.list()[0].name, "新文件夹");
}

/// 假 xochitl：`POST /upload` 把文件部分落成 `<uuid>.{ext}` + `.metadata`（+ EPUB 的渲染缓存 `.pdf`、PDF 的 `.content`）
/// 回 201，其余请求回 200——够 `Xochitl::upload_large_file` 走通"占位→替换成真文件"这条大文件通道。
pub(crate) fn fake_xochitl(lib: std::path::PathBuf) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap().to_string();
    std::thread::spawn(move || {
        let mut n = 0u32;
        // 真 xochitl 的"当前文件夹"：`GET /documents/<uuid>` 设、之后的 `/upload` 落进去（`GET /documents/` ＝ 根）。
        let mut current = String::new();
        for mut req in server.incoming_requests() {
            if req.method() != &tiny_http::Method::Post {
                if let Some(f) = req.url().strip_prefix("/documents/") {
                    current = f.trim_end_matches('/').to_string();
                }
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
            std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{fname}","parent":"{current}","createdTime":"{}"}}"#, rmsvc_core::clock::now_ms())).unwrap();
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
    // 大文件通道（`try_deliver_direct`）：超网页上传上限的 EPUB 先传占位再把磁盘上的文件替换成真书。
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    let opf = r#"<package version="2.0"><metadata><dc:title>大书</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let container = r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
    let epub = mini_epub(&[("META-INF/container.xml", container), ("content.opf", opf), ("c1.xhtml", &format!("<html><body><p>{}</p></body></html>", "字".repeat(500)))]);
    s.stage_new("big.epub", &epub).unwrap();
    assert!(epub.len() > 100);

    let out = s.deliver("big.epub", "", &empty_mkdir(&t)).unwrap();
    assert!(out.message.contains("已直接写入 xochitl 书库"), "{}", out.message);
    assert!(out.render.is_none(), "大文件通道不走渲染自检线程，靠 onopen 记录升级");
    let uuid_epub = std::fs::read_dir(&lib).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".epub")).expect("书库里应有文档").path();
    assert_eq!(std::fs::read(&uuid_epub).unwrap(), epub, "占位必须被真书替换");
    let d = sidecar::read(&s.dir().join("big.epub")).unwrap();
    assert!(d.native.is_some(), "应记一笔已加入原生");
    let rc = d.render.unwrap();
    assert_eq!(rc.status, "onopen", "EPUB 首次打开才渲染，先记 onopen");
    assert!(uuid_epub.file_name().unwrap().to_string_lossy().starts_with(&rc.uuid));
}

/// 经典交叉引用表的 `pages` 页空白 PDF（页树根是对象 2），末尾垫 200 字节让体积超过测试里调小的直传上限。
fn classic_pdf(pages: usize) -> Vec<u8> {
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", i + 3)).collect();
    let mut objs = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" "))];
    objs.extend((0..pages).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>".to_string()));
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offs = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offs.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    pdf.extend_from_slice(&[b'%'; 200]);
    pdf.push(b'\n');
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    pdf
}

#[test]
fn deliver_oversized_pdf_uses_direct_channel_and_records_ok_pages() {
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    let pdf = classic_pdf(3);
    s.stage_new("big.pdf", &pdf).unwrap();
    let out = s.deliver("big.pdf", "", &empty_mkdir(&t)).unwrap();
    assert!(out.message.contains("已直接写入 xochitl 书库"), "{}", out.message);
    let rc = sidecar::read(&s.dir().join("big.pdf")).unwrap().render.unwrap();
    assert_eq!((rc.status.as_str(), rc.pages), ("ok", 3), "PDF 页数就是真页数，直接 ok");
    let uuid_pdf = std::fs::read_dir(&lib).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".pdf")).unwrap().path();
    assert_eq!(std::fs::read(uuid_pdf).unwrap(), pdf);
}

/// 第三方 PDF：页树根不在对象 2（对象 2 是带 `/Count` 的书签根）。此前 `PdfFileReader::page_count` 只认"对象 2 = Pages"，
/// 这种书整本被拒收；现在顺着 Root → Pages 读真页数（交叉引用流 / 对象流的覆盖在 shelf-conv `pdfmeta` 的单测里）。
#[test]
fn deliver_oversized_third_party_pdf_reads_pages_via_root() {
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    let objs: [&[u8]; 4] = [b"<< /Type /Catalog /Pages 3 0 R /Outlines 2 0 R >>", b"<< /Type /Outlines /Count 9 >>", b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>", b"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 10 10] >>"];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offs = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offs.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    pdf.extend_from_slice(&[b'%'; 200]); // 让体积超过测试里调小的直传上限
    pdf.push(b'\n');
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for o in offs {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    s.stage_new("third.pdf", &pdf).unwrap();
    let out = s.deliver("third.pdf", "", &empty_mkdir(&t)).unwrap();
    assert!(out.message.contains("已直接写入 xochitl 书库"), "{}", out.message);
    let rc = sidecar::read(&s.dir().join("third.pdf")).unwrap().render.unwrap();
    assert_eq!((rc.status.as_str(), rc.pages), ("ok", 1), "页数来自 Root→Pages 的 /Count，不是对象 2 的书签数");
    let uuid_pdf = std::fs::read_dir(&lib).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".pdf")).unwrap().path();
    assert_eq!(std::fs::read(uuid_pdf).unwrap(), pdf);
}

#[test]
fn deliver_direct_channel_falls_back_when_placeholder_cannot_be_built() {
    // 是 zip 但没有 container.xml/OPF（造不出占位）→ 不走大文件通道，整本拒绝，且没有往 xochitl 传任何东西。
    let t = tempfile::tempdir().unwrap();
    let (s, lib) = oversized_staging(&t);
    s.stage_new("bad.epub", &mini_epub(&[("c1.xhtml", &format!("<html><body>{}</body></html>", "x".repeat(500)))])).unwrap();
    let err = s.deliver("bad.epub", "", &empty_mkdir(&t)).unwrap_err();
    assert!(err.contains("超过 xochitl 上传上限"), "{err}");
    assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 0, "没造出占位就不该上传任何东西");
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

/// 改名：沿用扩展名、边车跟着走；格式不能改；目标已存在拒绝；忙时拒绝。
#[test]
fn rename_keeps_format_moves_sidecar_and_refuses_conflicts() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("a.epub", b"A").unwrap();
    s.stage_new("b.epub", b"B").unwrap();
    s.mark_delivered("a.epub").unwrap();
    let dir = t.path().join("staging");

    assert_eq!(s.rename("a.epub", "  新名字 ").unwrap(), "新名字.epub", "不带扩展名沿用原格式、去首尾空白");
    assert_eq!(std::fs::read(dir.join("新名字.epub")).unwrap(), b"A");
    assert!(!dir.join("a.epub").exists());
    assert!(crate::sidecar::read(&dir.join("新名字.epub")).is_some_and(|d| d.native.is_some()), "落库记录跟着改名");

    assert!(s.rename("新名字.epub", "b").unwrap_err().contains("已有《b.epub》"));
    assert_eq!(s.rename("新名字.epub", "x.pdf").unwrap(), "x.pdf.epub", "不能借改名改格式：别的扩展名只当名字的一部分");
    assert!(s.rename("x.pdf.epub", "../evil").is_err(), "路径分隔符拒绝");
    assert!(s.rename("x.pdf.epub", " ").unwrap_err().contains("不能为空"));

    let _busy = s.busy_guard("b.epub", "").unwrap();
    assert!(s.rename("b.epub", "c").unwrap_err().contains("正在处理中"));
}

#[test]
fn open_for_download_returns_file_and_length() {
    use std::io::Read;
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("d.epub", b"hello").unwrap();
    let (mut f, n) = s.open_for_download("d.epub").unwrap();
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).unwrap();
    assert_eq!((buf.as_slice(), n), (&b"hello"[..], 5));
    assert!(s.open_for_download("nope.epub").is_err());
}

/// 「首次打开才渲染」的书被打开后，列表把 onopen 升级成 ok，并**写回边车**——之后的列表不再去读 xochitl 的 `.content`。
#[test]
fn list_persists_onopen_to_ok_upgrade() {
    const U: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
    let t = tempfile::tempdir().unwrap();
    let lib = t.path().join("xochitl");
    std::fs::create_dir_all(&lib).unwrap();
    let s = Staging::new(t.path().join("staging"), Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1)), 0);
    s.ensure().unwrap();
    s.stage_new("big.epub", b"PK").unwrap();
    s.set_render("big.epub", RenderCheck { uuid: U.into(), pages: 3, status: "onopen".into(), at: 1 }).unwrap();
    std::fs::write(lib.join(format!("{U}.content")), r#"{"pageCount":3}"#).unwrap();
    let rc = |s: &Staging| s.list()[0].delivered.clone().unwrap().render.unwrap();
    assert_eq!(rc(&s).status, "onopen", "页数没变＝还没打开过");
    std::fs::write(lib.join(format!("{U}.content")), r#"{"pageCount":412}"#).unwrap();
    assert_eq!((rc(&s).status.as_str(), rc(&s).pages), ("ok", 412));
    let stored = sidecar::read(&s.dir().join("big.epub")).unwrap().render.unwrap();
    assert_eq!((stored.status.as_str(), stored.pages), ("ok", 412), "升级已落盘");
    std::fs::remove_file(lib.join(format!("{U}.content"))).unwrap();
    assert_eq!(rc(&s).pages, 412, "之后列表不再依赖 .content");
}

/// 回归：多条入库路径（网页上传 / inbox 追平）同时落同名书，每一本都要落成独立文件、谁也不覆盖谁。
/// 此前靠网页上传把 spool 锁攥到请求体收完来串行化（当时的抓网文压根不拿锁）；现在"挑名 + 落地"由落名临界区保证。
#[test]
fn concurrent_landing_of_same_name_never_clobbers() {
    let t = tempfile::tempdir().unwrap();
    let s = Arc::new(staging(&t));
    let src_dir = t.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let n = 16;
    let hs: Vec<_> = (0..n)
        .map(|i| {
            let s = s.clone();
            let src = src_dir.join(format!("{i}.part"));
            std::fs::write(&src, format!("book-{i}")).unwrap();
            std::thread::spawn(move || if i % 2 == 0 { s.stage_from_path("同名.pdf", &src).unwrap() } else { s.stage_new("同名.pdf", format!("book-{i}").as_bytes()).unwrap() })
        })
        .collect();
    let names: std::collections::HashSet<String> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(names.len(), n, "每次落地都拿到不同的名字");
    let contents: std::collections::HashSet<Vec<u8>> = names.iter().map(|nm| std::fs::read(s.dir().join(nm)).unwrap()).collect();
    assert_eq!(contents.len(), n, "没有任何一本被别的覆盖");
}


/// 带 sheng-ren 页边距标记（`META-INF/eink-reader-margins`）的漫画登记标记里的页边距（没有开关，2026-10-07）；没有标记的书
/// （文字书、本仓库旧版优化的漫画）不登记。
#[test]
fn comic_margins_follow_sheng_ren_marker() {
    let t = tempfile::tempdir().unwrap();
    let s = staging(&t);
    s.stage_new("manga.epub", &mini_epub(&[(shelf_conv::epub::READER_MARGINS_MARKER, "1"), ("OEBPS/p1.xhtml", "<p>x</p>")])).unwrap();
    s.stage_new("novel.epub", &mini_epub(&[("OEBPS/p1.xhtml", "<p>x</p>")])).unwrap();
    let margins = |n: &str| shelf_conv::epub::Book::open(&s.dir().join(n)).ok().and_then(|mut b| b.reader_margins());
    assert_eq!(margins("manga.epub"), Some(1), "缺 OPF 也照样读得到标记");
    assert_eq!(margins("novel.epub"), None);
}

/// `lowSpace` 的判据：严格小于 300 MiB 才算不足，查不到空间不算（与网页此前写死的判断一致）。
#[test]
fn low_space_threshold_is_strict_and_unknown_is_not_low() {
    assert_eq!(LOW_SPACE_BYTES, 300 * 1024 * 1024);
    assert!(low_space(Some(LOW_SPACE_BYTES - 1)));
    assert!(!low_space(Some(LOW_SPACE_BYTES)));
    assert!(!low_space(None));
}
