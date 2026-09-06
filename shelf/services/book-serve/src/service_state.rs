//! 服务组合根：配置、inbox 队列、母版库、xochitl 客户端；inbox 追平处理。
use crate::config::BookConfig;
use crate::spool::Spool;
use crate::staging::{self, Staging};
use serde::Serialize;
use shelf_core::events::EventBus;
use shelf_core::formats::{self, BOOK_EXTS};
use shelf_core::paths::Paths;
use shelf_core::xochitl::Xochitl;
use std::sync::Arc;

pub struct State {
    pub cfg: BookConfig,
    pub spool: Spool,
    pub staging: Staging,
    pub xochitl: Arc<Xochitl>,
    /// 事件总线：母版库/inbox 每次变更发一条，网关汇聚推给网页（零轮询）。
    pub bus: Arc<EventBus>,
}

/// inbox 追平一项的结果（日志 / `POST /inbox/retry` 回执）。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct InboxOutcome {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

impl State {
    pub fn new(paths: &Paths) -> State {
        let cfg = BookConfig::load(paths);
        let xochitl = Arc::new(Xochitl::new(&cfg.xochitl_host, &paths.xochitl_dir(), cfg.upload_timeout_secs));
        let spool = Spool::new(paths.state_dir().join("books"));
        let staging = Staging::new(paths.staging_dir(), xochitl.clone(), cfg.library_folder.clone(), cfg.native_upload_limit_bytes());
        State { cfg, spool, staging, xochitl, bus: Arc::new(EventBus::new()) }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        self.spool.ensure()?;
        self.staging.ensure()
    }

    pub fn status(&self) -> serde_json::Value {
        let items = self.spool.list();
        serde_json::json!({
            "ok": true,
            "uploadReachable": self.xochitl.reachable(),
            "libraryFolder": self.cfg.library_folder,
            "annotFolder": self.cfg.annot_folder,
            // 投原生的体积门（字节），网页据此灰掉超限书的「投入原生书库」
            "nativeUploadLimitBytes": self.cfg.native_upload_limit_bytes(),
            "spool": {
                "pending": items.iter().filter(|i| i.state == "pending").count(),
                "failed": items.iter().filter(|i| i.state == "failed").count(),
            },
        })
    }

    /// 处理 inbox（scp 丢进来的 / 重试的）：**原样落母版库**（与网页/CLI 同一规则：所有书只落母版库，去向在网页选）。
    /// 非书籍格式进 failed/ 带原因、不反复重试。`only`=只处理该文件。
    pub fn process_inbox(&self, only: Option<&str>) -> Vec<InboxOutcome> {
        let _g = self.spool.guard();
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.spool.inbox()) else { return out };
        for e in rd.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else { continue };
            if !p.is_file() || only.map(|o| o != name).unwrap_or(false) || name.starts_with('.') {
                continue; // 半成品不动
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
            self.bus.publish("books", "inbox");
            if out.iter().any(|o| o.ok) {
                self.bus.publish("books", "staging");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_lands_books_and_fails_non_books() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None });
        let st = State::new(&paths);
        st.ensure_dirs().unwrap();
        std::fs::write(st.spool.inbox().join("b.mobi"), b"x").unwrap();
        std::fs::write(st.spool.inbox().join("p.jpg"), b"x").unwrap();
        let out = st.process_inbox(None);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|o| o.ok && o.name == "b.mobi"));
        assert!(out.iter().any(|o| !o.ok && o.name == "p.jpg" && o.message.contains("不是书籍格式")));
        assert!(st.staging.dir().join("b.mobi").is_file() && !st.spool.inbox().join("b.mobi").exists());
        assert_eq!(st.spool.list().iter().filter(|e| e.state == "failed").count(), 1);
    }
}
