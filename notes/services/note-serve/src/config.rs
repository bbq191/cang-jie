//! `$XDG_CONFIG_HOME/notes/note.json`。缺省即可用；首启写出缺省文件供用户改。
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct NoteConfig {
    /// xochitl web 主机（`/upload`），与书架同一套设备事实（USB/WiFi 都常驻可达，见
    /// `shelf_core::xochitl::DEFAULT_HOST` 文档）。
    pub xochitl_host: String,
    /// `/upload` 超时；笔记本单页文档很小，不用给大书那么长。
    pub upload_timeout_secs: u64,
    /// 一本书的文件夹名规则：`{title}` 替换成书名。
    pub folder_name_pattern: String,
}

impl Default for NoteConfig {
    fn default() -> Self {
        NoteConfig { xochitl_host: shelf_core::xochitl::DEFAULT_HOST.into(), upload_timeout_secs: 60, folder_name_pattern: "《{title}》".into() }
    }
}

impl NoteConfig {
    pub fn folder_name(&self, title: &str) -> String {
        self.folder_name_pattern.replace("{title}", title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_folder_naming() {
        let c = NoteConfig::default();
        assert_eq!(c.xochitl_host, shelf_core::xochitl::DEFAULT_HOST);
        assert_eq!(c.folder_name("人骨拼图"), "《人骨拼图》");
        let partial: NoteConfig = serde_json::from_str(r#"{"uploadTimeoutSecs": 120}"#).unwrap();
        assert_eq!(partial.upload_timeout_secs, 120);
        assert_eq!(partial.xochitl_host, shelf_core::xochitl::DEFAULT_HOST, "没给的字段落缺省");
    }
}
