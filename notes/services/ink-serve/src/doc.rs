//! xochitl 书库里一份文档的**只读**视图：`<uuid>.metadata`（名字/类型/回收站）、`<uuid>.content`（fileType、`pages` 页 id 表）、
//! `<uuid>/<page>.rm`（有手写/勾画的页才有文件）、`<uuid>.epubindex` + `<uuid>.epub`（页→章）。
//! （`<uuid>.thumbnails/` 缩略图已不读：裁图改自渲染，见 `crop.rs`。）
//! 绝不写书库目录（xochitl 不认外部改动，且 metadata 含凭证以外的隐私）。
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `.metadata` 的强类型视图与"活文档"判据用共享底座那份（书架/网关同一套）。
pub use rmsvc_core::xochitl::Metadata;

#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Content {
    #[serde(rename = "fileType")]
    pub file_type: String,
    /// 页 id 顺序表（0-based 下标 = 页号，与 .epubindex 起始页对齐）。`formatVersion 1` 的形状（真机 3.28 上的 EPUB 样本就是它）。
    pub pages: Vec<String>,
    /// `formatVersion 2` 的页表（笔记本是这个形状）：没有 `pages` 时用它，见 [`Content::page_ids`]。按原始 JSON 收，
    /// 形状对不上也不影响整份 `.content` 的解析（v1 的结果绝不能因为这个兜底字段变差）。
    #[serde(rename = "cPages")]
    c_pages: serde_json::Value,
}

impl Content {
    /// 按页序排列的页 id。有 `pages` 就用它；没有则取 `cPages.pages`：按 `idx`（分数索引字符串，字典序即页序）排、
    /// 去掉已删除的页。此前只认 `pages`，遇到 v2 形状的 `.content` 每页都会落到页号 0、整本书的条目都没有章、两处投影都不收
    /// ——目前真机样本里的 EPUB 都还是 v1，这是兜底，v1 的结果不变。
    pub fn page_ids(&self) -> Vec<&str> {
        if !self.pages.is_empty() {
            return self.pages.iter().map(String::as_str).collect();
        }
        let Some(pages) = self.c_pages.get("pages").and_then(|v| v.as_array()) else { return vec![] };
        let mut v: Vec<(&str, &str)> = pages
            .iter()
            .filter(|p| p.pointer("/deleted/value").and_then(|d| d.as_i64()).unwrap_or(0) == 0)
            .filter_map(|p| Some((p.pointer("/idx/value").and_then(|x| x.as_str()).unwrap_or(""), p.get("id")?.as_str().filter(|id| !id.is_empty())?)))
            .collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v.into_iter().map(|(_, id)| id).collect()
    }
}

/// 一份文档的路径集合。
pub struct Doc {
    pub uuid: String,
    lib: PathBuf,
}

impl Doc {
    pub fn new(lib: &Path, uuid: &str) -> Doc {
        Doc { uuid: uuid.to_string(), lib: lib.to_path_buf() }
    }
    fn side(&self, ext: &str) -> PathBuf {
        self.lib.join(format!("{}.{ext}", self.uuid))
    }
    pub fn metadata(&self) -> Option<Metadata> {
        self.read_metadata().ok().flatten()
    }
    /// 区分"没有 `.metadata`"（`Ok(None)`：书被彻底删了）与"读不了/解析失败"（`Err`：可能正被 xochitl 改写、
    /// 或格式不认识）。摄取拿前者当"书没了"去撤销条目；后者只能跳过这次，不能当成书没了——此前两者都是 `None`，
    /// 一次读到半截的 `.metadata` 就会把整本书的活条目全标 `Revoked`（2026-09-25 第四轮审计）。
    pub fn read_metadata(&self) -> Result<Option<Metadata>, String> {
        rmsvc_core::xochitl::read_meta(&self.lib, &self.uuid).map_err(|e| format!("{e}（先跳过，下次再试）"))
    }
    pub fn content(&self) -> Option<Content> {
        serde_json::from_str(&std::fs::read_to_string(self.side("content")).ok()?).ok()
    }
    /// 只取 `.content` 的 `fileType`（流式，不把整张页 id 表解析成 `Vec<String>`）；缺这个字段 → `None`。
    pub fn file_type(&self) -> Option<String> {
        rmsvc_core::xochitl::file_type(&self.lib, &self.uuid)
    }
    /// 打开 `.epub`（带缓冲、可 seek）：页→章只要目录那一两个 zip 条目，不整本读进内存。
    pub fn epub_file(&self) -> Option<std::io::BufReader<std::fs::File>> {
        std::fs::File::open(self.side("epub")).ok().map(std::io::BufReader::new)
    }
    pub fn epubindex_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.side("epubindex")).ok()
    }
    pub fn page_rm(&self, page_id: &str) -> PathBuf {
        self.lib.join(&self.uuid).join(format!("{page_id}.rm"))
    }
    /// 有没有任何 `.rm` 页（找到第一个就停，不逐个 stat）。
    pub fn has_annotated_pages(&self) -> bool {
        std::fs::read_dir(self.lib.join(&self.uuid)).map(|rd| rd.flatten().any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rm"))).unwrap_or(false)
    }
    /// 有 `.rm` 的页：(页 id, mtime 秒)。
    pub fn annotated_pages(&self) -> Vec<(String, u64)> {
        let Ok(rd) = std::fs::read_dir(self.lib.join(&self.uuid)) else { return vec![] };
        let mut out: Vec<(String, u64)> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("rm") {
                    return None;
                }
                let id = p.file_stem()?.to_str()?.to_string();
                let mtime = e.metadata().ok()?.modified().ok().map(rmsvc_core::clock::secs_of).unwrap_or(0);
                Some((id, mtime))
            })
            .collect();
        out.sort();
        out
    }
}

