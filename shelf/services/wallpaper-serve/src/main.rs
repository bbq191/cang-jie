//! wallpaper-serve —— 书架·壁纸（loopback 8793）+ 子命令。
//! `serve`：路由（经网关前缀 `/api/wallpapers`）`GET /`（池）· `POST /`（multipart 多图，缩放入池，`?activate=1` 顺手激活）·
//!   `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}`（PNG 预览）· `GET /status`。
//! `bind` / `unbind` / `roll`：给 boot 单元 / 钩子 / 手动用。**轮换由 serve 内的 journal 唤醒监听触发**（见 wake.rs：
//! 充电时按电源键不真 suspend，systemd-sleep 钩子不跑；xochitl 显示状态日志才是可靠信号）。
//! `activate <name>`：命令行激活。
mod mount;
mod store;
mod wake;

use shelf_core::asset::{self, AssetStore, AssetUploadFlow};
use shelf_core::http::{bind, ApiError, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;
use store::{Mode, WallpaperStore};

const SPEC: ServiceSpec = ServiceSpec {
    name: "wallpaper-serve",
    label: "壁纸",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8793",
    tab: Some(("壁纸", 40)),
};

struct State {
    store: WallpaperStore,
    paths: Paths,
}

impl State {
    fn status(&self) -> serde_json::Value {
        let st = self.store.state();
        serde_json::json!({
            "ok": true, "mode": st.mode, "current": st.current, "pool": self.store.names().len(),
            "mounted": mount::mounted_count(), "expectedMounts": 4,
            "screen": {"width": store::W, "height": store::H},
        })
    }
    /// 激活一张并确保 bind-mount 就位；返回挂载数（bind 失败不阻断，状态里能看到）。
    fn activate(&self, name: &str) -> Result<Option<usize>, String> {
        self.store.activate(name)?;
        Ok(mount::bind(self.store.current_path(), self.store.blank_path()).ok())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let store = WallpaperStore::new(&paths);
    if let Err(e) = store.ensure() {
        eprintln!("[wallpaper-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    match args.first().map(|s| s.as_str()) {
        Some("bind") => exit_with(mount::bind(store.current_path(), store.blank_path()).map(|n| format!("bind 就绪 {n}/4"))),
        Some("unbind") => exit_with(mount::unbind().map(|_| "已还原原生休眠屏".to_string())),
        Some("roll") => exit_with(store.roll().map(|n| n.map(|n| format!("轮换到 {n}")).unwrap_or_else(|| "不轮换（fixed 或空池）".into()))),
        Some("activate") => exit_with(args.get(1).ok_or("用法: activate <name>".to_string()).and_then(|n| store.activate(n).map(|_| format!("已激活 {n}")))),
        Some("serve") | None => {}
        Some(x) => {
            eprintln!("未知子命令 {x}（serve|bind|unbind|roll|activate）");
            std::process::exit(2);
        }
    }
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let st = Arc::new(State { store, paths: paths.clone() });
    let router = Router::new()
        .get("/", bind(&st, |s, _| {
            let w = s.store.state();
            Ok(Reply::ok(&serde_json::json!({"items": s.store.list(), "mode": w.mode, "current": w.current})))
        }))
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))))
        .post("/", bind(&st, |s, r| {
            let b = r.multipart_boundary()?;
            // `?activate=1` 显式激活首张成功项；池里还没有当前图时也自动激活（上传即可用）。
            let want = r.q_flag("activate") || s.store.state().current.is_none();
            let items = AssetUploadFlow::new(&s.paths).run(&s.store, &mut *r.body, &b).map_err(ApiError::bad)?;
            let mut activated = None;
            if want {
                if let Some(first) = items.iter().find(|i| i.ok).and_then(|i| i.item.as_ref()) {
                    s.activate(&first.name).map_err(ApiError::internal)?;
                    activated = Some(first.name.clone());
                }
            }
            Ok(Reply::ok(&asset::receipt(&items, serde_json::json!({"activated": activated, "note": "下次休眠即显示"}))))
        }))
        .put("/current", bind(&st, |s, r| {
            let j = r.json()?;
            let name = j.str("name")?;
            let mounted = s.activate(name).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "current": name, "mounted": mounted})))
        }))
        .put("/mode", bind(&st, |s, r| {
            let j = r.json()?;
            let mode: Mode = serde_json::from_value(j.0.get("mode").cloned().unwrap_or_default()).map_err(|_| ApiError::bad("mode ∈ sequential|random|fixed"))?;
            s.store.set_mode(mode).map_err(ApiError::internal)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "mode": mode})))
        }))
        .delete("/{name}", bind(&st, |s, r| {
            s.store.remove(r.param("name")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        }))
        .get("/{name}", bind(&st, |s, r| Ok(Reply::bytes("image/png", s.store.read(r.param("name")).map_err(ApiError::not_found)?))));
    wake::spawn(Arc::new(WallpaperStore::new(&paths)));
    println!("[wallpaper-serve] 池 {}，已挂 {}/4；监听 xochitl 唤醒日志轮换", st.store.pool().display(), mount::mounted_count());
    if let Err(e) = service::run(&SPEC, &bind_addr, &paths, router) {
        eprintln!("[wallpaper-serve] {e}");
        std::process::exit(1);
    }
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
