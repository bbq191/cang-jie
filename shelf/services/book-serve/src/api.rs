//! HTTP 适配层（唯一碰 http 类型的地方，只做取参 + 调领域方法 + 回执）。路由（经网关时前缀 `/api/books`）：
//! `GET /status` · `GET /inbox` · `POST /inbox/retry {name}` · `POST /inbox/delete {name}`
//! 母版库：`GET /staging` → `{items, freeBytes}` · `POST /staging`（multipart，原样入库）· `POST /staging/optimize {name}`
//! （2026-09-19 起不再分档位，只有一种"清洗+优化"行为）
//! · `POST /staging/deliver {name, folder?}`（2026-09-19 起投完永远保留母版，不再有 `keep` 参数）· `POST /staging/mark {name, target}` · `POST /staging/fetch-article {url, optimize?}`
//! · `POST /staging/delete {name}` · `GET /staging/render/{uuid}`（xochitl 渲染缓存 PDF；唯一使用者 host 端 doctor 已砍，路由保留）· `GET /events`（SSE：母版库/inbox 变更即推，网页零轮询）。
//! 原生回收站队列：`POST /trash/add {uuid, name}`（name 必须与书库 visibleName 相符）· `GET /trash/pending` → `{uuids}`（Sidebar 代理 qmd 拉取执行）· `GET /trash`。
//! 原生建文件夹队列：`POST /mkdir/add {name}` · `GET /mkdir/pending` → `{names}`（MainView 代理 shelf-mkdir-agent.qmd 拉取执行）· `GET /mkdir`。
//! 2026-09-05 起规则统一"所有书只落母版库"：旧 `POST /?target=` 直投路已删（`/staging*` 是唯一入口）。
use crate::service_state::State;
use crate::staging::{Reader, StagingStore};
use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::http::{bind, ApiError, ApiResult, Reply, Request, Router};
use std::sync::Arc;

