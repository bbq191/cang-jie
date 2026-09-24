//! 阅读方向查询（2026-09-24）：xochitl 阅读器里的 `reader-page-turn.qmd` 打开书时问
//! `GET /reading-direction/{uuid}` → `{"rtl": bool}`，为真就把左右滑动/点边缘翻页对调（日漫从右往左）。
//! 判据只有一个：书库里 `<uuid>.epub` 的 OPF `<spine page-progression-direction="rtl">`
//! （`bookconv::placeholder::epub_is_rtl`，只读两个 zip 条目）。不是 EPUB / 找不到 / 读不了一律 `false`
//! ——宁可按原生方向，也不误翻。按（大小, mtime）缓存，重复打开同一本书不再解 zip。
//!
//! **手动指定清单**（`$XDG_STATE_HOME/shelf/books/rtl-overrides.json`，uuid 字符串数组）：书里没写标记、但确实
//! 从右往左的书（calibre 转出的漫画大多不写）。2026-09-24 用户定："这次先手动指定，以后新传的书还是看书里自带的标记"
//! ——所以清单没有网页入口，只是给已经在设备上的那批书补一次；每次查询现读（文件很小）。
use rmsvc_core::fs::plain_name;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// 缓存条目上限：书库几十到几百本，超了整表清空重来即可（查询本身很便宜）。
const CACHE_MAX: usize = 1024;

pub struct ReadingDirection {
    lib: PathBuf,
    overrides: PathBuf,
    cache: Mutex<HashMap<String, (u64, Option<SystemTime>, bool)>>,
}

impl ReadingDirection {
    pub fn new(lib: &Path, overrides: &Path) -> ReadingDirection {
        ReadingDirection { lib: lib.to_path_buf(), overrides: overrides.to_path_buf(), cache: Mutex::new(HashMap::new()) }
    }

    /// 手动指定清单里有没有这本（文件缺失 / 解析不了 = 没有）。
    fn overridden(&self, uuid: &str) -> bool {
        std::fs::read(&self.overrides)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .is_some_and(|list| list.iter().any(|u| u == uuid))
    }

    /// uuid 非法（含路径分隔符等）→ `Err`；其余情况都给出答案。
    pub fn is_rtl(&self, uuid: &str) -> Result<bool, String> {
        let uuid = plain_name(uuid)?;
        if self.overridden(uuid) {
            return Ok(true);
        }
        let path = self.lib.join(format!("{uuid}.epub"));
        let Ok(md) = std::fs::metadata(&path) else { return Ok(false) };
        let key = (md.len(), md.modified().ok());
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&(len, mtime, rtl)) = cache.get(uuid) {
            if (len, mtime) == key {
                return Ok(rtl);
            }
        }
        let rtl = bookconv::placeholder::epub_is_rtl(&path);
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(uuid.to_string(), (key.0, key.1, rtl));
        Ok(rtl)
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
        let rd = ReadingDirection::new(t.path(), &t.path().join("rtl-overrides.json"));
        assert_eq!(rd.is_rtl("manga"), Ok(true));
        assert_eq!(rd.is_rtl("novel"), Ok(false));
        assert_eq!(rd.is_rtl("nope"), Ok(false), "书库里没有 = 按原生方向");
        assert!(rd.is_rtl("../x").is_err());
        // 缓存按（大小, mtime）失效：同名文件内容变了要重新判
        std::fs::remove_file(t.path().join("manga.epub")).unwrap();
        epub(t.path(), "manga", "<spine/>");
        let f = std::fs::File::options().write(true).open(t.path().join("manga.epub")).unwrap();
        f.set_modified(SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(rd.is_rtl("manga"), Ok(false));
    }

    /// 手动指定清单：书里没写标记也按从右往左；清单缺失或损坏不影响按书里标记判断。
    #[test]
    fn override_list_marks_books_without_spine_flag() {
        let t = tempfile::tempdir().unwrap();
        epub(t.path(), "ranma", "<spine/>");
        let list = t.path().join("rtl-overrides.json");
        let rd = ReadingDirection::new(t.path(), &list);
        assert_eq!(rd.is_rtl("ranma"), Ok(false));
        std::fs::write(&list, r#"["ranma"]"#).unwrap();
        assert_eq!(rd.is_rtl("ranma"), Ok(true), "清单现读，改了立即生效");
        std::fs::write(&list, "not json").unwrap();
        assert_eq!(rd.is_rtl("ranma"), Ok(false), "清单坏了当没有");
    }
}
