//! font-serve —— 书架·字体（loopback 8792）。路由（经网关前缀 `/api/fonts`）：
//! `GET /`（按家族归组的清单）· `POST /`（multipart 多文件，装进 fontconfig 用户字体目录；**不碰 KOReader**）·
//! `DELETE /{family}`（删整个家族的全部文件）· `GET /status`。所有字体一视同仁、无"内建"。
//! 字体菜单 qmd 读 `~/.local/share/shelf/fonts.json`（`shelf/xovi/font-menu-dynamic.qmd`）。
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
    match store.write_index() {
        Ok(f) => println!("[font-serve] 索引 {} 个家族", f.len()),
        Err(e) => eprintln!("[font-serve] 写 fonts.json 失败: {e}"),
    }
    let (s1, s2, s3, s4) = (store.clone(), store.clone(), store.clone(), store.clone());
    let p1 = paths.clone();
    let router = Router::new()
        .get("/", move |_| Ok(Reply::ok(&serde_json::json!({"items": s1.list(), "fontsDir": s1.fonts_dir(), "index": s1.json_path()}))))
        .post("/", move |r| {
            let Some(b) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
            let items = AssetUploadFlow::new(&p1).run(&*s2, &mut *r.body, &b).map_err(ApiError::bad)?;
            let ok = !items.is_empty() && items.iter().all(|i| i.ok);
            // 真机 2026-09-03（3.27.3.0）：上传后不重启，菜单出现新项、选中即渲染（渲染 PDF 嵌入 LXGWNeoXiHeiScreenFull）
            // → restartNeeded=false 成立。菜单每进程只建一次，qmd 的 onVisibleChanged 负责差量追加（S-B）。
            Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items, "restartNeeded": false, "note": "已装进原生阅读器（fontconfig）；「文字与布局」菜单重开即可选，无需重启。KOReader 请到 KOReader 页单独上传"})))
        })
        .delete("/{family}", move |r| {
            let removed = s3.remove_family(r.param("family")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed})))
        })
        .get("/status", move |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "count": s4.list().len(), "target": "native"}))));
    println!("[font-serve] 字体目录 {}，清单 {}", store.fonts_dir().display(), store.json_path().display());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[font-serve] {e}");
        std::process::exit(1);
    }
}
