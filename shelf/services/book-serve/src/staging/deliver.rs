//! 落库：加入 xochitl（整本 / 大文件通道）、渲染记录与落库记录、建文件夹等待。
use super::*;

/// 落库前等待「建文件夹」代理真的建出来目标文件夹的上限——`shelf-mkdir-agent.qmd` 现为长轮询
/// （入队即刻响应，旧版是 8 秒一次 Timer 轮询），20 秒足够留出建夹 + 落盘的余量；等不到不算错误，`ensure_folder` 会原样放行，交给
/// `Xochitl::upload` 现有的"找不到就落书库根"兜底（改动前就有的行为，不是新错误）。
pub(super) const FOLDER_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 大文件通道的安全上限（1GiB）：再大 xochitl 首次渲染的内存/时间没有验证过。
pub(super) const MAX_DIRECT_BYTES: u64 = 1 << 30;

impl Staging {
    // ───────────── 落库 ─────────────

    /// 加入 xochitl：纯复制原字节（不再优化）。xochitl 只读 EPUB/PDF（CBZ 漫画不加入 xochitl，用户定）。`folder`
    /// 空＝书库根目录（2026-09-19 用户明确要求去掉"留空落进配置里的缺省文件夹"这条隐藏行为——跟
    /// KOReader 那边"留空＝根目录"的语义对齐，不再有一个不写在界面上的"默认文件夹"概念；
    /// [`crate::config::BookConfig::library_folder`] 配置项随这次改动一并删除，不再有任何地方读它）；
    /// 母版库条目投完**永远保留**（2026-09-19 用户明确要求去掉"投完自动删除"这个功能——母版是可以
    /// 反复投给两个读器对照、换设备重投的底本，不该被一次性动作悄悄清掉；要删由用户自己在列表里点
    /// 删除）。返回回执文案 + EPUB 的渲染自检计划（调用方起线程跑 `render_check::run`）。
    /// **同步、阻塞**——上传大书、等建文件夹都能到分钟级；跟 [`Self::optimize`] 一样，
    /// HTTP 接口不该直接暴露这个方法，用 [`Self::spawn_deliver`] 走后台线程。
    pub fn deliver(&self, name: &str, folder: &str, mkdir: &MkdirQueue) -> Result<DeliverOutcome, String> {
        let (ct, p) = self.deliverable(name)?;
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let folder = folder.trim();
        self.ensure_folder(folder, mkdir);
        if self.native_limit > 0 && size > self.native_limit {
            // 超限：走大文件通道（2026-09-20 用户要求突破上传限制，真机验证 PDF 154MB/EPUB 153MB 可行）——
            // 占位文档 + 磁盘上替换成真文件。本机没有 xochitl 书库目录（非设备环境）、超过 `MAX_DIRECT_BYTES`
            // 或造占位失败才拒绝。**不再按卷拆分**（2026-09-30 用户定移除；此前 EPUB 漫画按 NCX、PDF 按书签拆成
            // 若干份分别上传，作为大文件通道之后的回退）。
            if let Some(outcome) = self.try_deliver_direct(name, &p, size, folder)? {
                return Ok(outcome);
            }
            return Err(format!("《{name}》{} MB 超过 xochitl 上传上限（{} MB），也走不了大文件通道（超过 1GB，或造不出占位文档），没有加入", size >> 20, self.native_limit >> 20));
        }
        // 2026-09-19 OOM 审计：这条路径以前 `std::fs::read` 整本读进 `Vec<u8>`，自检+上传各自又在
        // 内部再叠一份（`text_profile` 解压全部条目含图片、`Xochitl::upload` 内部克隆一份拼
        // multipart body），≤90MB 的书峰值能叠到 ~180-270MB。现在全程不把整本读进内存：自检走
        // `text_profile_file`（流式开文件，图片条目连解压都跳过），上传走 `upload_file`（流式发送
        // 体，见 rmsvc_core::xochitl 文档）。自检计划在上传前算好（投书时刻要早于 xochitl 给文档的
        // createdTime）；统计失败就不自检，不影响投书。
        let render = if formats::ext_of(name) == "epub" {
            let comic = self.comic_margin_eligible(&p);
            bookconv::stats::text_profile_file(&p).ok().map(|prof| RenderPlan { name: name.to_string(), title: prof.title.clone(), expected: prof.expected_pages(), since_ms: rmsvc_core::clock::now_ms(), comic })
        } else {
            None
        };
        let message = match self.xochitl.upload_file(&p, name, ct.mime(), folder)? {
            Delivery::Delivered(_) => format!("已加入 xochitl《{name}》"),
            Delivery::LikelyDelivered(_) => format!("已加入 xochitl《{name}》（设备处理较慢，稍候刷新书库）"),
        };
        let _ = self.mark_delivered(name, Reader::Native);
        Ok(DeliverOutcome { message, render })
    }

    /// 落库前的零耗时校验：xochitl 读得了的格式 + 书还在母版库。返回（内容类型, 路径）。
    fn deliverable(&self, name: &str) -> Result<(bookconv::convert::ContentType, PathBuf), String> {
        let ct = bookconv::convert::direct_content_type(name).ok_or("xochitl 只读 EPUB / PDF")?;
        Ok((ct, self.existing(name)?))
    }

