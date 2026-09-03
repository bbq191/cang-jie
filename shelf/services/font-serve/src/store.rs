//! 字体仓库（`AssetStore` 实现）：装进 `$XDG_DATA_HOME/fonts/`（fontconfig 标准位）→ `fc-cache -f` →
//! 写 `$XDG_DATA_HOME/shelf/fonts.json`（字体菜单 qmd 读）→ 可选镜像到 KOReader（HTTP 调 koreader-serve）。
//! 内建三项（随中文化套件装的字体）来自配置，只列不删。
use crate::ttf;
use serde::{Deserialize, Serialize};
use shelf_core::asset::{AssetItem, AssetStore};
use shelf_core::paths::Paths;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Names {
    pub cn: String,
    pub tw: String,
    pub en: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FontEntry {
    /// fontconfig 家族名（`epub.setFontName(key)` 用）。
    pub key: String,
    pub file: String,
    pub names: Names,
    /// builtin=随套件装、不可删；user=书架上传。
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FontConfig {
    pub mirror_to_koreader: bool,
    pub koreader_url: String,
    pub builtin: Vec<FontEntry>,
}

impl Default for FontConfig {
    fn default() -> Self {
        let b = |key: &str, file: &str, cn: &str, tw: &str, en: &str| FontEntry {
            key: key.into(),
            file: file.into(),
            names: Names { cn: cn.into(), tw: tw.into(), en: en.into() },
            source: "builtin".into(),
        };
        FontConfig {
            mirror_to_koreader: true,
            koreader_url: "http://127.0.0.1:8791".into(),
            builtin: vec![
                b("LXGW WenKai Mono GB Screen", "LXGWWenKaiMonoGBScreen.ttf", "霞鹜文楷（楷体）", "霞鶩文楷（楷體）", "LXGW WenKai (Kai)"),
                b("LXGW Neo ZhiSong Screen Full", "LXGWNeoZhiSongScreenFull.ttf", "霞鹜新致宋（宋体）", "霞鶩新致宋（宋體）", "LXGW ZhiSong (Song)"),
                b("KF Readerly", "KF_Readerly-Regular.ttf", "KF Readerly（阅读体）", "KF Readerly（閱讀體）", "KF Readerly"),
            ],
        }
    }
}

impl FontConfig {
    pub fn load(paths: &Paths) -> FontConfig {
        let f = paths.service_config("font");
        match std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            Some(c) => c,
            None => {
                let c = FontConfig::default();
                if !f.exists() {
                    let _ = std::fs::create_dir_all(paths.config_dir());
                    let _ = std::fs::write(&f, serde_json::to_string_pretty(&c).unwrap_or_default());
                }
                c
            }
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FontsJson {
    version: u32,
    fonts: Vec<FontEntry>,
}

pub struct FontStore {
    pub cfg: FontConfig,
    fonts_dir: PathBuf,
    json_path: PathBuf,
    /// 用户上传条目的持久清单（fonts.json 的 user 部分）。
    user_json: PathBuf,
    /// 测试可关：不真跑 fc-cache / 不镜像。
    pub side_effects: bool,
}

impl FontStore {
    pub fn new(paths: &Paths, cfg: FontConfig) -> FontStore {
        FontStore {
            cfg,
            fonts_dir: paths.user_fonts_dir(),
            json_path: paths.data_dir().join("fonts.json"),
            user_json: paths.data_dir().join("fonts.user.json"),
            side_effects: true,
        }
    }
    pub fn fonts_dir(&self) -> &Path {
        &self.fonts_dir
    }
    pub fn json_path(&self) -> &Path {
        &self.json_path
    }

    fn user_entries(&self) -> Vec<FontEntry> {
        std::fs::read_to_string(&self.user_json).ok().and_then(|t| serde_json::from_str::<Vec<FontEntry>>(&t).ok()).unwrap_or_default()
    }
    fn save_user(&self, v: &[FontEntry]) -> Result<(), String> {
        std::fs::write(&self.user_json, serde_json::to_string_pretty(v).unwrap_or_default()).map_err(|e| e.to_string())
    }

    /// 全部条目：内建（文件在才列）+ 用户（文件在才列）。
    pub fn entries(&self) -> Vec<FontEntry> {
        let mut v: Vec<FontEntry> = self.cfg.builtin.iter().filter(|b| self.fonts_dir.join(&b.file).is_file()).cloned().collect();
        v.extend(self.user_entries().into_iter().filter(|u| self.fonts_dir.join(&u.file).is_file()));
        v
    }

    /// 写 fonts.json（qmd 消费）。
    pub fn write_index(&self) -> Result<(), String> {
        if let Some(p) = self.json_path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let j = FontsJson { version: 1, fonts: self.entries() };
        let tmp = self.json_path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&j).unwrap_or_default()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.json_path).map_err(|e| e.to_string())
    }

    fn fc_cache(&self) {
        if self.side_effects {
            let _ = std::process::Command::new("fc-cache").arg("-f").status();
        }
    }

    /// 家族名：fc-scan 优先（与 fontconfig 一致），否则自解析 name 表。
    fn family_of(&self, path: &Path, bytes: &[u8]) -> Option<String> {
        if self.side_effects {
            if let Ok(o) = std::process::Command::new("fc-scan").args(["--format", "%{family[0]}", path.to_str()?]).output() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if o.status.success() && !s.is_empty() {
                    return Some(s);
                }
            }
        }
        ttf::family_name(bytes)
    }

    /// 镜像到 KOReader fonts/（HTTP 调 koreader-serve，失败只降级为回执说明）。
    pub fn mirror_to_koreader(&self, name: &str, path: &Path) -> Result<(), String> {
        if !self.side_effects {
            return Ok(());
        }
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let boundary = "----shelffont";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: font/ttf\r\n\r\n").as_bytes());
        body.extend_from_slice(&data);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let resp = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(30)).build()
            .post(&format!("{}/fonts", self.cfg.koreader_url))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body);
        match resp {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(c, r)) => Err(format!("koreader-serve HTTP {c}: {}", r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("koreader-serve 不可达: {e}")),
        }
    }
}

