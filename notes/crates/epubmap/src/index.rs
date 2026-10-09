//! `<uuid>.epubindex` 解析：xochitl 导入 EPUB 时生成的二进制索引，记录每个 spine 文件的起始页。
//! 2026-10-09 下沉到 `epubpkg::epubindex`（书架 book-serve 原地替换后找回阅读位置也要用），这里只转出，`epubmap::index::*` 路径不变。
pub use epubpkg::epubindex::{parse_epubindex, Section};
