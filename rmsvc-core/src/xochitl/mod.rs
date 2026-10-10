//! 原生 xochitl 书库免重启注入。**剥离移植**自旧项目 device-core `inject.rs` 的真机验证结论
//! （书架不引用旧 crate，此处独立实现）：
//! - `POST http://<host>/upload`（multipart 字段 `file`）免重启进库；xochitl web 只绑 USB 网口，
//!   设备端靠 lo/usb1 别名让 `10.11.99.1` 常驻可达。
//! - **GET-then-upload 归档**：`GET /documents/<folder-uuid>` 设"当前文件夹"是全局服务端状态，
//!   之后的 `/upload` 落进该文件夹（metadata.parent 会被忽略）。因为是全局状态，"设文件夹 → 上传"用一把锁串成一对
//!   （进程内锁 + 运行时目录下锁文件的 flock，跨进程也互斥，见 `claim::UploadLock`）。
//! - **上传并认领**：`/upload` 不回 uuid，[`Xochitl::upload_and_claim`] 按快照 + 判据认出新文档，见 `claim` 子模块。
//! - **防复制风暴**：大书 `/upload` 处理慢 → 408/读超时但文档已创建，此类错误**绝不重试**。
use std::io::{Cursor, Read, Write};
use std::path::Path;

// 书库 `.metadata`/`.content` 的只读查询单独一个子模块（不碰 HTTP），对外路径不变。
mod library;
pub use library::*;
// 文件夹类型（根 / uuid）与上传并认领。
mod claim;
pub mod content;
mod folder;
pub use claim::{ClaimBy, ClaimError, ClaimWait, Claimed, UploadBody, LOCK_FILE_NAME};
pub use content::PageTable;
pub use folder::{Folder, FolderId};

pub const DEFAULT_HOST: &str = "10.11.99.1";

