//! # ssrf_guard — SSRF 防护
//!
//! 对应 TS `utils/hooks/ssrfGuard.ts`。
//! 阻止 HTTP Hook 访问私有/链路本地地址。
//! 允许 loopback (127.0.0.0/8, ::1) 用于本地开发。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// SSRF 错误。
#[derive(Debug, Clone)]
pub struct SsrfError {
    /// 主机名。
    pub hostname: String,
    /// 解析的地址。
    pub address: String,
    /// 错误消息。
    pub message: String,
}

impl std::fmt::Display for SsrfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SsrfError {}

/// 检查 IP 地址是否被阻止。
///
/// 对应 TS `isBlockedAddress()`。
///
/// 阻止的 IPv4 范围：
/// - 0.0.0.0/8 "this" 网络
/// - 10.0.0.0/8 私有
/// - 100.64.0.0/10 共享地址空间/CGNAT
/// - 169.254.0.0/16 链路本地（云元数据）
/// - 172.16.0.0/12 私有
/// - 192.168.0.0/16 私有
///
/// 阻止的 IPv6 范围：
/// - :: 未指定
/// - fc00::/7 唯一本地
/// - fe80::/10 链路本地
/// - ::ffff:<v4> 映射 IPv4（在阻止范围内）
///
/// 允许（返回 false）：
/// - 127.0.0.0/8 loopback
/// - ::1 loopback
pub fn is_blocked_address(addr: &str) -> bool {
    match addr.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => is_blocked_v4(v4),
        Ok(IpAddr::V6(v6)) => is_blocked_v6(v6),
        Err(_) => false, // 非有效 IP，让 DNS 路径处理
    }
}

/// 检查 IPv4 地址是否被阻止。
fn is_blocked_v4(addr: Ipv4Addr) -> bool {
    let octets = addr.octets();
    let (a, b) = (octets[0], octets[1]);

    // Loopback 明确允许
    if a == 127 {
        return false;
    }

    // 0.0.0.0/8
    if a == 0 {
        return true;
    }
    // 10.0.0.0/8
    if a == 10 {
        return true;
    }
    // 169.254.0.0/16 — 链路本地，云元数据
    if a == 169 && b == 254 {
        return true;
    }
    // 172.16.0.0/12
    if a == 172 && (16..=31).contains(&b) {
        return true;
    }
    // 100.64.0.0/10 — 共享地址空间 (RFC 6598, CGNAT)
    if a == 100 && (64..=127).contains(&b) {
        return true;
    }
    // 192.168.0.0/16
    if a == 192 && b == 168 {
        return true;
    }

    false
}

/// 检查 IPv6 地址是否被阻止。
fn is_blocked_v6(addr: Ipv6Addr) -> bool {
    // ::1 loopback 明确允许
    if addr.is_loopback() {
        return false;
    }

    // :: 未指定
    if addr.is_unspecified() {
        return true;
    }

    // IPv4-mapped IPv6 地址 (::ffff:X.Y.Z.W)
    if let Some(v4) = addr.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }

    let segments = addr.segments();

    // fc00::/7 — 唯一本地地址 (fc00:: 到 fdff::)
    let first_byte = (segments[0] >> 8) as u8;
    if first_byte == 0xfc || first_byte == 0xfd {
        return true;
    }

    // fe80::/10 — 链路本地
    if segments[0] >= 0xfe80 && segments[0] <= 0xfebf {
        return true;
    }

    false
}

/// 创建 SSRF 错误。
pub fn ssrf_error(hostname: &str, address: &str) -> SsrfError {
    SsrfError {
        hostname: hostname.to_string(),
        address: address.to_string(),
        message: format!(
            "HTTP hook blocked: {hostname} resolves to {address} \
             (private/link-local address). Loopback (127.0.0.1, ::1) is allowed for local dev."
        ),
    }
}

/// 验证解析后的 IP 地址是否安全。
///
/// 对应 TS `ssrfGuardedLookup()` 中的验证逻辑。
pub fn validate_resolved_address(hostname: &str, address: &str) -> Result<(), SsrfError> {
    if is_blocked_address(address) {
        Err(ssrf_error(hostname, address))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback_allowed() {
        assert!(!is_blocked_address("127.0.0.1"));
        assert!(!is_blocked_address("127.0.0.2"));
        assert!(!is_blocked_address("::1"));
    }

    #[test]
    fn test_private_blocked() {
        assert!(is_blocked_address("10.0.0.1"));
        assert!(is_blocked_address("172.16.0.1"));
        assert!(is_blocked_address("192.168.1.1"));
    }

    #[test]
    fn test_link_local_blocked() {
        assert!(is_blocked_address("169.254.169.254"));
    }

    #[test]
    fn test_public_allowed() {
        assert!(!is_blocked_address("8.8.8.8"));
        assert!(!is_blocked_address("1.1.1.1"));
    }

    #[test]
    fn test_cgnat_blocked() {
        assert!(is_blocked_address("100.100.100.200"));
        assert!(is_blocked_address("100.64.0.1"));
    }
}
