use std::fmt;

/// Base error for Mossen operations.
#[derive(Debug, Clone)]
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
        write!(f, "MossenError: {}", self.message)
    }
}

impl std::error::Error for MossenError {}

/// Error for malformed commands.
#[derive(Debug, Clone)]
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
        write!(f, "MalformedCommandError: {}", self.message)
    }
}

impl std::error::Error for MalformedCommandError {}

/// Error for abort operations.
#[derive(Debug, Clone)]
pub struct AbortError {
    pub message: String,
}

impl AbortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AbortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AbortError: {}", self.message)
    }
}

impl std::error::Error for AbortError {}

/// Returns true if the error is any kind of abort error.
pub fn is_abort_error(e: &(dyn std::error::Error + 'static)) -> bool {
    if e.downcast_ref::<AbortError>().is_some() {
        return true;
    }
    // Check by name in the display string
    let display = format!("{}", e);
    display.contains("AbortError")
}

/// Custom error class for configuration file parsing errors.
#[derive(Debug, Clone)]
pub struct ConfigParseError {
    pub message: String,
    pub file_path: String,
    pub default_config: serde_json::Value,
}

impl ConfigParseError {
    pub fn new(
        message: impl Into<String>,
        file_path: impl Into<String>,
        default_config: serde_json::Value,
    ) -> Self {
        Self {
            message: message.into(),
            file_path: file_path.into(),
            default_config,
        }
    }
}

impl fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConfigParseError: {}", self.message)
    }
}

impl std::error::Error for ConfigParseError {}

/// Error for shell command failures.
#[derive(Debug, Clone)]
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
        write!(f, "ShellError: Shell command failed (code {})", self.code)
    }
}

impl std::error::Error for ShellError {}

/// Error for teleport operations.
#[derive(Debug, Clone)]
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
        write!(f, "TeleportOperationError: {}", self.message)
    }
}

impl std::error::Error for TeleportOperationError {}

/// Error with a message that is safe to log to telemetry.
#[derive(Debug, Clone)]
pub struct TelemetrySafeError {
    pub message: String,
    pub telemetry_message: String,
}

impl TelemetrySafeError {
    pub fn new(message: impl Into<String>, telemetry_message: Option<String>) -> Self {
        let msg: String = message.into();
        let telem = telemetry_message.unwrap_or_else(|| msg.clone());
        Self {
            message: msg,
            telemetry_message: telem,
        }
    }
}

impl fmt::Display for TelemetrySafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TelemetrySafeError: {}", self.message)
    }
}

impl std::error::Error for TelemetrySafeError {}

/// Check if an error has a specific message.
pub fn has_exact_error_message(error: &dyn std::error::Error, message: &str) -> bool {
    format!("{}", error).contains(message) || error.to_string() == message
}

/// Normalize an unknown value into an Error message.
pub fn to_error_message(e: &dyn std::error::Error) -> String {
    e.to_string()
}

/// Extract a string message from an error.
pub fn error_message(e: &dyn std::error::Error) -> String {
    e.to_string()
}

/// Extract the errno code from an IO error.
pub fn get_errno_code(e: &std::io::Error) -> Option<&'static str> {
    match e.kind() {
        std::io::ErrorKind::NotFound => Some("ENOENT"),
        std::io::ErrorKind::PermissionDenied => Some("EACCES"),
        std::io::ErrorKind::AlreadyExists => Some("EEXIST"),
        std::io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
        std::io::ErrorKind::ConnectionAborted => Some("ECONNABORTED"),
        std::io::ErrorKind::AddrNotAvailable => Some("ENOTFOUND"),
        _ => None,
    }
}

/// True if the error is ENOENT (file or directory does not exist).
pub fn is_enoent(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}

/// Extract the path from an IO error if available.
pub fn get_errno_path(_e: &std::io::Error) -> Option<String> {
    // Rust's std::io::Error doesn't carry path info by default.
    // Callers should track the path themselves.
    None
}

/// Extract error message + top N stack frames.
/// In Rust, we just return the error chain.
pub fn short_error_stack(e: &dyn std::error::Error, max_frames: usize) -> String {
    let mut result = e.to_string();
    let mut current = e.source();
    let mut count = 0;
    while let Some(source) = current {
        if count >= max_frames {
            break;
        }
        result.push_str(&format!("\n  caused by: {}", source));
        current = source.source();
        count += 1;
    }
    result
}

/// True if the error means the path is missing, inaccessible, or
/// structurally unreachable.
pub fn is_fs_inaccessible(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::PermissionDenied
            // EPERM on some platforms
            | std::io::ErrorKind::Other
    ) || {
        // Check raw OS error for ENOTDIR (20) and ELOOP (40) on unix
        #[cfg(unix)]
        {
            use std::os::unix::io::RawFd;
            let _ = RawFd::default; // suppress unused import
            match e.raw_os_error() {
                Some(libc::ENOTDIR) | Some(libc::ELOOP) | Some(libc::EPERM) => true,
                _ => false,
            }
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

/// Classification of HTTP/network errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxiosErrorKind {
    /// 401/403
    Auth,
    /// Connection aborted / timeout
    Timeout,
    /// Connection refused / not found
    Network,
    /// Other HTTP error (may have status)
    Http,
    /// Not an HTTP error
    Other,
}

/// Classified error result.
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub kind: AxiosErrorKind,
    pub status: Option<u16>,
    pub message: String,
}

/// Classify a reqwest error into one of a few buckets.
pub fn classify_reqwest_error(e: &reqwest::Error) -> ClassifiedError {
    let message = e.to_string();

    if let Some(status) = e.status() {
        let code = status.as_u16();
        if code == 401 || code == 403 {
            return ClassifiedError {
                kind: AxiosErrorKind::Auth,
                status: Some(code),
                message,
            };
        }
        return ClassifiedError {
            kind: AxiosErrorKind::Http,
            status: Some(code),
            message,
        };
    }

    if e.is_timeout() {
        return ClassifiedError {
            kind: AxiosErrorKind::Timeout,
            status: None,
            message,
        };
    }

    if e.is_connect() {
        return ClassifiedError {
            kind: AxiosErrorKind::Network,
            status: None,
            message,
        };
    }

    ClassifiedError {
        kind: AxiosErrorKind::Other,
        status: None,
        message,
    }
}

/// 对应 TS `toError`：把任意 Display 类型归一化为 `anyhow::Error`。
pub fn to_error<E: std::fmt::Display>(value: E) -> anyhow::Error {
    anyhow::anyhow!("{}", value)
}

/// 对应 TS `classifyAxiosError`：把 reqwest 错误粗分类。
pub fn classify_axios_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "network";
    }
    if let Some(status) = error.status() {
        if status.is_server_error() {
            return "server";
        }
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return "auth";
        }
    }
    "unknown"
}

/// 对应 TS `TelemetrySafeError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS`：
/// 已经过 PII 审计、可安全送 telemetry 的错误类型。
#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub struct TelemetrySafeError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {
    pub message: String,
    pub kind: String,
}

impl TelemetrySafeError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {
    pub fn new(message: impl Into<String>, kind: impl Into<String>) -> Self {
        Self { message: message.into(), kind: kind.into() }
    }
}

impl fmt::Display for TelemetrySafeError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for TelemetrySafeError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {}
