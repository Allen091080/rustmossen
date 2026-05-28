//! Error types and utilities for Mossen.
//!
//! Provides structured error types analogous to the TS `errors.ts` module,
//! plus helper functions for error classification and message extraction.

use std::io;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Core error types
// ---------------------------------------------------------------------------

/// Top-level Mossen error — catchall for domain errors that don't belong to
/// a more specific category.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct MossenError {
    pub message: String,
}

impl MossenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Error raised when a slash-command string cannot be parsed.
#[derive(Debug, Error)]
#[error("malformed command: {0}")]
pub struct MalformedCommandError(pub String);

/// Abort signal — used to cancel in-flight operations.
#[derive(Debug, Error)]
#[error("aborted{}", .0.as_ref().map(|m| format!(": {m}")).unwrap_or_default())]
pub struct AbortError(pub Option<String>);

impl AbortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(Some(message.into()))
    }

    pub fn empty() -> Self {
        Self(None)
    }
}

/// Configuration file parse error — carries the file path and a default config
/// fallback value (serialized as JSON).
#[derive(Debug, Error)]
#[error("config parse error in {file_path}: {message}")]
pub struct ConfigParseError {
    pub message: String,
    pub file_path: String,
    /// JSON-serialized default configuration.
    pub default_config: serde_json::Value,
}

/// Shell command execution failure.
#[derive(Debug, Error)]
#[error("shell command failed (exit code {code})")]
pub struct ShellError {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub interrupted: bool,
}

/// Error safe to log to telemetry (contains no PII or code).
#[derive(Debug, Error)]
#[error("{message}")]
pub struct TelemetrySafeError {
    pub message: String,
    pub telemetry_message: String,
}

impl TelemetrySafeError {
    pub fn new(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            telemetry_message: msg.clone(),
            message: msg,
        }
    }

    pub fn with_telemetry(message: impl Into<String>, telemetry: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            telemetry_message: telemetry.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP / network error classification
// ---------------------------------------------------------------------------

/// Coarse classification of an HTTP request error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErrorKind {
    /// 401 / 403
    Auth,
    /// Connection timeout
    Timeout,
    /// DNS failure / connection refused
    Network,
    /// Other HTTP error with a status code
    Http,
    /// Not an HTTP error at all
    Other,
}

/// Classified HTTP error with optional status and message.
#[derive(Debug, Clone)]
pub struct ClassifiedHttpError {
    pub kind: HttpErrorKind,
    pub status: Option<u16>,
    pub message: String,
}

/// Classify a `reqwest::Error` into one of the coarse buckets.
pub fn classify_http_error(err: &reqwest::Error) -> ClassifiedHttpError {
    let message = err.to_string();
    if let Some(status) = err.status() {
        let code = status.as_u16();
        if code == 401 || code == 403 {
            return ClassifiedHttpError {
                kind: HttpErrorKind::Auth,
                status: Some(code),
                message,
            };
        }
        return ClassifiedHttpError {
            kind: HttpErrorKind::Http,
            status: Some(code),
            message,
        };
    }
    if err.is_timeout() {
        return ClassifiedHttpError {
            kind: HttpErrorKind::Timeout,
            status: None,
            message,
        };
    }
    if err.is_connect() {
        return ClassifiedHttpError {
            kind: HttpErrorKind::Network,
            status: None,
            message,
        };
    }
    ClassifiedHttpError {
        kind: HttpErrorKind::Other,
        status: None,
        message,
    }
}

// ---------------------------------------------------------------------------
// Filesystem error helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the IO error indicates the path is missing or inaccessible.
///
/// Covers: NotFound, PermissionDenied, NotADirectory (via raw OS error on
/// Unix).
pub fn is_fs_inaccessible(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    ) || is_enotdir(err)
        || is_eloop(err)
}

/// Returns `true` if the IO error is ENOENT (not found).
pub fn is_enoent(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::NotFound
}

/// Check for ENOTDIR (raw OS error 20 on Unix).
fn is_enotdir(err: &io::Error) -> bool {
    err.raw_os_error() == Some(20) // ENOTDIR on macOS/Linux
}

/// Check for ELOOP (raw OS error 62 on macOS, 40 on Linux).
fn is_eloop(err: &io::Error) -> bool {
    let code = err.raw_os_error();
    code == Some(62) || code == Some(40)
}

// ---------------------------------------------------------------------------
// Message extraction helpers
// ---------------------------------------------------------------------------

/// Extract a human-readable message from any error.
pub fn error_message(err: &dyn std::error::Error) -> String {
    err.to_string()
}

/// Extract a short backtrace (first N frames) from an `anyhow::Error`.
/// In Rust this is less common than TS; we provide it for parity with
/// `shortErrorStack`.
pub fn short_error_chain(err: &anyhow::Error, max_causes: usize) -> String {
    let mut parts = Vec::with_capacity(max_causes + 1);
    parts.push(err.to_string());
    for (i, cause) in err.chain().skip(1).enumerate() {
        if i >= max_causes {
            parts.push("...".to_string());
            break;
        }
        parts.push(format!("  caused by: {cause}"));
    }
    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Convenience: convert arbitrary values to anyhow::Error
// ---------------------------------------------------------------------------

/// Convert a string into an `anyhow::Error`.
pub fn to_anyhow(msg: impl Into<String>) -> anyhow::Error {
    anyhow::anyhow!("{}", msg.into())
}
