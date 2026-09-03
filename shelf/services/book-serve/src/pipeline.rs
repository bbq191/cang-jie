//! 处理链（Pipeline）：`Precheck → Convert → Optimize → Check → Inject` 每步一个 [`Step`]，目标（Strategy）
//! 按需组装。步骤只调 bookconv / shelf-core 函数，不重写规则。
use bookconv::convert::{self, ContentType, EinkTone};
use bookconv::optimize::{self, OptimizeOpts};
use bookconv::wash::WashOpts;
use shelf_core::xochitl::{Delivery, Xochitl};

/// 流经处理链的文档。
pub struct Doc {
    /// 原始文件名（回执/日志用）。
    pub source_name: String,
    pub data: Vec<u8>,
    /// 到 Inject 前必须已定（直读格式一开始就定；转换后由转换器定）。
    pub content_type: Option<ContentType>,
    pub title: String,
    /// 过程说明（回执拼接）。
    pub notes: Vec<String>,
}

impl Doc {
    pub fn new(name: &str, data: Vec<u8>) -> Doc {
        let title = std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("book").to_string();
        Doc { source_name: name.to_string(), data, content_type: convert::direct_content_type(name), title, notes: vec![] }
    }
    pub fn out_filename(&self) -> String {
        let ext = self.content_type.map(|c| c.ext()).unwrap_or("bin");
        format!("{}.{}", bookconv::util::sanitize_filename(&self.title, "book"), ext)
    }
}

pub trait Step: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, doc: Doc) -> Result<Doc, String>;
}

pub struct Pipeline {
    steps: Vec<Box<dyn Step>>,
}

impl Pipeline {
    pub fn new() -> Pipeline {
        Pipeline { steps: vec![] }
    }
    pub fn then(mut self, s: impl Step + 'static) -> Pipeline {
        self.steps.push(Box::new(s));
        self
    }
    pub fn run(&self, mut doc: Doc) -> Result<Doc, String> {
        for s in &self.steps {
            doc = s.run(doc).map_err(|e| format!("{}: {e}", s.name()))?;
        }
        Ok(doc)
    }
    #[cfg(test)]
    pub fn names(&self) -> Vec<&'static str> {
        self.steps.iter().map(|s| s.name()).collect()
    }
}

/// 先验后转：DRM/HUFF-CDIC/损坏 秒回执，不白跑重活。
pub struct Precheck;
impl Step for Precheck {
    fn name(&self) -> &'static str {
        "precheck"
    }
    fn run(&self, doc: Doc) -> Result<Doc, String> {
        convert::precheck(&doc.source_name, &doc.data)?;
        Ok(doc)
    }
}

/// 可转换源（azw3/mobi/prc/azw/fb2/cbz）→ EPUB/PDF；直读格式原样通过。
pub struct Convert {
    pub tone: EinkTone,
}
impl Step for Convert {
    fn name(&self) -> &'static str {
        "convert"
    }
    fn run(&self, mut doc: Doc) -> Result<Doc, String> {
        if doc.content_type.is_some() {
            return Ok(doc);
        }
        match convert::convert_file(&doc.source_name, &doc.data, self.tone) {
            Some(Ok(c)) => {
                doc.notes.push(format!("已转换为 {}", c.content_type.ext()));
                doc.title = c.title;
                doc.content_type = Some(c.content_type);
                doc.data = c.data;
                Ok(doc)
            }
            Some(Err(e)) => Err(e),
            None => Err("非可转换源格式".into()),
        }
    }
}

/// EPUB 且无优化标记 → 跑清洗层 + 通用优化器（转换器产物已含优化，标记在则跳过）。
/// `wash=None` 只优化不清洗（= host `--no-wash`）。
pub struct Optimize {
    pub enabled: bool,
    pub wash: Option<WashOpts>,
}
impl Step for Optimize {
    fn name(&self) -> &'static str {
        "optimize"
    }
    fn run(&self, mut doc: Doc) -> Result<Doc, String> {
        if !self.enabled || doc.content_type != Some(ContentType::Epub) || optimize::is_optimized(&doc.data) {
            return Ok(doc);
        }
        let (out, rep) = optimize::optimize_epub_with(&doc.data, &OptimizeOpts { wash: self.wash.clone() })?;
        let mut note = format!("已{}（{} 章，{}→{} 字节", if rep.wash.is_some() { "清洗+优化" } else { "优化" }, rep.html_files, rep.bytes_before, rep.bytes_after);
        if let Some(w) = &rep.wash {
            if !w.pseudo_drm_stripped.is_empty() {
                note.push_str(&format!("，剥伪 DRM {} 项", w.pseudo_drm_stripped.len()));
            }
            if !w.empty_pages_removed.is_empty() {
                note.push_str(&format!("，删空页 {}", w.empty_pages_removed.len()));
            }
            if w.toc_generated > 0 {
                note.push_str(&format!("，自动目录 {} 条", w.toc_generated));
            }
        }
        note.push('）');
        doc.notes.push(note);
        doc.data = out;
        Ok(doc)
    }
}

