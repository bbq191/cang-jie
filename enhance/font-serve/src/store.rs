//! 字体仓库（`AssetStore` 实现）= `$XDG_DATA_HOME/fonts/`（fontconfig 用户字体目录）的管理器。
//! 目录里**所有**字体一视同仁、按 fontconfig 家族名归组（一个家族多字重文件=一项），都可删——没有"内建/系统"之分
//! （2026-09-03 用户纠正：那是对旧中文化套件 scp 字体的路径耦合）。唯一的提示：家族被
//! `~/.config/fontconfig/fonts.conf` 引用（界面 CJK 回退）的标 `fontconfigRef`，删前 UI 提醒但不拦。
//! 上传→落目录→`fc-cache -f`→重建 `$XDG_DATA_HOME/shelf/fonts.json`（字体菜单 qmd 读）。**只管原生阅读器**：
//! （2026-09-29 KOReader 已从设备卸载，koreader-serve 随之退役；此前 KOReader 字体由它单独管。）
//!
//! 同一个类型还管**界面字体**（[`Role::Ui`]，2026-10-07）：放在 `fonts/shelf-ui/` 子目录（fontconfig 递归扫得到、
//! xochitl 能按名字用；阅读字体的扫描只看顶层，所以不进阅读器菜单 `fonts.json`、不进中文回退链），清单写
//! `shelf/ui-fonts.json`，不写 fontconfig。选哪个当界面字体记在 `shelf/ui-font.json`（见 `ui.rs`）。
use serde::{Deserialize, Serialize};
use rmsvc_core::asset::{AssetItem, AssetStore};
use rmsvc_core::cache::{FileStamp, StampCache};
use rmsvc_core::formats::{self, FONT_EXTS};
use rmsvc_core::fs::{list_files, write_atomic, write_atomic_if_changed};
use rmsvc_core::proc::run_timeout;
use rmsvc_core::paths::Paths;
use rmsvc_core::ttf;
use crate::fontconfig;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 覆盖率 ≥ 此值才算"中文字体"、才进回退链（滤掉纯拉丁字体，避免拉丁字体当中文兜底）。
pub const CJK_MIN_PCT: u8 = 8;
/// 覆盖率 < 此值 = 低覆盖美术/子集字体，上传时警告（正文会缺字）。
pub const CJK_LOW_PCT: u8 = 80;
/// 界面字体子目录（在 fontconfig 用户字体目录下）。
pub const UI_SUBDIR: &str = "shelf-ui";
/// 装了界面字体时追加在每个字体请求末尾的系统中文字体（设备 /usr/share/fonts/ttf/noto 自带），
/// 让阅读的中文缺字回退仍落在它上面、不落到界面字体上（见 `fontconfig::render` 的 `pin`）。
pub const SYSTEM_CJK_PIN: &str = "Noto Sans SC";
/// fc-scan 单个文件的超时：几十 MB 的中文字体也在 1 秒内；卡住（坏文件/IO 挂起）不能把上传请求一直挂着，超时退回自解析 name 表。
const FC_SCAN_TIMEOUT: Duration = Duration::from_secs(30);
/// fc-cache 重建用户字体目录缓存的超时（host 实测几毫秒到零点几秒，设备上大字体多时留足余量）。
const FC_CACHE_TIMEOUT: Duration = Duration::from_secs(120);
/// 探测缓存条目上限：远大于实际会装的字体文件数（满了整表清空重来，只是多跑一轮 fc-scan）。
const PROBE_CACHE_CAP: usize = 4096;

/// 这个仓库管的是阅读字体还是界面字体。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Reading,
    Ui,
}

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
    /// 中文基本区覆盖率（0..=100）。0=非中文字体。用于回退排序 + 低覆盖警告。
    #[serde(default)]
    pub cjk_pct: u8,
    /// 被 fontconfig 用户配置引用（界面中文回退用），删前提醒。
    #[serde(default)]
    pub fontconfig_ref: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FontConfig {
    /// 对中文回退字体加 `embolden`（墨水屏细笔画/低对比补偿，对标旧中文化套件）。
    /// **默认开**（2026-09-04 用户目测更清楚）；显式写 false 则关。
    pub embolden_cjk_fallback: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        FontConfig { embolden_cjk_fallback: true }
    }
}

