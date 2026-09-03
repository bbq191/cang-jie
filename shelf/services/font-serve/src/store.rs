//! 字体仓库（`AssetStore` 实现）= `$XDG_DATA_HOME/fonts/`（fontconfig 用户字体目录）的管理器。
//! 目录里**所有**字体一视同仁、按 fontconfig 家族名归组（一个家族多字重文件=一项），都可删——没有"内建/系统"之分
//! （2026-09-03 用户纠正：那是对旧中文化套件 scp 字体的路径耦合）。唯一的提示：家族被
//! `~/.config/fontconfig/fonts.conf` 引用（界面 CJK 回退）的标 `fontconfigRef`，删前 UI 提醒但不拦。
//! 上传→落目录→`fc-cache -f`→重建 `$XDG_DATA_HOME/shelf/fonts.json`（字体菜单 qmd 读）。**只管原生阅读器**：
//! KOReader 的字体由 koreader-serve 单独管（用户 2026-09-03 定：两边各自装、不同时装填）。
use crate::ttf;
use serde::{Deserialize, Serialize};
use shelf_core::asset::{AssetItem, AssetStore};
use shelf_core::paths::Paths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FONT_EXT: &[&str] = &["ttf", "otf", "ttc"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Names {
    pub cn: String,
    pub tw: String,
    pub en: String,
}

/// fonts.json 里的一项 = 一个字体家族。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FontEntry {
    /// fontconfig 首家族名（`epub.setFontName(key)` 用）。
    pub key: String,
    /// 该家族的全部文件（多字重）。
    pub files: Vec<String>,
    pub names: Names,
    /// 被 fontconfig 用户配置引用（界面中文回退用），删前提醒。
    #[serde(default)]
    pub fontconfig_ref: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FontConfig {}

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
    #[allow(dead_code)] // 预留：将来字体级配置（如显示名覆盖）
    pub cfg: FontConfig,
    fonts_dir: PathBuf,
    json_path: PathBuf,
    fontconfig_conf: PathBuf,
    /// 测试可关：不真跑 fc-cache/fc-scan。
    pub side_effects: bool,
}

/// 一个文件的家族信息：(首家族名 = key, 全部家族名列表)。
fn split_families(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

impl FontStore {
    pub fn new(paths: &Paths, cfg: FontConfig) -> FontStore {
        FontStore {
            cfg,
            fonts_dir: paths.user_fonts_dir(),
            json_path: paths.data_dir().join("fonts.json"),
            fontconfig_conf: paths.config_root().join("fontconfig/fonts.conf"),
            side_effects: true,
        }
    }
    pub fn fonts_dir(&self) -> &Path {
        &self.fonts_dir
    }
    pub fn json_path(&self) -> &Path {
        &self.json_path
    }

    fn is_font_file(name: &str) -> bool {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        name.contains('.') && FONT_EXT.contains(&ext.as_str())
    }

    /// 文件的家族名列表：fc-scan 优先（带本地化名，如 "LXGW WenKai,霞鹜文楷"），否则自解析 name 表。
    fn families_of(&self, path: &Path) -> Vec<String> {
        if self.side_effects {
            if let Ok(o) = std::process::Command::new("fc-scan").args(["--format", "%{family}"]).arg(path).output() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if o.status.success() && !s.is_empty() {
                    return split_families(&s);
                }
            }
        }
        std::fs::read(path).ok().and_then(|b| ttf::family_name(&b)).map(|f| vec![f]).unwrap_or_default()
    }

    fn fontconfig_families(&self) -> Vec<String> {
        let Ok(t) = std::fs::read_to_string(&self.fontconfig_conf) else { return vec![] };
        let mut v = Vec::new();
        let mut rest = t.as_str();
        while let Some(i) = rest.find("<family>") {
            let after = &rest[i + 8..];
            if let Some(j) = after.find("</family>") {
                v.push(after[..j].trim().to_string());
                rest = &after[j..];
            } else {
                break;
            }
        }
        v
    }

    /// 扫目录 → 按首家族名归组 → 条目（key 排序）。
    pub fn scan(&self) -> Vec<FontEntry> {
        let refs = self.fontconfig_families();
        let mut groups: BTreeMap<String, FontEntry> = BTreeMap::new();
        let Ok(rd) = std::fs::read_dir(&self.fonts_dir) else { return vec![] };
        let mut files: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_str().map(|s| s.to_string())).filter(|n| !n.starts_with('.') && Self::is_font_file(n)).collect();
        files.sort();
        for f in files {
            let fams = self.families_of(&self.fonts_dir.join(&f));
            let key = match fams.first() {
                Some(k) => k.clone(),
                None => f.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| f.clone()),
            };
            // 本地化显示名：取第一个含非 ASCII 的家族名，没有就用 key
            let local = fams.iter().find(|s| !s.is_ascii()).cloned().unwrap_or_else(|| key.clone());
            let e = groups.entry(key.clone()).or_insert_with(|| FontEntry {
                key: key.clone(),
                files: vec![],
                names: Names { cn: local.clone(), tw: local.clone(), en: key.clone() },
                fontconfig_ref: refs.iter().any(|r| r == &key || fams.contains(r)),
            });
            e.files.push(f);
        }
        groups.into_values().collect()
    }

    /// 重建 fonts.json（qmd 消费）。
    pub fn write_index(&self) -> Result<Vec<FontEntry>, String> {
        if let Some(p) = self.json_path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let fonts = self.scan();
        let tmp = self.json_path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&FontsJson { version: 1, fonts: fonts.clone() }).unwrap_or_default()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.json_path).map_err(|e| e.to_string())?;
        Ok(fonts)
    }

    /// 读缓存的 fonts.json（列表接口用，避免每次 fc-scan）。缺则重建。
    pub fn entries(&self) -> Vec<FontEntry> {
        std::fs::read_to_string(&self.json_path).ok().and_then(|t| serde_json::from_str::<FontsJson>(&t).ok()).map(|j| j.fonts).unwrap_or_else(|| self.write_index().unwrap_or_default())
    }

    fn fc_cache(&self) {
        if self.side_effects {
            let _ = std::process::Command::new("fc-cache").arg("-f").status();
        }
    }


    /// 删整个家族（全部文件）。
    pub fn remove_family(&self, key: &str) -> Result<Vec<String>, String> {
        let Some(e) = self.entries().into_iter().find(|e| e.key == key) else { return Err("没有这个字体家族".into()) };
        for f in &e.files {
            std::fs::remove_file(self.fonts_dir.join(f)).map_err(|err| format!("删 {f} 失败: {err}"))?;
        }
        self.fc_cache();
        self.write_index()?;
        Ok(e.files)
    }
}