/// EPUB 质量门（= host `check_output.py` EPUB 项）：真 DRM / 目录命中率 / 双 id 硬拦，告警进回执。
pub struct Check {
    pub enabled: bool,
    pub require_toc: bool,
}
impl Step for Check {
    fn name(&self) -> &'static str {
        "check"
    }
    fn run(&self, mut doc: Doc) -> Result<Doc, String> {
        if !self.enabled || doc.content_type != Some(ContentType::Epub) {
            return Ok(doc);
        }
        let rep = bookconv::check::check_epub(&doc.data, self.require_toc)?;
        if !rep.ok {
            return Err(format!("质量门未过：{}（可加 check=off 强行投递）", rep.errors.join("；")));
        }
        doc.notes.push(format!("质量门通过（{}）", rep.summary()));
        Ok(doc)
    }
}

/// 注入 xochitl 书库（GET-then-upload 归档进文件夹；408/读超时视为已送达）。
pub struct Inject {
    pub xochitl: std::sync::Arc<Xochitl>,
    pub folder: String,
}
impl Step for Inject {
    fn name(&self) -> &'static str {
        "inject"
    }
    fn run(&self, mut doc: Doc) -> Result<Doc, String> {
        let Some(ct) = doc.content_type else { return Err("内容类型未定".into()) };
        let fname = doc.out_filename();
        match self.xochitl.upload(&doc.data, &fname, ct.mime(), &self.folder)? {
            Delivery::Delivered(_) => doc.notes.push(format!("已导入《{}》", doc.title)),
            Delivery::LikelyDelivered(_) => doc.notes.push(format!("已导入《{}》（设备处理较慢，稍候刷新书库）", doc.title)),
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tag(&'static str);
    impl Step for Tag {
        fn name(&self) -> &'static str {
            self.0
        }
        fn run(&self, mut d: Doc) -> Result<Doc, String> {
            d.notes.push(self.0.into());
            if self.0 == "boom" {
                return Err("x".into());
            }
            Ok(d)
        }
    }

    #[test]
    fn runs_in_order_and_prefixes_errors() {
        let p = Pipeline::new().then(Tag("a")).then(Tag("b"));
        let d = p.run(Doc::new("x.epub", vec![])).unwrap();
        assert_eq!(d.notes, vec!["a", "b"]);
        assert_eq!(d.content_type, Some(ContentType::Epub));
        assert_eq!(d.out_filename(), "x.epub");
        let e = Pipeline::new().then(Tag("boom")).run(Doc::new("y.pdf", vec![])).err().unwrap();
        assert_eq!(e, "boom: x");
    }

    #[test]
    fn convert_step_rejects_unknown_and_passes_direct() {
        let s = Convert { tone: EinkTone::Off };
        assert!(s.run(Doc::new("a.pdf", vec![1])).is_ok());
        assert!(s.run(Doc::new("a.xyz", vec![1])).is_err());
    }

    #[test]
    fn optimize_step_skips_when_disabled_or_non_epub() {
        let s = Optimize { enabled: false, wash: None };
        assert!(s.run(Doc::new("a.epub", b"notazip".to_vec())).is_ok());
        let s = Optimize { enabled: true, wash: Some(WashOpts::default()) };
        assert!(s.run(Doc::new("a.pdf", b"%PDF".to_vec())).is_ok());
        assert!(s.run(Doc::new("a.epub", b"notazip".to_vec())).is_err());
    }

    fn mini_epub(files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file("mimetype", o).unwrap();
            zw.write_all(b"application/epub+zip").unwrap();
            for (n, d) in files {
                zw.start_file(*n, o).unwrap();
                zw.write_all(d.as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn wash_then_check_end_to_end() {
        let opf = r#"<package version="2.0"><metadata><dc:title>t</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let epub = mini_epub(&[("content.opf", opf), ("c1.xhtml", r#"<html><head></head><body><h1>第一章</h1><p id="x" id="y" style="font-size:9px;margin:1em">正文</p></body></html>"#)]);
        // 双 id + 无目录：不清洗直接过门 → 双 id 硬拦
        let raw = Check { enabled: true, require_toc: false }.run(Doc::new("a.epub", epub.clone()));
        assert!(raw.err().unwrap().contains("双 id"));
        // 清洗+优化 → 双 id 折叠、自动目录 → 过门
        let d = Optimize { enabled: true, wash: Some(WashOpts::default()) }.run(Doc::new("a.epub", epub)).unwrap();
        assert!(d.notes[0].contains("清洗+优化") && d.notes[0].contains("自动目录 1 条"), "{:?}", d.notes);
        let d = Check { enabled: true, require_toc: true }.run(d).unwrap();
        assert!(d.notes[1].starts_with("质量门通过（目录 2 条"), "{:?}", d.notes);
        assert!(optimize::is_optimized(&d.data));
    }
}
