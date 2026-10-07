//! 事件汇聚（Fan-in）：网关维护一个总线，给浏览器/CLI 一条 `GET /api/events` SSE；每个领域服务各起一条
//! loopback 长连接订阅它的 `GET /events`（`rmsvc_core::events::follow`），收到的事件补上 `svc` 后转发。服务没起 / 重启 → 等注册表 inotify 唤醒重连（不轮询）。
//! 另跟着注册表目录的变化（rmsvc-core 的 [`registry_wake`]，订阅线程等服务上线用的同一条 inotify 监听，不另起一条）：
//! 服务注册/注销时发 `{"area":"manage"}`，网页管理台与 tab 列表据此刷新。
//! 设计约束（用户 2026-09-06）：不轮询、不监听全盘、日志写入不触发——事件只来自服务代码里的变更点与这两处 inotify。
use rmsvc_core::events::{follow, registry_wake, EventBus};
pub use rmsvc_core::events::Wake;
use rmsvc_core::paths::Paths;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// 网关自己产生的事件（批量队列进度）要发到同一条总线，而 batch 是进程级单例——[`spawn`] 把总线登记在这里，
/// [`notify_books`] 取用（没登记时是空操作，测试里就是这样）。
static BUS: OnceLock<Arc<EventBus>> = OnceLock::new();

/// 通知网页"传书/母版库"区域刷新：批量队列状态变了。取代前端在批量运行时每 3 秒轮询。
pub fn notify_books(kind: &str) {
    if let Some(b) = BUS.get() {
        b.publish("books", kind);
    }
}

/// "book-serve 有新事件"唤醒器（rmsvc-core 的 [`Wake`]）。批量队列要知道"这本书处理完没有"（`batch::poll_until_settled`），
/// 此前每 5 秒 `GET /staging` 一次（整个处理期间——大部头几分钟起）；book-serve 在忙态开始/结束处都发
/// `books` 事件（`staging` 等），网关本来就订阅着它，这里把"收到事件"变成唤醒信号，等待方事件到了才去查一次，
/// 超时只是兜底（事件丢了/订阅重连空窗）。
/// book-serve 事件唤醒器（进程级单例，[`spawn`] 的订阅线程在收到 `books` 段事件时 [`Wake::bump`]）。
pub fn books_wake() -> &'static Wake {
    static W: OnceLock<Wake> = OnceLock::new();
    W.get_or_init(Wake::default)
}

/// 建总线并起所有后台线程：每个**有事件流的**模块一条订阅线程（`rmsvc_core::events::follow`，注册表 inotify 唤醒、
/// 404/断线退避、loopback 长心跳）+ 一条等注册表变化发 `manage` 事件的线程（阻塞在 [`registry_wake`] 上，空闲零唤醒；
/// 此前另起一条 inotify 监听同一个目录，每次注册表变化唤醒两个线程）。返回总线（`GET /api/events` 用）。
pub fn spawn(paths: Arc<Paths>) -> Arc<EventBus> {
    let bus = Arc::new(EventBus::new());
    for m in crate::manage::MODULES.iter().filter(|m| m.events) {
        let (bus, paths, seg, svc) = (bus.clone(), paths.clone(), m.seg, m.service);
        std::thread::spawn(move || {
            follow(&paths, svc, |json| {
                bus.publish_raw(&tag_svc(json, seg));
                if seg == "books" {
                    books_wake().bump();
                }
            })
        });
    }
    {
        let (bus, wake) = (bus.clone(), registry_wake(&paths));
        std::thread::spawn(move || {
            let mut seen = wake.generation();
            loop {
                // 超时只是让循环别永远阻塞在一次调用里，代数没变就不发事件。
                let now = wake.wait_change(seen, Duration::from_secs(24 * 3600));
                if now != seen {
                    seen = now;
                    bus.publish("manage", "services");
                }
            }
        });
    }
    let _ = BUS.set(bus.clone());
    bus
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
