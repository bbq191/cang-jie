//! 母版库的查 / 删 / 列表与启动期维护（孤儿边车、被中断记录恢复）。
use super::*;

/// 判定一本母版库文件的优化等级（`full`/`core`/`old`/`none`）与是否 PDF 转出的 EPUB——要开 zip / 读文件头尾，
/// 结果由 `Staging::list` 按（大小, 修改时间）缓存。
pub(super) fn probe_level(path: &Path, format: &str) -> (&'static str, bool) {
    let pdf_source = format == "epub" && bookconv::pdf_ingest::looks_like_pdf_derived_epub(path);
    let level = if format == "pdf" {
        if bookconv::convert::pdfwrite::looks_like_own_bookconv_pdf(path) { "full" } else { "none" }
    } else if pdf_source {
        "full"
    } else if format != "epub" {
        "none"
    } else {
        match path.to_str().and_then(optimize::optimized_version_file) {
            Some(v) if v == optimize::OPTIMIZE_VERSION => "full",
            Some(v) if v.ends_with("-core") => "core",
            Some(_) => "old",
            None => "none",
        }
    };
    (level, pdf_source)
}

#[derive(Clone)]
pub(super) struct ProbeCache {
    pub(super) len: u64,
    pub(super) modified: Option<std::time::SystemTime>,
    pub(super) level: &'static str,
    pub(super) pdf_source: bool,
}

