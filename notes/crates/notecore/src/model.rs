//! 数据模型。所有字段可序列化（条目库落 JSON，网页/CLI 直接吃同一形状）。
use serde::{Deserialize, Serialize};

/// 分区 = 名字 + 简述（简述就是给 AI 的要求）+ 是否调模型 + 顺序。全局缺省，按书可覆盖。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default = "yes")]
    pub ai: bool,
    #[serde(default)]
    pub order: u32,
    /// 行首关键字/符号 → 默认归到本分区（如 `?` `查` `!` `背诵`）。
    #[serde(default)]
    pub triggers: Vec<String>,
}

fn yes() -> bool {
    true
}

/// 缺省分区表（用户可改）：查询 / 解释 / 背诵 / 其他。
pub fn default_sections() -> Vec<Section> {
    vec![
        Section { id: "lookup".into(), name: "查询".into(), brief: "查这段勾画里的人名、作者、术语，给出简介与出处".into(), ai: true, order: 0, triggers: vec!["查".into()] },
        Section { id: "explain".into(), name: "解释".into(), brief: "用通俗的话讲解这段勾画，先说结论再说为什么".into(), ai: true, order: 1, triggers: vec!["?".into(), "？".into(), "没懂".into()] },
        Section { id: "memorize".into(), name: "背诵".into(), brief: String::new(), ai: false, order: 2, triggers: vec!["!".into(), "！".into(), "背".into()] },
        Section { id: "other".into(), name: "其他".into(), brief: String::new(), ai: false, order: 3, triggers: vec![] },
    ]
}

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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// 有手写、还没转写。
    #[default]
    Pending,
    /// 有转写草稿、等人校对。
    Draft,
    /// 人已定稿（`text` 有值）。
    Reviewed,
    /// 笔画已从书页删除（不物理删，留痕）。
    Revoked,
}

/// 配对到的勾画（GlyphRange）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quote {
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
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub answer: Option<Answer>,
    #[serde(default)]
    pub status: Status,
    pub created: u64,
    pub updated: u64,
}

impl Entry {
    /// 投影用文本：校对文本优先，其次最新草稿。
    pub fn display_text(&self) -> Option<&str> {
        self.text.as_deref().or_else(|| self.drafts.first().map(|d| d.text.as_str()))
    }
    /// 指纹已变、还没有对应新指纹的草稿 → 需要（再）转写。
    pub fn needs_transcribe(&self) -> bool {
        matches!((&self.ink, self.status), (Some(ink), s) if s != Status::Revoked && !self.drafts.iter().any(|d| d.hash == ink.hash))
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
    pub sections: Vec<Section>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// 页 id → 上次摄取时页 `.rm` 的 mtime（秒），只扫变更页。
    #[serde(default)]
    pub page_mtimes: std::collections::BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let e = Entry { id: "e1".into(), page: "p".into(), page_index: 3, chapter: Some(1), chapter_title: "一".into(), subhead: None, quote: None, ink: Some(Ink { strokes: vec!["1:2".into()], bbox: (0.0, 0.0, 1.0, 1.0), hash: "h".into(), crop: String::new() }), drafts: vec![], text: None, style: Style::Checkbox, section: None, answer: None, status: Status::Pending, created: 1, updated: 1 };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains(r#""style":"checkbox""#) && j.contains(r#""status":"pending""#));
        let back: Entry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
        assert!(back.needs_transcribe());
        let b: Book = serde_json::from_str(r#"{"uuid":"u","title":"t"}"#).unwrap();
        assert!(b.entries.is_empty() && b.sections.is_empty());
        assert_eq!(default_sections().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["查询", "解释", "背诵", "其他"]);
    }

    #[test]
    fn display_text_prefers_reviewed() {
        let mut e: Entry = serde_json::from_str(r#"{"id":"e","page":"p","page_index":0,"created":0,"updated":0}"#).unwrap();
        assert_eq!(e.display_text(), None);
        e.drafts.push(Draft { text: "你好".into(), backend: "qwen".into(), at: 1, hash: "h".into() });
        assert_eq!(e.display_text(), Some("你好"));
        e.text = Some("您好".into());
        assert_eq!(e.display_text(), Some("您好"), "校对文本压过草稿");
        e.ink = Some(Ink { strokes: vec![], bbox: (0.0, 0.0, 0.0, 0.0), hash: "h".into(), crop: String::new() });
        assert!(!e.needs_transcribe(), "草稿指纹与簇指纹一致");
        e.ink.as_mut().unwrap().hash = "h2".into();
        assert!(e.needs_transcribe(), "簇变了要再转写（作为建议，不动 text）");
    }
}
