//! 服务组合根：配置、inbox 队列、母版库、xochitl 客户端；inbox 追平处理。
use crate::agent_failures::AgentFailures;
use crate::config::BookConfig;
use crate::delivery::XochitlDelivery;
use crate::mkdir::MkdirQueue;
use crate::spool::Spool;
use crate::staging::{self, Staging};
use crate::comic_margins::ComicMargins;
use crate::trash::TrashQueue;
use serde::Serialize;
use rmsvc_core::cache::TtlCache;
use rmsvc_core::events::EventBus;
use rmsvc_core::formats::{self, BOOK_EXTS};
use rmsvc_core::paths::Paths;
use rmsvc_core::xochitl::Xochitl;
use std::sync::Arc;
use std::time::Duration;
use crate::events as ev;

pub struct State {
    pub cfg: BookConfig,
    pub spool: Spool,
    pub staging: Staging,
    pub xochitl: Arc<Xochitl>,
    /// 事件总线：母版库/inbox 每次变更发一条，网关汇聚推给网页（零轮询）。
    pub bus: Arc<EventBus>,
    /// 原生书库「移进回收站」队列（QML 代理 shelf-trash-agent.qmd 拉取执行）。
    pub trash: TrashQueue,
    /// 漫画「页边距」待办（QML 代理 shelf-comic-margins.qmd 在书打开时查、设完销账，见 comic_margins.rs）。
    pub comic_margins: Arc<ComicMargins>,
    /// 原地替换后找回阅读位置的快照（QML 代理 shelf-keep-progress.qmd 在书打开时查、跳完销账，见 progress.rs）。
    pub progress: Arc<crate::progress::Progress>,
    /// 原生书库「建文件夹」队列（QML 代理 shelf-mkdir-agent.qmd 拉取执行，2026-09-19 复活，
    /// 见 mkdir.rs 模块文档）；`Arc`：投递层（`XochitlDelivery`）也持一份。
    pub mkdir: Arc<MkdirQueue>,
    /// 回收站 / 建文件夹代理执行不成、已放弃的记录（网页页头横幅，见 agent_failures.rs）。
    pub agent_failures: Arc<AgentFailures>,
    /// 直接导入 xochitl（不进母版库，sheng-ren 经 SSH 端口转发调，见 import.rs）。
    pub import: crate::import::Importer,
    /// 投进 xochitl 的后台作业队列：直接导入与母版库落库共用，一次一个（导入任务的结果在内存里留 1 小时，见 jobs.rs）。
    pub jobs: crate::jobs::Jobs,
    /// `GET /status` 的结果缓存（[`STATUS_TTL`]）。网页全量刷新时打这个接口，里面要读全部 `.metadata` 列文件夹。
    /// 本服务自己可能建出新文件夹的操作（直接导入）完成后 [`State::invalidate_status`] 主动失效；
    /// 用户在设备上新建文件夹这类外部变化最多滞后一个 TTL。
    /// `Arc`：直接导入的后台任务做完时也要让它失效（见 [`State::status_invalidator`]）。
    status_cache: Arc<TtlCache<serde_json::Value>>,
    /// inbox 文件修改时间静止多久才算"写完了"（见 [`State::process_inbox_counting_deferred`]）。缺省 [`INBOX_SETTLE`]，
    /// 须小于 inbox 监听的防抖时长（8 秒），写完那次事件触发的追平才不会再被暂缓。
    pub inbox_settle: Duration,
}

/// 见 [`State::inbox_settle`]。
pub const INBOX_SETTLE: Duration = Duration::from_secs(5);

/// `/status` 缓存时长：够挡住"连续几次 refresh"，又短到外部变化（xochitl 上下线）几秒内就能看到。
const STATUS_TTL: Duration = Duration::from_secs(3);

/// inbox 追平一项的结果（日志）。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct InboxOutcome {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

