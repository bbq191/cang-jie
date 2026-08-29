//! 设备笔记本 I/O 底层 —— 从 cj-stars-daemon 抽出的"读库 + 注入/替换/回收站"通用层，
//! 被全部 daemon 汇总本（卡片/索引/汇编/复盘/仪表/生词本）共用。抽出后可 host 测、daemon 变薄。
//!
//! 关键机制（真机命门，见 PKM 白皮书 §04/§05）：xochitl 运行时**无视磁盘直写**、只认 `/upload` 原生导入，
//! 且每次 upload 强制换 uuid → 更新一本笔记本 = 传新本 + 旧同名本入 pending-trash 队列（trash-agent 走
//! 原生 `selectionMoveToTrash` 内存即时移回收站）。**绝不直接改磁盘 metadata parent=trash**（xochitl 内存不认）。

use crate::cardnote;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use device_core::inject;

/// xochitl 本地 /upload 端点（USB 网卡，本机走本地路由可达、无需真插 USB）。
pub const UPLOAD_HOST: &str = "10.11.99.1";
/// 旧本回收队列（trash-agent.qmd 消费，走原生代码路移回收站）。
pub const PENDING_TRASH: &str = "/home/root/weread/pending-trash.json";

fn read_json(path: &str) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// 按 visibleName 精确匹配、在库（parent != trash）的文档，(uuid, lastModified) 降序（最新在前）。
pub fn find_docs_by_visible(dir: &str, visible: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                continue;
            }
            let v = read_json(p.to_str().unwrap_or(""));
            if v.get("visibleName").and_then(|x| x.as_str()) != Some(visible) {
                continue;
            }
            if v.get("parent").and_then(|x| x.as_str()) == Some("trash") {
                continue;
            }
            let uuid = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let lm = v.get("lastModified").and_then(|x| x.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            out.push((uuid, lm));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// 读一本笔记本各页 RootText（cPages 顺序）。
pub fn read_card_pages(dir: &str, uuid: &str) -> Vec<String> {
    cardnote::list_pages(dir, uuid).map(|rows| rows.into_iter().map(|(_, _, t)| t).collect()).unwrap_or_default()
}

/// 一个文档是否是"用户笔记本"（含卡片本 + MOC 本）：metadata type=DocumentType 且 content fileType=notebook。
/// 用于判断本轮 dirty 是否需要重建死链索引（epub/文件夹不算）。
pub fn is_user_notebook(dir: &str, uuid: &str) -> bool {
    let meta = read_json(&format!("{dir}/{uuid}.metadata"));
    if meta.get("type").and_then(|x| x.as_str()) != Some("DocumentType") {
        return false;
    }
    read_json(&format!("{dir}/{uuid}.content")).get("fileType").and_then(|x| x.as_str()) == Some("notebook")
}

/// 扫全库笔记本文本 → (笔记本名, 全页文本 join)。**只读文本、不碰笔迹几何**（cardindex 死链体检用）。
/// 只收 fileType=notebook（卡片本 + 用户 MOC 本），跳过 epub 与回收站。`exclude`=按标题排除的汇总本自身
/// （否则回喂自指 + 上传自触发无限重建）。
pub fn collect_notebook_texts(dir: &str, exclude: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                continue;
            }
            let uuid = match p.file_stem().and_then(|s| s.to_str()) {
                Some(u) => u.to_string(),
                None => continue,
            };
            let meta = read_json(p.to_str().unwrap_or(""));
            if meta.get("type").and_then(|x| x.as_str()) != Some("DocumentType") {
                continue;
            }
            if meta.get("parent").and_then(|x| x.as_str()) == Some("trash") {
                continue;
            }
            if !is_user_notebook(dir, &uuid) {
                continue;
            }
            let title = meta.get("visibleName").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if exclude.contains(&title.as_str()) {
                continue;
            }
            out.push((title, read_card_pages(dir, &uuid).join("\n")));
        }
    }
    out
}

/// 文档目录最近改动时间（秒，取页目录 mtime）——判断用户是否刚在编辑（防竞态）。
pub fn doc_age_secs(dir: &str, uuid: &str) -> u64 {
    std::fs::metadata(format!("{dir}/{uuid}"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            let mtime = t.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(now.saturating_sub(mtime))
        })
        .unwrap_or(u64::MAX)
}

/// 把旧本 uuid 推进 pending-trash 队列，由 trash-agent 走 xochitl 原生 selectionMoveToTrash 移回收站
/// （**内存即时生效、无形无感**）。⚠ 绝不直接改磁盘 metadata parent=trash（xochitl 内存不认、界面残留）。
pub fn queue_trash(uuid: &str) {
    let mut q: Vec<String> = std::fs::read_to_string(PENDING_TRASH)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if !q.iter().any(|u| u == uuid) {
        q.push(uuid.to_string());
        let _ = std::fs::write(PENDING_TRASH, serde_json::to_string(&q).unwrap_or_else(|_| "[]".into()));
    }
}

/// 把 daemon 自动生成的只读本同步成库内笔记本（复用卡片的 pack_rmdoc+upload+trash 链）。
/// 内容未变（对最新本逐字相同）→ 绝不上传（防 churn + 防自触发循环）；多余旧本入 trash 队列。
/// `folder`=归档目标文件夹 uuid（空=root）；上传前 `GET /documents/<folder>` 设当前文件夹，卡片落进去。
pub fn sync_auto_notebook(dir: &str, title: &str, pages: &[String], folder: &str) -> Option<String> {
    let existing = find_docs_by_visible(dir, title);
    let new_text = pages.join("\n");
    if let Some((u, _)) = existing.first() {
        if read_card_pages(dir, u).join("\n") == new_text {
            for (u2, _) in existing.iter().skip(1) {
                queue_trash(u2);
            }
            return None;
        }
    }
    let new_uuid = uuid::Uuid::new_v4().to_string();
    let refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
    let rmdoc = cardnote::pack_rmdoc(&new_uuid, title, &refs).ok()?;
    let net = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    inject::set_upload_folder(&net, UPLOAD_HOST, folder); // 落进 zettelkasten（空=root 兜底）
    if inject::upload_document(&net, UPLOAD_HOST, &rmdoc, &format!("{title}.rmdoc"), "application/zip").is_err() {
        return None;
    }
    for (u, _) in &existing {
        queue_trash(u);
    }
    Some(if existing.is_empty() { "创建" } else { "更新" }.to_string())
}
