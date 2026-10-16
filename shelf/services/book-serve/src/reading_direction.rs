//! 阅读方向查询：xochitl 阅读器里的 `reader-page-turn.qmd` 打开书时问 `GET /reading-direction/{uuid}` → `{"rtl": bool}`，
//! 为真就把这本书的翻页对调（左滑/右箭头/点右边缘 = 上一页），日漫等从右往左的书按书的方向翻。
//!
//! 判据只有书自己：书库里 `<uuid>.epub` 的 OPF `<spine page-progression-direction="rtl">`（[`shelf_conv::epub::Book::is_rtl`]，
//! 只读 container.xml 和 OPF）。不是 EPUB / 找不到 / 读不了一律 `false`——宁可按原生方向，也不误翻。没有开关、没有手动清单
//! （2026-10-16 用户定"由书籍自己决定翻页模式"；10-07 删掉的旧版带开关和 `rtl-overrides.json`，都不加回）。
//! 按（大小, mtime）缓存，重复打开同一本书不再解 zip；sheng-ren 原地替换书后戳变了会重新判。
use rmsvc_core::cache::{FileStamp, StampCache};
use rmsvc_core::fs::plain_name;
use std::path::{Path, PathBuf};

/// 缓存条目上限：书库几十到几百本，超了整表清空重来即可（查询本身很便宜）。
const CACHE_MAX: usize = 1024;

pub struct ReadingDirection {
    lib: PathBuf,
    /// uuid → 是否从右往左，按书库里那份 epub 的戳失效（解 zip 不持锁，结果幂等，偶尔重复算一次无妨）。
    cache: StampCache<bool>,
}

impl ReadingDirection {
    pub fn new(lib: &Path) -> ReadingDirection {
        ReadingDirection { lib: lib.to_path_buf(), cache: StampCache::new(CACHE_MAX) }
    }

    /// uuid 非法（含路径分隔符等）→ `Err`；其余情况都给出答案。
    pub fn is_rtl(&self, uuid: &str) -> Result<bool, String> {
        let uuid = plain_name(uuid)?;
        let path = self.lib.join(format!("{uuid}.epub"));
        let Some(stamp) = FileStamp::read(&path) else { return Ok(false) };
        Ok(self.cache.get_or(uuid, stamp, || shelf_conv::epub::Book::open(&path).is_ok_and(|b| b.is_rtl())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn epub(dir: &Path, uuid: &str, spine: &str) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join(format!("{uuid}.epub"))).unwrap());
        let o: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        z.start_file("META-INF/container.xml", o).unwrap();
        z.write_all(br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#).unwrap();
        z.start_file("content.opf", o).unwrap();
        z.write_all(format!("<package>{spine}</package>").as_bytes()).unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn answers_rtl_ltr_missing_and_rejects_bad_uuid() {
        let t = tempfile::tempdir().unwrap();
        epub(t.path(), "manga", r#"<spine page-progression-direction="rtl"/>"#);
        epub(t.path(), "novel", "<spine/>");
        std::fs::write(t.path().join("pdf.pdf"), b"%PDF").unwrap();
        std::fs::write(t.path().join("bad.epub"), b"not a zip").unwrap();
        let rd = ReadingDirection::new(t.path());
        assert_eq!(rd.is_rtl("manga"), Ok(true));
        assert_eq!(rd.is_rtl("novel"), Ok(false));
        assert_eq!(rd.is_rtl("pdf"), Ok(false), "不是 EPUB = 按原生方向");
        assert_eq!(rd.is_rtl("bad"), Ok(false));
        assert_eq!(rd.is_rtl("nope"), Ok(false), "书库里没有 = 按原生方向");
        assert!(rd.is_rtl("../x").is_err());
        // 缓存按（大小, mtime）失效：同名文件内容变了要重新判
        std::fs::remove_file(t.path().join("manga.epub")).unwrap();
        epub(t.path(), "manga", "<spine/>");
        let f = std::fs::File::options().write(true).open(t.path().join("manga.epub")).unwrap();
        f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(rd.is_rtl("manga"), Ok(false));
    }
}
