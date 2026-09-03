//! 服务共享状态：配置、spool、目标注册表、xochitl 客户端；inbox 追平处理。
use crate::config::BookConfig;
use crate::spool::Spool;
use crate::target::{DeliverOpts, Outcome, TargetRegistry};
use shelf_core::paths::Paths;
use shelf_core::xochitl::Xochitl;
use std::sync::Arc;

pub struct State {
    pub cfg: BookConfig,
    pub spool: Spool,
    pub targets: TargetRegistry,
    pub xochitl: Arc<Xochitl>,
}

impl State {
    pub fn new(paths: &Paths) -> State {
        let cfg = BookConfig::load(paths);
        let xochitl = Arc::new(Xochitl::new(&cfg.xochitl_host, &paths.xochitl_dir(), cfg.upload_timeout_secs));
        let spool = Spool::new(paths.state_dir().join("books"));
        let targets = TargetRegistry::new(&cfg, xochitl.clone());
        State { cfg, spool, targets, xochitl }
    }

    pub fn status(&self) -> serde_json::Value {
        let items = self.spool.list();
        serde_json::json!({
            "ok": true,
            "uploadReachable": self.xochitl.reachable(),
            "libraryFolder": self.cfg.library_folder,
            "annotFolder": self.cfg.annot_folder,
            "spool": {
                "pending": items.iter().filter(|i| i.state == "pending").count(),
                "failed": items.iter().filter(|i| i.state == "failed").count(),
            },
            "targets": self.targets.ids().iter().map(|(i, l)| serde_json::json!({"id": i, "label": l})).collect::<Vec<_>>(),
        })
    }

    /// 处理 inbox（scp 丢进来的 / 重试的），缺省目标 native。`only`=只处理该文件。
    pub fn process_inbox(&self, only: Option<&str>) -> Vec<Outcome> {
        let _g = self.spool.guard();
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.spool.inbox()) else { return out };
        let target = self.targets.get("native").expect("native target");
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else { continue };
            if only.map(|o| o != name).unwrap_or(false) {
                continue;
            }
            if name.starts_with('.') || target.accepts(&name).is_err() {
                continue; // 半成品/非书籍文件不动
            }
            let Some(work) = self.spool.claim(&name) else { continue };
            let data = match std::fs::read(&work) {
                Ok(d) => d,
                Err(e) => {
                    self.spool.archive_failed(&work);
                    out.push(Outcome { file: name, target: "native".into(), ok: false, message: format!("读取失败: {e}") });
                    continue;
                }
            };
            let o = target.deliver(&name, data, &DeliverOpts { folder: None, optimize: true });
            if o.ok {
                self.spool.archive_done(&work);
            } else {
                self.spool.archive_failed(&work);
            }
            println!("[book-serve] inbox {} → {}: {}", name, if o.ok { "ok" } else { "fail" }, o.message);
            out.push(o);
        }
        out
    }
}
