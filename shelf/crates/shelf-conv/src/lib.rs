//! shelf-conv —— 书架（book-serve）把书投进 xochitl 时用到的读书工具。
//!
//! **书架不优化书**（2026-10-07 用户定）：清洗、排版、注释、漫画图片处理全部在电脑上用 sheng-ren（booklib，`xochitl`
//! 阅读模式）做，优化好的书上传进母版库后原样投进 xochitl。本 crate 只读不改书：
//!
//! - `pdfmeta`：第三方 PDF 页数（大文件通道要写进 `.content`）；`placeholder`：大文件"占位 + 替换"投原生用的占位文档，
//!   以及读 EPUB 的翻页方向；
//! - `naming`：书名规范化（入库文件名、大文件通道占位的显示名）；
//! - `epub`：读 EPUB 的几样小事（OPF、书名、封面、翻页方向、sheng-ren 的漫画页边距标记）和写占位 EPUB 的 zip。
//!
//! 2026-10-07 起不再依赖 sheng-ren 的 `bookconv`：书架只读书的这几样，借它的公开接口要连带编进图片处理、网页抽取等
//! 一整串用不上的依赖，还要跟着它的 master 走；渲染自检也只剩"认出刚投的书"，不再按正文字数估期望页数（原 `stats`）。
pub mod epub;
pub mod naming;
pub mod pdfmeta;
pub mod placeholder;

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
