//! `$XDG_CONFIG_HOME/shelf/book.json`。缺省即可用；首启写出缺省文件供用户改。
use serde::{Deserialize, Serialize};
use shelf_core::paths::Paths;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BookConfig {
    /// `native` 目标落进的书库文件夹（visibleName；找不到→书库根）。
    pub library_folder: String,
    /// `annot` 目标（批注定稿 PDF）落进的文件夹。
    pub annot_folder: String,
    /// 直传 EPUB 缺优化标记时是否在设备端跑优化器（optimize=auto 的含义）。
    pub optimize_direct_epub: bool,
    /// 漫画 CBZ→PDF 黑白页转 1-bit 抖动（省刷新波形）。
    pub comic_mono: bool,
    /// xochitl web 主机（`/upload`）。
    pub xochitl_host: String,
    /// `/upload` 超时（大书处理慢；超时但已送达会被判 LikelyDelivered、绝不重试）。
    pub upload_timeout_secs: u64,
}

impl Default for BookConfig {
    fn default() -> Self {
        BookConfig {
            library_folder: "library".into(),
            annot_folder: "library".into(),
            optimize_direct_epub: true,
            comic_mono: false,
            xochitl_host: shelf_core::xochitl::DEFAULT_HOST.into(),
            upload_timeout_secs: 300,
        }
    }
}

impl BookConfig {
    pub fn load(paths: &Paths) -> BookConfig {
        let f = paths.service_config("book");
        match std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str::<BookConfig>(&t).ok()) {
            Some(c) => c,
            None => {
                let c = BookConfig::default();
                if !f.exists() {
                    let _ = std::fs::create_dir_all(paths.config_dir());
                    let _ = std::fs::write(&f, serde_json::to_string_pretty(&c).unwrap_or_default());
                }
                c
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_json_fills_defaults() {
        let c: BookConfig = serde_json::from_str(r#"{"libraryFolder":"books","comicMono":true}"#).unwrap();
        assert_eq!(c.library_folder, "books");
        assert!(c.comic_mono);
        assert!(c.optimize_direct_epub);
        assert_eq!(c.upload_timeout_secs, 300);
    }
}
