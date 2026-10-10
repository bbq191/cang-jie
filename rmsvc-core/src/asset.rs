//! 资产仓库（Repository）与上传流程模板（Template Method）。
//! 字体、壁纸、母版库（以前还有 KOReader 字体/词典，2026-09-30 随 koreader-serve 删除）都是"一个目录里的一堆文件"：上传→暂存→扩展名门→校验→安装→回执
//! 这一套只在 [`AssetUploadFlow`] 写一次，各仓库只实现差异（[`UploadTarget`] 的 `validate`/`install`；列表/删除是仓库自己的固有方法），
//! 拒收 / 成功文案也由仓库按需覆盖（`reject_message`/`success_message`），不再各服务手搓 multipart 循环。
use crate::formats;
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

impl AssetItem {
    pub fn plain(name: impl Into<String>, bytes: u64) -> AssetItem {
        AssetItem { name: name.into(), bytes, extra: serde_json::Value::Null }
    }
}

/// 单项上传结果（网页 uploader / CLI `print_receipts` 共同的契约：`name` + `ok` + `message`）。
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct UploadOutcome {
    pub name: String,
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<AssetItem>,
}

impl UploadOutcome {
    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> UploadOutcome {
        UploadOutcome { name: name.into(), ok: false, message: message.into(), item: None }
    }
}

/// 上传流程（[`AssetUploadFlow`]）真正要的那一半：扩展名门、校验、安装、回执文案。`list`/`remove` 是仓库自己的
/// 查询/删除，上传流程用不到，不该逼每个上传目标都实现（book-serve 的母版库适配器此前只好写两个死转发，审计 CORE-6）。
/// 上传目标只实现这个 trait，列表/删除写成固有方法（2026-10-10 第二阶段删掉了带 list/remove 的旧合体 trait `AssetStore`）。
pub trait UploadTarget {
    /// 资产类型（日志/暂存文件名用），如 "font" / "wallpaper" / "book"。
    fn kind(&self) -> &'static str;
    /// 允许的扩展名（小写、不带点）；**空＝任意**。
    fn allowed_ext(&self) -> &'static [&'static str];
    /// 校验暂存文件（格式/尺寸/与内建冲突…）。缺省不校验。
    fn validate(&self, _name: &str, _staged: &Path) -> Result<(), String> {
        Ok(())
    }
    /// 安装暂存文件（移动/转换到最终位置），返回条目。暂存文件之后由流程删除（已被 rename 走也无妨）。
    fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String>;
    /// 扩展名不在白名单时的回执文案。
    fn reject_message(&self) -> String {
        format!("不支持的扩展名（允许：{}）", self.allowed_ext().join(" / "))
    }
    /// 安装成功的回执文案（`requested`=上传时的文件名，`item.name` 可能被仓库改名）。
    fn success_message(&self, _requested: &str, _item: &AssetItem) -> String {
        "已安装".into()
    }
}

/// 整批是否全成功（非空且逐项 ok）。
pub fn all_ok(items: &[UploadOutcome]) -> bool {
    !items.is_empty() && items.iter().all(|i| i.ok)
}

/// 至少有一项成功（上传多个文件时，有成功的就该发"列表变了"事件）。
pub fn any_ok(items: &[UploadOutcome]) -> bool {
    items.iter().any(|i| i.ok)
}

/// 标准回执 `{ok, items, …extra}`——各上传处理器在此之上只加自己的字段（note / activated …）。
pub fn receipt(items: &[UploadOutcome], extra: serde_json::Value) -> serde_json::Value {
    let mut v = serde_json::json!({"ok": all_ok(items), "items": items});
    if let (Some(dst), Some(src)) = (v.as_object_mut(), extra.as_object()) {
        for (k, val) in src {
            dst.insert(k.clone(), val.clone());
        }
    }
    v
}

/// [`AssetUploadFlow::run`] 整体失败的原因（逐个文件的失败——扩展名不对、校验没过、安装失败——不在这里，记在回执逐项里，
/// 回执本身仍是 200：一批里有成有败是正常结果）。2026-10-10 前整体失败只回字符串，三个调用方一律报 400，建暂存目录失败
/// 这类设备故障也成了"请求不对"（审计 CORE-1）。
#[derive(Debug, PartialEq)]
pub enum FlowError {
    /// 请求体不是合法的 multipart（客户端的问题）→ 400。
    BadRequest(String),
    /// 暂存目录建不了、暂存文件开不了（设备的问题：盘满、目录权限）→ 500。
    Io(String),
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::BadRequest(m) | FlowError::Io(m) => f.write_str(m),
        }
    }
}

