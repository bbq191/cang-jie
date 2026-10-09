//! 底层系统调用小工具（crate 内部用）：`poll` 等几个描述符"可读"。mDNS 应答器（5353 套接字 + netlink）与目录监听
//! （inotify 描述符）共用——两边都是"空闲时无限期睡、有事才醒"，不起额外线程。
use std::os::fd::RawFd;
use std::time::Duration;

/// `poll` 一组描述符的"可读"位；`timeout` 为 `None` 表示无限等。`EINTR` 返回全 false（调用方重进循环即可）。
/// 超时按毫秒**向上取整**：向下取整时剩 0.4ms 会变成 `poll(0)` 立即返回，调用方循环里就成了忙等。
pub(crate) fn poll_readable(fds: &[RawFd], timeout: Option<Duration>) -> Vec<bool> {
    let timeout_ms: i32 = match timeout {
        None => -1,
        Some(d) => {
            let ms = d.as_nanos().div_ceil(1_000_000);
            i32::try_from(ms).unwrap_or(i32::MAX)
        }
    };
    let mut pfds: Vec<libc::pollfd> = fds.iter().map(|&fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
    // SAFETY: pfds 是长度如实的可写 pollfd 数组。
    let rc = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout_ms) };
    if rc <= 0 {
        return vec![false; fds.len()];
    }
    // POLLERR/POLLHUP 也当"可读"交给读调用去取错误，别让它空转。
    pfds.iter().map(|p| p.revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP) != 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn times_out_without_data_and_wakes_on_data() {
        let (a, mut b) = std::os::unix::net::UnixStream::pair().unwrap();
        let t = std::time::Instant::now();
        assert!(!poll_readable(&[a.as_raw_fd()], Some(Duration::from_millis(30)))[0]);
        assert!(t.elapsed() >= Duration::from_millis(30));
        std::io::Write::write_all(&mut b, b"x").unwrap();
        assert!(poll_readable(&[a.as_raw_fd()], None)[0]);
        // 亚毫秒超时向上取整成 1ms，不是 0（0＝立即返回，循环里会忙等）
        let t = std::time::Instant::now();
        let (c, _d) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(!poll_readable(&[c.as_raw_fd()], Some(Duration::from_micros(300)))[0]);
        assert!(t.elapsed() >= Duration::from_micros(300));
    }
}
