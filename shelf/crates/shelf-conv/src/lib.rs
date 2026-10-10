//! shelf-conv —— 书架（book-serve）把书投进 xochitl 时用到的读书工具。
//!
//! **书架不优化书**（2026-10-07 用户定）：清洗、排版、注释、漫画图片处理全部在电脑上用 sheng-ren（booklib，`xochitl`
//! 阅读模式）做，优化好的书上传进母版库后原样投进 xochitl。本 crate 只读不改书：
//!
//! - `pdfmeta`：第三方 PDF 页数（大文件通道要写进 `.content`）；`placeholder`：大文件"占位 + 替换"投原生用的占位文档；
//! - `naming`：入库文件名的书名规范化；
//! - `epub`：读 EPUB 的几样小事（OPF、书名、封面、sheng-ren 的漫画页边距标记）和写占位 EPUB 的 zip。
//!
//! 2026-10-07 起不再依赖 sheng-ren 的 `bookconv`：书架只读书的这几样（翻页方向 2026-10-07 也删了：书架只管入库），借它的公开接口要连带编进图片处理、网页抽取等
//! 一整串用不上的依赖，还要跟着它的 master 走；渲染自检也只剩"认出刚投的书"，不再按正文字数估期望页数（原 `stats`）。
pub mod epub;
pub mod naming;
pub mod pdfmeta;
pub mod placeholder;

// 2026-10-10 删 `ContentType` / `direct_content_type`（审计 SH-2）：与基座 `rmsvc_core::formats`（`NATIVE_EXTS`、`mime_of`、`has_ext`）
// 重复，book-serve 同一个文件里两套混用；一律改用基座那份。
