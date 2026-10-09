//! 原生书库的**只读元数据模型**：`<uuid>.metadata`/`.content` 的查询（找文件夹、列文件夹、文档去重命名、
//! 按创建时间圈"刚进库的那本"、渲染页数）。纯文件系统读取，不碰 HTTP——和上传客户端（父模块 [`super::Xochitl`]）
//! 分开，各自单一职责；对外路径仍是 `rmsvc_core::xochitl::*`（父模块 `pub use` 再导出）。
use std::path::Path;

/// 书库目录里所有可解析的 `<uuid>.metadata` → (uuid, JSON)。只读；解析失败的跳过。
fn metadata_entries(dir: &Path) -> Vec<(String, serde_json::Value)> {
    metadata_entries_since(dir, None)
}

/// 同 [`metadata_entries`]，`min_mtime` 给定时**只打开 mtime 不早于它的**：先用目录项自带的 stat 挡掉旧文件，
/// 不再对整个书库（几十上百份）逐个 open+读+解析 JSON——渲染自检/占位等待这类"找刚进库的那本"的调用会在
/// 一个 3 秒防抖 / 200ms 轮询循环里反复扫描（2026-09-22 审计）。
fn metadata_entries_since(dir: &Path, min_mtime: Option<std::time::SystemTime>) -> Vec<(String, serde_json::Value)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                return None;
            }
            if let Some(floor) = min_mtime {
                if e.metadata().ok()?.modified().ok()? < floor {
                    return None;
                }
            }
            let uuid = p.file_stem()?.to_str()?.to_string();
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some((uuid, v))
        })
        .collect()
}

fn str_of<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

/// 非回收站、未删除的条目（文件夹与文档共用的过滤）。
fn is_live(v: &serde_json::Value) -> bool {
    str_of(v, "parent") != "trash" && v.get("deleted").and_then(|x| x.as_bool()) != Some(true)
}

/// 书库里所有**活的**条目（非回收站、未删除，文件夹与文档都有）→ (uuid, 强类型 [`Metadata`])。只读；读不了、
/// 解析不了（含字段类型不对，如 `deleted` 不是布尔）的跳过。网关「设备健康 → 清理」列书库、book-serve 直接导入
/// 查同文件夹重复用（10-09 起直接给 `Metadata`，调用方不再各自 `from_value` 一遍）。
pub fn live_entries(dir: &Path) -> Vec<(String, Metadata)> {
    metadata_entries(dir).into_iter().filter_map(|(uuid, v)| serde_json::from_value::<Metadata>(v).ok().filter(Metadata::is_live).map(|m| (uuid, m))).collect()
}

/// `.metadata` 的 `createdTime`（毫秒；xochitl 写成字符串，也认数字）；缺或解析不了 → 0。
pub fn created_ms(v: &serde_json::Value) -> u64 {
    v.get("createdTime").and_then(|x| x.as_str().and_then(|s| s.parse().ok()).or_else(|| x.as_u64())).unwrap_or(0)
}

/// 是不是 xochitl 文档 uuid 的形状（36 字符，只含十六进制与 `-`）。拿来当文件名片段之前先过一遍，
/// 防路径注入（`../`）；只看形状，不代表书库里真有这份文档。
pub fn is_uuid_shape(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 按 visibleName 在**整个书库**里找活文件夹（不管它在哪一层），多个同名取先扫到的。网页「加入 xochitl → 文件夹」用；
/// 要按层找（某个父文件夹下的某个名字）用 [`find_child_folder`]。
pub fn find_folder_by_name(dir: &Path, name: &str) -> Option<String> {
    metadata_entries(dir).into_iter().find(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v) && str_of(v, "visibleName") == name).map(|(uuid, _)| uuid)
}

/// 在 `parent`（文件夹 uuid，空串＝书库根）**正下方**找名叫 `name` 的活文件夹（`CollectionType`、没删、不在回收站），
/// 返回它的 uuid（2026-10-07 直接导入按层建多级文件夹用）。同一层有多个同名的取 uuid 最小的那个，结果不随目录扫描顺序变。
pub fn find_child_folder(dir: &Path, parent: &str, name: &str) -> Option<String> {
    metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v) && str_of(v, "parent") == parent && str_of(v, "visibleName") == name)
        .map(|(uuid, _)| uuid)
        .min()
}

