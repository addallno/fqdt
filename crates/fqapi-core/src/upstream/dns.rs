//! 静态 musl 二进制在 Android 上没有 /etc/resolv.conf，系统 getaddrinfo 必然失败
//! （`failed to lookup address information: Try again`）。本模块提供进程内 DNS：
//! 探测 nameserver（环境变量 → getprop → 默认网关 → 公共 DNS），手写 UDP A 记录
//! 查询（零外部依赖），实现 reqwest `Resolve` 挂到所有出站 client。

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// 单个 nameserver 的查询超时。
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
/// 每个 nameserver 的尝试次数（UDP 丢包容忍）。
const QUERY_RETRIES: usize = 2;
/// 兜底公共 DNS（阿里/DNSPod），保证无任何探测结果时仍可用。
const FALLBACK_DNS: [&str; 2] = ["223.5.5.5", "119.29.29.29"];

/// 探测到的 nameserver 列表（进程内只探测一次）。
pub fn dns_servers() -> &'static [IpAddr] {
    static SERVERS: OnceLock<Vec<IpAddr>> = OnceLock::new();
    SERVERS.get_or_init(probe_servers)
}

/// 探测顺序：`FQDT_DNS` 环境变量（逗号分隔，显式短路）→ `/etc/resolv.conf`
/// （Linux/WSL 有系统 DNS 配置；WSL 网关不答 DNS，作 nameserver 会白等
/// 12s/主机，故 resolv.conf 命中即跳过网关）→ `getprop net.dns1/net.dns2`
/// （Android ≤7 有效）→ `/proc/net/route` 默认网关 → 公共 DNS 兜底。
fn probe_servers() -> Vec<IpAddr> {
    if let Ok(env) = std::env::var("FQDT_DNS") {
        let mut s: Vec<IpAddr> = Vec::new();
        for part in env.split([',', ';', ' ']) {
            if let Ok(ip) = part.trim().parse::<IpAddr>() {
                if !s.contains(&ip) {
                    s.push(ip);
                }
            }
        }
        if !s.is_empty() {
            return s;
        }
    }
    let conf = resolv_conf_servers();
    if !conf.is_empty() {
        let mut s = conf;
        for fb in FALLBACK_DNS {
            if let Ok(ip) = fb.parse::<IpAddr>() {
                s.push(ip);
            }
        }
        return s;
    }
    probe_android_or_gateway()
}

/// 无系统 resolv.conf 的环境（旧 Android）：getprop → 默认网关 → 公共兜底。
fn probe_android_or_gateway() -> Vec<IpAddr> {
    let mut servers: Vec<IpAddr> = Vec::new();
    let mut push = |s: &str| {
        if let Ok(ip) = s.trim().parse::<IpAddr>() {
            if !servers.contains(&ip) {
                servers.push(ip);
            }
        }
    };
    for key in ["net.dns1", "net.dns2"] {
        if let Ok(out) = std::process::Command::new("getprop").arg(key).output() {
            if out.status.success() {
                push(&String::from_utf8_lossy(&out.stdout));
            }
        }
    }
    if let Some(gw) = default_gateway() {
        push(&gw.to_string());
    }
    for fb in FALLBACK_DNS {
        push(fb);
    }
    servers
}

/// 读 `/etc/resolv.conf` 的 `nameserver` 行（WSL/Linux 系统真实 DNS）。
fn resolv_conf_servers() -> Vec<IpAddr> {
    let Ok(text) = std::fs::read_to_string("/etc/resolv.conf") else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("nameserver"))
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

/// 读 `/proc/net/route` 的默认路由网关（目的 `00000000`，网关字段小端十六进制）。
fn default_gateway() -> Option<IpAddr> {
    let text = std::fs::read_to_string("/proc/net/route").ok()?;
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 || cols[1] != "00000000" {
            continue;
        }
        let raw = u32::from_str_radix(cols[2], 16).ok()?;
        // /proc/net/route 中网关为小端主机序，转网络序即得点分地址。
        let bytes = raw.to_le_bytes();
        return Some(IpAddr::V4(Ipv4Addr::new(
            bytes[0], bytes[1], bytes[2], bytes[3],
        )));
    }
    None
}