/// 大文件通道认领占位条目：书库目录静默这么久再查一次（xochitl 建条目时连写好几个文件，攒一下只查一次）。
const LARGE_CLAIM_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);
/// 大文件通道认领占位条目的最长等待（与改动前 100 × 200ms 相同）。
const LARGE_CLAIM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

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
    /// 跨进程上传锁文件（见 `claim::UploadLock`）；`None` 只做进程内互斥。
    upload_lock: Option<std::path::PathBuf>,
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
        let upload_lock = claim::default_lock_file(library_dir);
        Xochitl { agent, host: host.to_string(), library_dir: library_dir.to_path_buf(), upload_lock }
    }

    /// 改用（或关掉，`None`）跨进程上传锁文件。缺省：书库目录是本机真实 xochitl 书库时用运行时目录下的
    /// [`LOCK_FILE_NAME`]，否则不用（见 `claim::default_lock_file`）。
    pub fn with_upload_lock(mut self, lock_file: Option<std::path::PathBuf>) -> Xochitl {
        self.upload_lock = lock_file;
        self
    }

    /// 按 visibleName 在**整个书库**里找活文件夹（多级同名时有歧义，见 [`find_folder_by_name`]）。按层找用 [`Self::child_folder`]。
    pub fn folder_by_name(&self, name: &str) -> Option<FolderId> {
        find_folder_by_name(&self.library_dir, name)
    }

    /// 在 `parent` 正下方按名字找活文件夹，见 [`child_folder`]。
    pub fn child_folder(&self, parent: &Folder, name: &str) -> Option<FolderId> {
        child_folder(&self.library_dir, parent, name)
    }

    /// 文档 `uuid` 当前所在的文件夹；查不到 / 在回收站 → `None`。见 [`folder_of_document`]。
    pub fn folder_of_document(&self, uuid: &str) -> Option<Folder> {
        folder_of_document(&self.library_dir, uuid)
    }

    /// 只上传、不认领（不需要新文档 uuid 的场合）。要 uuid 用 [`Self::upload_and_claim`]。
    pub fn upload_to(&self, body: UploadBody<'_>, filename: &str, content_type: &str, folder: &Folder) -> Result<Delivery, String> {
        match body {
            UploadBody::File(path) => {
                let file = std::fs::File::open(path).map_err(|e| format!("打开 {}: {e}", path.display()))?;
                let len = file.metadata().map_err(|e| e.to_string())?.len();
                self.upload_body(std::io::BufReader::new(file), len, filename, content_type, folder)
            }
            UploadBody::Bytes(b) => self.upload_body(Cursor::new(b), b.len() as u64, filename, content_type, folder),
        }
    }

    /// 文件夹的完整路径（`漫画/死亡筆記(愛藏版)`；根＝空串），见 [`folder_path_of`]。
    pub fn folder_path(&self, folder: &Folder) -> String {
        folder_path_of(&self.library_dir, folder)
    }

    /// 在 `folder` 范围内给 `base_name` 去重，撞名就加数字后缀。
    pub fn unique_name(&self, folder: &Folder, base_name: &str) -> String {
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

    /// **突破网页上传的体积上限**（xochitl `/upload` 约 100MB 硬限，超了直接断连）：先上传 `placeholder`
    /// （几 KB 的占位文档，EPUB 要带真书名和封面，见 `shelf_conv::placeholder`）让 xochitl 建好条目，再把磁盘上
    /// 那个文件原子替换成 `path` 的真文件。2026-09-20 真机验证：PDF 154MB/349 页、EPUB 153MB 都能打开。
    ///
    /// - **EPUB**：删掉占位的渲染缓存 `.pdf`/`.epubindex`，用户第一次打开时 xochitl 重新渲染（146MB 实测约 25s，
    ///   之后走缓存），页数、`sizeInBytes` 等届时自己更新。
    /// - **PDF**：`.content` 里有逐页 UUID 表/页数/大小，必须一并改成真文件的（`pdf_pages` 由调用方给），
    ///   否则只显示占位的页数。列表里的页数/大小要用户点开后才刷新（真机观察）。
    ///
    /// 只能在 xochitl 数据目录本机可写时用（book-serve 跑在设备上）。返回新文档 uuid。失败时占位文档可能
    /// 已留在书库里（回执里说明），不做危险的"回滚删除"。
    pub fn upload_large(&self, path: &Path, filename: &str, content_type: &str, folder: &Folder, placeholder: &[u8], pdf_pages: Option<usize>) -> Result<String, String> {
        let ext = crate::formats::ext_of(filename);
        if ext != "epub" && ext != "pdf" {
            return Err("只有 EPUB/PDF 能走大文件通道".into());
        }
        let dir = self.library_dir.clone();
        if !dir.is_dir() {
            return Err(format!("xochitl 书库目录不可写（{}），大文件通道只能在设备上用", dir.display()));
        }
        let want_len = std::fs::metadata(path).map_err(|e| format!("读 {} 失败: {e}", path.display()))?.len();
        // 整个"传占位 → 按占位字节数认领 → 换成真文件"串行：认领只凭"刚进库 + 大小等于占位"，PDF 占位是同一份固定字节，
        // 两本大 PDF 同时走这条路时会认领到同一个 uuid——一本被覆盖进别人的条目、另一份占位永远留在书库里。锁要拿到
        // 替换完成：替换前那份文件仍是占位大小，后来者照样会认错（2026-09-25 第四轮审计）。
        static LARGE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _serial = crate::sync::lock(&LARGE);
        // 等 xochitl 建好条目（`.metadata` + 占位文件都落地），按占位字节数确认是"我们这一份"而不是别人同时传的
        // （快照之外的新文档里找，见 `claim` 子模块；等书库目录的 inotify 变化再查）。
        let wait = ClaimWait { debounce: LARGE_CLAIM_DEBOUNCE, ..ClaimWait::new(LARGE_CLAIM_TIMEOUT) };
        let uuid = match self.upload_and_claim(UploadBody::Bytes(placeholder), filename, content_type, folder, ClaimBy::SameSize, wait) {
            Ok(c) => c.uuid,
            Err(ClaimError::Upload(e)) => return Err(e),
            Err(ClaimError::NotFound { .. }) => return Err("占位文档已上传，但 20 秒内没在书库里找到它（未替换成真文件）".into()),
        };
        // 临时副本用 ScratchFile：出错 / panic 时自动删，不把几百 MB 的半成品留在书库目录里（隐藏名，xochitl 不当文档）。
        let tmp = crate::fs::ScratchFile::new(&dir, &format!(".{uuid}."), &format!("{ext}.new"));
        let dest = dir.join(format!("{uuid}.{ext}"));
        let fail = |e: String| format!("{e}（书库里可能留有占位文档「{filename}」，请手动删除）");
        std::fs::copy(path, tmp.path()).map_err(|e| fail(format!("复制大文件失败: {e}")))?;
        let got = std::fs::metadata(tmp.path()).map(|m| m.len()).unwrap_or(0);
        if got != want_len {
            return Err(fail(format!("复制后大小不符（{got} ≠ {want_len}）")));
        }
        crate::fs::set_mode(tmp.path(), 0o600);
        if ext == "epub" {
            let _ = std::fs::remove_file(dir.join(format!("{uuid}.pdf")));
            let _ = std::fs::remove_file(dir.join(format!("{uuid}.epubindex")));
        } else if let Some(n) = pdf_pages {
            rewrite_pdf_content(&dir, &uuid, n, want_len).map_err(fail)?;
        }
        std::fs::rename(tmp.path(), &dest).map_err(|e| fail(format!("替换文件失败: {e}")))?;
        Ok(uuid)
    }

    /// 设当前文件夹 → 流式 `/upload`。
    fn upload_body(&self, body: impl Read, body_len: u64, filename: &str, content_type: &str, folder: &Folder) -> Result<Delivery, String> {
        // "设当前文件夹 → /upload" 必须成对、不被打断：当前文件夹是 xochitl 服务端的**全局**状态，两次投递并发时
        // （网关允许 3 本小书同时处理）A 设完文件夹、B 又设了自己的，A 的书就落进 B 的文件夹。进程内所有
        // `Xochitl` 实例共用一把锁，再加运行时目录下锁文件的 flock 管跨进程（note-serve 也会投笔记本，2026-09-24
        // 审计记过它管不到；2026-10-10 补上）；只锁上传本身。
        let _serial = claim::UploadLock::acquire(self.upload_lock.as_deref());
        let folder_uuid = folder.as_parent_str();
        // 设文件夹失败（文件夹刚被删、xochitl 回错）时退回书库根：不退的话"当前文件夹"还是上一次投递设的那个，
        // 这本书会落进别人的文件夹，而文档承诺的是"找不到→书库根"（2026-10-09 第六轮审计）。
        if !self.set_folder(folder_uuid) && !folder_uuid.is_empty() {
            self.set_folder("");
        }
        match send_multipart(&self.agent, &self.host, body, body_len, filename, content_type) {
            Ok(resp) => Ok(Delivery::Delivered(resp)),
            Err(e) if e.likely_delivered() => Ok(Delivery::LikelyDelivered(e.message())),
            Err(e) => Err(e.message()),
        }
    }
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
    crate::fs::write_atomic(&path, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| format!("写 .content 失败: {e}"))
}