pub fn router(st: Arc<State>) -> Router {
    Router::new()
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))))
        .get("/inbox", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.spool.list()})))))
        .post("/inbox/retry", bind(&st, |s, r| {
            let name = r.json()?.str("name")?.to_string();
            s.spool.retry(&name).map_err(ApiError::bad)?;
            s.invalidate_status();
            Ok(Reply::ok(&serde_json::json!({"ok": true, "items": s.process_inbox(Some(&name))})))
        }))
        .post("/inbox/delete", bind(&st, |s, r| {
            s.spool.delete_failed(r.json()?.str("name")?).map_err(ApiError::bad)?;
            s.invalidate_status();
            s.bus.publish("books", "inbox");
            ok()
        }))
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        // ── 母版库（中间层）：入库 / 优化 / 落库 / 删除各自正交 ──
        .get("/staging", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.staging.list(), "freeBytes": s.staging.free_bytes()})))))
        .post("/staging", bind(&st, staging_upload))
        .post("/staging/optimize", bind(&st, |s, r| {
            // 异步：耗时的优化（真机实测大漫画能跑到分钟级，见书架白皮书 §05）挪到后台线程，这里立即
            // 回"已开始"；真正结果通过 books/staging 事件 + GET /staging 列表里的 delivered.optimize 呈现。
            let j = r.json()?;
            let name = j.str("name")?.to_string();
            s.staging.spawn_optimize(&name, s.bus.clone()).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging"); // 立即推一次，UI 马上看到这条目进入 busy 状态
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("《{name}》已开始优化，完成后自动刷新"), "async": true})))
        }))
        .post("/staging/deliver", bind(&st, |s, r| {
            // 异步：耗时的落库（超限漫画按卷拆分要挨个建包+上传，真机能到分钟级）挪到后台线程，这里
            // 立即回"已开始"；真正结果通过 books/staging 事件 + GET /staging 列表里的 delivered.deliver
            // 呈现（渲染自检、mark_delivered 都在线程内部完成，见 spawn_deliver）。
            let j = r.json()?;
            let name = j.str("name")?.to_string();
            // 母版库永远保留（可再投另一读器对照，2026-09-19 起不再有"投完自动删除"这条路）；
            // folder 空＝配置缺省；非空且真不存在会先经 mkdir 队列建出来再投，见 ensure_folder。
            s.staging.spawn_deliver(&name, j.str_or("folder", ""), s.mkdir.clone(), s.bus.clone()).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging"); // 立即推一次，UI 马上看到这条目进入 busy 状态
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("《{name}》已开始投递，完成后自动刷新"), "async": true})))
        }))
        // 落库记录：KOReader adopt 在 koreader-serve 完成后由前端调这里记一笔（各服务只写自己的目录）。
        .post("/staging/mark", bind(&st, |s, r| {
            let j = r.json()?;
            let reader = Reader::parse(j.str("target")?).map_err(ApiError::bad)?;
            s.staging.mark_delivered(j.str("name")?, reader).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            ok()
        }))
        .post("/staging/fetch-article", bind(&st, |s, r| {
            let j = r.json()?;
            let out = s.staging.fetch_article(j.str("url")?, j.bool_or("optimize", false)).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            let message = match &out.optimize_error {
                Some(e) => format!("已抓取《{}》入母版库（同步优化失败：{e}，可在列表里手动点「优化」）", out.title),
                None if out.optimized => format!("已抓取《{}》入母版库并同步优化", out.title),
                None => format!("已抓取《{}》入母版库", out.title),
            };
            Ok(Reply::ok(&serde_json::json!({"ok": true, "name": out.name, "title": out.title, "message": message})))
        }))
        .get("/staging/render/{uuid}", bind(&st, |s, r| {
            let pdf = s.staging.render_pdf(r.param("uuid")).map_err(ApiError::not_found)?;
            Ok(Reply::bytes("application/pdf", pdf))
        }))
        // ── 漫画页边距待办（QML 代理 shelf-comic-margins.qmd 在书打开时查；见 comic_margins.rs）──
        .get("/margins/{uuid}", bind(&st, |s, r| match s.comic_margins.get(r.param("uuid")) {
            Some(m) => Ok(Reply::ok(&serde_json::json!({"margins": m}))),
            None => Err(ApiError::not_found("没有待设的页边距")),
        }))
        .post("/margins/applied", bind(&st, |s, r| {
            let n = s.comic_margins.applied(r.json()?.str("uuid")?).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "pending": n})))
        }))
        // ── 原生书库回收站队列（真正的软删由 xochitl 自己的 selectionMoveToTrash 执行，见 trash.rs / shelf-trash-agent.qmd）──
        .post("/trash/add", bind(&st, |s, r| {
            let j = r.json()?;
            let n = s.trash.add(j.str("uuid")?, j.str("name")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "trash");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "pending": n, "message": "已排队：书库视图下次有动静时移进回收站"})))
        }))
        .get("/trash/pending", bind(&st, |s, _| {
            let (uuids, pruned) = s.trash.pending().map_err(ApiError::internal)?;
            if pruned > 0 {
                s.bus.publish("books", "trash");
            }
            Ok(Reply::ok(&serde_json::json!({"uuids": uuids})))
        }))
        .get("/trash", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.trash.list()})))))
        // ── 原生书库建文件夹队列（真正的建夹由 xochitl 自己的 Library.createCollection 执行，见 mkdir.rs / shelf-mkdir-agent.qmd）──
        .post("/mkdir/add", bind(&st, |s, r| {
            let n = s.mkdir.add(r.json()?.str("name")?).map_err(ApiError::bad)?;
            s.invalidate_status(); // 文件夹候选可能变了
            if n > 0 {
                s.bus.publish("books", "mkdir");
            }
            Ok(Reply::ok(&serde_json::json!({"ok": true, "pending": n})))
        }))
        .get("/mkdir/pending", bind(&st, |s, _| {
            let (names, pruned) = s.mkdir.pending().map_err(ApiError::internal)?;
            if pruned > 0 {
                s.bus.publish("books", "mkdir");
            }
            Ok(Reply::ok(&serde_json::json!({"names": names})))
        }))
        .get("/mkdir", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.mkdir.list()})))))
        // 停止正在跑的优化/投递（2026-09-20）：登记取消标记，在下一个安全检查点停下；无法中途停的步骤如实回 cancelled:false。
        .post("/staging/cancel", bind(&st, |s, r| {
            let name = r.json()?.str("name")?.to_string();
            let supported = s.staging.request_cancel(&name).map_err(ApiError::bad)?;
            let message = if supported { "已请求停止，会在当前这一小步结束后停下" } else { "这一步无法中途停止（单文件上传中），会自然跑完" };
            Ok(Reply::ok(&serde_json::json!({"ok": true, "cancelled": supported, "message": message})))
        }))
        .post("/staging/delete", bind(&st, |s, r| {
            s.staging.remove(r.json()?.str("name")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            ok()
        }))
}

fn ok() -> ApiResult {
    Ok(Reply::ok(&serde_json::json!({"ok": true})))
}

