//! # ca_certs — CA 证书加载工具
//!
//! 对应 TypeScript `utils/caCerts.ts`。
//! 为 TLS 连接加载 CA 证书。

use once_cell::sync::Lazy;
use std::sync::Mutex;

static CA_CACHE: Lazy<Mutex<Option<Option<Vec<String>>>>> = Lazy::new(|| Mutex::new(None));

/// 获取 CA 证书配置。
///
/// 对应 TS `getCACertificates`。Rust 使用系统 native-tls/rustls，因此本实现
/// 仅返回从 `NODE_EXTRA_CA_CERTS` 指向的文件读取到的证书内容，未配置时返回
/// `None`，由运行时使用默认证书库。
pub fn get_ca_certificates() -> Option<Vec<String>> {
    if let Some(cached) = CA_CACHE.lock().unwrap().clone() {
        return cached;
    }
    let extra = std::env::var("NODE_EXTRA_CA_CERTS").ok();
    let result = match extra {
        None => None,
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(contents) => Some(vec![contents]),
            Err(_) => None,
        },
    };
    *CA_CACHE.lock().unwrap() = Some(result.clone());
    result
}

/// 清空 CA 证书缓存，对应 TS `clearCACertsCache`。
pub fn clear_ca_certs_cache() {
    *CA_CACHE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ca_certs_env_not_set() {
        std::env::remove_var("NODE_EXTRA_CA_CERTS");
        assert!(get_ca_certificates().is_none());
    }
}