    /// 给"已加入 xochitl 但没有渲染记录"的书补记（2026-09-20：大文件通道上线前直接投入的书没有渲染徽章，列表里不统一）。
    /// 按书名（规范名或文件名 stem）+ 文件大小在 xochitl 书库里认领对应文档；没渲染缓存（`.pdf`）＝没打开过 → `onopen`（记当前
    /// 占位页数，之后 `list()` 看到页数变了就升级）；有缓存＝已渲染过 → 直接 `ok` 记真页数。返回补记了几本。幂等、只补缺的。
    pub fn backfill_render_records(&self) -> usize {
        let lib = self.xochitl.library_dir().to_path_buf();
        if !lib.is_dir() {
            return 0;
        }
        let docs = rmsvc_core::xochitl::find_documents_since(&lib, 0);
        let mut n = 0;
        for e in self.list() {
            let has_native = e.delivered.as_ref().map(|d| d.native.is_some() && d.render.is_none()).unwrap_or(false);
            let ext = formats::ext_of(&e.name);
            if !has_native || (ext != "epub" && ext != "pdf") {
                continue;
            }
            let stem = e.name.strip_suffix(&format!(".{ext}")).unwrap_or(&e.name).to_string();
            let canon = bookconv::naming::canonical_book_name(&stem);
            let eq = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
            let doc = docs.iter().find(|d| {
                (eq(&d.visible_name, &canon) || eq(&d.visible_name, &stem) || eq(&d.visible_name, &e.name))
                    && std::fs::metadata(lib.join(format!("{}.{ext}", d.uuid))).map(|m| m.len() == e.bytes).unwrap_or(false)
            });
            let Some(doc) = doc else { continue };
            let pages = rmsvc_core::xochitl::page_count(&lib, &doc.uuid).unwrap_or(0);
            let opened = ext == "pdf" || lib.join(format!("{}.pdf", doc.uuid)).exists();
            let rc = sidecar::RenderCheck { uuid: doc.uuid.clone(), pages, expected: 0, status: if opened { "ok".into() } else { "onopen".into() }, at: rmsvc_core::clock::now_secs() };
            if self.set_render(&e.name, rc).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// 大文件通道（见 [`rmsvc_core::xochitl::Xochitl::upload_large_file`]）：成功 `Ok(Some)`；条件不满足（非
    /// EPUB/PDF、超过安全上限、本机没有 xochitl 书库目录、造占位失败）→ `Ok(None)`，调用方整本拒绝；
    /// 占位已上传之后才出的错 → `Err`（不再退回拒绝，否则书库里会留下半成品占位）。
    pub(super) fn try_deliver_direct(&self, name: &str, p: &Path, size: u64, folder: &str) -> Result<Option<DeliverOutcome>, String> {
        let ext = formats::ext_of(name);
        if (ext != "epub" && ext != "pdf") || size > MAX_DIRECT_BYTES || !self.xochitl.library_dir().is_dir() {
            return Ok(None);
        }
        let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(name);
        let (placeholder, content_type, pages) = if ext == "epub" {
            // 显示名：有卷标记用规范名（与文件名一致），否则沿用书自己的 dc:title。
            let title = bookconv::naming::has_volume_marker(stem).then(|| bookconv::naming::canonical_book_name(stem));
            match bookconv::placeholder::epub_placeholder(p, title.as_deref()) {
                Ok(b) => (b, "application/epub+zip", None),
                Err(_) => return Ok(None),
            }
        } else {
            // 第三方 PDF 常是交叉引用流/对象流、页树根不在对象 2：走通用的有界读取（`pdfmeta`），不整本读进内存。
            let pages = match bookconv::convert::pdfmeta::page_count(p) {
                Ok(n) => n,
                Err(_) => return Ok(None),
            };
            match bookconv::placeholder::pdf_placeholder() {
                Ok(b) => (b, "application/pdf", Some(pages)),
                Err(_) => return Ok(None),
            }
        };
        let uuid = self.xochitl.upload_large_file(p, name, content_type, folder, &placeholder, pages)?;
        if ext == "epub" && self.comic_margin_eligible(p) {
            self.register_comic_margins(&uuid, name);
        }
        let _ = self.mark_delivered(name, Reader::Native);
        // 渲染记录也写上，让"加入 xochitl"的书在列表里都有统一的渲染徽章（此前直接投入的书没有）：
        // - PDF：页数就是我们写进 `.content` 的真页数 → 直接 ok；
        // - EPUB：xochitl 要**首次打开**才渲染，此刻 `.content` 里是占位的页数。记 `onopen` + 占位页数，`list()` 之后每次
        //   读该文档 `.content` 的 pageCount，一变（用户打开过、xochitl 渲染完）就自动显示成真页数。
        let rc = match pages {
            Some(n) => sidecar::RenderCheck { uuid: uuid.clone(), pages: n as u64, expected: 0, status: "ok".into(), at: rmsvc_core::clock::now_secs() },
            None => sidecar::RenderCheck {
                uuid: uuid.clone(),
                pages: rmsvc_core::xochitl::page_count(self.xochitl.library_dir(), &uuid).unwrap_or(0),
                expected: 0,
                status: "onopen".into(),
                at: rmsvc_core::clock::now_secs(),
            },
        };
        let _ = self.set_render(name, rc);
        Ok(Some(DeliverOutcome {
            message: format!("《{stem}》{} MB 超过网页上传上限，已直接写入 xochitl 书库；首次打开需重新渲染，请稍候", size >> 20),
            render: None,
        }))
    }

    /// 落库前确保目标文件夹真的存在（2026-09-19，用户反馈"文件夹里写了名字依然不会创建文件夹"）：
    /// 已经存在（或本来就是空串＝书库根）直接放行；不存在就往 `mkdir` 队列扔一个"建文件夹"请求，
    /// 同步等 `shelf-mkdir-agent.qmd`（MainView 注入，长轮询 `/mkdir/pending?wait=`，唯一合法的建文件夹路径，
    /// 外部进程不能直接写 xochitl 书库的 `.metadata`）真的建出来再放行。等不到就超时放弃——不是
    /// 新错误，[`rmsvc_core::xochitl::Xochitl::upload`] 本来就有"文件夹名找不到就落书库根"的
    /// best-effort 兜底，改动前就是这个行为，这里只是尽量把"真建出来"这条更好的结果多等一会。
    pub(super) fn ensure_folder(&self, folder: &str, mkdir: &MkdirQueue) {
        if folder.is_empty() || self.xochitl.find_folder(folder).is_some() {
            return;
        }
        if mkdir.add(folder).is_err() {
            return; // 名字不合法（目前只剩"空"这一种情况——2026-09-19 起 `/`\`\` 不再算不合法，见 mkdir.rs::add）
        }
        let lib_dir = self.xochitl.library_dir();
        rmsvc_core::fswatch::watch_until(lib_dir, render_check::DEBOUNCE, FOLDER_WAIT_TIMEOUT, |_| self.xochitl.find_folder(folder).is_some());
    }

    /// [`Self::deliver`] 的异步版：同 [`Self::spawn_optimize`] 套路——先做零耗时校验（格式/文件存在），
    /// 校验过了才加忙锁、起后台线程跑真正耗时的部分。成功返回后 HTTP 层立即回"已开始"，真正结果通过
    /// `bus` 的 `books`/`staging` 事件 + `GET /staging` 列表里这条目的 `delivered.deliver`
    /// （[`sidecar::DeliverCheck`]）异步呈现；渲染自检计划、`mark_delivered` 全部在 `deliver` 内部
    /// 完成，不劳 HTTP 层操心。`catch_unwind` 兜底同 `spawn_optimize`。
    pub fn spawn_deliver(&self, name: &str, folder: &str, mkdir: Arc<MkdirQueue>, bus: Arc<rmsvc_core::events::EventBus>) -> Result<(), String> {
        self.deliverable(name)?;
        if !self.try_start_busy(name) {
            return Err(busy_err(name, ""));
        }
        let now = rmsvc_core::clock::now_secs();
        let _ = self.set_deliver_check(name, sidecar::DeliverCheck { status: "pending".into(), message: String::new(), at: now });
        let folder = folder.to_string();
        self.spawn_bg(name, bus, move |this, name, bus| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.deliver(name, &folder, &mkdir)))
                .unwrap_or_else(|_| Err("落库过程内部异常（已捕获，不影响其他操作）".to_string()));
            let at = rmsvc_core::clock::now_secs();
            let (status, message) = final_status(result.as_ref().map(|o| o.message.as_str()).map_err(String::as_str));
            let _ = this.set_deliver_check(name, sidecar::DeliverCheck { status, message, at });
            if let Ok(outcome) = &result {
                if let Some(plan) = outcome.render.clone() {
                    let (staging2, bus2, lib2) = (this.clone(), bus.clone(), this.xochitl.library_dir().to_path_buf());
                    std::thread::spawn(move || crate::render_check::run(&staging2, &bus2, &lib2, &plan));
                }
            }
        });
        Ok(())
    }

    /// 写异步落库结果到边车（书已从母版库删除 → Err，调用方只记日志/静默丢弃，不阻断别的流程——
    /// 比如落库还在跑的时候用户自己手动删了这本书）。
    pub fn set_deliver_check(&self, name: &str, dc: sidecar::DeliverCheck) -> Result<(), String> {
        self.update_sidecar(name, |d| d.deliver = Some(dc))
    }

    /// 写渲染自检结果到边车（书已从母版库删除 → Err，调用方只记日志）。
    pub fn set_render(&self, name: &str, rc: RenderCheck) -> Result<(), String> {
        self.update_sidecar(name, |d| d.render = Some(rc))
    }

    /// 记一次落库：写 sidecar `.<name>.delivered`。
    pub fn mark_delivered(&self, name: &str, reader: Reader) -> Result<(), String> {
        let now = rmsvc_core::clock::now_secs();
        self.update_sidecar(name, |d| match reader {
            Reader::Native => d.native = Some(now),
            Reader::Koreader => d.koreader = Some(now),
        })
    }
}