/// 流式发一份 multipart `/upload` 请求：`body`（文件内容，长度已知 `body_len`）不整体缓冲，
/// 用 `Cursor(头).chain(body).chain(Cursor(尾))` 直接喂给 `ureq`；显式给 `Content-Length` 让
/// `ureq` 按已知长度发送而不是退化成 chunked（`ureq::Request::send` 文档：调用方可设
/// `Content-Length`，设了就不用 chunked）——线上字节序列跟改动前逐字节相同，只是不再囤在一个
/// `Vec<u8>` 里。
fn send_multipart(agent: &ureq::Agent, host: &str, body: impl Read, body_len: u64, filename: &str, content_type: &str) -> Result<String, UploadError> {
    let boundary = format!("----shelf{}", uuid::Uuid::new_v4().simple());
    let mut header = Vec::new();
    let filename = header_safe_filename(filename);
    write!(header, "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n")
        .map_err(|e| UploadError::Other(e.to_string()))?;
    let footer = format!("\r\n--{boundary}--\r\n").into_bytes();
    let total_len = header.len() as u64 + body_len + footer.len() as u64;
    let reader = Cursor::new(header).chain(body).chain(Cursor::new(footer));
    let resp = agent
        .post(&format!("http://{host}/upload"))
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .set("Content-Length", &total_len.to_string())
        .send(reader);
    match resp {
        // 已经拿到 2xx：书一定进库了，读应答体出错（读超时、连接被断）也只是回执不全，按"很可能已送达"处理，绝不重试。
        Ok(r) => r.into_string().map_err(|e| UploadError::AfterSuccess(e.to_string())),
        Err(ureq::Error::Status(c, r)) => Err(UploadError::Status(c, r.into_string().unwrap_or_default())),
        Err(ureq::Error::Transport(t)) => Err(UploadError::classify_transport(&t)),
    }
}

/// `/upload` 失败的分类（按 `ureq::Error` 的种类判，不再把错误拼成字符串后找 `408`/`timeout` 子串——
/// xochitl 回 5xx 且响应体里带这些字样时会被误判成"已送达"、不重试，2026-10-10 审计 CORE-8）。
#[derive(Debug, PartialEq)]
enum UploadError {
    /// xochitl 回了非 2xx：状态码 + 响应体。408 = 它自己处理超时（大书），文档已建好。
    Status(u16, String),
    /// 连接阶段失败（DNS / 连不上 / 连接超时）：请求根本没送到，可安全重试。
    Connect(String),
    /// 连上之后读写超时：请求已经送到，xochitl 处理大书慢——很可能已创建文档。
    Timeout(String),
    /// 已收到 2xx，读应答体时出错。
    AfterSuccess(String),
    /// 其它（连接中途被断、应答格式坏……）：不确定是否送达，按失败处理（与改动前一致）。
    Other(String),
}

