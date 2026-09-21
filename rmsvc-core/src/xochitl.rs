//! 原生 xochitl 书库免重启注入。**剥离移植**自旧项目 device-core `inject.rs` 的真机验证结论
//! （书架不引用旧 crate，此处独立实现）：
//! - `POST http://<host>/upload`（multipart 字段 `file`）免重启进库；xochitl web 只绑 USB 网口，
//!   设备端靠 lo/usb1 别名让 `10.11.99.1` 常驻可达。
//! - **GET-then-upload 归档**：`GET /documents/<folder-uuid>` 设"当前文件夹"是全局服务端状态，
//!   之后的 `/upload` 落进该文件夹（metadata.parent 会被忽略）。
//! - **防复制风暴**：大书 `/upload` 处理慢 → 408/读超时但文档已创建，此类错误**绝不重试**。
use std::io::{Cursor, Read, Write};
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

    /// 给定文档 uuid，查它当前所在的设备文件夹 uuid（空串＝根）；查不到／在回收站 → `None`。
    pub fn parent_folder(&self, uuid: &str) -> Option<String> {
        parent_folder_of(&self.library_dir, uuid)
    }

    /// 在 `folder` 范围内给 `base_name` 去重，撞名就加数字后缀。
    pub fn unique_name(&self, folder: &str, base_name: &str) -> String {
        unique_document_name(&self.library_dir, folder, base_name)
    }

    /// 书库目录（`<uuid>.{metadata,content,epub,pdf}` 所在）。
    pub fn library_dir(&self) -> &Path {
        &self.library_dir
    }

    fn set_folder(&self, folder_uuid: &str) -> bool {
        let path = if folder_uuid.is_empty() { "documents/".to_string() } else { format!("documents/{folder_uuid}") };
        self.agent.get(&format!("http://{}/{}", self.host, path)).call().is_ok()
    }

    /// 上传进指定名字的文件夹（找不到→书库根，best-effort）。数据已经在内存里（漫画拆分份、
    /// note-serve 笔记本 zip 这类合成产物）用这个；落地文件直接上传用 [`Self::upload_file`]，
    /// 别自己先 `fs::read` 整个再传进来。
    pub fn upload(&self, data: &[u8], filename: &str, content_type: &str, folder_name: &str) -> Result<Delivery, String> {
        self.upload_body(Cursor::new(data), data.len() as u64, filename, content_type, folder_name)
    }

    /// 直接流式上传一个磁盘文件——内容全程不整体读进内存，只在 `send_multipart` 里按块过一遍
    /// （2026-09-19 OOM 审计：`Staging::deliver()` 落库不拆分路径曾经 `fs::read` 整本＋这里内部
    /// 再克隆一份拼 multipart body，峰值能到原文件 2 倍+；改流式后这条路径不再囤整本字节）。
    pub fn upload_file(&self, path: &Path, filename: &str, content_type: &str, folder_name: &str) -> Result<Delivery, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("打开 {}: {e}", path.display()))?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        self.upload_body(std::io::BufReader::new(file), len, filename, content_type, folder_name)
    }

    /// **突破网页上传的体积上限**（xochitl `/upload` 约 100MB 硬限，超了直接断连）：先上传 `placeholder`
    /// （几 KB 的占位文档，EPUB 要带真书名和封面，见 `bookconv::placeholder`）让 xochitl 建好条目，再把磁盘上
    /// 那个文件原子替换成 `path` 的真文件。2026-09-20 真机验证：PDF 154MB/349 页、EPUB 153MB 都能打开。
    ///
    /// - **EPUB**：删掉占位的渲染缓存 `.pdf`/`.epubindex`，用户第一次打开时 xochitl 重新渲染（146MB 实测约 25s，
    ///   之后走缓存），页数、`sizeInBytes` 等届时自己更新。
    /// - **PDF**：`.content` 里有逐页 UUID 表/页数/大小，必须一并改成真文件的（`pdf_pages` 由调用方给），
    ///   否则只显示占位的页数。列表里的页数/大小要用户点开后才刷新（真机观察）。
    ///
    /// 只能在 xochitl 数据目录本机可写时用（book-serve 跑在设备上）。返回新文档 uuid。失败时占位文档可能
    /// 已留在书库里（回执里说明），不做危险的"回滚删除"。
    pub fn upload_large_file(&self, path: &Path, filename: &str, content_type: &str, folder_name: &str, placeholder: &[u8], pdf_pages: Option<usize>) -> Result<String, String> {
        let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if ext != "epub" && ext != "pdf" {
            return Err("只有 EPUB/PDF 能走大文件通道".into());
        }
        let dir = self.library_dir.clone();
        if !dir.is_dir() {
            return Err(format!("xochitl 书库目录不可写（{}），大文件通道只能在设备上用", dir.display()));
        }
        let want_len = std::fs::metadata(path).map_err(|e| format!("读 {} 失败: {e}", path.display()))?.len();
        let since = crate::clock::now_ms().saturating_sub(2_000);
        self.upload(placeholder, filename, content_type, folder_name)?;
        // 等 xochitl 建好条目（`.metadata` + 占位文件都落地），按占位字节数确认是"我们这一份"而不是别人同时传的。
        let mut uuid = None;
        for _ in 0..100 {
            if let Some(d) = find_documents_since(&dir, since).into_iter().find(|d| std::fs::metadata(dir.join(format!("{}.{ext}", d.uuid))).map(|m| m.len() == placeholder.len() as u64).unwrap_or(false)) {
                uuid = Some(d.uuid);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let uuid = uuid.ok_or("占位文档已上传，但 20 秒内没在书库里找到它（未替换成真文件）")?;
        let tmp = dir.join(format!("{uuid}.{ext}.new"));
        let dest = dir.join(format!("{uuid}.{ext}"));
        let fail = |e: String| {
            let _ = std::fs::remove_file(&tmp);
            format!("{e}（书库里可能留有占位文档「{filename}」，请手动删除）")
        };
        std::fs::copy(path, &tmp).map_err(|e| fail(format!("复制大文件失败: {e}")))?;
        let got = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if got != want_len {
            return Err(fail(format!("复制后大小不符（{got} ≠ {want_len}）")));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        if ext == "epub" {
            let _ = std::fs::remove_file(dir.join(format!("{uuid}.pdf")));
            let _ = std::fs::remove_file(dir.join(format!("{uuid}.epubindex")));
        } else if let Some(n) = pdf_pages {
            rewrite_pdf_content(&dir, &uuid, n, want_len).map_err(fail)?;
        }
        std::fs::rename(&tmp, &dest).map_err(|e| fail(format!("替换文件失败: {e}")))?;
        Ok(uuid)
    }

    fn upload_body(&self, body: impl Read, body_len: u64, filename: &str, content_type: &str, folder_name: &str) -> Result<Delivery, String> {
        let folder = if folder_name.is_empty() { String::new() } else { self.find_folder(folder_name).unwrap_or_default() };
        self.set_folder(&folder);
        match send_multipart(&self.agent, &self.host, body, body_len, filename, content_type) {
            Ok(resp) => Ok(Delivery::Delivered(resp)),
            Err(e) if upload_likely_delivered(&e) => Ok(Delivery::LikelyDelivered(e)),
            Err(e) => Err(e),
        }
    }
}

/// 书库目录里所有可解析的 `<uuid>.metadata` → (uuid, JSON)。只读；解析失败的跳过。
fn metadata_entries(dir: &Path) -> Vec<(String, serde_json::Value)> {
    metadata_entries_since(dir, None)
}

/// 同 [`metadata_entries`]，`min_mtime` 给定时**只打开 mtime 不早于它的**：先用目录项自带的 stat 挡掉旧文件，
/// 不再对整个书库（几十上百份）逐个 open+读+解析 JSON——渲染自检/占位等待这类"找刚进库的那本"的调用会在
/// 一个 3 秒防抖 / 200ms 轮询循环里反复扫描（2026-09-22 审计）。
fn metadata_entries_since(dir: &Path, min_mtime: Option<std::time::SystemTime>) -> Vec<(String, serde_json::Value)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                return None;
            }
            if let Some(floor) = min_mtime {
                if e.metadata().ok()?.modified().ok()? < floor {
                    return None;
                }
            }
            let uuid = p.file_stem()?.to_str()?.to_string();
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some((uuid, v))
        })
        .collect()
}

