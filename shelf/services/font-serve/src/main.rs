//! font-serve —— 书架·字体（loopback 8792）。路由（经网关前缀 `/api/fonts`）：
//! `GET /`（按家族归组的清单）· `POST /`（multipart 多文件，装进 fontconfig 用户字体目录；**不碰 KOReader**）·
//! `DELETE /{family}`（删整个家族的全部文件）· `GET /status`。所有字体一视同仁、无"内建"。
//! 字体菜单 qmd 读 `~/.local/share/shelf/fonts.json`（`shelf/xovi/font-menu-dynamic.qmd`）。
mod store;

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
    let (s1, s2, s2b, s3, s4) = (store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
    let p1 = paths.clone();
    let router = Router::new()
        .get("/", move |_| Ok(Reply::ok(&serde_json::json!({"items": s1.list(), "fontsDir": s1.fonts_dir(), "index": s1.json_path()}))))
        .post("/", move |r| {
            let Some(b) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
            let items = AssetUploadFlow::new(&p1).run(&*s2, &mut *r.body, &b).map_err(ApiError::bad)?;
            let ok = !items.is_empty() && items.iter().all(|i| i.ok);
            // 真机 2026-09-03（3.27.3.0）：上传后不重启，菜单出现新项、选中即渲染。菜单 onVisibleChanged 差量刷新（S-B）。
            // fontconfig 回退由 write_index 随每次上传重写（weak 绑定：选的字体优先、缺字才回退）。
            let fallback = s2b.cjk_fallback_keys();
            let warns: Vec<String> = items.iter().filter_map(|i| i.item.as_ref().and_then(|it| it.extra.get("warn")).and_then(|w| w.as_str()).filter(|w| !w.is_empty()).map(|w| w.to_string())).collect();
            let mut note = String::from("已装进原生阅读器（fontconfig）；「文字与布局」菜单重开即可选，无需重启。KOReader 请到 KOReader 页单独上传");
            if !fallback.is_empty() {
                note.push_str(&format!("。中文缺字回退链：{}", fallback.join(" → ")));
            }
            if !warns.is_empty() {
                note.push_str(&format!("。⚠ {}", warns.join("；")));
            }
            Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items, "restartNeeded": false, "fallback": fallback, "note": note})))
        })
        .delete("/{family}", move |r| {
            let removed = s3.remove_family(r.param("family")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed})))
        })
        .put("/config", {
            let s = store.clone();
            move |r| {
                let v = r.json_body().map_err(ApiError::bad)?;
                let on = v.get("emboldenCjkFallback").and_then(|x| x.as_bool()).ok_or_else(|| ApiError::bad("需要 {emboldenCjkFallback: bool}"))?;
                s.set_embolden(on).map_err(ApiError::internal)?;
                Ok(Reply::ok(&serde_json::json!({"ok": true, "emboldenCjkFallback": on, "note": "已更新，翻书即见（fontconfig 实时生效，无需重启）"})))
            }
        })
        .get("/status", move |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "count": s4.list().len(), "target": "native", "cjkFallback": s4.cjk_fallback_keys(), "emboldenCjkFallback": s4.embolden()}))));
    println!("[font-serve] 字体目录 {}，清单 {}", store.fonts_dir().display(), store.json_path().display());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[font-serve] {e}");
        std::process::exit(1);
    }
}
