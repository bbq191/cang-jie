//! HTTP 适配层（唯一碰 http 类型的地方，只做取参 + 调领域方法 + 回执）。路由（经网关时前缀 `/api/books`）：
//! `GET /status`
//! 母版库：`GET /staging` → `{items, freeBytes}` · `POST /staging`（multipart，原样入库）
//! · `POST /staging/deliver {name, folder?}`（2026-09-19 起投完永远保留母版，不再有 `keep` 参数；`POST /staging/mark` 2026-10-07 删，
//!   原调用方是已删的网关批量「加入 KOReader」）
//! · `POST /staging/delete {name}` · `POST /staging/rename {name, newName}` · `GET /staging/file?name=`（原件下载，流式）
//! · 2026-10-07 删除：`/staging/optimize`、`/staging/fetch-article`、`/staging/originals/*`、`/staging/cancel`（书架不再优化书，
//!   优化全部在电脑上用 sheng-ren 做；剩下的投递没有能中途停的步骤）。
//! · `GET /events`（SSE：母版库/inbox 变更即推，网页零轮询）。
//! 原生回收站队列：`POST /trash/add {uuid, name}`（name 必须与书库 visibleName 相符）· `GET /trash/pending?wait=` → `{uuids}`（MainView 代理 qmd 长轮询拉取执行）· `GET /trash`（只供调试：ssh 上 curl 看队列）。
//! 原生建文件夹队列：`POST /mkdir/add {name}`（建在书库根）· `GET /mkdir/pending?wait=` → `{names, items: [{name, parent}]}`（MainView 代理
//! shelf-mkdir-agent.qmd 长轮询拉取执行；`items` 带上级文件夹 uuid，`names` 只放建在根的、给旧代理兼容）· `GET /mkdir`（只供调试）。
//! 代理放弃记录：`GET /agent-failures` → `{items:[{kind,name,uuid?,at}]}` · `POST /agent-failures/clear`（两个队列交满次数仍没做成的项）。
//! 2026-09-05 起规则统一"所有书只落母版库"：旧 `POST /?target=` 直投路已删（`/staging*` 是网页的唯一入口）。
//! 直接导入 xochitl、不进母版库（2026-10-07，电脑上的 sheng-ren `booklib sync` 经 SSH 端口转发直连 8790 调，见 import.rs）：
//! · `POST /import?name=<文件名.epub>&folder=<文件夹路径，`/` 分多级，可空＝书库根>`（请求体＝EPUB 原始字节）→ `{uuid, name, folder}`
//!   （同步，可达分钟级；文件夹逐级找、没有就建，`folder` 回实际落进的完整路径）
//! · `POST /import?uuid=<uuid>&name=<文件名.epub>`：原地替换已有文档内容、uuid 不变 → `{uuid, name, folder}`；文档不在 / 已删 / 在回收站 / 不是 EPUB → 404
//! · `GET /import/{uuid}` → `{uuid, name, folder, deleted}`（不存在 → 404）。删除用 `POST /trash/add {uuid, name}`。
use crate::service_state::State;
use crate::staging::StagingStore;
use rmsvc_core::asset::{self, AssetUploadFlow};
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
        .get("/staging", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.staging.list(), "freeBytes": s.staging.free_bytes()})))))
        // 原件下载：边读边发（大书上百 MB，不整本读进内存）；网关见到 Content-Disposition 也原样流式转发。
        .get("/staging/file", bind(&st, |s, r| {
            let name = r.q("name").ok_or_else(|| ApiError::bad("缺少 name"))?.to_string();
            let (f, len) = s.staging.open_for_download(&name).map_err(ApiError::bad)?;
            let ctype = shelf_conv::direct_content_type(&name).map(|c| c.mime()).unwrap_or("application/octet-stream");
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
            // folder 空＝书库根；非空且真不存在会先经 mkdir 队列建出来再投，见 ensure_folder。
            s.staging.spawn_deliver(&name, j.str_or("folder", ""), s.mkdir.clone(), s.bus.clone()).map_err(ApiError::bad)?;
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
        // ── 原生书库回收站队列（真正的软删由 xochitl 自己的 selectionMoveToTrash 执行，见 trash.rs / shelf-trash-agent.qmd）──
        .post("/trash/add", bind(&st, |s, r| {
            let j = r.json()?;
            let n = s.trash.add(j.str("uuid")?, j.str("name")?).map_err(ApiError::bad)?;
            s.bus.publish("books", "trash");
            Ok(Reply::ok(&serde_json::json!({"ok": true, "pending": n, "message": "已排队：书库视图下次有动静时移进回收站"})))
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
            // `items` 带上级文件夹（2026-10-07 按层建）；`names` 只放建在根的，给还没更新的旧代理用——旧代理一律建在根，
            // 把子文件夹的名字交给它会建错地方，所以不放进去（那几项交满次数后放弃）。
            let names: Vec<&str> = items.iter().filter(|i| i.parent.is_empty()).map(|i| i.name.as_str()).collect();
            Ok(Reply::ok(&serde_json::json!({"names": names, "items": items})))
        }))
        .get("/mkdir", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.mkdir.list()})))))
        // ── 代理执行不成、已放弃的记录（网页页头横幅；「知道了」→ clear），见 agent_failures.rs ──
        .get("/agent-failures", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.agent_failures.list()})))))
        .post("/agent-failures/clear", bind(&st, |s, _| {
            let n = s.agent_failures.clear().map_err(ApiError::internal)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "cleared": n})))
        }))
        // ── 直接导入 xochitl（不进母版库，见 import.rs）：同步处理，回执时书已在书库里 ──
        .post("/import", bind(&st, import_book))
        .get("/import/{uuid}", bind(&st, |s, r| match s.import.describe(r.param("uuid")) {
            Some(d) => Ok(Reply::ok(&serde_json::json!({"uuid": d.uuid, "name": d.name, "folder": d.folder, "deleted": d.deleted}))),
            None => Err(ApiError::not_found("xochitl 书库里没有这份文档")),
        }))
        .post("/staging/delete", bind(&st, |s, r| {
            s.staging.remove(r.json()?.str("name")?).map_err(ApiError::bad)?;
            staging_changed(s)
        }))
}

