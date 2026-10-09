//! Markdown 导出落盘：`notecore::export` 产出的纯文本写到 `$XDG_DATA_HOME/notes/vault/<书名>/`
//! （一章一个 `.md` + 一个书索引页）。曾有的 `manifest()` / `GET .../vault.json`（给 2026-09-18 已砍的
//! host `shelf notes pull` 读回落盘内容）没有别的调用方，2026-10-09 删掉。
//! **整理区第三轮反馈（2026-09-08）加了指纹比对**：`export_state.rs` 记"上次导出时的内容指纹"，
//! 指纹没变就跳过重写（不再是无条件每次全量重写）——跟落设备笔记本那条投影路径（`publish.rs` +
//! `notebooks.rs`）用同一套纪律；顺带给「整理」页提供"这一章 md 是不是已经跟当前内容同步"的判据，
//! 见 `main.rs` 新增的 `GET .../sync`。出错只影响那一章内容旧，不会半写坏文件（落盘走 `rmsvc_core::fs::write_atomic`：
//! 同目录临时文件 → rename；此前直接 `fs::write` 是先截断再写，掉电/崩溃会留下半截 md）。**2026-09-08 三期追加**：光落盘在设备上用户够不着（得 SSH），`GET .../export.md`
//! （`main.rs`）额外把同一份内容直接当浏览器下载返回——`content_disposition()` 给的文件名走
//! RFC 5987（`filename*=UTF-8''...`，中文文件名要这个；纯 ASCII 兜底 `filename=` 给老客户端），由
//! `rmsvc_core::multipart::content_disposition` 生成（与书架原件下载同一份）。
use crate::export_state::{ExportRecord, ExportState};
use notecore::model::Book;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 文件名不能带路径分隔符（书名/章名理论上可能带用户手滑打进去的 `/`）——替换成 `_`，不做更复杂的
/// 转义（其余字符 xochitl 书名场景本来就不会出现更奇怪的控制字符）。
/// 整段是空串/`.`/`..`（书名被改成这种样子）时换成 `_`：否则 `vault/..` 会把整本书写到 vault 目录之外。
fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
    if s.is_empty() || s == "." || s == ".." { "_".to_string() } else { s }
}

pub fn vault_dir(data_dir: &Path, book_title: &str) -> PathBuf {
    data_dir.join("vault").join(sanitize(book_title))
}

/// 一章导出的结果——跟 `publish::ChapterOutcome` 同一套三态，理由一样：`Unchanged` 不是失败，是
/// "指纹没变，没必要重写"；网页拿这个字段决定要不要提示"没有变化，未重新导出"。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportOutcome {
    Written,
    Unchanged,
    Empty,
}
impl ExportOutcome {
    pub fn has_content(self) -> bool {
        !matches!(self, ExportOutcome::Empty)
    }
}

