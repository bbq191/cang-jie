//! 注入 xochitl 文档库 —— 移植自 protocol/inject.py。
//! 免重启走 USB Web UI POST /upload（EPUB=application/epub+zip）。也可直写四件套（需重启感知）。

use serde_json::json;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub const XOCHITL_DIR: &str = "/home/root/.local/share/remarkable/xochitl";
pub const DEFAULT_FONT_NAME: &str = "LXGW WenKai";

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

pub fn build_metadata(title: &str) -> String {
    let now = now_ms().to_string();
    json!({
        "createdTime": now, "lastModified": now, "lastOpened": "0", "lastOpenedPage": 0,
        "new": true, "parent": "", "pinned": false,
        "source": "com.cangjie.weread", "type": "DocumentType", "visibleName": title,
    })
    .to_string()
}

pub fn build_content(title: &str, authors: &[String], publisher: &str, language: &str, font_name: &str) -> String {
    let mut meta = json!({"authors": authors, "title": title, "language": language});
    if !publisher.is_empty() {
        meta["publisher"] = json!(publisher);
    }
    json!({
        "fileType": "epub", "formatVersion": 2, "documentMetadata": meta, "extraMetadata": {},
        "fontName": font_name, "lineHeight": 100, "margins": 56, "orientation": "portrait",
        "pageCount": 0, "coverPageNumber": 0, "textScale": 1, "textAlignment": "justify",
        "tags": [], "pageTags": [],
    })
    .to_string()
}

/// 直写四件套到设备文档库（需重启 xochitl 感知）。返回 uuid。
pub fn write_document_files(
    epub: &[u8], title: &str, authors: &[String], publisher: &str, language: &str, font_name: &str,
) -> Result<String, String> {
    let u = uuid::Uuid::new_v4().to_string();
    let dir = XOCHITL_DIR;
    std::fs::write(format!("{dir}/{u}.epub"), epub).map_err(|e| e.to_string())?;
    std::fs::write(format!("{dir}/{u}.metadata"), build_metadata(title)).map_err(|e| e.to_string())?;
    std::fs::write(
        format!("{dir}/{u}.content"),
        build_content(title, authors, publisher, language, font_name),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(format!("{dir}/{u}.local"), json!({"contentFormatVersion": 2}).to_string())
        .map_err(|e| e.to_string())?;
    Ok(u)
}

/// b 方案「就地重渲」：覆盖 `<uuid>.epub` + 删派生件（`.pdf`/`.epubindex`/`.thumbnails`）。
/// 保留 `.metadata`(含 lastOpenedPage 进度)/`.content`/`.local`/`.pagedata` → xochitl 下次打开
/// 发现派生件缺失即从新 .epub 重渲（真机验证：内容更新+同文件跳转生效+进度保留，零临时/零幽灵/零重启）。
/// ⚠ 只改目标书自己的文件，绝不 rm 别的文档（运行时 rm 文档会造幽灵库条目）。
pub fn update_epub_inplace(uuid: &str, epub: &[u8]) -> Result<(), String> {
    let dir = XOCHITL_DIR;
    std::fs::write(format!("{dir}/{uuid}.epub"), epub).map_err(|e| format!("写 epub: {e}"))?;
    let _ = std::fs::remove_file(format!("{dir}/{uuid}.pdf"));
    let _ = std::fs::remove_file(format!("{dir}/{uuid}.epubindex"));
    let _ = std::fs::remove_dir_all(format!("{dir}/{uuid}.thumbnails"));
    Ok(())
}

/// 免重启：POST http://<host>/upload（multipart，字段 file）。返回响应体文本。
pub fn upload_epub(agent: &ureq::Agent, host: &str, epub: &[u8], filename: &str) -> Result<String, String> {
    upload_document(agent, host, epub, filename, "application/epub+zip")
}

/// 通用 /upload：multipart 字段名 file。EPUB=application/epub+zip；.rmdoc=application/zip。
pub fn upload_document(agent: &ureq::Agent, host: &str, data: &[u8], filename: &str, content_type: &str) -> Result<String, String> {
    let boundary = format!("----cangjie{}", uuid::Uuid::new_v4().simple());
    let mut body: Vec<u8> = Vec::with_capacity(data.len() + 256);
    write!(
        body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .map_err(|e| e.to_string())?;
    body.extend_from_slice(data);
    write!(body, "\r\n--{boundary}--\r\n").map_err(|e| e.to_string())?;

    let url = format!("http://{host}/upload");
    let resp = agent
        .post(&url)
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .send_bytes(&body);
    match resp {
        Ok(r) => r.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(c, r)) => Err(format!("HTTP {c}: {}", r.into_string().unwrap_or_default())),
        Err(e) => Err(format!("上传失败: {e}")),
    }
}