impl AssetStore for FontStore {
    fn kind(&self) -> &'static str {
        "font"
    }
    fn allowed_ext(&self) -> &'static [&'static str] {
        &["ttf", "otf", "ttc"]
    }
    fn validate(&self, name: &str, staged: &Path) -> Result<(), String> {
        if self.cfg.builtin.iter().any(|b| b.file.eq_ignore_ascii_case(name)) {
            return Err("与内建字体同名，拒绝覆盖".into());
        }
        let mut head = [0u8; 4];
        std::io::Read::read_exact(&mut std::fs::File::open(staged).map_err(|e| e.to_string())?, &mut head).map_err(|_| "文件太小".to_string())?;
        if !ttf::is_font(&head) {
            return Err("不是 TrueType/OpenType 字体文件".into());
        }
        Ok(())
    }
    fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String> {
        std::fs::create_dir_all(&self.fonts_dir).map_err(|e| e.to_string())?;
        let dest = self.fonts_dir.join(name);
        let bytes = std::fs::read(staged).map_err(|e| e.to_string())?;
        std::fs::write(&dest, &bytes).map_err(|e| format!("写入字体目录失败: {e}"))?;
        let family = self.family_of(&dest, &bytes).unwrap_or_else(|| name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).to_string());
        self.fc_cache();
        let mut users: Vec<FontEntry> = self.user_entries().into_iter().filter(|u| u.file != name).collect();
        users.push(FontEntry { key: family.clone(), file: name.into(), names: Names { cn: family.clone(), tw: family.clone(), en: family.clone() }, source: "user".into() });
        self.save_user(&users)?;
        self.write_index()?;
        let mut extra = serde_json::json!({"family": family, "source": "user"});
        if self.cfg.mirror_to_koreader {
            extra["koreader"] = match self.mirror_to_koreader(name, &dest) {
                Ok(()) => serde_json::json!("mirrored"),
                Err(e) => serde_json::json!(format!("未镜像：{e}")),
            };
        }
        Ok(AssetItem { name: name.into(), bytes: bytes.len() as u64, extra })
    }
    fn list(&self) -> Vec<AssetItem> {
        self.entries()
            .into_iter()
            .map(|e| {
                let bytes = std::fs::metadata(self.fonts_dir.join(&e.file)).map(|m| m.len()).unwrap_or(0);
                AssetItem { name: e.file.clone(), bytes, extra: serde_json::json!({"family": e.key, "source": e.source, "names": e.names}) }
            })
            .collect()
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        if self.cfg.builtin.iter().any(|b| b.file == name) {
            return Err("内建字体不可删".into());
        }
        let users = self.user_entries();
        if !users.iter().any(|u| u.file == name) {
            return Err("不是书架安装的字体".into());
        }
        let _ = std::fs::remove_file(self.fonts_dir.join(name));
        // 同步撤 KOReader 镜像（失败只打日志：KOReader 侧可在其 tab 里单独删）
        if self.cfg.mirror_to_koreader && self.side_effects {
            let url = format!("{}/fonts/{}", self.cfg.koreader_url, percent_encode(name));
            if let Err(e) = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).build().delete(&url).call() {
                eprintln!("[font-serve] 撤 KOReader 镜像 {name} 失败: {e}");
            }
        }
        self.save_user(&users.into_iter().filter(|u| u.file != name).collect::<Vec<_>>())?;
        self.fc_cache();
        self.write_index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shelf_core::asset::AssetUploadFlow;

    fn setup() -> (tempfile::TempDir, Paths, FontStore) {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None });
        paths.ensure().unwrap();
        let mut s = FontStore::new(&paths, FontConfig::default());
        s.side_effects = false;
        (t, paths, s)
    }

    fn body(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = Vec::new();
        for (f, d) in files {
            b.extend_from_slice(format!("--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{f}\"\r\n\r\n").as_bytes());
            b.extend_from_slice(d);
            b.extend_from_slice(b"\r\n");
        }
        b.extend_from_slice(b"--B--\r\n");
        b
    }

    #[test]
    fn upload_lists_and_removes_user_fonts_but_not_builtin() {
        let (_t, paths, store) = setup();
        // 预置一个内建字体文件
        std::fs::create_dir_all(store.fonts_dir()).unwrap();
        std::fs::write(store.fonts_dir().join("KF_Readerly-Regular.ttf"), b"\x00\x01\x00\x00").unwrap();
        let font = b"OTTO\x00\x00\x00\x00rest";
        let out = AssetUploadFlow::new(&paths).run(&store, &body(&[("My Font.otf", font), ("KF_Readerly-Regular.ttf", font), ("junk.ttf", b"PK\x03\x04junk")])[..], "B").unwrap();
        assert!(out[0].ok, "{:?}", out[0]);
        assert_eq!(out[1].message, "与内建字体同名，拒绝覆盖");
        assert_eq!(out[2].message, "不是 TrueType/OpenType 字体文件");
        let list = store.list();
        assert_eq!(list.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["KF_Readerly-Regular.ttf", "My Font.otf"]);
        assert_eq!(list[1].extra["family"], "My Font"); // 无 name 表 → 用文件名
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.json_path()).unwrap()).unwrap();
        assert_eq!(j["fonts"].as_array().unwrap().len(), 2);
        assert_eq!(j["fonts"][0]["source"], "builtin");
        assert!(store.remove("KF_Readerly-Regular.ttf").is_err());
        store.remove("My Font.otf").unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(!store.fonts_dir().join("My Font.otf").exists());
    }
}

fn percent_encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}