/// 对 `host` 做 A 记录查询，逐 nameserver 轮询直至拿到非空结果。
/// （单 server 的超时重试在 `query_server` 内部，这里不再套一层。）
pub fn lookup_a(host: &str) -> Result<Vec<IpAddr>, String> {
    let query = build_query(host)?;
    let mut last_err = "no nameserver".to_string();
    for server in dns_servers() {
        match query_server(*server, &query) {
            Ok(addrs) if !addrs.is_empty() => return Ok(addrs),
            Ok(_) => last_err = format!("{server}: 空 A 记录"),
            Err(e) => last_err = e,
        }
    }
    Err(format!("DNS {host}: {last_err}"))
}

/// 构造标准查询报文（RD=1，QTYPE=A，QCLASS=IN），返回 (报文, 事务 ID)。
fn build_query(host: &str) -> Result<(Vec<u8>, u16), String> {
    let id = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u16)
        .unwrap_or(0))
        ^ host.len() as u16;
    let mut q = Vec::with_capacity(host.len() + 18);
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&0x0100u16.to_be_bytes()); // 标准查询 + 递归期望
    q.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    q.extend_from_slice(&[0; 6]); // AN/NS/AR=0
    for label in host.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(format!("非法域名标签: {host}"));
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&1u16.to_be_bytes()); // QTYPE=A
    q.extend_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
    Ok((q, id))
}

/// 向单个 nameserver 发起一次 UDP 查询（带超时与重试）。
fn query_server(server: IpAddr, query: &(Vec<u8>, u16)) -> Result<Vec<IpAddr>, String> {
    let (packet, id) = query;
    let sock = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("{server}: bind {e}"))?;
    sock.set_read_timeout(Some(QUERY_TIMEOUT))
        .map_err(|e| format!("{server}: timeout {e}"))?;
    let dst = SocketAddr::new(server, 53);
    sock.connect(dst).map_err(|e| format!("{server}: connect {e}"))?;
    let mut last = String::new();
    for _ in 0..QUERY_RETRIES {
        if let Err(e) = sock.send(packet) {
            last = format!("{server}: send {e}");
            continue;
        }
        let mut buf = [0u8; 2048];
        match sock.recv(&mut buf) {
            Ok(n) => return parse_response(&buf[..n], *id),
            Err(e) => last = format!("{server}: recv {e}"),
        }
    }
    Err(last)
}

