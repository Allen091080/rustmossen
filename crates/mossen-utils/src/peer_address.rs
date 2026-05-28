//! # peer_address — 对等地址解析
//!
//! 对应 TypeScript `utils/peerAddress.ts`。
//! 对等地址解析，保持与 peerRegistry.ts 的分离。

/// 地址解析结果。
#[derive(Debug, Clone, PartialEq)]
pub enum AddressScheme {
    /// Unix Domain Socket
    Uds,
    /// Bridge
    Bridge,
    /// 其他
    Other,
}

/// 解析 URI 样式地址为 scheme + target。
pub fn parse_address(to: &str) -> (AddressScheme, String) {
    if to.starts_with("uds:") {
        (AddressScheme::Uds, to[4..].to_string())
    } else if to.starts_with("bridge:") {
        (AddressScheme::Bridge, to[7..].to_string())
    } else if to.starts_with('/') {
        // 遗留：旧代码 UDS 发送方发出裸套接字路径
        (AddressScheme::Uds, to.to_string())
    } else {
        (AddressScheme::Other, to.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uds_address() {
        let (scheme, target) = parse_address("uds:/tmp/socket.sock");
        assert_eq!(scheme, AddressScheme::Uds);
        assert_eq!(target, "/tmp/socket.sock");
    }

    #[test]
    fn test_parse_bridge_address() {
        let (scheme, target) = parse_address("bridge:localhost:8080");
        assert_eq!(scheme, AddressScheme::Bridge);
        assert_eq!(target, "localhost:8080");
    }

    #[test]
    fn test_parse_other_address() {
        let (scheme, target) = parse_address("localhost:3000");
        assert_eq!(scheme, AddressScheme::Other);
        assert_eq!(target, "localhost:3000");
    }
}
