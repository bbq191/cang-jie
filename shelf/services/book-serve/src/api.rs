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
    let sg = st.clone(); // 母版库列表
    let su = st.clone(); // 母版库上传
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
        // ── 母版库（中间层）：入库/优化/落库/删除各自正交 ──
        .get("/staging", move |_| Ok(Reply::ok(&serde_json::json!({"items": sg.spool.list_staging()}))))
        .post("/staging", move |r| staging_upload(&su, r)) // multipart 原样落，不优化不投递
        .post("/staging/optimize", {
            let s = st.clone();
            move |r| {
                let jb = r.json_body().map_err(ApiError::bad)?;
                let name = jb.get("name").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad("缺 name"))?.to_string();
                if !name.to_ascii_lowercase().ends_with(".epub") {
                    return Err(ApiError::bad("只有 EPUB 能优化（PDF 重排请在电脑用 shelf push）"));
                }
                // 档位 auto（缺省）/ keep-spacing / plain —— 与直传同一套 OptimizeMode。
                let mode = crate::target::OptimizeMode::parse(jb.get("mode").and_then(|v| v.as_str()).unwrap_or("auto"));
                let data = s.spool.read_staging(&name).map_err(ApiError::bad)?;
                // 通用优化（Inline 脚注 + 外链 css 缩进）：两读器都能显示；落库时纯复制此产物，保证两器同字节可对照。
                let opts = bookconv::optimize::OptimizeOpts { wash: mode.wash(), footnote: bookconv::optimize::FootnoteMode::Inline };
                let (out, _) = bookconv::optimize::optimize_epub_with(&data, &opts).map_err(ApiError::bad)?;
                s.spool.overwrite_staging(&name, &out).map_err(ApiError::bad)?;
                Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("已优化《{name}》")})))
            }
        })
        .post("/staging/deliver", {
            let s = st.clone();
            move |r| {
                let j = r.json_body().map_err(ApiError::bad)?;
                let name = j.get("name").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad("缺 name"))?.to_string();
                let keep = j.get("keep").and_then(|v| v.as_bool()).unwrap_or(true); // 母版库默认保留（可再投另一读器对照）
                let folder = j.get("folder").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| s.cfg.library_folder.clone());
                let data = s.spool.read_staging(&name).map_err(ApiError::bad)?;
                // 落库=纯复制原字节投 xochitl，不再优化（优化是母版库独立动作）。原生阅读器只读 EPUB/PDF。
                let ct = bookconv::convert::direct_content_type(&name).map(|c| c.mime()).ok_or_else(|| ApiError::bad("原生阅读器只读 EPUB / PDF；此格式请「加入 KOReader」，或在电脑用 shelf push 转成 EPUB"))?;
                s.xochitl.upload(&data, &name, ct, &folder).map_err(ApiError::bad)?;
                if !keep {
                    let _ = s.spool.remove_staging(&name);
                }
                Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("已投入原生书库《{name}》")})))
            }
        })
        .post("/staging/fetch-article", {
            let s = st.clone();
            move |r| {
                let j = r.json_body().map_err(ApiError::bad)?;
                let url = j.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad("缺 url"))?;
                // 网文抓取（Readability + 白名单）→ 组 EPUB 原样落母版库（未优化，用户按需再点优化）。
                let (epub, title) = bookconv::article::build_article_epub(url).map_err(ApiError::bad)?;
                let fname = format!("{}.epub", bookconv::util::sanitize_filename(&title, "article"));
                let landed = s.spool.stage_new(&fname, &epub).map_err(ApiError::bad)?;
                Ok(Reply::ok(&serde_json::json!({"ok": true, "file": landed, "title": title, "message": format!("已抓取《{title}》入母版库")})))
            }
        })
        .post("/staging/delete", move |r| {
            let name = name_of(r)?;
            st.spool.remove_staging(&name).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true})))
        })
}

/// multipart 逐文件原样落母版库（不优化、不投递）——中间层入库路。逐项回执带落地文件名。
fn staging_upload(st: &State, r: &mut Request<'_>) -> ApiResult {
    let Some(boundary) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
    let mut mp = MultipartReader::new(&mut *r.body, &boundary);
    let mut items: Vec<serde_json::Value> = Vec::new();
    loop {
        let mut part = match mp.next_part() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => return Err(ApiError::bad(format!("multipart 解析失败: {e}"))),
        };
        let Some(fname) = part.filename.clone() else { continue };
        let name = safe_basename(&fname, "upload.bin");
        // 母版库是总入口：任意格式原样入库（2026-09-05 用户定）。能投哪个读器按格式在落库时门控：
        // EPUB/PDF 可投原生；其它格式只能加入 KOReader（想进原生用电脑 shelf push 转 EPUB）。
        let _g = st.spool.guard();
        let staged = st.spool.stage(&name);
        match shelf_core::multipart::receive_part_to(&staged, &mut part) {
            Err(e) => {
                let _ = std::fs::remove_file(&staged);
                items.push(serde_json::json!({"file": name, "ok": false, "message": format!("接收失败: {e}")}));
            }
            Ok(0) => {
                let _ = std::fs::remove_file(&staged);
                items.push(serde_json::json!({"file": name, "ok": false, "message": "空文件"}));
            }
            Ok(_) => {
                let data = std::fs::read(&staged).map_err(|e| ApiError::internal(e.to_string()))?;
                let _ = std::fs::remove_file(&staged);
                match st.spool.stage_new(&name, &data) {
                    Ok(landed) => items.push(serde_json::json!({"file": landed, "ok": true, "message": "已入母版库"})),
                    Err(e) => items.push(serde_json::json!({"file": name, "ok": false, "message": e})),
                }
            }
        }
    }
    let ok = items.iter().all(|i| i["ok"].as_bool().unwrap_or(false)) && !items.is_empty();
    Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items})))
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
        let recv = shelf_core::multipart::receive_part_to(&staged, &mut part);
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
