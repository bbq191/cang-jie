//! ink-serve —— 笔记·矿（loopback 8795）。监听原生书库（事件驱动、防抖），书页 `.rm` 变了就只扫变更页：
//! 勾画（GlyphRange）+ 旁边手写（笔画簇）→ 条目 → 裁图 → 条目库（**唯一写者**，其它服务经这里改字段）。
//! 路由（经网关前缀 `/api/ink`；请求/应答形状见 `notecore::api`，收发两端共用）：
//! `GET /books`（只列还有活条目的书）· `GET /books/{uuid}`（每条条目带 `live`，只在应答里、不落盘）·
//! `GET /books/{uuid}/crops/{file}` · `GET /search` ·
//! `POST /books/{uuid}/entries/{id}`（网页改字：`text`/`askAi`/`question`/`destination`，`EntryPatch`；`text` 的行首
//! `-`/`1.`/`- [ ]`/`口`/`##`/`### ` 标记定样式并剥掉，见 `Entry::apply_marked_text`；未知字段/形状不对 400）·
//! `POST /books/{uuid}/entries/{id}/draft`（transcribe-serve 写草稿：转写原文，`Entry::accept_draft` 统一剥标记、
//! 样式只作建议）· `POST /books/{uuid}/entries/{id}/answer`（mind-serve 写回答）·
//! `POST /books/{uuid}/entries/{id}/request`|`skip`|`archive`（浏览/整理动作，见 `Entry::set_triage`；终态一律拒绝）·
//! `POST /books/{uuid}/entries/{id}/restore`（回收站恢复，见 `Entry::restore`）·
//! `POST /books/{uuid}/purge`（清空回收站，手动、不可恢复，见 `Book::purge_terminal`）·
//! `POST /books/{uuid}/rescan` · `GET /events`。条目库 `$XDG_STATE_HOME/notes/books/<uuid>.json`，裁图 `$XDG_DATA_HOME/notes/crops/`。
//! 条目状态只经 `notecore::model::Entry` 的方法改（2026-10-10 收拢，NT-1），这里不直接给 `status` 赋值。
//! 原 `POST /koreader/import` 随 KOReader 卸载已删（2026-09-30）；条目库里以前导入的 KOReader 书仍保留，见 `ingest::ingest_doc` 的早退。
mod bookdb;
mod config;
mod crop;
mod doc;
mod ingest;
mod search;

use bookdb::BookDb;
use config::IngestConfig;
use notecore::api::{AnswerPost, BookBrief, BookList, BookView, DraftPost, EntryPatch};
use notecore::model::{Entry, Status};
use rmsvc_core::events::EventBus;
use rmsvc_core::fs::plain_name;
use rmsvc_core::http::{bind, ApiError, ApiResult, Reply, Request, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::Arc;

pub const APP: &str = "notes";

/// 本服务发的事件（网关按 `area` 路由到笔记页；线上值不能改，前端与 transcribe-serve 按这两个字面量认）。
const EVENT_AREA: &str = "notes";
/// 条目库变了（摄取、改字段、清理）。
const EVENT_ENTRIES: &str = "entries";

const SPEC: ServiceSpec = ServiceSpec { name: "ink-serve", label: "笔记·矿", version: env!("CARGO_PKG_VERSION"), default_bind: "127.0.0.1:8795", tab: None };

struct State {
    paths: Paths,
    cfg: IngestConfig,
    db: BookDb,
    bus: Arc<EventBus>,
}

impl State {
    fn crops_dir(&self) -> std::path::PathBuf {
        self.paths.app_data_dir(APP).join("crops")
    }
    fn ingest(&self, uuid: &str) {
        // 兜住解析 panic（`.rm`/`.epubindex` 是设备写的二进制，解析器难保对所有畸形输入都不 panic）：摄取跑在
        // 书库监听线程里，一次 panic 会让监听线程整个退出、之后再也不摄取，而 HTTP 照常应答、看不出异常。
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ingest::ingest_doc(&self.paths.xochitl_dir(), &self.crops_dir(), &self.db, &self.cfg, uuid, rmsvc_core::clock::now_secs())));
        let r = r.unwrap_or_else(|_| Err("摄取时 panic（已兜住，本书这次跳过）".to_string()));
        match r {
            // `s.merge.revoked > 0` 单独成立的情况＝书被移进回收站/删除、`revoke_stale` 撤了条目但没扫任何页（pages==0）；
            // 这时也要发事件，不然网页「笔记」列表要等到下一次不相干的事件才会把这本书摘掉。
            Ok(Some(s)) if s.pages > 0 || s.merge.revoked > 0 => {
                println!("[ink-serve] {uuid}: 页 {} 新增 {} 变更 {} 不变 {} 撤销 {} 复活 {}", s.pages, s.merge.added, s.merge.changed, s.merge.unchanged, s.merge.revoked, s.merge.revived);
                self.bus.publish(EVENT_AREA, EVENT_ENTRIES);
            }
            Ok(_) => {}
            Err(e) => eprintln!("[ink-serve] {uuid}: {e}"),
        }
    }
}

