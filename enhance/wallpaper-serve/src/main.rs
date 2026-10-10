//! wallpaper-serve —— 书架·壁纸（loopback 8793）+ 子命令。
//! `serve`：路由（经网关前缀 `/api/wallpapers`）`GET /`（池）· `POST /`（multipart 多图，缩放入池，`?activate=1` 顺手激活）·
//!   `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}`（PNG 预览）· `GET /status`。
//! `enable` / `disable`：写 / 删 xochitl.conf 的 `SleepScreenPath`（安装器 / 卸载器 / 手动用）；`roll`：手动轮换；
//! `activate <name>`：命令行激活。**轮换由 serve 内的 inotify 监听触发**：xochitl 休眠时读完 current.png（IN_CLOSE_NOWRITE）即换下一张（wake.rs）。
//! 休眠屏机制（2026-09-06 起）：原生隐藏键 `SleepScreenPath=current.png`（native.rs），xochitl 每次休眠重读该文件——
//! 不再 bind-mount `/usr/share/remarkable/suspended.png`、不再盖插画卡、不写 `/usr`、没有开机单元和 sleep 钩子。
mod native;
mod store;
mod wake;

use native::Native;
use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::http::{bind, ApiError, Reply, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::Arc;
use store::{Mode, WallpaperStore, WpError};

const SPEC: ServiceSpec = ServiceSpec {
    name: "wallpaper-serve",
    label: "壁纸",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8793",
    tab: Some(("壁纸", 40)),
};

struct State {
    store: Arc<WallpaperStore>,
    native: Native,
    paths: Paths,
    bus: Arc<rmsvc_core::events::EventBus>,
}

impl State {
    fn status(&self) -> serde_json::Value {
        let st = self.store.state();
        serde_json::json!({
            "ok": true, "mode": st.mode, "current": st.current, "pool": self.store.names().len(),
            "native": self.native.status(),
            "screen": {"width": store::W, "height": store::H},
        })
    }
    /// 激活一张并确保原生键就位（首次写键 → 需整机重启一次才生效（别单独 restart xochitl，它退出时有概率崩溃，见 packaging/devlib.sh 头注 H3），状态里 `restartPending` 能看到）。
    fn activate(&self, name: &str) -> Result<bool, WpError> {
        self.store.activate(name)?;
        self.native.enable().map_err(WpError::Io)
    }
}

impl From<WpError> for ApiError {
    fn from(e: WpError) -> ApiError {
        match e {
            WpError::Bad(m) => ApiError::bad(m),
            WpError::NotFound(m) => ApiError::not_found(m),
            WpError::Busy(m) => ApiError::conflict(m),
            WpError::Io(m) => ApiError::internal(m),
        }
    }
}

fn enable_message(changed: bool) -> String {
    if changed {
        "已写入 xochitl.conf SleepScreenPath（首次生效需整机重启一次）".into()
    } else {
        "SleepScreenPath 已就位".into()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let store = Arc::new(WallpaperStore::new(&paths));
    if let Err(e) = store.ensure() {
        eprintln!("[wallpaper-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    let native = Native::new(&paths, store.current_path());
    match args.first().map(|s| s.as_str()) {
        Some("enable") => {
            if !store.current_path().is_file() {
                exit_with(Err("还没有激活的壁纸（先上传并激活一张）".into()));
            }
            exit_with(native.enable().map(enable_message))
        }
        Some("disable") => exit_with(native.disable().map(|c| if c { "已删 SleepScreenPath，xochitl 重启后回原生休眠屏".to_string() } else { "本就没有 SleepScreenPath".to_string() })),
        Some("roll") => exit_with(store.roll().map(|n| n.map(|n| format!("轮换到 {n}")).unwrap_or_else(|| "不轮换（fixed、空池，或池里只有当前这一张）".into()))),
        Some("activate") => exit_with(args.get(1).ok_or("用法: activate <name>".to_string()).and_then(|n| store.activate(n).map_err(|e| e.to_string()).and_then(|_| native.enable()).map(|c| format!("已激活 {n}；{}", enable_message(c))))),
        Some("serve") | None => {}
        Some(x) => {
            eprintln!("未知子命令 {x}（serve|enable|disable|roll|activate）");
            std::process::exit(2);
        }
    }
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let n = AssetUploadFlow::new(&paths).clean_stale();
    if n > 0 {
        println!("[wallpaper-serve] 清掉 {n} 个上次未完成的上传暂存");
    }
    let bus = Arc::new(rmsvc_core::events::EventBus::new());
    let st = Arc::new(State { store, native, paths: paths.clone(), bus: bus.clone() });
    let router = router(&st);
    wake::spawn(st.store.clone(), bus); // 与 API 共用同一个 store 实例（同一把状态锁）
    println!("[wallpaper-serve] 池 {}，原生休眠屏键 {}；xochitl 休眠读完休眠屏即轮换", st.store.pool().display(), if st.native.enabled() { "已就位" } else { "未写（激活首张时自动写）" });
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default())
}

/// 网页 API 路由（`main` 与测试共用）。
fn router(st: &Arc<State>) -> Router {
    Router::new()
        .get("/", bind(st, |s, _| {
            let w = s.store.state();
            Ok(Reply::ok(&serde_json::json!({"items": s.store.list(), "mode": w.mode, "current": w.current})))
        }))
        .get("/status", bind(st, |s, _| Ok(Reply::ok(&s.status()))))
        .get("/events", bind(st, |s, r| Ok(s.bus.sse_reply_for(r))))
        .post("/", bind(st, |s, r| {
            let b = r.multipart_boundary()?;
            // `?activate=1` 显式激活首张成功项；池里还没有当前图时也自动激活（上传即可用）。
            let want = r.q_flag("activate") || s.store.state().current.is_none();
            // run 的整体错误：multipart 解析失败 → 400，暂存建不了 → 500（`asset::FlowError`）；单张图装不上记在回执逐项里。
            let items = AssetUploadFlow::new(&s.paths).run(&*s.store, &mut *r.body, &b)?;
            let mut activated = None;
            let mut changed = false;
            if want {
                if let Some(first) = items.iter().find(|i| i.ok).and_then(|i| i.item.as_ref()) {
                    changed = s.activate(&first.name)?;
                    activated = Some(first.name.clone());
                }
            }
            let note = if changed || s.native.restart_pending() { "首次启用：整机重启一次后，下次休眠即显示" } else { "下次休眠即显示" };
            if asset::any_ok(&items) {
                s.bus.publish("wallpapers", "pool");
            }
            Ok(Reply::ok(&asset::receipt(&items, serde_json::json!({"activated": activated, "note": note, "restartPending": s.native.restart_pending()}))))
        }))
        .put("/current", bind(st, |s, r| {
            let j = r.json()?;
            let name = j.str("name")?;
            s.activate(name)?; // 名字不合法 400、池里没有 404、写盘 / 写 xochitl.conf 失败 500
            s.bus.publish("wallpapers", "pool");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "current": name, "native": s.native.status()})))
        }))
        .put("/mode", bind(st, |s, r| {
            let j = r.json()?;
            let mode: Mode = serde_json::from_value(j.0.get("mode").cloned().unwrap_or_default()).map_err(|_| ApiError::bad("mode ∈ sequential|random|fixed"))?;
            s.store.set_mode(mode).map_err(ApiError::internal)?;
            s.bus.publish("wallpapers", "pool");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "mode": mode})))
        }))
        .delete("/{name}", bind(st, |s, r| {
            s.store.remove(r.param("name"))?;
            s.bus.publish("wallpapers", "pool");
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }))
        .get("/{name}", bind(st, |s, r| {
            let name = r.param("name");
            Ok(Reply::bytes(rmsvc_core::formats::mime_of(name), s.store.read(name)?))
        }))
}

