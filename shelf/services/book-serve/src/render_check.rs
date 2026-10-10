//! 投原生后的**渲染自检**：xochitl 导入 EPUB 后渲染（真机：导入当下同步渲染），渲染完在 `<uuid>.content` 写 `pageCount`。
//! 投书后起一条线程，**限时**监听书库目录（`fswatch::wait_for`，最长 [`TIMEOUT`]，结束即撤、不常驻），等这本书的页数出来就记下、
//! 并在开始时登记漫画页边距。结果写进母版库边车 `.<name>.delivered` 的 `render` 字段（事件是有损信号，状态必须落盘），
//! 并推 `books/render` 事件（带 name/status/pages）。
//! 2026-10-07 前还按正文字数估期望页数、页数远低于期望时报 `warn`（抓整章渲染失败）：那主要是设备上优化出错的症状，
//! 书改在电脑上用 sheng-ren 优化、有它的质量门把关后删掉，只记页数。
//! 认书（2026-10-10，审计 X-1）：投递时已经当场认好（基座 `Xochitl::upload_and_claim`，快照之外的新文档里按字节认，**认不出就不认**、
//! 绝不取最新一本），这里拿到的是确定的 uuid。第一阶段在这里按快照慢慢认、母版没了再按书名认的那套随之删掉。
//! 只读 `.content`，绝不写 xochitl 目录。
use crate::events as ev;
use crate::sidecar::RenderCheck;
use crate::staging::{RenderPlan, Staging};
use rmsvc_core::clock::now_secs as now;
use rmsvc_core::events::EventBus;
use rmsvc_core::fswatch::wait_for;
use rmsvc_core::wire::RenderStatus;
use rmsvc_core::xochitl::page_count;
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
    let write = |pages: u64, status: RenderStatus| {
        // 用户在自检期间把书从母版库删了（投完就删很常见）：结果没处可记，静默跳过，不当成失败刷日志。
        if !staging.has(&plan.name) {
            return;
        }
        let rc = RenderCheck { uuid: plan.uuid.clone(), pages, status, at: now() };
        if let Err(e) = staging.set_render(&plan.name, rc) {
            println!("[book-serve] 渲染自检记录《{}》失败: {e}", plan.name);
        }
        bus.publish_with(ev::AREA, ev::RENDER, serde_json::json!({"name": plan.name, "status": status, "pages": pages}));
        println!("[book-serve] 渲染自检《{}》: {status} pages={pages} uuid={}", plan.name, plan.uuid);
    };
    write(0, RenderStatus::Pending);
    // 页边距在认出的那一刻登记（书还没渲染完也一样）。
    if let Some(m) = plan.comic_margins {
        staging.delivery().register_comic_margins(&plan.uuid, &plan.name, m);
    }
    match wait_for(lib_dir, debounce, timeout, || page_count(lib_dir, &plan.uuid)) {
        Some(pages) => write(pages, RenderStatus::Ok),
        None => write(0, RenderStatus::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar;
    use rmsvc_core::xochitl::Xochitl;
    use std::io::Read;
    use std::sync::Arc;

    /// 母版库里有 a.epub，书库目录 `xochitl/`（空）；返回 (staging, 书库目录)。
    fn setup(t: &tempfile::TempDir) -> (Staging, std::path::PathBuf) {
        setup_with(t, None)
    }

    /// 同 [`setup`]，投递层接上漫画页边距队列 `margins`。
    fn setup_with(t: &tempfile::TempDir, margins: Option<Arc<crate::comic_margins::ComicMargins>>) -> (Staging, std::path::PathBuf) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new("127.0.0.1:1", &lib, 1));
        let s = crate::staging::tests::staging_in(t.path().join("staging"), x, 1024 * 1024, margins);
        s.ensure().unwrap();
        s.stage_new("a.epub", b"PK-ours").unwrap();
        (s, lib)
    }

    /// 造一份"xochitl 已渲染完"的文档：`.metadata` + `.content`（pageCount）。
    fn render_doc(lib: &Path, uuid: &str, pages: u64) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), r#"{"type":"DocumentType","visibleName":"a","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{uuid}.content")), format!(r#"{{"pageCount":{pages}}}"#)).unwrap();
    }

    fn plan(uuid: &str) -> RenderPlan {
        RenderPlan { name: "a.epub".into(), uuid: uuid.into(), comic_margins: None }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn render_of(s: &Staging) -> Option<RenderCheck> {
        sidecar::read(&s.dir().join("a.epub")).and_then(|d| d.render)
    }

    #[test]
    fn run_records_ok_when_already_rendered_and_publishes_events() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc(&lib, "u1", 100);
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        run_with(&s, &bus, &lib, &plan("u1"), MS(20), MS(200));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status, rc.pages, rc.uuid.as_str()), (RenderStatus::Ok, 100, "u1"));
        let mut buf = [0u8; 1024];
        let n = sub.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).contains(r#""kind":"render""#), "应推 books/render 事件");
    }

    #[test]
    fn run_registers_margins_only_for_comic_plans() {
        // 新版管线的漫画：登记"首次打开时设页边距"；文字书不登记。
        const U: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        for (comic, expect) in [(Some(1), Some(1)), (None, None)] {
            let t = tempfile::tempdir().unwrap();
            let q = std::sync::Arc::new(crate::comic_margins::ComicMargins::new(t.path(), &t.path().join("xochitl")));
            let (s, lib) = setup_with(&t, Some(q.clone()));
            render_doc(&lib, U, 100);
            let mut p = plan(U);
            p.comic_margins = comic;
            run_with(&s, &EventBus::new(), &lib, &p, MS(20), MS(200));
            assert_eq!(q.get(U), expect, "comic={comic:?}");
        }
    }

    /// 页数一直没出来（xochitl 延后渲染）：限时收工，记 timeout，uuid 照记（认是认出来了）。
    #[test]
    fn run_records_timeout_when_book_never_renders() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let started = std::time::Instant::now();
        run_with(&s, &EventBus::new(), &lib, &plan("u1"), MS(20), MS(150));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status, rc.pages, rc.uuid.as_str()), (RenderStatus::Timeout, 0, "u1"));
        assert!(started.elapsed() < Duration::from_secs(5), "限时监听按给定 timeout 收工");
    }

    #[test]
    fn run_picks_up_book_that_renders_after_check_started() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        let lib2 = lib.clone();
        std::thread::spawn(move || {
            std::thread::sleep(MS(200));
            render_doc(&lib2, "u2", 42);
        });
        run_with(&s, &EventBus::new(), &lib, &plan("u2"), MS(30), Duration::from_secs(10));
        let rc = render_of(&s).unwrap();
        assert_eq!((rc.status, rc.pages, rc.uuid.as_str()), (RenderStatus::Ok, 42, "u2"), "wait_for 应在页数出现后很快检出");
    }

    #[test]
    fn run_skips_silently_when_book_deleted_during_check() {
        // 投完就删很常见：结果没处可记，静默跳过——不 panic、不复活 sidecar、不推事件。
        let t = tempfile::tempdir().unwrap();
        let (s, lib) = setup(&t);
        render_doc(&lib, "u1", 100);
        s.remove("a.epub").unwrap();
        let bus = EventBus::new();
        let _sub = bus.subscribe();
        run_with(&s, &bus, &lib, &plan("u1"), MS(20), MS(100));
        assert!(sidecar::read(&s.dir().join("a.epub")).is_none(), "书已删，不该再生成边车");
        assert!(!s.has("a.epub"));
    }
}
