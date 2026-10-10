//! 母版库的查 / 删 / 列表与启动期维护（孤儿边车、被中断记录恢复）。
use super::*;
use rmsvc_core::cache::{FileStamp, StampCache};

/// `path` 所在文件系统的可用字节数（statvfs，见 [`rmsvc_core::fs::fs_space`]，网关「设备健康」同一份）。
pub(super) fn free_bytes_of(path: &Path) -> Option<u64> {
    rmsvc_core::fs::fs_space(path).map(|(free, _)| free)
}

/// 母版库所在分区"空间不足"的阈值：剩余低于 300 MiB 时网页标红提醒（`GET /staging` 的 `lowSpace`）。
/// 判断放在服务端当单一事实源，此前阈值只写死在网页里。
pub const LOW_SPACE_BYTES: u64 = 300 << 20;

/// 剩余空间是否低于 [`LOW_SPACE_BYTES`]（严格小于）；查不到空间（`None`）不算不足。
pub fn low_space(free: Option<u64>) -> bool {
    free.is_some_and(|f| f < LOW_SPACE_BYTES)
}

/// `list()` 的两份按文件戳失效的缓存（见 [`StampCache`]）。网页每收到一条母版库事件就重拉一次列表，每本书每次都读边车、
/// 读 xochitl 的 `.content`，书一多就是持续的读盘和 CPU（电池）；文件没变时这些结论都不会变。
pub(super) struct ListCaches {
    /// 书名 → 落库边车内容，按边车文件的戳失效（边车都是原子写，每次改写换 inode）。
    pub(super) sidecars: StampCache<Option<Delivered>>,
    /// 文档 uuid → xochitl `.content` 里的页数，按 `.content` 的戳失效。只查"首次打开才渲染"（onopen）的书：
    /// 用户一直没打开的那些，此前每次列表都要把它的 `.content`（逐页表，大书几十 KB）读一遍解析一遍。
    pub(super) pages: StampCache<Option<u64>>,
}

/// 缓存条目上限（母版库几十到几百本；超了整表清空重来）。
const LIST_CACHE_CAP: usize = 4096;

impl Default for ListCaches {
    fn default() -> Self {
        ListCaches { sidecars: StampCache::new(LIST_CACHE_CAP), pages: StampCache::new(LIST_CACHE_CAP) }
    }
}