impl From<FlowError> for crate::http::ApiError {
    fn from(e: FlowError) -> Self {
        match e {
            FlowError::BadRequest(m) => crate::http::ApiError::bad(m),
            FlowError::Io(m) => crate::http::ApiError::internal(m),
        }
    }
}

/// 上传流程模板：multipart 逐文件流式落暂存 → 扩展名门 → validate → install；逐项独立成败。
pub struct AssetUploadFlow {
    tmp_dir: PathBuf,
}

impl AssetUploadFlow {
    /// 暂存在 [`Paths::upload_tmp_dir`]（/home 分区，与字体 / 壁纸目录同分区，install 可直接改名）。
    pub fn new(paths: &Paths) -> Self {
        AssetUploadFlow { tmp_dir: paths.upload_tmp_dir() }
    }
    /// 暂存在指定目录——大文件（书）应与最终目录同分区，install 才能 rename 而不是拷贝。
    pub fn in_dir(dir: PathBuf) -> Self {
        AssetUploadFlow { tmp_dir: dir }
    }

    /// 清掉上次进程中途被杀留下的暂存半成品（`.<pid>.<序号>.<kind>.part`；2026-10-10 前是 `.<uuid>.<kind>.part`，同样以 `.part` 结尾）。只在服务启动时调用（此时不可能有上传在进行），返回清掉几个。
    pub fn clean_stale(&self) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.tmp_dir) else { return 0 };
        rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".part") && std::fs::remove_file(e.path()).is_ok()).count()
    }

    /// 处理一整个 multipart 请求体。
    /// `store` 是任何 [`UploadTarget`]（`&dyn UploadTarget` 也照样能传）。
    pub fn run<R: Read, S: UploadTarget + ?Sized>(&self, store: &S, body: R, boundary: &str) -> Result<Vec<UploadOutcome>, FlowError> {
        std::fs::create_dir_all(&self.tmp_dir).map_err(|e| FlowError::Io(format!("建暂存目录 {} 失败: {e}", self.tmp_dir.display())))?;
        let mut mp = MultipartReader::new(body, boundary);
        let mut out = Vec::new();
        loop {
            let mut part = match mp.next_part() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(e) => return Err(FlowError::BadRequest(format!("multipart 解析失败: {e}"))),
            };
            let Some(fname) = part.filename.clone() else { continue };
            let name = safe_basename(&fname, "upload.bin");
            if !formats::has_ext(&name, store.allowed_ext()) {
                out.push(UploadOutcome::fail(name, store.reject_message()));
                continue;
            }
            // ScratchFile：install 失败或 panic 时暂存也会删掉（此前只在正常路径上手删）；名字仍以 `.part` 结尾，
            // 进程被杀留下的由 [`Self::clean_stale`] 启动时清。
            let staged_file = crate::fs::ScratchFile::new(&self.tmp_dir, ".", &format!("{}.part", store.kind()));
            let staged = staged_file.path();
            // 暂存文件开不了是设备的问题（盘满 / 目录被删），后面的文件也一样会失败 → 整体 500；接收中途出错（客户端断开、
            // 读超时）仍记在这一项上（`io::copy` 分不清是读请求体还是写盘出的错，按改动前的逐项处理）。
            let mut file = std::fs::File::create(staged).map_err(|e| FlowError::Io(format!("建暂存文件失败: {e}")))?;
            let outcome = match std::io::copy(&mut part, &mut file) {
                Err(e) => UploadOutcome::fail(name, format!("接收失败: {e}")),
                Ok(0) => UploadOutcome::fail(name, "空文件"),
                Ok(_) => match store.validate(&name, staged).and_then(|_| store.install(&name, staged)) {
                    // 成功项的 name 用落地名（仓库可能改名 / 加前缀），回执与列表一致。
                    Ok(item) => UploadOutcome { message: store.success_message(&name, &item), name: item.name.clone(), ok: true, item: Some(item) },
                    Err(e) => UploadOutcome::fail(name, e),
                },
            };
            drop(file);
            drop(staged_file);
            out.push(outcome);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn any_ok_vs_all_ok() {
        let ok = UploadOutcome { name: "a".into(), ok: true, message: String::new(), item: None };
        let bad = UploadOutcome::fail("b", "x");
        assert!(any_ok(&[ok.clone(), bad.clone()]) && !all_ok(&[ok.clone(), bad.clone()]));
        assert!(!any_ok(&[bad]) && !any_ok(&[]) && all_ok(&[ok]));
    }

    struct MemStore {
        dir: PathBuf,
        installed: Mutex<Vec<String>>,
    }
    impl UploadTarget for MemStore {
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
            Ok(AssetItem::plain(name, std::fs::metadata(&dest).unwrap().len()))
        }
        fn success_message(&self, requested: &str, item: &AssetItem) -> String {
            format!("装好 {requested}→{}", item.name)
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
        assert_eq!(out[0].message, "装好 ok.txt→ok.txt", "成功文案由仓库定");
        assert_eq!(out[3].message, "不支持的扩展名（允许：txt）");
        assert_eq!(*store.installed.lock().unwrap(), vec!["ok.txt", "evil.txt"]);
        assert!(std::fs::read_dir(paths.upload_tmp_dir()).unwrap().next().is_none(), "暂存应清空");
        assert!(paths.upload_tmp_dir().starts_with(t.path()), "暂存在 HOME 下（/home 分区），不在 tmpfs 运行时目录");
        std::fs::write(paths.upload_tmp_dir().join(".x.font.part"), b"half").unwrap();
        std::fs::write(paths.upload_tmp_dir().join("keep.txt"), b"k").unwrap();
        assert_eq!(AssetUploadFlow::new(&paths).clean_stale(), 1, "只清 .part 半成品");
        assert!(paths.upload_tmp_dir().join("keep.txt").exists());
        let r = receipt(&out, serde_json::json!({"note": "n"}));
        assert_eq!(r["ok"], false);
        assert_eq!(r["items"].as_array().unwrap().len(), 5);
        assert_eq!(r["note"], "n");
    }

    /// 整体失败分两种：请求体不是 multipart → 400；暂存目录建不了 → 500（不再一律 400）。
    #[test]
    fn flow_errors_split_bad_request_and_io() {
        let t = tempfile::tempdir().unwrap();
        let store = MemStore { dir: t.path().to_path_buf(), installed: Mutex::new(vec![]) };
        let e = AssetUploadFlow::in_dir(t.path().join("tmp")).run(&store, &b"--B\r\nno-blank-line-ever"[..], "B").unwrap_err();
        assert!(matches!(&e, FlowError::BadRequest(m) if m.starts_with("multipart 解析失败")), "{e:?}");
        assert_eq!(crate::http::ApiError::from(e).status, 400);
        std::fs::write(t.path().join("file"), b"x").unwrap();
        let body = b"--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\r\nabc\r\n--B--\r\n";
        let e = AssetUploadFlow::in_dir(t.path().join("file/tmp")).run(&store, &body[..], "B").unwrap_err();
        assert!(matches!(&e, FlowError::Io(m) if m.contains("暂存目录")), "{e:?}");
        assert_eq!(crate::http::ApiError::from(e).status, 500);
        assert!(store.installed.lock().unwrap().is_empty());
    }

    /// 只实现必需方法的上传目标走缺省文案；`&dyn UploadTarget` 照样能传。
    #[test]
    fn flow_accepts_plain_upload_target_and_dyn_target() {
        struct Plain(PathBuf);
        impl UploadTarget for Plain {
            fn kind(&self) -> &'static str {
                "plain"
            }
            fn allowed_ext(&self) -> &'static [&'static str] {
                &[]
            }
            fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String> {
                std::fs::copy(staged, self.0.join(name)).map(|n| AssetItem::plain(name, n)).map_err(|e| e.to_string())
            }
        }
        let t = tempfile::tempdir().unwrap();
        let flow = AssetUploadFlow::in_dir(t.path().join("tmp"));
        let body = b"--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.any\"\r\n\r\nabc\r\n--B--\r\n";
        let out = flow.run(&Plain(t.path().to_path_buf()), &body[..], "B").unwrap();
        assert!(all_ok(&out) && out[0].message == "已安装", "{out:?}");
        let mem = MemStore { dir: t.path().to_path_buf(), installed: Mutex::new(vec![]) };
        let dynamic: &dyn UploadTarget = &mem;
        let body = b"--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"y.txt\"\r\n\r\nabc\r\n--B--\r\n";
        assert!(all_ok(&flow.run(dynamic, &body[..], "B").unwrap()));
    }
}
