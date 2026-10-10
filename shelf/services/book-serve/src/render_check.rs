//! 投原生后的**渲染自检**：xochitl 导入 EPUB 后渲染（真机：导入当下同步渲染），渲染完在 `<uuid>.content` 写 `pageCount`。
//! 投书后起一条线程，**限时**监听书库目录（`fswatch::wait_for`，最长 [`TIMEOUT`]，结束即撤、不常驻），认出这本书就
//! 记下 uuid 和页数、登记漫画页边距（普通上传拿不到 uuid，只能在这里认）。结果写进母版库边车 `.<name>.delivered` 的
//! `render` 字段（事件是有损信号，状态必须落盘），并推 `books/render` 事件（带 name/status/pages）。
//! 2026-10-07 前还按正文字数估期望页数、页数远低于期望时报 `warn`（抓整章渲染失败）：那主要是设备上优化出错的症状，
//! 书改在电脑上用 sheng-ren 优化、有它的质量门把关后删掉，只记页数。
//! 认书（2026-10-10 改，审计 X-1）：`/upload` 不回 uuid → 上传前拍的书库快照之后新出现、`<uuid>.epub` 与母版逐字节相同的那份
//! （[`crate::delivery::Claim`]，与直接导入同一判据）；母版在自检期间被删 / 改名、没法比字节时，退而认 visibleName 与
//! dc:title / 文件名 stem 相符的新文档。**认不出就不认**：记 `timeout`、不登记页边距——此前"都不符时取最新一本"，
//! 并发投递时会把别人刚投的书认成自己的。只读 `.metadata/.content/.epub`，绝不写 xochitl 目录。
use crate::sidecar::RenderCheck;
use crate::staging::{RenderPlan, Staging};
use rmsvc_core::clock::now_secs as now;
use rmsvc_core::events::EventBus;
use rmsvc_core::fswatch::wait_for;
use rmsvc_core::xochitl::{page_count, DocInfo};
use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

/// 等渲染的上限；超时状态 `timeout`（不是错误：xochitl 可能延后渲染，设备上打开一次就有）。
pub const TIMEOUT: Duration = Duration::from_secs(600);
/// 书库目录写入防抖（xochitl 导入时连写 metadata/content/缩略图）。
pub const DEBOUNCE: Duration = Duration::from_secs(3);

pub fn run(staging: &Staging, bus: &EventBus, lib_dir: &Path, plan: &RenderPlan) {
    run_with(staging, bus, lib_dir, plan, DEBOUNCE, TIMEOUT)
}

/// 同 [`run`]，防抖与总时限可调（单测用毫秒级值，不真等 3 秒 / 10 分钟）。
pub fn run_with(staging: &Staging, bus: &EventBus, lib_dir: &Path, plan: &RenderPlan, debounce: Duration, timeout: Duration) {
    let write = |uuid: &str, pages: u64, status: &str| {
        // 用户在自检期间把书从母版库删了（投完就删很常见）：结果没处可记，静默跳过，不当成失败刷日志。
        if !staging.has(&plan.name) {
            return;
        }
        let rc = RenderCheck { uuid: uuid.to_string(), pages, status: status.to_string(), at: now() };
        if let Err(e) = staging.set_render(&plan.name, rc) {
            println!("[book-serve] 渲染自检记录《{}》失败: {e}", plan.name);
        }
        bus.publish_with("books", "render", serde_json::json!({"name": plan.name, "status": status, "pages": pages}));
        println!("[book-serve] 渲染自检《{}》: {status} pages={pages} uuid={uuid}", plan.name);
    };
    write("", 0, "pending");
    // 认出来就记住：之后每次书库有动静只查页数，不再逐字节比对。页边距在认出的那一刻登记（书还没渲染完也一样）。
    let claimed: RefCell<Option<String>> = RefCell::new(None);
    let check = || {
        if claimed.borrow().is_none() {
            let uuid = claim_of(lib_dir, plan)?;
            if let Some(m) = plan.comic_margins {
                staging.register_comic_margins(&uuid, &plan.name, m);
            }
            *claimed.borrow_mut() = Some(uuid);
        }
        let uuid = claimed.borrow().clone()?;
        page_count(lib_dir, &uuid).map(|pages| write(&uuid, pages, "ok"))
    };
    if wait_for(lib_dir, debounce, timeout, check).is_none() {
        let uuid = claimed.borrow().clone().unwrap_or_default();
        if uuid.is_empty() {
            println!("[book-serve] 渲染自检《{}》：{} 秒内没在书库里认出这本书（不认别人的书，也不登记页边距）", plan.name, timeout.as_secs());
        }
        write(&uuid, 0, "timeout");
    }
}

/// 在书库里找这本刚投的书并读页数；没认出或没渲染完 → None（单测用；线上在 [`run_with`] 里分两步做，认出后记住 uuid）。
#[cfg(test)]
pub fn probe(lib_dir: &Path, plan: &RenderPlan) -> Option<(String, u64)> {
    let uuid = claim_of(lib_dir, plan)?;
    page_count(lib_dir, &uuid).map(|n| (uuid, n))
}

