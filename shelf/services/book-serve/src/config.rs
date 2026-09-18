//! `$XDG_CONFIG_HOME/shelf/book.json`。缺省即可用；首启写出缺省文件供用户改。
use serde::{Deserialize, Serialize};
use rmsvc_core::paths::Paths;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BookConfig {
    /// 「投入原生书库」未指定文件夹时落进的书库文件夹（visibleName；找不到→书库根）。
    pub library_folder: String,
    /// xochitl web 主机（`/upload`）。
    pub xochitl_host: String,
    /// `/upload` 超时（大书处理慢；超时但已送达会被判 LikelyDelivered、绝不重试）。
    pub upload_timeout_secs: u64,
    /// 投原生的体积门（MB）：xochitl `/upload` 有硬上限（2026-09-19 真机 `curl` 直传二分法精确测出
    /// 边界——Content-Length ≤ 99,999,000 字节 200/继续处理，≥ 99,999,900 字节起 `HTTP 413 entity
    /// too large`，即整数 **100,000,000 字节（100MB 十进制）**；此前 150 是未验证过的猜测值，
    /// 「188MB 被拒、60MB 稳」这两个点都在，但中间这段从没真机测过，真机《镖人》11 卷里恰好有一卷
    /// ~96MB 落在这段"看着安全实际会炸"的区间，反复 `Connection reset by peer`/`Broken pipe`，
    /// 直到 curl 绕开 book-serve 直传才拿到干净的 413），超过就不发、直接回执指引分卷。90MB
    /// （94,371,840 字节）留够安全余量。0=不拦。
    pub native_upload_limit_mb: u64,
}

impl Default for BookConfig {
    fn default() -> Self {
        BookConfig { library_folder: "library".into(), xochitl_host: rmsvc_core::xochitl::DEFAULT_HOST.into(), upload_timeout_secs: 300, native_upload_limit_mb: 90 }
    }
}

impl BookConfig {
    pub fn load(paths: &Paths) -> BookConfig {
        rmsvc_core::config::load_or_seed(&paths.service_config("book"))
    }
    /// 体积门（字节）；0=不拦。
    pub fn native_upload_limit_bytes(&self) -> u64 {
        self.native_upload_limit_mb * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_json_fills_defaults_and_ignores_retired_keys() {
        // annotFolder：2026-09-19 随「加入原生书库 → 文件夹」改真实文件夹下拉候选一起退役
        // （见 rmsvc_core::xochitl::list_folders + service_state::status 的 xochitlFolders）。
        let c: BookConfig = serde_json::from_str(r#"{"libraryFolder":"books","comicMono":true,"optimizeDirectEpub":false,"annotFolder":"批注"}"#).unwrap();
        assert_eq!(c.library_folder, "books");
        assert_eq!(c.upload_timeout_secs, 300);
        assert_eq!(c.native_upload_limit_bytes(), 90 * 1024 * 1024);
    }
}
