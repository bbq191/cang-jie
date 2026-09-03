//! 原生 xochitl 书库免重启注入。**剥离移植**自旧项目 device-core `inject.rs` 的真机验证结论
//! （书架不引用旧 crate，此处独立实现）：
//! - `POST http://<host>/upload`（multipart 字段 `file`）免重启进库；xochitl web 只绑 USB 网口，
//!   设备端靠 lo/usb1 别名让 `10.11.99.1` 常驻可达。
//! - **GET-then-upload 归档**：`GET /documents/<folder-uuid>` 设"当前文件夹"是全局服务端状态，
//!   之后的 `/upload` 落进该文件夹（metadata.parent 会被忽略）。
//! - **防复制风暴**：大书 `/upload` 处理慢 → 408/读超时但文档已创建，此类错误**绝不重试**。
use std::io::Write;
use std::path::Path;

pub const DEFAULT_HOST: &str = "10.11.99.1";

/// 上传结果：`Delivered`=确认成功；`LikelyDelivered`=超时但很可能已创建（别重试）。
#[derive(Debug, PartialEq)]
pub enum Delivery {
    Delivered(String),
    LikelyDelivered(String),
}

pub struct Xochitl {
    agent: ureq::Agent,
    host: String,
    library_dir: std::path::PathBuf,
}

impl Xochitl {
    /// `library_dir`=书库目录（用于按名找文件夹）；`timeout_secs` 建议 300（大书）。
    pub fn new(host: &str, library_dir: &Path, timeout_secs: u64) -> Xochitl {
        // 连接 10s 即判"未送达"（:80 没绑/USB 未就绪，可安全重试）；整体 timeout 给大书处理留足（超时但已送达
        // 由 upload_likely_delivered 识别、绝不重试）。
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build();
        Xochitl { agent, host: host.to_string(), library_dir: library_dir.to_path_buf() }
    }

    /// `/upload` 是否可达（不真上传，GET 根页）。
    pub fn reachable(&self) -> bool {
        self.agent.get(&format!("http://{}/", self.host)).timeout(std::time::Duration::from_secs(3)).call().is_ok()
    }

    /// 按 visibleName 找非回收站文件夹 uuid。
    pub fn find_folder(&self, name: &str) -> Option<String> {
        find_folder_by_name(&self.library_dir, name)
    }

    fn set_folder(&self, folder_uuid: &str) -> bool {
        let path = if folder_uuid.is_empty() { "documents/".to_string() } else { format!("documents/{folder_uuid}") };
        self.agent.get(&format!("http://{}/{}", self.host, path)).call().is_ok()
    }

    /// 上传进指定名字的文件夹（找不到→书库根，best-effort）。
    pub fn upload(&self, data: &[u8], filename: &str, content_type: &str, folder_name: &str) -> Result<Delivery, String> {
        let folder = if folder_name.is_empty() { String::new() } else { self.find_folder(folder_name).unwrap_or_default() };
        self.set_folder(&folder);
        match upload_document(&self.agent, &self.host, data, filename, content_type) {
            Ok(body) => Ok(Delivery::Delivered(body)),
            Err(e) if upload_likely_delivered(&e) => Ok(Delivery::LikelyDelivered(e)),
            Err(e) => Err(e),
        }
    }
}

pub fn find_folder_by_name(dir: &Path, name: &str) -> Option<String> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
            continue;
        }
        let v: serde_json::Value = match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            Some(v) => v,
            None => continue,
        };
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

fn upload_document(agent: &ureq::Agent, host: &str, data: &[u8], filename: &str, content_type: &str) -> Result<String, String> {
    let boundary = format!("----shelf{}", uuid::Uuid::new_v4().simple());
    let mut body: Vec<u8> = Vec::with_capacity(data.len() + 256);
    write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n")
        .map_err(|e| e.to_string())?;
    body.extend_from_slice(data);
    write!(body, "\r\n--{boundary}--\r\n").map_err(|e| e.to_string())?;
    let resp = agent
        .post(&format!("http://{host}/upload"))
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .send_bytes(&body);
    match resp {
        Ok(r) => r.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(c, r)) => Err(format!("HTTP {c}: {}", r.into_string().unwrap_or_default())),
        Err(e) => Err(format!("上传失败: {e}")),
    }
}

/// 错误是否属于"很可能已送达"（408/读超时且非连接阶段）。
pub fn upload_likely_delivered(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    (e.contains("408") || e.contains("timed out") || e.contains("timeout")) && !e.contains("connect")
}

/// 按扩展名给 xochitl 可直读格式的 MIME。
pub fn content_type_of(filename: &str) -> Option<&'static str> {
    let l = filename.to_ascii_lowercase();
    if l.ends_with(".epub") {
        Some("application/epub+zip")
    } else if l.ends_with(".pdf") {
        Some("application/pdf")
    } else if l.ends_with(".rmdoc") {
        Some("application/zip")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_upload_errors() {
        assert!(upload_likely_delivered("HTTP 408: 408 request timeout"));
        assert!(upload_likely_delivered("上传失败: timed out reading response"));
        assert!(!upload_likely_delivered("上传失败: Connection refused (os error 111)"));
        assert!(!upload_likely_delivered("上传失败: connect timed out"));
    }

    #[test]
    fn finds_folder_skipping_trash_and_documents() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"CollectionType","visibleName":"library","parent":"trash"}"#);
        w("b.metadata", r#"{"type":"DocumentType","visibleName":"library","parent":""}"#);
        w("c.metadata", r#"{"type":"CollectionType","visibleName":"library","parent":""}"#);
        w("d.content", r#"{}"#);
        assert_eq!(find_folder_by_name(t.path(), "library"), Some("c".into()));
        assert_eq!(find_folder_by_name(t.path(), "none"), None);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type_of("A.EPUB"), Some("application/epub+zip"));
        assert_eq!(content_type_of("x.pdf"), Some("application/pdf"));
        assert_eq!(content_type_of("x.azw3"), None);
    }
}
