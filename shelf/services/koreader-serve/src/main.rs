//! koreader-serve —— 书架·KOReader（loopback 8791）。
//! 路由（经网关前缀 `/api/koreader`）：`GET /status` · `POST /books?folder=`（multipart 多文件）·
//! `GET /books[?folder=]` · `GET /fonts` · `POST /fonts`（直传 KOReader 字体；font-serve 镜像也走它）· `DELETE /fonts/{file}` · `GET /dicts` · `POST /dicts?name=`
//! `GET /config/{settings|defaults|gestures}`（原文）· `POST /config/{file}?dry_run=1`（body=补丁 Lua；运行中拒写）。
mod config;
mod koreader;

use koreader::KoReader;
use shelf_core::http::{ApiError, ApiResult, Reply, Request, Router};
use shelf_core::multipart::{boundary_of, MultipartReader};
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;

const SPEC: ServiceSpec = ServiceSpec {
    name: "koreader-serve",
    label: "KOReader",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8791",
    tab: Some(("KOReader", 20)),
};

fn status(k: &KoReader) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "installed": k.installed(),
        "running": k.running(),
        "version": k.version(),
        "root": k.root(),
        "booksDir": k.books_dir(),
        "books": k.list_books("").map(|v| v.iter().filter(|e| e.kind == "file").count()).unwrap_or(0),
        "fonts": k.list_dir(&k.fonts_dir(), &["ttf","otf","ttc"]).len(),
        "dicts": list_dicts(k).len(),
    })
}

/// multipart 多文件 → books/[folder]（.part→rename，KOReader 扫目录不见半成品）。
fn receive(k: &KoReader, r: &mut Request<'_>, into: &str, allow: &[&str]) -> ApiResult {
    if !k.installed() {
        return Err(ApiError { status: 409, message: "KOReader 未安装（appload 目录不存在）".into() });
    }
    let Some(boundary) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
    let folder = r.q("folder").unwrap_or("").to_string();
    let mut mp = MultipartReader::new(&mut *r.body, &boundary);
    let mut items = Vec::new();
    loop {
        let mut part = match mp.next_part() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => return Err(ApiError::bad(format!("multipart 解析失败: {e}"))),
        };
        let Some(fname) = part.filename.clone() else { continue };
        let ext = fname.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !allow.is_empty() && !allow.contains(&ext.as_str()) {
            items.push(serde_json::json!({"file": fname, "ok": false, "message": format!("不支持的扩展名（允许：{}）", allow.join("/"))}));
            continue;
        }
        let res = k.put_book(&folder, &fname, &mut part);
        items.push(match res {
            Ok(it) => serde_json::json!({"file": it.name, "ok": true, "target": "koreader", "message": format!("已放入 KOReader {}/（{} 字节）", into, it.bytes)}),
            Err(e) => serde_json::json!({"file": fname, "ok": false, "target": "koreader", "message": e}),
        });
    }
    let ok = !items.is_empty() && items.iter().all(|i| i["ok"].as_bool().unwrap_or(false));
    Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items, "note": if k.running() {"KOReader 正在运行：新书需在其文件浏览器里刷新"} else {""}})))
}

/// multipart 多文件原字节落到 `dir`（词典/字体共用；books 走 put_book 的 .part→rename）。
fn receive_into(k: &KoReader, r: &mut Request<'_>, dir: &std::path::Path, allow: &[&str]) -> ApiResult {
    if !k.installed() {
        return Err(ApiError { status: 409, message: "KOReader 未安装（appload 目录不存在）".into() });
    }
    let Some(boundary) = boundary_of(&r.content_type) else { return Err(ApiError::bad("需要 multipart/form-data")) };
    std::fs::create_dir_all(dir).map_err(|e| ApiError::internal(e.to_string()))?;
    let mut mp = MultipartReader::new(&mut *r.body, &boundary);
    let mut items = Vec::new();
    loop {
        let mut part = match mp.next_part() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(e) => return Err(ApiError::bad(format!("multipart 解析失败: {e}"))),
        };
        let Some(fname) = part.filename.clone() else { continue };
        let name = shelf_core::multipart::safe_basename(&fname, "file.bin");
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !allow.is_empty() && !allow.contains(&ext.as_str()) {
            items.push(serde_json::json!({"file": name, "ok": false, "message": format!("不支持的扩展名（允许：{}）", allow.join("/"))}));
            continue;
        }
        let dest = dir.join(&name);
        let res = std::fs::File::create(&dest).map_err(|e| e.to_string()).and_then(|mut f| std::io::copy(&mut part, &mut f).map_err(|e| e.to_string()));
        items.push(match res {
            Ok(bytes) => serde_json::json!({"file": name, "ok": true, "message": format!("已放入 {}（{} 字节）", dir.display(), bytes)}),
            Err(e) => serde_json::json!({"file": name, "ok": false, "message": e}),
        });
    }
    let ok = !items.is_empty() && items.iter().all(|i| i["ok"].as_bool().unwrap_or(false));
    Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items})))
}