/// multipart 逐文件原样落母版库（不优化、不落库）：走共享上传模板，暂存在 spool `.work/`（与母版库同分区，入库 rename）。
/// 可选 `?srcName=&srcBytes=`（CLI push 洗书产物才带）：记这份产物的原始输入身份到 sidecar，供下次
/// push 同一份原始文件时**处理前**就能查到已经处理过，见 `sidecar::SourceRef` 文档。
fn staging_upload(st: &State, r: &mut Request<'_>) -> ApiResult {
    let boundary = r.multipart_boundary()?;
    let source = match (r.q("srcName"), r.q("srcBytes").and_then(|v| v.parse::<u64>().ok())) {
        (Some(name), Some(bytes)) => Some(crate::sidecar::SourceRef { name: name.to_string(), bytes }),
        _ => None,
    };
    let _g = st.spool.guard();
    let items = AssetUploadFlow::in_dir(st.spool.work()).run(&StagingStore(&st.staging), &mut *r.body, &boundary).map_err(ApiError::bad)?;
    if let Some(src) = &source {
        // 一次 staging_upload 请求实际上永远只有一个文件部分（CLI/网页都逐文件各发一个 POST），
        // 但这里不假设，多个成功项就都记同一个来源——理论上不会发生，发生了也无害（都是同一份
        // 原始输入触发的上传）。
        for it in items.iter().filter(|i| i.ok) {
            let _ = st.staging.set_source(&it.name, src.clone());
        }
    }
    if items.iter().any(|i| i.ok) {
        st.bus.publish("books", "staging");
    }
    Ok(Reply::ok(&asset::receipt(&items, serde_json::Value::Null)))
}

#[cfg(test)]
mod tests {
    //! 进程内路由测试：不起 socket，直接 `Router::dispatch`——覆盖参数解析、错误码映射与忙锁冲突（这些以前只能上真机验证）。
    use super::*;
    use rmsvc_core::http::{parse_query, Method};
    use rmsvc_core::paths::Paths;
    use std::collections::HashMap;

