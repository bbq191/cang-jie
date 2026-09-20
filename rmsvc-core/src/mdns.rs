//! 极简 mDNS 应答器：让浏览器用 `https://shelf.local/` 而非记 IP（伪域名，零路由器/DNS 配置；
//! 2026-09-10 改绑标准 443 端口后，连端口号都不用带了）。
//! 只回答本机名字的 A 查询（`<name>.local`），应答地址选**与提问者同子网**的本机 IPv4（USB 网段问就答 10.11.99.1，
//! WiFi 网段问就答 WiFi 地址）。iOS/macOS/Windows 10+/Linux(avahi 或 systemd-resolved) 都能解析 .local；
//! **Android 系统解析器不查 mDNS**，安卓浏览器要走 host 热点的 dnsmasq 别名（见 shelf/docs §03j）。
//! 设备上没有 avahi，5353 端口空闲；若绑定失败（别的 mDNS 服务在跑）只打日志、功能退化，不影响网关。
use crate::netinfo::{ipv4_ifaces, Iface};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const PORT: u16 = 5353;
const TTL: u32 = 120;
/// 接口重扫间隔 = socket 读超时。此前读超时 5 秒（没有 mDNS 流量也每 5 秒醒一次）+ 每 30 秒 fork 一次 `ip`；
/// 现在读超时与重扫合并成 60 秒：**只在读超时那次（或收到包但距上次重扫已过一个间隔）顺带重扫**，
/// 重扫本身改读 `/proc`（见 `netinfo`），不 fork。代价：WiFi 后连/换网后，新地址最迟 60 秒内才会
/// 加入多播组、被应答——`shelf.local` 首次发现晚一点，用户主要通过已经解析出的地址访问，可接受；
/// 换来空闲时的定时唤醒从 12 次/分钟降到 1 次/分钟（LAN 上别的设备的 mDNS 查询仍会唤醒，那部分不可控）。
const RESCAN_INTERVAL: Duration = Duration::from_secs(60);

/// 解析一个 DNS 报文里的问题名（第一段 label 序列，不支持压缩指针——mDNS 查询极少用）。
/// 返回 (names, qtypes) 对列表。
pub fn parse_questions(pkt: &[u8]) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    if pkt.len() < 12 {
        return out;
    }
    let flags = u16::from_be_bytes([pkt[2], pkt[3]]);
    if flags & 0x8000 != 0 {
        return out; // 是应答不是查询
    }
    let qd = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let mut i = 12;
    for _ in 0..qd {
        let mut labels = Vec::new();
        loop {
            let Some(&len) = pkt.get(i) else { return out };
            i += 1;
            if len == 0 {
                break;
            }
            if len & 0xC0 == 0xC0 {
                i += 1; // 压缩指针：跳过并结束该名字（不解引用）
                break;
            }
            let end = i + len as usize;
            let Some(part) = pkt.get(i..end) else { return out };
            labels.push(String::from_utf8_lossy(part).to_string());
            i = end;
        }
        let Some(q) = pkt.get(i..i + 4) else { return out };
        let qtype = u16::from_be_bytes([q[0], q[1]]);
        i += 4;
        out.push((labels.join(".").to_ascii_lowercase(), qtype));
    }
    out
}

/// 构造 A 记录应答（QR|AA，id 0，cache-flush 位）。
pub fn build_answer(name: &str, ip: Ipv4Addr) -> Vec<u8> {
    let mut p = vec![0, 0, 0x84, 0x00, 0, 0, 0, 1, 0, 0, 0, 0];
    for label in name.split('.') {
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.push(0);
    p.extend_from_slice(&[0, 1, 0x80, 0x01]); // A, IN + cache-flush
    p.extend_from_slice(&TTL.to_be_bytes());
    p.extend_from_slice(&[0, 4]);
    p.extend_from_slice(&ip.octets());
    p
}

/// 选与提问者同子网的本机地址。
pub fn pick_ip(ifaces: &[Iface], from: Ipv4Addr) -> Option<Ipv4Addr> {
    ifaces.iter().find(|i| i.contains(from)).map(|i| i.ip)
}

fn open_socket() -> Result<Socket, String> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
    s.set_reuse_address(true).map_err(|e| e.to_string())?;
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    let _ = s.set_reuse_port(true);
    s.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT)).into()).map_err(|e| format!("绑定 udp/{PORT}: {e}"))?;
    let _ = s.set_multicast_loop_v4(false);
    let _ = s.set_multicast_ttl_v4(255);
    s.set_read_timeout(Some(RESCAN_INTERVAL)).map_err(|e| e.to_string())?;
    Ok(s)
}

