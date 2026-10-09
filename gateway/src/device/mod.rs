//! 设备健康 / OTA 提示 / 遗留清理（2026-09-25）：网关自身固定能力，跟 `manage`、`enhance` 一样不经注册表、不代理。
//!
//! - [`health`]：「管理 → 设备健康」卡片，`GET /api/device/health`（用户打开/点刷新才采集，结果缓存 [`HEALTH_TTL`]；`?fresh=1` 跳过缓存）。
//! - [`ota`]：页头"需要重新安装"横幅，`GET /api/device/ota`（固件哈希启动后后台只算一次；其余判据现查、缓存 [`ota::CHECK_TTL`]）。
//! - [`wifi`]：页头"这个 WiFi 上不了外网"横幅，`GET /api/device/wifi`（读 `packaging/wifi-watch` 连上新网络时探测写下的
//!   状态文件，2026-09-28）。
//! - [`cleanup`]：清理遗留数据，`GET /api/device/cleanup` 列出、`POST /api/device/cleanup/delete {area, names}` 逐个删除；
//!   xochitl 书库里的重复副本只列出，删走 book-serve 的回收站队列（前端直接调 `/api/books/trash/add`）。
//!
//! 全都**不轮询**：没有定时器、没有后台采样，只有网关启动时那一次固件哈希。
pub mod cleanup;
pub mod health;
pub mod ota;

use rmsvc_core::cache::TtlCache;
use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

/// 健康卡片缓存：切 tab、SSE 触发的重复刷新在这段时间内不重复 fork `systemctl`；刷新按钮带 `fresh=1` 必然现采。
const HEALTH_TTL: Duration = Duration::from_secs(15);

fn health_cache() -> &'static TtlCache<serde_json::Value> {
    static C: OnceLock<TtlCache<serde_json::Value>> = OnceLock::new();
    C.get_or_init(|| TtlCache::new(HEALTH_TTL))
}

fn ota_cache() -> &'static TtlCache<serde_json::Value> {
    static C: OnceLock<TtlCache<serde_json::Value>> = OnceLock::new();
    C.get_or_init(|| TtlCache::new(ota::CHECK_TTL))
}

/// 网关启动时调：起固件哈希的后台线程（只算一次）。
pub fn start() {
    ota::spawn_firmware_hash();
}

pub fn health(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    if req.q_flag("fresh") {
        health_cache().invalidate();
    }
    let v = health_cache().get_or(|| health::collect(paths, Path::new("/proc"), health::systemctl_show(&health::units()), health::prev_boot_journal()));
    Ok(Reply::ok(&v))
}

pub fn ota_status(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    if req.q_flag("fresh") {
        ota_cache().invalidate();
    }
    let v = ota_cache().get_or(|| serde_json::to_value(ota::check(paths, &ota::unit_dir())).unwrap_or_default());
    Ok(Reply::ok(&v))
}

/// wifi-watch 写的上网探测结果（`$XDG_STATE_HOME/shelf/wifi-connectivity.json`，设备上即
/// `/home/root/.local/state/shelf/`）：`{"ssid","state":"ok|portal|none","code","at"}`。文件不在（WiFi 关着、还没探过、
/// 或 wifi-watch 是旧版）→ `{"state":"unknown"}`，前端不显示横幅。只读一个小文件，不缓存。
pub fn wifi(paths: &Paths, _req: &mut Request<'_>) -> ApiResult {
    Ok(Reply::ok(&wifi_status(&paths.state_dir().join(WIFI_STATE_FILE))))
}

const WIFI_STATE_FILE: &str = "wifi-connectivity.json";

fn wifi_status(file: &Path) -> serde_json::Value {
    Some(rmsvc_core::config::load_or_default::<serde_json::Value>(file))
        .filter(|v| v.get("state").and_then(|s| s.as_str()).is_some())
        .unwrap_or_else(|| serde_json::json!({"state": "unknown"}))
}

pub fn cleanup_list(paths: &Paths, _req: &mut Request<'_>) -> ApiResult {
    let loaded = crate::enhance::xochitl_loaded(paths);
    Ok(Reply::ok(&serde_json::json!({
        "files": cleanup::list_files(paths),
        "library": cleanup::list_library(&paths.xochitl_dir()),
        // 回收站代理（qmd）是否已载入 xochitl：没载入时进了队列也不会被执行，前端据此提示。
        "trashAgent": loaded.qmds.iter().any(|q| q == "shelf-trash-agent.qmd"),
        "xochitl": loaded.xochitl,
    })))
}

pub fn cleanup_delete(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let j = req.json()?;
    let area = j.str("area")?.to_string();
    let names = j.str_list("names");
    if names.is_empty() {
        return Err(ApiError::bad("names 不能为空（只删逐个列出的文件）"));
    }
    let o = cleanup::delete(paths, &area, &names);
    health_cache().invalidate();
    Ok(Reply::ok(&serde_json::json!({
        "ok": o.failed.is_empty(),
        "deleted": o.deleted,
        "failed": o.failed.iter().map(|(n, e)| serde_json::json!({"name": n, "error": e})).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::Method;
    use std::collections::HashMap;

    fn post(paths: &Paths, body: &[u8]) -> ApiResult {
        let mut b: &[u8] = body;
        let mut r = Request { method: Method::Post, path: "/api/device/cleanup/delete".into(), query: HashMap::new(), params: HashMap::new(), content_type: "application/json".into(), content_length: None, headers: vec![], body: &mut b };
        cleanup_delete(paths, &mut r)
    }

    #[test]
    fn wifi_status_reads_file_or_reports_unknown() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join(WIFI_STATE_FILE);
        assert_eq!(wifi_status(&f)["state"], "unknown", "文件不在");
        std::fs::write(&f, "{坏").unwrap();
        assert_eq!(wifi_status(&f)["state"], "unknown", "写坏了也不报错");
        std::fs::write(&f, r#"{"ssid":"IHG \"Free\"","state":"portal","code":"302","at":1}"#).unwrap();
        let v = wifi_status(&f);
        assert_eq!((v["state"].as_str(), v["ssid"].as_str()), (Some("portal"), Some("IHG \"Free\"")));
    }

    #[test]
    fn delete_endpoint_requires_names_and_reports_per_file() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let done = cleanup::areas(&paths)[0].1.clone();
        assert!(done.starts_with(t.path()));
        std::fs::create_dir_all(&done).unwrap();
        std::fs::write(done.join("a.epub"), b"x").unwrap();
        assert!(post(&paths, br#"{"area":"books-done","names":[]}"#).is_err());
        assert!(post(&paths, br#"{"area":"books-done"}"#).is_err());
        let v: serde_json::Value = serde_json::from_slice(&post(&paths, br#"{"area":"books-done","names":["a.epub","../x"]}"#).unwrap().body).unwrap();
        assert_eq!(v["deleted"], serde_json::json!(["a.epub"]));
        assert_eq!(v["failed"][0]["name"], "../x");
        assert_eq!(v["ok"], false);
        assert!(!done.join("a.epub").exists());
    }
}
