//! Error types and error handling utilities.
//!
//! Provides custom error types, error classification for HTTP errors,
//! and utility functions for extracting error information.

use std::fmt;

/// Base error type for Mossen operations.
#[derive(Debug)]
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

impl fmt::Display for MossenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MossenError {}

/// Error for malformed commands.
#[derive(Debug)]
pub struct MalformedCommandError {
    pub message: String,
}

impl MalformedCommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MalformedCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MalformedCommandError {}

/// Error indicating an operation was aborted.
#[derive(Debug)]
pub struct AbortError {
    pub message: String,
}

impl AbortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn default_abort() -> Self {
        Self {
            message: "AbortError".to_string(),
        }
    }
}

impl fmt::Display for AbortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AbortError {}

/// Check if an error is an abort error.
///
/// Returns true if the error is an AbortError instance or has a name matching "AbortError".
pub fn is_abort_error(e: &(dyn std::error::Error + 'static)) -> bool {
    e.downcast_ref::<AbortError>().is_some()
}

/// Configuration file parsing error with file path and default config.
#[derive(Debug)]
pub struct ConfigParseError {
    pub message: String,
    pub file_path: String,
    pub default_config: String,
}

impl ConfigParseError {
    pub fn new(
        message: impl Into<String>,
        file_path: impl Into<String>,
        default_config: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            file_path: file_path.into(),
            default_config: default_config.into(),
        }
    }
}

impl fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConfigParseError: {} ({})", self.message, self.file_path)
    }
}

impl std::error::Error for ConfigParseError {}

/// Shell command execution error.
#[derive(Debug)]
pub struct ShellError {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub interrupted: bool,
}

impl ShellError {
    pub fn new(stdout: String, stderr: String, code: i32, interrupted: bool) -> Self {
        Self {
            stdout,
            stderr,
            code,
            interrupted,
        }
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shell command failed (code: {})", self.code)
    }
}

impl std::error::Error for ShellError {}

/// Teleport operation error with a formatted message for display.
#[derive(Debug)]
pub struct TeleportOperationError {
    pub message: String,
    pub formatted_message: String,
}

impl TeleportOperationError {
    pub fn new(message: impl Into<String>, formatted_message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            formatted_message: formatted_message.into(),
        }
    }
}

impl fmt::Display for TeleportOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TeleportOperationError {}

/// Error with a message that is safe to log to telemetry.
///
/// Single-arg: same message for user and telemetry.
/// Two-arg: different messages (full message may have file path, telemetry doesn't).
#[derive(Debug)]
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

    pub fn with_telemetry_message(
        message: impl Into<String>,
        telemetry_message: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            telemetry_message: telemetry_message.into(),
        }
    }
}

impl fmt::Display for TelemetrySafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TelemetrySafeError {}

/// Check if an error has an exact message match.
pub fn has_exact_error_message(error: &dyn std::error::Error, message: &str) -> bool {
    error.to_string() == message
}

/// Normalize an unknown value into an error message string.
pub fn to_error_message(e: &dyn std::error::Error) -> String {
    e.to_string()
}

/// Extract a string message from an error.
pub fn error_message(e: &dyn std::error::Error) -> String {
    e.to_string()
}

/// IO error code extracted from an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrnoCode {
    Enoent,
    Eacces,
    Eperm,
    Enotdir,
    Eloop,
    Other(String),
}

/// Extract the errno code from an IO error.
pub fn get_errno_code(e: &std::io::Error) -> Option<ErrnoCode> {
    match e.kind() {
        std::io::ErrorKind::NotFound => Some(ErrnoCode::Enoent),
        std::io::ErrorKind::PermissionDenied => Some(ErrnoCode::Eacces),
        _ => {
            // Check raw OS error on Unix
            #[cfg(unix)]
            {
                if let Some(code) = e.raw_os_error() {
                    return match code {
                        2 => Some(ErrnoCode::Enoent),   // ENOENT
                        13 => Some(ErrnoCode::Eacces),  // EACCES
                        1 => Some(ErrnoCode::Eperm),    // EPERM
                        20 => Some(ErrnoCode::Enotdir), // ENOTDIR
                        40 => Some(ErrnoCode::Eloop),   // ELOOP
                        _ => Some(ErrnoCode::Other(format!("errno:{}", code))),
                    };
                }
            }
            None
        }
    }
}

/// Check if an IO error is ENOENT (file or directory does not exist).
pub fn is_enoent(e: &std::io::Error) -> bool {
    get_errno_code(e) == Some(ErrnoCode::Enoent)
}

/// Get the filesystem path from an IO error, if available.
pub fn get_errno_path(e: &std::io::Error) -> Option<String> {
    // Rust std::io::Error doesn't carry a path; this would need custom error types
    // In practice, callers should track the path they were operating on.
    let _ = e;
    None
}

/// Check if an IO error means the path is inaccessible.
///
/// Covers: ENOENT, EACCES, EPERM, ENOTDIR, ELOOP.
pub fn is_fs_inaccessible(e: &std::io::Error) -> bool {
    matches!(
        get_errno_code(e),
        Some(ErrnoCode::Enoent)
            | Some(ErrnoCode::Eacces)
            | Some(ErrnoCode::Eperm)
            | Some(ErrnoCode::Enotdir)
            | Some(ErrnoCode::Eloop)
    )
}

/// Extract error message + top N stack frames from an error.
///
/// Use when the error flows to the model as a tool_result — full stack
/// traces waste context tokens.
pub fn short_error_stack(e: &dyn std::error::Error, max_frames: usize) -> String {
    // In Rust, we typically use the Display impl + source chain
    let mut result = e.to_string();
    let mut source = e.source();
    let mut frames = 0;

    while let Some(cause) = source {
        if frames >= max_frames {
            break;
        }
        result.push_str(&format!("\n  caused by: {}", cause));
        source = cause.source();
        frames += 1;
    }

    result
}

/// Classification of HTTP/network errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErrorKind {
    /// 401/403 — authentication failure.
    Auth,
    /// Connection timeout.
    Timeout,
    /// Connection refused / DNS failure.
    Network,
    /// Other HTTP error (may have status).
    Http,
    /// Not an HTTP error.
    Other,
}

/// Classified HTTP error result.
#[derive(Debug, Clone)]
pub struct ClassifiedHttpError {
    pub kind: HttpErrorKind,
    pub status: Option<u16>,
    pub message: String,
}

/// Classify an HTTP error (e.g., from reqwest) into one of a few buckets.
pub fn classify_http_error(e: &reqwest::Error) -> ClassifiedHttpError {
    let message = e.to_string();

    if let Some(status) = e.status() {
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

    if e.is_timeout() {
        return ClassifiedHttpError {
            kind: HttpErrorKind::Timeout,
            status: None,
            message,
        };
    }

    if e.is_connect() {
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