impl FontConfig {
    pub fn load(paths: &Paths) -> FontConfig {
        rmsvc_core::config::load_or_seed(&paths.service_config("font"))
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FontsJson {
    version: u32,
    fonts: Vec<FontEntry>,
}

pub struct FontStore {
    role: Role,
    /// 中文回退加粗（墨水屏补偿）。运行时可切（PUT /config），用 AtomicBool 免锁。
    embolden: std::sync::atomic::AtomicBool,
    config_path: PathBuf,
    fonts_dir: PathBuf,
    json_path: PathBuf,
    fontconfig_conf: PathBuf,
    /// 逐文件探测结果缓存（家族名 + 中文覆盖率），键=文件名，凭文件戳（大小、mtime、inode）判失效：
    /// 每次上传/删除都要重扫整个字体目录，没有缓存就是对每个字体重新 fork 一次 fc-scan + 整文件读入解析。
    probes: StampCache<(Vec<String>, u8)>,
    /// 测试可关：不真跑 fc-cache/fc-scan。
    pub side_effects: bool,
}

/// fc-scan `%{family}` 输出（逗号分隔的本地化家族名）→ 家族名列表（首个 = key）。
fn split_families(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// 目录里的字体文件名（只认普通文件、不含隐藏文件，按名排序）；目录不在 → 空。扫描、开机一致性检查、"有没有界面字体"共用。
fn font_files(dir: &Path) -> Vec<String> {
    list_files(dir, |n| formats::has_ext(n, FONT_EXTS))
}

impl FontStore {
    pub fn new(paths: &Paths, cfg: FontConfig) -> FontStore {
        FontStore {
            role: Role::Reading,
            embolden: std::sync::atomic::AtomicBool::new(cfg.embolden_cjk_fallback),
            config_path: paths.service_config("font"),
            fonts_dir: paths.user_fonts_dir(),
            json_path: paths.data_dir().join("fonts.json"),
            fontconfig_conf: paths.config_root().join("fontconfig/fonts.conf"),
            probes: StampCache::new(PROBE_CACHE_CAP),
            side_effects: true,
        }
    }

    /// 界面字体仓库：`fonts/shelf-ui/` + `shelf/ui-fonts.json`，不写 fontconfig、没有加粗开关。
    pub fn ui(paths: &Paths) -> FontStore {
        let mut s = FontStore::new(paths, FontConfig { embolden_cjk_fallback: false });
        s.role = Role::Ui;
        s.fonts_dir = paths.user_fonts_dir().join(UI_SUBDIR);
        s.json_path = paths.data_dir().join("ui-fonts.json");
        s
    }

    /// 界面字体目录里有没有字体文件（阅读仓库据此决定要不要加系统中文保底）。
    fn ui_fonts_present(&self) -> bool {
        !font_files(&self.fonts_dir.join(UI_SUBDIR)).is_empty()
    }

    pub fn embolden(&self) -> bool {
        self.embolden.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 切换中文回退加粗：存配置 + 重写 fontconfig（fontconfig 实时生效，翻书即见，无需重启 xochitl）。
    pub fn set_embolden(&self, on: bool) -> Result<(), String> {
        self.embolden.store(on, std::sync::atomic::Ordering::Relaxed);
        rmsvc_core::config::save(&self.config_path, &FontConfig { embolden_cjk_fallback: on }, None)?;
        self.write_fontconfig(&self.entries())?;
        Ok(())
    }
    pub fn fonts_dir(&self) -> &Path {
        &self.fonts_dir
    }
    pub fn json_path(&self) -> &Path {
        &self.json_path
    }

    /// 探测一个字体文件：家族名（fc-scan 优先——带本地化名，如 "LXGW WenKai,霞鹜文楷"；否则自解析 name 表）+
    /// 中文覆盖率。文件整读一次供两者共用（旧实现回落路径要读两遍）。
    fn probe_file(&self, path: &Path) -> (Vec<String>, u8) {
        let bytes = std::fs::read(path).ok();
        let pct = bytes.as_deref().and_then(ttf::han_coverage_pct).unwrap_or(0);
        if self.side_effects {
            if let Some(Ok(s)) = path.to_str().map(|p| run_timeout("fc-scan", &["--format", "%{family}", p], FC_SCAN_TIMEOUT)) {
                if !s.is_empty() {
                    return (split_families(&s), pct);
                }
            }
        }
        (bytes.as_deref().and_then(ttf::family_name).map(|f| vec![f]).unwrap_or_default(), pct)
    }

    /// 带缓存的 [`Self::probe_file`]：文件戳没变就复用上次结果（算的时候不持锁，fork + 读大文件不挡别的查询）。
    /// 文件 stat 不到（刚被删）→ 照算不缓存。
    fn probe_cached(&self, name: &str, path: &Path) -> (Vec<String>, u8) {
        match FileStamp::read(path) {
            Some(stamp) => self.probes.get_or(name, stamp, || self.probe_file(path)),
            None => self.probe_file(path),
        }
    }

    fn fontconfig_families(&self) -> Vec<String> {
        std::fs::read_to_string(&self.fontconfig_conf).map(|t| fontconfig::referenced_families(&t)).unwrap_or_default()
    }

    /// 扫目录 → 按首家族名归组 → 条目（key 排序）。
    pub fn scan(&self) -> Vec<FontEntry> {
        let refs = self.fontconfig_families();
        let mut groups: BTreeMap<String, FontEntry> = BTreeMap::new();
        let files = font_files(&self.fonts_dir);
        // 缓存里去掉已不在目录的文件（删字体后不留脏项）
        self.probes.retain(|k| files.binary_search_by(|f| f.as_str().cmp(k)).is_ok());
        for f in files {
            let path = self.fonts_dir.join(&f);
            let (fams, pct) = self.probe_cached(&f, &path);
            let key = match fams.first() {
                Some(k) => k.clone(),
                None => formats::stem_of(&f).to_string(),
            };
            // 本地化显示名：取第一个含非 ASCII 的家族名，没有就用 key
            let local = fams.iter().find(|s| !s.is_ascii()).cloned().unwrap_or_else(|| key.clone());
            let e = groups.entry(key.clone()).or_insert_with(|| FontEntry {
                key: key.clone(),
                files: vec![],
                names: Names { cn: local.clone(), tw: local.clone(), en: key.clone() },
                cjk_pct: 0,
                fontconfig_ref: refs.iter().any(|r| r == &key || fams.contains(r)),
            });
            e.cjk_pct = e.cjk_pct.max(pct);
            e.files.push(f);
        }
        groups.into_values().collect()
    }

    /// 重建 fonts.json（qmd 消费；界面仓库是 ui-fonts.json，只给网页列表用）。
    pub fn write_index(&self) -> Result<Vec<FontEntry>, String> {
        let fonts = self.scan();
        rmsvc_core::config::save(&self.json_path, &FontsJson { version: 1, fonts: fonts.clone() }, None)?;
        if self.role == Role::Ui {
            return Ok(fonts);
        }
        if let Err(e) = self.write_fontconfig(&fonts) {
            eprintln!("[font-serve] 写 fontconfig 回退失败: {e}");
        }
        Ok(fonts)
    }

    /// 启动时用：fonts.json 仍与字体目录一致就直接用它，不再每次开机把每个字体整文件读一遍 + 每个 fork 一次
    /// fc-scan（中文字体动辄十几到几十 MB，开机正是 xochitl 起界面、最不该抢 I/O 的时候）；不一致/缺失/损坏才全量重扫。
    /// 一致 = 目录里的字体文件集合与索引里的完全相同，且目录与各文件的 mtime 都不晚于 fonts.json。fonts.conf 仍按
    /// 当前条目重渲染一次（内容没变不落盘，见 [`Self::write_fontconfig`]），新版本改了回退配置格式也能在启动时带上。
    pub fn startup_index(&self) -> Result<Vec<FontEntry>, String> {
        if let Some(fonts) = self.fresh_index() {
            if self.role == Role::Ui {
                return Ok(fonts);
            }
            if let Err(e) = self.write_fontconfig(&fonts) {
                eprintln!("[font-serve] 写 fontconfig 回退失败: {e}");
            }
            return Ok(fonts);
        }
        self.write_index()
    }

    fn fresh_index(&self) -> Option<Vec<FontEntry>> {
        let json_mtime = std::fs::metadata(&self.json_path).ok()?.modified().ok()?;
        let fonts = serde_json::from_str::<FontsJson>(&std::fs::read_to_string(&self.json_path).ok()?).ok()?.fonts;
        let newer = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).map(|t| t > json_mtime).unwrap_or(true);
        if newer(&self.fonts_dir) {
            return None;
        }
        let on_disk = font_files(&self.fonts_dir);
        let mut indexed: Vec<String> = fonts.iter().flat_map(|e| e.files.iter().cloned()).collect();
        indexed.sort();
        if on_disk != indexed || on_disk.iter().any(|f| newer(&self.fonts_dir.join(f))) {
            return None;
        }
        Some(fonts)
    }

    /// 读缓存的 fonts.json（列表接口用，避免每次 fc-scan）。缺则重建。
    pub fn entries(&self) -> Vec<FontEntry> {
        std::fs::read_to_string(&self.json_path).ok().and_then(|t| serde_json::from_str::<FontsJson>(&t).ok()).map(|j| j.fonts).unwrap_or_else(|| self.write_index().unwrap_or_default())
    }

    /// 只重建用户字体目录（`fonts/`，含 `shelf-ui/` 子目录，fc-cache 对目录参数递归）的缓存。此前不带目录参数，
    /// 每装/删一个字体都把系统字体目录也强制重扫一遍，那些目录根本没变（host 实测 CPU 0.7s → 0.006s）。
    /// 界面仓库也从 `fonts/` 起扫：首次建 `shelf-ui/` 时父目录的缓存（记着有哪些子目录）也要跟着更新。
    /// 上传不在 [`AssetStore::install`] 里逐个文件跑（一次传几个字体就要重建几遍），由路由在整批装完后调一次。
    pub fn fc_cache(&self) {
        if self.side_effects {
            let root = match self.role {
                Role::Reading => self.fonts_dir.as_path(),
                Role::Ui => self.fonts_dir.parent().unwrap_or(&self.fonts_dir),
            };
            if let Some(Err(e)) = root.to_str().map(|r| run_timeout("fc-cache", &["-f", r], FC_CACHE_TIMEOUT)) {
                eprintln!("[font-serve] fc-cache 失败: {e}");
            }
        }
    }

    /// 中文字体（覆盖率 ≥ CJK_MIN_PCT），按覆盖率降序——回退链首选覆盖最全的。
    fn cjk_fallback_order(fonts: &[FontEntry]) -> Vec<&FontEntry> {
        let mut v: Vec<&FontEntry> = fonts.iter().filter(|e| e.cjk_pct >= CJK_MIN_PCT).collect();
        v.sort_by(|a, b| b.cjk_pct.cmp(&a.cjk_pct).then(a.key.cmp(&b.key)));
        v
    }

    /// 生成 shelf 自管的 `~/.config/fontconfig/fonts.conf`：把界面/书籍的中文回退动态指向**当前已装**的
    /// 中文字体（覆盖率降序）。关键：全部用 **append + binding=weak**——所以阅读器 `setFontName(你选的字体)`
    /// 永远排在最前、真正生效（修 bug1「上传不生效」），只有你选的字体缺的那个字才字形级回退到兜底中文字体
    /// （修 bug2「书里方框」，只要装了任意中文字体就不豆腐）。generic sans/serif/mono 也指向它们（界面/笔记）。
    /// 首次接管前，把已存在的非 shelf 配置备份到 `~/.config/shelf/fontconfig-fonts.conf.pre-shelf.bak`。
    pub fn write_fontconfig(&self, fonts: &[FontEntry]) -> Result<(), String> {
        let cjk = Self::cjk_fallback_order(fonts);
        // 备份既有的非 shelf 配置（只备份一次）
        if let Ok(existing) = std::fs::read_to_string(&self.fontconfig_conf) {
            if !existing.contains(fontconfig::FC_MARK) {
                let bak = self.config_root_backup();
                if !bak.exists() {
                    let _ = write_atomic(&bak, existing.as_bytes());
                }
            }
        }
        let keys: Vec<&str> = cjk.iter().map(|e| e.key.as_str()).collect();
        let pin: &[&str] = if self.ui_fonts_present() { &[SYSTEM_CJK_PIN] } else { &[] };
        let xml = fontconfig::render(&keys, self.embolden(), pin, &self.config_root_backup());
        // 内容没变不重写：fontconfig 按配置文件 mtime 判断要不要重载，白写一次会让正在用它的进程（xochitl）重读配置。
        write_atomic_if_changed(&self.fontconfig_conf, xml.as_bytes()).map(|_| ()).map_err(|e| e.to_string())
    }

    /// ~/.config/shelf/fontconfig-fonts.conf.pre-shelf.bak（首次接管前的原配置备份）。
    fn config_root_backup(&self) -> PathBuf {
        self.fontconfig_conf.parent().and_then(|p| p.parent()).map(|c| c.join("shelf/fontconfig-fonts.conf.pre-shelf.bak")).unwrap_or_else(|| PathBuf::from("fontconfig-fonts.conf.pre-shelf.bak"))
    }

    /// 界面字体装上 / 删掉后由调用方叫：重写阅读仓库的 fonts.conf（保底规则跟着有无界面字体变）。
    pub fn refresh_fontconfig(&self) {
        if let Err(e) = self.write_fontconfig(&self.entries()) {
            eprintln!("[font-serve] 写 fontconfig 回退失败: {e}");
        }
    }

    /// 当前中文回退链（覆盖率降序的字体 key）——回执/状态展示用。
    pub fn cjk_fallback_keys(&self) -> Vec<String> {
        Self::cjk_fallback_order(&self.entries()).into_iter().map(|e| e.key.clone()).collect()
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
        match self.role {
            Role::Reading => "font",
            Role::Ui => "ui-font",
        }
    }
    fn allowed_ext(&self) -> &'static [&'static str] {
        FONT_EXTS
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
        // 暂存与字体目录同在 /home：直接改名；跨分区（测试 / 非常规布局）才退回"拷到同目录临时名再改名"，
        // 字体目录里不会出现 fontconfig / 别的进程看得到的半截字体文件。
        rmsvc_core::fs::move_into(staged, &dest).map_err(|e| format!("写入字体目录失败: {e}"))?;
        let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        // fc-cache 由调用方在整批上传后跑一次（见 fc_cache）；建索引用 fc-scan 直接读文件，不依赖缓存。
        let fonts = self.write_index()?;
        let entry = fonts.iter().find(|e| e.files.iter().any(|f| f == name)).cloned();
        let pct = entry.as_ref().map(|e| e.cjk_pct).unwrap_or(0);
        let is_cjk = pct >= CJK_MIN_PCT;
        let mut warn = String::new();
        if self.role == Role::Ui {
            if pct < CJK_LOW_PCT {
                warn = format!("中文覆盖率仅 {pct}%，界面里缺的字会显示成系统的 {SYSTEM_CJK_PIN}");
            }
        } else if is_cjk && pct < CJK_LOW_PCT {
            warn = format!("中文覆盖率仅 {pct}%（正文会有生僻字缺字/方框，适合做标题或点缀，不建议当正文主字体）");
        } else if !is_cjk && pct > 0 {
            warn = format!("中文覆盖率仅 {pct}%，不作中文回退");
        }
        let extra = serde_json::json!({
            "family": entry.as_ref().map(|e| e.key.clone()).unwrap_or_default(),
            "names": entry.as_ref().map(|e| e.names.clone()),
            "cjkPct": pct, "isCjk": is_cjk,
            "fallback": is_cjk && self.role == Role::Reading, "warn": warn,
        });
        Ok(AssetItem { name: name.into(), bytes, extra })
    }
    fn success_message(&self, _requested: &str, item: &AssetItem) -> String {
        match item.extra.get("family").and_then(|f| f.as_str()).filter(|f| !f.is_empty()) {
            Some(f) => format!("已安装（家族 {f}）"),
            None => "已安装".into(),
        }
    }
    fn list(&self) -> Vec<AssetItem> {
        self.entries()
            .into_iter()
            .map(|e| {
                let bytes: u64 = e.files.iter().map(|f| std::fs::metadata(self.fonts_dir.join(f)).map(|m| m.len()).unwrap_or(0)).sum();
                AssetItem { name: e.key.clone(), bytes, extra: serde_json::json!({"family": e.key, "files": e.files, "names": e.names, "cjkPct": e.cjk_pct, "fontconfigRef": e.fontconfig_ref}) }
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
    use rmsvc_core::asset::AssetUploadFlow;

    fn setup() -> (tempfile::TempDir, Paths, FontStore) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
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


    fn entry(key: &str, pct: u8) -> FontEntry {
        FontEntry { key: key.into(), files: vec![format!("{key}.ttf")], names: Names { cn: key.into(), tw: key.into(), en: key.into() }, cjk_pct: pct, fontconfig_ref: false }
    }

    #[test]
    fn fontconfig_weak_fallback_ordered_by_coverage_and_backs_up() {
        let (_t, _paths, store) = setup();
        std::fs::create_dir_all(store.fontconfig_conf.parent().unwrap()).unwrap();
        // 首次接管前已有非 shelf 配置 → 应备份
        std::fs::write(&store.fontconfig_conf, "<fontconfig><!-- chinese-ime 旧配置 --></fontconfig>").unwrap();
        let fonts = vec![entry("Art Font", 35), entry("Big CJK", 99), entry("Latin Only", 0), entry("Mid CJK", 90)];
        store.write_fontconfig(&fonts).unwrap();
        let out = std::fs::read_to_string(&store.fontconfig_conf).unwrap();
        assert!(out.contains(fontconfig::FC_MARK), "带 shelf 标记");
        assert!(std::fs::read_to_string(store.config_root_backup()).unwrap().contains("chinese-ime"), "原配置已备份");
        // 全 weak，无 strong
        assert!(!out.contains("strong"), "不得有 strong 绑定（否则盖过用户选择）: {out}");
        assert!(out.contains(r#"binding="weak""#));
        // 覆盖率降序：Big CJK(99) 在 Mid CJK(90) 之前，二者在 Art Font(35) 之前；Latin(0) 与 <8% 不入链
        let ib = out.find("Big CJK").unwrap();
        let im = out.find("Mid CJK").unwrap();
        let ia = out.find("Art Font").unwrap();
        assert!(ib < im && im < ia, "按覆盖率降序: {out}");
        assert!(!out.contains("Latin Only"), "非中文字体不入回退链");
        // 幂等 + 二次不再覆盖备份
        std::fs::write(store.config_root_backup(), "SENTINEL").unwrap();
        store.write_fontconfig(&fonts).unwrap();
        assert_eq!(std::fs::read_to_string(store.config_root_backup()).unwrap(), "SENTINEL", "已备份则不再覆盖");
        // 无中文字体 → 空回退但不报错
        store.write_fontconfig(&[entry("Latin", 0)]).unwrap();
        assert!(std::fs::read_to_string(&store.fontconfig_conf).unwrap().contains("没有已装的中文字体"));
        // embolden 机制：关则无 <match target=font>，开则每个回退字体一条（不依赖默认值）
        store.embolden.store(false, std::sync::atomic::Ordering::Relaxed);
        store.write_fontconfig(&fonts).unwrap();
        assert!(!std::fs::read_to_string(&store.fontconfig_conf).unwrap().contains("embolden"), "关则无");
        store.embolden.store(true, std::sync::atomic::Ordering::Relaxed);
        store.write_fontconfig(&fonts).unwrap();
        let e = std::fs::read_to_string(&store.fontconfig_conf).unwrap();
        assert!(e.contains("<match target=\"font\">") && e.contains("<edit name=\"embolden\""), "开 embolden 应加 match: {e}");
    }
    #[test]
    fn probe_cache_hits_on_same_fingerprint_and_invalidates_on_change() {
        let (_t, _paths, store) = setup();
        std::fs::create_dir_all(store.fonts_dir()).unwrap();
        let f = store.fonts_dir().join("X.ttf");
        std::fs::write(&f, b"\x00\x01\x00\x00").unwrap();
        // 无 name 表 → 家族=文件名去扩展名，探测结果进缓存
        assert_eq!(store.scan()[0].key, "X");
        assert_eq!(store.probes.len(), 1);
        // 文件戳不变 → 命中缓存：塞个假结果，scan 必须原样用它而不是重新探测
        store.probes.put("X.ttf", FileStamp::read(&f).unwrap(), (vec!["Cached".into()], 50));
        let e = store.scan();
        assert_eq!((e[0].key.as_str(), e[0].cjk_pct), ("Cached", 50));
        // 文件变了（大小不同）→ 缓存失效重探
        std::fs::write(&f, b"\x00\x01\x00\x00more").unwrap();
        assert_eq!(store.scan()[0].key, "X");
        // 文件被删 → 缓存项随下次扫描清掉
        std::fs::remove_file(&f).unwrap();
        assert!(store.scan().is_empty());
        assert!(store.probes.is_empty());
    }

    #[test]
    fn startup_reuses_fresh_index_and_rescans_when_dir_changes() {
        let (_t, _paths, store) = setup();
        std::fs::create_dir_all(store.fonts_dir()).unwrap();
        std::fs::write(store.fonts_dir().join("A.ttf"), b"\x00\x01\x00\x00").unwrap();
        assert!(store.fresh_index().is_none(), "没有 fonts.json → 全量扫");
        assert_eq!(store.startup_index().unwrap()[0].key, "A");
        let conf_mtime = std::fs::metadata(&store.fontconfig_conf).unwrap().modified().unwrap();
        // 索引与目录一致：直接复用（塞个假 key 进 fonts.json，复用时必须原样读回而不是重扫）
        let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.json_path()).unwrap()).unwrap();
        j["fonts"][0]["key"] = "FromIndex".into();
        std::fs::write(store.json_path(), serde_json::to_vec(&j).unwrap()).unwrap();
        assert_eq!(store.startup_index().unwrap()[0].key, "FromIndex");
        store.write_fontconfig(&store.entries()).unwrap();
        assert_eq!(std::fs::metadata(&store.fontconfig_conf).unwrap().modified().unwrap(), conf_mtime, "内容没变不重写 fonts.conf");
        // 目录里多了一个文件（且比索引新）→ 重扫
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(store.fonts_dir().join("B.ttf"), b"\x00\x01\x00\x00").unwrap();
        assert!(store.fresh_index().is_none());
        let keys: Vec<String> = store.startup_index().unwrap().into_iter().map(|e| e.key).collect();
        assert_eq!(keys, vec!["A", "B"]);
        // 文件被删（文件集合不一致）→ 重扫
        std::fs::remove_file(store.fonts_dir().join("B.ttf")).unwrap();
        assert!(store.fresh_index().is_none());
        // fonts.json 损坏 → 重扫
        std::fs::write(store.json_path(), "{broken").unwrap();
        assert_eq!(store.startup_index().unwrap()[0].key, "A");
    }

    #[test]
    fn ui_fonts_stay_out_of_reading_and_pin_system_cjk() {
        let (_t, paths, store) = setup();
        let mut ui = FontStore::ui(&paths);
        ui.side_effects = false;
        std::fs::create_dir_all(store.fonts_dir()).unwrap();
        std::fs::write(store.fonts_dir().join("Read.ttf"), b"\x00\x01\x00\x00").unwrap();
        store.write_index().unwrap();
        let conf = || std::fs::read_to_string(&store.fontconfig_conf).unwrap();
        assert!(!conf().contains(SYSTEM_CJK_PIN), "没有界面字体时不加保底");
        // 装一个界面字体：落在 shelf-ui/，进 ui-fonts.json，不进阅读的 fonts.json / 菜单
        let out = AssetUploadFlow::new(&paths).run(&ui, &body(&[("Ui.ttf", b"\x00\x01\x00\x00")])[..], "B").unwrap();
        assert!(out[0].ok, "{:?}", out[0].message);
        assert!(store.fonts_dir().join(UI_SUBDIR).join("Ui.ttf").exists());
        assert_eq!(ui.list().iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["Ui"]);
        assert_eq!(out[0].item.as_ref().unwrap().extra["fallback"], false, "界面字体不当回退");
        assert_eq!(store.write_index().unwrap().iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), vec!["Read"], "阅读扫描只看顶层");
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(store.json_path()).unwrap()).unwrap();
        assert!(!j.to_string().contains("Ui.ttf"));
        assert!(!conf().contains(">Ui<"), "界面字体不进 fonts.conf 的回退链");
        // 阅读仓库重写 fonts.conf 时带上系统中文保底
        store.refresh_fontconfig();
        assert!(conf().contains(&format!("mode=\"append\" binding=\"weak\"><string>{SYSTEM_CJK_PIN}</string>")));
        // 删掉界面字体：保底撤掉
        ui.remove_family("Ui").unwrap();
        store.refresh_fontconfig();
        assert!(!conf().contains(SYSTEM_CJK_PIN));
        assert!(ui.list().is_empty());
    }

    #[test]
    fn embolden_defaults_on() {
        assert!(FontConfig::default().embolden_cjk_fallback, "默认开");
    }
    #[test]
    fn family_splitting_and_localized_name() {
        assert_eq!(split_families("LXGW WenKai,霞鹜文楷"), vec!["LXGW WenKai", "霞鹜文楷"]);
        assert_eq!(split_families("KF Readerly"), vec!["KF Readerly"]);
    }
}
