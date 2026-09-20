//! 入库：新书落母版库（字节 / 已落盘暂存文件）与网文抓取。
use super::*;

pub(super) fn landed_name(p: &Path) -> String {
    p.file_name().and_then(|s| s.to_str()).unwrap_or("book").to_string()
}

impl Staging {
    // ───────────── 入库 ─────────────

    /// 新入库（字节）：原子写，同名加数字前缀不覆盖。返回落地文件名。
    pub fn stage_new(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        let target = unique_path(&self.dir, &canonical_staged_name(plain_name(name)?));
        sidecar::remove(&target); // 目标名是全新的，遗留的同名边车一定是旧书的，别让新书继承
        write_atomic(&target, bytes).map_err(|e| format!("写母版库失败: {e}"))?;
        Ok(landed_name(&target))
    }

    /// 新入库（已落盘的暂存文件）：同分区 rename 不拷贝（上传 / inbox 追平的大书走这里）。返回落地文件名。
    pub fn stage_from_path(&self, name: &str, src: &Path) -> Result<String, String> {
        let target = unique_path(&self.dir, &canonical_staged_name(plain_name(name)?));
        sidecar::remove(&target);
        if std::fs::rename(src, &target).is_err() {
            std::fs::copy(src, &target).map_err(|e| format!("写母版库失败: {e}"))?;
            let _ = std::fs::remove_file(src);
        }
        Ok(landed_name(&target))
    }

    /// 网文抓取（Readability + 白名单）→ 组 EPUB 落母版库。`optimize`＝网页「同步优化」复选框：请求了就紧接着
    /// 跑一遍跟「母版库→优化」按钮同一个 `optimize()`，不用用户再手动点一次——`article.rs` 的属性
    /// 白名单本来就不留 class/style，正文没有任何 CSS，不经 wash 层的边距/段距归零会在 xochitl 上按默认段距
    /// 渲染出大片留空（真机反馈）。`optimize=false` 保留原行为：core 级落库，用户按需再点。同步优化失败不影响
    /// 入库结果（已经抓到的文章不因为这一步失败就整个丢掉），失败原因原样带回给调用方决定怎么措辞。
    pub fn fetch_article(&self, url: &str, optimize: bool) -> Result<FetchArticleOutcome, String> {
        let (epub, title) = bookconv::article::build_article_epub(url)?;
        let fname = format!("{}.epub", bookconv::util::sanitize_filename(&title, "article"));
        let name = self.stage_new(&fname, &epub)?;
        let (optimized, optimize_error) = if optimize {
            match self.optimize(&name, |_, _| {}) {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e)),
            }
        } else {
            (false, None)
        };
        Ok(FetchArticleOutcome { name, title, optimized, optimize_error })
    }
}
