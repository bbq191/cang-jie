//! 服务共享状态：配置、spool、目标注册表、xochitl 客户端；inbox 追平处理。
use crate::config::BookConfig;
use crate::spool::Spool;
use crate::target::{Outcome, TargetRegistry};
use shelf_core::paths::Paths;
use shelf_core::xochitl::Xochitl;
use std::sync::Arc;

pub struct State {
    pub cfg: BookConfig,
    /// 只读：系统增强面板的开关文件（`$XDG_DATA_HOME/cangjie-ime/reading-qol.json`，书架不写它）。
    pub reading_qol: std::path::PathBuf,
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
        let reading_qol = paths.data_root().join("cangjie-ime/reading-qol.json");
        State { cfg, spool, targets, xochitl, reading_qol }
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
            "readingQol": std::fs::read_to_string(&self.reading_qol).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()),
        })
    }

    /// 处理 inbox（scp 丢进来的 / 重试的）：**原样落母版库**，不再直投 native。
    /// 规则与网页/CLI 一致——所有书只允许落母版库，去向在网页选（2026-09-05 用户定）。`only`=只处理该文件。
    pub fn process_inbox(&self, only: Option<&str>) -> Vec<Outcome> {
        let _g = self.spool.guard();
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.spool.inbox()) else { return out };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else { continue };
            if only.map(|o| o != name).unwrap_or(false) || name.starts_with('.') {
                continue; // 半成品不动；任意格式都收（能投哪个读器在落库时按格式门控）
            }
            let Some(work) = self.spool.claim(&name) else { continue };
            let o = match std::fs::read(&work).map_err(|e| format!("读取失败: {e}")).and_then(|d| self.spool.stage_new(&name, &d)) {
                Ok(landed) => {
                    let _ = std::fs::remove_file(&work);
                    Outcome { file: landed, target: "staging".into(), ok: true, message: "已入母版库".into() }
                }
                Err(e) => {
                    self.spool.archive_failed(&work, &e);
                    Outcome { file: name.clone(), target: "staging".into(), ok: false, message: e }
                }
            };
            println!("[book-serve] inbox {} → {}: {}", name, if o.ok { "ok" } else { "fail" }, o.message);
            out.push(o);
        }
        out
    }
}
