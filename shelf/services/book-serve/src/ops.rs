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

    /// 尝试给条目加忙锁；已经忙着 → `None`（调用方据此拒绝这次操作，不排队不覆盖）。返回的守卫离开作用域（含 panic 展开）
    /// 时自动解锁——此前各处手写"加锁 … 记得解锁"，改名要配对解两把、后台线程要在 panic 兜底之后再解，容易漏。
    pub fn try_guard(&self, name: &str) -> Option<OpGuard> {
        lock(&self.0).insert(name.to_string()).then(|| OpGuard { reg: self.clone(), name: name.to_string() })
    }
}

/// [`OpRegistry::try_guard`] 的忙锁，Drop 时解锁。可以移进后台线程。
pub struct OpGuard {
    reg: OpRegistry,
    name: String,
}

impl Drop for OpGuard {
    fn drop(&mut self) {
        lock(&self.reg.0).remove(&self.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_is_exclusive_and_guard_drop_clears_it() {
        let r = OpRegistry::default();
        let g = r.try_guard("a.epub").unwrap();
        assert!(r.try_guard("a.epub").is_none(), "同一条目不能同时跑两个操作");
        assert!(r.is_busy("a.epub") && !r.is_busy("b.epub"));
        drop(g);
        assert!(!r.is_busy("a.epub"));
        let r2 = r.clone();
        let _ = std::thread::spawn(move || {
            let _g = r2.try_guard("a.epub").unwrap();
            panic!("boom");
        })
        .join();
        assert!(!r.is_busy("a.epub"), "panic 展开时守卫照样解锁");
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
        assert!(r.try_guard("a").is_some(), "poison 后仍可用");
    }
}