/// 书库里所有活文件夹的（父文件夹 uuid, 名字）——建文件夹队列每次唤醒判"哪些已经建出来了"用，整库只扫一遍。
pub fn folder_keys(dir: &Path) -> std::collections::HashSet<(String, String)> {
    metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v))
        .map(|(_, v)| (str_of(&v, "parent").to_string(), str_of(&v, "visibleName").to_string()))
        .collect()
}

/// 文件夹 `folder` 从书库根往下的完整路径，各级名字用 `/` 连起来（`漫画/死亡筆記(愛藏版)`）；空串（书库根）→ 空串。
/// 往上找 `parent` 直到根；中途某一级读不到、不是文件夹、在回收站、或层数超过 64（防 `.metadata` 里成环）就停在那里，
/// 只拼已经找到的那几级。名字里本来带 `/` 的（如《乱马1/2》）原样拼进去，路径就有歧义——只给人看、给客户端比对，不再拆回去用。
pub fn folder_path_of(dir: &Path, folder: &str) -> String {
    let mut names = Vec::new();
    let mut cur = folder.to_string();
    while !cur.is_empty() && is_uuid_shape(&cur) && names.len() < 64 {
        let Some(v) = read_metadata(dir, &cur) else { break };
        if str_of(&v, "type") != "CollectionType" || !is_live(&v) {
            break;
        }
        names.push(str_of(&v, "visibleName").to_string());
        cur = str_of(&v, "parent").to_string();
    }
    names.reverse();
    names.join("/")
}

/// 原生书库里所有活文件夹的名字（去重、按名排序）——给网页「加入原生书库 → 文件夹」下拉候选用，
/// 反映设备上**真实存在**的文件夹（跟已删的 koreader-serve 当年给 KOReader 目录下拉候选同一个道理），不是
/// 写死的预设列表（2026-09-19 用户反馈：原来的「书库/批注/自定义」三选一预设看不出真实文件夹，
/// 批注那档还常年跟书库撞成一样，见书架白皮书对应记录）。
pub fn list_folders(dir: &Path) -> Vec<String> {
    metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "CollectionType" && is_live(v))
        .map(|(_, v)| str_of(&v, "visibleName").to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 读一份 `<uuid>.metadata`（JSON）；不在、读不了、不是合法 JSON → `None`。调用方自己校验 uuid 形状（`is_uuid_shape`）。
/// 书架的回收站代理、直接导入和这里的 [`parent_folder_of`] 共用（此前三处各读一遍）。
pub fn read_metadata(dir: &Path, uuid: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{uuid}.metadata"))).ok()?).ok()
}

/// 给定一份文档的 uuid，读它 `.metadata` 的 `parent` 字段——就是它当前所在的设备文件夹 uuid
/// （空串＝书库根）。找不到 `.metadata`、解析失败、或书在回收站（`parent=="trash"`），一律返回
/// `None`，调用方按 best-effort 落书库根处理（2026-09-09 补：`note-serve` 生成章节笔记本时不再
/// 新建/确保文件夹，改成直接复用书本自己已经在的文件夹）。
pub fn parent_folder_of(dir: &Path, uuid: &str) -> Option<String> {
    let v = read_metadata(dir, uuid)?;
    let parent = v.get("parent").and_then(|x| x.as_str())?;
    (parent != "trash").then(|| parent.to_string())
}

/// 在 `folder`（文件夹 uuid，空串＝根）范围内，如果 `base_name` 已经被别的活文档占用，就在末尾加
/// 数字后缀（`"标题"` → `"标题 2"` → `"标题 3"` ...）直到不冲突；没冲突就原样返回。只读 `.metadata`，
/// 不写、不建任何东西。
pub fn unique_document_name(dir: &Path, folder: &str, base_name: &str) -> String {
    let names: std::collections::HashSet<String> = metadata_entries(dir)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "DocumentType" && is_live(v) && str_of(v, "parent") == folder)
        .map(|(_, v)| str_of(&v, "visibleName").to_string())
        .collect();
    if !names.contains(base_name) {
        return base_name.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base_name} {n}");
        if !names.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// `<uuid>.metadata` 的强类型视图（只取各服务真用到的字段；其余字段 xochitl 自己管，这里不碰）。
/// 各服务此前各写 `v.get("type").and_then(as_str).unwrap_or("")` 再重判一遍"在不在回收站/删没删"。
#[derive(serde::Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Metadata {
    #[serde(rename = "visibleName", deserialize_with = "null_default")]
    pub visible_name: String,
    /// `DocumentType` / `CollectionType`。
    #[serde(rename = "type", deserialize_with = "null_default")]
    pub kind: String,
    /// 所在文件夹 uuid（空串＝根，`trash`＝回收站）。
    #[serde(deserialize_with = "null_default")]
    pub parent: String,
    #[serde(deserialize_with = "null_default")]
    pub deleted: bool,
    /// xochitl 写成毫秒字符串，也认数字；用 [`Metadata::created_ms`] 取。
    #[serde(rename = "createdTime")]
    pub created_time: serde_json::Value,
}

