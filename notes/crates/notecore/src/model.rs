//! 数据模型。所有字段可序列化（条目库落 JSON，网页/CLI 直接吃同一形状）。
use serde::{Deserialize, Serialize};

/// 笔记本里一行的段落样式（对应 xochitl 3.28 打字格式；Title/Subheading 由投影自动给，条目只在这四种里）。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    #[default]
    Body,
    Bullet,
    Numbered,
    Checkbox,
}

impl Style {
    /// 对应 `.rm` 段落样式码（2026-09-07 真机样本 `testdata/seven_styles` 坐实，见 `rmv6::v6::scene_item::text::ParagraphStyle`）。
    /// NumberedList(10) 格式子块跟其余样式一样只有 2 字节，没有隐藏内容（早前"多 7 字节未解码载荷"的
    /// 说法是分析失误，那 7 字节其实属于 Subheading 1，见 `rmv6::write` 模块文档与白皮书 §03i）——
    /// `rmv6::write` 已支持写 NUMBERED 且真机验证过编号正确自动生成。
    pub fn wire_code(self) -> u8 {
        match self {
            Style::Body => 0x01,     // PLAIN
            Style::Bullet => 0x04,   // BULLET
            Style::Numbered => 0x0a, // NUMBERED，真机验证过，见 rmv6::write
            Style::Checkbox => 0x06, // CHECKBOX（未勾选；勾上号 7 要点方框，打字给不出，写入器别用）
        }
    }
}

/// 转写/校对/问答完之后，这条内容最终要投影去哪（三期，2026-09-08）：默认 `Both`——两处都要，跟这个
/// 字段加之前"两个投影都只看 `chapter`+`status`、来者不拒"的行为完全一致（`project.rs` 该收的还收，
/// `export.rs` 新功能对已有内容立刻可用，不用先给每条条目手动选一遍才肯导出）。用户想收窄再改成
/// `Notebook`（只留设备）/`Obsidian`（只导出，不回落设备笔记本）。`project.rs` 只收
/// `wants_notebook()`；`export.rs` 只收 `wants_obsidian()`。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Destination {
    Notebook,
    Obsidian,
    #[default]
    Both,
}

impl Destination {
    pub fn wants_notebook(self) -> bool {
        matches!(self, Destination::Notebook | Destination::Both)
    }
    pub fn wants_obsidian(self) -> bool {
        matches!(self, Destination::Obsidian | Destination::Both)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// ink-serve 刚探测到（有画线/手写），还没被用户要求转笔记——**不会**被自动转写。
    /// 2026-09-07 二期定案：探测（挖到了）与转笔记（真想要）拆成两个独立动作，见浏览态设计；
    /// 之前版本摄取时给的初始态是 `Pending`，现在收窄到"用户在浏览列表点了『转入笔记』才算数"。
    #[default]
    Mined,
    /// 用户点了「转入笔记」，等转写。
    Pending,
    /// 有转写草稿、等人校对。
    Draft,
    /// 人已定稿（`text` 有值）。
    Reviewed,
    /// 用户在浏览列表点了「不需要」——跟 `Mined` 一样不会被自动转写，区别只是不再出现在待办列表里。
    Skipped,
    /// 笔画已从书页删除（不物理删，留痕）。
    Revoked,
    /// 用户在「整理」里点了「不要了」（三期，2026-09-08）：转写/问答都看完了，两处投影
    /// （设备笔记本 `project.rs` / Obsidian `export.rs`）都不要再出现——跟 `Revoked` 一样是终态、
    /// 不物理删（留痕，靠 `Book::purge_terminal` 手动清），区别只是触发方是用户主动"删除"而不是
    /// 笔画被擦掉。
    Archived,
}

/// 配对到的勾画（GlyphRange）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quote {
    /// 勾画自己的稳定 id（`.rm` 里 `GlyphRange` 的 CRDT id，不是笔画 id）：给"纯勾画条目"（没有旁边
    /// 手写，`Entry.ink` 是 `None`）做重扫认领用，笔画哈希那套配不上它。`#[serde(default)]` 兼容
    /// 二期这个字段加之前落盘的旧条目库（旧数据反序列化成空串，不影响已有的手写配对逻辑）。
    #[serde(default)]
    pub id: String,
    pub text: String,
    pub color: String,
    /// 每行一个矩形 (x, y, w, h)，页坐标。
    pub rects: Vec<(f32, f32, f32, f32)>,
}

