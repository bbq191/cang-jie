//! 服务组合根：KOReader 安装目录模型、配置同步、事件总线，加状态汇总与上传公共流程（HTTP 路由在 `api.rs`）。
use crate::config::ConfigSync;
use crate::koreader::{self, KoReader, KoStore};
use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::formats::FONT_EXTS;
use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;
use std::sync::Arc;

pub struct State {
    pub ko: Arc<KoReader>,
    pub paths: Paths,
    pub sync: ConfigSync,
    pub bus: Arc<rmsvc_core::events::EventBus>,
}

impl State {
    pub fn new(paths: &Paths) -> State {
        let ko = Arc::new(KoReader::new(paths.koreader_root()));
        State {
            sync: ConfigSync { ko: ko.clone(), backup_dir: paths.state_dir().join("koreader-backups"), tmp_dir: paths.runtime_dir().join("koreader") },
            ko,
            paths: paths.clone(),
            bus: Arc::new(rmsvc_core::events::EventBus::new()),
        }
    }

    pub fn require_installed(&self) -> Result<(), ApiError> {
        if self.ko.installed() {
            Ok(())
        } else {
            Err(ApiError { status: 409, message: "KOReader 未安装（appload 目录不存在）".into() })
        }
    }

    pub fn status(&self) -> serde_json::Value {
        let k = &self.ko;
        serde_json::json!({
            "ok": true,
            "installed": k.installed(),
            "running": k.running(),
            "version": k.version(),
            "root": k.root(),
            "booksDir": k.books_dir(),
            "books": k.list_books("").map(|v| v.iter().filter(|e| e.kind == "file").count()).unwrap_or(0),
            "fonts": koreader::list_files(&k.fonts_dir(), FONT_EXTS).len(),
            "dicts": k.list_dicts().len(),
        })
    }

    /// multipart 多文件 → `store`（fonts/dicts 共用同一 [`AssetUploadFlow`]）。KOReader 未装→409。
    pub fn upload(&self, r: &mut Request<'_>, store: &KoStore) -> ApiResult {
        self.require_installed()?;
        let boundary = r.multipart_boundary()?;
        let items = AssetUploadFlow::new(&self.paths).run(store, &mut *r.body, &boundary).map_err(ApiError::bad)?;
        Ok(Reply::ok(&asset::receipt(&items, serde_json::json!({"note": self.ko.running_note("KOReader 正在运行：重启它后才生效")}))))
    }

    pub fn font_store(&self) -> KoStore {
        KoStore::new(self.ko.fonts_dir(), "koreader-font", FONT_EXTS, "fonts/")
    }
}
