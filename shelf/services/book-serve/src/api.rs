//! HTTP 适配层（唯一碰 http 类型的地方，只做取参 + 调领域方法 + 回执）。路由（经网关时前缀 `/api/books`）：
//! `GET /status`
//! 母版库：`GET /staging` → `{items, freeBytes, lowSpace}`（`lowSpace`：剩余空间低于 300 MiB，判据见 `staging::low_space`，查不到空间为 false）· `POST /staging`（multipart，原样入库）
//! · `POST /staging/deliver {name, folder?}`（2026-09-19 起投完永远保留母版，不再有 `keep` 参数；`POST /staging/mark` 2026-10-07 删，
//!   原调用方是已删的网关批量「加入 KOReader」）
//! · `POST /staging/delete {name}` · `POST /staging/rename {name, newName}` · `GET /staging/file?name=`（原件下载，流式）
//! · 2026-10-07 删除：`/staging/optimize`、`/staging/fetch-article`、`/staging/originals/*`、`/staging/cancel`（书架不再优化书，
//!   优化全部在电脑上用 sheng-ren 做；剩下的投递没有能中途停的步骤）。
//! · `GET /events`（SSE：母版库/inbox 变更即推，网页零轮询）。
//! 原生回收站队列：`POST /trash/add {uuid, name}`（name 必须与书库 visibleName 相符）· `GET /trash/pending?wait=` → `{uuids}`（MainView 代理 qmd 长轮询拉取执行）· `GET /trash`（只供调试：ssh 上 curl 看队列）。
//! 原生建文件夹队列：`POST /mkdir/add {name}`（建在书库根）· `GET /mkdir/pending?wait=` → `{items: [{name, parent}]}`（MainView 代理
//! shelf-mkdir-agent.qmd 长轮询拉取执行；`parent` 是上级文件夹 uuid，空串＝书库根）· `GET /mkdir`（只供调试）。
//! 代理放弃记录：`GET /agent-failures` → `{items:[{kind,name,uuid?,at}]}` · `POST /agent-failures/clear`（两个队列交满次数仍没做成的项）。
//! 2026-09-05 起规则统一"所有书只落母版库"：旧 `POST /?target=` 直投路已删（`/staging*` 是网页的唯一入口）。
//! 直接导入 xochitl、不进母版库（2026-10-07，电脑上的 sheng-ren `booklib sync` 经 SSH 端口转发直连 8790 调，见 import.rs）。
//! **异步**（同 `/staging/deliver`）：收完请求体、做完快速校验就回 `202 {job}`，建文件夹、上传、等 xochitl 排版、认领在后台
//! 作业队列里一次一个做（jobs.rs，与 `/staging/deliver` 共用），结果用任务 id 查：
//! · `POST /import?name=<文件名.epub>&folder=<文件夹路径，`/` 分多级，可空＝书库根>`（请求体＝EPUB 原始字节）→ `202 {job}`
//!   （文件夹逐级找、没有就建；目标文件夹里已有逐字节相同的书就认回它、不重复加入）
//! · `POST /import?uuid=<uuid>&name=<文件名.epub>`：原地替换已有文档内容、uuid 不变 → `202 {job}`；文档不在 / 已删 / 在回收站 / 不是 EPUB → 404
//!   （文件名 / zip 头 / 大小不对 → 400；同一 uuid 正在替换 → 409，等 `GET /import/{uuid}` 的 `replacing` 变假再交；都在回 202 之前）
//! · `GET /import/jobs/{id}` → `{job, state: running|done|failed, stage, uuid?, name?, folder?, message?}`（done 带 uuid/name/folder，
//!   `folder`＝实际落进的完整路径；failed 带 message；排队中 running + stage「排队」）；不存在（含 book-serve 重启丢了、做完超过 1 小时清掉）→ 404
//! · `GET /import/{uuid}` → `{uuid, name, folder, deleted, replacing}`（不存在 → 404；`replacing`＝正在原地替换）。删除用 `POST /trash/add {uuid, name}`。
//! · `POST /import/states {uuids: [...]}` → `{docs: {<uuid>: {name, folder, deleted, replacing}}}`：一次查一批（不存在的不出现在 `docs` 里）；
//!   sheng-ren 每轮 sync 开头查一次，免得每本书各开一次请求（2026-10-09）。一次最多 [`STATES_MAX`] 个，多了 400。
use crate::service_state::State;
use crate::staging::StagingStore;
use rmsvc_core::asset::{self, AssetUploadFlow};
use rmsvc_core::formats;
use rmsvc_core::http::{bind, ApiError, ApiResult, Reply, Request, Router};
use std::sync::Arc;

