//! mTLS configuration — client certificates, CA bundles, and TLS agent creation.

use std::sync::Mutex;

use once_cell::sync::Lazy;

/// mTLS configuration for client authentication.
#[derive(Debug, Clone)]
pub struct MtlsConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub passphrase: Option<String>,
}

/// Full TLS configuration (mTLS + CA certs).
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub passphrase: Option<String>,
    pub ca: Option<String>,
}

/// Cached mTLS config.
static MTLS_CONFIG_CACHE: Lazy<Mutex<Option<Option<MtlsConfig>>>> =
    Lazy::new(|| Mutex::new(None));

/// Get mTLS configuration from environment variables.
pub fn get_mtls_config() -> Option<MtlsConfig> {
    let mut cache = MTLS_CONFIG_CACHE.lock().unwrap();
    if let Some(ref cached) = *cache {
        return cached.clone();
    }

    let mut config = MtlsConfig {
        cert: None,
        key: None,
        passphrase: None,
    };

    // Client certificate
    if let Ok(cert_path) = std::env::var("MOSSEN_CODE_CLIENT_CERT") {
        match std::fs::read_to_string(&cert_path) {
            Ok(content) => {
                tracing::debug!("mTLS: Loaded client certificate from MOSSEN_CODE_CLIENT_CERT");
                config.cert = Some(content);
            }
            Err(e) => {
                tracing::error!("mTLS: Failed to load client certificate: {}", e);
            }
        }
    }

    // Client key
    if let Ok(key_path) = std::env::var("MOSSEN_CODE_CLIENT_KEY") {
        match std::fs::read_to_string(&key_path) {
            Ok(content) => {
                tracing::debug!("mTLS: Loaded client key from MOSSEN_CODE_CLIENT_KEY");
                config.key = Some(content);
            }
            Err(e) => {
                tracing::error!("mTLS: Failed to load client key: {}", e);
            }
        }
    }

    // Key passphrase
    if let Ok(passphrase) = std::env::var("MOSSEN_CODE_CLIENT_KEY_PASSPHRASE") {
        tracing::debug!("mTLS: Using client key passphrase");
        config.passphrase = Some(passphrase);
    }

    let result = if config.cert.is_some() || config.key.is_some() || config.passphrase.is_some() {
        Some(config)
    } else {
        None
    };

    *cache = Some(result.clone());
    result
}

/// Get TLS options for WebSocket connections.
pub fn get_websocket_tls_options() -> Option<TlsConfig> {
    let mtls_config = get_mtls_config();
    let ca_certs = get_ca_certificates();

    if mtls_config.is_none() && ca_certs.is_none() {
        return None;
    }

    let mtls = mtls_config.unwrap_or(MtlsConfig {
        cert: None,
        key: None,
        passphrase: None,
    });

    Some(TlsConfig {
        cert: mtls.cert,
        key: mtls.key,
        passphrase: mtls.passphrase,
        ca: ca_certs,
    })
}

/// Get TLS fetch options with mTLS + CA certs configuration.
pub fn get_tls_fetch_options() -> Option<TlsConfig> {
    let mtls_config = get_mtls_config();
    let ca_certs = get_ca_certificates();

    if mtls_config.is_none() && ca_certs.is_none() {
        return None;
    }

    let mtls = mtls_config.unwrap_or(MtlsConfig {
        cert: None,
        key: None,
        passphrase: None,
    });

    Some(TlsConfig {
        cert: mtls.cert,
        key: mtls.key,
        passphrase: mtls.passphrase,
        ca: ca_certs,
    })
}

/// Clear the mTLS configuration cache.
pub fn clear_mtls_cache() {
    let mut cache = MTLS_CONFIG_CACHE.lock().unwrap();
    *cache = None;
    tracing::debug!("Cleared mTLS configuration cache");
}

/// Configure global TLS settings (log NODE_EXTRA_CA_CERTS detection).
pub fn configure_global_mtls() {
    let mtls_config = get_mtls_config();
    if mtls_config.is_none() {
        return;
    }
    if std::env::var("NODE_EXTRA_CA_CERTS").is_ok() {
        tracing::debug!(
            "NODE_EXTRA_CA_CERTS detected - will be appended to built-in CAs"
        );
    }
}

/// Get CA certificates from environment (NODE_EXTRA_CA_CERTS).
fn get_ca_certificates() -> Option<String> {
    let path = std::env::var("NODE_EXTRA_CA_CERTS").ok()?;
    std::fs::read_to_string(&path).ok()
}

/// 对应 TS `getMTLSAgent`：返回 mTLS 配置摘要（key/cert 路径等）。
pub fn get_mtls_agent() -> Option<serde_json::Value> {
    let cert = std::env::var("MOSSEN_MTLS_CERT").ok();
    let key = std::env::var("MOSSEN_MTLS_KEY").ok();
    let ca = std::env::var("NODE_EXTRA_CA_CERTS").ok();
    if cert.is_some() || key.is_some() || ca.is_some() {
        Some(serde_json::json!({
            "cert": cert,
            "key": key,
            "ca": ca,
        }))
    } else {
        None
    }
}
