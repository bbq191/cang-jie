//! 入库：新书落母版库（字节 / 已落盘暂存文件）。
use super::*;

pub(super) fn landed_name(p: &Path) -> String {
    p.file_name().and_then(|s| s.to_str()).unwrap_or("book").to_string()
}

impl Staging {
    // ───────────── 入库 ─────────────

    /// 同名的书已在母版库、且内容逐字节相同 → 返回它的名字（不再落一份 `1_书名`）。
    /// 2026-09-28：同一本书传了两次，母版库里出现「東野圭吾《白夜行》.epub」和「1_東野圭吾《白夜行》.epub」，
    /// 两份都被加入 KOReader 和 xochitl。只认**完全相同**的内容：同名不同内容（比如另一个版本，或 sheng-ren 重新优化过）
    /// 仍按原规则加数字前缀，不覆盖、不丢。调用方须持落名锁。
    fn identical_existing(&self, canon: &str, incoming: Content<'_>) -> Option<String> {
        let existing = self.dir.join(canon);
        same_content(&existing, incoming).then(|| canon.to_string())
    }

    /// 新入库（字节）：原子写，同名加数字前缀不覆盖（内容完全相同则认已有那本，见 [`Self::identical_existing`]）。返回落地文件名。
    #[cfg(test)]
    pub fn stage_new(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        let _land = self.land_guard();
        let canon = canonical_staged_name(plain_name(name)?);
        if let Some(same) = self.identical_existing(&canon, Content::Bytes(bytes)) {
            return Ok(same);
        }
        let target = unique_path(&self.dir, &canon);
        sidecar::remove(&target); // 目标名是全新的，遗留的同名边车一定是旧书的，别让新书继承
        rmsvc_core::fs::write_atomic(&target, bytes).map_err(|e| format!("写母版库失败: {e}"))?;
        Ok(landed_name(&target))
    }

    /// 新入库（已落盘的暂存文件）：同分区 rename 不拷贝（上传 / inbox 追平的大书走这里）。返回落地文件名。
    ///
    /// 跨分区（rename 失败）时先在临界区**外**把字节拷进母版库目录下的点前缀临时文件，再回临界区挑名、同目录 rename 落地
    /// （2026-09-25 第四轮审计）：此前直接往最终名上 `copy`——拷贝期间一本半截的书已经出现在列表里、可被落库/下载，
    /// 拷贝失败还留下这半截；而且整段拷贝（上百 MB）都攥着落名锁，别的入库全被卡住。
    pub fn stage_from_path(&self, name: &str, src: &Path) -> Result<String, String> {
        let canon = canonical_staged_name(plain_name(name)?);
        {
            let _land = self.land_guard();
            if let Some(same) = self.identical_existing(&canon, Content::File(src)) {
                let _ = std::fs::remove_file(src);
                return Ok(same);
            }
            let target = unique_path(&self.dir, &canon);
            sidecar::remove(&target);
            if std::fs::rename(src, &target).is_ok() {
                return Ok(landed_name(&target));
            }
        }
        // 中转文件出错/中途 panic 都由 `ScratchFile` 的 Drop 清掉；成功时它已被 rename 成正式名。
        let tmp = self.scratch();
        std::fs::copy(src, tmp.path()).map_err(|e| format!("写母版库失败: {e}"))?;
        let landed = {
            let _land = self.land_guard();
            let target = unique_path(&self.dir, &canon);
            sidecar::remove(&target);
            std::fs::rename(tmp.path(), &target).map(|_| landed_name(&target)).map_err(|e| format!("写母版库失败: {e}"))?
        };
        let _ = std::fs::remove_file(src);
        Ok(landed)
    }
}
