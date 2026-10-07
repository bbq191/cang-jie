//! **直接导入 xochitl**（不进母版库，2026-10-07）：电脑上的 sheng-ren（`booklib sync`）经 SSH 端口转发直连本服务
//! （`ssh -L` 到设备 `127.0.0.1:8790`，不经网关、不带 `/api/books` 前缀），把已经按 `xochitl` 阅读模式优化好的 EPUB
//! 直接加入 xochitl，之后书变了就原地替换。三个动作：
//!
//! - **新导入**（[`Importer::import_new`]）：请求体流式落到本服务状态目录下的临时文件（`books/import-tmp/`，**不是**
//!   母版库目录，不进母版库列表），再照落库那条路加入 xochitl——≤ 体积门走普通 `/upload`，超了走大文件通道（占位 + 磁盘替换）。
//!   `/upload` 不回 uuid：按"上传前没有、上传后新出现、`<uuid>.epub` 与上传的字节逐字节相同"认出这份新文档。
//! - **原地替换**（[`Importer::replace`]）：已有文档的 `<uuid>.epub` 换成新内容，uuid 不变（阅读进度 `lastOpenedPage`、
//!   所在文件夹、页边距等 `.content`/`.metadata` 里的东西都不动，漫画页边距也不重新登记——用户在阅读器里调过的不被覆盖）；
//!   删掉渲染缓存 `.pdf`/`.epubindex`，让 xochitl 下次打开时重排。
//! - **查询**（[`Importer::describe`]）：这份文档还在不在、叫什么、在哪个文件夹（客户端据此决定替换还是重新导入）。
//!
//! 删除不另设接口：客户端用已有的 `POST /trash/add {uuid, name}`（走 xochitl 自己的回收站代理，见 `trash.rs`）。
//!
//! **内存**：book-serve 的 systemd `MemoryMax=192M`，整本书绝不读进内存——请求体按块直接写盘，上传也是流式（`upload_file`）。
//! **同步处理**：HTTP 请求一直挂到加入完成（大书能到分钟级），客户端超时要设得够长；rmsvc-core 只对"读请求体时的空闲"
//! 设了 60 秒超时（`READ_IDLE_TIMEOUT`），处理期间服务端不读 socket，不受它影响。
use crate::mkdir::MkdirQueue;
use crate::ops::OpRegistry;
use crate::scratch::ScratchFile;
use crate::staging::{Staging, MAX_DIRECT_BYTES};
use rmsvc_core::fs::{plain_name, same_content, Content};
use rmsvc_core::xochitl::{find_documents_since, is_uuid_shape, read_metadata, Delivery, Xochitl};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// 普通上传回 2xx 之后，等 xochitl 在书库里建好这份文档的上限（真机导入当下同步渲染，回执时多半已经落盘）。
pub const CLAIM_WAIT: Duration = Duration::from_secs(20);
/// `/upload` 读超时 / 408（"很可能已送达"，见 `upload_likely_delivered`）时多等一会：xochitl 还在处理大书。
pub const CLAIM_WAIT_SLOW: Duration = Duration::from_secs(120);
/// 认领时书库目录写入的防抖（xochitl 导入时连写 metadata/content/epub）。
const CLAIM_DEBOUNCE: Duration = Duration::from_millis(500);

/// 直接导入的错误：HTTP 层按种类映射成 400 / 404 / 500（领域模块不碰 http 类型）。
#[derive(Debug, PartialEq)]
pub enum ImportError {
    /// 请求本身不对（文件名、格式、大小、uuid 形状、正在替换中）→ 400。
    Bad(String),
    /// 要替换的文档不在 / 已删除 / 在回收站 / 不是 EPUB → 404（客户端据此改成新导入）。
    NotFound(String),
    /// 设备这边出错（写盘、上传、认不出新文档）→ 500。
    Failed(String),
}

/// 导入 / 替换成功后、以及查询时回给客户端的文档状况。
#[derive(Debug, Clone, PartialEq)]
pub struct DocState {
    pub uuid: String,
    /// `.metadata` 的 `visibleName`（`POST /trash/add` 要用这个名字）。
    pub name: String,
    /// 所在文件夹的名字；书库根 / 已删除 → 空串。
    pub folder: String,
    /// `deleted` 为真或进了回收站。
    pub deleted: bool,
}

