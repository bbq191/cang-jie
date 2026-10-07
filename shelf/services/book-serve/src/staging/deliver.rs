//! 落库：加入 xochitl（整本 / 大文件通道）、渲染记录与落库记录、建文件夹等待。
use super::*;

/// 落库前等待「建文件夹」代理真的建出来目标文件夹的上限——`shelf-mkdir-agent.qmd` 现为长轮询
/// （入队即刻响应，旧版是 8 秒一次 Timer 轮询），20 秒足够留出建夹 + 落盘的余量；等不到不算错误，`ensure_folder` 会原样放行，交给
/// `Xochitl::upload` 现有的"找不到就落书库根"兜底（改动前就有的行为，不是新错误）。
pub(crate) const FOLDER_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 大文件通道的安全上限（1GiB）：再大 xochitl 首次渲染的内存/时间没有验证过。
pub(crate) const MAX_DIRECT_BYTES: u64 = 1 << 30;

/// [`Staging::upload_large`] 的结果：新文档的 uuid；PDF 还有写进 `.content` 的页数（EPUB 首次打开才渲染，`None`）。
pub(crate) struct LargeUpload {
    pub uuid: String,
    pub pages: Option<usize>,
}

impl Staging {
    // ───────────── 落库 ─────────────

    /// 加入 xochitl：纯复制原字节（书架不优化书，书应先在电脑上用 sheng-ren 优化好）。xochitl 只读 EPUB/PDF。`folder`
    /// 空＝书库根目录（2026-09-19 起没有"默认文件夹"配置）；母版库条目投完**永远保留**（2026-09-19 用户明确要求去掉"投完自动删除"这个功能——母版是可以
    /// 换设备重投的底本，不该被一次性动作悄悄清掉；要删由用户自己在列表里点
    /// 删除）。返回回执文案 + EPUB 的渲染自检计划（调用方起线程跑 `render_check::run`）。
    /// **同步、阻塞**——上传大书、等建文件夹都能到分钟级；HTTP 接口不该直接暴露这个方法，用 [`Self::spawn_deliver`] 走后台线程。
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
            let folder_uuid = if folder.is_empty() { String::new() } else { self.xochitl.find_folder(folder).unwrap_or_default() };
            if let Some(outcome) = self.try_deliver_direct(name, &p, size, &folder_uuid)? {
                return Ok(outcome);
            }
            return Err(format!("《{name}》{} MB 超过 xochitl 上传上限（{} MB），也走不了大文件通道（超过 1GB，或造不出占位文档），没有加入", size >> 20, self.native_limit >> 20));
        }
        // 全程不把整本读进内存：上传走 `upload_file`（流式发送体，见 rmsvc_core::xochitl 文档），自检只读 OPF 里的书名。
        // 自检计划在上传前算好（投书时刻要早于 xochitl 给文档的 createdTime）。书名和漫画页边距标记从同一次打开的 zip 里取。
        let render = (formats::ext_of(name) == "epub").then(|| {
            let mut book = shelf_conv::epub::Book::open(&p).ok();
            RenderPlan {
                name: name.to_string(),
                title: book.as_ref().and_then(|b| b.title()),
                since_ms: rmsvc_core::clock::now_ms(),
                comic_margins: book.as_mut().and_then(|b| b.reader_margins()),
            }
        });
        let message = match self.xochitl.upload_file(&p, name, ct.mime(), folder)? {
            Delivery::Delivered(_) => format!("已加入 xochitl《{name}》"),
            Delivery::LikelyDelivered(_) => format!("已加入 xochitl《{name}》（设备处理较慢，稍候刷新书库）"),
        };
        let _ = self.mark_delivered(name);
        Ok(DeliverOutcome { message, render })
    }

    /// 落库前的零耗时校验：xochitl 读得了的格式 + 书还在母版库。返回（内容类型, 路径）。
    fn deliverable(&self, name: &str) -> Result<(shelf_conv::ContentType, PathBuf), String> {
        let ct = shelf_conv::direct_content_type(name).ok_or("xochitl 只读 EPUB / PDF")?;
        Ok((ct, self.existing(name)?))
    }

    /// 母版库这本书走大文件通道（见 [`Self::upload_large`]）并记好落库/渲染记录：成功 `Ok(Some)`；条件不满足 → `Ok(None)`，
    /// 调用方整本拒绝；占位已上传之后才出的错 → `Err`（不再退回拒绝，否则书库里会留下半成品占位）。
    /// `folder_uuid` 是目标文件夹的 uuid（空串＝根；按名字找不到也是根，同 `upload_file` 的兜底）。
    fn try_deliver_direct(&self, name: &str, p: &Path, size: u64, folder_uuid: &str) -> Result<Option<DeliverOutcome>, String> {
        let Some(up) = self.upload_large(p, name, folder_uuid)? else { return Ok(None) };
        let _ = self.mark_delivered(name);
        // 渲染记录也写上，让"加入 xochitl"的书在列表里都有统一的渲染徽章：
        // - PDF：页数就是我们写进 `.content` 的真页数 → 直接 ok；
        // - EPUB：xochitl 要**首次打开**才渲染，此刻 `.content` 里是占位的页数。记 `onopen` + 占位页数，`list()` 之后每次
        //   读该文档 `.content` 的 pageCount，一变（用户打开过、xochitl 渲染完）就自动显示成真页数。
        let (pages, status) = match up.pages {
            Some(n) => (n as u64, "ok"),
            None => (rmsvc_core::xochitl::page_count(self.xochitl.library_dir(), &up.uuid).unwrap_or(0), "onopen"),
        };
        let _ = self.set_render(name, sidecar::RenderCheck { uuid: up.uuid, pages, status: status.into(), at: rmsvc_core::clock::now_secs() });
        let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
        Ok(Some(DeliverOutcome {
            message: format!("《{stem}》{} MB 超过网页上传上限，已直接写入 xochitl 书库；首次打开需重新渲染，请稍候", size >> 20),
            render: None,
        }))
    }

    /// 大文件通道（见 [`rmsvc_core::xochitl::Xochitl::upload_large_file`]）：造占位 → 上传占位 → 磁盘上换成真文件 →（漫画）登记
    /// 页边距。母版库落库和直接导入（`import.rs`）共用。条件不满足（非 EPUB/PDF、超过 [`MAX_DIRECT_BYTES`]、本机没有 xochitl
    /// 书库目录、读不了书造不出占位）→ `Ok(None)`；占位已上传之后才出的错 → `Err`。目标文件夹按 **uuid** 给（`folder_uuid`，
    /// 空串＝根）——直接导入的多级文件夹里不同上级下可能有同名子文件夹，按名字找会落错。
    pub(crate) fn upload_large(&self, p: &Path, name: &str, folder_uuid: &str) -> Result<Option<LargeUpload>, String> {
        let ext = formats::ext_of(name);
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if (ext != "epub" && ext != "pdf") || size > MAX_DIRECT_BYTES || !self.xochitl.library_dir().is_dir() {
            return Ok(None);
        }
        let (placeholder, content_type, pages, margins) = if ext == "epub" {
            // 显示名沿用书自己的 dc:title（sheng-ren 优化时已写好规范书名）；书名、封面、页边距标记同一次打开 zip 取完。
            let Ok(mut book) = shelf_conv::epub::Book::open(p) else { return Ok(None) };
            let Ok(ph) = shelf_conv::placeholder::epub_placeholder(&mut book) else { return Ok(None) };
            (ph, "application/epub+zip", None, book.reader_margins())
        } else {
            // 第三方 PDF 常是交叉引用流/对象流、页树根不在对象 2：走通用的有界读取（`pdfmeta`），不整本读进内存。
            let Ok(pages) = shelf_conv::pdfmeta::page_count(p) else { return Ok(None) };
            (shelf_conv::placeholder::pdf_placeholder(), "application/pdf", Some(pages), None)
        };
        let uuid = self.xochitl.upload_large_file_into(p, name, content_type, folder_uuid, &placeholder, pages)?;
        if let Some(m) = margins {
            self.register_comic_margins(&uuid, name, m);
        }
        Ok(Some(LargeUpload { uuid, pages }))
    }

    /// 落库前确保目标文件夹真的存在（2026-09-19，用户反馈"文件夹里写了名字依然不会创建文件夹"）：
    /// 已经存在（或本来就是空串＝书库根）直接放行；不存在就往 `mkdir` 队列扔一个"建文件夹"请求，
    /// 同步等 `shelf-mkdir-agent.qmd`（MainView 注入，长轮询 `/mkdir/pending?wait=`，唯一合法的建文件夹路径，
    /// 外部进程不能直接写 xochitl 书库的 `.metadata`）真的建出来再放行。等不到就超时放弃——不是
    /// 新错误，[`rmsvc_core::xochitl::Xochitl::upload`] 本来就有"文件夹名找不到就落书库根"的
    /// best-effort 兜底，改动前就是这个行为，这里只是尽量把"真建出来"这条更好的结果多等一会。
    pub(crate) fn ensure_folder(&self, folder: &str, mkdir: &MkdirQueue) {
        if folder.is_empty() || self.xochitl.find_folder(folder).is_some() {
            return;
        }
        // `Ok(0)` = 两次查询之间刚被建出来了，不用等；`Err` = 名字不合法（目前只剩"空"）。此前只判 `Err`，`Ok(0)` 也进
        // `watch_until`，它不先查一次，没有新事件就白等满 20 秒。
        if !matches!(mkdir.add(folder), Ok(n) if n > 0) {
            return;
        }
        let lib_dir = self.xochitl.library_dir();
        rmsvc_core::fswatch::watch_until(lib_dir, render_check::DEBOUNCE, FOLDER_WAIT_TIMEOUT, |_| self.xochitl.find_folder(folder).is_some());
    }

    /// 按层确保多级文件夹（2026-10-07，直接导入用）：从书库根往下，每一级在上一级正下方按名字找（[`rmsvc_core::xochitl::find_child_folder`]），
    /// 没有就入队请代理在上一级里建（`MkdirQueue::add_in`，代理调 `Library.createCollection(上一级 uuid, 名字)`），同 [`Self::ensure_folder`]
    /// 等它真建出来（每一级最多 `wait`，线上是 [`FOLDER_WAIT_TIMEOUT`]）。某一级等不到 / 入不了队就**停在已经有的那一级**，不往别处落。
    /// 返回（最里层拿到的文件夹 uuid，空串＝根；一共拿到了几级）。`segments` 由调用方拆好（不含空段）。
    pub(crate) fn ensure_folder_path(&self, segments: &[&str], mkdir: &MkdirQueue, wait: std::time::Duration) -> (String, usize) {
        let lib_dir = self.xochitl.library_dir();
        let mut parent = String::new();
        for (depth, seg) in segments.iter().enumerate() {
            let found = self.xochitl.find_child_folder(&parent, seg).or_else(|| {
                match mkdir.add_in(&parent, seg) {
                    // 两次查询之间刚被建出来了（同 ensure_folder 的 `Ok(0)`）
                    Ok(0) => {}
                    Ok(_) => {
                        rmsvc_core::fswatch::watch_until(lib_dir, render_check::DEBOUNCE, wait, |_| self.xochitl.find_child_folder(&parent, seg).is_some());
                    }
                    Err(e) => println!("[book-serve] 建文件夹《{seg}》入队失败: {e}"),
                }
                self.xochitl.find_child_folder(&parent, seg)
            });
            match found {
                Some(uuid) => parent = uuid,
                None => {
                    println!("[book-serve] 文件夹「{}」第 {} 级《{seg}》{} 秒内没建出来，书落在上一级「{}」", segments.join("/"), depth + 1, wait.as_secs(), segments[..depth].join("/"));
                    return (parent, depth);
                }
            }
        }
        (parent, segments.len())
    }

    /// [`Self::deliver`] 的异步版：先做零耗时校验（格式/文件存在），
    /// 校验过了才加忙锁、起后台线程跑真正耗时的部分。成功返回后 HTTP 层立即回"已开始"，真正结果通过
    /// `bus` 的 `books`/`staging` 事件 + `GET /staging` 列表里这条目的 `delivered.deliver`
    /// （[`sidecar::DeliverCheck`]）异步呈现；渲染自检计划、`mark_delivered` 全部在 `deliver` 内部
    /// 完成，不劳 HTTP 层操心。panic 由 `catch_unwind` 兜住，转成失败记录。
    pub fn spawn_deliver(&self, name: &str, folder: &str, mkdir: Arc<MkdirQueue>, bus: Arc<rmsvc_core::events::EventBus>) -> Result<(), String> {
        self.deliverable(name)?;
        let busy = self.busy_guard(name, "")?;
        let now = rmsvc_core::clock::now_secs();
        let _ = self.set_deliver_check(name, sidecar::DeliverCheck { status: "pending".into(), message: String::new(), at: now });
        let folder = folder.to_string();
        self.spawn_bg(name, busy, bus, move |this, name, bus| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.deliver(name, &folder, &mkdir)))
                .unwrap_or_else(|_| Err("落库过程内部异常（已捕获，不影响其他操作）".to_string()));
            let at = rmsvc_core::clock::now_secs();
            let (status, message) = match &result {
                Ok(o) => ("ok".to_string(), o.message.clone()),
                Err(e) => ("failed".to_string(), e.clone()),
            };
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

    /// 记一次落库（加入 xochitl）：写 sidecar `.<name>.delivered` 的 `native` 时间戳。
    pub fn mark_delivered(&self, name: &str) -> Result<(), String> {
        let now = rmsvc_core::clock::now_secs();
        self.update_sidecar(name, |d| d.native = Some(now))
    }
}