/// 解析 DNS 响应：校验事务 ID/QR/RCODE，收集 answer 中的 A 记录。
fn parse_response(resp: &[u8], expect_id: u16) -> Result<Vec<IpAddr>, String> {
    if resp.len() < 12 {
        return Err("响应过短".into());
    }
    let id = u16::from_be_bytes([resp[0], resp[1]]);
    if id != expect_id {
        return Err("事务 ID 不匹配".into());
    }
    let flags = u16::from_be_bytes([resp[2], resp[3]]);
    if flags & 0x8000 == 0 {
        return Err("非响应报文".into());
    }
    let rcode = flags & 0x000F;
    if rcode != 0 {
        return Err(format!("rcode={rcode}（NXDOMAIN 等）"));
    }
    let qd = u16::from_be_bytes([resp[4], resp[5]]);
    let an = u16::from_be_bytes([resp[6], resp[7]]);
    let mut pos = 12usize;
    for _ in 0..qd {
        pos = skip_name(resp, pos)?;
        pos = pos
            .checked_add(4)
            .filter(|&p| p <= resp.len())
            .ok_or("question 越界")?;
    }
    let mut addrs = Vec::new();
    for _ in 0..an {
        pos = skip_name(resp, pos)?;
        if pos + 10 > resp.len() {
            break;
        }
        let rtype = u16::from_be_bytes([resp[pos], resp[pos + 1]]);
        let rdlen = u16::from_be_bytes([resp[pos + 8], resp[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > resp.len() {
            break;
        }
        // 只取 A；CNAME/AAAA 等按 rdlen 直接跳过（同响应内 CNAME 链尾的 A 会被收集）。
        if rtype == 1 && rdlen == 4 {
            addrs.push(IpAddr::V4(Ipv4Addr::new(
                resp[pos],
                resp[pos + 1],
                resp[pos + 2],
                resp[pos + 3],
            )));
        }
        pos += rdlen;
    }
    Ok(addrs)
}

/// 跳过域名（含 0xC0 压缩指针），返回结束后的偏移。
fn skip_name(buf: &[u8], mut pos: usize) -> Result<usize, String> {
    loop {
        let len = *buf.get(pos).ok_or("名字越界")?;
        if len == 0 {
            return Ok(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            buf.get(pos + 1).ok_or("压缩指针越界")?;
            return Ok(pos + 2);
        }
        pos = pos
            .checked_add(1 + len as usize)
            .filter(|&p| p <= buf.len())
            .ok_or("名字越界")?;
    }
}

/// 挂到 `ClientBuilder::dns_resolver` 的解析器（阻塞查询移出 async 上下文）。
pub struct DnsResolver;

impl Resolve for DnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let task = tokio::task::spawn_blocking(move || lookup_a(&host));
            match task.await {
                Ok(Ok(addrs)) => {
                    // 端口由 hyper connector 按目标 URI 覆盖，此处置 0。
                    let iter = addrs.into_iter().map(|ip| SocketAddr::new(ip, 0));
                    Ok(Box::new(iter) as Addrs)
                }
                Ok(Err(e)) => Err(Box::new(std::io::Error::other(e)) as BoxError),
                Err(e) => Err(Box::new(e) as BoxError),
            }
        })
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_query_roundtrip() {
        let (q, id) = build_query("www.example.com").unwrap();
        // 手工构造一个同 ID 的响应：header + 原 question + 一条 A 记录。
        let mut resp = q.clone();
        resp[2..4].copy_from_slice(&0x8180u16.to_be_bytes()); // QR+RD+RA, rcode 0
        resp[6..8].copy_from_slice(&1u16.to_be_bytes()); // AN=1
        resp.extend_from_slice(&[0xC0, 0x0C]); // 压缩指针指回 question 的名字
        resp.extend_from_slice(&1u16.to_be_bytes()); // A
        resp.extend_from_slice(&1u16.to_be_bytes()); // IN
        resp.extend_from_slice(&[0, 0, 0, 60]); // TTL
        resp.extend_from_slice(&4u16.to_be_bytes()); // RDLENGTH
        resp.extend_from_slice(&[93, 184, 216, 34]); // 93.184.216.34
        let addrs = parse_response(&resp, id).unwrap();
        assert_eq!(addrs, vec![IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))]);
    }

    #[test]
    fn rejects_wrong_id() {
        let (q, id) = build_query("a.example.com").unwrap();
        let mut resp = q;
        resp[0..2].copy_from_slice(&(id ^ 0xFFFF).to_be_bytes());
        assert!(parse_response(&resp, id).is_err());
    }

    #[test]
    fn skip_name_handles_compression() {
        let buf = [3, b'w', b'w', b'w', 0xC0, 0x10];
        assert_eq!(skip_name(&buf, 0).unwrap(), 6);
    }

    #[test]
    fn probe_servers_nonempty() {
        let s = dns_servers();
        assert!(s.len() >= 2, "至少应有公共 DNS 兜底: {s:?}");
        assert!(s.contains(&"223.5.5.5".parse().unwrap()));
    }
}
