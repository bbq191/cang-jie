//! shelf-conv —— 书架（book-serve）独有的内容层。
//!
//! **通用的 EPUB 优化规则不在这里**：清洗、排版、注释、漫画图片处理、质量门、EPUB 读写都在 sheng-ren 的 `bookconv`
//! （git 依赖，跟 master 走，`cargo update -p bookconv` 跟进），规则文档见 sheng-ren 仓库 `docs/`。本 crate 只放 sheng-ren
//! 没有、而母版库要用的东西：
//!
//! - `pdf_ingest`：入库 PDF 的分类、裁边（漫画/扫描件）与转 EPUB（有文字层）；`pdf_epub`：PDF 转出的 EPUB 的组装；
//! - `pdfwrite`/`pdfmeta`/`pdfimg`：PDF 写出、第三方 PDF 页数、PDF 页图片处理；
//! - `placeholder`：大文件"占位 + 替换"投原生用的占位文档；`stats`：投原生后的渲染页数自检统计；
//! - `naming`：文件名版本的书名规范化；`legacy`：旧版（cang-jie 自带 bookconv v16 及以前）优化产物交给 sheng-ren 优化器前的兼容预处理。
pub mod legacy;
pub mod naming;
pub mod pdf_epub;
pub mod pdf_ingest;
pub mod pdfimg;
pub mod pdfmeta;
pub mod pdfwrite;
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

/// HTML 片段里最后一个可见字符（跳过末尾的标签与空白），用于判断一段话是否以句末标点收尾。
pub fn strip_tags_tail_char(html: &str) -> Option<char> {
    let mut in_tag = false;
    for c in html.chars().rev() {
        match c {
            '>' => in_tag = true,
            '<' => in_tag = false,
            _ if in_tag || c.is_whitespace() => {}
            _ => return Some(c),
        }
    }
    None
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

    #[test]
    fn tail_char_skips_tags_and_space() {
        assert_eq!(strip_tags_tail_char("<p>句子。</p> \n"), Some('。'));
        assert_eq!(strip_tags_tail_char("<br/>"), None);
    }
}