/// 两个代理队列长轮询的 `?wait=<秒>`：缺省/非法＝0（立即返回），上限 [`AGENT_WAIT_MAX_SECS`]。
fn agent_wait(r: &Request<'_>) -> std::time::Duration {
    std::time::Duration::from_secs(r.q("wait").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0).min(AGENT_WAIT_MAX_SECS))
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
    if items.iter().any(|i| i.ok) {
        st.bus.publish("books", "staging");
    }
    Ok(Reply::ok(&asset::receipt(&items, serde_json::Value::Null)))
}

/// `POST /import`：有 `uuid` → 原地替换，没有 → 新导入。请求体是 EPUB 原始字节（流式落盘，不进内存）。
fn import_book(st: &State, r: &mut Request<'_>) -> ApiResult {
    use crate::import::ImportError;
    let name = r.q("name").ok_or_else(|| ApiError::bad("缺少 name"))?.to_string();
    let len = r.content_length;
    let res = match r.q("uuid").map(str::to_string) {
        Some(uuid) => st.import.replace(&uuid, &name, &mut *r.body, len),
        None => {
            let folder = r.q("folder").unwrap_or("").to_string();
            st.import.import_new(&name, &folder, &mut *r.body, len, &st.mkdir)
        }
    };
    let d = res.map_err(|e| match e {
        ImportError::Bad(m) => ApiError::bad(m),
        ImportError::NotFound(m) => ApiError::not_found(m),
        ImportError::Failed(m) => ApiError::internal(m),
    })?;
    st.invalidate_status(); // 可能新建了文件夹
    st.bus.publish("books", "import");
    Ok(Reply::ok(&serde_json::json!({"uuid": d.uuid, "name": d.name, "folder": d.folder})))
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
        let lib = Paths::resolve({
            let h = t.path().to_str().unwrap().to_string();
            move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None }
        })
        .xochitl_dir();
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

    /// 直接导入的路由：错误码映射（非 epub 400、文档不在 / 已删 404）、原地替换保留 uuid、查询。
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
        assert_eq!(post_raw(&router, &format!("uuid={U}&name=a.epub"), epub).0, 404, "文档不在 → 404，客户端改成新导入");
        assert_eq!(call(&router, Method::Get, &format!("/import/{U}"), "").0, 404);
        std::fs::write(lib.join(format!("{U}.metadata")), r#"{"type":"DocumentType","visibleName":"书","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{U}.epub")), b"PK\x03\x04old").unwrap();
        std::fs::write(lib.join(format!("{U}.pdf")), b"cache").unwrap();
        let (code, v) = post_raw(&router, &format!("uuid={U}&name=a.epub"), epub);
        assert_eq!(code, 200, "{v}");
        assert_eq!(v, serde_json::json!({"uuid": U, "name": "书", "folder": ""}));
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), epub);
        assert!(!lib.join(format!("{U}.pdf")).exists());
        let (code, v) = call(&router, Method::Get, &format!("/import/{U}"), "");
        assert_eq!((code, v), (200, serde_json::json!({"uuid": U, "name": "书", "folder": "", "deleted": false})));
        std::fs::write(lib.join(format!("{U}.metadata")), r#"{"type":"DocumentType","visibleName":"书","parent":"trash"}"#).unwrap();
        assert_eq!(call(&router, Method::Get, &format!("/import/{U}"), "").1["deleted"], true, "进了回收站算 deleted");
        assert_eq!(post_raw(&router, &format!("uuid={U}&name=a.epub"), epub).0, 404, "回收站里的不替换");
        assert!(st.staging.list().is_empty(), "不进母版库");
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
        assert_eq!((code, v), (200, serde_json::json!({"uuid": U, "name": "书", "folder": "漫画/死亡筆記(愛藏版)", "deleted": false})));
    }

    /// `GET /mkdir/pending`：`items` 带上级，`names` 只放建在根的（旧代理一律建在根，不能把子文件夹交给它）。
    #[test]
    fn mkdir_pending_returns_items_and_root_only_names() {
        const P: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        assert_eq!(call(&router, Method::Post, "/mkdir/add", r#"{"name":"漫画/卷01"}"#).0, 200, "网页入口：名字里的 / 当普通字符，建在根");
        st.mkdir.add_in(P, "卷01").unwrap();
        let (code, v) = call(&router, Method::Get, "/mkdir/pending", "");
        assert_eq!(code, 200);
        assert_eq!(v, serde_json::json!({"names": ["漫画/卷01"], "items": [{"name": "漫画/卷01", "parent": ""}, {"name": "卷01", "parent": P}]}));
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
