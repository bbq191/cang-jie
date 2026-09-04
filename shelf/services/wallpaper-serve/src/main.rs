//! wallpaper-serve —— 书架·壁纸（loopback 8793）+ 子命令。
//! `serve`：路由（经网关前缀 `/api/wallpapers`）`GET /`（池）· `POST /`（multipart 多图，缩放入池，`?activate=1` 顺手激活）·
//!   `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}`（PNG 预览）· `GET /status`。
//! `bind` / `unbind` / `roll`：给 boot 单元 / 钩子 / 手动用。**轮换由 serve 内的 journal 唤醒监听触发**（见 wake.rs：
//! 充电时按电源键不真 suspend，systemd-sleep 钩子不跑；xochitl 显示状态日志才是可靠信号）。
//! `activate <name>`：命令行激活。
mod mount;
mod store;
mod wake;

use shelf_core::asset::{AssetStore, AssetUploadFlow};
use shelf_core::http::{ApiError, Reply, Router};
use shelf_core::multipart::boundary_of;
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

fn status(s: &WallpaperStore) -> serde_json::Value {
    let st = s.state();
    serde_json::json!({
        "ok": true, "mode": st.mode, "current": st.current, "pool": s.names().len(),
        "mounted": mount::mounted_count(), "expectedMounts": 4,
        "screen": {"width": store::W, "height": store::H},
    })
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
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let (s1, s2, s3, s4, s5, s6, s7) = (store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
    let p1 = paths.clone();
    let router = Router::new()
        .get("/", move |_| Ok(Reply::ok(&serde_json::json!({"items": s1.list(), "mode": s1.state().mode, "current": s1.state().current}))))
        .get("/status", move |_| Ok(Reply::ok(&status(&s2))))
        .post("/", move |r| {
            let Some(b) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
            let activate = r.q("activate").map(|v| v == "1" || v == "true").unwrap_or(false);
            let items = AssetUploadFlow::new(&p1).run(&*s3, &mut *r.body, &b).map_err(ApiError::bad)?;
            let mut activated = None;
            if activate {
                if let Some(first) = items.iter().find(|i| i.ok).and_then(|i| i.item.as_ref()) {
                    s3.activate(&first.name).map_err(ApiError::internal)?;
                    let _ = mount::bind(s3.current_path(), s3.blank_path());
                    activated = Some(first.name.clone());
                }
            } else if s3.state().current.is_none() {
                // 首张自动激活：上传即可用
                if let Some(first) = items.iter().find(|i| i.ok).and_then(|i| i.item.as_ref()) {
                    s3.activate(&first.name).map_err(ApiError::internal)?;
                    let _ = mount::bind(s3.current_path(), s3.blank_path());
                    activated = Some(first.name.clone());
                }
            }
            Ok(Reply::ok(&serde_json::json!({"ok": shelf_core::asset::all_ok(&items), "items": items, "activated": activated, "note": "下次休眠即显示"})))
        })
        .put("/current", move |r| {
            let j = r.json_body().map_err(ApiError::bad)?;
            let name = j.get("name").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad("缺 name"))?;
            s4.activate(name).map_err(ApiError::bad)?;
            let mounted = mount::bind(s4.current_path(), s4.blank_path()).ok();
            Ok(Reply::ok(&serde_json::json!({"ok": true, "current": name, "mounted": mounted})))
        })
        .put("/mode", move |r| {
            let j = r.json_body().map_err(ApiError::bad)?;
            let mode: Mode = serde_json::from_value(j.get("mode").cloned().unwrap_or_default()).map_err(|_| ApiError::bad("mode ∈ sequential|random|fixed"))?;
            s5.set_mode(mode).map_err(ApiError::internal)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "mode": mode})))
        })
        .delete("/{name}", move |r| {
            s6.remove(r.param("name")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        })
        .get("/{name}", move |r| {
            let name = r.param("name").to_string();
            if name.contains('/') || name.starts_with('.') {
                return Err(ApiError::bad("非法文件名"));
            }
            let data = std::fs::read(s7.pool().join(&name)).map_err(|_| ApiError::not_found("池里没有这张图"))?;
            Ok(Reply::bytes("image/png", data))
        });
    wake::spawn(store.clone());
    println!("[wallpaper-serve] 池 {}，已挂 {}/4；监听 xochitl 唤醒日志轮换", store.pool().display(), mount::mounted_count());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
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
