//! MOC 死链体检 / 全库卡片 ID 索引（纯逻辑，host 可测）。
//!
//! 背景：用户用卡片盒（Zettelkasten）+ MOC（Map of Content）组织知识——卡片顶部有机器锚点
//! `[ID: 章名-p页]`，MOC / 卡片里用「指向 → ID: X」软链接互指。reMarkable 无跨文件硬超链接、
//! 软链接靠全局搜索，且**删卡不会自动清理指向它的链接 → 死链**。本模块只做「体检」：扫全库
//! 笔记本文本，列出所有已定义锚点 + 找出指向不存在锚点的死链。**不自动生成 MOC**（手动策展
//! 才是知识结网的价值），只补上「死链无法自动更新」这个用户明确列出的痛点。
//!
//! 纯字符串扫描、不引 regex：锚点定义 `[ID: X]`、软链接引用 `指向/链接 … ID: X`。

use std::collections::BTreeSet;

/// 全库卡片索引：已定义锚点集 + 所有软链接引用（来源笔记本名, 目标 ID）。
#[derive(Debug, Default, PartialEq)]
pub struct CardIndex {
    pub defined: BTreeSet<String>,
    pub refs: Vec<(String, String)>,
}

/// 抽一段文本里所有已定义锚点 `[ID: X]`（`]` 终止 → 目标可含空格，如 "Book Two - Chapter Eleven-p200"）。
pub fn defined_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(p) = rest.find("[ID:") {
        let after = &rest[p + "[ID:".len()..];
        match after.find(']') {
            Some(end) => {
                let id = after[..end].trim().to_string();
                if !id.is_empty() {
                    out.push(id);
                }
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// 定位一行里 `ID:` / `ID：`（含全角冒号、允许 ID 与冒号间有空格）后紧邻的位置（字节偏移）。
fn id_sep_end(line: &str) -> Option<usize> {
    for pat in ["ID:", "ID：", "ID :", "ID ："] {
        if let Some(p) = line.find(pat) {
            return Some(p + pat.len());
        }
    }
    None
}

/// 抽一段文本里所有软链接引用的目标 ID。判据：**该行含「指向」或「链接」**（软链接意图）
/// 且有 `ID:`/`ID：` → 目标 = 分隔符后到 `]` 或行尾，trim，非空才收。
/// 这样兼容 `指向 -> ID: X`、`🔗 指向 → ID：X`、`[链接/ID: X]` 三种写法；
/// 纯定义行 `[ID: …]`（不含指向/链接）被跳过；scaffold 空的「🔗 指向 → ID：」目标空被跳过。
pub fn referenced_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !(line.contains("指向") || line.contains("链接")) {
            continue;
        }
        if let Some(sep_end) = id_sep_end(line) {
            let after = &line[sep_end..];
            let target = match after.find(']') {
                Some(e) => &after[..e],
                None => after,
            };
            let t = target.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// 聚合全库：`docs` = (笔记本名, 全页文本 join)。任一 doc 既可定义锚点也可引用。
pub fn build_index(docs: &[(String, String)]) -> CardIndex {
    let mut idx = CardIndex::default();
    for (title, text) in docs {
        for id in defined_ids(text) {
            idx.defined.insert(id);
        }
        for t in referenced_ids(text) {
            idx.refs.push((title.clone(), t));
        }
    }
    idx
}

/// 死链：引用的目标 ID 不在已定义锚点集里。返回 (来源笔记本名, 目标 ID)，按出现顺序去重。
pub fn dead_links(idx: &CardIndex) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (book, target) in &idx.refs {
        if !idx.defined.contains(target) && seen.insert((book.clone(), target.clone())) {
            out.push((book.clone(), target.clone()));
        }
    }
    out
}

/// 渲染 Markdown 体检报告：全库锚点清单 + 死链清单。
pub fn render_report(idx: &CardIndex) -> String {
    let dead = dead_links(idx);
    let mut s = String::new();
    s.push_str("# 卡片索引 / MOC 死链体检\n\n");
    s.push_str(&format!(
        "> {} 个锚点 · {} 处软链接 · {} 处死链 · daemon 只读扫描生成\n\n",
        idx.defined.len(),
        idx.refs.len(),
        dead.len()
    ));

    s.push_str("## 死链（指向了不存在的 ID）\n\n");
    if dead.is_empty() {
        s.push_str("✓ 无死链\n\n");
    } else {
        for (book, target) in &dead {
            s.push_str(&format!("- ⚠ 《{book}》 指向 → `{target}` ← 该 ID 不存在\n"));
        }
        s.push('\n');
    }

    s.push_str(&format!("## 全库锚点（{} 个）\n\n", idx.defined.len()));
    for id in &idx.defined {
        s.push_str(&format!("- `[ID: {id}]`\n"));
    }
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defined_handles_spaces_and_multiple() {
        let t = "标题\n[ID: 第一章-p3] 正文\n中间\n[ID: Book Two - Chapter Eleven-p200]";
        assert_eq!(defined_ids(t), vec!["第一章-p3", "Book Two - Chapter Eleven-p200"]);
    }

    #[test]
    fn referenced_three_syntaxes_and_fullwidth() {
        let t = "\
指向 -> ID: 沉没成本\n\
🔗 指往无关行\n\
🔗 指向 → ID：损失厌恶\n\
· 顶部 [链接/ID: 沙丘-资源控制] 接驳\n\
🔗 指向 → ID：\n\
[ID: 定义行-p1]";
        let mut got = referenced_ids(t);
        got.sort();
        assert_eq!(got, vec!["损失厌恶", "沉没成本", "沙丘-资源控制"]);
        // 纯定义行不被当引用；空目标被跳过。
        assert!(!got.contains(&"定义行-p1".to_string()));
    }

    #[test]
    fn dead_links_flags_missing_targets() {
        let docs = vec![
            ("《书A》- 总结卡片".to_string(), "[ID: 认知-损失厌恶]\n🔗 指向 → ID：沉没成本陷阱".to_string()),
            ("000-全局MOC".to_string(), "· 认知模型 ➔ [链接/ID: 认知-损失厌恶]\n· 断链 ➔ [链接/ID: 已删卡]".to_string()),
        ];
        let idx = build_index(&docs);
        assert!(idx.defined.contains("认知-损失厌恶"));
        let dead = dead_links(&idx);
        // 「认知-损失厌恶」有定义 → 不算死链；「沉没成本陷阱」「已删卡」无定义 → 死链。
        let targets: BTreeSet<&str> = dead.iter().map(|(_, t)| t.as_str()).collect();
        assert!(targets.contains("沉没成本陷阱"));
        assert!(targets.contains("已删卡"));
        assert!(!targets.contains("认知-损失厌恶"));
    }

    #[test]
    fn report_no_dead_links_clean() {
        let docs = vec![("卡".to_string(), "[ID: a]\n指向 → ID：a".to_string())];
        let idx = build_index(&docs);
        let r = render_report(&idx);
        assert!(r.contains("✓ 无死链"), "{r}");
        assert!(r.contains("`[ID: a]`"));
    }
}
