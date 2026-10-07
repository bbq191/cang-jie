//! 落库记录边车（Repository）：母版库每本书旁的隐藏 JSON `.<文件名>.delivered`（书名太长时用短名，见 [`file_name_for`]）——最近一次加入 xochitl 的 unix 秒 +
//! 最近一次投原生的渲染自检结果 + 最近一次落库的异步结果。只管"读 / 改 / 删这份记录"，
//! 母版库动作（入库/落库）在 `staging`，自检逻辑在 `render_check`；两边都通过这里落盘，谁也不碰
//! 对方的字段语义（2026-09-06 从 staging.rs 拆出）。
use serde::{Deserialize, Serialize};
use rmsvc_core::fs::write_atomic;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 落库记录：最近一次加入 xochitl 的 unix 秒；`render`=最近一次投原生的渲染自检结果。
/// 旧版边车里可能还带着已退役的字段——`koreader`（加入 KOReader 的时刻，2026-10-07 删）、`source`（host CLI 洗书产物的原始输入身份，2026-09-18 CLI 已砍、此后没有写方）、
/// `direction`（按书阅读方向，2026-09-30 已撤）、`deliver.progress`（按卷拆分投递的进度，同日已撤）：serde 缺省忽略
/// 未知字段，照常读。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Delivered {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderCheck>,
    /// 最近一次「落库」的结果（`staging::Staging::spawn_deliver` 异步执行时写，2026-09-19）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<DeliverCheck>,
}

/// 异步落库的结果：`status` = pending（后台线程跑着）/ ok / failed。`message` 是回执文案（成功＝"已加入 xochitl《...》"；
/// 失败＝错误原因）。落库只有整本上传 / 大文件通道两条路，都是一步到位、没有分步进度。旧边车里的 `optimize` 字段
/// （书架 2026-10-07 前的「优化」结果）解析时当未知字段忽略，下次改写边车时自然消失。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DeliverCheck {
    pub status: String,
    pub message: String,
    pub at: u64,
}

/// 渲染自检结果：`status` = pending（等 xochitl 渲染）/ ok / onopen（大文件通道的 EPUB，首次打开才渲染）/ timeout。
/// 2026-10-07 前还有 `warn`（页数远低于按字数估的期望页数）和 `expected` 字段：书改在电脑上用 sheng-ren 优化、有它的质量门把关后删掉，
/// 旧边车里的 `expected` 解析时忽略，旧的 `warn` 记录照样显示页数。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RenderCheck {
    pub uuid: String,
    pub pages: u64,
    pub status: String,
    pub at: u64,
}

/// Linux 单段文件名上限（字节）。
const NAME_MAX: usize = 255;
/// 短名里保留的书名前缀最多这么多字节：`.` + 200 + `.` + 16 位 hex + `.delivered` = 228 字节，
/// 原子写的临时名（`rmsvc_core::fs` 把目标名截到 200 字节再加后缀）也放得下。
const SHORT_BASE_MAX: usize = 200;
const SUFFIX: &str = ".delivered";

/// 书名 → 边车文件名。**所有**读、写、改名、删除、孤儿清理、启动修复都经这一个函数（或 [`path_for`]），不许各处自己拼。
/// - 普通书名：`.<书名>.delivered`（跟以前逐字节一致，设备上已有的边车照常认）；
/// - 拼出来超过 255 字节（书名约 244 字节以上，中文八十来个字）：`.<书名按字符边界截到 200 字节>.<书名 sha256 前 16 位 hex>.delivered`。
///   以前这种书名的边车写不进去（`File name too long`），网页上看不到它的落库状态；所以短名形式没有旧数据要迁移。
pub fn file_name_for(book_name: &str) -> String {
    let plain = format!(".{book_name}{SUFFIX}");
    if plain.len() <= NAME_MAX {
        return plain;
    }
    let mut end = book_name.len().min(SHORT_BASE_MAX);
    while !book_name.is_char_boundary(end) {
        end -= 1;
    }
    let digest = Sha256::digest(book_name.as_bytes());
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!(".{}.{hex}{SUFFIX}", &book_name[..end])
}

/// 边车路径：同目录、隐藏名（母版库列表按点开头跳过），文件名见 [`file_name_for`]。
pub fn path_for(book: &Path) -> PathBuf {
    let name = book.file_name().and_then(|s| s.to_str()).unwrap_or("book");
    book.with_file_name(file_name_for(name))
}

/// 目录项名字看起来是边车（点开头、`.delivered` 结尾）。
pub fn is_sidecar_name(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(SUFFIX)
}

/// 目录里现存的每本书（非点开头的普通文件）的边车文件名 → 书路径。短名形式的边车没法从文件名反推书名，
/// 孤儿清理、启动修复都靠这张反查表认"边车是谁的"（2026-09-30）。
pub fn owners(dir: &Path) -> HashMap<String, PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return HashMap::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let p = e.path();
            (!name.starts_with('.') && p.is_file()).then(|| (file_name_for(&name), p))
        })
        .collect()
}

/// 书改名后边车跟着走（没有边车不算错）。调用方负责在书本身改名成功之后调。
pub fn rename(old_book: &Path, new_book: &Path) {
    let (old_car, new_car) = (path_for(old_book), path_for(new_book));
    if old_car.exists() {
        let _ = std::fs::rename(old_car, new_car);
    }
}

