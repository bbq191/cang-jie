//! reMarkable 设备端共享底座（块3阅读 + 块5 PKM 共用）。
//!
//! 把两块都要的**低层设备能力**独立成 crate，让 pkm 只依赖这一小坨、**不再全量编译整条微信读书管线**
//! （下载/codec/login/qr/optimize…都留在 `weread-device`）。这几个模块本就零 `crate::` 内依赖、自足。
//!   - `epubindex` ：EPUB 页 → 章-节名（逆向 xochitl 的 `.epubindex` + nav.xhtml/toc.ncx）
//!   - `inject`    ：xochitl 书库注入（`/upload` 原生导入 + 直写文档件）
//!   - `notebook_rm`：造 `.rm` 笔记页 + 读回 RootText（打字批注）
//!   - `fswatch`   ：inotify + 防抖文件监听（空闲阻塞睡死零唤醒）
//!
//! `weread-device`（块3）re-export 这四个（`pub use device_core::…`）→ reading 的 bins 与代码零改动。
pub mod epubindex;
pub mod fswatch;
pub mod inject;
pub mod notebook_rm;
pub mod vision;
