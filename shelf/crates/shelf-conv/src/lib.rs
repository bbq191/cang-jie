//! shelf-conv —— 书架（book-serve）把书投进 xochitl 时用到的读书工具。
//!
//! **书架不优化书**（2026-10-07 用户定）：清洗、排版、注释、漫画图片处理全部在电脑上用 sheng-ren（booklib，`xochitl`
//! 阅读模式）做，优化好的书上传进母版库后原样投进 xochitl。本 crate 只读不改书：
//!
//! - `pdfmeta`：第三方 PDF 页数（大文件通道要写进 `.content`）；`placeholder`：大文件"占位 + 替换"投原生用的占位文档，
//!   以及读 EPUB 的翻页方向；
//! - `stats`：投原生后的渲染页数自检统计；`naming`：文件名版本的书名规范化。
//!
//! 读 EPUB 的公共件（zip、OPF、封面、正文文字）用 sheng-ren `bookconv` 的公开接口。
pub mod naming;
pub mod pdfmeta;
pub mod placeholder;
pub mod stats;

/// xochitl 能直接读的格式（母版库落库与下载用）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ContentType {
    Epub,
    Pdf,
}

impl ContentType {
    pub fn mime(self) -> &'static str {
        match self {
            ContentType::Epub => "application/epub+zip",
            ContentType::Pdf => "application/pdf",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            ContentType::Epub => "epub",
            ContentType::Pdf => "pdf",
        }
    }
}

/// 按扩展名判 xochitl 能直接读的格式（EPUB/PDF），其余 `None`。
pub fn direct_content_type(filename: &str) -> Option<ContentType> {
    let l = filename.to_ascii_lowercase();
    if l.ends_with(".epub") {
        Some(ContentType::Epub)
    } else if l.ends_with(".pdf") {
        Some(ContentType::Pdf)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_types_by_extension() {
        assert_eq!(direct_content_type("A.EPUB"), Some(ContentType::Epub));
        assert_eq!(direct_content_type("b.pdf").map(|c| c.mime()), Some("application/pdf"));
        assert_eq!(direct_content_type("c.cbz"), None);
    }
}