/// `GET /mkdir/pending?wait=`、`GET /trash/pending?wait=` 长轮询等待时长上限（秒）。QML 端（shelf-mkdir-agent.qmd）发 wait=290：设备 Qt 6.10
/// 的 QML XHR 不设传输超时（2026-09-24 核实，见 qmd 头注；09-22 版按"缺省 30s 超时"的假设把这里定成 28）。
/// 在等的这段时间服务端不读 socket，所以不受 rmsvc-core 的读空闲超时影响。
const AGENT_WAIT_MAX_SECS: u64 = 300;

pub fn router(st: Arc<State>) -> Router {
    Router::new()
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))))
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        // ── 母版库（中间层）：入库 / 落库 / 删除各自正交 ──
        .get("/staging", bind(&st, |s, _| {
            let free = s.staging.free_bytes();
            Ok(Reply::ok(&serde_json::json!({"items": s.staging.list(), "freeBytes": free, "lowSpace": crate::staging::low_space(free)})))
        }))
        // 原件下载：边读边发（大书上百 MB，不整本读进内存）；网关见到 Content-Disposition 也原样流式转发。
        .get("/staging/file", bind(&st, |s, r| {
            let name = r.q_required("name")?.to_string();
            let (f, len) = s.staging.open_for_download(&name).map_err(ApiError::bad)?;
            // 母版库只收 EPUB/PDF；更早留下的别的格式按二进制下载
            let ctype = if formats::has_ext(&name, formats::NATIVE_EXTS) { formats::mime_of(&name) } else { "application/octet-stream" };
            Ok(Reply::sized_stream(ctype, Box::new(std::io::BufReader::new(f)), len).with_header("Content-Disposition", &rmsvc_core::multipart::content_disposition(&name)))
        }))
        .post("/staging/rename", bind(&st, |s, r| {
            let j = r.json()?;
            let new_name = s.staging.rename(j.str("name")?, j.str("newName")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "name": new_name})))
        }))
        .post("/staging", bind(&st, staging_upload))
        .post("/staging/deliver", bind(&st, |s, r| {
            // 异步：耗时的落库（大书上传、等建文件夹、大文件通道拷贝，真机能到分钟级）挪到后台线程，这里
            // 立即回"已开始"；真正结果通过 books/staging 事件 + GET /staging 列表里的 delivered.deliver
            // 呈现（渲染自检、mark_delivered 都在线程内部完成，见 spawn_deliver）。
            let j = r.json()?;
            let name = j.str("name")?.to_string();
            // 母版库永远保留（可再投另一读器对照，2026-09-19 起不再有"投完自动删除"这条路）；
            // folder 空＝书库根；非空＝书库根下这个名字的文件夹，没有会先经 mkdir 队列建出来再投，见 XochitlDelivery::ensure_folder。
            // 排进与直接导入共用的后台作业队列，一次一本（见 jobs.rs）。
            s.staging.spawn_deliver(&name, j.str_or("folder", ""), &s.jobs, s.bus.clone()).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging"); // 立即推一次，UI 马上看到这条目进入 busy 状态
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("《{name}》已开始投递，完成后自动刷新"), "async": true})))
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
        // ── 原地替换后找回阅读位置（QML 代理 shelf-keep-progress.qmd 在书打开时查；见 progress.rs）──
        // `?pages=` 是阅读器当前总页数（新 .content 还没写时定最后一个文件的页数用），可省
        .get("/progress/{uuid}", bind(&st, |s, r| match s.progress.lookup(r.param("uuid"), r.q_parse::<u32>("pages")) {
            crate::progress::Lookup::None => Err(ApiError::not_found("没有待恢复的阅读位置")),
            crate::progress::Lookup::Pending => Ok(Reply::json(202, &serde_json::json!({"pending": true}))),
            crate::progress::Lookup::Page(p) => Ok(Reply::ok(&serde_json::json!({"page": p}))),
        }))
        .post("/progress/applied", bind(&st, |s, r| {
            let j = r.json()?;
            let uuid = j.str("uuid")?;
            let removed = s.progress.applied(uuid).map_err(ApiError::bad)?;
            // 代理等了约一分钟新排版还没出来就放弃（reason=timeout）：删掉快照，免得之后某次打开把用户已经读到的地方又拉回去
            if j.str_or("reason", "") == "timeout" && removed {
                println!("[book-serve] {uuid} 新排版迟迟没出来，放弃恢复阅读位置");
            }
            Ok(Reply::ok(&serde_json::json!({"ok": true, "removed": removed})))
        }))
        // ── 原生书库回收站队列（真正的软删由 xochitl 自己的 selectionMoveToTrash 执行，见 trash.rs / shelf-trash-agent.qmd）──
        // 回执文案（2026-10-10 改）：09-25 起代理全局常驻、长轮询，入队即由 xochitl 内的代理执行；此前写的是旧机制
        // "书库视图下次有动静时移进回收站"。
        .post("/trash/add", bind(&st, |s, r| {
            let j = r.json()?;
            let n = s.trash.add(j.str("uuid")?, j.str("name")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "trash");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "pending": n, "message": "已排队，xochitl 会随即移进回收站"})))
        }))
        .get("/trash/pending", bind(&st, |s, r| {
            let (uuids, pruned) = s.trash.pending_wait(agent_wait(r)).map_err(ApiError::internal)?;
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
        // `?wait=<秒>` 长轮询（上限 [`AGENT_WAIT_MAX_SECS`]）：有待办立即回，否则阻塞到入队或到期回空；缺省 0＝立即返回。
        .get("/mkdir/pending", bind(&st, |s, r| {
            let (items, pruned) = s.mkdir.pending_wait(agent_wait(r)).map_err(ApiError::internal)?;
            if pruned > 0 {
                s.bus.publish("books", "mkdir");
            }
            // `items` 带上级文件夹（2026-10-07 按层建）。2026-10-10 删了只放根下项的 `names`：那是给 10-07 前的旧代理留的兼容，
            // 代理与本服务总由 shelf/install.sh 一起装，走不到。
            Ok(Reply::ok(&serde_json::json!({"items": items})))
        }))
        .get("/mkdir", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.mkdir.list()})))))
        // ── 代理执行不成、已放弃的记录（网页页头横幅；「知道了」→ clear），见 agent_failures.rs ──
        .get("/agent-failures", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.agent_failures.list()})))))
        .post("/agent-failures/clear", bind(&st, |s, _| {
            let n = s.agent_failures.clear().map_err(ApiError::internal)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "cleared": n})))
        }))
        // ── 直接导入 xochitl（不进母版库，见 import.rs）：异步，回 202 + 任务 id，结果查 /import/jobs/{id} ──
        .post("/import", bind(&st, import_book))
        .get("/import/jobs/{id}", bind(&st, |s, r| s.jobs.get(r.param("id")).map(|v| Reply::ok(&v)).ok_or_else(|| ApiError::not_found("没有这个导入任务（可能 book-serve 重启过，或做完超过 1 小时已清掉）"))))
        .post("/import/states", bind(&st, import_states))
        .get("/import/{uuid}", bind(&st, |s, r| match s.import.describe(r.param("uuid")) {
            Some(d) => Ok(Reply::ok(&serde_json::json!({"uuid": d.uuid, "name": d.name, "folder": d.folder, "deleted": d.deleted, "replacing": d.replacing}))),
            None => Err(ApiError::not_found("xochitl 书库里没有这份文档")),
        }))
        .post("/staging/delete", bind(&st, |s, r| {
            s.staging.remove(r.json()?.str("name")?).map_err(ApiError::bad)?;
            staging_changed(s)
        }))
}