/// 书库根下的活文件夹名（去重排序）。
fn root_folders(lib: &std::path::Path) -> Vec<String> {
    let names: std::collections::BTreeSet<String> = rmsvc_core::xochitl::live_entries(lib).into_iter().filter(|(_, m)| m.is_folder() && m.parent.is_empty()).map(|(_, m)| m.visible_name).collect();
    names.into_iter().collect()
}

impl State {
    pub fn new(paths: &Paths) -> State {
        let cfg = BookConfig::load(paths);
        let xochitl = Arc::new(Xochitl::new(&cfg.xochitl_host, &paths.xochitl_dir(), cfg.upload_timeout_secs));
        let books_state = paths.state_dir().join("books"); // inbox/.work/failed 与三个待办队列共用的状态目录
        let spool = Spool::new(books_state.clone());
        let comic_margins = Arc::new(ComicMargins::new(&books_state, &paths.xochitl_dir()));
        let progress = Arc::new(crate::progress::Progress::new(&books_state, &paths.xochitl_dir()));
        let bus = Arc::new(EventBus::new());
        let agent_failures = Arc::new(AgentFailures::new(&books_state, Some(bus.clone())));
        let trash = TrashQueue::new(&books_state, &paths.xochitl_dir()).with_failures(agent_failures.clone());
        let mkdir = Arc::new(MkdirQueue::new(&books_state, &paths.xochitl_dir()).with_failures(agent_failures.clone()));
        // 投进 xochitl 那一层只组装这一份，母版库落库与直接导入共用（见 delivery.rs）。
        let delivery = Arc::new(XochitlDelivery::new(xochitl.clone(), mkdir.clone(), cfg.native_upload_limit_bytes()).with_comic_margins(comic_margins.clone()));
        // 母版库（中间层暂存池）：所有内容源先原样落这里，用户再加入 xochitl。与 spool 同根，在 /home 分区，重启 / OTA 不丢；
        // 不套 spool done/ 的 LRU 淘汰——留住用户还没落库的书。路径 2026-10-10 前由基座 `Paths::staging_dir` 给（只有本服务用，审计 CORE-3），取值不变。
        let staging = Staging::new(books_state.join("staging"), delivery.clone());
        let import = crate::import::Importer::new(delivery, books_state.join("import-tmp")).with_progress(progress.clone());
        State { cfg, spool, staging, xochitl, bus, trash, comic_margins, progress, mkdir, agent_failures, import, jobs: crate::jobs::Jobs::default(), status_cache: Arc::new(TtlCache::new(STATUS_TTL)), inbox_settle: INBOX_SETTLE }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        self.spool.ensure()?;
        self.staging.ensure()?;
        let parts = self.import.ensure()?;
        if parts > 0 {
            println!("[book-serve] 清掉 {parts} 个上次没传完的直接导入临时文件");
        }
        let stale_margins = self.comic_margins.prune_missing();
        if stale_margins > 0 {
            println!("[book-serve] 清掉 {stale_margins} 条书已不在库里的页边距待办");
        }
        let stale_progress = self.progress.prune(crate::progress::MAX_AGE);
        if stale_progress > 0 {
            println!("[book-serve] 清掉 {stale_progress} 份过期（或书已不在）的阅读位置快照");
        }
        let (fixed, tmps) = self.staging.recover_interrupted();
        if fixed > 0 || tmps > 0 {
            println!("[book-serve] 修正 {fixed} 条上次被中断的处理记录，清掉 {tmps} 个半成品临时文件");
        }
        let orphans = self.staging.gc_orphan_sidecars();
        if orphans > 0 {
            println!("[book-serve] 清掉 {orphans} 个没有对应书的落库记录");
        }
        Ok(())
    }

    /// 让下一次 `/status` 必定重算（可能建出新文件夹的操作完成后调）。
    pub fn invalidate_status(&self) {
        self.status_cache.invalidate();
    }

