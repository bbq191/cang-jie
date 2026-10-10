//! 落库：母版库的书加入 xochitl（整本 / 大文件通道）、渲染记录与落库记录。投进 xochitl 那一层（建文件夹、上传、认领、
//! 登记页边距）在 [`crate::delivery::XochitlDelivery`]，与直接导入共用。
use super::*;
use crate::jobs::Jobs;
use rmsvc_core::xochitl::{ClaimError, Folder};

/// 大文件通道投 EPUB 后等占位 `.content` 写出 pageCount 的上限与防抖（见 `try_deliver_direct`）。
const PLACEHOLDER_PAGES_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
const PLACEHOLDER_PAGES_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

impl Staging {
    // ───────────── 落库 ─────────────

    /// 加入 xochitl：纯复制原字节（书架不优化书，书应先在电脑上用 sheng-ren 优化好）。xochitl 只读 EPUB/PDF。`folder`
    /// 空＝书库根目录（2026-09-19 起没有"默认文件夹"配置），非空＝书库根下这个名字的文件夹（没有就建，见
    /// [`XochitlDelivery::ensure_folder`]）；母版库条目投完**永远保留**（2026-09-19 用户明确要求去掉"投完自动删除"这个功能——母版是可以
    /// 换设备重投的底本，不该被一次性动作悄悄清掉；要删由用户自己在列表里点
    /// 删除）。返回回执文案 + EPUB 的渲染自检计划（调用方起线程跑 `render_check::run`）。
    /// **同步、阻塞**——上传大书、等建文件夹都能到分钟级；HTTP 接口不该直接暴露这个方法，用 [`Self::spawn_deliver`] 排进后台作业队列。
    pub fn deliver(&self, name: &str, folder: &str) -> Result<DeliverOutcome, String> {
        let p = self.deliverable(name)?;
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let folder = self.delivery.ensure_folder(folder);
        if self.delivery.over_limit(size) {
            // 超限：走大文件通道（2026-09-20 用户要求突破上传限制，真机验证 PDF 154MB/EPUB 153MB 可行）——
            // 占位文档 + 磁盘上替换成真文件。本机没有 xochitl 书库目录（非设备环境）、超过 `MAX_DIRECT_BYTES`
            // 或造占位失败才拒绝。**不再按卷拆分**（2026-09-30 用户定移除；此前 EPUB 漫画按 NCX、PDF 按书签拆成
            // 若干份分别上传，作为大文件通道之后的回退）。
            if let Some(outcome) = self.try_deliver_direct(name, &p, size, &folder)? {
                return Ok(outcome);
            }
            return Err(format!("《{name}》{} MB 超过 xochitl 上传上限（{} MB），也走不了大文件通道（超过 1GB，或造不出占位文档），没有加入", size >> 20, self.delivery.native_limit() >> 20));
        }
        // 全程不把整本读进内存：上传是流式的（见 rmsvc_core::xochitl 文档）。
        if formats::ext_of(name) != "epub" {
            // PDF：没有渲染自检、不登记页边距，用不着 uuid，只上传。
            let delivery = self.delivery.upload(&p, name, &folder)?;
            let _ = self.mark_delivered(name);
            return Ok(DeliverOutcome { message: delivered_message(name, &delivery), render: None });
        }
        // EPUB：当场按字节认出这本书（与直接导入同一判据、同样的等待，见 `XochitlDelivery::upload_and_claim`），渲染自检线程拿着 uuid
        // 只等页数。2026-10-10 第二阶段前是上传前拍快照、交给自检线程在 10 分钟里慢慢认；母版在这期间被删 / 改名时没法比字节，
        // 只好退而按书名认。现在认领时忙锁还占着（删除 / 改名都被拦下），母版一定还在，书名兜底随之删掉。
        let comic_margins = shelf_conv::epub::Book::open(&p).ok().and_then(|mut b| b.reader_margins());
        match self.delivery.upload_and_claim(&p, name, &folder) {
            Ok(c) => {
                let _ = self.mark_delivered(name);
                Ok(DeliverOutcome { message: delivered_message(name, &c.delivery), render: Some(RenderPlan { name: name.to_string(), uuid: c.uuid, comic_margins }) })
            }
            Err(ClaimError::Upload(e)) => Err(e),
            // 书已经交给 xochitl 了，只是没在等待时限内认出来：落库照算成功（与以前"上传成功即成功"一致），渲染记成 timeout、
            // 不登记页边距（认不出就不认，绝不把别人的书当成自己的）。
            Err(ClaimError::NotFound { delivery, waited }) => {
                println!("[book-serve] 《{name}》已上传给 xochitl，但 {} 秒内没在书库里认出它（不认别人的书，也不登记页边距）", waited.as_secs());
                let _ = self.mark_delivered(name);
                let _ = self.set_render(name, RenderCheck { uuid: String::new(), pages: 0, status: RenderStatus::Timeout, at: rmsvc_core::clock::now_secs() });
                Ok(DeliverOutcome { message: delivered_message(name, &delivery), render: None })
            }
        }
    }

    /// 落库前的零耗时校验：xochitl 读得了的格式 + 书还在母版库。返回路径。
    fn deliverable(&self, name: &str) -> Result<PathBuf, String> {
        if !formats::has_ext(name, formats::NATIVE_EXTS) {
            return Err("xochitl 只读 EPUB / PDF".into());
        }
        self.existing(name)
    }