/// 认出这本刚投的书：母版还在 → 按字节（[`crate::delivery::Claim::find`]）；母版已被删 / 改名 → 快照之后新出现的文档里按书名认（[`pick`]）。
fn claim_of(lib_dir: &Path, plan: &RenderPlan) -> Option<String> {
    if plan.path.is_file() {
        return plan.claim.find(lib_dir, &plan.path);
    }
    pick(&plan.claim.fresh(lib_dir), plan.title.as_deref(), &plan.name).map(|d| d.uuid.clone())
}

/// 候选（新→旧）里挑 visibleName 与 dc:title / 文件名 stem 相符（忽略大小写）的；都不符 → `None`（不再取最新一本）。
pub fn pick<'a>(docs: &'a [DocInfo], title: Option<&str>, name: &str) -> Option<&'a DocInfo> {
    let stem = rmsvc_core::formats::stem_of(name);
    let eq = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    docs.iter().find(|d| title.is_some_and(|t| eq(&d.visible_name, t)) || eq(&d.visible_name, stem))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(uuid: &str, name: &str, t: u64) -> DocInfo {
        DocInfo { uuid: uuid.into(), visible_name: name.into(), created_ms: t }
    }

    use crate::sidecar;
    use rmsvc_core::xochitl::Xochitl;
    use std::io::Read;
    use std::sync::Arc;

    /// 母版库里的 a.epub 的内容（xochitl 收下后 `<uuid>.epub` 与它逐字节相同）。
    const OURS: &[u8] = b"PK-ours";

    /// 母版库里有 a.epub，书库目录 `xochitl/`（空）；返回 (staging, 书库目录)。
    fn setup(t: &tempfile::TempDir) -> (Staging, std::path::PathBuf) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1));
        let s = Staging::new(t.path().join("staging"), x, 1024 * 1024);
        s.ensure().unwrap();
        s.stage_new("a.epub", OURS).unwrap();
        (s, lib)
    }

    /// 造一份"xochitl 已渲染完"的文档：`.metadata`（createdTime 晚于快照时刻）+ `.epub`（`bytes`）+ `.content`（pageCount）。
    fn render_doc_bytes(lib: &Path, uuid: &str, name: &str, pages: u64, bytes: &[u8]) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{name}","parent":"","createdTime":"5000"}}"#)).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), bytes).unwrap();
        std::fs::write(lib.join(format!("{uuid}.content")), format!(r#"{{"pageCount":{pages}}}"#)).unwrap();
    }

    /// 同 [`render_doc_bytes`]，内容就是我们投的那本。
    fn render_doc(lib: &Path, uuid: &str, name: &str, pages: u64) {
        render_doc_bytes(lib, uuid, name, pages, OURS)
    }

    fn plan_in(s: &Staging) -> RenderPlan {
        RenderPlan { name: "a.epub".into(), title: None, path: s.dir().join("a.epub"), claim: crate::delivery::Claim { since_ms: 1000, before: Default::default() }, comic_margins: None }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn render_of(s: &Staging) -> Option<RenderCheck> {
        sidecar::read(&s.dir().join("a.epub")).and_then(|d| d.render)
    }

    #[test]
    fn run_records_ok_when_already_rendered_and_publishes_events() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc(&lib, "u1", "a", 100);
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        run_with(&s, &bus, &lib, &plan_in(&s), MS(20), MS(200));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status.as_str(), rc.pages, rc.uuid.as_str()), ("ok", 100, "u1"));
        let mut buf = [0u8; 1024];
        let n = sub.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).contains(r#""kind":"render""#), "应推 books/render 事件");
    }

    #[test]
    fn run_registers_margins_only_for_comic_plans() {
        // 新版管线的漫画：导入完成（找到 uuid）后登记"首次打开时设页边距"；文字书不登记。
        const U: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        for (comic, expect) in [(Some(1), Some(1)), (None, None)] {
            let t = tempfile::tempdir().unwrap();
            let (s, lib) = setup(&t);
            let q = std::sync::Arc::new(crate::comic_margins::ComicMargins::new(t.path(), &lib));
            let s = s.with_comic_margins(q.clone());
            render_doc(&lib, U, "a", 100);
            let mut p = plan_in(&s);
            p.comic_margins = comic;
            run_with(&s, &EventBus::new(), &lib, &p, MS(20), MS(200));
            assert_eq!(q.get(U), expect, "comic={comic:?}");
        }
    }

    #[test]
    fn run_records_timeout_when_book_never_appears() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let started = std::time::Instant::now();
        run_with(&s, &EventBus::new(), &lib, &plan_in(&s), MS(20), MS(150));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status.as_str(), rc.pages), ("timeout", 0));
        assert!(started.elapsed() < Duration::from_secs(5), "限时监听按给定 timeout 收工");
    }

    #[test]
    fn run_picks_up_book_that_renders_after_check_started() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let lib2 = lib.clone();
        std::thread::spawn(move || {
            std::thread::sleep(MS(200));
            render_doc(&lib2, "u2", "a", 42);
        });
        run_with(&s, &EventBus::new(), &lib, &plan_in(&s), MS(30), Duration::from_secs(10));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status.as_str(), rc.pages, rc.uuid.as_str()), ("ok", 42, "u2"), "wait_for 应在书出现后很快检出");
    }

    #[test]
    fn run_skips_silently_when_book_deleted_during_check() {
        // 投完就删很常见：结果没处可记，静默跳过——不 panic、不复活 sidecar、不推事件。
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc(&lib, "u1", "a", 100);
        let p = plan_in(&s);
        s.remove("a.epub").unwrap();
        let bus = EventBus::new();
        let _sub = bus.subscribe();
        run_with(&s, &bus, &lib, &p, MS(20), MS(100));
        assert!(sidecar::read(&s.dir().join("a.epub")).is_none(), "书已删，不该再生成边车");
        assert!(!s.has("a.epub"));
    }

    /// 回归（2026-10-10，审计 X-1）：书库里只有别人刚投的书（书名、内容都对不上）时认不出，不再"都不符时取最新一本"——
    /// 此前会把这本的页数、uuid 记成自己的，漫画还会把页边距登记到别人的书上。
    #[test]
    fn never_claims_someone_elses_new_book() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc_bytes(&lib, "u-other", "别人的书", 100, b"PK-them");
        assert_eq!(probe(&lib, &plan_in(&s)), None);
    }

    /// 并发投递：别人的书书名跟我们的一样、比我们晚进库（新→旧排在前面），照样按字节认回自己那份；
    /// 投递前就在书库里的同字节文档（快照里有）不认。认不出时漫画不登记页边距、记 timeout。
    #[test]
    fn concurrent_delivery_claims_by_bytes_and_skips_preexisting() {
        const MINE: &str = "aaaaaaaa-0000-4000-8000-000000000001";
        const THEIRS: &str = "aaaaaaaa-0000-4000-8000-000000000002";
        const OLD: &str = "aaaaaaaa-0000-4000-8000-000000000003";
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let q = Arc::new(crate::comic_margins::ComicMargins::new(t.path(), &lib));
        let s = s.with_comic_margins(q.clone());
        render_doc(&lib, MINE, "a", 10);
        std::fs::write(lib.join(format!("{THEIRS}.metadata")), r#"{"type":"DocumentType","visibleName":"a","parent":"","createdTime":"9000"}"#).unwrap();
        std::fs::write(lib.join(format!("{THEIRS}.epub")), b"PK-them").unwrap();
        std::fs::write(lib.join(format!("{THEIRS}.content")), r#"{"pageCount":99}"#).unwrap();
        let mut p = plan_in(&s);
        p.comic_margins = Some(1);
        run_with(&s, &EventBus::new(), &lib, &p, MS(20), MS(200));
        assert_eq!((render_of(&s).unwrap().uuid.as_str(), q.get(MINE), q.get(THEIRS)), (MINE, Some(1), None));
        // 只有投递前就在的同字节文档：不认，不登记
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let q = Arc::new(crate::comic_margins::ComicMargins::new(t.path(), &lib));
        let s = s.with_comic_margins(q.clone());
        render_doc(&lib, OLD, "a", 10);
        let mut p = plan_in(&s);
        p.claim.before.insert(OLD.into());
        p.comic_margins = Some(1);
        run_with(&s, &EventBus::new(), &lib, &p, MS(20), MS(150));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status.as_str(), rc.uuid.as_str(), q.get(OLD)), ("timeout", "", None));
    }

    /// 母版在自检期间被删 / 改名（没法比字节）：退而按书名认；书名也对不上就不认。
    #[test]
    fn falls_back_to_title_only_when_master_is_gone() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc_bytes(&lib, "u-t", "Tell Me Your Dreams", 7, b"PK-rewritten");
        let mut p = plan_in(&s);
        p.title = Some("tell me your dreams".into());
        assert_eq!(probe(&lib, &p), None, "母版还在时只认字节");
        s.remove("a.epub").unwrap();
        assert_eq!(probe(&lib, &p), Some(("u-t".to_string(), 7)), "母版没了按书名认");
        p.title = None;
        assert_eq!(probe(&lib, &p), None, "书名也对不上：不认");
    }

    #[test]
    fn picks_by_title_then_stem_never_newest() {
        let docs = vec![d("n", "Other", 3), d("t", "tell me your dreams", 2), d("s", "Tell Me Your Dreams (v6)", 1)];
        assert_eq!(pick(&docs, Some("Tell Me Your Dreams"), "Tell Me Your Dreams (v6).epub").unwrap().uuid, "t", "dc:title 优先（忽略大小写）");
        assert_eq!(pick(&docs, None, "Tell Me Your Dreams (v6).epub").unwrap().uuid, "s", "退而按文件名 stem");
        assert!(pick(&docs, Some("没有的"), "x.epub").is_none(), "都不符不认（此前取最新一本）");
        assert!(pick(&[], Some("a"), "a.epub").is_none());
    }
}