/// 导出一本书的全部内容（各章 + 索引页）落盘；返回每一章的结果（按章序号排列，含空章）。
pub fn export_book(data_dir: &Path, book: &Book, state: &ExportState) -> Result<Vec<ExportOutcome>, String> {
    let dir = vault_dir(data_dir, &book.title);
    std::fs::create_dir_all(&dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let mut outcomes = Vec::with_capacity(book.chapters.len());
    let mut any_content = false;
    for (idx, title) in book.chapters.iter().enumerate() {
        let o = export_chapter(&dir, book, idx, title, state)?;
        any_content |= o.has_content();
        outcomes.push(o);
    }
    // 索引页"哪些章有内容"取决于全书，只要有任何一章有内容就重写；全书都没内容就不落索引（也不用
    // 判断"要不要删掉旧索引"——空书这个状态本来就不该走到导出这步，真出现了留一份旧索引不算大问题）。
    if any_content {
        if let Some(md) = notecore::export::export_index_md(book) {
            let path = dir.join(format!("{}.md", sanitize(&book.title)));
            rmsvc_core::fs::write_atomic(&path, md.as_bytes()).map_err(|e| format!("写 {} 失败: {e}", path.display()))?;
        }
    }
    Ok(outcomes)
}

/// 导出单章（+ 顺带刷新索引页，因为这一章"有没有内容"可能因此变化）：指纹跟上次导出一样就跳过
/// （`Unchanged`），这一章真的没有活条目了才清掉旧记录（`Empty`，避免"整理"页一直显示"已同步"）——
/// 条目还活着、只是这次不想要 Obsidian 这个去处（比如切到 `Notebook`）不算"没有"，不清（2026-09-17
/// 真机 bug 修复，跟 `publish.rs::generate_chapter` 同一处理，见那边的详细注释；`.md` 文件本身这条
/// 路径从来不删，只是本地记录清了会让「整理」页的徽章凭空消失）。
pub fn export_chapter(dir: &Path, book: &Book, chapter_idx: usize, title: &str, state: &ExportState) -> Result<ExportOutcome, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let Some(fingerprint) = notecore::export::fingerprint_chapter(book, chapter_idx) else {
        if !book.chapter_has_live_entries(chapter_idx) {
            state.clear(&book.uuid, chapter_idx)?;
        }
        return Ok(ExportOutcome::Empty);
    };
    let existing = state.get(&book.uuid, chapter_idx);
    if existing.as_ref().map(|r| r.fingerprint.as_str()) == Some(fingerprint.as_str()) {
        return Ok(ExportOutcome::Unchanged);
    }
    // 指纹是 Some，md 也该有内容（两者算的是同一份 live_entries）；万一不一致宁可报错也不 panic。
    let md = notecore::export::export_chapter_md(book, chapter_idx).ok_or_else(|| format!("第 {} 章有指纹却没有可导出内容（内部不一致）", chapter_idx + 1))?;
    let path = dir.join(format!("{}.md", sanitize(&notecore::export::chapter_stem(chapter_idx, title))));
    rmsvc_core::fs::write_atomic(&path, md.as_bytes()).map_err(|e| format!("写 {} 失败: {e}", path.display()))?;
    state.set(&book.uuid, chapter_idx, ExportRecord { fingerprint, exported_at: rmsvc_core::clock::now_secs() })?;
    Ok(ExportOutcome::Written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecore::model::{Entry, Status, Style};

    fn entry(id: &str, chapter: usize, page_index: usize) -> Entry {
        Entry {
            id: id.into(),
            page: "p".into(),
            page_index,
            chapter: Some(chapter),
            chapter_title: String::new(),
            subhead: None,
            quote: None,
            ink: None,
            drafts: vec![],
            text: Some("内容".into()),
            style: Style::Body,
            ask_ai: false,
            question: None,
            answer: None,
            status: Status::Reviewed,
            destination: Default::default(),
            source: Default::default(),
            created: 0,
            updated: 0,
        }
    }

    fn book() -> Book {
        Book { uuid: "u".into(), title: "人骨拼图".into(), author: String::new(), chapters: vec!["第一章".into(), "空章".into()], entries: vec![entry("e1", 0, 0)], page_mtimes: Default::default() }
    }
    fn state(tmp: &std::path::Path) -> ExportState {
        let st = ExportState::new(tmp.join("exports"));
        st.ensure().unwrap();
        st
    }

    #[test]
    fn writes_chapter_and_index_files_skips_empty_chapter() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let outcomes = export_book(tmp.path(), &book(), &st).unwrap();
        assert_eq!(outcomes, [ExportOutcome::Written, ExportOutcome::Empty], "第一章写了，空章没内容");
        let dir = vault_dir(tmp.path(), "人骨拼图");
        assert!(dir.join("第1章 第一章.md").is_file());
        assert!(dir.join("人骨拼图.md").is_file());
        assert!(!dir.join("第2章 空章.md").exists());
    }

    #[test]
    fn empty_book_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let mut b = book();
        b.entries.clear();
        let outcomes = export_book(tmp.path(), &b, &st).unwrap();
        assert!(outcomes.iter().all(|o| *o == ExportOutcome::Empty));
        assert!(!vault_dir(tmp.path(), "人骨拼图").join("人骨拼图.md").exists());
    }

    #[test]
    fn second_call_with_same_content_is_unchanged_and_does_not_rewrite_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        export_book(tmp.path(), &book(), &st).unwrap();
        let path = vault_dir(tmp.path(), "人骨拼图").join("第1章 第一章.md");
        let mtime1 = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let outcomes = export_book(tmp.path(), &book(), &st).unwrap();
        assert_eq!(outcomes[0], ExportOutcome::Unchanged, "内容没变，指纹跟上次导出记的一样");
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime1, "文件真没被重写");
    }

    #[test]
    fn rewrite_is_idempotent_and_overwrites_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        export_book(tmp.path(), &book(), &st).unwrap();
        let mut b2 = book();
        b2.entries[0].text = Some("崭新文字".into());
        let outcomes = export_book(tmp.path(), &b2, &st).unwrap();
        assert_eq!(outcomes[0], ExportOutcome::Written, "内容变了，指纹跟不上，要重写");
        let content = std::fs::read_to_string(vault_dir(tmp.path(), "人骨拼图").join("第1章 第一章.md")).unwrap();
        assert!(content.contains("崭新文字 ^e1\n"));
        assert!(!content.contains("内容 ^e1\n"), "旧内容被整文件覆盖，不是追加: {content}");
    }

    #[test]
    fn chapter_becoming_empty_clears_the_synced_record() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        export_book(tmp.path(), &book(), &st).unwrap();
        assert!(st.get("u", 0).is_some());
        let mut emptied = book();
        emptied.entries.clear();
        export_book(tmp.path(), &emptied, &st).unwrap();
        assert!(st.get("u", 0).is_none(), "章没内容了，旧的\"已同步\"记录该清掉，不然「整理」页会误判成还同步着");
    }

    /// 2026-09-17 真机 bug 的另一半（对称于 `publish.rs` 的同名场景）：条目没死，只是从
    /// `Notebook`/`Both` 切成纯笔记本（不再要 Obsidian），`.md` 文件本身这条路径不删，但旧版本
    /// 会把 `exported_at` 记录也清掉，「整理」页的 Obsidian 徽章因此凭空消失。
    #[test]
    fn switching_destination_away_from_obsidian_keeps_the_record_not_ghost_synced() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        export_book(tmp.path(), &book(), &st).unwrap();
        assert!(st.get("u", 0).is_some(), "先正常导出一次");
        let mut switched = book();
        switched.entries[0].destination = notecore::model::Destination::Notebook; // 条目还活着，只是不再要 Obsidian 了
        let outcomes = export_book(tmp.path(), &switched, &st).unwrap();
        assert_eq!(outcomes[0], ExportOutcome::Empty, "这次没有条目要 Obsidian，仍然是 Empty");
        assert!(st.get("u", 0).is_some(), "但条目没死，历史记录不该被清掉——已经导出的 .md 文件还在磁盘上");
    }

    #[test]
    fn sanitizes_slash_in_titles_to_avoid_path_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let mut b = book();
        b.title = "带/斜杠的书名".into();
        export_book(tmp.path(), &b, &st).unwrap();
        assert!(vault_dir(tmp.path(), "带/斜杠的书名").is_dir());
        assert_eq!(vault_dir(tmp.path(), "带/斜杠的书名").file_name().unwrap(), "带_斜杠的书名");
        for bad in ["..", ".", ""] {
            assert_eq!(vault_dir(tmp.path(), bad), tmp.path().join("vault/_"), "{bad:?} 不能落到 vault 之外/vault 本身");
        }
    }



}