/// 字段写成 JSON `null` 时按缺省值处理（迁移前各服务手写的 `as_str().unwrap_or("")` 就是这样认的；
/// 不然一个 `"parent": null` 会让整条 `.metadata` 解析失败）。
fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    Ok(<Option<T> as serde::Deserialize>::deserialize(d)?.unwrap_or_default())
}

impl Metadata {
    /// 非回收站、未删除（与 [`live_entries`] 同一判据）。
    pub fn is_live(&self) -> bool {
        self.parent != "trash" && !self.deleted
    }
    pub fn is_document(&self) -> bool {
        self.kind == "DocumentType"
    }
    pub fn is_folder(&self) -> bool {
        self.kind == "CollectionType"
    }
    pub fn is_live_document(&self) -> bool {
        self.is_document() && self.is_live()
    }
    /// `createdTime`（毫秒）；缺或解析不了 → 0。
    pub fn created_ms(&self) -> u64 {
        self.created_time.as_str().and_then(|s| s.parse().ok()).or_else(|| self.created_time.as_u64()).unwrap_or(0)
    }
}

/// 读一份 `<uuid>.metadata` 成 [`Metadata`]，**区分"没有"与"读不了"**：文件不在 → `Ok(None)`（书被彻底删了）；
/// 读失败/解析失败 → `Err`（可能正被 xochitl 改写，调用方应跳过这次、别当成书没了）。调用方自己校验 uuid 形状。
pub fn read_meta(dir: &Path, uuid: &str) -> Result<Option<Metadata>, String> {
    let p = dir.join(format!("{uuid}.metadata"));
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读 {} 失败: {e}", p.display())),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| format!("{} 解析失败: {e}", p.display()))
}

/// `<uuid>.content` 的 `fileType`（`epub`/`pdf`/`notebook`）；流式只取这一个字段，不把整张页 id 表解析进内存。
pub fn file_type(dir: &Path, uuid: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct OnlyFileType {
        #[serde(rename = "fileType", default)]
        file_type: String,
    }
    let f = std::fs::File::open(dir.join(format!("{uuid}.content"))).ok()?;
    serde_json::from_reader::<_, OnlyFileType>(std::io::BufReader::new(f)).ok().map(|c| c.file_type).filter(|t| !t.is_empty())
}

/// 书库里一份文档（非文件夹、非回收站）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    pub uuid: String,
    pub visible_name: String,
    /// xochitl `createdTime`（毫秒字符串）。
    pub created_ms: u64,
}

/// `createdTime >= since_ms` 的文档，新→旧。投原生后找"刚进库的那本"用（`/upload` 不回 uuid；visibleName
/// 取自 EPUB 元数据不等于文件名，所以按时间圈候选、再按书名挑）。只读 `.metadata`，不写。
pub fn find_documents_since(dir: &Path, since_ms: u64) -> Vec<DocInfo> {
    // `createdTime >= since` 的文档，其 `.metadata` 一定是创建时或之后写的，mtime 不会更早（留 5 秒余量给文件系统
    // 时间戳粒度/时钟取整）；`since_ms == 0`（补记全库）不设下限。
    let floor = (since_ms > 0).then(|| std::time::UNIX_EPOCH + std::time::Duration::from_millis(since_ms.saturating_sub(5_000)));
    let mut out: Vec<DocInfo> = metadata_entries_since(dir, floor)
        .into_iter()
        .filter(|(_, v)| str_of(v, "type") == "DocumentType" && is_live(v))
        .filter_map(|(uuid, v)| {
            let created_ms = created_ms(&v);
            (created_ms >= since_ms).then(|| DocInfo { uuid, visible_name: str_of(&v, "visibleName").to_string(), created_ms })
        })
        .collect();
    out.sort_by_key(|d| std::cmp::Reverse(d.created_ms));
    out
}

