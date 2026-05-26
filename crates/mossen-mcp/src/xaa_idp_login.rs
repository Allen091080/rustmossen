//! # xaa_idp_login — XAA IdP id_token 缓存与发现
//!
//! 对应 TypeScript `services/mcp/xaaIdpLogin.ts`。XAA 流程的 IdP 侧：
//! - 是否启用 XAA；
//! - IdP 设置访问；
//! - id_token 缓存（按 issuer 归一化）；
//! - IdP client secret 缓存；
//! - OIDC discovery 入口（HTTP 由调用方注入）。

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// `xaaIdpLogin.ts` `isXaaEnabled`。
pub fn is_xaa_enabled() -> bool {
    std::env::var("MOSSEN_CODE_ENABLE_XAA")
        .map(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// `xaaIdpLogin.ts` `XaaIdpSettings`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XaaIdpSettings {
    pub issuer: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
}

/// `xaaIdpLogin.ts` `getXaaIdpSettings` 的 Rust 形态。
///
/// 调用方注入完整 `settings` JSON；这里只负责取出 `xaaIdp` 字段。
pub fn get_xaa_idp_settings(settings: &serde_json::Value) -> Option<XaaIdpSettings> {
    let xaa = settings.get("xaaIdp")?;
    serde_json::from_value(xaa.clone()).ok()
}

const ID_TOKEN_EXPIRY_BUFFER_S: i64 = 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdpIdTokenEntry {
    id_token: String,
    /// Unix-ms.
    expires_at: i64,
}

#[derive(Default)]
struct XaaIdpStorage {
    mcp_xaa_idp: HashMap<String, IdpIdTokenEntry>,
    mcp_xaa_idp_config: HashMap<String, String>,
}

fn storage() -> &'static RwLock<XaaIdpStorage> {
    static S: OnceLock<RwLock<XaaIdpStorage>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(XaaIdpStorage::default()))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `xaaIdpLogin.ts` `issuerKey`。
pub fn issuer_key(issuer: &str) -> String {
    if let Ok(mut u) = url::Url::parse(issuer) {
        if let Some(host) = u.host_str() {
            let lower = host.to_ascii_lowercase();
            let _ = u.set_host(Some(&lower));
        }
        // Strip trailing slashes from path.
        let path = u.path().trim_end_matches('/').to_string();
        u.set_path(&path);
        u.to_string()
    } else {
        issuer.trim_end_matches('/').to_string()
    }
}

/// `xaaIdpLogin.ts` `getCachedIdpIdToken`。
pub fn get_cached_idp_id_token(idp_issuer: &str) -> Option<String> {
    let s = storage().read().unwrap();
    let entry = s.mcp_xaa_idp.get(&issuer_key(idp_issuer))?;
    let remaining = entry.expires_at - now_ms();
    if remaining <= ID_TOKEN_EXPIRY_BUFFER_S * 1000 {
        return None;
    }
    Some(entry.id_token.clone())
}

fn save_idp_id_token(idp_issuer: &str, id_token: &str, expires_at: i64) {
    let mut s = storage().write().unwrap();
    s.mcp_xaa_idp.insert(
        issuer_key(idp_issuer),
        IdpIdTokenEntry {
            id_token: id_token.to_string(),
            expires_at,
        },
    );
}

/// `xaaIdpLogin.ts` `saveIdpIdTokenFromJwt`。
///
/// 解析 JWT 的 `exp` 字段（如果存在）作为缓存过期时间；否则 fallback 1h。
/// 返回最终 `expires_at`（unix-ms）。
pub fn save_idp_id_token_from_jwt(idp_issuer: &str, id_token: &str) -> i64 {
    let exp_from_jwt = jwt_exp(id_token);
    let expires_at = exp_from_jwt
        .map(|e| e * 1000)
        .unwrap_or_else(|| now_ms() + 3_600_000);
    save_idp_id_token(idp_issuer, id_token, expires_at);
    expires_at
}

