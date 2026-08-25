//! PKM 知识管理设备端库（块5）。首个能力=★全局待办的核心模块：
//!   - stardetect：扫书库 .rm 找红笔画的五角星（PenColor + 几何判据）
//!   - cardsync  ：星→总结卡片页的增量 merge（想法增改删双向去重）
//!   - cardnote  ：造/更新"总结卡片"笔记本（.rm 笔记页，复用 reading 的 notebook_rm）
//!   - notebook_sync：设备笔记本 I/O 底层（读库 + /upload 注入 + pending-trash），全汇总本共用
//!   - starscan  ：星→卡片编排（章节/标签/高亮 → sync_one_card），从 daemon 抽出
//!   - cardindex ：全库卡片 ID 索引 + MOC 死链体检（只读文本、纯逻辑）
//!   - dict      ：本地词典 mmap 二分查词（牛津英汉双解/现汉，用户自备、数据不入库）
//!   - cardvocab ：生词条 + 句界扩原句 + 《生词本》渲染（纯逻辑可测）
//!   - vocabscan ：生词本扫描编排（全库源书→灰词→查词的 IO 层，从 daemon 抽出，daemon 只触发+注入）
//!   ↑ dict/cardvocab/locate/vocabscan 是「划词查字典」，概念属**块4 系统增强**（跨块，见系统增强白皮书 §08）
//!
//! 阅读栈能力（页→章 epubindex、文件监听 fswatch、书库注入 inject）复用 `weread_device`，
//! 单向依赖不反向。
pub mod cardindex;
pub mod cardhl;
pub mod cardagg;
pub mod cardreview;
pub mod cardstats;
pub mod cardnote;
pub mod cardsync;
pub mod cardvocab;
pub mod dict;
pub mod locate;
pub mod notebook_sync;
pub mod starscan;
pub mod stardetect;
pub mod vocabscan;