impl AssetStore for FontStore {
    fn kind(&self) -> &'static str {
        "font"
    }
    fn allowed_ext(&self) -> &'static [&'static str] {
        FONT_EXT
    }
    fn validate(&self, _name: &str, staged: &Path) -> Result<(), String> {
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
        std::fs::copy(staged, &dest).map_err(|e| format!("写入字体目录失败: {e}"))?;
        let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        self.fc_cache();
        let fonts = self.write_index()?;
        let entry = fonts.iter().find(|e| e.files.iter().any(|f| f == name)).cloned();
        let extra = serde_json::json!({"family": entry.as_ref().map(|e| e.key.clone()).unwrap_or_default(), "names": entry.as_ref().map(|e| e.names.clone())});
        Ok(AssetItem { name: name.into(), bytes, extra })
    }
    fn list(&self) -> Vec<AssetItem> {
        self.entries()
            .into_iter()
            .map(|e| {
                let bytes: u64 = e.files.iter().map(|f| std::fs::metadata(self.fonts_dir.join(f)).map(|m| m.len()).unwrap_or(0)).sum();
                AssetItem { name: e.key.clone(), bytes, extra: serde_json::json!({"family": e.key, "files": e.files, "names": e.names, "fontconfigRef": e.fontconfig_ref}) }
            })
            .collect()
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        self.remove_family(name).map(|_| ())
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
    fn all_fonts_uniform_grouped_by_family_and_deletable() {
        let (_t, paths, store) = setup();
        std::fs::create_dir_all(store.fonts_dir()).unwrap();
        // 预先存在的（旧套件 scp 进来的）字体也一视同仁；无 name 表 → 家族=文件名去扩展名
        std::fs::write(store.fonts_dir().join("Old-Regular.ttf"), b"\x00\x01\x00\x00").unwrap();
        // fontconfig 引用它 → 标 fontconfigRef
        std::fs::create_dir_all(store.fontconfig_conf.parent().unwrap()).unwrap();
        std::fs::write(&store.fontconfig_conf, "<fontconfig><alias><family>Old-Regular</family></alias></fontconfig>").unwrap();
        let font = b"OTTO\x00\x00\x00\x00rest";
        let out = AssetUploadFlow::new(&paths).run(&store, &body(&[("New.otf", font), ("junk.ttf", b"PK\x03\x04junk")])[..], "B").unwrap();
        assert!(out[0].ok && out[0].item.as_ref().unwrap().extra["family"] == "New");
        assert_eq!(out[1].message, "不是 TrueType/OpenType 字体文件");
        let list = store.list();
        assert_eq!(list.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["New", "Old-Regular"]);
        assert_eq!(list[1].extra["fontconfigRef"], true);
        assert_eq!(list[0].extra["fontconfigRef"], false);
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.json_path()).unwrap()).unwrap();
        assert_eq!(j["fonts"].as_array().unwrap().len(), 2);
        assert!(j["fonts"][0].get("source").is_none(), "不再有 source/内建概念");
        // 旧字体也可删
        assert_eq!(store.remove_family("Old-Regular").unwrap(), vec!["Old-Regular.ttf"]);
        assert_eq!(store.list().len(), 1);
        assert!(store.remove("nope").is_err());
    }

    #[test]
    fn family_splitting_and_localized_name() {
        assert_eq!(split_families("LXGW WenKai,霞鹜文楷"), vec!["LXGW WenKai", "霞鹜文楷"]);
        assert_eq!(split_families("KF Readerly"), vec!["KF Readerly"]);
    }
}
