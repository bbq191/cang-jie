//! 投递目标（Strategy）：`native`（原生阅读）/ `annot`（原生批注=PDF 定稿）。
//! KOReader 目标由 koreader-serve 承担（网关/UI/CLI 按 target 直接打它），本服务对它一无所知。
use crate::config::BookConfig;
use crate::pipeline::{Check, Convert, Doc, Inject, Optimize, Pipeline, Precheck};
use bookconv::convert::{self, EinkTone};
use bookconv::wash::WashOpts;
use serde::Serialize;
use shelf_core::xochitl::Xochitl;
use std::collections::HashMap;
use std::sync::Arc;

/// `optimize=` 档位：`auto`（清洗+优化，缺省）/ `keep-spacing`（清洗但保留段距）/ `plain`（只优化不清洗）/ `off`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptimizeMode {
    #[default]
    Auto,
    KeepSpacing,
    Plain,
    Off,
}

impl OptimizeMode {
    pub fn parse(s: &str) -> OptimizeMode {
        match s {
            "off" => OptimizeMode::Off,
            "plain" => OptimizeMode::Plain,
            "keep-spacing" | "keep_spacing" => OptimizeMode::KeepSpacing,
            _ => OptimizeMode::Auto,
        }
    }
    fn wash(self) -> Option<WashOpts> {
        match self {
            OptimizeMode::Auto => Some(WashOpts::default()),
            OptimizeMode::KeepSpacing => Some(WashOpts { keep_para_spacing: true, ..Default::default() }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeliverOpts {
    /// 覆盖配置里的文件夹（空=用配置）。
    pub folder: Option<String>,
    pub optimize: OptimizeMode,
    /// `check=off` 时 false（强行投递）。
    pub check: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Outcome {
    pub file: String,
    pub target: String,
    pub ok: bool,
    pub message: String,
}

pub trait DeliveryTarget: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// 按文件名判能否接收；Err 带面向用户的原因。
    fn accepts(&self, filename: &str) -> Result<(), String>;
    fn pipeline(&self, opts: &DeliverOpts) -> Pipeline;

    fn deliver(&self, filename: &str, data: Vec<u8>, opts: &DeliverOpts) -> Outcome {
        let target = self.id().to_string();
        if let Err(e) = self.accepts(filename) {
            return Outcome { file: filename.into(), target, ok: false, message: e };
        }
        match self.pipeline(opts).run(Doc::new(filename, data)) {
            Ok(doc) => Outcome { file: filename.into(), target, ok: true, message: doc.notes.join("；") },
            Err(e) => Outcome { file: filename.into(), target, ok: false, message: e },
        }
    }
}

pub struct Native {
    cfg: BookConfig,
    xochitl: Arc<Xochitl>,
}
pub struct Annot {
    cfg: BookConfig,
    xochitl: Arc<Xochitl>,
}

impl DeliveryTarget for Native {
    fn id(&self) -> &'static str {
        "native"
    }
    fn label(&self) -> &'static str {
        "原生阅读"
    }
    fn accepts(&self, filename: &str) -> Result<(), String> {
        // 只收 EPUB / PDF（2026-09-04 定案：格式仅保留两种；其它格式转换质量交给电脑端 Calibre）。
        if convert::direct_content_type(filename).is_some() {
            Ok(())
        } else {
            Err("原生阅读只收 EPUB / PDF。AZW3 / MOBI / FB2 / CBZ 等请在电脑用 `shelf push`（Calibre 转换质量更高）后再投".into())
        }
    }
    fn pipeline(&self, opts: &DeliverOpts) -> Pipeline {
        let tone = if self.cfg.comic_mono { EinkTone::Mono } else { EinkTone::Off };
        Pipeline::new()
            .then(Precheck)
            .then(Convert { tone })
            .then(Optimize { enabled: opts.optimize != OptimizeMode::Off && self.cfg.optimize_direct_epub, wash: opts.optimize.wash(), footnote: bookconv::optimize::FootnoteMode::Inline })
            .then(Check { enabled: opts.check, require_toc: false })
            .then(Inject { xochitl: self.xochitl.clone(), folder: opts.folder.clone().unwrap_or_else(|| self.cfg.library_folder.clone()) })
    }
}

impl DeliveryTarget for Annot {
    fn id(&self) -> &'static str {
        "annot"
    }
    fn label(&self) -> &'static str {
        "原生批注（PDF 定稿）"
    }
    fn accepts(&self, filename: &str) -> Result<(), String> {
        let l = filename.to_ascii_lowercase();
        if l.ends_with(".pdf") {
            Ok(())
        } else if l.ends_with(".epub") {
            Err("批注目标只收 PDF：EPUB 请在 host 用 `shelf push --target annot` 定稿（Calibre 954×1696 固定版式）后再投".into())
        } else {
            Err("批注目标只收 PDF（CBZ / 其它格式请在电脑用 `shelf push` 转换后再投）".into())
        }
    }
    fn pipeline(&self, opts: &DeliverOpts) -> Pipeline {
        let tone = if self.cfg.comic_mono { EinkTone::Mono } else { EinkTone::Off };
        Pipeline::new()
            .then(Precheck)
            .then(Convert { tone })
            .then(Inject { xochitl: self.xochitl.clone(), folder: opts.folder.clone().unwrap_or_else(|| self.cfg.annot_folder.clone()) })
    }
}

