//! PKM 知识管理设备端库（块5）。首个能力=★全局待办的核心模块：
//!   - stardetect：扫书库 .rm 找红笔画的五角星（PenColor + 几何判据）
//!   - cardsync  ：星→总结卡片页的增量 merge（想法增改删双向去重）
//!   - cardnote  ：造/更新"总结卡片"笔记本（.rm 笔记页，复用 reading 的 notebook_rm）
//! 阅读栈能力（页→章 epubindex、文件监听 fswatch、书库注入 inject）复用 `weread_device`，
//! 单向依赖不反向。
pub mod cardnote;
pub mod cardsync;
pub mod stardetect;
