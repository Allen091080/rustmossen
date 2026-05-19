//! # Error Utilities
//!
//! 翻译自 `services/api/errorUtils.ts` (261行)
//! 提供连接错误提取、SSL 错误检测、API 错误格式化。

use std::collections::HashSet;
use std::sync::LazyLock;
use super::sdk::MossenAPIError;

#[allow(unused_imports)]
use serde_json;

/// SSL/TLS error codes from OpenSSL
static SSL_ERROR_CODES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    // Certificate verification errors
    set.insert("UNABLE_TO_VERIFY_LEAF_SIGNATURE");
    set.insert("UNABLE_TO_GET_ISSUER_CERT");
    set.insert("UNABLE_TO_GET_ISSUER_CERT_LOCALLY");
    set.insert("CERT_SIGNATURE_FAILURE");
    set.insert("CERT_NOT_YET_VALID");
    set.insert("CERT_HAS_EXPIRED");
    set.insert("CERT_REVOKED");
    set.insert("CERT_REJECTED");
    set.insert("CERT_UNTRUSTED");
    // Self-signed certificate errors
    set.insert("DEPTH_ZERO_SELF_SIGNED_CERT");
    set.insert("SELF_SIGNED_CERT_IN_CHAIN");
    // Chain errors
    set.insert("CERT_CHAIN_TOO_LONG");
    set.insert("PATH_LENGTH_EXCEEDED");
    // Hostname/altname errors
    set.insert("ERR_TLS_CERT_ALTNAME_INVALID");
    set.insert("HOSTNAME_MISMATCH");
    // TLS handshake errors
    set.insert("ERR_TLS_HANDSHAKE_TIMEOUT");
    set.insert("ERR_SSL_WRONG_VERSION_NUMBER");
    set.insert("ERR_SSL_DECRYPTION_FAILED_OR_BAD_RECORD_MAC");
    set
});

/// Details extracted from a connection error's cause chain.
#[derive(Debug, Clone)]
pub struct ConnectionErrorDetails {
    pub code: String,
    pub message: String,
    pub is_ssl_error: bool,
}

/// Extracts connection error details from the error cause chain.
/// The provider SDK wraps underlying errors in the `cause` property.
/// This function walks the cause chain to find the root error code/message.
pub fn extract_connection_error_details(error: &MossenAPIError) -> Option<ConnectionErrorDetails> {
    // In Rust, we check if the error has a connection error code
    let code = error.error_code.as_deref()?;
    let is_ssl_error = SSL_ERROR_CODES.contains(code);
    Some(ConnectionErrorDetails {
        code: code.to_string(),
        message: error.message.clone(),
        is_ssl_error,
    })
}

/// Returns an actionable hint for SSL/TLS errors, intended for contexts outside
/// the main API client (OAuth token exchange, preflight connectivity checks).
pub fn get_ssl_error_hint(error: &MossenAPIError) -> Option<String> {
    let details = extract_connection_error_details(error)?;
    if !details.is_ssl_error {
        return None;
    }
    Some(format!(
        "SSL certificate error ({}). If you are behind a corporate proxy or TLS-intercepting firewall, \
         set NODE_EXTRA_CA_CERTS to your CA bundle path, or ask IT to allowlist *.mossen.invalid. \
         Run /doctor for details.",
        details.code
    ))
}

/// Strips HTML content (e.g., CloudFlare error pages) from a message string,
/// returning a user-friendly title or empty string if HTML is detected.
/// Returns the original message unchanged if no HTML is found.
fn sanitize_message_html(message: &str) -> String {
    if message.contains("<!DOCTYPE html") || message.contains("<html") {
        // Try to extract <title> content
        if let Some(start) = message.find("<title>") {
            let after_tag = &message[start + 7..];
            if let Some(end) = after_tag.find("</title>") {
                let title = after_tag[..end].trim();
                if !title.is_empty() {
                    return title.to_string();
                }
            }
        }
        return String::new();
    }
    message.to_string()
}

/// Detects if an error message contains HTML content (e.g., CloudFlare error pages)
/// and returns a user-friendly message instead.
pub fn sanitize_api_error(api_error: &MossenAPIError) -> String {
    let message = &api_error.message;
    if message.is_empty() {
        return String::new();
    }
    sanitize_message_html(message)
}

