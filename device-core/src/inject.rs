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

/// 按名认领库内文件夹 uuid（CollectionType + visibleName==name + 非 trash）。多个同名取首个；无 → None。
/// 用于自动归档：daemon 把 zettelkasten、wr-serve 把 library 的 uuid 解析出来传给 `set_upload_folder`。
pub fn find_folder_by_name(dir: &str, name: &str) -> Option<String> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
            continue;
        }
        let v: serde_json::Value = std::fs::read_to_string(&p)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null);
        if v.get("type").and_then(|x| x.as_str()) != Some("CollectionType") {
            continue;
        }
        if v.get("parent").and_then(|x| x.as_str()) == Some("trash") {
            continue;
        }
        if v.get("visibleName").and_then(|x| x.as_str()) == Some(name) {
            return p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
        }
    }
    None
}

/// 设「当前文件夹」上下文：`GET /documents/<folder>`（folder 空 → `/documents/` 回根）。
/// 真机验证：这是**全局服务端状态**，此后的 `/upload` 落进该文件夹（reMarkable web UI 就这么归档的，
/// 见 §四 命门翻案——metadata parent 被忽略，但 GET-then-upload 有效）。best-effort，失败返回 false
/// （调用方可继续按 root 传，不致命）。
pub fn set_upload_folder(agent: &ureq::Agent, host: &str, folder: &str) -> bool {
    let path = if folder.is_empty() { "documents/".to_string() } else { format!("documents/{folder}") };
    agent.get(&format!("http://{host}/{path}")).call().is_ok()
}

/// 组合助手：把文档上传进指定名字的文件夹（`find_folder_by_name`→`set_upload_folder`→`upload_document`
/// 一步到位）。folder_name 找不到 → 落书库根（best-effort，不致命）。复用方：wr-serve 自动优化 / 多格式
/// 转换摄入都往同名 library 落，避免各写一遍这套 find/set/upload 三步。
pub fn upload_to_folder(
    agent: &ureq::Agent,
    host: &str,
    xochitl_dir: &str,
    data: &[u8],
    filename: &str,
    content_type: &str,
    folder_name: &str,
) -> Result<String, String> {
    let folder = find_folder_by_name(xochitl_dir, folder_name).unwrap_or_default();
    set_upload_folder(agent, host, &folder);
    upload_document(agent, host, data, filename, content_type)
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

/// 上传返回错误时，判断请求**是否很可能已送达并被 xochitl 创建**。
/// 大书 /upload：xochitl 收完整请求后处理很慢 → 回 `408 request timeout` 或客户端读响应超时，
/// **但文档实际已创建**。此时**绝不能重试**（每重试一次多一本 → 真机「飘」复制风暴）。
/// 反之「连接被拒/连接错误」= :80 没绑、请求没送达，文档未创建（这类才可安全重试）。
/// true=已送达(当成功、别重试)；false=连接层失败(未创建)。
pub fn upload_likely_delivered(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    (e.contains("408") || e.contains("timed out") || e.contains("timeout"))
        && !e.contains("connect")
}

#[cfg(test)]
mod delivered_tests {
    use super::upload_likely_delivered;
    #[test]
    fn classifies_upload_errors() {
        // 已送达（别重试）：408 / 读响应超时
        assert!(upload_likely_delivered("HTTP 408: 408 request timeout"));
        assert!(upload_likely_delivered(
            "上传失败: Network Error: Error encountered in the status line: timed out reading response"
        ));
        // 未送达（可能重试/终态失败）：连接被拒 / 连接错误
        assert!(!upload_likely_delivered(
            "上传失败: http://10.11.99.1/upload: Connection Failed: Connect error: Connection refused (os error 111)"
        ));
        assert!(!upload_likely_delivered("上传失败: connect timed out")); // 连接阶段超时=未送达
    }
}
