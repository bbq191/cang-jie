//! koreader-serve —— 书架·KOReader（loopback 8791）。
//! 路由（经网关前缀 `/api/koreader`）：`GET /status` · `POST /books?folder=`（multipart 多文件）·
//! `GET /books[?folder=]` · `GET /fonts` · `POST /fonts`（字体镜像，font-serve 调用）· `GET /dicts`。
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
        "books": k.list_dir(&k.books_dir(), &[]).len(),
        "fonts": k.list_dir(&k.fonts_dir(), &["ttf","otf","ttc"]).len(),
    })
}

/// multipart 多文件 → 指定目录，逐项回执（books/ 与 fonts/ 共用）。
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
        let res = if into == "fonts" {
            let dir = k.fonts_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| {
                let name = shelf_core::multipart::safe_basename(&fname, "font.ttf");
                let dest = dir.join(&name);
                std::fs::File::create(&dest).map_err(|e| e.to_string()).and_then(|mut f| std::io::copy(&mut part, &mut f).map_err(|e| e.to_string())).map(|bytes| koreader::FileItem { name, bytes })
            })
        } else {
            k.put_book(&folder, &fname, &mut part)
        };
        items.push(match res {
            Ok(it) => serde_json::json!({"file": it.name, "ok": true, "target": "koreader", "message": format!("已放入 KOReader {}/（{} 字节）", into, it.bytes)}),
            Err(e) => serde_json::json!({"file": fname, "ok": false, "target": "koreader", "message": e}),
        });
    }
    let ok = !items.is_empty() && items.iter().all(|i| i["ok"].as_bool().unwrap_or(false));
    Ok(Reply::ok(&serde_json::json!({"ok": ok, "items": items, "note": if k.running() {"KOReader 正在运行：新书需在其文件浏览器里刷新"} else {""}})))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let k = Arc::new(KoReader::new(paths.koreader_root()));
    let (k1, k2, k3, k4, k5, k6) = (k.clone(), k.clone(), k.clone(), k.clone(), k.clone(), k.clone());
    let router = Router::new()
        .get("/status", move |_| Ok(Reply::ok(&status(&k1))))
        .post("/books", move |r| receive(&k2, r, "books", &[]))
        .get("/books", move |r| {
            let dir = k3.subdir(r.q("folder").unwrap_or("")).map_err(ApiError::bad)?;
            Ok(Reply::ok(&serde_json::json!({"items": k3.list_dir(&dir, &[])})))
        })
        .get("/fonts", move |_| Ok(Reply::ok(&serde_json::json!({"items": k4.list_dir(&k4.fonts_dir(), &["ttf","otf","ttc"])}))))
        .post("/fonts", move |r| receive(&k5, r, "fonts", &["ttf", "otf", "ttc"]))
        .get("/dicts", move |_| Ok(Reply::ok(&serde_json::json!({"items": k6.list_dir(&k6.dict_dir(), &["ifo"])}))));
    println!("[koreader-serve] root={} installed={}", k.root().display(), k.installed());
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[koreader-serve] {e}");
        std::process::exit(1);
    }
}
