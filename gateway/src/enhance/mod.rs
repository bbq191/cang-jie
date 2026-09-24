//! 系统增强工具开关（Track 3，2026-09-09）：网关自身固定能力（跟 `manage` 一样不经过服务注册表/反代），
//! 给原来只能在设备原生「设置」App 里改的开关一个网页入口。现接了五个开关：CJK 荧光笔吸附/CJK 手写
//! 笔迹优化/「导入 md 文档」可见性/漫画页边距最小化（都在 [`qol`]，同一份 `reading-qol.json`）+ 电池刺客 battop
//! （[`battop`]，独立 systemd unit）。
//!
//! **「CJK 手写笔迹优化」这句注释曾经写"目前完全不存在、没有反编译地基"——那是 2026-09-09 刚开线时
//! 的状态，早就过时了**：`enhance/handwriting-stroke/src/hw_stroke.c` 现在是真机验证过的 xovi 扩展
//! （两个 hook 目标、笔尖角度+提按速度两个效果），这里的开关是纯网页层派生态（见 `qol::hw_stroke_enabled`
//! 头注为什么不需要单独的布尔字段），不需要碰设备端 C 代码/重新编译部署 `.so`。
//!
//! 以后再加系统增强能力，往这个目录加一个新文件（比照 `qol.rs`/`battop.rs`）+ 这里挂一个路由，
//! 不需要单独起一个 service（这两个能力都是同机文件 I/O / systemctl 直调，没有独立进程边界的理由）。
mod battop;
mod loaded;
mod qol;

use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;

pub fn status(paths: &Paths) -> Reply {
    let b = battop::status();
    Reply::ok(&serde_json::json!({
        "hlSnapCjk": qol::hl_snap_cjk(paths),
        "hwStrokeEnabled": qol::hw_stroke_enabled(paths),
        "notesImportMdEnabled": qol::notes_import_md_enabled(paths),
        "comicMinMargin": qol::comic_min_margin(paths),
        "battop": {"installed": b.installed, "running": b.running, "lastSampleAt": b.last_sample_at},
        "loaded": loaded::scan(std::path::Path::new("/proc")),
    }))
}

/// `PUT /api/enhance/qol`：接 `{hlSnapCjk}`/`{hwStrokeEnabled}`/`{notesImportMdEnabled}`/`{comicMinMargin}`，body 里出现
/// 哪个就改哪个（`qol::patch` 本身是通用的 key-patch，将来加键直接扩这里）。
pub fn set_qol(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let body = req.json()?;
    let mut changes = serde_json::Map::new();
    // 与 reading-qol.json 键同名的直通布尔开关。
    for key in ["hlSnapCjk", "notesImportMdEnabled", "comicMinMargin"] {
        if let Some(v) = body.0.get(key).and_then(|v| v.as_bool()) {
            changes.insert(key.into(), serde_json::Value::Bool(v));
        }
    }
    if let Some(v) = body.0.get("hwStrokeEnabled").and_then(|v| v.as_bool()) {
        // 两个 min_ratio 永远同步写——网页层只表达"开/关"这一个语义，角度/宽度/速度阈值这几个精调
        // 字段留给手改 reading-qol.json，网页开关不碰（见白皮书 §03f「实验室」小节的设计取舍）。
        let ratio = if v { 0.6 } else { 1.0 };
        changes.insert("hwStrokeNibMinRatio".into(), serde_json::json!(ratio));
        changes.insert("hwStrokeSpeedMinRatio".into(), serde_json::json!(ratio));
    }
    if changes.is_empty() {
        return Err(ApiError::bad("body 需要 hlSnapCjk/hwStrokeEnabled/notesImportMdEnabled/comicMinMargin 其中一个布尔字段"));
    }
    qol::patch(paths, changes).map_err(ApiError::internal)?;
    Ok(status(paths))
}

/// `POST /api/enhance/battop/{start|stop}`。**没有独立顶层「电池刺客」标签页了**（2026-09-10
/// 用户拍板：降级移入「管理→实验室」，只留卡片，不单开顶层 tab）——上一版为了让那个独立标签页
/// "跑起来才出现"而加的 `events::Hub` 事件 publish 已经跟着撤掉，这里恢复成不需要额外状态的
/// 单一函数。
pub fn battop_toggle(_paths: &Paths, action: &str) -> ApiResult {
    battop::toggle(action).map_err(ApiError::bad)?;
    Ok(Reply::ok(&serde_json::json!({"ok": true})))
}

/// `GET /api/enhance/battop/summary`：原样转发 `battop::summary()`（4 个时间窗 × 应用/进程/唤醒源
/// top15，battop 自己聚合好的，这里不重新算）。还没有数据（刚装/从没跑过一次采样）时 `available:false`，
/// 网页显示"还没有数据，等下一次采样"而不是报错——这不是异常状态，是正常的"刚装上"过渡态。
pub fn battop_summary(_paths: &Paths, _req: &mut Request<'_>) -> ApiResult {
    match battop::summary() {
        Some(v) => Ok(Reply::ok(&serde_json::json!({"available": true, "summary": v}))),
        None => Ok(Reply::ok(&serde_json::json!({"available": false}))),
    }
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
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" { Some(h.clone()) } else { None });
        let rep = put(&paths, br#"{"comicMinMargin":true,"hlSnapCjk":false,"junk":1}"#).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&rep.body).unwrap();
        assert_eq!((v["comicMinMargin"].as_bool(), v["hlSnapCjk"].as_bool()), (Some(true), Some(false)));
        assert_eq!(v["notesImportMdEnabled"], false, "没传的键保持缺省");
        // 只传 hwStrokeEnabled：两个 ratio 同步写，上一次的键不被冲掉
        let rep = put(&paths, br#"{"hwStrokeEnabled":true}"#).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&rep.body).unwrap()["hwStrokeEnabled"], true);
        assert!(qol::comic_min_margin(&paths));
        // 没有任何可识别的布尔字段 → 拒绝
        assert!(put(&paths, br#"{"hlSnapCjk":"yes"}"#).is_err());
    }
}
