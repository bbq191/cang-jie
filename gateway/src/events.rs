//! 事件汇聚（Fan-in）：网关维护一个总线，给浏览器/CLI 一条 `GET /api/events` SSE；每个领域服务各起一条
//! loopback 长连接订阅它的 `GET /events`（`rmsvc_core::events::follow`），收到的事件补上 `svc` 后转发。服务没起 / 重启 → 等注册表 inotify 唤醒重连（不轮询）。
//! 另监听注册表目录（inotify，tmpfs）：服务注册/注销时发 `{"area":"manage"}`，网页管理台与 tab 列表据此刷新。
//! 设计约束（用户 2026-09-06）：不轮询、不监听全盘、日志写入不触发——事件只来自服务代码里的变更点与这两处 inotify。
use rmsvc_core::events::{follow, EventBus};
use rmsvc_core::paths::Paths;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub struct Hub {
    pub bus: Arc<EventBus>,
}

/// 网关自己产生的事件（批量队列进度、并发闸门排队/处理状态变化）要发到同一条总线，而 batch/budget 是
/// 进程级单例、拿不到 `Hub`——`Hub::spawn` 把总线登记在这里，[`notify_books`] 取用（没登记时是空操作，测试里就是这样）。
static BUS: OnceLock<Arc<EventBus>> = OnceLock::new();

/// 通知网页"传书/母版库"区域刷新：批量队列状态、闸门排队/处理状态变了。取代前端在批量运行时每 3 秒轮询。
pub fn notify_books(kind: &str) {
    if let Some(b) = BUS.get() {
        b.publish("books", kind);
    }
}

impl Hub {
    /// 起所有后台线程：每个**有事件流的**模块一条订阅线程（`rmsvc_core::events::follow`，注册表 inotify 唤醒、
    /// 404/断线退避、loopback 长心跳）+ 注册表目录监听。
    pub fn spawn(paths: Arc<Paths>) -> Hub {
        let bus = Arc::new(EventBus::new());
        for m in crate::manage::MODULES.iter().filter(|m| m.events) {
            let (bus, paths, seg, svc) = (bus.clone(), paths.clone(), m.seg, m.service);
            std::thread::spawn(move || follow(&paths, svc, |json| bus.publish_raw(&tag_svc(json, seg))));
        }
        {
            let (bus, dir) = (bus.clone(), paths.services_dir());
            std::thread::spawn(move || {
                rmsvc_core::fswatch::watch_debounced(&dir, Duration::from_millis(500), |_| bus.publish("manage", "services"));
            });
        }
        let _ = BUS.set(bus.clone());
        Hub { bus }
    }
}

/// 给服务发来的事件补 `"svc":"<seg>"`（URL 段名，与网页 tab / `/api/<seg>` 一致）。
pub fn tag_svc(json: &str, seg: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(mut v) => {
            if let Some(o) = v.as_object_mut() {
                o.insert("svc".into(), serde_json::Value::String(seg.into()));
            }
            v.to_string()
        }
        Err(_) => serde_json::json!({"svc": seg, "raw": json}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags_svc_and_tolerates_bad_json() {
        let t = tag_svc(r#"{"area":"books","kind":"staging","at":1}"#, "books");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["svc"], "books");
        assert_eq!(v["kind"], "staging");
        let bad = tag_svc("not json", "fonts");
        assert!(bad.contains(r#""svc":"fonts""#) && bad.contains("raw"));
    }
}