/// 阻塞跑应答器（放线程里）。`names` 不带 `.local`。接口每 [`RESCAN_INTERVAL`]（60s）重扫（WiFi 后连也能答）。
pub fn serve(names: Vec<String>) -> Result<(), String> {
    let raw = open_socket()?;
    let sock: std::net::UdpSocket = raw.try_clone().map_err(|e| e.to_string())?.into();
    let wanted: Vec<String> = names.iter().map(|n| format!("{}.local", n.trim().to_ascii_lowercase())).filter(|n| n != ".local").collect();
    if wanted.is_empty() {
        return Err("无 mDNS 名字".into());
    }
    let mut ifaces: Vec<Iface> = Vec::new();
    let mut joined: Vec<Ipv4Addr> = Vec::new();
    let mut last_scan: Option<std::time::Instant> = None;
    let mut buf = [0u8; 1500];
    let rescan = |ifaces: &mut Vec<Iface>, joined: &mut Vec<Ipv4Addr>| {
        *ifaces = ipv4_ifaces();
        for i in ifaces.iter() {
            if !joined.contains(&i.ip) && sock.join_multicast_v4(&GROUP, &i.ip).is_ok() {
                joined.push(i.ip);
            }
        }
    };
    loop {
        // 启动时扫一次；之后只在"读超时"或"收到包但已过一个重扫间隔"时重扫。
        if last_scan.is_none() {
            rescan(&mut ifaces, &mut joined);
            last_scan = Some(std::time::Instant::now());
        }
        let (n, from) = match sock.recv_from(&mut buf) {
            Ok(x) => {
                if last_scan.is_some_and(|t| t.elapsed() >= RESCAN_INTERVAL) {
                    rescan(&mut ifaces, &mut joined);
                    last_scan = Some(std::time::Instant::now());
                }
                x
            }
            // 超时：顺带重扫（WiFi 后连也能答）；EINTR（Interrupted，如进程 spawn 子进程时 SIGCHLD 打断阻塞 recv）只是重试，别刷屏
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                rescan(&mut ifaces, &mut joined);
                last_scan = Some(std::time::Instant::now());
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                std::thread::sleep(Duration::from_secs(1));
                eprintln!("[mdns] recv: {e}");
                continue;
            }
        };
        let SocketAddr::V4(from4) = from else { continue };
        for (qname, qtype) in parse_questions(&buf[..n]) {
            if !(qtype == 1 || qtype == 255) || !wanted.iter().any(|w| *w == qname) {
                continue;
            }
            let Some(ip) = pick_ip(&ifaces, *from4.ip()) else { continue };
            let pkt = build_answer(&qname, ip);
            let _ = raw.set_multicast_if_v4(&ip);
            let _ = sock.send_to(&pkt, SocketAddrV4::new(GROUP, PORT));
            if from4.port() != PORT {
                let _ = sock.send_to(&pkt, from4); // 单播提问（QU/传统解析器）也单播答一份
            }
        }
    }
}

/// 后台线程启动；失败只打日志。
pub fn spawn(names: Vec<String>) {
    std::thread::Builder::new().name("mdns".into()).spawn(move || {
        if let Err(e) = serve(names) {
            eprintln!("[mdns] 未启用: {e}");
        }
    }).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_roundtrip() {
        // 手工拼一个查询：id 0x1234, flags 0, qd=1, "shelf.local" A IN
        let mut q = vec![0x12, 0x34, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for l in ["shelf", "local"] {
            q.push(l.len() as u8);
            q.extend_from_slice(l.as_bytes());
        }
        q.extend_from_slice(&[0, 0, 1, 0, 1]);
        assert_eq!(parse_questions(&q), vec![("shelf.local".into(), 1)]);
        let a = build_answer("shelf.local", Ipv4Addr::new(10, 42, 0, 224));
        assert_eq!(&a[2..4], &[0x84, 0]);
        assert_eq!(&a[a.len() - 4..], &[10, 42, 0, 224]);
        assert!(parse_questions(&a).is_empty(), "应答不当查询");
        assert!(parse_questions(&q[..5]).is_empty(), "截断不 panic");
        let ifs = vec![Iface { name: "wlan0".into(), ip: "10.42.0.224".parse().unwrap(), prefix: 24 }, Iface { name: "usb0".into(), ip: "10.11.99.1".parse().unwrap(), prefix: 24 }];
        assert_eq!(pick_ip(&ifs, "10.11.99.2".parse().unwrap()), Some("10.11.99.1".parse().unwrap()));
        assert_eq!(pick_ip(&ifs, "192.168.1.5".parse().unwrap()), None);
    }
}
