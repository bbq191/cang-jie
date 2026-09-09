//! 系统增强工具开关（Track 3，2026-09-09）：网关自身固定能力（跟 `manage` 一样不经过服务注册表/反代），
//! 给原来只能在设备原生「设置」App 里改的开关一个网页入口。当前只接了两个真正有地基的开关：
//! CJK 荧光笔吸附（[`qol`]）+ 电池刺客 battop（[`battop`]）。「CJK 手写笔迹优化」（笔锋按 CJK 书写习惯
//! 渲染，用户已澄清与 AI/大模型无关）目前完全不存在、没有反编译地基，前端只给「未上线」占位，这里
//! 没有对应端点——见 `shelf/docs/reMarkable书架白皮书.md` 路线图条目。
//!
//! 以后再加系统增强能力，往这个目录加一个新文件（比照 `qol.rs`/`battop.rs`）+ 这里挂一个路由，
//! 不需要单独起一个 service（这两个能力都是同机文件 I/O / systemctl 直调，没有独立进程边界的理由）。
mod battop;
mod qol;

use shelf_core::http::{ApiError, ApiResult, Reply, Request};
use shelf_core::paths::Paths;

pub fn status(paths: &Paths) -> Reply {
    let b = battop::status();
    Reply::ok(&serde_json::json!({
        "hlSnapCjk": qol::hl_snap_cjk(paths),
        "battop": {"installed": b.installed, "running": b.running, "lastSampleAt": b.last_sample_at},
    }))
}

/// `PUT /api/enhance/qol`：目前只接 `{hlSnapCjk}`（`qol::patch` 本身是通用的 key-patch，将来加键直接扩 body 字段）。
pub fn set_qol(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let body = req.json()?;
    let mut changes = serde_json::Map::new();
    if let Some(v) = body.0.get("hlSnapCjk").and_then(|v| v.as_bool()) {
        changes.insert("hlSnapCjk".into(), serde_json::Value::Bool(v));
    }
    if changes.is_empty() {
        return Err(ApiError::bad("缺 hlSnapCjk"));
    }
    qol::patch(paths, changes).map_err(ApiError::internal)?;
    Ok(status(paths))
}

/// `POST /api/enhance/battop/{start|stop}`。
pub fn battop_toggle(_paths: &Paths, action: &str) -> ApiResult {
    battop::toggle(action).map_err(ApiError::bad)?;
    Ok(Reply::ok(&serde_json::json!({"ok": true})))
}
