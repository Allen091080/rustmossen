//! Cryptographic and hashing utilities.
//!
//! Provides non-cryptographic hashing (djb2), content hashing (SHA-256),
//! UUID generation, and base64 encoding/decoding.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Non-cryptographic hashing
// ---------------------------------------------------------------------------

/// djb2 string hash — fast non-cryptographic hash returning a signed 32-bit int.
/// Deterministic across platforms.
pub fn djb2_hash(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for byte in s.bytes() {
        // (hash << 5) - hash + byte, wrapping
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(byte as i32);
    }
    hash
}

/// Hash content using SHA-256, returning the hex digest.
/// Used for change-detection (not security-critical).
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Hash two strings together without concatenation.
/// Uses incremental SHA-256 with a null separator to disambiguate
/// ("ts","code") from ("tsc","ode").
pub fn hash_pair(a: &str, b: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(a.as_bytes());
    hasher.update(b"\0");
    hasher.update(b.as_bytes());
    hex_encode(&hasher.finalize())
}

// ---------------------------------------------------------------------------
// UUID
// ---------------------------------------------------------------------------

/// Generate a new random UUID v4.
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Validate whether a string is a valid UUID (8-4-4-4-12 hex format).
/// Returns `Some(uuid_string)` if valid, `None` otherwise.
pub fn validate_uuid(maybe_uuid: &str) -> Option<&str> {
    // Quick length check
    if maybe_uuid.len() != 36 {
        return None;
    }
    // Try parsing
    Uuid::parse_str(maybe_uuid).ok().map(|_| maybe_uuid)
}

/// Generate a new agent ID with optional label prefix.
/// Format: `a{label-}{16 hex chars}` or `a{16 hex chars}`.
pub fn create_agent_id(label: Option<&str>) -> String {
    let suffix = hex_encode(&rand::random::<[u8; 8]>());
    match label {
        Some(l) => format!("a{l}-{suffix}"),
        None => format!("a{suffix}"),
    }
}

// ---------------------------------------------------------------------------
// Base64
// ---------------------------------------------------------------------------

/// Encode bytes as standard base64.
pub fn base64_encode(data: &[u8]) -> String {
    BASE64_STANDARD.encode(data)
}

/// Decode a base64 string to bytes.
pub fn base64_decode(encoded: &str) -> anyhow::Result<Vec<u8>> {
    Ok(BASE64_STANDARD.decode(encoded)?)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_djb2_hash_deterministic() {
        assert_eq!(djb2_hash("hello"), djb2_hash("hello"));
        assert_ne!(djb2_hash("hello"), djb2_hash("world"));
    }

    #[test]
    fn test_hash_pair_disambiguation() {
        // ("ts","code") != ("tsc","ode")
        assert_ne!(hash_pair("ts", "code"), hash_pair("tsc", "ode"));
    }

    #[test]
    fn test_validate_uuid() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000").is_some());
        assert!(validate_uuid("not-a-uuid").is_none());
        assert!(validate_uuid("").is_none());
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}
