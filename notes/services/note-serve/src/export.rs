//! Markdown 导出落盘：`notecore::export` 产出的纯文本写到 `$XDG_DATA_HOME/notes/vault/<书名>/`
//! （一章一个 `.md` + 一个书索引页），供 host `notes pull` 拉到本机 Obsidian vault。按文件名幂等
//! 覆盖（每次全量重写文件内容，不做增量 diff——文件很小，重写比比对更简单可靠，出错也只是那一个
//! 文件内容旧，不会半写坏文件：先建目录再逐文件 `fs::write`，`fs::write` 本身是整文件替换）。
use notecore::model::Book;
use std::path::{Path, PathBuf};

/// 文件名不能带路径分隔符（书名/章名理论上可能带用户手滑打进去的 `/`）——替换成 `_`，不做更复杂的
/// 转义（其余字符 xochitl 书名场景本来就不会出现更奇怪的控制字符）。
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect()
}

pub fn vault_dir(data_dir: &Path, book_title: &str) -> PathBuf {
    data_dir.join("vault").join(sanitize(book_title))
}

/// 导出一本书的全部内容（各章 + 索引页）落盘。返回写了几个文件（0 = 整本书没有任何可导出的条目，
/// 不是错误）。
pub fn export_book(data_dir: &Path, book: &Book) -> Result<usize, String> {
    let dir = vault_dir(data_dir, &book.title);
    std::fs::create_dir_all(&dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let mut n = 0;
    for (idx, title) in book.chapters.iter().enumerate() {
        if export_chapter(&dir, book, idx, title)? {
            n += 1;
        }
    }
    match notecore::export::export_index_md(book) {
        Some(md) => {
            let path = dir.join(format!("{}.md", sanitize(&book.title)));
            std::fs::write(&path, md).map_err(|e| format!("写 {} 失败: {e}", path.display()))?;
            n += 1;
        }
        None => {
            // 整本书没有任何一章有内容：索引页也不落——跟"空章不落章文件"是同一条纪律。
        }
    }
    Ok(n)
}

/// 导出单章（+ 顺带刷新索引页，因为这一章"有没有内容"可能因此变化）。返回这一章是否写出了文件。
pub fn export_chapter(dir: &Path, book: &Book, chapter_idx: usize, title: &str) -> Result<bool, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    match notecore::export::export_chapter_md(book, chapter_idx) {
        Some(md) => {
            let path = dir.join(format!("{}.md", sanitize(&notecore::export::chapter_stem(chapter_idx, title))));
            std::fs::write(&path, md).map_err(|e| format!("写 {} 失败: {e}", path.display()))?;
            Ok(true)
        }
        None => Ok(false),
    }
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
            section: None,
            ask_ai: false,
            question: None,
            answer: None,
            status: Status::Reviewed,
            destination: Default::default(),
            created: 0,
            updated: 0,
        }
    }

    fn book() -> Book {
        Book { uuid: "u".into(), title: "人骨拼图".into(), author: String::new(), chapters: vec!["第一章".into(), "空章".into()], sections: vec![], entries: vec![entry("e1", 0, 0)], page_mtimes: Default::default() }
    }

    #[test]
    fn writes_chapter_and_index_files_skips_empty_chapter() {
        let tmp = tempfile::tempdir().unwrap();
        let n = export_book(tmp.path(), &book()).unwrap();
        assert_eq!(n, 2, "第一章 + 索引，空章不落文件");
        let dir = vault_dir(tmp.path(), "人骨拼图");
        assert!(dir.join("第1章 第一章.md").is_file());
        assert!(dir.join("人骨拼图.md").is_file());
        assert!(!dir.join("第2章 空章.md").exists());
    }

    #[test]
    fn empty_book_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = book();
        b.entries.clear();
        let n = export_book(tmp.path(), &b).unwrap();
        assert_eq!(n, 0);
        assert!(!vault_dir(tmp.path(), "人骨拼图").join("人骨拼图.md").exists());
    }

    #[test]
    fn rewrite_is_idempotent_and_overwrites_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        export_book(tmp.path(), &book()).unwrap();
        let mut b2 = book();
        b2.entries[0].text = Some("崭新文字".into());
        export_book(tmp.path(), &b2).unwrap();
        let content = std::fs::read_to_string(vault_dir(tmp.path(), "人骨拼图").join("第1章 第一章.md")).unwrap();
        assert!(content.contains("崭新文字 ^e1\n"));
        assert!(!content.contains("内容 ^e1\n"), "旧内容被整文件覆盖，不是追加: {content}");
    }

    #[test]
    fn sanitizes_slash_in_titles_to_avoid_path_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = book();
        b.title = "带/斜杠的书名".into();
        export_book(tmp.path(), &b).unwrap();
        assert!(vault_dir(tmp.path(), "带/斜杠的书名").is_dir());
        assert_eq!(vault_dir(tmp.path(), "带/斜杠的书名").file_name().unwrap(), "带_斜杠的书名");
    }
}