/// data/dict/ 下每个子目录=一本词典（有 .ifo 才算）。
fn list_dicts(k: &KoReader) -> Vec<serde_json::Value> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(k.dict_dir()) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                let n = k.list_dir(&e.path(), &["ifo"]).len();
                if n > 0 {
                    v.push(serde_json::json!({"name": e.file_name().to_string_lossy(), "ifo": n}));
                }
            }
        }
    }
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let k = Arc::new(KoReader::new(paths.koreader_root()));
    let (k1, k2, k3, k4, k5, k6) = (k.clone(), k.clone(), k.clone(), k.clone(), k.clone(), k.clone());
    let (k7, k8, k9) = (k.clone(), k.clone(), k.clone());
    let backup_dir = paths.state_dir().join("koreader-backups");
    let tmp_dir = paths.runtime_dir().join("koreader");
    let router = Router::new()
        .get("/status", move |_| Ok(Reply::ok(&status(&k1))))
        .post("/books", move |r| receive(&k2, r, "books", &[]))
        .get("/books", move |r| {
            let folder = r.q("folder").unwrap_or("").trim_matches('/').to_string();
            let items = k3.list_books(&folder).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"folder": folder, "items": items})))
        })
        .get("/fonts", move |_| {
            let dir = k4.fonts_dir();
            let items: Vec<serde_json::Value> = k4.list_dir(&dir, &["ttf", "otf", "ttc"]).into_iter().map(|it| {
                // 中文基本区覆盖率（同原生字体一致的判据），低覆盖当正文会缺字
                let pct = std::fs::read(dir.join(&it.name)).ok().and_then(|b| shelf_core::ttf::han_coverage_pct(&b)).unwrap_or(0);
                serde_json::json!({"name": it.name, "bytes": it.bytes, "cjkPct": pct})
            }).collect();
            Ok(Reply::ok(&serde_json::json!({"items": items})))
        })
        .post("/fonts", move |r| receive_into(&k5, r, &k5.fonts_dir(), &["ttf", "otf", "ttc"]))
        .delete("/fonts/{file}", {
            let k = k.clone();
            move |r| {
                let name = shelf_core::multipart::safe_basename(r.param("file"), "");
                if name.is_empty() || name.starts_with('.') {
                    return Err(ApiError::bad("非法文件名"));
                }
                let p = k.fonts_dir().join(&name);
                if !p.is_file() {
                    return Err(ApiError::not_found("KOReader fonts/ 里没有这个文件"));
                }
                std::fs::remove_file(&p).map_err(|e| ApiError::internal(format!("删除失败: {e}")))?;
                Ok(Reply::ok(&serde_json::json!({"ok": true, "note": if k.running() {"KOReader 运行中：重启它后字体列表才更新"} else {""}})))
            }
        })
        .get("/dicts", move |_| Ok(Reply::ok(&serde_json::json!({"items": list_dicts(&k6)}))))
        .post("/dicts", move |r| {
            let name = r.q("name").unwrap_or("").trim().to_string();
            if name.is_empty() || name.contains('/') || name.contains("..") {
                return Err(ApiError::bad("需要 ?name=<词典目录名>（单层）"));
            }
            receive_into(&k7, r, &k7.dict_dir().join(&name), &["ifo", "idx", "dict", "dz", "syn", "oft"])
        })
        .get("/config/{file}", move |r| {
            let cs = config::ConfigSync { ko: &k8, backup_dir: backup_dir.clone(), tmp_dir: tmp_dir.clone() };
            let text = cs.read(r.param("file")).map_err(ApiError::bad)?;
            Ok(Reply::bytes("text/plain; charset=utf-8", text.into_bytes()))
        })
        .post("/config/{file}", {
            let backup_dir = paths.state_dir().join("koreader-backups");
            let tmp_dir = paths.runtime_dir().join("koreader");
            move |r| {
                let file = r.param("file").to_string();
                let dry = r.q("dry_run").map(|v| v == "1" || v == "true").unwrap_or(false);
                let patch = String::from_utf8(r.read_small_body().map_err(ApiError::bad)?).map_err(|_| ApiError::bad("补丁不是 UTF-8"))?;
                let cs = config::ConfigSync { ko: &k9, backup_dir: backup_dir.clone(), tmp_dir: tmp_dir.clone() };
                let res = cs.apply(&file, &patch, dry).map_err(|e| ApiError { status: if e.contains("正在运行") { 409 } else { 400 }, message: e })?;
                Ok(Reply::ok(&res))
            }
        });
    println!("[koreader-serve] root={} installed={}", k.root().display(), k.installed());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[koreader-serve] {e}");
        std::process::exit(1);
    }
}