/// `xaaIdpLogin.ts` `clearIdpIdToken`。
pub fn clear_idp_id_token(idp_issuer: &str) {
    let mut s = storage().write().unwrap();
    s.mcp_xaa_idp.remove(&issuer_key(idp_issuer));
}

/// `xaaIdpLogin.ts` `saveIdpClientSecret`。
///
/// 返回 (success, warning)。Rust 内存实现总是成功。
pub fn save_idp_client_secret(idp_issuer: &str, client_secret: &str) -> (bool, Option<String>) {
    let mut s = storage().write().unwrap();
    s.mcp_xaa_idp_config
        .insert(issuer_key(idp_issuer), client_secret.to_string());
    (true, None)
}

/// `xaaIdpLogin.ts` `getIdpClientSecret`。
pub fn get_idp_client_secret(idp_issuer: &str) -> Option<String> {
    let s = storage().read().unwrap();
    s.mcp_xaa_idp_config.get(&issuer_key(idp_issuer)).cloned()
}

/// `xaaIdpLogin.ts` `clearIdpClientSecret`。
pub fn clear_idp_client_secret(idp_issuer: &str) {
    let mut s = storage().write().unwrap();
    s.mcp_xaa_idp_config.remove(&issuer_key(idp_issuer));
}

/// `xaaIdpLogin.ts` `discoverOidc` 的 Rust 形态。
///
/// 调用方注入 HTTP fetch；本函数负责把 issuer 拼接到
/// `<issuer>/.well-known/openid-configuration`（避免 WHATWG URL 替换路径
/// 的陷阱）。
pub async fn discover_oidc<F, Fut>(issuer: &str, fetch_json: F) -> Result<serde_json::Value, String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, String>>,
{
    let trimmed = issuer.trim_end_matches('/');
    let url = format!("{}/.well-known/openid-configuration", trimmed);
    fetch_json(url)
        .await
        .map_err(|e| format!("XAA: OIDC discovery failed: {}", e))
}

/// `xaaIdpLogin.ts` `acquireIdpIdToken` 的 Rust 形态。
///
/// 入口职责：
/// 1. 命中缓存则直接返回；
/// 2. 未命中则调用注入的 `do_login` 启动浏览器流程，得到 (id_token, expires_at)；
/// 3. 写入缓存并返回。
pub async fn acquire_idp_id_token<F, Fut>(idp_issuer: &str, do_login: F) -> Result<String, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(String, i64), String>>,
{
    if let Some(cached) = get_cached_idp_id_token(idp_issuer) {
        return Ok(cached);
    }
    let (token, expires_at) = do_login().await?;
    save_idp_id_token(idp_issuer, &token, expires_at);
    Ok(token)
}

// ---------------------------------------------------------------------------
// Internal JWT exp parser
// ---------------------------------------------------------------------------

fn jwt_exp(id_token: &str) -> Option<i64> {
    let mut parts = id_token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    let payload = base64url_decode(payload_b64).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    v.get("exp").and_then(|e| e.as_i64())
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, String> {
    // base64url -> base64
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    base64_decode(&s)
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [255u8; 256];
    for (i, &c) in ALPHA.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=' && b != b'\n').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for b in bytes {
        let val = table[b as usize];
        if val == 255 {
            return Err(format!("invalid base64 char: {}", b as char));
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuer_key_lowercases_host_strips_slash() {
        let k = issuer_key("https://Example.COM/some/path/");
        assert_eq!(k, "https://example.com/some/path");
    }

    #[test]
    fn save_then_get_idp_id_token() {
        clear_idp_id_token("https://i.test");
        let expires_at = now_ms() + 3_600_000;
        save_idp_id_token("https://i.test", "tok", expires_at);
        assert_eq!(
            get_cached_idp_id_token("https://i.test").as_deref(),
            Some("tok")
        );
    }
}