pub fn read(book: &Path) -> Option<Delivered> {
    serde_json::from_slice(&std::fs::read(path_for(book)).ok()?).ok()
}

/// 读—改—原子写。没有边车从空记录起。
///
/// 全局互斥：落库线程、渲染自检线程、HTTP 线程会并发改同一份边车，`write_atomic` 只保证文件不写一半、
/// 不保证不丢更新（A 读→B 读→A 写→B 写，A 的字段没了）。边车都很小、写得不频繁，一把全局锁足够。
pub fn update(book: &Path, f: impl FnOnce(&mut Delivered)) -> Result<(), String> {
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = rmsvc_core::sync::lock(&WRITE);
    let mut d = read(book).unwrap_or_default();
    f(&mut d);
    let s = serde_json::to_vec(&d).map_err(|e| e.to_string())?;
    write_atomic(&path_for(book), &s).map_err(|e| format!("写落库记录失败: {e}"))
}

/// 删边车（书删了连带删；不存在不算错）。
pub fn remove(book: &Path) {
    let _ = std::fs::remove_file(path_for(book));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_reads_back_and_tolerates_old_records() {
        let t = tempfile::tempdir().unwrap();
        let book = t.path().join("b.epub");
        assert_eq!(path_for(&book).file_name().unwrap(), ".b.epub.delivered");
        assert!(read(&book).is_none());
        update(&book, |d| d.native = Some(7)).unwrap();
        update(&book, |d| d.render = Some(RenderCheck { uuid: "u".into(), pages: 3, status: "ok".into(), at: 1 })).unwrap();
        let d = read(&book).unwrap();
        assert_eq!(d.native, Some(7));
        assert_eq!(d.render.as_ref().map(|r| r.pages), Some(3));
        // 旧版边车（无 render 字段）照读；带已退役字段的也照读、字段忽略：`direction`（按书方向，2026-09-30 撤）、
        // `source`（CLI 洗书原始输入，CLI 09-18 砍）、`deliver.progress`（按卷拆分投递进度，09-30 撤）
        std::fs::write(path_for(&book), br#"{"native":1,"koreader":2,"direction":"rtl","source":{"name":"a.pdf","bytes":9},"deliver":{"status":"ok","message":"m","at":3,"progress":{"done":1,"total":2}}}"#).unwrap();
        let deliver = Some(DeliverCheck { status: "ok".into(), message: "m".into(), at: 3 });
        assert_eq!(read(&book), Some(Delivered { native: Some(1), render: None, deliver }));
        remove(&book);
        assert!(read(&book).is_none());
        remove(&book);
    }

    /// 普通书名的边车名跟改动前逐字节一致（设备上已有的边车必须照常认）；拼出来正好 255 字节的也还是老格式。
    #[test]
    fn plain_names_keep_legacy_sidecar_name() {
        let legacy = |n: &str| format!(".{n}.delivered");
        for n in ["b.epub", "三体 - 01卷.epub", "亂馬1⁄2 典藏版 - 19卷.pdf", &format!("{}.pdf", "a".repeat(240))] {
            assert_eq!(file_name_for(n), legacy(n), "{n}");
            assert_eq!(path_for(&Path::new("/x").join(n)), Path::new("/x").join(legacy(n)));
        }
        let edge = "a".repeat(NAME_MAX - 11);
        assert_eq!(file_name_for(&edge).len(), NAME_MAX);
        assert_eq!(file_name_for(&edge), legacy(&edge), "正好 255 字节不换短名");
    }

    #[test]
    fn long_names_get_deterministic_short_sidecar_name() {
        // 250 字节的中文书名：老格式要 261 字节，写不进去
        let long = format!("{}ab.epub", "书".repeat(81));
        assert_eq!(long.len(), 250);
        let short = file_name_for(&long);
        assert!(short.len() <= NAME_MAX, "{}", short.len());
        assert!(is_sidecar_name(&short));
        assert_eq!(short, file_name_for(&long), "确定性");
        assert!(short.starts_with(&format!(".{}", "书".repeat(66))), "{short}"); // 200 字节按字符边界截到 198（66 个汉字）
        // 前 200 字节相同、后面不同的两本书不能共用一个边车
        let other = format!("{}cd.epub", "书".repeat(81));
        assert_ne!(file_name_for(&other), short);
        // 原子写的临时名（目标名截到 200 字节 + `.pid.seq.tmp`）也不超长：真的写一次、读回来
        let t = tempfile::tempdir().unwrap();
        let book = t.path().join(&long);
        std::fs::write(&book, b"x").unwrap();
        update(&book, |d| d.native = Some(5)).unwrap();
        assert_eq!(read(&book).and_then(|d| d.native), Some(5));
        assert!(t.path().join(&short).is_file());
        assert_eq!(owners(t.path()).get(&short), Some(&book), "反查表认得短名边车是这本书的");
        let moved = t.path().join(&other);
        std::fs::rename(&book, &moved).unwrap();
        rename(&book, &moved);
        assert_eq!(read(&moved).and_then(|d| d.native), Some(5), "改名后边车跟着走");
        assert!(!t.path().join(&short).exists());
        remove(&moved);
        assert!(read(&moved).is_none());
    }
}