    /// 母版库这本书走大文件通道（见 [`XochitlDelivery::upload_large`]）并记好落库/渲染记录：成功 `Ok(Some)`；条件不满足 → `Ok(None)`，
    /// 调用方整本拒绝；占位已上传之后才出的错 → `Err`（不再退回拒绝，否则书库里会留下半成品占位）。
    /// `folder` 是目标文件夹（见 [`XochitlDelivery::ensure_folder`]）。
    fn try_deliver_direct(&self, name: &str, p: &Path, size: u64, folder: &Folder) -> Result<Option<DeliverOutcome>, String> {
        let Some(up) = self.delivery.upload_large(p, name, folder)? else { return Ok(None) };
        let _ = self.mark_delivered(name);
        // 渲染记录也写上，让"加入 xochitl"的书在列表里都有统一的渲染徽章：
        // - PDF：页数就是我们写进 `.content` 的真页数 → 直接 ok；
        // - EPUB：xochitl 要**首次打开**才渲染，此刻 `.content` 里是占位的页数。记 `onopen` + 占位页数，`list()` 之后每次
        //   读该文档 `.content` 的 pageCount，一变（用户打开过、xochitl 渲染完）就自动显示成真页数。
        //   占位页数必须真读到：要是 xochitl 还没把占位的 pageCount 写进 `.content` 就记成 0，之后列表一读到占位自己的
        //   页数（≠ 0）就会误判成"已渲染"。所以先等它出现（书库目录有变化才查，最多 PLACEHOLDER_PAGES_WAIT）；
        //   通常替换前复制大文件的那几秒里早就写好了，这里不会真等。
        let (pages, status) = match up.pages {
            Some(n) => (n as u64, RenderStatus::Ok),
            None => {
                let lib = self.delivery.library_dir();
                let n = rmsvc_core::fswatch::wait_for(lib, PLACEHOLDER_PAGES_DEBOUNCE, PLACEHOLDER_PAGES_WAIT, || rmsvc_core::xochitl::page_count(lib, &up.uuid));
                (n.unwrap_or(0), RenderStatus::Onopen)
            }
        };
        let _ = self.set_render(name, sidecar::RenderCheck { uuid: up.uuid, pages, status, at: rmsvc_core::clock::now_secs() });
        let stem = formats::stem_of(name);
        Ok(Some(DeliverOutcome {
            message: format!("《{stem}》{} MB 超过网页上传上限，已直接写入 xochitl 书库；首次打开需重新渲染，请稍候", size >> 20),
            render: None,
        }))
    }

    /// [`Self::deliver`] 的异步版：先做零耗时校验（格式/文件存在），校验过了才加忙锁、边车记 `pending`，把真正耗时的部分排进
    /// 后台作业队列（`jobs`，与直接导入同一个，一次一本——2026-10-10 前每次落库新起一个线程）。成功返回后 HTTP 层立即回"已开始"，
    /// 真正结果通过 `bus` 的 `books`/`staging` 事件 + `GET /staging` 列表里这条目的 `delivered.deliver`（[`sidecar::DeliverCheck`]）
    /// 异步呈现。排队期间忙锁一直占着（网页显示"处理中"，删除 / 改名 / 再次加入都被拦下）。渲染自检、`mark_delivered` 全部在
    /// 作业内部完成，不劳 HTTP 层操心；渲染自检要等最多 10 分钟，另起线程跑，不占着队列。panic 由 `catch_unwind` 兜住，转成失败记录。
    pub fn spawn_deliver(&self, name: &str, folder: &str, jobs: &Jobs, bus: Arc<rmsvc_core::events::EventBus>) -> Result<(), String> {
        self.deliverable(name)?;
        let busy = self.busy_guard(name, "")?;
        let now = rmsvc_core::clock::now_secs();
        let _ = self.set_deliver_check(name, sidecar::DeliverCheck { status: DeliverStatus::Pending, message: String::new(), at: now });
        let (this, owned_name, folder) = (self.clone(), name.to_string(), folder.to_string());
        let queued = jobs.run(Box::new(move || {
            let name = owned_name;
            // 落库本身的 panic 转成带原因的失败记录；外层再兜一次，保证无论如何都解锁、发事件。
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.deliver(&name, &folder)))
                    .unwrap_or_else(|_| Err("落库过程内部异常（已捕获，不影响其他操作）".to_string()));
                let at = rmsvc_core::clock::now_secs();
                let (status, message) = match &result {
                    Ok(o) => (DeliverStatus::Ok, o.message.clone()),
                    Err(e) => (DeliverStatus::Failed, e.clone()),
                };
                let _ = this.set_deliver_check(&name, sidecar::DeliverCheck { status, message, at });
                if let Ok(DeliverOutcome { render: Some(plan), .. }) = result {
                    let (staging2, bus2, lib2) = (this.clone(), bus.clone(), this.delivery.library_dir().to_path_buf());
                    std::thread::spawn(move || crate::render_check::run(&staging2, &bus2, &lib2, &plan));
                }
            }));
            drop(busy); // 先解锁再推事件：网页据事件重拉列表时这条已不是"处理中"
            bus.publish("books", "staging");
        }));
        if let Err(e) = queued {
            // 作业连同忙锁一起被丢弃了：把边车里的 pending 收成失败，别让网页一直显示"处理中"
            let _ = self.set_deliver_check(name, sidecar::DeliverCheck { status: DeliverStatus::Failed, message: e.clone(), at: now });
            return Err(e);
        }
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

/// 落库回执：xochitl 回了 2xx →"已加入"；读超时 / 408（很可能已送达，大书还在排版）→ 多一句"稍候刷新书库"。
fn delivered_message(name: &str, delivery: &Delivery) -> String {
    match delivery {
        Delivery::Delivered(_) => format!("已加入 xochitl《{name}》"),
        Delivery::LikelyDelivered(_) => format!("已加入 xochitl《{name}》（设备处理较慢，稍候刷新书库）"),
    }
}
