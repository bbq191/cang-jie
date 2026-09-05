//! `$XDG_CONFIG_HOME/shelf/book.json`。缺省即可用；首启写出缺省文件供用户改。
use serde::{Deserialize, Serialize};
use shelf_core::paths::Paths;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BookConfig {
    /// 「投入原生书库」未指定文件夹时落进的书库文件夹（visibleName；找不到→书库根）。
    pub library_folder: String,
    /// 网页「批注文件夹」预设对应的文件夹（PDF 手写定稿）。
    pub annot_folder: String,
    /// xochitl web 主机（`/upload`）。
    pub xochitl_host: String,
    /// `/upload` 超时（大书处理慢；超时但已送达会被判 LikelyDelivered、绝不重试）。
    pub upload_timeout_secs: u64,
}

impl Default for BookConfig {
    fn default() -> Self {
        BookConfig { library_folder: "library".into(), annot_folder: "library".into(), xochitl_host: shelf_core::xochitl::DEFAULT_HOST.into(), upload_timeout_secs: 300 }
    }
}

impl BookConfig {
    pub fn load(paths: &Paths) -> BookConfig {
        shelf_core::config::load_or_seed(&paths.service_config("book"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_json_fills_defaults_and_ignores_retired_keys() {
        let c: BookConfig = serde_json::from_str(r#"{"libraryFolder":"books","comicMono":true,"optimizeDirectEpub":false}"#).unwrap();
        assert_eq!(c.library_folder, "books");
        assert_eq!(c.upload_timeout_secs, 300);
    }
}
