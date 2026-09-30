//! 落库记录边车（Repository）：母版库每本书旁的隐藏 JSON `.<文件名>.delivered`——各读器最近一次落库的 unix 秒 +
//! 最近一次投原生的渲染自检结果 + 最近一次优化 / 落库的异步结果。只管"读 / 改 / 删这份记录"，
//! 母版库动作（入库/优化/落库）在 `staging`，自检逻辑在 `render_check`；两边都通过这里落盘，谁也不碰
//! 对方的字段语义（2026-09-06 从 staging.rs 拆出）。
use serde::{Deserialize, Serialize};
use rmsvc_core::fs::write_atomic;
use std::path::{Path, PathBuf};

/// 落库记录：各读器最近一次落库的 unix 秒；`render`=最近一次投原生的渲染自检结果。
/// 旧版边车里可能还带着已退役的字段——`source`（host CLI 洗书产物的原始输入身份，2026-09-18 CLI 已砍、此后没有写方）、
/// `direction`（按书阅读方向，2026-09-30 已撤）、`deliver.progress`（按卷拆分投递的进度，同日已撤）：serde 缺省忽略
/// 未知字段，照常读。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Delivered {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<u64>,
    /// KOReader 落库时刻（网关批量「加入 KOReader」经 `POST /staging/mark` 记；KOReader 已从设备卸载，旧记录照常显示）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub koreader: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderCheck>,
    /// 最近一次「优化」的结果（`staging::Staging::spawn_optimize` 异步执行时写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimize: Option<OptimizeCheck>,
    /// 最近一次「落库」的结果（`staging::Staging::spawn_deliver` 异步执行时写，2026-09-19）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<DeliverCheck>,
}

/// 异步优化的结果：`status` = pending（后台线程跑着）/ ok / failed / cancelled。`message` 是回执文案
/// （成功＝"已优化《...》（...）"；失败＝错误原因）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct OptimizeCheck {
    pub status: String,
    pub message: String,
    pub at: u64,
    /// 条目级进度（已处理/总条目数，不是字节）——`optimize_epub_file_streaming` 阶段二逐条目写出
    /// 时回调（2026-09-19 用户反馈"进度条一直感觉不会动"）。网页据此画百分比条，没有就画不确定进度的滚动条。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<StepProgress>,
}

/// 异步落库的结果：`status` = pending（后台线程跑着）/ ok / failed。`message` 是回执文案（成功＝"已加入 xochitl《...》"；
/// 失败＝错误原因）。跟 [`OptimizeCheck`] 分开成两个类型，是因为它们是两件独立的事。落库只有整本上传 / 大文件通道
/// 两条路，都是一步到位、没有分步进度（按卷拆分投递连同它的进度字段 2026-09-30 已撤）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DeliverCheck {
    pub status: String,
    pub message: String,
    pub at: u64,
}

/// 分步进度：`done`＝已经完成的步数，`total`＝这次操作总共会有多少步（EPUB 优化：一步＝阶段二写出一个条目）。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StepProgress {
    pub done: u32,
    pub total: u32,
}

/// 渲染自检结果：`status` = pending（等 xochitl 渲染）/ ok / warn（页数远低于期望＝整章渲染失败）/ timeout。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RenderCheck {
    pub uuid: String,
    pub pages: u64,
    pub expected: u64,
    pub status: String,
    pub at: u64,
}

/// 边车路径：`.<文件名>.delivered`（同目录、隐藏名，母版库列表按点开头跳过）。
pub fn path_for(book: &Path) -> PathBuf {
    let name = book.file_name().and_then(|s| s.to_str()).unwrap_or("book");
    book.with_file_name(format!(".{name}.delivered"))
}

pub fn read(book: &Path) -> Option<Delivered> {
    serde_json::from_slice(&std::fs::read(path_for(book)).ok()?).ok()
}

/// 读—改—原子写。没有边车从空记录起。
///
/// 全局互斥：优化进度回调、渲染自检线程、HTTP 线程会并发改同一份边车，`write_atomic` 只保证文件不写一半、
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
        update(&book, |d| d.render = Some(RenderCheck { uuid: "u".into(), pages: 3, expected: 4, status: "ok".into(), at: 1 })).unwrap();
        let d = read(&book).unwrap();
        assert_eq!((d.native, d.koreader), (Some(7), None));
        assert_eq!(d.render.as_ref().map(|r| r.pages), Some(3));
        // 旧版边车（无 render 字段）照读；带已退役字段的也照读、字段忽略：`direction`（按书方向，2026-09-30 撤）、
        // `source`（CLI 洗书原始输入，CLI 09-18 砍）、`deliver.progress`（按卷拆分投递进度，09-30 撤）
        std::fs::write(path_for(&book), br#"{"native":1,"koreader":2,"direction":"rtl","source":{"name":"a.pdf","bytes":9},"deliver":{"status":"ok","message":"m","at":3,"progress":{"done":1,"total":2}}}"#).unwrap();
        let deliver = Some(DeliverCheck { status: "ok".into(), message: "m".into(), at: 3 });
        assert_eq!(read(&book), Some(Delivered { native: Some(1), koreader: Some(2), render: None, optimize: None, deliver }));
        remove(&book);
        assert!(read(&book).is_none());
        remove(&book);
    }
}