/// 对一本书里的一条条目做「读—改—写」，并统一映射结果：书/条目不存在 → 404；业务规则拒绝（`f` 返回 `Err`）→ 400；
/// 成功 → 发 `entries` 事件。改字段（POST entries/{id}）、浏览态动作、回收站恢复共用（此前各写一遍逐行相同的
/// "先 load 判存在 → update → 三分支 match → publish"）。
fn edit_entry(s: &State, uuid: &str, id: &str, f: impl FnOnce(&mut Entry) -> Result<(), String>) -> ApiResult {
    match s.db.update_existing(uuid, |b| b.entries.iter_mut().find(|e| e.id == id).map(f)).map_err(ApiError::internal)? {
        None => return Err(ApiError::not_found("没有这本书的条目")),
        Some(None) => return Err(ApiError::not_found("没有这条目")),
        Some(Some(Err(e))) => return Err(ApiError::bad(e)),
        Some(Some(Ok(()))) => {}
    }
    s.bus.publish(EVENT_AREA, EVENT_ENTRIES);
    Ok(Reply::ok(&serde_json::json!({"ok": true})))
}

/// 浏览态动作：`Mined→Pending`（转入笔记）/ `Mined→Skipped`（不需要），见 `notecore::model::Entry::set_triage`。
fn triage(s: &State, r: &mut Request<'_>, target: Status) -> ApiResult {
    let now = rmsvc_core::clock::now_secs();
    edit_entry(s, r.param("uuid"), r.param("id"), |e| e.set_triage(target, now))
}