/// 两个代理队列长轮询的 `?wait=<秒>`：缺省/非法＝0（立即返回），上限 [`AGENT_WAIT_MAX_SECS`]。
fn agent_wait(r: &Request<'_>) -> std::time::Duration {
    std::time::Duration::from_secs(r.q_parse::<u64>("wait").unwrap_or(0).min(AGENT_WAIT_MAX_SECS))
}

/// 母版库变更类操作的统一收尾：推一条 `books/staging`（网页据此重拉列表，零轮询）再回 `{ok:true}`。
fn staging_changed(s: &State) -> ApiResult {
    s.bus.publish("books", "staging");
    Ok(Reply::ok(&serde_json::json!({"ok": true})))
}

/// multipart 逐文件原样落母版库（不优化、不落库）：走共享上传模板，暂存在 spool `.work/`（与母版库同分区，入库 rename）。
/// （曾经的 `?srcName=&srcBytes=` 是 host CLI 洗书产物记原始输入用的，CLI 2026-09-18 已砍，参数随之删除。）
fn staging_upload(st: &State, r: &mut Request<'_>) -> ApiResult {
    let boundary = r.multipart_boundary()?;
    // 不持 spool 锁：暂存名是随机的 `.<uuid>.book.part`，与 inbox 追平互不相干；真正会撞的"挑名 + 落地"
    // 由 `Staging` 内部的落名临界区串行化（见 `staging::Staging` 的 `land` 字段）。
    let items = AssetUploadFlow::in_dir(st.spool.work()).run(&StagingStore(&st.staging), &mut *r.body, &boundary).map_err(ApiError::bad)?;
    if asset::any_ok(&items) {
        st.bus.publish("books", "staging");
    }
    Ok(Reply::ok(&asset::receipt(&items, serde_json::Value::Null)))
}

