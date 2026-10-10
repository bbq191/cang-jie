//! font-serve —— 书架·字体（loopback 8792）。路由（经网关前缀 `/api/fonts`）：
//! `GET /`（按家族归组的清单）· `POST /`（multipart 多文件，装进 fontconfig 用户字体目录）·
//! `DELETE /{family}`（删整个家族的全部文件）· `GET /status`。所有字体一视同仁、无"内建"。
//! 字体菜单 qmd 读 `~/.local/share/shelf/fonts.json`（`shelf/xovi/font-menu-dynamic.qmd`）。
//! 开机时 fonts.json 与字体目录一致（文件集合相同、mtime 不晚于索引）就直接复用，不再逐个 fc-scan；
//! fonts.conf 内容没变不重写（2026-09-25）。上传暂存在 `$XDG_STATE_HOME/shelf/upload`，启动时清半成品。
//! 界面字体（2026-10-07）：`GET /ui`（清单 + 当前选择）· `POST /ui`（上传）· `DELETE /ui/{family}` · `PUT /ui/select {sans, serif}`。
//! 界面字体只给 xochitl 界面用，不进阅读器菜单、不当阅读的中文回退（见 `store.rs` 头注与 `ui.rs`）。
mod fontconfig;
mod store;
// TTF/OTF 字体表最小解析（家族名、CJK 覆盖）：只有本服务用，2026-10-10 从 rmsvc-core 搬来（审计 CORE-3）。
mod ttf;
mod ui;