/// 一片手写：笔画 id 集合、包围盒、指纹、裁图文件名。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ink {
    pub strokes: Vec<String>,
    pub bbox: (f32, f32, f32, f32),
    pub hash: String,
    #[serde(default)]
    pub crop: String,
}

/// 转写草稿（可多次，最新在前）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Draft {
    pub text: String,
    pub backend: String,
    pub at: u64,
    /// 这份草稿对应的簇指纹（指纹变了旧草稿失效）。
    pub hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Answer {
    pub text: String,
    pub backend: String,
    pub at: u64,
    /// 生成时用的分区简述（简述改了要重跑）。
    pub brief: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// 稳定 id：创建时按 (书, 页, 首笔 id) 生成，之后**永不重算**（簇变了靠笔画重叠认领同一条目）。
    pub id: String,
    pub page: String,
    pub page_index: usize,
    #[serde(default)]
    pub chapter: Option<usize>,
    #[serde(default)]
    pub chapter_title: String,
    #[serde(default)]
    pub subhead: Option<String>,
    #[serde(default)]
    pub quote: Option<Quote>,
    #[serde(default)]
    pub ink: Option<Ink>,
    #[serde(default)]
    pub drafts: Vec<Draft>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub style: Style,
    /// 用户勾了「问AI」——二期改按条目单发（`mind-serve`，2026-09-07 二期）。三期（2026-09-08）
    /// 砍掉了"分区"这个概念——AI 触发早就是这个字段的事了，分区兼职的"笔记本排版分组"角色也
    /// 一并砍掉，改成整章条目按页序平铺（各自的 `Style` 就是唯一的格式区分，见 `project.rs`/`export.rs`）。
    #[serde(default)]
    pub ask_ai: bool,
    /// 用户输的问题（`ask_ai` 为真时才有意义）；答案写回 `answer`，`Answer.brief` 存的就是这句问题的存档。
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub answer: Option<Answer>,
    #[serde(default)]
    pub status: Status,
    /// 落设备笔记本 / 落 Obsidian / 两处都要（三期）。`#[serde(default)]` 兼容三期之前落盘的旧条目库。
    #[serde(default)]
    pub destination: Destination,
    pub created: u64,
    pub updated: u64,
}

impl Entry {
    /// 投影用文本：校对文本优先，其次最新草稿。
    pub fn display_text(&self) -> Option<&str> {
        self.text.as_deref().or_else(|| self.drafts.first().map(|d| d.text.as_str()))
    }
    /// 指纹已变、还没有对应新指纹的草稿 → 需要（再）转写。**只认用户已经表态要转的条目**
    /// （`Mined`/`Skipped` 都不算——探测到不等于想转笔记，见浏览态设计，2026-09-07 二期）；
    /// 已经在 `Pending`/`Draft`/`Reviewed` 的条目如果笔画又变了（补了几笔），仍然继续认，
    /// 不会因为已经校对过就不再建议新草稿（校对文本本身不会被覆盖，见增量规则）。
    pub fn needs_transcribe(&self) -> bool {
        matches!((&self.ink, self.status), (Some(ink), s) if !matches!(s, Status::Revoked | Status::Mined | Status::Skipped | Status::Archived) && !self.drafts.iter().any(|d| d.hash == ink.hash))
    }
    /// 浏览态动作：转成 `Pending`（转入笔记）/`Skipped`（不需要）/`Archived`（三期"不要了"）。
    /// 已撤销/已归档的条目是终态，操作没有意义，拒绝。**纯勾画条目**（`ink` 是 `None`，内容全是
    /// `quote`）没有手写可转写——勾画文字是 `GlyphRange` 原生给的精确文字，不需要过一遍视觉模型；
    /// 这种条目"转入笔记"就直接落定（`text = quote.text`、状态跳到 `Reviewed`），不经过
    /// `Pending`/`Draft` 那两步，不然会卡在 `Pending` 里——`needs_transcribe()` 要求 `ink` 是
    /// `Some`，永远不会被自动转写捡走（2026-09-07 二期真机验证时发现的缺口，见笔记线白皮书 §03o）。
    pub fn set_triage(&mut self, target: Status, now: u64) -> Result<(), String> {
        if matches!(self.status, Status::Revoked | Status::Archived) {
            return Err("这条已撤销/已删除，不能再操作".into());
        }
        if target == Status::Pending && self.ink.is_none() {
            if let Some(q) = &self.quote {
                self.text = Some(q.text.clone());
                self.status = Status::Reviewed;
                self.updated = now;
                return Ok(());
            }
        }
        self.status = target;
        self.updated = now;
        Ok(())
    }
}

