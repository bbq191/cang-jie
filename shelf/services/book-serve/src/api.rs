//! HTTP 适配层（唯一碰 http 类型的地方）。路由（经网关时前缀 `/api/books`）：
//! `POST /?target=native|annot&folder=&optimize=auto|keep-spacing|plain|off&check=on|off`（multipart，多文件逐项回执）
//! `GET /status` · `GET /targets` · `GET /inbox` · `POST /inbox/retry {name}` · `POST /inbox/delete {name}`
use crate::service_state::State;
use crate::target::{DeliverOpts, Outcome};
use shelf_core::http::{ApiError, ApiResult, Reply, Request, Router};
use shelf_core::multipart::{boundary_of, safe_basename, MultipartReader};
use std::io::Read;
use std::sync::Arc;

pub fn router(st: Arc<State>) -> Router {
    let s1 = st.clone();
    let s2 = st.clone();
    let s3 = st.clone();
    let s4 = st.clone();
    let s5 = st.clone();
    Router::new()
        .post("/", move |r| upload(&s1, r))
        .get("/status", move |_| Ok(Reply::ok(&s2.status())))
        .get("/targets", move |_| Ok(Reply::ok(&serde_json::json!({"targets": s3.targets.ids().iter().map(|(i,l)| serde_json::json!({"id":i,"label":l})).collect::<Vec<_>>()}))))
        .get("/inbox", move |_| Ok(Reply::ok(&serde_json::json!({"items": s4.spool.list()}))))
        .post("/inbox/retry", {
            let s = st.clone();
            move |r| {
                let name = name_of(r)?;
                s.spool.retry(&name).map_err(ApiError::bad)?;
                let items = s.process_inbox(Some(&name));
                Ok(Reply::ok(&serde_json::json!({"ok": true, "items": items})))
            }
        })
        .post("/inbox/delete", move |r| {
            let name = name_of(r)?;
            s5.spool.delete_failed(&name).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        })
}

fn name_of(r: &mut Request<'_>) -> Result<String, ApiError> {
    let j = r.json_body().map_err(ApiError::bad)?;
    j.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).ok_or_else(|| ApiError::bad("缺 name"))
}

/// multipart 逐文件：流式落 .work 暂存 → 目标投递 → 归档 done/failed。**不经 inbox**（inbox 只给 scp 追平）。
fn upload(st: &State, r: &mut Request<'_>) -> ApiResult {
    let target_id = r.q("target").unwrap_or("").to_string();
    let Some(target) = st.targets.get(&target_id) else {
        return Err(ApiError::bad(format!("未知目标 {target_id:?}（本服务只收 native/annot；KOReader 走 /api/koreader/books）")));
    };
    let opts = DeliverOpts {
        folder: r.q("folder").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        optimize: crate::target::OptimizeMode::parse(r.q("optimize").unwrap_or("auto")),
        check: r.q("check").map(|v| v != "off").unwrap_or(true),
    };
    let Some(boundary) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
    let mut mp = MultipartReader::new(&mut *r.body, &boundary);
    let mut items: Vec<Outcome> = Vec::new();
    loop {
        let mut part = match mp.next_part() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => return Err(ApiError::bad(format!("multipart 解析失败: {e}"))),
        };
        let Some(fname) = part.filename.clone() else { continue };
        let name = safe_basename(&fname, "upload.bin");
        if let Err(e) = target.accepts(&name) {
            items.push(Outcome { file: name, target: target.id().into(), ok: false, message: e });
            continue;
        }
        let _g = st.spool.guard();
        let staged = st.spool.stage(&name);
        let recv = (|| -> Result<u64, String> {
            let mut f = std::fs::File::create(&staged).map_err(|e| e.to_string())?;
            std::io::copy(&mut part, &mut f).map_err(|e| e.to_string())
        })();
        match recv {
            Err(e) => {
                let _ = std::fs::remove_file(&staged);
                items.push(Outcome { file: name, target: target.id().into(), ok: false, message: format!("接收失败: {e}") });
            }
            Ok(0) => {
                let _ = std::fs::remove_file(&staged);
                items.push(Outcome { file: name, target: target.id().into(), ok: false, message: "空文件".into() });
            }
            Ok(_) => {
                let data = std::fs::read(&staged).map_err(|e| ApiError::internal(e.to_string()))?;
                let o = target.deliver(&name, data, &opts);
                if o.ok {
                    st.spool.archive_done(&staged);
                } else {
                    st.spool.archive_failed(&staged, &o.message);
                }
                items.push(o);
            }
        }
    }
    let ok = items.iter().all(|i| i.ok) && !items.is_empty();
    Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items})))
}

/// 小 body 读尽（保留给将来 JSON 端点复用）。
#[allow(dead_code)]
fn drain(r: &mut Request<'_>) {
    let mut sink = Vec::new();
    let _ = r.body.take(4096).read_to_end(&mut sink);
}