/// Nested API error shape after JSON round-tripping.
#[derive(Debug, serde::Deserialize)]
struct NestedAPIError {
    error: Option<NestedErrorInner>,
}

#[derive(Debug, serde::Deserialize)]
struct NestedErrorInner {
    message: Option<String>,
    error: Option<NestedErrorDeep>,
}

#[derive(Debug, serde::Deserialize)]
struct NestedErrorDeep {
    message: Option<String>,
}

fn has_nested_error(error: &MossenAPIError) -> Option<&serde_json::Value> {
    error.raw_body.as_ref().and_then(|body| {
        if let serde_json::Value::Object(map) = body {
            map.get("error").filter(|v| v.is_object())
        } else {
            None
        }
    })
}

/// Extract a human-readable message from a deserialized API error that lacks
/// a top-level `.message`.
fn extract_nested_error_message(error: &MossenAPIError) -> Option<String> {
    let body = error.raw_body.as_ref()?;
    let nested: NestedAPIError = serde_json::from_value(body.clone()).ok()?;
    let inner = nested.error?;

    // Standard provider API shape: { error: { error: { message } } }
    if let Some(deep) = &inner.error {
        if let Some(msg) = &deep.message {
            if !msg.is_empty() {
                let sanitized = sanitize_message_html(msg);
                if !sanitized.is_empty() {
                    return Some(sanitized);
                }
            }
        }
    }

    // Bedrock shape: { error: { message } }
    if let Some(msg) = &inner.message {
        if !msg.is_empty() {
            let sanitized = sanitize_message_html(msg);
            if !sanitized.is_empty() {
                return Some(sanitized);
            }
        }
    }

    None
}

/// Formats an API error into a human-readable string with connection error details.
pub fn format_api_error(error: &MossenAPIError) -> String {
    // Extract connection error details from the cause chain
    let connection_details = extract_connection_error_details(error);

    if let Some(ref details) = connection_details {
        let code = &details.code;

        // Handle timeout errors
        if code == "ETIMEDOUT" {
            return "Request timed out. Check your internet connection and proxy settings".to_string();
        }

        // Handle SSL/TLS errors with specific messages
        if details.is_ssl_error {
            return match code.as_str() {
                "UNABLE_TO_VERIFY_LEAF_SIGNATURE"
                | "UNABLE_TO_GET_ISSUER_CERT"
                | "UNABLE_TO_GET_ISSUER_CERT_LOCALLY" => {
                    "Unable to connect to API: SSL certificate verification failed. \
                     Check your proxy or corporate SSL certificates"
                        .to_string()
                }
                "CERT_HAS_EXPIRED" => {
                    "Unable to connect to API: SSL certificate has expired".to_string()
                }
                "CERT_REVOKED" => {
                    "Unable to connect to API: SSL certificate has been revoked".to_string()
                }
                "DEPTH_ZERO_SELF_SIGNED_CERT" | "SELF_SIGNED_CERT_IN_CHAIN" => {
                    "Unable to connect to API: Self-signed certificate detected. \
                     Check your proxy or corporate SSL certificates"
                        .to_string()
                }
                "ERR_TLS_CERT_ALTNAME_INVALID" | "HOSTNAME_MISMATCH" => {
                    "Unable to connect to API: SSL certificate hostname mismatch".to_string()
                }
                "CERT_NOT_YET_VALID" => {
                    "Unable to connect to API: SSL certificate is not yet valid".to_string()
                }
                _ => format!("Unable to connect to API: SSL error ({})", code),
            };
        }
    }

    if error.message == "Connection error." {
        // If we have a code but it's not SSL, include it for debugging
        if let Some(ref details) = connection_details {
            return format!("Unable to connect to API ({})", details.code);
        }
        return "Unable to connect to API. Check your internet connection".to_string();
    }

    // Guard: when deserialized from JSONL, the error object may be a plain object
    // without a `.message` property.
    if error.message.is_empty() {
        return extract_nested_error_message(error).unwrap_or_else(|| {
            format!("API error (status {})", error.status)
        });
    }

    let sanitized_message = sanitize_api_error(error);
    let original = &error.message;

    // Use sanitized message if it's different from the original (i.e., HTML was sanitized)
    if sanitized_message != *original && !sanitized_message.is_empty() {
        sanitized_message
    } else {
        original.to_string()
    }
}