fn exit_with(r: Result<String, String>) -> ! {
    match r {
        Ok(m) => {
            println!("[wallpaper-serve] {m}");
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("[wallpaper-serve] {e}");
            std::process::exit(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::{Method, TestRequest};

    fn png(v: u8) -> Vec<u8> {
        let img = image::GrayImage::from_pixel(8, 8, image::Luma([v]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// 沙箱里的服务状态（池里放好 a.png、b.png），路径全在临时目录下。
    fn setup() -> (tempfile::TempDir, Arc<State>, Router) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        paths.ensure().unwrap();
        let store = Arc::new(WallpaperStore::new(&paths));
        store.ensure().unwrap();
        for (n, v) in [("a.png", 10), ("b.png", 200)] {
            std::fs::write(store.pool().join(n), png(v)).unwrap();
        }
        let native = Native::new(&paths, store.current_path());
        let st = Arc::new(State { store, native, paths, bus: Arc::new(rmsvc_core::events::EventBus::new()) });
        let r = router(&st);
        (t, st, r)
    }

    fn put_current(r: &Router, name: &str) -> u16 {
        TestRequest::new(Method::Put, "/current").json(&serde_json::json!({"name": name})).dispatch(r).status
    }

    /// 激活：名字不合法 400、池里没有 404、写 current.png 失败（设备侧故障）500——此前三种都报 400。
    #[test]
    fn activate_splits_bad_request_from_io_failure() {
        let (_t, st, r) = setup();
        assert_eq!(put_current(&r, "../a.png"), 400);
        assert_eq!(put_current(&r, ".hidden.png"), 400);
        assert_eq!(put_current(&r, "nope.png"), 404);
        assert_eq!(put_current(&r, "a.png"), 200);
        assert_eq!(st.store.state().current.as_deref(), Some("a.png"));
        // current.png 被换成目录：打开写失败 → 500，状态不变
        std::fs::remove_file(st.store.current_path()).unwrap();
        std::fs::create_dir(st.store.current_path()).unwrap();
        let reply = TestRequest::new(Method::Put, "/current").json(&serde_json::json!({"name": "b.png"})).dispatch(&r);
        assert_eq!(reply.status, 500, "{}", String::from_utf8_lossy(reply.body.as_bytes()));
        assert_eq!(st.store.state().current.as_deref(), Some("a.png"));
    }

    #[test]
    fn delete_and_preview_status_codes() {
        let (_t, st, r) = setup();
        st.store.activate("a.png").unwrap();
        let del = |name: &str| TestRequest::new(Method::Delete, &format!("/{name}")).dispatch(&r).status;
        assert_eq!(del("a.png"), 409, "正在用的不让删");
        assert_eq!(del(".x.png"), 400);
        assert_eq!(del("nope.png"), 404);
        assert_eq!(del("b.png"), 200);
        assert!(!st.store.pool().join("b.png").exists());
        let get = |name: &str| TestRequest::new(Method::Get, &format!("/{name}")).dispatch(&r);
        assert_eq!(get("a.png").status, 200);
        assert_eq!(get("b.png").status, 404);
        assert_eq!(get(".x.png").status, 400);
    }

    /// `/events` 走 sse_reply_for（心跳读本请求的 `?ka=`，不靠线程局部）：回的是事件流。
    #[test]
    fn events_route_streams() {
        let (_t, _st, r) = setup();
        let reply = TestRequest::new(Method::Get, "/events").query("ka", "30").dispatch(&r);
        assert_eq!(reply.status, 200);
        assert!(reply.content_type.starts_with("text/event-stream") && matches!(reply.body, rmsvc_core::http::Body::EventStream(_)));
    }
}