impl UploadError {
    fn classify_transport(t: &ureq::Transport) -> UploadError {
        let msg = format!("上传失败: {t}");
        match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::InvalidUrl | ureq::ErrorKind::UnknownScheme => UploadError::Connect(msg),
            ureq::ErrorKind::Io if Self::is_timeout(t) => UploadError::Timeout(msg),
            _ => UploadError::Other(msg),
        }
    }

    /// 传输错误的根源是 I/O 超时（ureq 把读超时的 WouldBlock 归一成 TimedOut；写超时同样是 TimedOut/WouldBlock）。
    fn is_timeout(t: &ureq::Transport) -> bool {
        let mut cur: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(t);
        while let Some(e) = cur {
            if let Some(io) = e.downcast_ref::<std::io::Error>() {
                return matches!(io.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock);
            }
            cur = e.source();
        }
        false
    }

    /// 是否属于"很可能已送达"（防复制风暴：这类错误绝不重试）。
    fn likely_delivered(&self) -> bool {
        matches!(self, UploadError::Status(408, _) | UploadError::Timeout(_) | UploadError::AfterSuccess(_))
    }

    /// 给人看的文案（与改动前的字符串格式一致：`HTTP <码>: <体>` / `上传失败: <原因>`）。
    fn message(self) -> String {
        match self {
            UploadError::Status(c, body) => format!("HTTP {c}: {body}"),
            UploadError::Connect(m) | UploadError::Timeout(m) | UploadError::Other(m) => m,
            UploadError::AfterSuccess(m) => format!("已送达，读回执失败: {m}"),
        }
    }
}

