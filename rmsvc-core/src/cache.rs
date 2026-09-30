//! 单值 TTL 缓存：给"每次网页 refresh 都会打一遍、但算一次很重"的状态接口用（如 `/status` 里发 HTTP 探活、
//! 读全部 `.metadata`、遍历 `/proc`）。按时间失效；状态会被本服务自己的操作改变的字段，操作完成路径里
//! 调 [`TtlCache::invalidate`] 主动失效，保证"操作完马上刷新能看到变化"。
//!
//! 计算期间持锁：并发的请求排队等同一份结果，而不是各自重算一遍（重活恰恰是这个缓存要省的）。
//! `invalidate` 也要拿同一把锁，所以"计算进行中被操作打断"的情形下，失效发生在这次计算结束之后，
//! 不会被这次（可能已过时的）计算结果盖掉。
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TtlCache<T> {
    ttl: Duration,
    slot: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> TtlCache<T> {
    pub fn new(ttl: Duration) -> TtlCache<T> {
        TtlCache { ttl, slot: Mutex::new(None) }
    }

    /// 缓存没过期就直接给，否则调 `compute` 重算并存下。
    pub fn get_or(&self, compute: impl FnOnce() -> T) -> T {
        self.get_or_at(Instant::now(), compute)
    }

    /// 同 [`Self::get_or`]，"现在"由调用方给（单测用，不用真睡觉）。
    pub fn get_or_at(&self, now: Instant, compute: impl FnOnce() -> T) -> T {
        let mut g = crate::sync::lock(&self.slot);
        if let Some((at, v)) = g.as_ref() {
            if now.saturating_duration_since(*at) < self.ttl {
                return v.clone();
            }
        }
        let v = compute();
        *g = Some((now, v.clone()));
        v
    }

    /// 让下一次 `get_or` 必定重算（操作改变了缓存里的内容之后调）。
    pub fn invalidate(&self) {
        *crate::sync::lock(&self.slot) = None;
    }
}

/// 文件"戳"：(长度, 修改时间, inode)。内容变了戳就变——就地改写会改 mtime，原子写（tmp→rename）换成新 inode。
/// 带上 inode 是因为内核文件时间戳是按时钟节拍取的（毫秒级粒度），同一节拍里两次改写成等长内容时只看
/// (长度, mtime) 会误判"没变"。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    ino: u64,
}

impl FileStamp {
    pub fn of(md: &std::fs::Metadata) -> FileStamp {
        #[cfg(unix)]
        let ino = std::os::unix::fs::MetadataExt::ino(md);
        #[cfg(not(unix))]
        let ino = 0;
        FileStamp { len: md.len(), modified: md.modified().ok(), ino }
    }
    /// stat 一下取戳；文件不在 → `None`。
    pub fn read(path: &std::path::Path) -> Option<FileStamp> {
        std::fs::metadata(path).ok().map(|m| FileStamp::of(&m))
    }
}

/// 按文件戳失效的键值缓存：给"每次列表都要开文件判一遍、但文件很少变"的查询用（母版库列表的优化等级 / 落库边车、
/// 阅读方向判 zip……）。键是调用方定的名字，值连同算它时那份文件的 [`FileStamp`] 存下；下次戳相同直接给，
/// 不同就重算——判一次只剩一次 stat。此前 book-serve 里三处各写一份"(大小, mtime) 缓存"（2026-09-30 收编）。
///
/// 计算**不持锁**（两个请求同时算同一个键只是多算一次，结果幂等），免得一本大书的判定挡住别的查询。
/// 条目数到 `cap` 时整表清空重来（重算很便宜，不值得做 LRU）；调用方也可以按"还存在的键"[`Self::retain`]。
pub struct StampCache<V> {
    map: Mutex<std::collections::HashMap<String, (FileStamp, V)>>,
    cap: usize,
}

impl<V: Clone> StampCache<V> {
    pub fn new(cap: usize) -> StampCache<V> {
        StampCache { map: Mutex::new(std::collections::HashMap::new()), cap: cap.max(1) }
    }

    /// 戳没变就给缓存值，否则调 `compute` 重算并存下。
    pub fn get_or(&self, key: &str, stamp: FileStamp, compute: impl FnOnce() -> V) -> V {
        if let Some(v) = self.get(key, stamp) {
            return v;
        }
        let v = compute();
        self.put(key, stamp, v.clone());
        v
    }

    /// 戳相同时的缓存值。
    pub fn get(&self, key: &str, stamp: FileStamp) -> Option<V> {
        crate::sync::lock(&self.map).get(key).filter(|(s, _)| *s == stamp).map(|(_, v)| v.clone())
    }

    /// 直接存一个值（调用方自己算好的，或测试里放一个"假结论"来证明命中缓存时没有重算）。
    pub fn put(&self, key: &str, stamp: FileStamp, v: V) {
        let mut m = crate::sync::lock(&self.map);
        if m.len() >= self.cap && !m.contains_key(key) {
            m.clear();
        }
        m.insert(key.to_string(), (stamp, v));
    }

    /// 只留 `keep` 为真的键（条目已删/改名时清掉，免得缓存只增不减）。
    pub fn retain(&self, mut keep: impl FnMut(&str) -> bool) {
        crate::sync::lock(&self.map).retain(|k, _| keep(k));
    }

    pub fn len(&self) -> usize {
        crate::sync::lock(&self.map).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn stamp_cache_hits_until_file_changes_and_caps_size() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a");
        std::fs::write(&p, b"one").unwrap();
        let c: StampCache<u32> = StampCache::new(2);
        let calls = Cell::new(0);
        let get = || c.get_or("a", FileStamp::read(&p).unwrap(), || { calls.set(calls.get() + 1); calls.get() });
        assert_eq!((get(), get()), (1, 1), "戳没变：只算一次");
        // 原子写成等长内容（同一时钟节拍里 mtime 可能相同）：新 inode 让戳变了
        crate::fs::write_atomic(&p, b"two").unwrap();
        assert_eq!(get(), 2, "内容换了就重算");
        c.put("b", FileStamp::read(&p).unwrap(), 9);
        assert_eq!(c.len(), 2);
        c.put("c", FileStamp::read(&p).unwrap(), 9);
        assert_eq!(c.len(), 1, "满了整表清空再放");
        c.retain(|k| k != "c");
        assert!(c.is_empty());
        assert!(FileStamp::read(&t.path().join("nope")).is_none());
    }

    #[test]
    fn hits_within_ttl_and_recomputes_after_expiry() {
        let c = TtlCache::new(Duration::from_secs(3));
        let calls = Cell::new(0);
        let t0 = Instant::now();
        let get = |at: Instant| c.get_or_at(at, || { calls.set(calls.get() + 1); calls.get() });
        assert_eq!(get(t0), 1);
        assert_eq!(get(t0 + Duration::from_secs(2)), 1, "TTL 内命中，不重算");
        assert_eq!(get(t0 + Duration::from_secs(3)), 2, "到期重算");
        assert_eq!(get(t0 + Duration::from_secs(4)), 2, "新值从重算时刻起重新计时");
    }

    #[test]
    fn invalidate_forces_recompute_even_within_ttl() {
        let c = TtlCache::new(Duration::from_secs(60));
        let n = Cell::new(0);
        assert_eq!(c.get_or(|| { n.set(n.get() + 1); n.get() }), 1);
        assert_eq!(c.get_or(|| { n.set(n.get() + 1); n.get() }), 1);
        c.invalidate();
        assert_eq!(c.get_or(|| { n.set(n.get() + 1); n.get() }), 2, "操作后失效，马上看到新值");
    }
}