/// 目标注册表：`target=` 查找，无 if-else 链。
pub struct TargetRegistry {
    targets: HashMap<&'static str, Arc<dyn DeliveryTarget>>,
    default: &'static str,
}

impl TargetRegistry {
    pub fn new(cfg: &BookConfig, xochitl: Arc<Xochitl>) -> TargetRegistry {
        let mut targets: HashMap<&'static str, Arc<dyn DeliveryTarget>> = HashMap::new();
        targets.insert("native", Arc::new(Native { cfg: cfg.clone(), xochitl: xochitl.clone() }));
        targets.insert("annot", Arc::new(Annot { cfg: cfg.clone(), xochitl }));
        TargetRegistry { targets, default: "native" }
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn DeliveryTarget>> {
        let id = if id.is_empty() { self.default } else { id };
        self.targets.get(id).cloned()
    }
    pub fn ids(&self) -> Vec<(&'static str, &'static str)> {
        let mut v: Vec<_> = self.targets.values().map(|t| (t.id(), t.label())).collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> TargetRegistry {
        let x = Arc::new(Xochitl::new("127.0.0.1:1", std::path::Path::new("/nonexistent"), 1));
        TargetRegistry::new(&BookConfig::default(), x)
    }

    #[test]
    fn registry_lookup_and_default() {
        let r = reg();
        assert_eq!(r.get("").unwrap().id(), "native");
        assert_eq!(r.get("annot").unwrap().id(), "annot");
        assert!(r.get("koreader").is_none());
        assert_eq!(r.ids(), vec![("annot", "原生批注（PDF 定稿）"), ("native", "原生阅读")]);
    }

    #[test]
    fn accept_rules() {
        let r = reg();
        let n = r.get("native").unwrap();
        // 只收 EPUB/PDF；AZW3/CBZ 等一律拒（引导走 host Calibre）
        assert!(n.accepts("b.EPUB").is_ok() && n.accepts("d.pdf").is_ok());
        assert!(n.accepts("a.azw3").is_err() && n.accepts("z.cbz").is_err() && n.accepts("c.txt").is_err());
        let a = r.get("annot").unwrap();
        assert!(a.accepts("x.pdf").is_ok());
        assert!(a.accepts("x.cbz").is_err());
        assert!(a.accepts("x.epub").unwrap_err().contains("shelf push --target annot"));
        assert!(a.accepts("x.azw3").is_err());
    }

    #[test]
    fn pipelines_are_composed_per_target() {
        let r = reg();
        let o = DeliverOpts { folder: None, optimize: OptimizeMode::Auto, check: true };
        assert_eq!(r.get("native").unwrap().pipeline(&o).names(), vec!["precheck", "convert", "optimize", "check", "inject"]);
        assert_eq!(OptimizeMode::parse("keep-spacing").wash().unwrap().keep_para_spacing, true);
        assert!(OptimizeMode::parse("plain").wash().is_none() && OptimizeMode::parse("").wash().is_some());
        assert_eq!(r.get("annot").unwrap().pipeline(&o).names(), vec!["precheck", "convert", "inject"]);
    }

    #[test]
    fn deliver_reports_reject_without_running_pipeline() {
        let r = reg();
        let o = r.get("annot").unwrap().deliver("x.epub", vec![], &DeliverOpts::default());
        assert!(!o.ok && o.target == "annot");
        // 直读 PDF 走到 inject 才失败（xochitl 不可达）→ 错误带步骤名
        let o = r.get("native").unwrap().deliver("x.pdf", b"%PDF-1.4".to_vec(), &DeliverOpts::default());
        assert!(!o.ok && o.message.starts_with("inject:"), "{}", o.message);
    }
}
