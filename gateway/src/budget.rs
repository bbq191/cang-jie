//! 全局并发/内存预算闸门：漫画→PDF 那条改动做完后真机测出"optimize/超限分卷投递的内存峰值
//! ≈ 这次要处理的文件体积本身"（比如 245MB 源书 optimize 峰值 206MB）。`book-serve` 的忙锁
//! 是按书名分别加的，点不同的书互不阻塞——同时点几本大部头漫画，内存峰值会线性叠加，这台设备
//! 只有 ~2GB 内存、`systemd MemoryMax` 又没有真正生效，没有安全网，是真实的 OOM 风险。
//!
//! 落在这里（`gateway`）而不是各服务自己做：`book-serve`/`koreader-serve`/`gateway` 是三个
//! 完全独立的进程，进程内的 `Mutex`/`HashSet` 忙锁天然不跨进程；但所有跨服务请求（"优化"/
//! "加入xochitl"/"加入KOReader"）物理上都要经过网关这一个转发关口（见 `proxy.rs`），网关是
//! 天然的单点，用进程内的锁就够，不需要引入任何跨进程锁/共享内存/IPC。
//!
//! 不做"连续字节预算求和"这种精细模型——没有足够数据给不同操作类型/书籍类型精确的内存倍率，
//! 强行量化是假精确。按用户原话直接做成**两档**：大文件（超过 [`LARGE_THRESHOLD_BYTES`]）
//! 同一时刻最多一个在跑；小文件允许 [`MAX_SMALL_CONCURRENT`] 个同时跑。

use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 超过这个体积算"大档"。独立于 book-serve 的 `native_limit`（xochitl 上传硬限）——两个数字
/// 恰好都是 90MB 只是巧合，语义不同（那边是"传不传得上 xochitl"，这边是"值不值得单独占一个大
/// 档名额"），不做跨进程配置同步。
pub const LARGE_THRESHOLD_BYTES: u64 = 90 * 1024 * 1024;
const MAX_SMALL_CONCURRENT: u32 = 3;
/// 排队等名额的上限——不是永久卡死，超时给清楚的错误文案，跟项目里 `FOLDER_WAIT_TIMEOUT`/
/// `PIECE_RENDER_TIMEOUT` 那种"等一个条件、有超时兜底"的既有模式一致。
const ADMIT_WAIT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Large,
    Small,
}

/// 体积 → 档位。
pub fn tier_of(bytes: u64) -> Tier {
    if bytes > LARGE_THRESHOLD_BYTES {
        Tier::Large
    } else {
        Tier::Small
    }
}

#[derive(Debug)]
struct State {
    large: u32,
    small: u32,
}

#[derive(Debug)]
pub struct Budget {
    state: Mutex<State>,
    cv: Condvar,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    pub fn new() -> Budget {
        Budget { state: Mutex::new(State { large: 0, small: 0 }), cv: Condvar::new() }
    }

    /// 阻塞直到拿到这个档位的名额（或等到 [`ADMIT_WAIT_TIMEOUT`] 超时），返回一个 RAII guard，
    /// `Drop` 时自动释放名额、唤醒其他等待者。
    pub fn admit(&self, tier: Tier) -> Result<Slot<'_>, String> {
        self.admit_within(tier, ADMIT_WAIT_TIMEOUT)
    }

    /// `admit` 的实现，超时时长可注入——真实调用方永远用 [`ADMIT_WAIT_TIMEOUT`]（分钟级），
    /// 单测用毫秒级超时验证"占满后确实会超时报错"这条路径，不用真的空等 30 分钟。
    pub fn admit_within(&self, tier: Tier, timeout: Duration) -> Result<Slot<'_>, String> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.state.lock().unwrap();
        loop {
            let has_room = match tier {
                Tier::Large => guard.large == 0,
                Tier::Small => guard.small < MAX_SMALL_CONCURRENT,
            };
            if has_room {
                match tier {
                    Tier::Large => guard.large += 1,
                    Tier::Small => guard.small += 1,
                }
                return Ok(Slot { budget: self, tier });
            }
            let now = Instant::now();
            if now >= deadline {
                return Err("排队等待并发处理名额超时，请稍后重试（可能有大部头正在处理）".into());
            }
            let (g2, _) = self.cv.wait_timeout(guard, deadline - now).unwrap();
            guard = g2; // 醒来（虚假唤醒/真超时/真释放都在这里）重新判一次条件，不额外分支处理
        }
    }

    fn release(&self, tier: Tier) {
        let mut guard = self.state.lock().unwrap();
        match tier {
            Tier::Large => guard.large = guard.large.saturating_sub(1),
            Tier::Small => guard.small = guard.small.saturating_sub(1),
        }
        self.cv.notify_all();
    }
}