/// `<uuid>.content` 的 `pageCount`：xochitl 渲染完（导入 / 打开）才写；缺或 0 → None。
pub fn page_count(dir: &Path, uuid: &str) -> Option<u64> {
    let t = std::fs::read_to_string(dir.join(format!("{uuid}.content"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    v.get("pageCount").and_then(|x| x.as_u64()).filter(|&n| n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_null_fields_fall_back_to_default() {
        let m: Metadata = serde_json::from_str(r#"{"visibleName":"书","type":"DocumentType","parent":null,"deleted":null}"#).unwrap();
        assert_eq!(m.parent, "");
        assert!(!m.deleted);
        assert!(m.is_live_document());
    }

    #[test]
    fn typed_metadata_and_file_type() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        std::fs::write(d.join("a.metadata"), r#"{"visibleName":"书","type":"DocumentType","parent":"","createdTime":"1700000000123","extra":1}"#).unwrap();
        std::fs::write(d.join("b.metadata"), r#"{"visibleName":"夹","type":"CollectionType","parent":"trash","createdTime":5}"#).unwrap();
        std::fs::write(d.join("c.metadata"), "{ half").unwrap();
        std::fs::write(d.join("a.content"), r#"{"fileType":"epub","pages":["p1","p2"]}"#).unwrap();
        let a = read_meta(d, "a").unwrap().unwrap();
        assert!(a.is_live_document() && !a.is_folder());
        assert_eq!((a.visible_name.as_str(), a.created_ms()), ("书", 1_700_000_000_123));
        let b = read_meta(d, "b").unwrap().unwrap();
        assert!(b.is_folder() && !b.is_live() && b.created_ms() == 5);
        assert_eq!(read_meta(d, "missing").unwrap(), None, "没有 → Ok(None)");
        assert!(read_meta(d, "c").is_err(), "半截 → Err，不当成没有");
        assert_eq!(file_type(d, "a").as_deref(), Some("epub"));
        assert_eq!(file_type(d, "b"), None);
        // live_entries：跳过回收站里的 b、半截的 c、已删除的 e、字段类型不对的 f，只剩 a（直接给强类型）
        std::fs::write(d.join("e.metadata"), r#"{"visibleName":"删","type":"DocumentType","parent":"","deleted":true}"#).unwrap();
        std::fs::write(d.join("f.metadata"), r#"{"visibleName":"怪","type":"DocumentType","parent":"","deleted":"yes"}"#).unwrap();
        let live = live_entries(d);
        assert_eq!(live.len(), 1);
        assert_eq!((live[0].0.as_str(), live[0].1.visible_name.as_str()), ("a", "书"));
    }

    #[test]
    fn uuid_shape_accepts_real_uuid_rejects_traversal() {
        assert!(is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(!is_uuid_shape("../../etc/passwd"));
        assert!(!is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef012345678"), "少一位");
        assert!(!is_uuid_shape("0a1b2c3d-4e5f-6789-abcd-ef012345678g"), "非十六进制");
    }

    #[test]
    fn finds_folder_skipping_trash_and_documents() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"CollectionType","visibleName":"library","parent":"trash"}"#);
        w("b.metadata", r#"{"type":"DocumentType","visibleName":"library","parent":""}"#);
        w("c.metadata", r#"{"type":"CollectionType","visibleName":"library","parent":""}"#);
        w("d.content", r#"{}"#);
        assert_eq!(find_folder_by_name(t.path(), "library"), Some("c".into()));
        assert_eq!(find_folder_by_name(t.path(), "none"), None);
    }

    /// 按层找：只认指定父文件夹正下方的；不同父文件夹下的同名子文件夹互不相混。
    #[test]
    fn find_child_folder_matches_parent_and_name() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("m.metadata", r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#);
        w("n.metadata", r#"{"type":"CollectionType","visibleName":"小说","parent":""}"#);
        w("m1.metadata", r#"{"type":"CollectionType","visibleName":"卷01","parent":"m"}"#);
        w("n1.metadata", r#"{"type":"CollectionType","visibleName":"卷01","parent":"n"}"#);
        w("t1.metadata", r#"{"type":"CollectionType","visibleName":"卷02","parent":"trash"}"#);
        w("d1.metadata", r#"{"type":"DocumentType","visibleName":"卷03","parent":"m"}"#);
        w("x1.metadata", r#"{"type":"CollectionType","visibleName":"卷04","parent":"m","deleted":true}"#);
        assert_eq!(find_child_folder(t.path(), "", "漫画"), Some("m".into()));
        assert_eq!(find_child_folder(t.path(), "m", "卷01"), Some("m1".into()));
        assert_eq!(find_child_folder(t.path(), "n", "卷01"), Some("n1".into()));
        assert_eq!(find_child_folder(t.path(), "", "卷01"), None, "根下没有卷01");
        assert_eq!(find_child_folder(t.path(), "m", "卷02"), None, "回收站里的不算");
        assert_eq!(find_child_folder(t.path(), "m", "卷03"), None, "文档不是文件夹");
        assert_eq!(find_child_folder(t.path(), "m", "卷04"), None, "已删的不算");
        let keys = folder_keys(t.path());
        assert!(keys.contains(&("m".to_string(), "卷01".to_string())) && keys.contains(&("n".to_string(), "卷01".to_string())));
        assert!(!keys.contains(&("m".to_string(), "卷04".to_string())));
    }

    #[test]
    fn folder_path_walks_up_to_root() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(format!("{n}.metadata")), j).unwrap();
        let (a, b, c) = ("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "cccccccc-cccc-4ccc-8ccc-cccccccccccc");
        w(a, r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#);
        w(b, &format!(r#"{{"type":"CollectionType","visibleName":"死亡筆記(愛藏版)","parent":"{a}"}}"#));
        w(c, &format!(r#"{{"type":"CollectionType","visibleName":"环","parent":"{c}"}}"#));
        assert_eq!(folder_path_of(t.path(), ""), "");
        assert_eq!(folder_path_of(t.path(), a), "漫画");
        assert_eq!(folder_path_of(t.path(), b), "漫画/死亡筆記(愛藏版)");
        assert_eq!(folder_path_of(t.path(), "dddddddd-dddd-4ddd-8ddd-dddddddddddd"), "", "读不到就是空");
        assert_eq!(folder_path_of(t.path(), c).split('/').count(), 64, "成环也会停");
    }

    #[test]
    fn lists_folders_deduped_sorted_skipping_trash_and_documents() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"CollectionType","visibleName":"雪人","parent":""}"#);
        w("b.metadata", r#"{"type":"CollectionType","visibleName":"批注","parent":""}"#);
        w("c.metadata", r#"{"type":"CollectionType","visibleName":"批注","parent":""}"#); // 同名文件夹去重
        w("d.metadata", r#"{"type":"CollectionType","visibleName":"回收站里的","parent":"trash"}"#);
        w("e.metadata", r#"{"type":"DocumentType","visibleName":"这是本书不是文件夹","parent":""}"#);
        assert_eq!(list_folders(t.path()), vec!["批注".to_string(), "雪人".to_string()]);
    }

    #[test]
    fn parent_folder_of_reads_parent_field_and_treats_trash_as_none() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("book-in-folder.metadata", r#"{"type":"DocumentType","visibleName":"人骨拼图","parent":"folder-uuid"}"#);
        w("book-at-root.metadata", r#"{"type":"DocumentType","visibleName":"飘","parent":""}"#);
        w("book-in-trash.metadata", r#"{"type":"DocumentType","visibleName":"删了","parent":"trash"}"#);
        assert_eq!(parent_folder_of(t.path(), "book-in-folder"), Some("folder-uuid".into()));
        assert_eq!(parent_folder_of(t.path(), "book-at-root"), Some(String::new()), "根目录是空串，不是 None");
        assert_eq!(parent_folder_of(t.path(), "book-in-trash"), None, "书在回收站，别把笔记也生成进去");
        assert_eq!(parent_folder_of(t.path(), "no-such-uuid"), None, "查不到就 None，调用方 best-effort 落根");
    }

    #[test]
    fn unique_document_name_appends_suffix_only_within_same_folder() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("a.metadata", r#"{"type":"DocumentType","visibleName":"楔子","parent":"f1"}"#);
        w("b.metadata", r#"{"type":"DocumentType","visibleName":"楔子 2","parent":"f1"}"#);
        w("c.metadata", r#"{"type":"DocumentType","visibleName":"楔子","parent":"f2"}"#);
        w("trashed.metadata", r#"{"type":"DocumentType","visibleName":"楔子 3","parent":"trash"}"#);
        assert_eq!(unique_document_name(t.path(), "f1", "楔子"), "楔子 3", "f1 下已有「楔子」和「楔子 2」（后者活着占用），下一个该是 3");
        assert_eq!(unique_document_name(t.path(), "f2", "楔子"), "楔子 2", "f2 只有一份同名，跟 f1 的计数互不影响");
        assert_eq!(unique_document_name(t.path(), "f3", "楔子"), "楔子", "f3 没有同名文档，原样返回");
        assert_eq!(unique_document_name(t.path(), "trash", "楔子 3"), "楔子 3", "回收站里的同名文档不算占用（is_live 过滤掉）");
    }

    #[test]
    fn finds_documents_since_newest_first_and_reads_page_count() {
        let t = tempfile::tempdir().unwrap();
        let w = |n: &str, j: &str| std::fs::write(t.path().join(n), j).unwrap();
        w("old.metadata", r#"{"type":"DocumentType","visibleName":"旧书","parent":"","createdTime":"1000"}"#);
        w("new.metadata", r#"{"type":"DocumentType","visibleName":"New Book","parent":"","createdTime":"3000"}"#);
        w("mid.metadata", r#"{"type":"DocumentType","visibleName":"Mid","parent":"","createdTime":"2000"}"#);
        w("tr.metadata", r#"{"type":"DocumentType","visibleName":"Trash","parent":"trash","createdTime":"5000"}"#);
        w("del.metadata", r#"{"type":"DocumentType","visibleName":"Del","parent":"","deleted":true,"createdTime":"5000"}"#);
        w("dir.metadata", r#"{"type":"CollectionType","visibleName":"Folder","parent":"","createdTime":"5000"}"#);
        w("new.content", r#"{"pageCount": 352, "fileType": "epub"}"#);
        w("mid.content", r#"{"pageCount": 0}"#);
        let docs = find_documents_since(t.path(), 2000);
        assert_eq!(docs.iter().map(|d| d.uuid.as_str()).collect::<Vec<_>>(), ["new", "mid"]);
        assert_eq!(docs[0].visible_name, "New Book");
        assert_eq!(page_count(t.path(), "new"), Some(352));
        assert_eq!(page_count(t.path(), "mid"), None, "0 页＝还没渲染");
        assert_eq!(page_count(t.path(), "old"), None, "没有 .content");
    }

    /// `find_documents_since` 用 mtime 下限先挡旧文件：结果语义不变（仍以 createdTime 为准），只是不再打开旧文件。
    #[test]
    fn find_documents_since_skips_stale_metadata_by_mtime_but_keeps_result_semantics() {
        let t = tempfile::tempdir().unwrap();
        let now = crate::clock::now_ms();
        let w = |n: &str, created: u64, age_secs: u64| {
            let p = t.path().join(n);
            std::fs::write(&p, format!(r#"{{"type":"DocumentType","visibleName":"{n}","parent":"","createdTime":"{created}"}}"#)).unwrap();
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs)).unwrap();
        };
        w("fresh.metadata", now, 0);
        w("shelved.metadata", now - 3_600_000, 3_600); // 一小时前进库、没再动过
        w("touched.metadata", now - 3_600_000, 0); // 老书但刚被 xochitl 改写过 .metadata：过 mtime 门，被 createdTime 挡掉
        let got: Vec<String> = find_documents_since(t.path(), now - 1_000).into_iter().map(|d| d.uuid).collect();
        assert_eq!(got, ["fresh"]);
        let all: Vec<String> = find_documents_since(t.path(), 0).into_iter().map(|d| d.uuid).collect();
        assert_eq!(all.len(), 3, "since=0（补记全库）不设 mtime 下限");
    }
}
