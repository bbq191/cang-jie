//! 系统增强工具开关（Track 3，2026-09-09）：网关自身固定能力（跟 `manage` 一样不经过服务注册表/反代），
//! 给原来只能在设备原生「设置」App 里改的开关一个网页入口。现接的开关：CJK 荧光笔吸附/
//! 「导入 md 文档」可见性/漫画页边距最小化/单击翻页/日漫翻页规则（都在 [`qol`]，同一份 `reading-qol.json`）。
//!
//! 2026-09-30 移除：电池刺客（battop，原 `battop.rs` + `/api/enhance/battop/*`）与手写优化（hw-stroke 扩展的
//! `hwStrokeEnabled` 派生开关）。`reading-qol.json` 里残留的 `hwStroke*` 键不主动清——全量写回、不认识的键原样保留。
//!
//! 以后再加系统增强能力，往这个目录加一个新文件（比照 `qol.rs`）+ 这里挂一个路由，
//! 不需要单独起一个 service（同机文件 I/O，没有独立进程边界的理由）。
mod loaded;
mod qol;

use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;

/// xochitl 扩展加载状态的扫描器（进程级缓存，见 [`loaded::Scanner`]）。
static LOADED: loaded::Scanner = loaded::Scanner::new();

/// xochitl 主进程映射扫描（同一个进程只扫一次 maps，见 [`loaded::Scanner`]）：「设备健康」的 OTA 判定与清理页复用，
/// 不另起一套 `/proc` 遍历。
pub fn xochitl_loaded(paths: &Paths) -> loaded::Loaded {
    LOADED.scan(std::path::Path::new("/proc"), &paths.home().join("xovi/exthome/qt-resource-rebuilder"))
}

pub fn status(paths: &Paths) -> Reply {
    let q = qol::Qol::load(paths);
    Reply::ok(&serde_json::json!({
        "hlSnapCjk": q.hl_snap_cjk(),
        "notesImportMdEnabled": q.notes_import_md_enabled(),
        "comicMinMargin": q.comic_min_margin(),
        "tapPageTurn": q.tap_page_turn(),
        "rtlPageTurn": q.rtl_page_turn(),
        "loaded": xochitl_loaded(paths),
    }))
}

/// `PUT /api/enhance/qol`：接 `{hlSnapCjk}`/`{notesImportMdEnabled}`/`{comicMinMargin}`/`{tapPageTurn}`/`{rtlPageTurn}`，body 里出现
/// 哪个就改哪个（`qol::patch` 本身是通用的 key-patch，将来加键直接扩这里）。
pub fn set_qol(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let body = req.json()?;
    let mut changes = serde_json::Map::new();
    // 与 reading-qol.json 键同名的直通布尔开关。
    for key in ["hlSnapCjk", "notesImportMdEnabled", "comicMinMargin", "tapPageTurn", "rtlPageTurn"] {
        if let Some(v) = body.0.get(key).and_then(|v| v.as_bool()) {
            changes.insert(key.into(), serde_json::Value::Bool(v));
        }
    }
    if changes.is_empty() {
        return Err(ApiError::bad("body 需要 hlSnapCjk/notesImportMdEnabled/comicMinMargin/tapPageTurn/rtlPageTurn 其中一个布尔字段"));
    }
    qol::patch(paths, changes).map_err(ApiError::internal)?;
    Ok(status(paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::Method;
    use std::collections::HashMap;

    fn put(paths: &Paths, body: &[u8]) -> ApiResult {
        let mut b: &[u8] = body;
        let mut r = Request { method: Method::Put, path: "/api/enhance/qol".into(), query: HashMap::new(), params: HashMap::new(), content_type: "application/json".into(), content_length: None, headers: vec![], body: &mut b };
        set_qol(paths, &mut r)
    }

    #[test]
    fn set_qol_applies_only_present_boolean_keys_and_rejects_empty() {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::testutil::sandbox(&t);
        let rep = put(&paths, br#"{"comicMinMargin":true,"hlSnapCjk":false,"junk":1}"#).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&rep.body).unwrap();
        assert_eq!((v["comicMinMargin"].as_bool(), v["hlSnapCjk"].as_bool()), (Some(true), Some(false)));
        assert_eq!(v["notesImportMdEnabled"], false, "没传的键保持缺省");
        // 只传 tapPageTurn：上一次写的键不被冲掉
        put(&paths, br#"{"tapPageTurn":true}"#).unwrap();
        assert!(qol::Qol::load(&paths).comic_min_margin());
        // 已移除的 hwStrokeEnabled（2026-09-30）不再被认：单独传它 → 拒绝，也不写任何键
        assert!(put(&paths, br#"{"hwStrokeEnabled":true}"#).is_err());
        assert!(!std::fs::read_to_string(paths.home().join(".local/share/cangjie-ime/reading-qol.json")).unwrap().contains("hwStroke"));
        // 没有任何可识别的布尔字段 → 拒绝
        assert!(put(&paths, br#"{"hlSnapCjk":"yes"}"#).is_err());
    }

    /// 两个翻页开关：缺省关；各自独立写，互不冲掉，也不冲掉别的键。
    #[test]
    fn page_turn_switches_default_off_and_independent() {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::testutil::sandbox(&t);
        let q = qol::Qol::load(&paths);
        assert!(!q.tap_page_turn() && !q.rtl_page_turn());
        put(&paths, br#"{"comicMinMargin":true}"#).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&put(&paths, br#"{"tapPageTurn":true}"#).unwrap().body).unwrap();
        assert_eq!((v["tapPageTurn"].as_bool(), v["rtlPageTurn"].as_bool()), (Some(true), Some(false)));
        put(&paths, br#"{"rtlPageTurn":true}"#).unwrap();
        let q = qol::Qol::load(&paths);
        assert!(q.tap_page_turn() && q.rtl_page_turn() && q.comic_min_margin());
    }
}