/// 一本书的条目库文件形状。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Book {
    pub uuid: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub chapters: Vec<String>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// 页 id → 上次摄取时页 `.rm` 的 mtime（秒），只扫变更页。
    #[serde(default)]
    pub page_mtimes: std::collections::BTreeMap<String, u64>,
}

impl Book {
    /// 清空回收站：物理移除 `Archived`/`Revoked`/`Skipped` 这三种"终态、不再活跃"的条目——软删会
    /// 无限攒（每条撤销/跳过/归档的条目永远留痕），这是唯一真正腾空间的操作。**手动触发，不自动跑**，
    /// 跟书级回收站"不自动清空回收站"是同一条纪律（见笔记线白皮书 §05 明确不做清单）；一旦清掉就是
    /// 真删除，不可恢复——调用方（ink-serve）该在网页上给一个需要用户主动点的按钮，不要在别的操作
    /// 里顺手带上。返回删了几条。
    pub fn purge_terminal(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| !matches!(e.status, Status::Archived | Status::Revoked | Status::Skipped));
        before - self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let e = Entry { id: "e1".into(), page: "p".into(), page_index: 3, chapter: Some(1), chapter_title: "一".into(), subhead: None, quote: None, ink: Some(Ink { strokes: vec!["1:2".into()], bbox: (0.0, 0.0, 1.0, 1.0), hash: "h".into(), crop: String::new() }), drafts: vec![], text: None, style: Style::Checkbox, ask_ai: false, question: None, answer: None, status: Status::Pending, destination: Default::default(), created: 1, updated: 1 };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains(r#""style":"checkbox""#) && j.contains(r#""status":"pending""#));
        let back: Entry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
        assert!(back.needs_transcribe());
        let b: Book = serde_json::from_str(r#"{"uuid":"u","title":"t"}"#).unwrap();
        assert!(b.entries.is_empty());
    }

    #[test]
    fn style_wire_codes_match_device_sample() {
        assert_eq!(Style::Body.wire_code(), 0x01);
        assert_eq!(Style::Bullet.wire_code(), 0x04);
        assert_eq!(Style::Numbered.wire_code(), 0x0a);
        assert_eq!(Style::Checkbox.wire_code(), 0x06);
    }

    #[test]
    fn display_text_prefers_reviewed() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0}"#).unwrap();
        assert_eq!(e.display_text(), None);
        e.drafts.push(Draft { text: "你好".into(), backend: "qwen".into(), at: 1, hash: "h".into() });
        assert_eq!(e.display_text(), Some("你好"));
        e.text = Some("您好".into());
        e.status = Status::Reviewed; // 已经有草稿+校对文本，说明这条早就被请求过、不再是 Mined 了
        assert_eq!(e.display_text(), Some("您好"), "校对文本压过草稿");
        e.ink = Some(Ink { strokes: vec![], bbox: (0.0, 0.0, 0.0, 0.0), hash: "h".into(), crop: String::new() });
        assert!(!e.needs_transcribe(), "草稿指纹与簇指纹一致");
        e.ink.as_mut().unwrap().hash = "h2".into();
        assert!(e.needs_transcribe(), "簇变了要再转写（作为建议，不动 text）：已经在 Reviewed 态，笔画再变仍然要再认");
        e.status = Status::Mined;
        assert!(!e.needs_transcribe(), "但如果这条从没被请求过（Mined），笔画再怎么变也不自动转写");
    }

    #[test]
    fn set_triage_moves_mined_to_pending_or_skipped_but_refuses_revoked() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0}"#).unwrap();
        assert_eq!(e.status, Status::Mined, "缺省态就是 Mined");

        e.set_triage(Status::Pending, 10).unwrap();
        assert_eq!((e.status, e.updated), (Status::Pending, 10), "转入笔记");

        e.status = Status::Mined;
        e.set_triage(Status::Skipped, 20).unwrap();
        assert_eq!((e.status, e.updated), (Status::Skipped, 20), "不需要");

        e.status = Status::Revoked;
        let err = e.set_triage(Status::Pending, 30).unwrap_err();
        assert!(err.contains("已撤销"));
        assert_eq!(e.status, Status::Revoked, "拒绝后状态不变");
    }

    /// 纯勾画条目（`ink: None`）没有手写可转写：转入笔记直接落定成 `Reviewed`，不经过
    /// `Pending`/`Draft`（不然会卡住——`needs_transcribe()` 要求 `ink` 是 `Some`，永远不会被
    /// 自动转写捡走），文本直接取勾画原文。「不需要」还是走普通的 `Skipped`，不特殊。
    #[test]
    fn set_triage_finalizes_quote_only_entries_straight_to_reviewed() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0,"quote":{"id":"q1","text":"勾画原文","color":"yellow","rects":[]}}"#).unwrap();
        assert!(e.ink.is_none() && e.quote.is_some());

        e.set_triage(Status::Pending, 10).unwrap();
        assert_eq!((e.status, e.text.as_deref(), e.updated), (Status::Reviewed, Some("勾画原文"), 10), "转入笔记直接定稿，跳过 Pending/Draft");

        // 「不需要」不受影响，还是 Skipped。
        e.status = Status::Mined;
        e.text = None;
        e.set_triage(Status::Skipped, 20).unwrap();
        assert_eq!(e.status, Status::Skipped);
    }

    #[test]
    fn destination_default_is_both_and_truth_table_is_correct() {
        assert_eq!(Destination::default(), Destination::Both, "默认两处都要——新功能对已有内容立刻可用");
        assert!(Destination::Notebook.wants_notebook() && !Destination::Notebook.wants_obsidian());
        assert!(!Destination::Obsidian.wants_notebook() && Destination::Obsidian.wants_obsidian());
        assert!(Destination::Both.wants_notebook() && Destination::Both.wants_obsidian());
    }

    #[test]
    fn set_triage_can_archive_and_refuses_further_operations_on_archived() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0,"status":"reviewed"}"#).unwrap();
        e.set_triage(Status::Archived, 5).unwrap();
        assert_eq!((e.status, e.updated), (Status::Archived, 5));
        let err = e.set_triage(Status::Pending, 10).unwrap_err();
        assert!(err.contains("已删除"), "{err}");
        assert_eq!(e.status, Status::Archived, "拒绝后状态不变");
    }

    #[test]
    fn archived_entries_are_never_auto_transcribed() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0,"ink":{"strokes":["1:1"],"bbox":[0,0,1,1],"hash":"h"}}"#).unwrap();
        e.status = Status::Archived;
        assert!(!e.needs_transcribe(), "已归档的条目跟已撤销/已跳过一样不该被自动转写捡走");
    }

    #[test]
    fn purge_terminal_removes_only_archived_revoked_skipped() {
        fn entry(id: &str, status: Status) -> Entry {
            let mut e: Entry = serde_json::from_str(&format!(r#"{{"id":"{id}","page":"p","page_index":0,"created":0,"updated":0}}"#)).unwrap();
            e.status = status;
            e
        }
        let mut b = Book {
            uuid: "u".into(),
            title: "t".into(),
            entries: vec![entry("keep-mined", Status::Mined), entry("keep-pending", Status::Pending), entry("keep-reviewed", Status::Reviewed), entry("drop-skipped", Status::Skipped), entry("drop-revoked", Status::Revoked), entry("drop-archived", Status::Archived)],
            ..Default::default()
        };
        let removed = b.purge_terminal();
        assert_eq!(removed, 3);
        let remaining: Vec<&str> = b.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(remaining, ["keep-mined", "keep-pending", "keep-reviewed"]);
        assert_eq!(b.purge_terminal(), 0, "再清一次没东西可清");
    }
}
