//! 资产仓库（Repository）与上传流程模板（Template Method）。
//! 字体、壁纸都是"一个目录里的一堆文件 + 一份清单"：上传→暂存→校验→安装→回执 这一套只在
//! [`AssetUploadFlow`] 写一次，各仓库只实现差异（`validate`/`install`/`list`/`remove`）。
use crate::multipart::{safe_basename, MultipartReader};
use crate::paths::Paths;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AssetItem {
    pub name: String,
    pub bytes: u64,
    /// 各仓库自定义附加信息（字体家族名 / 壁纸尺寸…）。
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

/// 单项上传结果。
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct UploadOutcome {
    pub name: String,
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<AssetItem>,
}

pub trait AssetStore {
    /// 资产类型（日志/回执用），如 "font" / "wallpaper"。
    fn kind(&self) -> &'static str;
    /// 允许的扩展名（小写、不带点）。
    fn allowed_ext(&self) -> &'static [&'static str];
    /// 校验暂存文件（格式/尺寸/与内建冲突…）。
    fn validate(&self, name: &str, staged: &Path) -> Result<(), String>;
    /// 安装暂存文件（移动/转换到最终位置），返回条目。
    fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String>;
    fn list(&self) -> Vec<AssetItem>;
    fn remove(&self, name: &str) -> Result<(), String>;
}

/// 上传流程模板：multipart 逐文件流式落暂存 → 扩展名门 → validate → install；逐项独立成败。
pub struct AssetUploadFlow<'a> {
    paths: &'a Paths,
}

impl<'a> AssetUploadFlow<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        AssetUploadFlow { paths }
    }

    fn ext_ok(store: &dyn AssetStore, name: &str) -> bool {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        name.contains('.') && store.allowed_ext().contains(&ext.as_str())
    }

    /// 处理一整个 multipart 请求体。
    pub fn run<R: Read>(&self, store: &dyn AssetStore, body: R, boundary: &str) -> Result<Vec<UploadOutcome>, String> {
        let tmp_dir = self.paths.upload_tmp_dir();
        std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        let mut mp = MultipartReader::new(body, boundary);
        let mut out = Vec::new();
        loop {
            let mut part = match mp.next_part() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(e) => return Err(format!("multipart 解析失败: {e}")),
            };
            let Some(fname) = part.filename.clone() else { continue };
            let name = safe_basename(&fname, "upload.bin");
            if !Self::ext_ok(store, &name) {
                out.push(UploadOutcome { name, ok: false, message: format!("不支持的扩展名（允许：{}）", store.allowed_ext().join("/")), item: None });
                continue;
            }
            let staged: PathBuf = tmp_dir.join(format!("{}.{}.part", uuid::Uuid::new_v4().simple(), store.kind()));
            let written = (|| -> Result<u64, String> {
                let mut f = std::fs::File::create(&staged).map_err(|e| e.to_string())?;
                std::io::copy(&mut part, &mut f).map_err(|e| e.to_string())
            })();
            let outcome = match written {
                Err(e) => UploadOutcome { name: name.clone(), ok: false, message: format!("接收失败: {e}"), item: None },
                Ok(0) => UploadOutcome { name: name.clone(), ok: false, message: "空文件".into(), item: None },
                Ok(_) => match store.validate(&name, &staged).and_then(|_| store.install(&name, &staged)) {
                    Ok(item) => UploadOutcome { name: name.clone(), ok: true, message: "已安装".into(), item: Some(item) },
                    Err(e) => UploadOutcome { name: name.clone(), ok: false, message: e, item: None },
                },
            };
            let _ = std::fs::remove_file(&staged);
            out.push(outcome);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MemStore {
        dir: PathBuf,
        installed: Mutex<Vec<String>>,
    }
    impl AssetStore for MemStore {
        fn kind(&self) -> &'static str {
            "test"
        }
        fn allowed_ext(&self) -> &'static [&'static str] {
            &["txt"]
        }
        fn validate(&self, name: &str, staged: &Path) -> Result<(), String> {
            if name.starts_with("bad") {
                return Err("名字不合法".into());
            }
            if std::fs::metadata(staged).map(|m| m.len()).unwrap_or(0) > 10 {
                return Err("太大".into());
            }
            Ok(())
        }
        fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String> {
            let dest = self.dir.join(name);
            std::fs::copy(staged, &dest).map_err(|e| e.to_string())?;
            self.installed.lock().unwrap().push(name.to_string());
            Ok(AssetItem { name: name.into(), bytes: std::fs::metadata(&dest).unwrap().len(), extra: serde_json::Value::Null })
        }
        fn list(&self) -> Vec<AssetItem> {
            vec![]
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn flow_reports_per_item_and_cleans_tmp() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None });
        let store = MemStore { dir: t.path().join("dst"), installed: Mutex::new(vec![]) };
        std::fs::create_dir_all(&store.dir).unwrap();
        let mut body = Vec::new();
        for (f, d) in [("ok.txt", "hello"), ("bad.txt", "x"), ("big.txt", "0123456789ab"), ("nope.bin", "z"), ("../evil.txt", "e")] {
            body.extend_from_slice(format!("--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{f}\"\r\n\r\n{d}\r\n").as_bytes());
        }
        body.extend_from_slice(b"--B--\r\n");
        let out = AssetUploadFlow::new(&paths).run(&store, &body[..], "B").unwrap();
        let summary: Vec<(String, bool)> = out.iter().map(|o| (o.name.clone(), o.ok)).collect();
        assert_eq!(
            summary,
            vec![("ok.txt".into(), true), ("bad.txt".into(), false), ("big.txt".into(), false), ("nope.bin".into(), false), ("evil.txt".into(), true)]
        );
        assert_eq!(*store.installed.lock().unwrap(), vec!["ok.txt", "evil.txt"]);
        assert!(std::fs::read_dir(paths.upload_tmp_dir()).unwrap().next().is_none(), "暂存应清空");
    }
}
