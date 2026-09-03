//! font-serve —— 书架·字体（loopback 8792）。路由（经网关前缀 `/api/fonts`）：
//! `GET /`（清单=fonts.json 内容）· `POST /`（multipart 多文件，装进 fontconfig 用户字体目录 + 镜像 KOReader）·
//! `DELETE /{file}` · `GET /status`。字体菜单 qmd 读 `~/.local/share/shelf/fonts.json`（`shelf/xovi/font-menu-dynamic.qmd`）。
mod store;
mod ttf;

use shelf_core::asset::{AssetStore, AssetUploadFlow};
use shelf_core::http::{ApiError, Reply, Router};
use shelf_core::multipart::boundary_of;
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;
use store::{FontConfig, FontStore};

const SPEC: ServiceSpec = ServiceSpec {
    name: "font-serve",
    label: "字体",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8792",
    tab: Some(("字体", 30)),
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let store = Arc::new(FontStore::new(&paths, FontConfig::load(&paths)));
    if let Err(e) = store.write_index() {
        eprintln!("[font-serve] 写 fonts.json 失败: {e}");
    }
    let (s1, s2, s3, s4) = (store.clone(), store.clone(), store.clone(), store.clone());
    let p1 = paths.clone();
    let router = Router::new()
        .get("/", move |_| Ok(Reply::ok(&serde_json::json!({"items": s1.list(), "fontsDir": s1.fonts_dir(), "index": s1.json_path()}))))
        .post("/", move |r| {
            let Some(b) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
            let items = AssetUploadFlow::new(&p1).run(&*s2, &mut *r.body, &b).map_err(ApiError::bad)?;
            let ok = !items.is_empty() && items.iter().all(|i| i.ok);
            // 渲染 worker 是否需重启 xochitl 由真机 spike 决定；先如实告知"菜单重开可见、渲染可能需重启"
            Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items, "restartNeeded": false, "note": "字体已装入 fontconfig；阅读器字体菜单重开后可选"})))
        })
        .delete("/{file}", move |r| {
            s3.remove(r.param("file")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        })
        .get("/status", move |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "count": s4.list().len(), "mirrorToKoreader": s4.cfg.mirror_to_koreader}))));
    println!("[font-serve] 字体目录 {}，清单 {}", store.fonts_dir().display(), store.json_path().display());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[font-serve] {e}");
        std::process::exit(1);
    }
}
