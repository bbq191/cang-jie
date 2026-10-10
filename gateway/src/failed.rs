//! 网关自有接口里共用的"失败项"应答片段（2026-10-10，GW-1/FE-4）。各接口自己的应答结构就近定义在各自模块里
//! （`batch::Status`、`device::CleanupList`…），这里只放被不止一个接口用到的。
//! 原名 `wire.rs`，与 `rmsvc_core::wire`（跨服务共用的状态枚举 `DeliverStatus`/`RenderStatus`）同名易混，同日改成按内容
//! 命名：跨服务共享的线上值一律用 `rmsvc_core::wire`，只有网关自己的应答片段留在这里。
use serde::Serialize;

/// "这一项没做成"：批量队列（`GET /api/batch/status` 的 `failed`）与清理遗留文件（`POST /api/device/cleanup/delete`
/// 的 `failed`）同一个形状 `{name, message}`，网页用同一套写法列失败原因。清理接口 2026-10-10 前是 `{name, error}`。
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Failed {
    pub name: String,
    pub message: String,
}

impl Failed {
    pub fn new(name: &str, message: &str) -> Failed {
        Failed { name: name.to_string(), message: message.to_string() }
    }
}