fn str_of<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

/// 非回收站、未删除的条目（文件夹与文档共用的过滤）。
fn is_live(v: &serde_json::Value) -> bool {
    str_of(v, "parent") != "trash" && v.get("deleted").and_then(|x| x.as_bool()) != Some(true)
}

/// 是不是 xochitl 文档 uuid 的形状（36 字符，只含十六进制与 `-`）。拿来当文件名片段之前先过一遍，
/// 防路径注入（`../`）；只看形状，不代表书库里真有这份文档。
pub fn is_uuid_shape(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn find_folder_by_name(dir: &Path, name: &str) -> Option<String> {
    metadata_entries(dir).into_iter().find(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v) && str_of(v, "visibleName") == name).map(|(uuid, _)| uuid)
}

/// 原生书库里所有活文件夹的名字（去重、按名排序）——给网页「加入原生书库 → 文件夹」下拉候选用，
/// 跟 koreader-serve 给 KOReader 目录下拉候选同一个道理：反映设备上**真实存在**的文件夹，不是
/// 写死的预设列表（2026-09-19 用户反馈：原来的「书库/批注/自定义」三选一预设看不出真实文件夹，
/// 批注那档还常年跟书库撞成一样，见书架白皮书对应记录）。
pub fn list_folders(dir: &Path) -> Vec<String> {
    metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v))
        .map(|(_, v)| str_of(&v, "visibleName").to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 给定一份文档的 uuid，读它 `.metadata` 的 `parent` 字段——就是它当前所在的设备文件夹 uuid
/// （空串＝书库根）。找不到 `.metadata`、解析失败、或书在回收站（`parent=="trash"`），一律返回
/// `None`，调用方按 best-effort 落书库根处理（2026-09-09 补：`note-serve` 生成章节笔记本时不再
/// 新建/确保文件夹，改成直接复用书本自己已经在的文件夹）。
pub fn parent_folder_of(dir: &Path, uuid: &str) -> Option<String> {
    let t = std::fs::read_to_string(dir.join(format!("{uuid}.metadata"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    let parent = v.get("parent").and_then(|x| x.as_str())?;
    (parent != "trash").then(|| parent.to_string())
}

/// 在 `folder`（文件夹 uuid，空串＝根）范围内，如果 `base_name` 已经被别的活文档占用，就在末尾加
/// 数字后缀（`"标题"` → `"标题 2"` → `"标题 3"` ...）直到不冲突；没冲突就原样返回。只读 `.metadata`，
/// 不写、不建任何东西。
pub fn unique_document_name(dir: &Path, folder: &str, base_name: &str) -> String {
    let names: std::collections::HashSet<String> = metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "DocumentType" && is_live(v) && str_of(v, "parent") == folder)
        .map(|(_, v)| str_of(&v, "visibleName").to_string())
        .collect();
    if !names.contains(base_name) {
        return base_name.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base_name} {n}");
        if !names.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// 书库里一份文档（非文件夹、非回收站）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    pub uuid: String,
    pub visible_name: String,
    /// xochitl `createdTime`（毫秒字符串）。
    pub created_ms: u64,
}

/// `createdTime >= since_ms` 的文档，新→旧。投原生后找"刚进库的那本"用（`/upload` 不回 uuid；visibleName
/// 取自 EPUB 元数据不等于文件名，所以按时间圈候选、再按书名挑）。只读 `.metadata`，不写。
pub fn find_documents_since(dir: &Path, since_ms: u64) -> Vec<DocInfo> {
    // `createdTime >= since` 的文档，其 `.metadata` 一定是创建时或之后写的，mtime 不会更早（留 5 秒余量给文件系统
    // 时间戳粒度/时钟取整）；`since_ms == 0`（补记全库）不设下限。
    let floor = (since_ms > 0).then(|| std::time::UNIX_EPOCH + std::time::Duration::from_millis(since_ms.saturating_sub(5_000)));
    let mut out: Vec<DocInfo> = metadata_entries_since(dir, floor)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "DocumentType" && is_live(v))
        .filter_map(|(uuid, v)| {
            let created_ms = v.get("createdTime").and_then(|x| x.as_str().and_then(|s| s.parse::<u64>().ok()).or_else(|| x.as_u64())).unwrap_or(0);
            (created_ms >= since_ms).then(|| DocInfo { uuid, visible_name: str_of(&v, "visibleName").to_string(), created_ms })
        })
        .collect();
    out.sort_by_key(|d| std::cmp::Reverse(d.created_ms));
    out
}

/// 把占位 PDF 的 `.content` 改成真 PDF 的：页数、逐页 UUID 表、`redirectionPageMap`、`sizeInBytes`。
fn rewrite_pdf_content(dir: &Path, uuid: &str, pages: usize, size: u64) -> Result<(), String> {
    let path = dir.join(format!("{uuid}.content"));
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| format!("读 .content 失败: {e}"))?).map_err(|e| format!(".content 不是合法 JSON: {e}"))?;
    let obj = v.as_object_mut().ok_or(".content 不是对象")?;
    obj.insert("pageCount".into(), pages.into());
    obj.insert("originalPageCount".into(), pages.into());
    obj.insert("pages".into(), (0..pages).map(|_| serde_json::Value::String(uuid::Uuid::new_v4().to_string())).collect::<Vec<_>>().into());
    obj.insert("redirectionPageMap".into(), (0..pages).collect::<Vec<_>>().into());
    obj.insert("sizeInBytes".into(), size.to_string().into());
    let tmp = dir.join(format!("{uuid}.content.new"));
    std::fs::write(&tmp, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?).map_err(|e| format!("写 .content 失败: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换 .content 失败: {e}"))
}