impl Staging {
    /// 清理没有对应书的落库边车（`.<书名>.delivered`，长书名是短名形式）：书早已删除/被外部清掉，边车成了孤儿。留着不仅占目录，
    /// 更会让**同名新书**误继承旧的"已加入/渲染"记录。返回清掉的个数。
    ///
    /// "是不是孤儿"按 [`sidecar::owners`] 反查（短名形式的边车没法从文件名反推书名）；删之前再查一次最新的反查表，
    /// 不误删两次查询之间刚入库的书的边车。
    pub fn gc_orphan_sidecars(&self) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let owners = sidecar::owners(&self.dir);
        let candidates: Vec<_> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| sidecar::is_sidecar_name(name) && !owners.contains_key(name))
            .collect();
        if candidates.is_empty() {
            return 0;
        }
        let owners = sidecar::owners(&self.dir);
        candidates.into_iter().filter(|name| !owners.contains_key(name) && std::fs::remove_file(self.dir.join(name)).is_ok()).count()
    }
    /// 启动时修正上一个进程被打断留下的状态（崩溃 / OOM / 被 systemd 杀 / 断电）：
    /// - 边车里停在 `pending` 的落库记录 → 改成 `failed`（否则界面永远显示"处理中"，而实际早没有线程在跑）；
    /// - 渲染自检停在 `pending` → `timeout`（自检线程随进程没了；xochitl 可能延后渲染，打开一次就有页数）；
    /// - 点前缀的 `*.tmp`（跨分区入库的中转、旧版留下的优化半成品——可达数百 MB；以及边车原子写没来得及改名的
    ///   `.<书名>.delivered.<pid>.<序号>.tmp`）→ 删除。这些都只可能是本服务写的，启动时没有操作在跑，全清安全。
    ///
    /// 只在启动时调用（此时不可能有操作在跑）。返回 (修正的记录数, 清掉的半成品数)。
    pub fn recover_interrupted(&self) -> (usize, usize) {
        // 入库中转是 `.<pid>.<序号>.landing.tmp`（`ScratchFile`）；2026-10-07 前的优化半成品（`.<书名>.optimizing.tmp`）也按后缀认。
        let tmps = rmsvc_core::fs::clean_dir(&self.dir, |n| n.starts_with('.') && n.ends_with(".tmp"));
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return (0, tmps) };
        let owners = sidecar::owners(&self.dir);
        let now = rmsvc_core::clock::now_secs();
        let mut fixed = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // 边车 → 书按反查表认（短名形式没法从文件名反推书名）；没有书的孤儿边车不修，留给 `gc_orphan_sidecars` 清。
            let Some(book_path) = sidecar::is_sidecar_name(&name).then(|| owners.get(&name)).flatten() else { continue };
            let stale = |st: &str| st == "pending";
            let Some(d) = sidecar::read(book_path) else { continue };
            let needs = d.deliver.as_ref().is_some_and(|o| stale(&o.status))
                || d.render.as_ref().is_some_and(|o| stale(&o.status));
            if !needs {
                continue;
            }
            let ok = sidecar::update(book_path, |d| {
                if let Some(o) = d.deliver.as_mut().filter(|o| stale(&o.status)) {
                    *o = sidecar::DeliverCheck { status: "failed".into(), message: "服务重启，上次加入被中断，可重新加入".into(), at: now };
                }
                if let Some(r) = d.render.as_mut().filter(|r| stale(&r.status)) {
                    r.status = "timeout".into();
                    r.at = now;
                }
            })
            .is_ok();
            if ok {
                fixed += 1;
            }
        }
        (fixed, tmps)
    }
    /// 母版库条目改名：只改文件名（不改书内的书名/作者），格式不能变——新名字不带扩展名就沿用原扩展名，
    /// 带了别的扩展名则拒绝。新名已存在 / 任一名字正在处理中 → 拒绝。落库边车跟着改名。返回新名字。
    pub fn rename(&self, name: &str, new_name: &str) -> Result<String, String> {
        let src = self.existing(name)?;
        let ext = formats::ext_of(name);
        let new_name = new_name.trim();
        let new_name = if formats::ext_of(new_name) == ext { new_name.to_string() } else { format!("{new_name}.{ext}") };
        let stem_ok = new_name.strip_suffix(&format!(".{ext}")).is_some_and(|s| !s.trim().is_empty());
        if !stem_ok {
            return Err("新名字不能为空".into());
        }
        let dst = self.path_of(&new_name)?;
        if new_name == name {
            return Ok(new_name);
        }
        let _busy = self.busy_guard(name, "再改名")?;
        let _busy_new = self.busy_guard(&new_name, "再改名")?;
        let _land = self.land_guard();
        if dst.exists() {
            return Err(format!("母版库里已有《{new_name}》"));
        }
        std::fs::rename(&src, &dst).map_err(|e| format!("改名失败: {e}"))?;
        sidecar::rename(&src, &dst);
        // 渲染自检线程按旧书名记结果（落库完忙锁就放了，自检还要再等最多 10 分钟）：改名后它找不到书、不再写，边车会一直停在
        // "渲染中"直到下次重启。这里直接收成 timeout（列表显示"未见渲染"，不影响阅读）。
        if sidecar::read(&dst).and_then(|d| d.render).is_some_and(|r| r.status == "pending") {
            let now = rmsvc_core::clock::now_secs();
            let _ = sidecar::update(&dst, |d| {
                if let Some(r) = d.render.as_mut().filter(|r| r.status == "pending") {
                    r.status = "timeout".into();
                    r.at = now;
                }
            });
        }
        Ok(new_name)
    }

    /// 打开母版库条目供下载：`(文件, 字节数)`。
    pub fn open_for_download(&self, name: &str) -> Result<(std::fs::File, u64), String> {
        let p = self.existing(name)?;
        let f = std::fs::File::open(&p).map_err(|e| format!("打开失败: {e}"))?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        Ok((f, len))
    }
    // ───────────── 查 / 删 ─────────────

    /// 删除一本书（连同落库边车）。删除期间**占着忙锁**：此前只是先查"忙不忙"再删，查完到删之间别的请求可能刚好
    /// 开始落库/改名，后台线程随即对着一个已删的文件跑（2026-09-25 第四轮审计）。
    pub fn remove(&self, name: &str) -> Result<(), String> {
        let p = self.path_of(name)?;
        let _busy = self.busy_guard(name, "再删除")?;
        sidecar::remove(&p);
        std::fs::remove_file(&p).map_err(|e| format!("删除失败: {e}"))
    }

    /// 所在分区剩余空间（字节，非特权用户可用的那部分，与 `df` 的 Available 同义）；查不到 None。
    /// 走 `statvfs(2)`：`GET /staging` 每次网页刷新都调，此前每次 fork+exec 一个 `df -k` 再解析文本
    /// （busybox 设备名过长时还要拍平换行的输出），现在一次系统调用，无子进程。
    pub fn free_bytes(&self) -> Option<u64> {
        free_bytes_of(&self.dir)
    }

    /// 读这本书的落库边车（经 [`ListCaches::sidecars`]：边车没变就不再开文件）。没有边车 → `None`。
    fn read_sidecar(&self, name: &str, book: &Path) -> Option<Delivered> {
        let stamp = FileStamp::read(&sidecar::path_for(book))?;
        self.caches.sidecars.get_or(name, stamp, || sidecar::read(book))
    }

    /// xochitl 书库里这份文档 `.content` 的页数（经 [`ListCaches::pages`]）。
    fn content_pages(&self, uuid: &str) -> Option<u64> {
        let lib = self.delivery.library_dir();
        let stamp = FileStamp::read(&lib.join(format!("{uuid}.content")))?;
        self.caches.pages.get_or(uuid, stamp, || rmsvc_core::xochitl::page_count(lib, uuid))
    }

    /// 列母版库，最新入库在前（同秒按名）。隐藏文件（sidecar / 半成品）不列。
    pub fn list(&self) -> Vec<StagingEntry> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        let mut seen = std::collections::HashSet::new();
        let mut seen_docs = std::collections::HashSet::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(md) = e.metadata() else { continue };
            if name.starts_with('.') || !md.is_file() {
                continue;
            }
            let ext = formats::ext_of(&name);
            let format = formats::NATIVE_EXTS.iter().copied().find(|e| *e == ext).unwrap_or("other");
            seen.insert(name.clone());
            let mtime = md.modified().ok().map(rmsvc_core::clock::secs_of).unwrap_or(0);
            let busy = self.is_busy(&name);
            let mut delivered = self.read_sidecar(&name, &e.path());
            if let Some(rc) = delivered.as_mut().and_then(|d| d.render.as_mut()) {
                // 直接投入的 EPUB 首次打开才渲染：xochitl 渲染完会把 `.content` 的 pageCount 改成真页数，跟记录里的占位页数不同
                // 就说明已经渲染过 → 升级成 ok。没打开过 → 保持 onopen。
                if rc.status == "onopen" {
                    seen_docs.insert(rc.uuid.clone());
                    if let Some(n) = self.content_pages(&rc.uuid) {
                        // 记录里是 0 = 投递时没等到占位页数（极少见）：这时 `.content` 里的数可能还是占位的，不能当已渲染；
                        // 改看 `.epubindex`——大文件通道替换时删掉了它，重新出现只能是 xochitl 渲染了真书。
                        let rendered = rc.pages != 0 || self.delivery.library_dir().join(format!("{}.epubindex", rc.uuid)).exists();
                        if n != rc.pages && rendered {
                            rc.status = "ok".into();
                            rc.pages = n;
                            // 升级结果写回边车：此前只改返回给网页的这份拷贝，边车里永远是 onopen，于是之后每次列表
                            // （网页每收一条事件就拉一次）都要再去 xochitl 书库读一遍这本的 `.content`，读到天荒地老。
                            let uuid = rc.uuid.clone();
                            let _ = sidecar::update(&e.path(), |d| {
                                if let Some(r) = d.render.as_mut().filter(|r| r.status == "onopen" && r.uuid == uuid) {
                                    r.status = "ok".into();
                                    r.pages = n;
                                }
                            });
                        }
                    }
                }
            }
            out.push(StagingEntry {
                name,
                bytes: md.len(),
                format,
                mtime,
                delivered,
                busy,
            });
        }
        // 已被删除/改名的条目从缓存清掉，避免缓存无限增长
        self.caches.sidecars.retain(|k| seen.contains(k));
        self.caches.pages.retain(|k| seen_docs.contains(k));
        out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
        out
    }
}
