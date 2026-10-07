//! 母版库条目的"正在处理"登记簿（忙锁）。
//!
//! 进程内存态，**不落盘**：进程重启＝没有任何操作还在跑，"忙"天然清零（sidecar 里的 status 只管"上次结果展示"，
//! 不参与忙判断；重启后遗留的 pending 由 `Staging::recover_interrupted` 修正）。同一条目的落库、删除、改名互斥。
//! 以前还有取消协作（EPUB 优化每处理完一个条目检查一次取消标记）；书架 2026-10-07 不再优化书，剩下的操作（整本上传、
//! 大文件通道）都没有安全的中断点，取消随之删除。

use rmsvc_core::sync::lock;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct OpRegistry(Arc<Mutex<HashSet<String>>>);

impl OpRegistry {
    pub fn is_busy(&self, name: &str) -> bool {
        lock(&self.0).contains(name)
    }

    /// 尝试给条目加忙锁；已经忙着 → false（调用方据此拒绝这次操作，不排队不覆盖）。
    pub fn try_start(&self, name: &str) -> bool {
        lock(&self.0).insert(name.to_string())
    }

    /// 结束操作，清掉忙锁。
    pub fn end(&self, name: &str) {
        lock(&self.0).remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_is_exclusive_and_end_clears_it() {
        let r = OpRegistry::default();
        assert!(r.try_start("a.epub"));
        assert!(!r.try_start("a.epub"), "同一条目不能同时跑两个操作");
        assert!(r.is_busy("a.epub") && !r.is_busy("b.epub"));
        r.end("a.epub");
        assert!(!r.is_busy("a.epub"));
        assert!(r.try_start("a.epub"));
    }

    #[test]
    fn poisoned_lock_does_not_cascade() {
        let r = OpRegistry::default();
        let r2 = r.clone();
        let _ = std::thread::spawn(move || {
            let _g = lock(&r2.0);
            panic!("boom");
        })
        .join();
        assert!(r.try_start("a"), "poison 后仍可用");
    }
}