/// `POST /import/states` 一次最多查这么多个（sheng-ren 的书库一台 Move 几百本；读的只是 `.metadata`）。
const STATES_MAX: usize = 10_000;

/// `POST /import/states {uuids}`：一批文档的状态，口径同 `GET /import/{uuid}`（不存在的不回）。
fn import_states(st: &State, r: &mut Request<'_>) -> ApiResult {
    let uuids = r.json()?.str_list("uuids");
    if uuids.len() > STATES_MAX {
        return Err(ApiError::bad(format!("一次最多查 {STATES_MAX} 个")));
    }
    let docs: serde_json::Map<String, serde_json::Value> = uuids
        .iter()
        .filter_map(|u| st.import.describe(u))
        .map(|d| (d.uuid, serde_json::json!({"name": d.name, "folder": d.folder, "deleted": d.deleted, "replacing": d.replacing})))
        .collect();
    Ok(Reply::ok(&serde_json::json!({"docs": docs})))
}

/// `POST /import`：有 `uuid` → 原地替换，没有 → 新导入。请求体是 EPUB 原始字节（流式落盘，不进内存）。
/// 这里只收体、做快速校验（出错照旧 400 / 404），然后把剩下的交给后台任务队列，立即回 `202 {job}`。
fn import_book(st: &State, r: &mut Request<'_>) -> ApiResult {
    use crate::import::ImportError;
    let name = r.q_required("name")?.to_string();
    let len = r.content_length;
    let accepted = match r.q("uuid").map(str::to_string) {
        Some(uuid) => st.import.accept_replace(&uuid, &name, &mut *r.body, len),
        None => {
            let folder = r.q("folder").unwrap_or("").to_string();
            st.import.accept_new(&name, &folder, &mut *r.body, len)
        }
    };
    // 失败也记进日志（此前只回给客户端，客户端等不到回执时设备上查不到原因）
    let accepted = accepted.map_err(|e| {
        eprintln!("[book-serve] 直接导入《{name}》失败：{e:?}");
        match e {
            ImportError::Bad(m) => ApiError::bad(m),
            ImportError::Busy(m) => ApiError { status: 409, message: m },
            ImportError::NotFound(m) => ApiError::not_found(m),
            ImportError::Failed(m) => ApiError::internal(m),
        }
    })?;
    let (import, bus, invalidate) = (st.import.clone(), st.bus.clone(), st.status_invalidator());
    let id = st.jobs.submit(Box::new(move |stage| {
        let res = import.run(accepted, stage);
        match &res {
            Ok(_) => {
                invalidate(); // 可能新建了文件夹
                bus.publish("books", "import");
            }
            Err(e) => eprintln!("[book-serve] 直接导入《{name}》失败：{e:?}"),
        }
        res.map_err(|e| e.message().to_string())
    }));
    Ok(Reply::json(202, &serde_json::json!({"job": id})))
}

#[cfg(test)]
mod tests {
    //! 进程内路由测试：不起 socket，直接 `Router::dispatch`——覆盖参数解析、错误码映射与忙锁冲突（这些以前只能上真机验证）。
    use super::*;
    use rmsvc_core::http::{parse_query, Method};
    use rmsvc_core::paths::Paths;
    use std::collections::HashMap;

