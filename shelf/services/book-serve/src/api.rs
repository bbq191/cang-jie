//! HTTP 适配层（唯一碰 http 类型的地方，只做取参 + 调领域方法 + 回执）。路由（经网关时前缀 `/api/books`）：
//! `GET /status`
//! 母版库：`GET /staging` → `{items, freeBytes}` · `POST /staging`（multipart，原样入库）
//! · `POST /staging/deliver {name, folder?}`（2026-09-19 起投完永远保留母版，不再有 `keep` 参数）· `POST /staging/mark {name, target?}`（target 只剩 native，可省略；koreader 已删）
//! · `POST /staging/delete {name}` · `POST /staging/rename {name, newName}` · `GET /staging/file?name=`（原件下载，流式）
//! · 2026-10-07 删除：`/staging/optimize`、`/staging/fetch-article`、`/staging/originals/*`、`/staging/cancel`（书架不再优化书，
//!   优化全部在电脑上用 sheng-ren 做；剩下的投递没有能中途停的步骤）。
//! · `GET /events`（SSE：母版库/inbox 变更即推，网页零轮询）。
//! 阅读方向：`GET /reading-direction/{uuid}` → `{rtl}`（xochitl 里 reader-page-turn.qmd 用；只看书里自带的 OPF 标记，2026-09-30 起不能在网页上按书指定）。
//! 原生回收站队列：`POST /trash/add {uuid, name}`（name 必须与书库 visibleName 相符）· `GET /trash/pending?wait=` → `{uuids}`（MainView 代理 qmd 长轮询拉取执行）· `GET /trash`。
//! 原生建文件夹队列：`POST /mkdir/add {name}` · `GET /mkdir/pending` → `{names}`（MainView 代理 shelf-mkdir-agent.qmd 拉取执行）· `GET /mkdir`。
//! 代理放弃记录：`GET /agent-failures` → `{items:[{kind,name,uuid?,at}]}` · `POST /agent-failures/clear`（两个队列交满次数仍没做成的项）。
//! 2026-09-05 起规则统一"所有书只落母版库"：旧 `POST /?target=` 直投路已删（`/staging*` 是唯一入口）。
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
            // folder 空＝配置缺省；非空且真不存在会先经 mkdir 队列建出来再投，见 ensure_folder。
            s.staging.spawn_deliver(&name, j.str_or("folder", ""), s.mkdir.clone(), s.bus.clone()).map_err(ApiError::bad)?;
            s.bus.publish("books", "staging"); // 立即推一次，UI 马上看到这条目进入 busy 状态
            Ok(Reply::ok(&serde_json::json!({"ok": true, "message": format!("《{name}》已开始投递，完成后自动刷新"), "async": true})))
        }))
        // 手动记一笔落库（加入 xochitl）。以前网关批量「加入 KOReader」在 koreader-serve adopt 完成后调这里记
        // `target=koreader`；KOReader 2026-09-29 从设备卸载、koreader-serve 源码 2026-09-30 从仓库删除（见 git 历史），
        // 去向只剩 native：`target` 可省略，传 `native` 仍接受（向后兼容），传别的（含 `koreader`）一律 400。
        .post("/staging/mark", bind(&st, |s, r| {
            let j = r.json()?;
            check_mark_target(j.str_or("target", "native")).map_err(ApiError::bad)?;
            s.staging.mark_delivered(j.str("name")?).map_err(ApiError::bad)?;
            staging_changed(s)
        }))
        // ── 阅读方向（reader-page-turn.qmd 打开书时查；rtl=从右往左翻页的书，见 reading_direction.rs）──
        .get("/reading-direction/{uuid}", bind(&st, |s, r| {
            let rtl = s.reading_direction.is_rtl(r.param("uuid")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"rtl": rtl})))
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
            let (names, pruned) = s.mkdir.pending_wait(agent_wait(r)).map_err(ApiError::internal)?;
            if pruned > 0 {
                s.bus.publish("books", "mkdir");
            }
            Ok(Reply::ok(&serde_json::json!({"names": names})))
        }))
        .get("/mkdir", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.mkdir.list()})))))
        // ── 代理执行不成、已放弃的记录（网页页头横幅；「知道了」→ clear），见 agent_failures.rs ──
        .get("/agent-failures", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"items": s.agent_failures.list()})))))
        .post("/agent-failures/clear", bind(&st, |s, _| {
            let n = s.agent_failures.clear().map_err(ApiError::internal)?;
            Ok(Reply::ok(&serde_json::json!({"ok": true, "cleared": n})))
        }))
        .post("/staging/delete", bind(&st, |s, r| {
            s.staging.remove(r.json()?.str("name")?).map_err(ApiError::bad)?;
            staging_changed(s)
        }))
}

/// `POST /staging/mark` 的 `target`：只剩 `native`（KOReader 去向已删，见路由处注释）。
fn check_mark_target(target: &str) -> Result<(), String> {
    match target {
        "native" => Ok(()),
        "koreader" => Err("KOReader 已不再支持（2026-09-29 从设备卸载），target 只能是 native".into()),
        _ => Err("target 只能是 native".into()),
    }
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
        assert!(st.staging.try_start_busy("x.epub"));
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
        st.staging.end_busy("x.epub");
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
    fn mark_and_unknown_routes() {
        let t = tempfile::tempdir().unwrap();
        let st = state(&t);
        let router = router(st.clone());
        st.staging.stage_new("m.epub", b"PK").unwrap();
        // KOReader 去向已删（2026-09-30）：传 koreader 报错、不落记录；native（或省略 target）照常记原生落库。
        let (code, v) = call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub","target":"koreader"}"#);
        assert_eq!(code, 400);
        assert!(msg(&v).contains("KOReader 已不再支持"), "{v}");
        assert!(st.staging.list()[0].delivered.is_none(), "报错时不记任何落库");
        let (code, v) = call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub","target":"kindle"}"#);
        assert_eq!(code, 400);
        assert!(msg(&v).contains("target 只能是 native"), "{v}");
        assert_eq!(call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub","target":"native"}"#).0, 200, "native 仍接受（向后兼容）");
        assert!(st.staging.list()[0].delivered.as_ref().unwrap().native.is_some(), "记了一笔原生落库");
        assert_eq!(call(&router, Method::Post, "/staging/mark", r#"{"name":"m.epub"}"#).0, 200, "target 可省略");
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
        std::thread::spawn(move || done_tx.send(st2.process_inbox(None)).unwrap());
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