    /// 同 [`State::invalidate_status`]，但可以带进后台线程（直接导入的任务做完时调）。
    pub fn status_invalidator(&self) -> impl Fn() + Send + 'static {
        let c = self.status_cache.clone();
        move || c.invalidate()
    }

    pub fn status(&self) -> serde_json::Value {
        self.status_cache.get_or(|| self.compute_status())
    }

    fn compute_status(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            // 原生书库**根下**真实存在的文件夹名（去重排序），给网页「加入原生书库 → 文件夹」下拉候选用——
            // 2026-09-19 取代原来写死的「书库/批注/自定义」三选一预设（`annotFolder` 已删）。
            // 2026-10-10 起只列根下的：落库按名字只在书库根正下方找 / 建（`XochitlDelivery::ensure_folder`），列出子文件夹的名字，
            // 选了它反而会在根下另建一个同名文件夹。
            "xochitlFolders": root_folders(self.xochitl.library_dir()),
            // 2026-10-07 删掉网页从来没读过的三项：`uploadReachable`（xochitl 不在时每次白等 3 秒探活）、
            // `nativeUploadLimitBytes`（有大文件通道后不再灰掉超限书）、`spool`（inbox 待处理/失败计数）。
        })
    }

    /// 处理 inbox（scp 丢进来的 / 重试的）：**原样落母版库**（与网页上传同一规则：所有书只落母版库，去向在网页选）。
    /// 非书籍格式进 failed/ 带原因、不反复重试。
    pub fn process_inbox(&self) -> Vec<InboxOutcome> {
        self.process_inbox_counting_deferred().0
    }

    /// 同 [`Self::process_inbox`]，另返回因"还在写"（修改时间离现在不足 [`State::inbox_settle`]）而暂缓的文件数。
    ///
    /// **正在写的文件不动**（2026-09-25 第四轮审计）：scp 直接往最终文件名里写，一建出文件就有 CREATE 事件，防抖 8 秒后
    /// 追平——WiFi 传大书超过 8 秒时，这里会把还在写的文件认领、改名进母版库，写入者手里的 fd 跟着 inode 继续写，
    /// 母版库里于是出现一本半截书（可被落库）。写完时的 CLOSE_WRITE 会再触发一轮追平，
    /// 那时修改时间已经静止超过防抖时长，照常处理。
    pub fn process_inbox_counting_deferred(&self) -> (Vec<InboxOutcome>, usize) {
        let _g = self.spool.guard();
        let mut out = Vec::new();
        let mut deferred = 0;
        let Ok(rd) = std::fs::read_dir(self.spool.inbox()) else { return (out, 0) };
        let now = std::time::SystemTime::now();
        for e in rd.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else { continue };
            if !p.is_file() || name.starts_with('.') {
                continue; // 半成品不动
            }
            let fresh = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|m| now.duration_since(m).ok()).is_some_and(|age| age < self.inbox_settle);
            if fresh {
                deferred += 1;
                continue;
            }
            let Some(work) = self.spool.claim(&name) else { continue };
            let res = if formats::has_ext(&name, BOOK_EXTS) { self.staging.stage_from_path(&name, &work) } else { Err(staging::reject_message()) };
            let o = match res {
                Ok(landed) => InboxOutcome { name: landed, ok: true, message: "已入母版库".into() },
                Err(e) => {
                    self.spool.archive_failed(&work, &e);
                    InboxOutcome { name: name.clone(), ok: false, message: e }
                }
            };
            println!("[book-serve] inbox {} → {}: {}", name, if o.ok { "ok" } else { "fail" }, o.message);
            out.push(o);
        }
        if !out.is_empty() {
            self.bus.publish(ev::AREA, ev::INBOX);
            if out.iter().any(|o| o.ok) {
                self.bus.publish(ev::AREA, ev::STAGING);
            }
        }
        (out, deferred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_lands_books_and_fails_non_books() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let mut st = State::new(&paths);
        st.inbox_settle = Duration::ZERO;
        st.ensure_dirs().unwrap();
        // 2026-09-18 起母版库只收 EPUB/PDF（cbz 已随"仅 KOReader"档退役），接受项夹具改用 .epub。
        std::fs::write(st.spool.inbox().join("b.epub"), b"x").unwrap();
        std::fs::write(st.spool.inbox().join("p.jpg"), b"x").unwrap();
        let out = st.process_inbox();
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|o| o.ok && o.name == "b.epub"));
        assert!(out.iter().any(|o| !o.ok && o.name == "p.jpg" && o.message.contains("不是书籍格式")));
        assert!(st.staging.dir().join("b.epub").is_file() && !st.spool.inbox().join("b.epub").exists());
        assert_eq!(st.spool.list().iter().filter(|e| e.state == "failed").count(), 1);
    }

    /// 造一个 xochitl 指向本机关闭端口（连接秒拒，不会真等 3 秒超时）的 State。
    fn state_with_dead_xochitl(home: &std::path::Path) -> State {
        let paths = Paths::sandbox(home);
        let cfg = paths.service_config("book");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, r#"{"xochitlHost":"127.0.0.1:9"}"#).unwrap();
        let mut st = State::new(&paths);
        st.inbox_settle = Duration::ZERO;
        st.ensure_dirs().unwrap();
        st
    }

    #[test]
    fn status_lists_folders_cached_within_ttl_until_invalidated() {
        let t = tempfile::tempdir().unwrap();
        let st = state_with_dead_xochitl(t.path());
        let lib = st.xochitl.library_dir().to_path_buf();
        std::fs::create_dir_all(&lib).unwrap();
        assert_eq!(st.status()["xochitlFolders"], serde_json::json!([]));
        // 设备上新建了文件夹：TTL 内仍是缓存的旧值（不重读全部 .metadata）
        std::fs::write(lib.join("f1.metadata"), r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#).unwrap();
        // 子文件夹不列（落库只在根下按名字找，见 root_folders）
        std::fs::write(lib.join("f2.metadata"), r#"{"type":"CollectionType","visibleName":"卷01","parent":"f1"}"#).unwrap();
        assert_eq!(st.status()["xochitlFolders"], serde_json::json!([]), "TTL 内命中缓存");
        st.invalidate_status();
        let s = st.status();
        assert_eq!((s["ok"].clone(), s["xochitlFolders"].clone()), (serde_json::json!(true), serde_json::json!(["漫画"])));
        assert!(s.get("uploadReachable").is_none() && s.get("spool").is_none(), "网页不读的字段已删");
    }

    /// 回归：还在写的文件（修改时间离现在不足 settle）不认领，写完静止后照常入库。
    #[test]
    fn inbox_defers_files_still_being_written() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let st = State::new(&paths);
        st.ensure_dirs().unwrap();
        let f = st.spool.inbox().join("scp.epub");
        std::fs::write(&f, b"half").unwrap();
        let (out, deferred) = st.process_inbox_counting_deferred();
        assert!(out.is_empty() && deferred == 1, "刚写的文件暂缓");
        assert!(f.exists() && !st.staging.has("scp.epub"));
        let old = std::time::SystemTime::now() - INBOX_SETTLE - Duration::from_secs(1);
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(old).unwrap();
        let (out, deferred) = st.process_inbox_counting_deferred();
        assert_eq!((out.len(), deferred), (1, 0));
        assert!(st.staging.has("scp.epub"));
    }

    #[test]
    fn inbox_rejects_retired_host_convertible_exts() {
        // 2026-09-17 EPUB 线架构调整：azw3/mobi/fb2/txt 不再自动转 EPUB，母版库直接拒收。
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let mut st = State::new(&paths);
        st.inbox_settle = Duration::ZERO;
        st.ensure_dirs().unwrap();
        for name in ["b.azw3", "b.mobi", "b.fb2", "b.txt"] {
            std::fs::write(st.spool.inbox().join(name), b"x").unwrap();
        }
        let out = st.process_inbox();
        assert!(out.iter().all(|o| !o.ok && o.message.contains("不是书籍格式")), "{out:?}");
    }
}