use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::http::{bind, ApiError, Reply, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::Arc;
use store::{FontConfig, FontError, FontStore};

/// 网页 tab 叫「xochitl」：原生阅读器的字体在这里管（传书是网关固定页，不由本服务挂 tab）。
const SPEC: ServiceSpec = ServiceSpec {
    name: "font-serve",
    label: "字体",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8792",
    tab_order: Some(10),
};

struct State {
    store: FontStore,
    ui_store: FontStore,
    ui: ui::UiFont,
    paths: Paths,
    bus: Arc<rmsvc_core::events::EventBus>,
}

/// `GET /ui` 与改选择后的回执：界面字体清单 + 当前选择 + 是否待整机重启。
fn ui_status(s: &State) -> serde_json::Value {
    let sel = s.ui.get();
    serde_json::json!({"ok": true, "items": s.ui_store.list(), "sans": sel.sans, "serif": sel.serif, "restartNeeded": s.ui.restart_needed()})
}

impl From<FontError> for ApiError {
    fn from(e: FontError) -> ApiError {
        match e {
            FontError::Bad(m) => ApiError::bad(m),
            FontError::NotFound(m) => ApiError::not_found(m),
            FontError::Io(m) => ApiError::internal(m),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let n = AssetUploadFlow::new(&paths).clean_stale();
    if n > 0 {
        println!("[font-serve] 清掉 {n} 个上次未完成的上传暂存");
    }
    let store = FontStore::new(&paths, FontConfig::load(&paths));
    let ui_store = FontStore::ui(&paths);
    match ui_store.startup_index() {
        Ok(f) => println!("[font-serve] 界面字体 {} 个家族", f.len()),
        Err(e) => eprintln!("[font-serve] 写 ui-fonts.json 失败: {e}"),
    }
    // 阅读仓库排在界面仓库之后：fonts.conf 的系统中文保底要看界面字体目录有没有字体。
    match store.startup_index() {
        Ok(f) => println!("[font-serve] 索引 {} 个家族", f.len()),
        Err(e) => eprintln!("[font-serve] 写 fonts.json 失败: {e}"),
    }
    let st = Arc::new(State { store, ui_store, ui: ui::UiFont::load(&paths), paths: paths.clone(), bus: Arc::new(rmsvc_core::events::EventBus::new()) });
    let router = router(&st);
    println!("[font-serve] 字体目录 {}，清单 {}", st.store.fonts_dir().display(), st.store.json_path().display());
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default())
}

/// 网页 API 路由（`main` 与测试共用）。
fn router(st: &Arc<State>) -> Router {
    Router::new()
        .get("/", bind(st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.store.list(), "fontsDir": s.store.fonts_dir(), "index": s.store.json_path()})))))
        .post("/", bind(st, |s, r| {
            let b = r.multipart_boundary()?;
            // run 的整体错误：multipart 解析失败 → 400，暂存建不了 → 500（`asset::FlowError`）；单个字体装不上记在回执逐项里。
            let items = AssetUploadFlow::new(&s.paths).run(&s.store, &mut *r.body, &b)?;
            let any_ok = asset::any_ok(&items);
            if any_ok {
                s.store.fc_cache(); // 整批装完重建一次（不在 install 里逐个跑）
            }
            // 真机 2026-09-03（3.27.3.0）：上传后不重启，菜单出现新项、选中即渲染。菜单 onVisibleChanged 差量刷新（S-B）。
            // fontconfig 回退由 write_index 随每次上传重写（weak 绑定：选的字体优先、缺字才回退）。
            let fallback = s.store.cjk_fallback_keys();
            let warns: Vec<String> = items.iter().filter_map(|i| i.item.as_ref().and_then(|it| it.extra.get("warn")).and_then(|w| w.as_str()).filter(|w| !w.is_empty()).map(|w| w.to_string())).collect();
            let mut note = String::from("已装进原生阅读器（fontconfig）；「文字与布局」菜单重开即可选，无需重启");
            if !fallback.is_empty() {
                note.push_str(&format!("。中文缺字回退链：{}", fallback.join(" → ")));
            }
            if !warns.is_empty() {
                note.push_str(&format!("。⚠ {}", warns.join("；")));
            }
            if any_ok {
                s.bus.publish("fonts", "fonts");
            }
            Ok(Reply::ok(&asset::receipt(&items, serde_json::json!({"restartNeeded": false, "fallback": fallback, "note": note}))))
        }))
        .get("/ui", bind(st, |s, _| Ok(Reply::ok(&ui_status(s)))))
        .post("/ui", bind(st, |s, r| {
            let b = r.multipart_boundary()?;
            let items = AssetUploadFlow::new(&s.paths).run(&s.ui_store, &mut *r.body, &b)?;
            if asset::any_ok(&items) {
                s.ui_store.fc_cache();
                s.store.refresh_fontconfig();
                s.bus.publish("fonts", "ui");
            }
            let note = "已装进界面字体（不进阅读器菜单）；在上面选它当界面字体，整机重启后生效";
            Ok(Reply::ok(&asset::receipt(&items, serde_json::json!({"note": note}))))
        }))
        .delete("/ui/{family}", bind(st, |s, r| {
            let family = r.param("family").to_string();
            let removed = s.ui_store.remove_family(&family)?; // 没这个家族 404、删文件 / 重写索引失败 500
            let unselected = s.ui.forget(&family).map_err(ApiError::internal)?;
            s.store.refresh_fontconfig();
            s.bus.publish("fonts", "ui");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed, "unselected": unselected, "restartNeeded": s.ui.restart_needed()})))
        }))
        .put("/ui/select", bind(st, |s, r| {
            let body = r.json()?;
            let field = |k: &str| body.opt_str(k).map(str::to_string).ok_or_else(|| ApiError::bad("需要 {sans: 字符串, serif: 字符串}（空串 = 原生）"));
            let (sans, serif) = (field("sans")?, field("serif")?);
            let installed: Vec<String> = s.ui_store.entries().into_iter().map(|e| e.key).collect();
            s.ui.set(&sans, &serif, &installed)?; // 没装的 400、存选择失败 500
            s.bus.publish("fonts", "ui");
            Ok(Reply::ok(&ui_status(s)))
        }))
        .get("/events", bind(st, |s, r| Ok(s.bus.sse_reply_for(r))))
        .delete("/{family}", bind(st, |s, r| {
            let removed = s.store.remove_family(r.param("family"))?;
            s.bus.publish("fonts", "fonts");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed})))
        }))
        .put("/config", bind(st, |s, r| {
            let on = r.json()?.opt_bool("emboldenCjkFallback").ok_or_else(|| ApiError::bad("需要 {emboldenCjkFallback: bool}"))?;
            s.store.set_embolden(on).map_err(ApiError::internal)?;
            s.bus.publish("fonts", "config");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "emboldenCjkFallback": on, "note": "已更新，翻书即见（fontconfig 实时生效，无需重启）"})))
        }))
        .get("/status", bind(st, |s, _| Ok(Reply::ok(&serde_json::json!({"ok": true, "count": s.store.entries().len(), "target": "native", "cjkFallback": s.store.cjk_fallback_keys(), "emboldenCjkFallback": s.store.embolden()})))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::{Method, TestRequest};

    /// 沙箱里的服务状态：阅读字体 Old-Regular、界面字体 Ui 各一个；不跑 fc-cache / fc-scan。
    fn setup() -> (tempfile::TempDir, Arc<State>, Router) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        paths.ensure().unwrap();
        let mut store = FontStore::new(&paths, FontConfig::default());
        store.side_effects = false;
        let mut ui_store = FontStore::ui(&paths);
        ui_store.side_effects = false;
        for (dir, f) in [(store.fonts_dir().to_path_buf(), "Old-Regular.ttf"), (ui_store.fonts_dir().to_path_buf(), "Ui.ttf")] {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(f), b"\x00\x01\x00\x00").unwrap();
        }
        store.write_index().unwrap();
        ui_store.write_index().unwrap();
        let st = Arc::new(State { store, ui_store, ui: ui::UiFont::load(&paths), paths, bus: Arc::new(rmsvc_core::events::EventBus::new()) });
        let r = router(&st);
        (t, st, r)
    }

    fn select(r: &Router, sans: &str) -> rmsvc_core::http::Reply {
        TestRequest::new(Method::Put, "/ui/select").json(&serde_json::json!({"sans": sans, "serif": ""})).dispatch(r)
    }

    /// 删字体家族：没有这个家族 404（此前 400），删好 200。
    #[test]
    fn remove_family_not_found_is_404() {
        let (_t, _st, r) = setup();
        assert_eq!(TestRequest::new(Method::Delete, "/Nope").dispatch(&r).status, 404);
        assert_eq!(TestRequest::new(Method::Delete, "/ui/Nope").dispatch(&r).status, 404);
        assert_eq!(TestRequest::new(Method::Delete, "/Old-Regular").dispatch(&r).status, 200);
        assert_eq!(TestRequest::new(Method::Delete, "/Old-Regular").dispatch(&r).status, 404);
    }

    /// 界面字体选择：选没装的 400；存选择失败（设备侧写盘故障）500，此前同报 400。
    #[test]
    fn ui_select_splits_bad_request_from_io_failure() {
        let (t, st, r) = setup();
        assert_eq!(select(&r, "Nope").status, 400);
        assert_eq!(select(&r, "Ui").status, 200);
        assert_eq!(st.ui.get().sans, "Ui");
        // ui-font.json 的位置被一个目录占住：原子写改名失败 → 500，选择不变
        let json = Paths::sandbox(t.path()).data_dir().join("ui-font.json");
        std::fs::remove_file(&json).unwrap();
        std::fs::create_dir(&json).unwrap();
        std::fs::write(json.join("x"), b"x").unwrap();
        let reply = select(&r, "");
        assert_eq!(reply.status, 500, "{}", String::from_utf8_lossy(reply.body.as_bytes()));
        assert_eq!(st.ui.get().sans, "Ui");
    }

    /// `/events` 走 sse_reply_for：回的是事件流。
    #[test]
    fn events_route_streams() {
        let (_t, _st, r) = setup();
        let reply = TestRequest::new(Method::Get, "/events").dispatch(&r);
        assert_eq!(reply.status, 200);
        assert!(reply.content_type.starts_with("text/event-stream") && matches!(reply.body, rmsvc_core::http::Body::EventStream(_)));
    }
}
