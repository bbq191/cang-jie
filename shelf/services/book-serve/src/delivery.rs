//! 把一份文件投进 xochitl 之后**认出它是哪份文档**（2026-10-10，审计 X-1）。xochitl 的 `/upload` 不回 uuid，只能事后在书库里找：
//! 上传前给书库拍个快照（[`Claim::snapshot`]：这一刻已经在的近期文档），上传后找"快照里没有、新出现、`<uuid>.epub`
//! 与上传的字节逐字节相同"的那份（[`Claim::find`]）。直接导入（`import.rs`）与母版库落库后的渲染自检（`render_check.rs`）共用。
//!
//! 此前两处判据不一样：直接导入按字节认；渲染自检按"投书时刻之后进库 + 书名相符，都不符时取最新一本"认——并发投递时
//! 会把别人刚投的书认成自己的，漫画还会把页边距登记到别人的书上。
use rmsvc_core::fs::{same_content, Content};
use rmsvc_core::xochitl::{find_documents_since, DocInfo};
use std::collections::HashSet;
use std::path::Path;

/// 上传前拍的快照：候选只看 `createdTime >= since_ms` 的文档，`before` 里的（上传前就在的）不算。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Claim {
    pub since_ms: u64,
    pub before: HashSet<String>,
}

impl Claim {
    /// 上传前调。时刻往前留 2 秒：xochitl 的 `createdTime` 与本机时钟取整不同步，宁可多圈几份候选（`before` 和逐字节比对会排除）。
    pub fn snapshot(lib: &Path) -> Claim {
        let since_ms = rmsvc_core::clock::now_ms().saturating_sub(2_000);
        let before = find_documents_since(lib, since_ms).into_iter().map(|d| d.uuid).collect();
        Claim { since_ms, before }
    }

    /// 快照之后新出现的文档（新→旧）。
    pub fn fresh(&self, lib: &Path) -> Vec<DocInfo> {
        find_documents_since(lib, self.since_ms).into_iter().filter(|d| !self.before.contains(&d.uuid)).collect()
    }

    /// 新出现、`<uuid>.epub` 与 `part` 逐字节相同的那份（先比大小，一样才读）；还没有 → `None`。
    pub fn find(&self, lib: &Path, part: &Path) -> Option<String> {
        self.fresh(lib).into_iter().find(|d| same_content(&lib.join(format!("{}.epub", d.uuid)), Content::File(part))).map(|d| d.uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(lib: &Path, uuid: &str, bytes: &[u8]) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"x","parent":"","createdTime":"{}"}}"#, rmsvc_core::clock::now_ms())).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), bytes).unwrap();
    }

    /// 快照前就在的（哪怕字节相同）不认；快照后新出现的只认字节相同的那份。
    #[test]
    fn claims_only_new_byte_identical_document() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        let part = t.path().join("a.epub");
        std::fs::write(&part, b"PK-ours").unwrap();
        doc(&lib, "old-same", b"PK-ours");
        let c = Claim::snapshot(&lib);
        assert!(c.before.contains("old-same"));
        assert_eq!(c.find(&lib, &part), None, "上传前就在的不认");
        doc(&lib, "new-other", b"PK-them");
        assert_eq!(c.find(&lib, &part), None, "别人的新书不认");
        doc(&lib, "new-ours", b"PK-ours");
        assert_eq!(c.find(&lib, &part).as_deref(), Some("new-ours"));
    }
}
