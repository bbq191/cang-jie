//! 本服务推的 SSE 事件的 area / kind（`bus.publish(area, kind)`，网关汇聚到 `/api/events` 后网页按它们刷新）。
//! 2026-10-10 起收成常量，不再各处写字面量；取值不变——网页 `gateway/ui/core.js` 的 `EV` 表认的就是这些字符串，改名两边都要动
//! （下面的测试核对 `EV` 表里有每一个）。各 kind 的含义见网关白皮书「事件 area/kind 总表」。

/// 本服务所有事件的 area。
pub const AREA: &str = "books";
/// 母版库列表变了（入库 / 删除 / 改名 / 落库开始与结束）。
pub const STAGING: &str = "staging";
/// inbox 追平处理了文件。
pub const INBOX: &str = "inbox";
/// 渲染自检有结果（事件体带 name/status/pages）。
pub const RENDER: &str = "render";
/// 回收站队列变了。
pub const TRASH: &str = "trash";
/// 建文件夹队列变了。
pub const MKDIR: &str = "mkdir";
/// 直接导入做完一本。
pub const IMPORT: &str = "import";
/// 代理放弃记录变了（见 `agent_failures.rs`）。
pub const AGENT_FAILED: &str = "agent-failed";

#[cfg(test)]
mod tests {
    /// 网页按 `EV` 表认事件：这里每个取值都得在表里，不然网页收到了也不认。
    #[test]
    fn web_ev_table_knows_every_kind() {
        let core = include_str!("../../../../gateway/ui/core.js");
        let ev = &core[core.find("const EV={").expect("core.js 应定义 EV 表")..];
        for v in [super::AREA, super::STAGING, super::INBOX, super::RENDER, super::TRASH, super::MKDIR, super::IMPORT, super::AGENT_FAILED] {
            assert!(ev.contains(&format!(":'{v}'")), "core.js 的 EV 表缺 {v}");
        }
    }
}