pub struct Importer {
    xochitl: Arc<Xochitl>,
    /// 借用落库那套：`ensure_folder`（经建文件夹队列建出目标文件夹）、漫画页边距的判定与登记。母版库本身不碰。
    staging: Staging,
    /// 新导入和原地替换的请求体都先落在这（`$XDG_STATE_HOME/shelf/books/import-tmp/`，/home 分区，与 xochitl 书库同分区，
    /// 替换时收完再 rename 进书库）。进程中途被杀留下的半成品由 [`Importer::ensure`] 在启动时清掉——此前替换直接写书库里的
    /// `<uuid>.epub.new`，被杀就一直留在书库目录里（最大 1GB）。
    tmp_dir: PathBuf,
    /// 普通上传的体积门（字节，0=不拦），同 `BookConfig::native_upload_limit_bytes`。
    native_limit: u64,
    /// 正在替换的 uuid：同一份文档同时只允许一个替换。
    replacing: OpRegistry,
    pub claim_wait: Duration,
    pub claim_wait_slow: Duration,
}

impl Importer {
    pub fn new(xochitl: Arc<Xochitl>, staging: Staging, tmp_dir: PathBuf, native_limit: u64) -> Importer {
        Importer { xochitl, staging, tmp_dir, native_limit, replacing: OpRegistry::default(), claim_wait: CLAIM_WAIT, claim_wait_slow: CLAIM_WAIT_SLOW }
    }