impl Staging {
    /// 清理没有对应书的落库边车（`.<书名>.delivered`）：书早已删除/被外部清掉，边车成了孤儿。留着不仅占目录，
    /// 更会让**同名新书**误继承旧的"已加入/渲染"记录。返回清掉的个数。
    pub fn gc_orphan_sidecars(&self) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(book) = name.strip_prefix('.').and_then(|s| s.strip_suffix(".delivered")) else { continue };
            if !self.dir.join(book).is_file() && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }
    /// 启动时修正上一个进程被打断留下的状态（崩溃 / OOM / 被 systemd 杀 / 断电）：
    /// - 边车里停在 `pending` 的优化 / 落库记录 → 改成 `failed`（否则界面永远显示"处理中"，而实际早没有线程在跑）；
    /// - 渲染自检停在 `pending` → `timeout`（自检线程随进程没了；xochitl 可能延后渲染，打开一次就有页数）；
    /// - `.<书名>.optimizing.tmp` 半成品（点前缀，列表看不见，可达数百 MB）→ 删除。
    /// 只在启动时调用（此时不可能有操作在跑）。返回 (修正的记录数, 清掉的半成品数)。
    pub fn recover_interrupted(&self) -> (usize, usize) {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return (0, 0) };
        let now = rmsvc_core::clock::now_secs();
        let (mut fixed, mut tmps) = (0, 0);
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name.ends_with(".optimizing.tmp") {
                if std::fs::remove_file(e.path()).is_ok() {
                    tmps += 1;
                }
                continue;
            }
            let Some(book) = name.strip_prefix('.').and_then(|s| s.strip_suffix(".delivered")) else { continue };
            let book_path = self.dir.join(book);
            let stale = |st: &str| st == "pending";
            let Some(d) = sidecar::read(&book_path) else { continue };
            let needs = d.optimize.as_ref().is_some_and(|o| stale(&o.status))
                || d.deliver.as_ref().is_some_and(|o| stale(&o.status))
                || d.render.as_ref().is_some_and(|o| stale(&o.status));
            if !needs {
                continue;
            }
            let ok = sidecar::update(&book_path, |d| {
                if let Some(o) = d.optimize.as_mut().filter(|o| stale(&o.status)) {
                    *o = sidecar::OptimizeCheck { status: "failed".into(), message: "服务重启，上次优化被中断，可重新点「优化」".into(), at: now, progress: None };
                }
                if let Some(o) = d.deliver.as_mut().filter(|o| stale(&o.status)) {
                    *o = sidecar::DeliverCheck { status: "failed".into(), message: "服务重启，上次加入被中断，可重新加入".into(), at: now, progress: None };
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
    // ───────────── 查 / 删 ─────────────

    pub fn remove(&self, name: &str) -> Result<(), String> {
        if self.is_busy(name) {
            return Err(busy_err(name, "再删除"));
        }
        self.remove_unlocked(name)
    }

    /// 实际删除，不查忙锁——[`Self::remove`] 自己查完忙锁之后调这个真正干活；外部一律走
    /// [`Self::remove`]，不要绕过忙锁检查直接调这个。
    pub(super) fn remove_unlocked(&self, name: &str) -> Result<(), String> {
        let p = self.path_of(name)?;
        sidecar::remove(&p);
        std::fs::remove_file(&p).map_err(|e| format!("删除失败: {e}"))
    }

    /// 所在分区剩余空间（字节）。`df -k` 解析：表头后的所有行拍平成 token（设备名太长时 busybox 会把数字
    /// 换到下一行，真机 `/dev/mapper/home-encrypted-disk` 就这样），第 4 个 token = Available KB；解析不了 None。
    pub fn free_bytes(&self) -> Option<u64> {
        let out = std::process::Command::new("df").arg("-k").arg(&self.dir).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        let toks: Vec<&str> = s.lines().skip(1).flat_map(|l| l.split_whitespace()).collect();
        toks.get(3)?.parse::<u64>().ok().map(|kb| kb * 1024)
    }

    /// 列母版库，最新入库在前（同秒按名）。隐藏文件（sidecar / 半成品）不列。
    pub fn list(&self) -> Vec<StagingEntry> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        let mut seen = std::collections::HashSet::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(md) = e.metadata() else { continue };
            if name.starts_with('.') || !md.is_file() {
                continue;
            }
            let format = match formats::ext_of(&name).as_str() {
                "epub" => "epub",
                "pdf" => "pdf",
                "cbz" => "cbz",
                _ => "other",
            };
            // 优化状态对 EPUB 有意义；PDF 里"我们自己优化产出的产物"（漫画 EPUB 分卷投递的 PDF 件或入库 PDF 裁边）
            // 也算已优化（靠书签目录或 Producer 标记廉价识别，见 `pdfwrite.rs::looks_like_own_
            // bookconv_pdf` 文档注释——用户自己上传的原生 PDF 没有这俩标记，维持 none）。
            // 入库 PDF 转出来的 EPUB（`pdf_source`）视为一次性产物已经完成，直接报 full，不进
            // 常规文字 EPUB 那条 full/core/old/none 优化阶梯——真跑一遍 `optimized_version_file`
            // 只会白白报 none（这类 EPUB 从没被 `optimize_epub_file_streaming` 处理过，没有那个
            // 内嵌版本标记），误导前端以为它"未优化"、显示可以点「优化」，见 `StagingEntry::
            // pdf_source` 文档。
            let modified = md.modified().ok();
            let cached = self
                .probes
                .lock()
                .unwrap()
                .get(&name)
                .filter(|c| c.len == md.len() && c.modified == modified)
                .cloned();
            let (level, pdf_source) = match cached {
                Some(c) => (c.level, c.pdf_source),
                None => {
                    let (level, pdf_source) = probe_level(&e.path(), format);
                    crate::ops::lock(&self.probes).insert(name.clone(), ProbeCache { len: md.len(), modified, level, pdf_source });
                    (level, pdf_source)
                }
            };
            seen.insert(name.clone());
            let mtime = md.modified().ok().map(rmsvc_core::clock::secs_of).unwrap_or(0);
            let busy = self.is_busy(&name);
            let mut delivered = sidecar::read(&e.path());
            if let Some(rc) = delivered.as_mut().and_then(|d| d.render.as_mut()) {
                // 直接投入的 EPUB 首次打开才渲染：xochitl 渲染完会把 `.content` 的 pageCount 改成真页数，跟记录里的占位页数不同
                // 就说明已经渲染过 → 升级成 ok。没打开过 → 保持 onopen。
                if rc.status == "onopen" {
                    if let Some(n) = rmsvc_core::xochitl::page_count(self.xochitl.library_dir(), &rc.uuid) {
                        if n != rc.pages {
                            rc.status = "ok".into();
                            rc.pages = n;
                        }
                    }
                }
            }
            out.push(StagingEntry { name, bytes: md.len(), format, optimized: level == "full", level, mtime, delivered, busy, pdf_source });
        }
        // 已被删除/改名的条目从缓存清掉，避免缓存无限增长
        crate::ops::lock(&self.probes).retain(|k, _| seen.contains(k));
        out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
        out
    }
}
