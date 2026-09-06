//! ink-serve —— 笔记·矿（loopback 8795）。监听原生书库（事件驱动、防抖），书页 `.rm` 变了就只扫变更页：
//! 勾画（GlyphRange）+ 旁边手写（笔画簇）→ 条目 → 裁图 → 条目库（**唯一写者**，其它服务经这里改字段）。零网络。
//! 路由（经网关前缀 `/api/ink`）：`GET /books` · `GET /books/{uuid}` · `GET /books/{uuid}/crops/{file}` ·
//! `POST /books/{uuid}/entries/{id}`（text/section/style/draft/answer 字段更新，缺省底座无 PATCH）· `PUT /books/{uuid}/sections` ·
//! `POST /books/{uuid}/rescan` · `GET /events`。条目库 `$XDG_STATE_HOME/notes/books/<uuid>.json`，裁图 `$XDG_DATA_HOME/notes/crops/`。
mod bookdb;
mod config;
mod crop;
mod doc;
mod ingest;

use bookdb::BookDb;
use config::IngestConfig;
use notecore::model::{Answer, Draft, Section, Status, Style};
use shelf_core::events::EventBus;
use shelf_core::fs::plain_name;
use shelf_core::http::{bind, ApiError, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;

pub const APP: &str = "notes";

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
        match ingest::ingest_doc(&self.paths.xochitl_dir(), &self.crops_dir(), &self.db, &self.cfg, uuid, shelf_core::clock::now_secs()) {
            Ok(Some(s)) if s.pages > 0 => {
                println!("[ink-serve] {uuid}: 页 {} 新增 {} 变更 {} 不变 {} 撤销 {}", s.pages, s.merge.added, s.merge.changed, s.merge.unchanged, s.merge.revoked);
                self.bus.publish("ink", "entries");
            }
            Ok(_) => {}
            Err(e) => eprintln!("[ink-serve] {uuid}: {e}"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let cfg: IngestConfig = shelf_core::config::load_or_seed(&paths.app_config_dir(APP).join("ink.json"));
    let db = BookDb::new(paths.app_state_dir(APP).join("books"));
    let st = Arc::new(State { paths: paths.clone(), cfg, db, bus: Arc::new(EventBus::new()) });
    if let Err(e) = std::fs::create_dir_all(st.crops_dir()).and_then(|_| st.db.ensure()) {
        eprintln!("[ink-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    // 追平 + 监听：启动扫一遍有手写页的 EPUB；之后书库目录有写入（合上书 xochitl 重写 .content/.metadata）防抖后只扫涉及的文档。
    {
        let st = st.clone();
        std::thread::spawn(move || {
            for u in ingest::candidate_docs(&st.paths.xochitl_dir()) {
                st.ingest(&u);
            }
            let lib = st.paths.xochitl_dir();
            let debounce = std::time::Duration::from_secs(st.cfg.debounce_secs.max(1));
            shelf_core::fswatch::watch_debounced(&lib, debounce, |names| {
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
    let router = Router::new()
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        .get("/books", bind(&st, |s, _| {
            let items: Vec<serde_json::Value> = s.db.list().iter().map(|b| serde_json::json!({"uuid": b.uuid, "title": b.title, "chapters": b.chapters.len(), "entries": b.entries.iter().filter(|e| e.status != Status::Revoked).count(), "pending": b.entries.iter().filter(|e| e.needs_transcribe()).count()})).collect();
            Ok(Reply::ok(&serde_json::json!({"items": items})))
        }))
        .get("/books/{uuid}", bind(&st, |s, r| {
            let b = s.db.load(r.param("uuid")).ok_or_else(|| ApiError::not_found("没有这本书的条目"))?;
            Ok(Reply::ok(&b))
        }))
        .get("/books/{uuid}/crops/{file}", bind(&st, |s, r| {
            let f = plain_name(r.param("file")).map_err(ApiError::bad)?;
            let bytes = std::fs::read(s.crops_dir().join(f)).map_err(|_| ApiError::not_found("没有这张裁图"))?;
            Ok(Reply::bytes("image/png", bytes))
        }))
        .post("/books/{uuid}/entries/{id}", bind(&st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            let j = r.json()?;
            if s.db.load(&uuid).is_none() {
                return Err(ApiError::not_found("没有这本书的条目"));
            }
            let now = shelf_core::clock::now_secs();
            let found = s.db.update(&uuid, || Default::default(), |b| {
                let Some(e) = b.entries.iter_mut().find(|e| e.id == id) else { return false };
                if let Some(t) = j.0.get("text").and_then(|v| v.as_str()) {
                    e.text = (!t.trim().is_empty()).then(|| t.to_string());
                    e.status = if e.text.is_some() { Status::Reviewed } else if e.drafts.is_empty() { Status::Pending } else { Status::Draft };
                }
                if let Some(v) = j.0.get("section") {
                    e.section = v.as_str().filter(|s| !s.is_empty()).map(str::to_string);
                }
                if let Some(v) = j.0.get("style").and_then(|v| serde_json::from_value::<Style>(v.clone()).ok()) {
                    e.style = v;
                }
                if let Some(d) = j.0.get("draft").and_then(|v| serde_json::from_value::<Draft>(v.clone()).ok()) {
                    e.drafts.insert(0, d);
                    if e.text.is_none() {
                        e.status = Status::Draft;
                    }
                }
                if let Some(a) = j.0.get("answer") {
                    e.answer = serde_json::from_value::<Answer>(a.clone()).ok();
                }
                e.updated = now;
                true
            }).map_err(ApiError::internal)?;
            if !found {
                return Err(ApiError::not_found("没有这条目"));
            }
            s.bus.publish("ink", "entries");
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }))
        .put("/books/{uuid}/sections", bind(&st, |s, r| {
            let uuid = r.param("uuid").to_string();
            let secs: Vec<Section> = serde_json::from_value(r.json()?.0.get("sections").cloned().unwrap_or_default()).map_err(|e| ApiError::bad(format!("sections 形状不对: {e}")))?;
            if s.db.load(&uuid).is_none() {
                return Err(ApiError::not_found("没有这本书的条目"));
            }
            s.db.update(&uuid, || Default::default(), |b| b.sections = secs).map_err(ApiError::internal)?;
            s.bus.publish("ink", "sections");
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }))
        .post("/books/{uuid}/rescan", bind(&st, |s, r| {
            let uuid = r.param("uuid").to_string();
            // 强制：清掉页 mtime 记录再摄取
            let _ = s.db.update(&uuid, || Default::default(), |b| b.page_mtimes.clear());
            s.ingest(&uuid);
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }));
    println!("[ink-serve] 条目库 {}；裁图 {}；监听 {}", st.db.dir().display(), st.crops_dir().display(), st.paths.xochitl_dir().display());
    if let Err(e) = service::run(&SPEC, &bind_addr, &paths, router) {
        eprintln!("[ink-serve] {e}");
        std::process::exit(1);
    }
}