/// 从书库目录事件文件名里认出文档 uuid（`<uuid>.content` / `<uuid>.metadata` / `<uuid>`）。
pub fn uuid_of_event(name: &str) -> Option<&str> {
    let stem = name.split('.').next()?;
    rmsvc_core::xochitl::is_uuid_shape(stem).then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_content_and_metadata_shapes() {
        let c: Content = serde_json::from_str(include_str!("../../../testdata/renggu/book.content")).unwrap();
        assert_eq!((c.file_type.as_str(), c.pages.len()), ("epub", 523));
        assert_eq!(c.pages[0], "f02e9084-d864-46f3-a07e-ae84ba19d344");
        assert_eq!(c.page_ids().len(), 523, "v1 形状照旧用 pages");
        let v2: Content = serde_json::from_str(r#"{"fileType":"epub","formatVersion":2,"cPages":{"pages":[{"id":"c","idx":{"timestamp":"1:2","value":"bc"}},{"id":"a","idx":{"timestamp":"1:2","value":"ba"}},{"id":"x","idx":{"timestamp":"1:2","value":"bb"},"deleted":{"timestamp":"1:3","value":1}},{"id":"b","idx":{"timestamp":"1:2","value":"bb"}}]}}"#).unwrap();
        assert_eq!(v2.page_ids(), ["a", "b", "c"], "v2 形状按 idx 排、去掉已删页");
        let odd: Content = serde_json::from_str(r#"{"fileType":"epub","pages":["p1"],"cPages":{"pages":[{"id":5,"deleted":true}]}}"#).unwrap();
        assert_eq!(odd.page_ids(), ["p1"], "cPages 形状不认识也不影响 v1 解析");
        let real_v2: Content = serde_json::from_str(include_str!("../../../testdata/seven_styles/book.content")).unwrap();
        assert_eq!(real_v2.page_ids(), ["1ab4edce-0a88-4271-9624-9b6abe2673e5", "0439fde3-2b3d-4f33-8735-fb44134c5efc"], "真机笔记本样本（v2）读得出页表");
        let m: Metadata = serde_json::from_str(r#"{"visibleName":"人骨拼圖","type":"DocumentType","parent":"","lastModified":"1"}"#).unwrap();
        assert!(m.is_live_document());
        let t: Metadata = serde_json::from_str(r#"{"visibleName":"x","type":"DocumentType","parent":"trash"}"#).unwrap();
        assert!(!t.is_live_document());
        assert_eq!(uuid_of_event("3eb5dece-5e28-4969-8926-c8973d49020d.content"), Some("3eb5dece-5e28-4969-8926-c8973d49020d"));
        assert_eq!(uuid_of_event("3eb5dece-5e28-4969-8926-c8973d49020d"), Some("3eb5dece-5e28-4969-8926-c8973d49020d"));
        assert_eq!(uuid_of_event("hashtab"), None);
    }

    #[test]
    fn doc_paths_and_annotated_pages() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path();
        let d = Doc::new(lib, "u1");
        assert_eq!(d.page_rm("p"), lib.join("u1/p.rm"));
        assert!(d.annotated_pages().is_empty() && !d.has_annotated_pages());
        std::fs::create_dir_all(lib.join("u1")).unwrap();
        std::fs::write(lib.join("u1/a.png"), b"x").unwrap();
        assert!(!d.has_annotated_pages(), "只认 .rm");
        std::fs::write(lib.join("u1/b.rm"), b"x").unwrap();
        assert!(d.has_annotated_pages());
        assert_eq!(d.file_type(), None, "没有 .content");
        std::fs::write(lib.join("u1.content"), r#"{"fileType":"pdf","pages":["p1","p2"],"x":{"y":[1]}}"#).unwrap();
        assert_eq!(d.file_type().as_deref(), Some("pdf"));
        std::fs::write(lib.join("u1.content"), r#"{"fileType":"#).unwrap();
        assert_eq!(d.file_type(), None, "半截文件");
        std::fs::write(lib.join("u1/a.rm"), b"x").unwrap();
        assert_eq!(d.annotated_pages().iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }
}