/// `<uuid>.content` 的 `pageCount`：xochitl 渲染完（导入 / 打开）才写；缺或 0 → None。
pub fn page_count(dir: &Path, uuid: &str) -> Option<u64> {
    let t = std::fs::read_to_string(dir.join(format!("{uuid}.content"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    v.get("pageCount").and_then(|x| x.as_u64()).filter(|&n| n > 0)
}

/// 流式发一份 multipart `/upload` 请求：`body`（文件内容，长度已知 `body_len`）不整体缓冲，
/// 用 `Cursor(头).chain(body).chain(Cursor(尾))` 直接喂给 `ureq`；显式给 `Content-Length` 让
/// `ureq` 按已知长度发送而不是退化成 chunked（`ureq::Request::send` 文档：调用方可设
/// `Content-Length`，设了就不用 chunked）——线上字节序列跟改动前逐字节相同，只是不再囤在一个
/// `Vec<u8>` 里。
fn send_multipart(agent: &ureq::Agent, host: &str, body: impl Read, body_len: u64, filename: &str, content_type: &str) -> Result<String, String> {
    let boundary = format!("----shelf{}", uuid::Uuid::new_v4().simple());
    let mut header = Vec::new();
    write!(header, "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let footer = format!("\r\n--{boundary}--\r\n").into_bytes();
    let total_len = header.len() as u64 + body_len + footer.len() as u64;
    let reader = Cursor::new(header).chain(body).chain(Cursor::new(footer));
    let resp = agent
        .post(&format!("http://{host}/upload"))
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .set("Content-Length", &total_len.to_string())
        .send(reader);
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

#[cfg(test)]
mod tests {
    #[test]
    fn uuid_shape_accepts_real_uuid_rejects_traversal() {
        assert!(is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(!is_uuid_shape("../../etc/passwd"));
        assert!(!is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef012345678"), "少一位");
        assert!(!is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef012345678g"), "非十六进制");
    }

    /// 假 xochitl：`GET /documents/..` 回 200；`POST /upload` 解出 multipart 里的文件部分，落成
    /// `<uuid>.{ext}` + `.metadata`（+ EPUB 的渲染缓存 `.pdf`/`.epubindex` 与 PDF 的 `.content`），回 201。
    fn fake_xochitl(lib: std::path::PathBuf) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                if req.method() == &tiny_http::Method::Post {
                    let mut body = Vec::new();
                    std::io::Read::read_to_end(req.as_reader(), &mut body).unwrap();
                    let start = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                    let head = String::from_utf8_lossy(&body[..start]).to_string();
                    let fname = head.split("filename=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
                    let end = body.windows(2).rposition(|w| w == b"\r\n").map(|_| body.len()).unwrap();
                    let tail = body.windows(4).rposition(|w| w == b"\r\n--").unwrap_or(end);
                    let file = &body[start..tail];
                    let uuid = uuid::Uuid::new_v4().to_string();
                    let ext = fname.rsplit('.').next().unwrap();
                    std::fs::write(lib.join(format!("{uuid}.{ext}")), file).unwrap();
                    std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{fname}","parent":"","createdTime":"{}"}}"#, crate::clock::now_ms())).unwrap();
                    if ext == "epub" {
                        std::fs::write(lib.join(format!("{uuid}.pdf")), b"render-cache").unwrap();
                        std::fs::write(lib.join(format!("{uuid}.epubindex")), b"idx").unwrap();
                    } else {
                        std::fs::write(lib.join(format!("{uuid}.content")), r#"{"fileType":"pdf","pageCount":1,"pages":["x"],"redirectionPageMap":[0],"sizeInBytes":"5"}"#).unwrap();
                    }
                    let _ = req.respond(tiny_http::Response::from_string(r#"{"status":"Upload successful"}"#).with_status_code(201));
                } else {
                    let _ = req.respond(tiny_http::Response::from_string("[]"));
                }
            }
        });
        addr
    }

    #[test]
    fn upload_large_file_swaps_epub_and_drops_render_cache() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let big = lib.path().join("big-source.bin");
        let big_bytes = vec![7u8; 300_000];
        std::fs::write(&big, &big_bytes).unwrap();
        let uuid = x.upload_large_file(&big, "书 - 02卷.epub", "application/epub+zip", "", b"PLACEHOLDER-EPUB", None).unwrap();
        assert_eq!(std::fs::read(lib.path().join(format!("{uuid}.epub"))).unwrap(), big_bytes, "占位必须被真文件替换");
        assert!(!lib.path().join(format!("{uuid}.pdf")).exists(), "占位的渲染缓存必须删掉，让 xochitl 重新渲染");
        assert!(!lib.path().join(format!("{uuid}.epubindex")).exists());
        assert!(!lib.path().join(format!("{uuid}.epub.new")).exists(), "不留临时文件");
    }

    #[test]
    fn upload_large_file_pdf_rewrites_content_pages() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let big = lib.path().join("big-source.bin");
        std::fs::write(&big, vec![9u8; 123_456]).unwrap();
        let uuid = x.upload_large_file(&big, "漫画.pdf", "application/pdf", "", b"%PDF-placeholder", Some(349)).unwrap();
        assert_eq!(std::fs::metadata(lib.path().join(format!("{uuid}.pdf"))).unwrap().len(), 123_456);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(lib.path().join(format!("{uuid}.content"))).unwrap()).unwrap();
        assert_eq!(v["pageCount"], 349);
        assert_eq!(v["pages"].as_array().unwrap().len(), 349);
        assert_eq!(v["sizeInBytes"], "123456");
    }

    #[test]
    fn upload_large_file_rejects_other_formats_and_missing_library() {
        let lib = tempfile::tempdir().unwrap();
        let x = Xochitl::new("127.0.0.1:1", lib.path(), 1);
        assert!(x.upload_large_file(Path::new("/x"), "a.cbz", "x", "", b"p", None).unwrap_err().contains("EPUB/PDF"));
        let y = Xochitl::new("127.0.0.1:1", Path::new("/nonexistent-lib"), 1);
        assert!(y.upload_large_file(Path::new("/x"), "a.epub", "x", "", b"p", None).unwrap_err().contains("只能在设备上用"));
    }

    #[test]
    fn rewrite_pdf_content_updates_pages_map_and_size_keeping_other_fields() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("u1.content"),
            r#"{"fileType":"pdf","pageCount":3,"originalPageCount":3,"pages":["a","b","c"],"redirectionPageMap":[0,1,2],"sizeInBytes":"99","zoomMode":"bestFit"}"#,
        )
        .unwrap();
        rewrite_pdf_content(d.path(), "u1", 349, 154_367_634).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.path().join("u1.content")).unwrap()).unwrap();
        assert_eq!(v["pageCount"], 349);
        assert_eq!(v["originalPageCount"], 349);
        assert_eq!(v["pages"].as_array().unwrap().len(), 349);
        assert_eq!(v["redirectionPageMap"].as_array().unwrap().len(), 349);
        assert_eq!(v["redirectionPageMap"][348], 348);
        assert_eq!(v["sizeInBytes"], "154367634");
        assert_eq!(v["zoomMode"], "bestFit", "其它字段必须原样保留");
        let ids: std::collections::HashSet<_> = v["pages"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        assert_eq!(ids.len(), 349, "逐页 UUID 必须互不相同");
    }

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
    fn lists_folders_deduped_sorted_skipping_trash_and_documents() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"CollectionType","visibleName":"雪人","parent":""}"#);
        w("b.metadata", r#"{"type":"CollectionType","visibleName":"批注","parent":""}"#);
        w("c.metadata", r#"{"type":"CollectionType","visibleName":"批注","parent":""}"#); // 同名文件夹去重
        w("d.metadata", r#"{"type":"CollectionType","visibleName":"回收站里的","parent":"trash"}"#);
        w("e.metadata", r#"{"type":"DocumentType","visibleName":"这是本书不是文件夹","parent":""}"#);
        assert_eq!(list_folders(t.path()), vec!["批注".to_string(), "雪人".to_string()]);
    }

    #[test]
    fn parent_folder_of_reads_parent_field_and_treats_trash_as_none() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("book-in-folder.metadata", r#"{"type":"DocumentType","visibleName":"人骨拼图","parent":"folder-uuid"}"#);
        w("book-at-root.metadata", r#"{"type":"DocumentType","visibleName":"飘","parent":""}"#);
        w("book-in-trash.metadata", r#"{"type":"DocumentType","visibleName":"删了","parent":"trash"}"#);
        assert_eq!(parent_folder_of(t.path(), "book-in-folder"), Some("folder-uuid".into()));
        assert_eq!(parent_folder_of(t.path(), "book-at-root"), Some(String::new()), "根目录是空串，不是 None");
        assert_eq!(parent_folder_of(t.path(), "book-in-trash"), None, "书在回收站，别把笔记也生成进去");
        assert_eq!(parent_folder_of(t.path(), "no-such-uuid"), None, "查不到就 None，调用方 best-effort 落根");
    }

    #[test]
    fn unique_document_name_appends_suffix_only_within_same_folder() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"DocumentType","visibleName":"楔子","parent":"f1"}"#);
        w("b.metadata", r#"{"type":"DocumentType","visibleName":"楔子 2","parent":"f1"}"#);
        w("c.metadata", r#"{"type":"DocumentType","visibleName":"楔子","parent":"f2"}"#);
        w("trashed.metadata", r#"{"type":"DocumentType","visibleName":"楔子 3","parent":"trash"}"#);
        assert_eq!(unique_document_name(t.path(), "f1", "楔子"), "楔子 3", "f1 下已有「楔子」和「楔子 2」（后者活着占用），下一个该是 3");
        assert_eq!(unique_document_name(t.path(), "f2", "楔子"), "楔子 2", "f2 只有一份同名，跟 f1 的计数互不影响");
        assert_eq!(unique_document_name(t.path(), "f3", "楔子"), "楔子", "f3 没有同名文档，原样返回");
        assert_eq!(unique_document_name(t.path(), "trash", "楔子 3"), "楔子 3", "回收站里的同名文档不算占用（is_live 过滤掉）");
    }

    #[test]
    fn finds_documents_since_newest_first_and_reads_page_count() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("old.metadata", r#"{"type":"DocumentType","visibleName":"旧书","parent":"","createdTime":"1000"}"#);
        w("new.metadata", r#"{"type":"DocumentType","visibleName":"New Book","parent":"","createdTime":"3000"}"#);
        w("mid.metadata", r#"{"type":"DocumentType","visibleName":"Mid","parent":"","createdTime":"2000"}"#);
        w("tr.metadata", r#"{"type":"DocumentType","visibleName":"Trash","parent":"trash","createdTime":"5000"}"#);
        w("del.metadata", r#"{"type":"DocumentType","visibleName":"Del","parent":"","deleted":true,"createdTime":"5000"}"#);
        w("dir.metadata", r#"{"type":"CollectionType","visibleName":"Folder","parent":"","createdTime":"5000"}"#);
        w("new.content", r#"{"pageCount": 352, "fileType": "epub"}"#);
        w("mid.content", r#"{"pageCount": 0}"#);
        let docs = find_documents_since(t.path(), 2000);
        assert_eq!(docs.iter().map(|d| d.uuid.as_str()).collect::<Vec<_>>(), ["new", "mid"]);
        assert_eq!(docs[0].visible_name, "New Book");
        assert_eq!(page_count(t.path(), "new"), Some(352));
        assert_eq!(page_count(t.path(), "mid"), None, "0 页＝还没渲染");
        assert_eq!(page_count(t.path(), "old"), None, "没有 .content");
    }

    /// `find_documents_since` 用 mtime 下限先挡旧文件：结果语义不变（仍以 createdTime 为准），只是不再打开旧文件。
    #[test]
    fn find_documents_since_skips_stale_metadata_by_mtime_but_keeps_result_semantics() {
        let t = tempfile::tempdir().unwrap();
        let now = crate::clock::now_ms();
        let w = |n: &str, created: u64, age_secs: u64| {
            let p = t.path().join(n);
            std::fs::write(&p, format!(r#"{{"type":"DocumentType","visibleName":"{n}","parent":"","createdTime":"{created}"}}"#)).unwrap();
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs)).unwrap();
        };
        w("fresh.metadata", now, 0);
        w("shelved.metadata", now - 3_600_000, 3_600); // 一小时前进库、没再动过
        w("touched.metadata", now - 3_600_000, 0); // 老书但刚被 xochitl 改写过 .metadata：过 mtime 门，被 createdTime 挡掉
        let got: Vec<String> = find_documents_since(t.path(), now - 1_000).into_iter().map(|d| d.uuid).collect();
        assert_eq!(got, ["fresh"]);
        let all: Vec<String> = find_documents_since(t.path(), 0).into_iter().map(|d| d.uuid).collect();
        assert_eq!(all.len(), 3, "since=0（补记全库）不设 mtime 下限");
    }
}

/// 2026-09-19 OOM 审计：`upload`/`upload_file` 改流式发送体后，线上字节序列应该跟改动前的
/// "整块 Vec 拼 body" 写法完全一致，只是不再整块囤内存。起一个最小 HTTP mock（GET 任意路径回
/// 200 空体，POST /upload 把收到的原始 body 送回 channel）验证这一点——不需要真连 xochitl。
#[cfg(test)]
mod upload_tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// 强制每个请求 `Connection: close`（下一个请求另起连接），mock 逻辑不用管 keep-alive 复用；
    /// `upload`/`upload_file` 先各发一次 `set_folder` 的 GET 再发 POST /upload，遇到 POST 就把
    /// body 送回 channel 并停止收连接。
    fn mock_xochitl() -> (String, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    break;
                }
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        content_length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; content_length];
                reader.read_exact(&mut body).unwrap();
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
                let is_post = request_line.starts_with("POST");
                if is_post {
                    let _ = tx.send(body);
                    break;
                }
            }
        });
        (format!("127.0.0.1:{}", addr.port()), rx)
    }

    fn assert_well_formed_upload(body: &[u8], data: &[u8], filename: &str, content_type: &str) {
        let text = String::from_utf8_lossy(body);
        assert!(text.contains(&format!("filename=\"{filename}\"")), "{text}");
        assert!(text.contains(&format!("Content-Type: {content_type}")), "{text}");
        assert!(body.windows(data.len().max(1)).any(|w| w == data), "body 应该原样包含完整数据");
        assert!(text.trim_end().ends_with("--"), "multipart 尾部 boundary 收尾要完整");
    }

    #[test]
    fn upload_streams_in_memory_data_as_well_formed_multipart() {
        let t = tempfile::tempdir().unwrap();
        let (host, rx) = mock_xochitl();
        let x = Xochitl::new(&host, t.path(), 5);
        let data = b"hello epub bytes, this is the whole book content".to_vec();
        let r = x.upload(&data, "book.epub", "application/epub+zip", "");
        assert!(matches!(r, Ok(Delivery::Delivered(_))), "{r:?}");
        let body = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_well_formed_upload(&body, &data, "book.epub", "application/epub+zip");
    }

    #[test]
    fn upload_file_streams_disk_file_as_well_formed_multipart() {
        let t = tempfile::tempdir().unwrap();
        let data = b"streamed straight from disk, never buffered whole in memory".to_vec();
        let path = t.path().join("src.epub");
        std::fs::write(&path, &data).unwrap();
        let (host, rx) = mock_xochitl();
        let x = Xochitl::new(&host, t.path(), 5);
        let r = x.upload_file(&path, "book.epub", "application/epub+zip", "");
        assert!(matches!(r, Ok(Delivery::Delivered(_))), "{r:?}");
        let body = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_well_formed_upload(&body, &data, "book.epub", "application/epub+zip");
    }
}
