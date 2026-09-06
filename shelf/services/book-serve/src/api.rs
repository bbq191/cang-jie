//! HTTP 适配层（唯一碰 http 类型的地方，只做取参 + 调领域方法 + 回执）。路由（经网关时前缀 `/api/books`）：
//! `GET /status` · `GET /inbox` · `POST /inbox/retry {name}` · `POST /inbox/delete {name}`
//! 母版库：`GET /staging` → `{items, freeBytes}` · `POST /staging`（multipart，原样入库）· `POST /staging/optimize {name, mode}`
//! · `POST /staging/deliver {name, folder?, keep?}` · `POST /staging/mark {name, target}` · `POST /staging/fetch-article {url}`
//! · `POST /staging/delete {name}` · `GET /events`（SSE：母版库/inbox 变更即推，网页零轮询）。
//! 2026-09-05 起规则统一"所有书只落母版库"：旧 `POST /?target=` 直投路已删（`/staging*` 是唯一入口）。
use crate::service_state::State;
use crate::staging::{OptimizeMode, Reader, StagingStore};
use shelf_core::asset::{self, AssetUploadFlow};
use shelf_core::http::{bind, ApiError, ApiResult, Reply, Request, Router};
use std::sync::Arc;

pub fn router(st: Arc<State>) -> Router {
    Router::new()
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))))
        .get("/inbox", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.spool.list()})))))
        .post("/inbox/retry", bind(&st, |s, r| {
            let name = r.json()?.str("name")?.to_string();
            s.spool.retry(&name).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "items": s.process_inbox(Some(&name))})))
        }))
        .post("/inbox/delete", bind(&st, |s, r| {
            s.spool.delete_failed(r.json()?.str("name")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "inbox");
            ok()
        }))
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        // ── 母版库（中间层）：入库 / 优化 / 落库 / 删除各自正交 ──
        .get("/staging", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.staging.list(), "freeBytes": s.staging.free_bytes()})))))
        .post("/staging", bind(&st, staging_upload))
        .post("/staging/optimize", bind(&st, |s, r| {
            let j = r.json()?;
            let msg = s.staging.optimize(j.str("name")?, OptimizeMode::parse(j.str_or("mode", "auto"))).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": msg})))
        }))
        .post("/staging/deliver", bind(&st, |s, r| {
            let j = r.json()?;
            // 母版库默认保留（可再投另一读器对照）；folder 空＝配置缺省。
            let msg = s.staging.deliver(j.str("name")?, j.str_or("folder", ""), j.bool_or("keep", true)).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": msg})))
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
            let (landed, title) = s.staging.fetch_article(r.json()?.str("url")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "name": landed, "title": title, "message": format!("已抓取《{title}》入母版库")})))
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
fn staging_upload(st: &State, r: &mut Request<'_>) -> ApiResult {
    let boundary = r.multipart_boundary()?;
    let _g = st.spool.guard();
    let items = AssetUploadFlow::in_dir(st.spool.work()).run(&StagingStore(&st.staging), &mut *r.body, &boundary).map_err(ApiError::bad)?;
    if items.iter().any(|i| i.ok) {
        st.bus.publish("books", "staging");
    }
    Ok(Reply::ok(&asset::receipt(&items, serde_json::Value::Null)))
}