/// 一个名额；`Drop` 时自动归还。可以安全地 move 进另一个线程（比如等异步任务真正跑完再释放）。
#[derive(Debug)]
pub struct Slot<'a> {
    budget: &'a Budget,
    tier: Tier,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.budget.release(self.tier);
    }
}

/// 进程内唯一实例——网关本身就是单进程，不需要通过 `bind()` 挂共享状态穿透各路由处理函数，
/// 用一个懒初始化的 `'static` 单例最省事，`proxy::forward` 直接调这个函数即可。
pub fn global() -> &'static Budget {
    static B: OnceLock<Budget> = OnceLock::new();
    B.get_or_init(Budget::new)
}

/// 判断"这个名字对应的条目是不是已经不再忙"——给异步操作（优化/落库）完成侦测用，输入是
/// `GET /staging` 原样返回的 JSON（`{"items":[...]}`）。两种情况都算"已完成"：条目还在列表里
/// 且 `busy==false`；条目已经不在列表里了（比如漫画→PDF 优化把 `<stem>.epub` 改名成
/// `<stem>.pdf`，原名字这时候找不到了，不能死等一个永远不会再出现的 `busy:false`）。只有"条目
/// 还在且 busy==true"才算没完成。解析失败（网络抖动/服务重启瞬间）也当"已完成"处理——宁可提前
/// 放行下一个排队的，不要因为侦测本身出错就把名额锁死。
pub fn is_settled(list_json: &serde_json::Value, name: &str) -> bool {
    let Some(items) = list_json.get("items").and_then(|v| v.as_array()) else { return true };
    match items.iter().find(|it| it.get("name").and_then(|v| v.as_str()) == Some(name)) {
        Some(it) => !it.get("busy").and_then(|v| v.as_bool()).unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn tier_of_boundary() {
        assert_eq!(tier_of(LARGE_THRESHOLD_BYTES), Tier::Small, "刚好等于阈值算小档（严格大于才算大）");
        assert_eq!(tier_of(LARGE_THRESHOLD_BYTES + 1), Tier::Large);
        assert_eq!(tier_of(1024), Tier::Small);
    }

    #[test]
    fn small_tier_allows_up_to_max_concurrent() {
        let b = Budget::new();
        let s1 = b.admit(Tier::Small).unwrap();
        let s2 = b.admit(Tier::Small).unwrap();
        let s3 = b.admit(Tier::Small).unwrap();
        assert_eq!(b.state.lock().unwrap().small, 3);
        drop((s1, s2, s3));
        assert_eq!(b.state.lock().unwrap().small, 0, "释放后计数应归零");
    }

    #[test]
    fn large_tier_second_admit_blocks_until_first_drops() {
        let b = Arc::new(Budget::new());
        let s1 = b.admit(Tier::Large).unwrap();
        let started = Arc::new(AtomicU32::new(0));
        let admitted_at = Arc::new(Mutex::new(None::<Instant>));

        let (b2, started2, admitted_at2) = (b.clone(), started.clone(), admitted_at.clone());
        let handle = std::thread::spawn(move || {
            started2.store(1, Ordering::SeqCst);
            let _s2 = b2.admit(Tier::Large).unwrap();
            *admitted_at2.lock().unwrap() = Some(Instant::now());
        });

        // 等第二个线程确实开始排队了
        while started.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(50));
        assert!(admitted_at.lock().unwrap().is_none(), "第一个名额还没释放，第二个不该被放行");

        let released_at = Instant::now();
        drop(s1);
        handle.join().unwrap();
        let got_at = admitted_at.lock().unwrap().unwrap();
        assert!(got_at >= released_at, "第二个必须在第一个释放之后才被放行");
    }

    #[test]
    fn admit_times_out_when_stuck() {
        let b = Budget::new();
        let _held = b.admit(Tier::Large).unwrap(); // 占满 Large 档且不释放
        let started = Instant::now();
        let err = b.admit_within(Tier::Large, Duration::from_millis(80)).unwrap_err();
        assert!(err.contains("超时"), "{err}");
        assert!(started.elapsed() >= Duration::from_millis(80), "应该是真的等到超时才返回，不是立刻失败");
    }

    #[test]
    fn is_settled_true_when_entry_missing_or_not_busy() {
        let list = serde_json::json!({"items": [
            {"name": "a.epub", "busy": true},
            {"name": "b.pdf", "busy": false},
        ]});
        assert!(!is_settled(&list, "a.epub"), "还在忙不该算完成");
        assert!(is_settled(&list, "b.pdf"), "busy=false 算完成");
        assert!(is_settled(&list, "c.epub"), "条目已经不存在（比如改名）也算完成");
    }

    #[test]
    fn is_settled_defaults_to_true_on_malformed_json() {
        assert!(is_settled(&serde_json::json!({}), "a.epub"), "解析不出 items 时宁可放行不要锁死名额");
    }
}
