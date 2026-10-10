//! 事件汇聚（Fan-in）：网关维护一个总线，给浏览器/CLI 一条 `GET /api/events` SSE；每个领域服务各起一条
//! loopback 长连接订阅它的 `GET /events`（`rmsvc_core::events::follow`），收到的事件补上 `svc` 与 `tab` 后转发（见 [`tag`]）。服务没起 / 重启 → 等注册表 inotify 唤醒重连（不轮询）。
//! 另跟着注册表目录的变化（rmsvc-core 的 [`registry_wake`]，订阅线程等服务上线用的同一条 inotify 监听，不另起一条）：
//! 服务注册/注销时发 `{"area":"manage"}`，网页管理台与 tab 列表据此刷新。
//! 设计约束（用户 2026-09-06）：不轮询、不监听全盘、日志写入不触发——事件只来自服务代码里的变更点与这两处 inotify。
use rmsvc_core::events::{follow, registry_wake, EventBus};
pub use rmsvc_core::events::Wake;
use rmsvc_core::paths::Paths;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// 网关自己发的事件的 area/kind（网页 `core.js` 的 `EV` 表有对应项，`ui.rs` 的测试核对；全量取值见网关白皮书「事件 area/kind 总表」）。
pub const AREA_BOOKS: &str = "books";
pub const AREA_MANAGE: &str = "manage";
/// 批量队列状态变了（`batch::persist`）。
pub const KIND_BATCH: &str = "batch";
/// 服务注册表变了（服务启停/装卸）。
pub const KIND_SERVICES: &str = "services";

/// 网页顶层 tab 的路由键：每条经网关的事件都带 `"tab"`，网页按它找 tab（`core.js` 的 `EV.tab` 有对应项，`ui.rs` 的测试核对）。
/// 服务事件归哪个 tab 由 `manage::MODULES` 的 `tab` 声明，网关自己的事件在发出处写定。网页的顶层 tab 是
/// 传书 / 笔记 / 其他 / 管理（字体、壁纸是「其他」里的子面板，子面板再按事件的 `svc` 分）。
pub const TAB_BOOKS: &str = "books";
pub const TAB_NOTES: &str = "notes";
pub const TAB_OTHER: &str = "other";
pub const TAB_MANAGE: &str = "manage";

/// 网关自己产生的事件（批量队列进度）要发到同一条总线，而 batch 是进程级单例——[`spawn`] 把总线登记在这里，
/// [`notify_books`] 取用（没登记时是空操作，测试里就是这样）。
static BUS: OnceLock<Arc<EventBus>> = OnceLock::new();