    fn state(t: &tempfile::TempDir) -> Arc<State> {
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None });
        let cfg = paths.service_config("book");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, r#"{"xochitlHost":"127.0.0.1:9"}"#).unwrap(); // 关闭端口：连接秒拒，不真等超时
        let st = State::new(&paths);
        st.ensure_dirs().unwrap();
        Arc::new(st)
    }

    fn call(router: &Router, m: Method, path: &str, body: &str) -> (u16, serde_json::Value) {
        let mut b = body.as_bytes();
        let mut r = Request { method: m, path: path.into(), query: parse_query(""), params: HashMap::new(), content_type: "application/json".into(), content_length: Some(body.len()), headers: vec![], body: &mut b };
        let rep = router.dispatch(&mut r);
        (rep.status, serde_json::from_slice(&rep.body).unwrap_or(serde_json::Value::Null))
    }

    fn msg(v: &serde_json::Value) -> String {
        v["message"].as_str().unwrap_or("").to_string()
    }

    #[test]
    fn margins_endpoint_returns_pending_then_404_after_applied() {
        const U: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let lib = Paths::resolve({
            let h = t.path().to_str().unwrap().to_string();
            move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None }
        })
        .xochitl_dir();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{U}.metadata")), "{}").unwrap();
        let path = format!("/margins/{U}");
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "没登记 → 404，QML 代理静默不动");
        let qol = Paths::resolve({
            let h = t.path().to_str().unwrap().to_string();
            move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None }
        })
        .home()
        .join(".local/share/cangjie-ime/reading-qol.json");
        std::fs::create_dir_all(qol.parent().unwrap()).unwrap();
        st.comic_margins.add(U, 1).unwrap();
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "实验室开关默认关：已登记也 404，QML 代理不动");
        std::fs::write(&qol, r#"{"comicMinMargin":true}"#).unwrap();
        let (code, v) = call(&router, Method::Get, &path, "");
        assert_eq!((code, v["margins"].as_u64()), (200, Some(1)));
        assert_eq!(call(&router, Method::Post, "/margins/applied", &format!(r#"{{"uuid":"{U}"}}"#)).0, 200);
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "销账后不再返回");
        assert_eq!(call(&router, Method::Get, "/margins/not-a-uuid", "").0, 404, "非法 uuid 一律 404");
    }

    #[test]
    fn busy_book_rejects_optimize_deliver_delete_with_400_and_hint() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        st.staging.stage_new("x.epub", b"PK").unwrap();
        assert!(st.staging.try_start_busy("x.epub"));
        for (path, body) in [
            ("/staging/optimize", r#"{"name":"x.epub"}"#),
            ("/staging/deliver", r#"{"name":"x.epub"}"#),
            ("/staging/delete", r#"{"name":"x.epub"}"#),
        ] {
            let (code, v) = call(&router, Method::Post, path, body);
            assert_eq!(code, 400, "{path}");
            assert!(msg(&v).contains("正在处理中"), "{path}: {v}");
            assert_eq!(v["ok"], false);
        }
        assert!(msg(&call(&router, Method::Post, "/staging/delete", r#"{"name":"x.epub"}"#).1).contains("再删除"), "删除的忙提示带后缀");
        // 解锁后删除恢复正常（200），列表里没有了
        st.staging.end_busy("x.epub");
        assert_eq!(call(&router, Method::Post, "/staging/delete", r#"{"name":"x.epub"}"#).0, 200);
        assert!(st.staging.list().is_empty());
    }

    #[test]
    fn optimize_and_deliver_validate_before_starting_anything() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        // 不存在的书 / 不支持的格式：400，且没有留下忙锁
        for (path, body, hint) in [
            ("/staging/optimize", r#"{"name":"nope.epub"}"#, "没有这本书"),
            ("/staging/optimize", r#"{"name":"a.cbz"}"#, "只有 EPUB/PDF"),
            ("/staging/deliver", r#"{"name":"nope.epub"}"#, "没有这本书"),
            ("/staging/deliver", r#"{"name":"a.cbz"}"#, "xochitl 只读 EPUB"),
        ] {
            let (code, v) = call(&router, Method::Post, path, body);
            assert_eq!(code, 400, "{path} {body}");
            assert!(msg(&v).contains(hint), "{path} {body}: {v}");
        }
        assert!(!st.staging.is_busy("nope.epub") && !st.staging.is_busy("a.cbz"));
        // 缺字段 400；非法 JSON 400
        assert_eq!(call(&router, Method::Post, "/staging/optimize", "{}").0, 400);
        assert_eq!(call(&router, Method::Post, "/staging/optimize", "not json").0, 400);
    }

    #[test]
    fn cancel_reports_three_states_not_busy_uncancellable_cancellable() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let cancel = |name: &str| call(&router, Method::Post, "/staging/cancel", &format!(r#"{{"name":"{name}"}}"#));
        // 没在处理 → 400
        let (code, v) = cancel("x.epub");
        assert_eq!(code, 400);
        assert!(msg(&v).contains("没有在处理"), "{v}");
        // 在处理但这一步不能中途停（如单文件上传）→ 200 cancelled:false
        assert!(st.staging.try_start_busy("x.epub"));
        let (code, v) = cancel("x.epub");
        assert_eq!((code, &v["cancelled"]), (200, &serde_json::json!(false)), "{v}");
        assert!(msg(&v).contains("无法中途停止"));
        // 声明可取消 → 200 cancelled:true，且取消标记已登记
        st.staging.mark_cancellable("x.epub");
        let (code, v) = cancel("x.epub");
        assert_eq!((code, &v["cancelled"]), (200, &serde_json::json!(true)), "{v}");
        assert!(st.staging.is_cancelled("x.epub"));
    }

    #[test]
    fn mark_and_render_and_unknown_routes() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        st.staging.stage_new("m.epub", b"PK").unwrap();
        assert_eq!(call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub","target":"koreader"}"#).0, 200);
        assert!(st.staging.list()[0].delivered.as_ref().unwrap().koreader.is_some(), "记了一笔 KOReader 落库");
        let (code, v) = call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub","target":"kindle"}"#);
        assert_eq!(code, 400);
        assert!(msg(&v).contains("native / koreader"));
        // 渲染缓存 uuid 形状不对：404（路径穿越防线）；方法不对 405；路径不存在 404
        assert_eq!(call(&router, Method::Get, "/staging/render/..%2Fetc", "").0, 404);
        assert_eq!(call(&router, Method::Get, "/staging/optimize", "").0, 405);
        assert_eq!(call(&router, Method::Get, "/nope", "").0, 404);
    }

    #[test]
    fn status_route_reports_spool_and_dead_xochitl() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let (code, v) = call(&router, Method::Get, "/status", "");
        assert_eq!(code, 200);
        assert_eq!((v["ok"].clone(), v["uploadReachable"].clone()), (serde_json::json!(true), serde_json::json!(false)));
        assert_eq!(v["spool"]["pending"], 0);
    }
}