    /// 建临时目录，并清掉上次进程留下的半成品（被杀时 Drop 来不及删）。返回清掉几个。
    pub fn ensure(&self) -> std::io::Result<usize> {
        std::fs::create_dir_all(&self.tmp_dir)?;
        let mut n = 0;
        for e in std::fs::read_dir(&self.tmp_dir)?.flatten() {
            if e.file_type().is_ok_and(|t| t.is_file()) && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    fn lib(&self) -> Result<&Path, ImportError> {
        let lib = self.xochitl.library_dir();
        if !lib.is_dir() {
            return Err(ImportError::Failed(format!("xochitl 书库目录不在（{}），直接导入只能在设备上用", lib.display())));
        }
        Ok(lib)
    }

    /// 新导入：`body` 落临时文件 → 确保文件夹 → 加入 xochitl → 认出 uuid → （漫画）登记页边距。**同步、阻塞**（分钟级）。
    /// `folder` 空＝书库根；非空且不存在先经建文件夹队列建（等不到就落书库根，同 `Staging::deliver`）。
    pub fn import_new(&self, name: &str, folder: &str, body: &mut dyn Read, declared_len: Option<usize>, mkdir: &MkdirQueue) -> Result<DocState, ImportError> {
        check_name(name)?;
        let lib = self.lib()?;
        let part = ScratchFile::new(&self.tmp_dir, "", "epub.part");
        let size = receive(body, declared_len, part.path())?;
        // 体收完、校验过才动文件夹：坏请求不该留下一个空文件夹。
        let folder = folder.trim();
        self.staging.ensure_folder(folder, mkdir);
        let uuid = if self.native_limit > 0 && size > self.native_limit {
            // 大文件通道自己登记漫画页边距（同母版库落库那条路）。
            let up = self.staging.upload_large(part.path(), name, folder).map_err(ImportError::Failed)?;
            up.ok_or_else(|| ImportError::Bad("读不了这本 EPUB，造不出大文件通道的占位文档".into()))?.uuid
        } else {
            let uuid = self.upload_and_claim(lib, part.path(), name, folder)?;
            if let Some(m) = shelf_conv::epub::Book::open(part.path()).ok().and_then(|mut b| b.reader_margins()) {
                self.staging.register_comic_margins(&uuid, name, m);
            }
            uuid
        };
        println!("[book-serve] 直接导入《{name}》{} KB → {uuid}", size >> 10);
        self.describe(&uuid).ok_or_else(|| ImportError::Failed(format!("已加入 xochitl（{uuid}），但读不到它的 .metadata")))
    }

    /// 普通 `/upload` + 认领：上传前先记下"已经在的候选"，上传后找新出现、内容逐字节相同的那份。
    /// 整段串行（进程内一把锁）：两次导入同一份字节时，后一次不会把前一次刚建的文档认成自己的。
    fn upload_and_claim(&self, lib: &Path, part: &Path, name: &str, folder: &str) -> Result<String, ImportError> {
        static CLAIM: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _serial = rmsvc_core::sync::lock(&CLAIM);
        let since = rmsvc_core::clock::now_ms().saturating_sub(2_000);
        let before: std::collections::HashSet<String> = find_documents_since(lib, since).into_iter().map(|d| d.uuid).collect();
        let wait = match self.xochitl.upload_file(part, name, "application/epub+zip", folder) {
            Ok(Delivery::Delivered(_)) => self.claim_wait,
            Ok(Delivery::LikelyDelivered(_)) => self.claim_wait_slow,
            Err(e) => return Err(ImportError::Failed(format!("上传给 xochitl 失败: {e}"))),
        };
        // 先查一次（真机导入当下同步渲染，回执时多半已经落盘），没有再等书库目录的变化（事件驱动，此前每 200ms 扫一遍书库）。
        let claim = || {
            find_documents_since(lib, since)
                .into_iter()
                .find(|d| !before.contains(&d.uuid) && same_content(&lib.join(format!("{}.epub", d.uuid)), Content::File(part)))
                .map(|d| d.uuid)
        };
        let mut found = claim();
        if found.is_none() {
            rmsvc_core::fswatch::watch_until(lib, CLAIM_DEBOUNCE, wait, |_| {
                found = claim();
                found.is_some()
            });
        }
        found.or_else(claim).ok_or_else(|| {
            ImportError::Failed(format!("已上传给 xochitl，但 {} 秒内没在书库里认出《{name}》（可能稍后才出现；先在设备上看一眼，别马上重试，免得重复）", wait.as_secs()))
        })
    }

    /// 原地替换 `<uuid>.epub`，uuid 不变：体流式写进临时目录（0600）→ 删渲染缓存 → rename 进书库（同分区，原子）。
    /// `.content`/`.metadata` 不动（保留 `lastOpenedPage`、页边距、所在文件夹），漫画页边距不重新登记。
    ///
    /// 删 `.pdf`/`.epubindex` 让 xochitl 下次打开时重排，与大文件通道替换占位的做法一致——**那条路（从没打开过的占位）2026-09-20/25
    /// 真机验证过；这条路对「已经打开过、已排版」的书 xochitl 会不会正确重排、`lastOpenedPage` 落在新排版的哪一页，尚未真机验证。**
    /// 书正开着时替换，xochitl 手里的旧排版要等关书再开才换。缩略图（`<uuid>.thumbnails/`）不动，封面变了要等 xochitl 自己刷新。
    pub fn replace(&self, uuid: &str, name: &str, body: &mut dyn Read, declared_len: Option<usize>) -> Result<DocState, ImportError> {
        check_name(name)?;
        if !is_uuid_shape(uuid) {
            return Err(ImportError::Bad("uuid 格式不对".into()));
        }
        let lib = self.lib()?;
        self.replaceable(uuid)?;
        let _busy = self.replacing.try_guard(uuid).ok_or_else(|| ImportError::Bad("这份文档正在替换中，请稍候".into()))?;
        let part = ScratchFile::new(&self.tmp_dir, "", "epub.part");
        receive(body, declared_len, part.path())?;
        // 收体可能要好几分钟，期间用户可能在设备上把书删了：再确认一次，别把新文件写进回收站里的条目。
        self.replaceable(uuid)?;
        let _ = std::fs::remove_file(lib.join(format!("{uuid}.pdf")));
        let _ = std::fs::remove_file(lib.join(format!("{uuid}.epubindex")));
        std::fs::rename(part.path(), lib.join(format!("{uuid}.epub"))).map_err(|e| ImportError::Failed(format!("替换文件失败: {e}")))?;
        println!("[book-serve] 原地替换《{name}》→ {uuid}");
        self.describe(uuid).ok_or_else(|| ImportError::Failed(format!("已替换（{uuid}），但读不到它的 .metadata")))
    }

    /// 能不能原地替换：`.metadata` 在、是文档、没删除没进回收站、有 `<uuid>.epub`。不能 → [`ImportError::NotFound`]。
    fn replaceable(&self, uuid: &str) -> Result<(), ImportError> {
        match self.describe(uuid) {
            None => Err(ImportError::NotFound("xochitl 书库里没有这份文档".into())),
            Some(d) if d.deleted => Err(ImportError::NotFound("这份文档已删除或在回收站里".into())),
            Some(_) if !self.xochitl.library_dir().join(format!("{uuid}.epub")).is_file() => Err(ImportError::NotFound("这份文档不是 EPUB".into())),
            Some(_) => Ok(()),
        }
    }

    /// 查一份文档：`.metadata` 不在、读不了、不是文档（文件夹）、uuid 形状不对 → `None`。
    pub fn describe(&self, uuid: &str) -> Option<DocState> {
        if !is_uuid_shape(uuid) {
            return None;
        }
        let lib = self.xochitl.library_dir();
        let v = read_metadata(lib, uuid)?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        if s("type") != "DocumentType" {
            return None;
        }
        let parent = s("parent");
        let deleted = parent == "trash" || v.get("deleted").and_then(|x| x.as_bool()) == Some(true);
        let folder = if deleted || parent.is_empty() || !is_uuid_shape(&parent) {
            String::new()
        } else {
            read_metadata(lib, &parent).and_then(|p| p.get("visibleName").and_then(|x| x.as_str()).map(str::to_string)).unwrap_or_default()
        };
        Some(DocState { uuid: uuid.to_string(), name: s("visibleName"), folder, deleted })
    }
}

/// 文件名：单段、扩展名 `.epub`（不区分大小写）。只收 EPUB——优化好的书都是 EPUB，xochitl 也只认 EPUB/PDF。
fn check_name(name: &str) -> Result<(), ImportError> {
    plain_name(name).map_err(ImportError::Bad)?;
    if rmsvc_core::formats::ext_of(name) != "epub" {
        return Err(ImportError::Bad("直接导入只收 .epub".into()));
    }
    Ok(())
}

/// 把请求体按块写进 `dest`（0600，覆盖旧的），不整本进内存。校验：非空、不超过 [`MAX_DIRECT_BYTES`]、收到的字节数等于
/// `Content-Length`（客户端中途断开时 tiny_http 的定长读取会提前结束）、开头是 zip 本地文件头（`PK\3\4`，EPUB 是 zip）。
/// 返回字节数；出错时由调用方的 [`ScratchFile`] 删掉半成品。
fn receive(body: &mut dyn Read, declared_len: Option<usize>, dest: &Path) -> Result<u64, ImportError> {
    let too_big = || ImportError::Bad(format!("超过 {} MB 上限", MAX_DIRECT_BYTES >> 20));
    match declared_len {
        Some(0) => return Err(ImportError::Bad("请求体是空的".into())),
        Some(n) if n as u64 > MAX_DIRECT_BYTES => return Err(too_big()),
        _ => {}
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let f = o.open(dest).map_err(|e| ImportError::Failed(format!("建临时文件 {} 失败: {e}", dest.display())))?;
    let mut w = std::io::BufWriter::with_capacity(64 * 1024, f);
    let n = std::io::copy(&mut body.take(MAX_DIRECT_BYTES + 1), &mut w).map_err(|e| ImportError::Bad(format!("接收请求体中断: {e}")))?;
    let f = w.into_inner().map_err(|e| ImportError::Failed(format!("写临时文件失败: {}", e.error())))?;
    f.sync_all().map_err(|e| ImportError::Failed(format!("写临时文件失败: {e}")))?;
    if n == 0 {
        return Err(ImportError::Bad("请求体是空的".into()));
    }
    if n > MAX_DIRECT_BYTES {
        return Err(too_big());
    }
    if let Some(d) = declared_len.filter(|&d| d as u64 != n) {
        return Err(ImportError::Bad(format!("请求体不完整（收到 {n} 字节，Content-Length 是 {d}）")));
    }
    let mut magic = [0u8; 4];
    let head_ok = std::fs::File::open(dest).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && magic == *b"PK\x03\x04";
    if !head_ok {
        return Err(ImportError::Bad("不是 EPUB（不是 zip 文件）".into()));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staging::tests::{fake_xochitl, mini_epub};

    const U: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

    /// 书库目录 + 指向 `host` 的 xochitl 客户端 + 体积门 `limit`。
    fn importer(t: &tempfile::TempDir, host: &str, limit: u64) -> (Importer, PathBuf) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new(host, &lib, 10));
        let s = Staging::new(t.path().join("staging"), x.clone(), limit);
        let mut im = Importer::new(x, s, t.path().join("import-tmp"), limit);
        im.ensure().unwrap();
        im.claim_wait = Duration::from_secs(3);
        im.claim_wait_slow = Duration::from_secs(3);
        (im, lib)
    }

    fn mkdir(t: &tempfile::TempDir) -> MkdirQueue {
        MkdirQueue::new(&t.path().join("state"), &t.path().join("xochitl"))
    }

    fn put_doc(lib: &Path, uuid: &str, meta: &str) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), meta).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), b"PK\x03\x04old").unwrap();
        std::fs::write(lib.join(format!("{uuid}.pdf")), b"render-cache").unwrap();
        std::fs::write(lib.join(format!("{uuid}.epubindex")), b"idx").unwrap();
        std::fs::write(lib.join(format!("{uuid}.content")), r#"{"fileType":"epub","lastOpenedPage":42}"#).unwrap();
    }

    #[test]
    fn import_new_claims_the_uuid_of_the_uploaded_bytes_and_cleans_tmp() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let (im, lib) = importer(&t, &fake_xochitl(lib), 1 << 20);
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let d = im.import_new("甲.epub", "", &mut a.as_slice(), Some(a.len()), &mkdir(&t)).unwrap();
        assert_eq!(std::fs::read(lib.join(format!("{}.epub", d.uuid))).unwrap(), a, "认出的是自己上传的那份");
        assert_eq!((d.name.as_str(), d.folder.as_str(), d.deleted), ("甲.epub", "", false));
        // 同一份字节再导入一次：认到新建的那份，不会把上一份当成自己的
        let d2 = im.import_new("甲.epub", "", &mut a.as_slice(), Some(a.len()), &mkdir(&t)).unwrap();
        assert_ne!(d.uuid, d2.uuid);
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "临时文件用完就删");
        assert!(!t.path().join("staging").exists() || std::fs::read_dir(t.path().join("staging")).unwrap().count() == 0, "不进母版库");
    }

    #[test]
    fn import_new_over_limit_goes_through_large_file_channel() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let (im, lib) = importer(&t, &fake_xochitl(lib), 100);
        let opf = r#"<package version="2.0"><metadata><dc:title>大书</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let container = r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
        let big = mini_epub(&[("META-INF/container.xml", container), ("content.opf", opf), ("c1.xhtml", &"字".repeat(500))]);
        let d = im.import_new("大书.epub", "", &mut big.as_slice(), Some(big.len()), &mkdir(&t)).unwrap();
        assert_eq!(std::fs::read(lib.join(format!("{}.epub", d.uuid))).unwrap(), big, "占位已换成真书");
    }

    #[test]
    fn import_rejects_non_epub_bad_body_and_cleans_up() {
        let t = tempfile::tempdir().unwrap();
        let (im, _) = importer(&t, "127.0.0.1:9", 1 << 20);
        let m = mkdir(&t);
        let ok = mini_epub(&[]);
        for (name, body, len, hint) in [
            ("a.pdf", ok.clone(), Some(ok.len()), "只收 .epub"),
            ("../a.epub", ok.clone(), Some(ok.len()), "非法文件名"),
            ("a.epub", vec![], Some(0), "空的"),
            ("a.epub", vec![], None, "空的"),
            ("a.epub", b"not a zip".to_vec(), Some(9), "不是 EPUB"),
            ("a.epub", ok.clone(), Some(ok.len() + 5), "不完整"),
            ("a.epub", ok.clone(), Some((MAX_DIRECT_BYTES + 1) as usize), "上限"),
        ] {
            match im.import_new(name, "", &mut body.as_slice(), len, &m) {
                Err(ImportError::Bad(e)) => assert!(e.contains(hint), "{name}: {e}"),
                other => panic!("{name}: {other:?}"),
            }
        }
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "出错也不留临时文件");
    }

    #[test]
    fn replace_keeps_uuid_and_metadata_drops_render_cache() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        std::fs::write(lib.join("ffffffff-ffff-4fff-8fff-ffffffffffff.metadata"), r#"{"type":"CollectionType","visibleName":"小说","parent":""}"#).unwrap();
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"白夜行","parent":"ffffffff-ffff-4fff-8fff-ffffffffffff","deleted":false}"#);
        let new = mini_epub(&[("OEBPS/a.xhtml", "<p>新版</p>")]);
        let d = im.replace(U, "白夜行.epub", &mut new.as_slice(), Some(new.len())).unwrap();
        assert_eq!(d, DocState { uuid: U.into(), name: "白夜行".into(), folder: "小说".into(), deleted: false });
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), new);
        assert!(!lib.join(format!("{U}.pdf")).exists() && !lib.join(format!("{U}.epubindex")).exists(), "渲染缓存删掉，下次打开重排");
        assert!(!lib.join(format!("{U}.epub.new")).exists());
        assert!(std::fs::read_to_string(lib.join(format!("{U}.content"))).unwrap().contains("\"lastOpenedPage\":42"), ".content 原样");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(lib.join(format!("{U}.epub"))).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // 坏体：原文件不动，不留 .new
        assert!(matches!(im.replace(U, "白夜行.epub", &mut &b"junk"[..], Some(4)), Err(ImportError::Bad(_))));
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), new);
        assert!(!lib.join(format!("{U}.epub.new")).exists());
    }

    #[test]
    fn replace_missing_deleted_trashed_or_non_epub_is_not_found() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        let body = mini_epub(&[]);
        let del = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let trash = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        let pdf = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
        put_doc(&lib, del, r#"{"type":"DocumentType","visibleName":"x","parent":"","deleted":true}"#);
        put_doc(&lib, trash, r#"{"type":"DocumentType","visibleName":"x","parent":"trash"}"#);
        put_doc(&lib, pdf, r#"{"type":"DocumentType","visibleName":"x","parent":""}"#);
        std::fs::remove_file(lib.join(format!("{pdf}.epub"))).unwrap();
        for u in [U, del, trash, pdf] {
            assert!(matches!(im.replace(u, "x.epub", &mut body.as_slice(), Some(body.len())), Err(ImportError::NotFound(_))), "{u}");
        }
        assert!(matches!(im.replace("../../etc", "x.epub", &mut body.as_slice(), None), Err(ImportError::Bad(_))));
        assert_eq!(std::fs::read(lib.join(format!("{del}.epub"))).unwrap(), b"PK\x03\x04old", "没动已删的书");
    }

    #[test]
    fn describe_reports_name_folder_and_deleted() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"书","parent":"trash"}"#);
        assert_eq!(im.describe(U), Some(DocState { uuid: U.into(), name: "书".into(), folder: String::new(), deleted: true }));
        assert_eq!(im.describe("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"), None);
        assert_eq!(im.describe("not-a-uuid"), None);
    }

    /// 原地替换不重新登记漫画页边距：书一直是同一个文档，`.content` 里的页边距原样保留，用户在阅读器里调过的不被覆盖；
    /// 请求体先落临时目录，收完才进书库，书库目录里不留 `.epub.new` 之类的半成品。
    #[test]
    fn replace_keeps_margins_untouched_and_leaves_no_temp_in_library() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new("127.0.0.1:9", &lib, 1));
        let q = Arc::new(crate::comic_margins::ComicMargins::new(&t.path().join("state"), &lib));
        let s = Staging::new(t.path().join("staging"), x.clone(), 0).with_comic_margins(q.clone());
        let im = Importer::new(x, s, t.path().join("import-tmp"), 0);
        im.ensure().unwrap();
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"漫画","parent":""}"#);
        let manga = mini_epub(&[(shelf_conv::epub::READER_MARGINS_MARKER, "1")]);
        im.replace(U, "漫画.epub", &mut manga.as_slice(), Some(manga.len())).unwrap();
        assert_eq!(q.get(U), None, "替换不重新登记");
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), manga);
        let stray: Vec<_> = std::fs::read_dir(&lib).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with(U) || n.ends_with(".new")).collect();
        assert!(stray.is_empty(), "书库里不该有别的文件：{stray:?}");
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "临时文件用完即走");
    }
}