/// 请求体 → 契约类型（`notecore::api`，`deny_unknown_fields`）：JSON 坏了、字段名/形状不对一律 400，
/// 不再像此前那样逐字段 `from_value(..).ok()` 静默丢掉（`answer` 形状写错曾直接把已有回答清空，NT-2）。
fn decode<T: serde::de::DeserializeOwned>(r: &mut Request<'_>) -> Result<T, ApiError> {
    let bytes = r.small_body()?; // 超限 413、读失败 400
    serde_json::from_slice(&bytes).map_err(|e| ApiError::bad(format!("请求体格式不对: {e}")))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let cfg: IngestConfig = rmsvc_core::config::load_or_seed(&paths.app_config_dir(APP).join("ink.json"));
    let db = BookDb::new(paths.app_state_dir(APP).join("books"));
    let st = Arc::new(State { paths: paths.clone(), cfg, db, bus: Arc::new(EventBus::new()) });
    if let Err(e) = std::fs::create_dir_all(st.crops_dir()).and_then(|_| st.db.ensure()) {
        eprintln!("[ink-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    // 追平 + 监听：启动扫一遍有手写页的 EPUB，**外加**条目库里已知但这次没扫到的书（回收站/删除清场，见
    // `ingest::revoke_stale`——上次运行之后被移到回收站/删掉的书，不追平一次不会被摘出网页列表）；
    // 之后书库目录有写入（合上书 xochitl 重写 .content/.metadata）防抖后只扫涉及的文档。
    {
        let st = st.clone();
        std::thread::spawn(move || {
            let mut catchup: std::collections::BTreeSet<String> = ingest::candidate_docs(&st.paths.xochitl_dir()).into_iter().collect();
            catchup.extend(st.db.list().into_iter().map(|b| b.uuid.clone()));
            for u in catchup {
                if let Err(e) = ingest::forget_unmapped_pages(&st.db, &u) {
                    eprintln!("[ink-serve] {u}: {e}");
                }
                st.ingest(&u);
            }
            let lib = st.paths.xochitl_dir();
            let debounce = std::time::Duration::from_secs(st.cfg.debounce_secs.max(1));
            rmsvc_core::fswatch::watch_debounced(&lib, debounce, |names| {
                let mut seen = std::collections::BTreeSet::new();
                for n in names {
                    if let Some(u) = doc::uuid_of_event(n) {
                        seen.insert(u.to_string());
                    }
                }
                for u in seen {
                    st.ingest(&u);
                }
            });
        });
    }
    let router = router(&st);
    println!("[ink-serve] 条目库 {}；裁图 {}；监听 {}", st.db.dir().display(), st.crops_dir().display(), st.paths.xochitl_dir().display());
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default())
}

/// 路由表（单独成函数，测试直接对它发请求）。
fn router(st: &Arc<State>) -> Router {
    Router::new()
        .get("/events", bind(st, |s, r| Ok(s.bus.sse_reply_for(r))))
        .get("/books", bind(st, |s, _| {
            Ok(Reply::ok(&BookList { items: s.db.list_active().iter().map(|b| BookBrief::of(b)).collect() }))
        }))
        // 全文搜索：跨书搜勾画原文/定稿/草稿/提问/AI 回答/书名，见 search.rs。`limit` 缺省 50、上限 200。
        .get("/search", bind(st, |s, r| {
            let q = r.q("q").unwrap_or_default();
            let limit = r.q_parse::<usize>("limit").unwrap_or(50).clamp(1, 200);
            Ok(Reply::ok(&serde_json::json!({"items": search::search(&s.db.list(), q, limit)})))
        }))
        .get("/books/{uuid}", bind(st, |s, r| {
            let b = s.db.read(r.param("uuid")).map_err(ApiError::internal)?.ok_or_else(|| ApiError::not_found("没有这本书的条目"))?;
            // 每条条目带上 `live`（只在应答里，不落盘），网页不再自己抄一份"哪些状态算活"的判据。
            Ok(Reply::ok(&BookView::of(&b)))
        }))
        .get("/books/{uuid}/crops/{file}", bind(st, |s, r| {
            let f = plain_name(r.param("file")).map_err(ApiError::bad)?;
            let bytes = std::fs::read(s.crops_dir().join(f)).map_err(|_| ApiError::not_found("没有这张裁图"))?;
            // 裁图名带笔迹哈希（`<条目id>-<hash>.png`，笔迹一变就换新名字、旧图删掉），内容永不变：让浏览器长期缓存，
            // 笔记页每次重画不再重新下载（网关转发时透传这个头）。
            Ok(Reply::bytes(rmsvc_core::formats::mime_of(f), bytes).with_header("Cache-Control", "private, max-age=31536000, immutable"))
        }))
        .post("/books/{uuid}/entries/{id}", bind(st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            let p: EntryPatch = decode(r)?;
            let now = rmsvc_core::clock::now_secs();
            edit_entry(s, &uuid, &id, |e| p.apply(e, now))
        }))
        // 转写服务写草稿（原文，行首标记由 `Entry::accept_draft` 统一剥、样式只作建议，见 NT-1）。
        .post("/books/{uuid}/entries/{id}/draft", bind(st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            let d: DraftPost = decode(r)?;
            let now = rmsvc_core::clock::now_secs();
            edit_entry(s, &uuid, &id, |e| e.accept_draft(&d.text, &d.backend, &d.hash, now))
        }))
        // 问 AI 服务写回答。形状不对在 `decode` 就 400，碰不到已有回答。
        .post("/books/{uuid}/entries/{id}/answer", bind(st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            let a: AnswerPost = decode(r)?;
            let now = rmsvc_core::clock::now_secs();
            edit_entry(s, &uuid, &id, |e| e.accept_answer(a.into_answer(now), now))
        }))
        .post("/books/{uuid}/entries/{id}/request", bind(st, |s, r| triage(s, r, Status::Pending)))
        .post("/books/{uuid}/entries/{id}/skip", bind(st, |s, r| triage(s, r, Status::Skipped)))
        .post("/books/{uuid}/entries/{id}/archive", bind(st, |s, r| triage(s, r, Status::Archived)))
        .post("/books/{uuid}/entries/{id}/restore", bind(st, |s, r| {
            let now = rmsvc_core::clock::now_secs();
            edit_entry(s, r.param("uuid"), r.param("id"), |e| e.restore(now))
        }))
        .post("/books/{uuid}/purge", bind(st, |s, r| {
            let removed = ingest::purge_terminal(&s.db, &s.crops_dir(), r.param("uuid")).map_err(ApiError::internal)?.ok_or_else(|| ApiError::not_found("没有这本书的条目"))?;
            if removed > 0 {
                s.bus.publish(EVENT_AREA, EVENT_ENTRIES);
            }
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed})))
        }))
        .post("/books/{uuid}/rescan", bind(st, |s, r| {
            let uuid = plain_name(r.param("uuid")).map_err(ApiError::bad)?.to_string(); // ingest 会拼 xochitl 目录路径，同样要防穿越
            // 强制：清掉页 mtime 记录再摄取
            // 只对条目库里已有的书清（`update_existing`）：此前用 `update(.., Default::default)` 会给一个从未摄取过的 uuid
            // 建出一份 uuid/标题都是空串的空书，之后摄取沿用它、书就永远带着空 uuid。
            // 条目库读不出（文件坏了）要报出来，不能吞掉再假装"已重扫"。
            s.db.update_existing(&uuid, |b| b.page_mtimes.clear()).map_err(ApiError::internal)?;
            s.ingest(&uuid);
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::{Method, TestRequest};

    /// 沙箱里起一份 State（HOME/XDG 全在临时目录，不碰开发机），条目库放一本书一条条目（已有回答）。
    fn setup() -> (tempfile::TempDir, Arc<State>, Router) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let db = BookDb::new(paths.app_state_dir(APP).join("books"));
        db.ensure().unwrap();
        let e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0,"status":"pending","answer":{"text":"旧回答","backend":"b","at":1,"brief":"问"}}"#).unwrap();
        db.update("u", || notecore::model::Book { uuid: "u".into(), title: "书".into(), ..Default::default() }, |b| b.entries.push(e)).unwrap();
        let st = Arc::new(State { paths, cfg: IngestConfig::default(), db, bus: Arc::new(EventBus::new()) });
        let r = router(&st);
        (t, st, r)
    }
    fn call(r: &Router, method: Method, path: &str, body: &str) -> (u16, serde_json::Value) {
        let rep = TestRequest::new(method, path).content_type("application/json").body(body).dispatch(r);
        (rep.status, serde_json::from_slice(rep.body.as_bytes()).unwrap_or_default())
    }
    fn entry_of(st: &State) -> Entry {
        st.db.load("u").unwrap().entries[0].clone()
    }

    /// NT-2：回答形状不对（走老的通用端点、或新端点缺字段）一律 400，已有回答原样不动；形状对才替换。
    #[test]
    fn misshaped_answer_is_400_and_never_clears_existing_answer() {
        let (_t, st, r) = setup();
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e", r#"{"answer":{"text":1}}"#).0, 400);
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e", r#"{"answer":null}"#).0, 400);
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e/answer", r#"{"text":"新"}"#).0, 400);
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e/answer", "不是 JSON").0, 400);
        let huge = format!(r#"{{"text":"{}","backend":"b","brief":"q"}}"#, "x".repeat(rmsvc_core::http::SMALL_BODY_MAX as usize));
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e/answer", &huge).0, 413, "超限是 413，不是 400");
        assert_eq!(entry_of(&st).answer.unwrap().text, "旧回答");
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e", r#"{"askAi":"yes"}"#).0, 400, "类型不对不再悄悄当没给");
        assert!(!entry_of(&st).ask_ai);

        assert_eq!(call(&r, Method::Post, "/books/u/entries/e/answer", r#"{"text":"新回答","backend":"qwen-plus","brief":"问"}"#).0, 200);
        let a = entry_of(&st).answer.unwrap();
        assert_eq!((a.text.as_str(), a.backend.as_str()), ("新回答", "qwen-plus"));
    }

    /// NT-1 端到端：转写服务送原文，再转写（样式已是圆点）后草稿里也不留 `- `。
    #[test]
    fn draft_endpoint_strips_marker_every_time() {
        let (_t, st, r) = setup();
        for h in ["h1", "h2"] {
            let (code, _) = call(&r, Method::Post, "/books/u/entries/e/draft", &format!(r#"{{"text":"- 查作者","backend":"qwen3-vl-plus","hash":"{h}"}}"#));
            assert_eq!(code, 200);
        }
        let e = entry_of(&st);
        assert_eq!((e.style, e.status, e.display_text()), (notecore::model::Style::Bullet, Status::Draft, Some("查作者")));
        assert_eq!(call(&r, Method::Post, "/books/u/entries/e/draft", r#"{"draft":{"text":"x"},"style":"bullet"}"#).0, 400, "老的请求体形状不再接受");
        assert_eq!(call(&r, Method::Post, "/books/u/entries/nope/draft", r#"{"text":"x","backend":"b","hash":"h"}"#).0, 404);
    }

    /// 契约字段 `live`：只在应答里，条目库文件里没有。
    #[test]
    fn book_reply_has_live_but_disk_file_does_not() {
        let (_t, st, r) = setup();
        let (code, v) = call(&r, Method::Get, "/books/u", "");
        assert_eq!(code, 200);
        assert_eq!(v["entries"][0]["live"], true, "pending 算活");
        assert_eq!(v["entries"][0]["status"], "pending");
        call(&r, Method::Post, "/books/u/entries/e/archive", "");
        assert_eq!(call(&r, Method::Get, "/books/u", "").1["entries"][0]["live"], false);
        let disk = std::fs::read_to_string(st.db.dir().join("u.json")).unwrap();
        assert!(!disk.contains("\"live\""), "live 不落盘: {disk}");
        let (_, list) = call(&r, Method::Get, "/books", "");
        assert_eq!(list["items"][0]["uuid"], "u");
    }

    /// 条目库坏了：rescan 报 500，不再吞掉错误回 ok。
    #[test]
    fn rescan_reports_corrupt_book() {
        let (_t, st, r) = setup();
        std::fs::write(st.db.dir().join("u.json"), b"{ broken").unwrap();
        assert_eq!(call(&r, Method::Post, "/books/u/rescan", "").0, 500);
    }
}