/// 通知网页"传书/母版库"区域刷新：批量队列状态变了。取代前端在批量运行时每 3 秒轮询。
pub fn notify_books(kind: &str) {
    if let Some(b) = BUS.get() {
        publish_own(b, AREA_BOOKS, kind);
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
        let (bus, paths) = (bus.clone(), paths.clone());
        std::thread::spawn(move || {
            follow(&paths, m.service, |json| {
                bus.publish_raw(&tag(json, m));
                if let Some(w) = m.wake {
                    w().bump();
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
                    publish_own(&bus, AREA_MANAGE, KIND_SERVICES);
                }
            }
        });
    }
    let _ = BUS.set(bus.clone());
    bus
}

/// 发一条网关自己的事件，带上它归属的 `tab`（批量队列进度 → 传书，服务注册表变化 → 管理）。
fn publish_own(bus: &EventBus, area: &str, kind: &str) {
    let tab = if area == AREA_MANAGE { TAB_MANAGE } else { TAB_BOOKS };
    bus.publish_with(area, kind, serde_json::json!({"tab": tab}));
}

/// 给服务 `m` 发来的事件补 `"svc":"<seg>"`（URL 段名，与 `/api/<seg>` 一致）和 `"tab":"<m.tab>"`（网页按它路由）。
/// 两个字段都以网关为准，服务自己带了同名字段也覆盖；`area`/`kind` 原样保留（网页按 `kind` 决定刷多少）。
pub fn tag(json: &str, m: &crate::manage::Module) -> String {
    use serde_json::Value;
    match serde_json::from_str::<Value>(json) {
        Ok(mut v) => {
            if let Some(o) = v.as_object_mut() {
                o.insert("svc".into(), Value::String(m.seg.into()));
                o.insert("tab".into(), Value::String(m.tab.into()));
            }
            v.to_string()
        }
        Err(_) => serde_json::json!({"svc": m.seg, "tab": m.tab, "raw": json}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manage::by_seg;

    fn tagged(json: &str, seg: &str) -> serde_json::Value {
        serde_json::from_str(&tag(json, by_seg(seg).unwrap())).unwrap()
    }

    #[test]
    fn tags_svc_and_tab_and_tolerates_bad_json() {
        let v = tagged(r#"{"area":"books","kind":"staging","at":1}"#, "books");
        assert_eq!((v["svc"].as_str(), v["tab"].as_str(), v["kind"].as_str(), v["area"].as_str()), (Some("books"), Some(TAB_BOOKS), Some("staging"), Some("books")));
        let bad = tagged("not json", "fonts");
        assert_eq!((bad["svc"].as_str(), bad["tab"].as_str(), bad["raw"].as_str()), (Some("fonts"), Some(TAB_OTHER), Some("not json")));
        let v = tagged(r#"{"area":"x","kind":"y","svc":"forged","tab":"manage"}"#, "wallpapers");
        assert_eq!((v["svc"].as_str(), v["tab"].as_str()), (Some("wallpapers"), Some(TAB_OTHER)), "以网关为准");
    }

    /// FE-6：事件归哪个 tab 由 MODULES 声明，不靠 `area` 恰好等于某个 URL 段——ink-serve / transcribe-serve 发的是
    /// `area:"notes"`，自己的段却是 `ink`/`transcribe`，照样归笔记页；字体、壁纸归「其他」。
    #[test]
    fn modules_route_events_to_tabs() {
        for (seg, tab) in [("books", TAB_BOOKS), ("ink", TAB_NOTES), ("transcribe", TAB_NOTES), ("mind", TAB_NOTES), ("notes", TAB_NOTES), ("fonts", TAB_OTHER), ("wallpapers", TAB_OTHER)] {
            assert_eq!(by_seg(seg).unwrap().tab, tab, "{seg}");
        }
        let v = tagged(r#"{"area":"notes","kind":"entries"}"#, "ink");
        assert_eq!((v["svc"].as_str(), v["tab"].as_str()), (Some("ink"), Some(TAB_NOTES)));
        let all = [TAB_BOOKS, TAB_NOTES, TAB_OTHER, TAB_MANAGE];
        assert!(crate::manage::MODULES.iter().all(|m| all.contains(&m.tab)), "每个模块的 tab 都得是网页认识的");
    }

    /// 网关自己发的事件（批量队列进度、服务注册表变化）同样带 `tab`。
    #[test]
    fn gateway_own_events_carry_tab() {
        let bus = EventBus::new();
        let mut s = bus.subscribe_with(Duration::from_secs(600));
        for (area, kind, tab) in [(AREA_BOOKS, KIND_BATCH, TAB_BOOKS), (AREA_MANAGE, KIND_SERVICES, TAB_MANAGE)] {
            publish_own(&bus, area, kind);
            let mut buf = [0u8; 512];
            let n = std::io::Read::read(&mut s, &mut buf).unwrap();
            let line = std::str::from_utf8(&buf[..n]).unwrap();
            let v: serde_json::Value = serde_json::from_str(line.trim().trim_start_matches("data: ")).unwrap();
            assert_eq!((v["area"].as_str(), v["kind"].as_str(), v["tab"].as_str()), (Some(area), Some(kind), Some(tab)));
        }
    }
}
