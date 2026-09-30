//! xochitl 书库里一份文档的**只读**视图：`<uuid>.metadata`（名字/类型/回收站）、`<uuid>.content`（fileType、`pages` 页 id 表）、
//! `<uuid>/<page>.rm`（有手写/勾画的页才有文件）、`<uuid>.epubindex` + `<uuid>.epub`（页→章）。
//! （`<uuid>.thumbnails/` 缩略图已不读：裁图改自渲染，见 `crop.rs`。）
//! 绝不写书库目录（xochitl 不认外部改动，且 metadata 含凭证以外的隐私）。
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Metadata {
    #[serde(rename = "visibleName")]
    pub visible_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub parent: String,
    pub deleted: bool,
}

#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Content {
    #[serde(rename = "fileType")]
    pub file_type: String,
    /// 页 id 顺序表（0-based 下标 = 页号，与 .epubindex 起始页对齐）。
    pub pages: Vec<String>,
}

impl Metadata {
    pub fn is_live_document(&self) -> bool {
        self.kind == "DocumentType" && self.parent != "trash" && !self.deleted
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
        let p = self.side("metadata");
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("读 {} 失败: {e}", p.display())),
        };
        serde_json::from_str(&text).map(Some).map_err(|e| format!("{} 解析失败（先跳过，下次再试）: {e}", p.display()))
    }
    pub fn content(&self) -> Option<Content> {
        serde_json::from_str(&std::fs::read_to_string(self.side("content")).ok()?).ok()
    }
    /// 只取 `.content` 的 `fileType`（不把整张页 id 表解析成 `Vec<String>`）。
    pub fn file_type(&self) -> Option<String> {
        #[derive(Deserialize)]
        struct OnlyFileType {
            #[serde(rename = "fileType", default)]
            file_type: String,
        }
        let f = std::fs::File::open(self.side("content")).ok()?;
        serde_json::from_reader::<_, OnlyFileType>(std::io::BufReader::new(f)).ok().map(|c| c.file_type)
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
    (stem.len() == 36 && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_content_and_metadata_shapes() {
        let c: Content = serde_json::from_str(include_str!("../../../testdata/renggu/book.content")).unwrap();
        assert_eq!((c.file_type.as_str(), c.pages.len()), ("epub", 523));
        assert_eq!(c.pages[0], "f02e9084-d864-46f3-a07e-ae84ba19d344");
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