/// multipart 头里的 `filename="…"` 不能含 `"` 与 CR/LF：母版库文件名只校验"单段"（`fs::plain_name`），
/// 带引号的书名（`他说"好".pdf`）此前原样拼进头里，xochitl 解析到第一个 `"` 就截断，书库里显示成半截名；
/// 带换行则直接把后面的内容当成新的头。`"` 换成 `'`、CR/LF 换成空格，其余字节原样（中文照旧直传，
/// 与改动前一致——xochitl 按 UTF-8 读这个字段，真机一直这么传）。
fn header_safe_filename(name: &str) -> String {
    name.chars().map(|c| match c {
        '"' => '\'',
        '\r' | '\n' => ' ',
        c => c,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 假 xochitl：`GET /documents/..` 回 200；`POST /upload` 解出 multipart 里的文件部分，落成
    /// `<uuid>.{ext}` + `.metadata`（+ EPUB 的渲染缓存 `.pdf`/`.epubindex` 与 PDF 的 `.content`），回 201。
    fn fake_xochitl(lib: std::path::PathBuf) -> String {
        fake_xochitl_delayed(lib, std::time::Duration::ZERO)
    }

    /// 同 [`fake_xochitl`]，但条目在回应之后 `delay` 才落盘（真 xochitl 导入是异步的，回完 `/upload` 过一会才出现 `.metadata`）。
    fn fake_xochitl_delayed(lib: std::path::PathBuf, delay: std::time::Duration) -> String {
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
                    let ext = fname.rsplit('.').next().unwrap().to_string();
                    let (file, lib, created) = (file.to_vec(), lib.clone(), crate::clock::now_ms());
                    let materialize = move || {
                        std::fs::write(lib.join(format!("{uuid}.{ext}")), file).unwrap();
                        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{fname}","parent":"","createdTime":"{created}"}}"#)).unwrap();
                        if ext == "epub" {
                            std::fs::write(lib.join(format!("{uuid}.pdf")), b"render-cache").unwrap();
                            std::fs::write(lib.join(format!("{uuid}.epubindex")), b"idx").unwrap();
                        } else {
                            std::fs::write(lib.join(format!("{uuid}.content")), r#"{"fileType":"pdf","pageCount":1,"pages":["x"],"redirectionPageMap":[0],"sizeInBytes":"5"}"#).unwrap();
                        }
                    };
                    if delay.is_zero() {
                        materialize();
                    } else {
                        std::thread::spawn(move || {
                            std::thread::sleep(delay);
                            materialize();
                        });
                    }
                    let _ = req.respond(tiny_http::Response::from_string(r#"{"status":"Upload successful"}"#).with_status_code(201));
                } else {
                    let _ = req.respond(tiny_http::Response::from_string("[]"));
                }
            }
        });
        addr
    }

    /// 回归：并发投递到不同文件夹，每本书都落进自己要的文件夹（"设当前文件夹 → /upload" 不被别的投递插队）。
    /// 假 xochitl 记下每次 `GET /documents/<uuid>` 设的当前文件夹，`POST /upload` 时把"文件名 → 当前文件夹"记下来。
    #[test]
    fn concurrent_uploads_land_in_their_own_folders() {
        let lib = tempfile::tempdir().unwrap();
        for (uuid, name) in [("fa", "甲"), ("fb", "乙")] {
            std::fs::write(lib.path().join(format!("{uuid}.metadata")), format!(r#"{{"type":"CollectionType","visibleName":"{name}","parent":""}}"#)).unwrap();
        }
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        let landed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let l2 = landed.clone();
        std::thread::spawn(move || {
            let mut current = String::new();
            for mut req in server.incoming_requests() {
                if req.method() == &tiny_http::Method::Post {
                    let mut body = Vec::new();
                    std::io::Read::read_to_end(req.as_reader(), &mut body).unwrap();
                    let text = String::from_utf8_lossy(&body).to_string();
                    let fname = text.split("filename=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
                    l2.lock().unwrap().push((fname, current.clone()));
                } else {
                    current = req.url().trim_start_matches("/documents/").trim_start_matches('/').to_string();
                    std::thread::sleep(std::time::Duration::from_millis(2)); // 拉大"设完文件夹到上传"之间的窗口
                }
                let _ = req.respond(tiny_http::Response::from_string("{}"));
            }
        });
        let x = std::sync::Arc::new(Xochitl::new(&addr, lib.path(), 10));
        let hs: Vec<_> = [("甲", "fa"), ("乙", "fb")]
            .into_iter()
            .map(|(folder, _)| {
                let x = x.clone();
                std::thread::spawn(move || {
                    for i in 0..10 {
                        let target = Folder::from(x.folder_by_name(folder));
                        x.upload_to(UploadBody::Bytes(b"data"), &format!("{folder}-{i}.pdf"), "application/pdf", &target).unwrap();
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        let got = landed.lock().unwrap().clone();
        assert_eq!(got.len(), 20);
        for (fname, folder) in got {
            let want = if fname.starts_with('甲') { "fa" } else { "fb" };
            assert_eq!(folder, want, "{fname} 落错了文件夹");
        }
    }

    /// 按 [`Folder`] 指定目标文件夹：不经名字查找，书库里不同层有同名文件夹也落进给定的那一个。
    #[test]
    fn upload_to_sets_given_folder() {
        let lib = tempfile::tempdir().unwrap();
        for (uuid, name, parent) in [("p1", "漫画", ""), ("c1", "卷01", "p1"), ("p2", "小说", ""), ("c2", "卷01", "p2")] {
            std::fs::write(lib.path().join(format!("{uuid}.metadata")), format!(r#"{{"type":"CollectionType","visibleName":"{name}","parent":"{parent}"}}"#)).unwrap();
        }
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let status = if req.method() == &tiny_http::Method::Get {
                    s2.lock().unwrap().push(req.url().to_string());
                    if req.url().ends_with("/gone") { 404 } else { 200 }
                } else {
                    std::io::Read::read_to_end(req.as_reader(), &mut Vec::new()).unwrap();
                    200
                };
                let _ = req.respond(tiny_http::Response::from_string("{}").with_status_code(status));
            }
        });
        let x = Xochitl::new(&addr, lib.path(), 10);
        let f = lib.path().join("a.epub");
        std::fs::write(&f, b"PK\x03\x04").unwrap();
        let id = |s: &str| Folder::Id(FolderId::unchecked(s));
        x.upload_to(UploadBody::File(&f), "a.epub", "application/epub+zip", &id("c2")).unwrap();
        x.upload_to(UploadBody::File(&f), "a.epub", "application/epub+zip", &Folder::Root).unwrap();
        x.upload_to(UploadBody::File(&f), "a.epub", "application/epub+zip", &x.folder_by_name("小说").into()).unwrap();
        x.upload_to(UploadBody::Bytes(b"PK\x03\x04"), "n.rmdoc", "application/zip", &id("c1")).unwrap();
        assert_eq!(*seen.lock().unwrap(), ["/documents/c2", "/documents/", "/documents/p2", "/documents/c1"]);
        // 设文件夹失败（假服务对 /documents/gone 回 404）→ 退回根，而不是沿用上一次设的文件夹
        seen.lock().unwrap().clear();
        x.upload_to(UploadBody::File(&f), "a.epub", "application/epub+zip", &id("gone")).unwrap();
        assert_eq!(*seen.lock().unwrap(), ["/documents/gone", "/documents/"]);
        assert_eq!(x.child_folder(&id("p1"), "卷01").as_ref().map(FolderId::as_str), Some("c1"));
    }

    /// 上传并认领：快照里已有的同字节旧文档不算；按字节 / visibleName 认出新出现的那一份。
    #[test]
    fn upload_and_claim_skips_snapshot_and_matches_by_strategy() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let now = crate::clock::now_ms();
        // 两秒窗口内刚进库的同字节旧文档（比如上一回导入的同一本书）：快照排除它
        std::fs::write(lib.path().join("old.epub"), b"PK-same").unwrap();
        std::fs::write(lib.path().join("old.metadata"), format!(r#"{{"type":"DocumentType","visibleName":"a.epub","parent":"","createdTime":"{now}"}}"#)).unwrap();
        let src = lib.path().join("src.bin");
        std::fs::write(&src, b"PK-same").unwrap();
        let wait = ClaimWait::new(std::time::Duration::from_secs(5));
        let c = x.upload_and_claim(UploadBody::File(&src), "a.epub", "application/epub+zip", &Folder::Root, ClaimBy::SameBytes, wait).unwrap();
        assert_ne!(c.uuid, "old", "快照里已有的旧文档不能认成新的");
        assert!(matches!(c.delivery, Delivery::Delivered(_)));
        assert_eq!(std::fs::read(lib.path().join(format!("{}.epub", c.uuid))).unwrap(), b"PK-same");
        let n = x.upload_and_claim(UploadBody::Bytes(b"notebook"), "笔记.rmdoc", "application/zip", &Folder::Root, ClaimBy::VisibleName("笔记.rmdoc"), wait).unwrap();
        assert!(n.uuid != c.uuid && n.uuid != "old");
        let s = x.upload_and_claim(UploadBody::Bytes(b"%PDF-sz"), "p.pdf", "application/pdf", &Folder::Root, ClaimBy::SameSize, wait).unwrap();
        assert_eq!(std::fs::metadata(lib.path().join(format!("{}.pdf", s.uuid))).unwrap().len(), 7);
    }

    /// 判据都不符时报"没认出来"，**绝不取最新一本**（此前渲染自检的兜底会把页边距登记到别人刚投的书上）。
    #[test]
    fn upload_and_claim_never_falls_back_to_newest() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let wait = ClaimWait::new(std::time::Duration::from_millis(400));
        let r = x.upload_and_claim(UploadBody::Bytes(b"PK"), "b.epub", "application/epub+zip", &Folder::Root, ClaimBy::VisibleName("别的书名"), wait);
        assert!(matches!(r, Err(ClaimError::NotFound { delivery: Delivery::Delivered(_), .. })), "{r:?}");
        assert_eq!(find_documents_since(lib.path(), 0).len(), 1, "书确实进库了，只是判据不符，不认");
        let e = Xochitl::new("127.0.0.1:1", lib.path(), 1).upload_and_claim(UploadBody::Bytes(b"PK"), "b.epub", "x", &Folder::Root, ClaimBy::SameBytes, wait).unwrap_err();
        assert!(matches!(&e, ClaimError::Upload(m) if m.starts_with("上传失败")), "{e:?}");
        assert!(e.to_string().starts_with("上传给 xochitl 失败"));
    }

    /// 跨进程上传锁：别的进程（这里用另开的一个文件描述符模拟——flock 按打开的文件描述互斥，同进程两次 open 也互斥）
    /// 持有锁文件时，"设文件夹 → /upload"要等它放手才开始。
    #[test]
    fn upload_waits_for_cross_process_lock_file() {
        let lib = tempfile::tempdir().unwrap();
        let lock = lib.path().join("run/shelf").join(LOCK_FILE_NAME);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h2 = hits.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                std::io::Read::read_to_end(req.as_reader(), &mut Vec::new()).unwrap();
                h2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = req.respond(tiny_http::Response::from_string("{}"));
            }
        });
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        let other = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock).unwrap();
        other.lock().unwrap();
        let x = Xochitl::new(&addr, lib.path(), 5).with_upload_lock(Some(lock.clone()));
        let t = std::thread::spawn(move || x.upload_to(UploadBody::Bytes(b"PK"), "a.epub", "application/epub+zip", &Folder::Root));
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "锁被别的进程拿着时不能发请求");
        drop(other);
        assert!(matches!(t.join().unwrap(), Ok(Delivery::Delivered(_))));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2, "放手后照常 GET 设文件夹 + POST 上传");
    }

    #[test]
    fn default_lock_file_only_for_real_library() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(claim::default_lock_file(t.path()), None, "临时书库（单测）不碰真实运行时目录");
        let p = crate::paths::Paths::from_env();
        assert_eq!(claim::default_lock_file(&p.xochitl_dir()), Some(p.runtime_dir().join(LOCK_FILE_NAME)));
    }

    #[test]
    fn folder_helpers_return_typed_ids() {
        let lib = tempfile::tempdir().unwrap();
        let (a, b) = ("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        std::fs::write(lib.path().join(format!("{a}.metadata")), r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#).unwrap();
        std::fs::write(lib.path().join(format!("{b}.metadata")), format!(r#"{{"type":"CollectionType","visibleName":"卷01","parent":"{a}"}}"#)).unwrap();
        std::fs::write(lib.path().join("doc.metadata"), format!(r#"{{"type":"DocumentType","visibleName":"书","parent":"{b}"}}"#)).unwrap();
        let x = Xochitl::new("127.0.0.1:1", lib.path(), 1);
        let ma = x.folder_by_name("漫画").unwrap();
        assert_eq!(ma.as_str(), a);
        assert_eq!(x.child_folder(&Folder::Id(ma), "卷01").unwrap().as_str(), b);
        assert_eq!(x.child_folder(&Folder::Root, "卷01"), None);
        assert_eq!(x.folder_of_document("doc"), Some(Folder::Id(FolderId::parse(b).unwrap())));
    }

    #[test]
    fn upload_large_swaps_epub_and_drops_render_cache() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let big = lib.path().join("big-source.bin");
        let big_bytes = vec![7u8; 300_000];
        std::fs::write(&big, &big_bytes).unwrap();
        let uuid = x.upload_large(&big, "书 - 02卷.epub", "application/epub+zip", &Folder::Root, b"PLACEHOLDER-EPUB", None).unwrap();
        assert_eq!(std::fs::read(lib.path().join(format!("{uuid}.epub"))).unwrap(), big_bytes, "占位必须被真文件替换");
        assert!(!lib.path().join(format!("{uuid}.pdf")).exists(), "占位的渲染缓存必须删掉，让 xochitl 重新渲染");
        assert!(!lib.path().join(format!("{uuid}.epubindex")).exists());
        let leftovers: Vec<String> = std::fs::read_dir(lib.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".new") || n.ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "不留临时文件: {leftovers:?}");
    }

    /// 回归：两本大 PDF 同时走大文件通道（占位字节相同），各自认领到自己的条目、内容不串。
    #[test]
    fn concurrent_large_pdf_uploads_claim_distinct_entries() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl_delayed(lib.path().to_path_buf(), std::time::Duration::from_millis(300));
        let x = std::sync::Arc::new(Xochitl::new(&addr, lib.path(), 10));
        let hs: Vec<_> = (0..3u8)
            .map(|i| {
                let (x, dir) = (x.clone(), lib.path().to_path_buf());
                std::thread::spawn(move || {
                    let src = dir.join(format!("src-{i}.bin"));
                    let bytes = vec![i + 1; 8_000_000 + i as usize * 1000];
                    std::fs::write(&src, &bytes).unwrap();
                    let uuid = x.upload_large(&src, &format!("大书{i}.pdf"), "application/pdf", &Folder::Root, b"%PDF-placeholder", Some(3)).unwrap();
                    (uuid, bytes)
                })
            })
            .collect();
        let got: Vec<(String, Vec<u8>)> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        let uuids: std::collections::HashSet<_> = got.iter().map(|(u, _)| u.clone()).collect();
        assert_eq!(uuids.len(), 3, "每本认领到不同条目");
        for (uuid, bytes) in got {
            assert_eq!(std::fs::read(lib.path().join(format!("{uuid}.pdf"))).unwrap(), bytes, "条目里是自己的内容");
        }
    }

    #[test]
    fn upload_large_pdf_rewrites_content_pages() {
        let lib = tempfile::tempdir().unwrap();
        let addr = fake_xochitl(lib.path().to_path_buf());
        let x = Xochitl::new(&addr, lib.path(), 10);
        let big = lib.path().join("big-source.bin");
        std::fs::write(&big, vec![9u8; 123_456]).unwrap();
        let uuid = x.upload_large(&big, "漫画.pdf", "application/pdf", &Folder::Root, b"%PDF-placeholder", Some(349)).unwrap();
        assert_eq!(std::fs::metadata(lib.path().join(format!("{uuid}.pdf"))).unwrap().len(), 123_456);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(lib.path().join(format!("{uuid}.content"))).unwrap()).unwrap();
        assert_eq!(v["pageCount"], 349);
        assert_eq!(v["pages"].as_array().unwrap().len(), 349);
        assert_eq!(v["sizeInBytes"], "123456");
    }

    #[test]
    fn upload_large_rejects_other_formats_and_missing_library() {
        let lib = tempfile::tempdir().unwrap();
        let x = Xochitl::new("127.0.0.1:1", lib.path(), 1);
        assert!(x.upload_large(Path::new("/x"), "a.cbz", "x", &Folder::Root, b"p", None).unwrap_err().contains("EPUB/PDF"));
        let y = Xochitl::new("127.0.0.1:1", Path::new("/nonexistent-lib"), 1);
        assert!(y.upload_large(Path::new("/x"), "a.epub", "x", &Folder::Root, b"p", None).unwrap_err().contains("只能在设备上用"));
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

    #[test]
    fn header_filename_strips_quotes_and_newlines_only() {
        assert_eq!(header_safe_filename("他说\"好\".pdf"), "他说'好'.pdf");
        assert_eq!(header_safe_filename("a\r\nX-Evil: 1.epub"), "a  X-Evil: 1.epub");
        assert_eq!(header_safe_filename("镖人 - 01卷.epub"), "镖人 - 01卷.epub", "普通名字原样");
    }

    /// 回归：xochitl 回 5xx、响应体里恰好带 "timeout" 字样时，不能当成"很可能已送达"（那样调用方不会重试，
    /// 书其实没进库）。此前把错误拼成 `"HTTP 500: …"` 字符串再找 `timeout` 子串，就会误判。
    #[test]
    fn server_error_mentioning_timeout_is_not_likely_delivered() {
        let lib = tempfile::tempdir().unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                std::io::Read::read_to_end(req.as_reader(), &mut Vec::new()).unwrap();
                let (code, body) = if req.method() == &tiny_http::Method::Post { (500, "upstream timeout while importing") } else { (200, "{}") };
                let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(code));
            }
        });
        let x = Xochitl::new(&addr, lib.path(), 5);
        let r = x.upload_to(UploadBody::Bytes(b"PK\x03\x04"), "a.epub", "application/epub+zip", &Folder::Root);
        assert!(matches!(&r, Err(e) if e.contains("HTTP 500") && e.contains("timeout")), "5xx 是明确的失败，应可重试: {r:?}");
    }

    #[test]
    fn classifies_upload_errors() {
        assert!(UploadError::Status(408, "408 request timeout".into()).likely_delivered());
        assert!(!UploadError::Status(500, "upstream timeout".into()).likely_delivered(), "5xx 不论体里写什么都是失败");
        assert!(!UploadError::Status(504, "Gateway Timeout".into()).likely_delivered());
        assert!(UploadError::Timeout("上传失败: timed out reading response".into()).likely_delivered());
        assert!(UploadError::AfterSuccess("x".into()).likely_delivered());
        assert!(!UploadError::Connect("上传失败: connect timed out".into()).likely_delivered());
        assert!(!UploadError::Other("上传失败: connection reset".into()).likely_delivered());
        assert_eq!(UploadError::Status(500, "boom".into()).message(), "HTTP 500: boom", "文案格式不变");
    }

    /// 真实传输错误的分类：连不上 → 失败可重试；连上后 xochitl 迟迟不回（大书处理慢）→ 读超时 → 很可能已送达。
    #[test]
    fn transport_errors_classified_by_kind() {
        let lib = tempfile::tempdir().unwrap();
        let refused = Xochitl::new("127.0.0.1:1", lib.path(), 2).upload_to(UploadBody::Bytes(b"x"), "a.pdf", "application/pdf", &Folder::Root);
        assert!(matches!(&refused, Err(e) if e.starts_with("上传失败")), "{refused:?}");
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                std::io::Read::read_to_end(req.as_reader(), &mut Vec::new()).unwrap();
                if req.method() == &tiny_http::Method::Post {
                    std::thread::sleep(std::time::Duration::from_millis(2500)); // 比客户端 1 秒超时长
                }
                let _ = req.respond(tiny_http::Response::from_string("{}"));
            }
        });
        let slow = Xochitl::new(&addr, lib.path(), 1).upload_to(UploadBody::Bytes(b"x"), "a.pdf", "application/pdf", &Folder::Root);
        assert!(matches!(&slow, Ok(Delivery::LikelyDelivered(_))), "{slow:?}");
    }
}

/// 2026-09-19 OOM 审计：上传改流式发送体（今 [`Xochitl::upload_to`] 的 `UploadBody::Bytes` / `UploadBody::File` 两路）后，线上字节序列应该跟改动前的
/// "整块 Vec 拼 body" 写法完全一致，只是不再整块囤内存。起一个最小 HTTP mock（GET 任意路径回
/// 200 空体，POST /upload 把收到的原始 body 送回 channel）验证这一点——不需要真连 xochitl。
#[cfg(test)]
mod upload_tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// 强制每个请求 `Connection: close`（下一个请求另起连接），mock 逻辑不用管 keep-alive 复用；
    /// 每次上传先发一次 `set_folder` 的 GET 再发 POST /upload，遇到 POST 就把
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
        let r = x.upload_to(UploadBody::Bytes(&data), "book.epub", "application/epub+zip", &Folder::Root);
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
        let r = x.upload_to(UploadBody::File(&path), "book.epub", "application/epub+zip", &Folder::Root);
        assert!(matches!(r, Ok(Delivery::Delivered(_))), "{r:?}");
        let body = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_well_formed_upload(&body, &data, "book.epub", "application/epub+zip");
    }
}
