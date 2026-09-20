//! 服务组合根：KOReader 安装目录模型、配置同步、事件总线，加状态汇总与上传公共流程（HTTP 路由在 `api.rs`）。
use crate::config::ConfigSync;
use crate::koreader::{self, KoReader, KoStore};
use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::cache::TtlCache;
use rmsvc_core::formats::FONT_EXTS;
use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;
use std::sync::Arc;
use std::time::Duration;

pub struct State {
    pub ko: Arc<KoReader>,
    pub paths: Paths,
    pub sync: ConfigSync,
    pub bus: Arc<rmsvc_core::events::EventBus>,
    /// `GET /status` 的结果缓存（[`STATUS_TTL`]）。网页每次 refresh 都会打这个接口，而它要遍历 `/proc` 读每个
    /// 进程的 cmdline 判 KOReader 是否在跑，再扫书/字体/词典目录。本服务自己的操作（传书/字体/词典/配置）
    /// 走 [`State::notify`]，先失效再发事件；KOReader 启停这类外部变化最多滞后一个 TTL。
    /// 注意：会**改配置**的安全判断（`ConfigSync::apply` 里的"运行中拒写"）用的是实时 `running()`，不走这个缓存。
    status_cache: TtlCache<serde_json::Value>,
}

/// `/status` 缓存时长：够挡住"连续几次 refresh"，又短到 KOReader 启停几秒内就能在页面上看到。
const STATUS_TTL: Duration = Duration::from_secs(3);

impl State {
    pub fn new(paths: &Paths) -> State {
        let ko = Arc::new(KoReader::new(paths.koreader_root()));
        State {
            sync: ConfigSync { ko: ko.clone(), backup_dir: paths.state_dir().join("koreader-backups"), tmp_dir: paths.runtime_dir().join("koreader") },
            ko,
            paths: paths.clone(),
            bus: Arc::new(rmsvc_core::events::EventBus::new()),
            status_cache: TtlCache::new(STATUS_TTL),
        }
    }

    pub fn require_installed(&self) -> Result<(), ApiError> {
        if self.ko.installed() {
            Ok(())
        } else {
            Err(ApiError { status: 409, message: "KOReader 未安装（appload 目录不存在）".into() })
        }
    }

    /// 本服务的操作改变了状态：先让 `/status` 缓存失效，再发事件（网页收到事件马上来取 `/status`，必须看到新值）。
    pub fn notify(&self, kind: &str) {
        self.status_cache.invalidate();
        self.bus.publish("koreader", kind);
    }

    pub fn status(&self) -> serde_json::Value {
        self.status_cache.get_or(|| self.compute_status())
    }

    fn compute_status(&self) -> serde_json::Value {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state_in(root: &std::path::Path) -> State {
        let h = root.to_str().unwrap().to_string();
        let ko = root.join("ko");
        let paths = Paths::resolve(move |k| match k {
            "HOME" | "XDG_RUNTIME_DIR" => Some(h.clone()),
            "SHELF_KOREADER_ROOT" => Some(ko.to_str().unwrap().to_string()),
            _ => None,
        });
        State::new(&paths)
    }

    #[test]
    fn status_cached_within_ttl_and_notify_invalidates_immediately() {
        let t = tempfile::tempdir().unwrap();
        let st = state_in(t.path());
        std::fs::create_dir_all(st.ko.fonts_dir()).unwrap();
        assert_eq!(st.status()["fonts"], 0);
        std::fs::write(st.ko.fonts_dir().join("a.ttf"), b"x").unwrap();
        assert_eq!(st.status()["fonts"], 0, "TTL 内命中缓存，不重扫目录");
        st.notify("fonts");
        assert_eq!(st.status()["fonts"], 1, "操作完成路径 notify 后，马上刷新就看到变化");
    }
}