    fn state(t: &tempfile::TempDir) -> Arc<State> {
        state_with_host(t, "127.0.0.1:9") // 关闭端口：连接秒拒，不真等超时
    }

    fn paths(t: &tempfile::TempDir) -> Paths {
        Paths::sandbox(t.path())
    }

    /// xochitl 客户端连 `host` 的服务状态（直接导入的路由测试接假 xochitl）。
    fn state_with_host(t: &tempfile::TempDir, host: &str) -> Arc<State> {
        let paths = paths(t);
        let cfg = paths.service_config("book");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, format!(r#"{{"xochitlHost":"{host}"}}"#)).unwrap();
        let mut st = State::new(&paths);
        st.inbox_settle = std::time::Duration::ZERO; // 测试里刚写的 inbox 文件也立即处理
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
        let lib = paths(&t).xochitl_dir();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{U}.metadata")), "{}").unwrap();
        let path = format!("/margins/{U}");
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "没登记 → 404，QML 代理静默不动");
        st.comic_margins.add(U, 1).unwrap();
        let (code, v) = call(&router, Method::Get, &path, "");
        assert_eq!((code, v["margins"].as_u64()), (200, Some(1)));
        assert_eq!(call(&router, Method::Post, "/margins/applied", &format!(r#"{{"uuid":"{U}"}}"#)).0, 200);
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "销账后不再返回");
        assert_eq!(call(&router, Method::Get, "/margins/not-a-uuid", "").0, 404, "非法 uuid 一律 404");
    }

    /// 阅读位置快照：没有 → 404；新排版没出来 → 202 pending；出来了 → 200 page；应用后 → 404。
    #[test]
    fn progress_endpoint_404_pending_page_then_404_after_applied() {
        use crate::progress::tests as pt;
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let lib = paths(&t).xochitl_dir();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{}.metadata", pt::U)), r#"{"type":"DocumentType","lastOpenedPage":120}"#).unwrap();
        std::fs::write(lib.join(format!("{}.epubindex", pt::U)), pt::REAL_INDEX).unwrap();
        let path = format!("/progress/{}", pt::U);
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "没快照 → 404，代理静默不动");
        st.progress.before_replace(pt::U).unwrap();
        std::fs::remove_file(lib.join(format!("{}.epubindex", pt::U))).unwrap();
        let (code, v) = call(&router, Method::Get, &path, "");
        assert_eq!((code, v["pending"].as_bool()), (202, Some(true)));
        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(lib.join(format!("{}.epubindex", pt::U)), pt::REAL_INDEX).unwrap();
        let (code, v) = call(&router, Method::Get, &path, "");
        assert_eq!((code, v["page"].as_u64()), (200, Some(120)));
        let (code, v) = call(&router, Method::Post, "/progress/applied", &format!(r#"{{"uuid":"{}"}}"#, pt::U));
        assert_eq!((code, v["removed"].as_bool()), (200, Some(true)));
        assert_eq!(call(&router, Method::Get, &path, "").0, 404, "应用后不再返回");
        assert_eq!(call(&router, Method::Get, "/progress/not-a-uuid", "").0, 404);
        assert_eq!(call(&router, Method::Post, "/progress/applied", r#"{"uuid":"../x"}"#).0, 400);
    }

    /// `GET /staging` 带 `lowSpace`（布尔，与 `freeBytes` 同一次取样算出）。
    #[test]
    fn staging_list_reports_low_space_flag() {
        let t = tempfile::tempdir().unwrap();
        let router = router(state(&t));
        let (code, v) = call(&router, Method::Get, "/staging", "");
        assert_eq!(code, 200);
        let free = v["freeBytes"].as_u64();
        assert_eq!(v["lowSpace"].as_bool(), Some(crate::staging::low_space(free)), "{v}");
    }

    #[test]
    fn busy_book_rejects_deliver_delete_with_400_and_hint() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        st.staging.stage_new("x.epub", b"PK").unwrap();
        let busy = st.staging.busy_guard("x.epub", "").unwrap();
        for (path, body) in [
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
        drop(busy);
        assert_eq!(call(&router, Method::Post, "/staging/delete", r#"{"name":"x.epub"}"#).0, 200);
        assert!(st.staging.list().is_empty());
    }

    #[test]
    fn deliver_validates_before_starting_anything() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        // 不存在的书 / 不支持的格式：400，且没有留下忙锁
        for (path, body, hint) in [
            ("/staging/deliver", r#"{"name":"nope.epub"}"#, "没有这本书"),
            ("/staging/deliver", r#"{"name":"a.cbz"}"#, "xochitl 只读 EPUB"),
        ] {
            let (code, v) = call(&router, Method::Post, path, body);
            assert_eq!(code, 400, "{path} {body}");
            assert!(msg(&v).contains(hint), "{path} {body}: {v}");
        }
        assert!(!st.staging.is_busy("nope.epub") && !st.staging.is_busy("a.cbz"));
        // 缺字段 400；非法 JSON 400
        assert_eq!(call(&router, Method::Post, "/staging/deliver", "{}").0, 400);
        assert_eq!(call(&router, Method::Post, "/staging/deliver", "not json").0, 400);
    }

    #[test]
    fn unknown_routes() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        assert_eq!(call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub"}"#).0, 404, "/staging/mark 已删");
        // 方法不对 405；路径不存在 404（已删的死路由 /inbox*、/staging/render/*、/staging/optimize 等同样 404）
        assert_eq!(call(&router, Method::Get, "/staging/render/abc", "").0, 404);
        assert_eq!(call(&router, Method::Get, "/inbox", "").0, 404);
        assert_eq!(call(&router, Method::Get, "/staging/deliver", "").0, 405);
        for gone in ["/staging/optimize", "/staging/fetch-article", "/staging/cancel", "/staging/originals/restore"] {
            assert_eq!(call(&router, Method::Post, gone, r#"{"name":"m.epub"}"#).0, 404, "{gone} 已删");
        }
        assert_eq!(call(&router, Method::Get, "/nope", "").0, 404);
    }


    /// 回归：网页上传收请求体期间（WiFi 上传大书可达分钟级）不再攥着 spool 锁——inbox 追平照常进行，
    /// 上传收完也照常入库。请求体用一个"等放行信号才吐数据"的 Reader 模拟慢客户端。
    #[test]
    fn slow_upload_does_not_block_inbox_processing() {
        struct Gate(std::sync::mpsc::Receiver<Vec<u8>>, Vec<u8>);
        impl std::io::Read for Gate {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.1.is_empty() {
                    match self.0.recv() {
                        Ok(b) => self.1 = b,
                        Err(_) => return Ok(0),
                    }
                }
                let n = self.1.len().min(out.len());
                out[..n].copy_from_slice(&self.1[..n]);
                self.1.drain(..n);
                Ok(n)
            }
        }
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let up = std::thread::spawn(move || {
            let mut body = Gate(rx, Vec::new());
            let mut r = Request { method: Method::Post, path: "/staging".into(), query: parse_query(""), params: HashMap::new(), content_type: "multipart/form-data; boundary=B".into(), content_length: None, headers: vec![], body: &mut body };
            router.dispatch(&mut r).status
        });
        tx.send(b"--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"up.epub\"\r\n\r\nPK".to_vec()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100)); // 上传线程此刻卡在读请求体
        std::fs::write(st.spool.inbox().join("scp.epub"), b"x").unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let st2 = st.clone();
        std::thread::spawn(move || done_tx.send(st2.process_inbox()).unwrap());
        let out = done_rx.recv_timeout(std::time::Duration::from_secs(5)).expect("上传收体期间 inbox 追平不该被卡住");
        assert!(out.iter().any(|o| o.ok && o.name == "scp.epub"));
        tx.send(b"\r\n--B--\r\n".to_vec()).unwrap();
        drop(tx);
        assert_eq!(up.join().unwrap(), 200);
        assert!(st.staging.has("up.epub") && st.staging.has("scp.epub"));
    }

    #[test]
    fn download_route_streams_with_mime_length_and_disposition() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        st.staging.stage_new("书.PDF", b"%PDF-1").unwrap();
        let mut empty: &[u8] = b"";
        let mut r = Request { method: Method::Get, path: "/staging/file".into(), query: parse_query("name=%E4%B9%A6.PDF"), params: HashMap::new(), content_type: String::new(), content_length: None, headers: vec![], body: &mut empty };
        let rep = router.dispatch(&mut r);
        assert_eq!((rep.status, rep.content_type.as_str()), (200, "application/pdf"), "扩展名大小写不敏感");
        let h = |k: &str| rep.headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(h("Content-Length"), "6");
        assert!(h("Content-Disposition").contains("filename*=UTF-8''%E4%B9%A6.PDF"));
    }

    /// `POST /import` 带原始字节体（查询串取参）。
    fn post_raw(router: &Router, query: &str, body: &[u8]) -> (u16, serde_json::Value) {
        let mut b = body;
        let mut r = Request { method: Method::Post, path: "/import".into(), query: parse_query(query), params: HashMap::new(), content_type: "application/epub+zip".into(), content_length: Some(body.len()), headers: vec![], body: &mut b };
        let rep = router.dispatch(&mut r);
        (rep.status, serde_json::from_slice(&rep.body).unwrap_or(serde_json::Value::Null))
    }

    /// `POST /import` 回 202 后等任务做完，回任务查询结果。
    fn post_and_wait(router: &Router, query: &str, body: &[u8]) -> serde_json::Value {
        let (code, v) = post_raw(router, query, body);
        assert_eq!(code, 202, "{v}");
        let id = v["job"].as_str().expect("回执带任务 id").to_string();
        let t0 = std::time::Instant::now();
        loop {
            let (code, v) = call(router, Method::Get, &format!("/import/jobs/{id}"), "");
            assert_eq!(code, 200, "{v}");
            assert_eq!(v["job"], id.as_str());
            if v["state"] != "running" {
                return v;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(10), "任务一直没做完：{v}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// 直接导入的路由：同步校验的错误码照旧（非 epub / 缺 name 400、文档不在 / 已删 404，都在回 202 之前）；
    /// 原地替换走任务、保留 uuid；查询。
    #[test]
    fn import_routes_map_errors_replace_and_query() {
        const U: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let lib = st.xochitl.library_dir().to_path_buf();
        std::fs::create_dir_all(&lib).unwrap();
        let epub = b"PK\x03\x04rest-of-zip";
        let (code, v) = post_raw(&router, "name=a.pdf", epub);
        assert_eq!(code, 400);
        assert!(msg(&v).contains("只收 .epub"), "{v}");
        assert_eq!(post_raw(&router, "", epub).0, 400, "缺 name");
        assert_eq!(post_raw(&router, "name=a.epub", b"not a zip").0, 400, "zip 头不对，回 202 之前就拒");
        assert_eq!(post_raw(&router, &format!("uuid={U}&name=a.epub"), epub).0, 404, "文档不在 → 404，客户端改成新导入");
        assert_eq!(call(&router, Method::Get, &format!("/import/{U}"), "").0, 404);
        std::fs::write(lib.join(format!("{U}.metadata")), r#"{"type":"DocumentType","visibleName":"书","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{U}.epub")), b"PK\x03\x04old").unwrap();
        std::fs::write(lib.join(format!("{U}.pdf")), b"cache").unwrap();
        let v = post_and_wait(&router, &format!("uuid={U}&name=a.epub"), epub);
        assert_eq!((v["state"].as_str(), v["uuid"].as_str(), v["name"].as_str(), v["folder"].as_str()), (Some("done"), Some(U), Some("书"), Some("")), "{v}");
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), epub);
        assert!(!lib.join(format!("{U}.pdf")).exists());
        let (code, v) = call(&router, Method::Get, &format!("/import/{U}"), "");
        assert_eq!((code, v), (200, serde_json::json!({"uuid": U, "name": "书", "folder": "", "deleted": false, "replacing": false})));
        std::fs::write(lib.join(format!("{U}.metadata")), r#"{"type":"DocumentType","visibleName":"书","parent":"trash"}"#).unwrap();
        assert_eq!(call(&router, Method::Get, &format!("/import/{U}"), "").1["deleted"], true, "进了回收站算 deleted");
        assert_eq!(post_raw(&router, &format!("uuid={U}&name=a.epub"), epub).0, 404, "回收站里的不替换");
        assert_eq!(call(&router, Method::Get, "/import/jobs/nope", "").0, 404, "未知任务 404");
        // 一次查一批：在的（含回收站里的）回状态，不在的、不像 uuid 的不回
        const V: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let (code, v) = call(&router, Method::Post, "/import/states", &serde_json::json!({"uuids": [U, V, "../x"]}).to_string());
        assert_eq!((code, v), (200, serde_json::json!({"docs": {U: {"name": "书", "folder": "", "deleted": true, "replacing": false}}})));
        assert!(st.staging.list().is_empty(), "不进母版库");
    }

    /// 新导入：202 → 任务做完 done 带 uuid/name/folder；同一份字节再导一次认回同一个 uuid；内容不同另加一本；
    /// 临时文件用完就删、不进母版库。
    #[test]
    fn import_new_is_async_and_idempotent() {
        let t = tempfile::tempdir().unwrap();
        let lib = paths(&t).xochitl_dir();
        std::fs::create_dir_all(&lib).unwrap();
        let st = state_with_host(&t, &crate::staging::tests::fake_xochitl(lib.clone()));
        let router = router(st.clone());
        let a = crate::staging::tests::mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let v = post_and_wait(&router, "name=%E7%94%B2.epub", &a);
        assert_eq!((v["state"].as_str(), v["stage"].as_str(), v["name"].as_str(), v["folder"].as_str()), (Some("done"), Some("完成"), Some("甲.epub"), Some("")), "{v}");
        let uuid = v["uuid"].as_str().unwrap().to_string();
        assert_eq!(std::fs::read(lib.join(format!("{uuid}.epub"))).unwrap(), a);
        let again = post_and_wait(&router, "name=%E7%94%B2.epub", &a);
        assert_eq!(again["uuid"].as_str(), Some(uuid.as_str()), "同一文件夹同字节：认回原来那份");
        let b = crate::staging::tests::mini_epub(&[("OEBPS/a.xhtml", "<p>乙</p>")]);
        let other = post_and_wait(&router, "name=%E7%94%B2.epub", &b);
        assert_eq!(other["state"], "done");
        assert_ne!(other["uuid"].as_str(), Some(uuid.as_str()), "内容不同另加一本");
        let tmp = paths(&t).state_dir().join("books").join("import-tmp");
        assert_eq!(std::fs::read_dir(tmp).unwrap().count(), 0, "临时文件用完就删");
        assert!(st.staging.list().is_empty(), "不进母版库");
    }

    /// xochitl 连不上：202 照回，任务 failed 带 message。
    #[test]
    fn import_new_upload_failure_is_reported_on_the_job() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        std::fs::create_dir_all(st.xochitl.library_dir()).unwrap();
        let a = crate::staging::tests::mini_epub(&[]);
        let v = post_and_wait(&router, "name=a.epub", &a);
        assert_eq!((v["state"].as_str(), v["stage"].as_str()), (Some("failed"), Some("失败")), "{v}");
        assert!(msg(&v).contains("上传给 xochitl 失败"), "{v}");
        assert!(v.get("uuid").is_none());
    }

    /// `GET /import/{uuid}` 的 folder 是从书库根起的完整路径。
    #[test]
    fn import_query_reports_full_folder_path() {
        const U: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        const TOP: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        const IN: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let lib = st.xochitl.library_dir().to_path_buf();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{TOP}.metadata")), r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{IN}.metadata")), format!(r#"{{"type":"CollectionType","visibleName":"死亡筆記(愛藏版)","parent":"{TOP}"}}"#)).unwrap();
        std::fs::write(lib.join(format!("{U}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"书","parent":"{IN}"}}"#)).unwrap();
        let (code, v) = call(&router, Method::Get, &format!("/import/{U}"), "");
        assert_eq!((code, v), (200, serde_json::json!({"uuid": U, "name": "书", "folder": "漫画/死亡筆記(愛藏版)", "deleted": false, "replacing": false})));
    }

    /// `GET /mkdir/pending`：`items` 带上级（空串＝根）；不再有给旧代理的 `names`。
    #[test]
    fn mkdir_pending_returns_items_with_parent() {
        const P: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        assert_eq!(call(&router, Method::Post, "/mkdir/add", r#"{"name":"漫画/卷01"}"#).0, 200, "网页入口：名字里的 / 当普通字符，建在根");
        st.mkdir.add_in(P, "卷01").unwrap();
        let (code, v) = call(&router, Method::Get, "/mkdir/pending", "");
        assert_eq!(code, 200);
        assert_eq!(v, serde_json::json!({"items": [{"name": "漫画/卷01", "parent": ""}, {"name": "卷01", "parent": P}]}));
    }

    #[test]
    fn status_route_reports_ok_and_folders() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        let (code, v) = call(&router, Method::Get, "/status", "");
        assert_eq!(code, 200);
        assert_eq!(v["ok"], serde_json::json!(true));
        assert!(v["xochitlFolders"].is_array());
    }
}
