//! # API Error Handling
//!
//! 翻译自 `services/api/errors.ts` (1240行)
//! 提供 API 错误到用户消息的转换、错误分类、重试分类。

use regex::Regex;
use std::sync::LazyLock;
use super::error_utils::{extract_connection_error_details, format_api_error};
use super::sdk::MossenAPIError;

pub const API_ERROR_MESSAGE_PREFIX: &str = "API Error";
pub const PROMPT_TOO_LONG_ERROR_MESSAGE: &str = "Prompt is too long";
pub const CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE: &str = "Credit balance is too low";
pub const INVALID_API_KEY_ERROR_MESSAGE: &str =
    "Authentication missing · Configure Mossen backend credentials";
pub const INVALID_API_KEY_ERROR_MESSAGE_EXTERNAL: &str =
    "Invalid API key · Fix external API key";
pub const ORG_DISABLED_ERROR_MESSAGE_ENV_KEY_WITH_OAUTH: &str =
    "The configured provider API key belongs to a disabled organization · Unset it to use Mossen-managed backend credentials instead";
pub const ORG_DISABLED_ERROR_MESSAGE_ENV_KEY: &str =
    "The configured provider API key belongs to a disabled organization · Update or unset it";
pub const TOKEN_REVOKED_ERROR_MESSAGE: &str =
    "Hosted adapter token revoked · Refresh external hosted credentials";
pub const CCR_AUTH_ERROR_MESSAGE: &str =
    "Authentication error · This may be a temporary network issue, please try again";
pub const REPEATED_529_ERROR_MESSAGE: &str = "Repeated 529 Overloaded errors";
pub const CUSTOM_OFF_SWITCH_MESSAGE: &str =
    "Opus is experiencing high load, please use /model to switch to Sonnet";
pub const API_TIMEOUT_ERROR_MESSAGE: &str = "Request timed out";

pub fn starts_with_api_error_prefix(text: &str) -> bool {
    text.starts_with(API_ERROR_MESSAGE_PREFIX)
}

/// Represents an assistant message created from an error.
#[derive(Debug, Clone)]
pub struct AssistantErrorMessage {
    pub content: String,
    pub error_type: Option<String>,
    pub error_details: Option<String>,
    pub is_api_error_message: bool,
}

impl AssistantErrorMessage {
    pub fn new(content: &str, error: Option<&str>, error_details: Option<&str>) -> Self {
        Self {
            content: content.to_string(),
            error_type: error.map(|s| s.to_string()),
            error_details: error_details.map(|s| s.to_string()),
            is_api_error_message: true,
        }
    }
}

/// Check if a message content starts with the prompt too long message.
pub fn is_prompt_too_long_content(content: &str) -> bool {
    content.starts_with(PROMPT_TOO_LONG_ERROR_MESSAGE)
}

/// Parse actual/limit token counts from a raw prompt-too-long API error message.
pub fn parse_prompt_too_long_token_counts(raw_message: &str) -> (Option<u64>, Option<u64>) {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)prompt is too long[^0-9]*(\d+)\s*tokens?\s*>\s*(\d+)").unwrap()
    });
    if let Some(caps) = RE.captures(raw_message) {
        let actual = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let limit = caps.get(2).and_then(|m| m.as_str().parse().ok());
        (actual, limit)
    } else {
        (None, None)
    }
}

/// Returns how many tokens over the limit a prompt-too-long error reports.
pub fn get_prompt_too_long_token_gap(error_details: Option<&str>) -> Option<u64> {
    let details = error_details?;
    let (actual, limit) = parse_prompt_too_long_token_counts(details);
    match (actual, limit) {
        (Some(a), Some(l)) if a > l => Some(a - l),
        _ => None,
    }
}

/// Is this raw API error text a media-size rejection?
pub fn is_media_size_error(raw: &str) -> bool {
    static PDF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"maximum of \d+ PDF pages").unwrap());
    (raw.contains("image exceeds") && raw.contains("maximum"))
        || (raw.contains("image dimensions exceed") && raw.contains("many-image"))
        || PDF_RE.is_match(raw)
}

/// Type guard to check if a value is a valid API Message response.
pub fn is_valid_api_message(value: &serde_json::Value) -> bool {
    if let serde_json::Value::Object(map) = value {
        map.contains_key("content")
            && map.contains_key("model")
            && map.contains_key("usage")
            && map.get("content").map_or(false, |v| v.is_array())
            && map.get("model").map_or(false, |v| v.is_string())
            && map.get("usage").map_or(false, |v| v.is_object())
    } else {
        false
    }
}

/// Given a response that doesn't look right, see if it contains any known error types.
pub fn extract_unknown_error_format(value: &serde_json::Value) -> Option<String> {
    // Amazon Bedrock routing errors
    if let Some(output) = value.get("Output") {
        if let Some(type_val) = output.get("__type") {
            return type_val.as_str().map(|s| s.to_string());
        }
    }
    None
}

/// Configuration context for error message generation.
#[derive(Debug, Clone, Default)]
pub struct ErrorMessageContext {
    pub is_non_interactive: bool,
    pub is_custom_backend: bool,
    pub custom_backend_name: Option<String>,
    pub is_hosted_subscriber: bool,
    pub is_ccr_mode: bool,
    pub api_pdf_max_pages: u32,
    pub pdf_target_raw_size: u64,
    pub model: String,
}

/// Get assistant message from error, using the provided context for environment decisions.
pub fn get_assistant_message_from_error(
    error: &MossenAPIError,
    model: &str,
    ctx: &ErrorMessageContext,
) -> AssistantErrorMessage {
    let message = &error.message;
    let status = error.status;

    // Check for SDK timeout errors (connection timeout)
    if message.to_lowercase().contains("timeout") && status == 0 {
        return AssistantErrorMessage::new(API_TIMEOUT_ERROR_MESSAGE, Some("unknown"), None);
    }

    // Check for emergency capacity off switch for Opus PAYG users
    if message.contains(CUSTOM_OFF_SWITCH_MESSAGE) {
        return AssistantErrorMessage::new(CUSTOM_OFF_SWITCH_MESSAGE, Some("rate_limit"), None);
    }

    // 429 rate limit handling
    if status == 429 {
        // Check for extra usage required for long context
        if message.contains("Extra usage is required for long context") {
            let hint = if ctx.is_non_interactive {
                "enable extra usage in hosted billing settings, or use --model to switch to standard context"
            } else {
                "run /extra-usage to enable, or /model to switch to standard context"
            };
            return AssistantErrorMessage::new(
                &format!("{}: Extra usage is required for 1M context · {}", API_ERROR_MESSAGE_PREFIX, hint),
                Some("rate_limit"),
                None,
            );
        }
        // SDK's MossenAPIError.makeMessage prepends "429 " and JSON-stringifies the body
        let stripped = message.strip_prefix("429 ").unwrap_or(message);
        let inner_message = extract_inner_message(stripped);
        let detail = if !inner_message.is_empty() {
            inner_message
        } else {
            stripped.to_string()
        };
        return AssistantErrorMessage::new(
            &format!(
                "{}: Request rejected (429) · {}",
                API_ERROR_MESSAGE_PREFIX,
                if detail.is_empty() {
                    "this may be a temporary capacity issue with the configured backend"
                } else {
                    &detail
                }
            ),
            Some("rate_limit"),
            None,
        );
    }

    // Handle prompt too long errors
    if message.to_lowercase().contains("prompt is too long") {
        return AssistantErrorMessage::new(
            PROMPT_TOO_LONG_ERROR_MESSAGE,
            Some("invalid_request"),
            Some(message),
        );
    }

    // Check for PDF page limit errors
    static PDF_PAGES_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"maximum of \d+ PDF pages").unwrap());
    if PDF_PAGES_RE.is_match(message) {
        let content = get_pdf_too_large_error_message(ctx);
        return AssistantErrorMessage::new(&content, Some("invalid_request"), Some(message));
    }

    // Check for password-protected PDF errors
    if message.contains("The PDF specified is password protected") {
        let content = get_pdf_password_protected_error_message(ctx);
        return AssistantErrorMessage::new(&content, Some("invalid_request"), None);
    }

    // Check for invalid PDF errors
    if message.contains("The PDF specified was not valid") {
        let content = get_pdf_invalid_error_message(ctx);
        return AssistantErrorMessage::new(&content, Some("invalid_request"), None);
    }

    // Check for image size errors
    if status == 400 && message.contains("image exceeds") && message.contains("maximum") {
        let content = get_image_too_large_error_message(ctx);
        return AssistantErrorMessage::new(&content, None, Some(message));
    }

    // Check for many-image dimension errors
    if status == 400
        && message.contains("image dimensions exceed")
        && message.contains("many-image")
    {
        let content = if ctx.is_non_interactive {
            "An image in the conversation exceeds the dimension limit for many-image requests (2000px). Start a new session with fewer images.".to_string()
        } else {
            "An image in the conversation exceeds the dimension limit for many-image requests (2000px). Run /compact to remove old images from context, or start a new session.".to_string()
        };
        return AssistantErrorMessage::new(&content, Some("invalid_request"), Some(message));
    }

    // Check for request too large errors (413 status)
    if status == 413 {
        let content = get_request_too_large_error_message(ctx);
        return AssistantErrorMessage::new(&content, Some("invalid_request"), None);
    }

    // Tool use/result concurrency error
    if status == 400
        && message.contains("`tool_use` ids were found without `tool_result` blocks immediately after")
    {
        let base_message = "API Error: 400 due to tool use concurrency issues.";
        let rewind_instruction = if ctx.is_non_interactive {
            ""
        } else {
            " Run /rewind to recover the conversation."
        };
        return AssistantErrorMessage::new(
            &format!("{}{}", base_message, rewind_instruction),
            Some("invalid_request"),
            None,
        );
    }

    // Duplicate tool_use IDs
    if status == 400 && message.contains("`tool_use` ids must be unique") {
        let rewind_instruction = if ctx.is_non_interactive {
            ""
        } else {
            " Run /rewind to recover the conversation."
        };
        return AssistantErrorMessage::new(
            &format!(
                "API Error: 400 duplicate tool_use ID in conversation history.{}",
                rewind_instruction
            ),
            Some("invalid_request"),
            Some(message),
        );
    }

    // Credit balance too low
    if message.contains("Your credit balance is too low") {
        return AssistantErrorMessage::new(
            CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE,
            Some("billing_error"),
            None,
        );
    }

    // Organization disabled
    if status == 400 && message.to_lowercase().contains("organization has been disabled") {
        return AssistantErrorMessage::new(
            ORG_DISABLED_ERROR_MESSAGE_ENV_KEY,
            Some("invalid_request"),
            None,
        );
    }

    // x-api-key errors (authentication)
    if message.to_lowercase().contains("x-api-key") {
        if ctx.is_ccr_mode {
            return AssistantErrorMessage::new(
                CCR_AUTH_ERROR_MESSAGE,
                Some("authentication_failed"),
                None,
            );
        }
        return AssistantErrorMessage::new(
            INVALID_API_KEY_ERROR_MESSAGE,
            Some("authentication_failed"),
            None,
        );
    }

    // OAuth token revocation error
    if status == 403 && message.contains("OAuth token has been revoked") {
        return AssistantErrorMessage::new(
            TOKEN_REVOKED_ERROR_MESSAGE,
            Some("authentication_failed"),
            None,
        );
    }

    // Generic 401/403 authentication errors
    if status == 401 || status == 403 {
        if ctx.is_ccr_mode {
            return AssistantErrorMessage::new(
                CCR_AUTH_ERROR_MESSAGE,
                Some("authentication_failed"),
                None,
            );
        }
        let content = format!(
            "Configure Mossen backend credentials · {}: {}",
            API_ERROR_MESSAGE_PREFIX, message
        );
        return AssistantErrorMessage::new(&content, Some("authentication_failed"), None);
    }

    // 404 Not Found
    if status == 404 {
        let switch_cmd = if ctx.is_non_interactive { "--model" } else { "/model" };
        let content = format!(
            "There's an issue with the selected model ({}). It may not exist or you may not have access to it. Run {} to pick a different model.",
            model, switch_cmd
        );
        return AssistantErrorMessage::new(&content, Some("invalid_request"), None);
    }

    // Connection errors (non-timeout) — use format_api_error for detailed messages
    if error.error_code.is_some() {
        return AssistantErrorMessage::new(
            &format!("{}: {}", API_ERROR_MESSAGE_PREFIX, format_api_error(error)),
            Some("unknown"),
            None,
        );
    }

    // Generic error fallback
    if !message.is_empty() {
        return AssistantErrorMessage::new(
            &format!("{}: {}", API_ERROR_MESSAGE_PREFIX, message),
            Some("unknown"),
            None,
        );
    }

    AssistantErrorMessage::new(API_ERROR_MESSAGE_PREFIX, Some("unknown"), None)
}

/// Classifies an API error into a specific error type for analytics tracking.
pub fn classify_api_error(error: &MossenAPIError) -> &'static str {
    let message = &error.message;
    let status = error.status;

    // Aborted requests
    if message == "Request was aborted." {
        return "aborted";
    }

    // Timeout errors
    if message.to_lowercase().contains("timeout") && status == 0 {
        return "api_timeout";
    }

    // Repeated 529 errors
    if message.contains(REPEATED_529_ERROR_MESSAGE) {
        return "repeated_529";
    }

    // Emergency capacity off switch
    if message.contains(CUSTOM_OFF_SWITCH_MESSAGE) {
        return "capacity_off_switch";
    }

    // Rate limiting
    if status == 429 {
        return "rate_limit";
    }

    // Server overload (529)
    if status == 529 || message.contains("\"type\":\"overloaded_error\"") {
        return "server_overload";
    }

    // Prompt/content size errors
    if message.to_lowercase().contains(&PROMPT_TOO_LONG_ERROR_MESSAGE.to_lowercase()) {
        return "prompt_too_long";
    }

    // PDF errors
    static PDF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"maximum of \d+ PDF pages").unwrap());
    if PDF_RE.is_match(message) {
        return "pdf_too_large";
    }

    if message.contains("The PDF specified is password protected") {
        return "pdf_password_protected";
    }

    // Image size errors
    if status == 400 && message.contains("image exceeds") && message.contains("maximum") {
        return "image_too_large";
    }

    // Many-image dimension errors
    if status == 400
        && message.contains("image dimensions exceed")
        && message.contains("many-image")
    {
        return "image_too_large";
    }

    // Tool use errors (400)
    if status == 400
        && message.contains("`tool_use` ids were found without `tool_result` blocks immediately after")
    {
        return "tool_use_mismatch";
    }

    if status == 400 && message.contains("unexpected `tool_use_id` found in `tool_result`") {
        return "unexpected_tool_result";
    }

    if status == 400 && message.contains("`tool_use` ids must be unique") {
        return "duplicate_tool_use_id";
    }

    // Invalid model errors (400)
    if status == 400 && message.to_lowercase().contains("invalid model name") {
        return "invalid_model";
    }

    // Credit/billing errors
    if message
        .to_lowercase()
        .contains(&CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE.to_lowercase())
    {
        return "credit_balance_low";
    }

    // Authentication errors
    if message.to_lowercase().contains("x-api-key") {
        return "invalid_api_key";
    }

    if status == 403 && message.contains("OAuth token has been revoked") {
        return "token_revoked";
    }

    if (status == 401 || status == 403)
        && message.contains("OAuth authentication is currently not allowed for this organization")
    {
        return "oauth_org_not_allowed";
    }

    // Generic auth errors
    if status == 401 || status == 403 {
        return "auth_error";
    }

    // Status code based fallbacks
    if status >= 500 {
        return "server_error";
    }
    if status >= 400 {
        return "client_error";
    }

    // Connection errors - check for SSL/TLS issues first
    if let Some(details) = extract_connection_error_details(error) {
        if details.is_ssl_error {
            return "ssl_cert_error";
        }
        return "connection_error";
    }

    "unknown"
}

/// SDK assistant message error type for retry categorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdkAssistantMessageError {
    RateLimit,
    AuthenticationFailed,
    ServerError,
    Unknown,
}

/// Categorize a retryable API error for the SDK.
pub fn categorize_retryable_api_error(error: &MossenAPIError) -> SdkAssistantMessageError {
    let status = error.status;
    if status == 529 || error.message.contains("\"type\":\"overloaded_error\"") {
        return SdkAssistantMessageError::RateLimit;
    }
    if status == 429 {
        return SdkAssistantMessageError::RateLimit;
    }
    if status == 401 || status == 403 {
        return SdkAssistantMessageError::AuthenticationFailed;
    }
    if status >= 408 {
        return SdkAssistantMessageError::ServerError;
    }
    SdkAssistantMessageError::Unknown
}

// --- Helper functions ---

fn extract_inner_message(text: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""message"\s*:\s*"([^"]*)""#).unwrap());
    RE.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn format_file_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}

fn get_pdf_too_large_error_message(ctx: &ErrorMessageContext) -> String {
    let limits = format!(
        "max {} pages, {}",
        ctx.api_pdf_max_pages,
        format_file_size(ctx.pdf_target_raw_size)
    );
    if ctx.is_non_interactive {
        format!("PDF too large ({}). Try reading the file a different way (e.g., extract text with pdftotext).", limits)
    } else {
        format!("PDF too large ({}). Double press esc to go back and try again, or use pdftotext to convert to text first.", limits)
    }
}

fn get_pdf_password_protected_error_message(ctx: &ErrorMessageContext) -> String {
    if ctx.is_non_interactive {
        "PDF is password protected. Try using a CLI tool to extract or convert the PDF.".to_string()
    } else {
        "PDF is password protected. Please double press esc to edit your message and try again."
            .to_string()
    }
}

fn get_pdf_invalid_error_message(ctx: &ErrorMessageContext) -> String {
    if ctx.is_non_interactive {
        "The PDF file was not valid. Try converting it to text first (e.g., pdftotext).".to_string()
    } else {
        "The PDF file was not valid. Double press esc to go back and try again with a different file.".to_string()
    }
}

fn get_image_too_large_error_message(ctx: &ErrorMessageContext) -> String {
    if ctx.is_non_interactive {
        "Image was too large. Try resizing the image or using a different approach.".to_string()
    } else {
        "Image was too large. Double press esc to go back and try again with a smaller image."
            .to_string()
    }
}

fn get_request_too_large_error_message(ctx: &ErrorMessageContext) -> String {
    let limits = format!("max {}", format_file_size(ctx.pdf_target_raw_size));
    if ctx.is_non_interactive {
        format!("Request too large ({}). Try with a smaller file.", limits)
    } else {
        format!(
            "Request too large ({}). Double press esc to go back and try with a smaller file.",
            limits
        )
    }
}

// ---------------------------------------------------------------------------
// Public TS-mirror wrappers — these forward to the lower-level helpers but
// keep names parity with `services/api/errors.ts` so external callers can
// import them by the original snake_case names.
// ---------------------------------------------------------------------------

pub const OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE: &str =
    "OAuth authentication is currently not allowed for this organization. Please contact your administrator.";

/// `services/api/errors.ts` `isPromptTooLongMessage`.
/// Accepts the raw assistant message content (the TS variant unwraps the
/// envelope; here we just inspect the prose because the equivalent
/// `AssistantMessage` type isn't shared).
pub fn is_prompt_too_long_message(msg_content: &str) -> bool {
    is_prompt_too_long_content(msg_content)
}

/// `services/api/errors.ts` `isMediaSizeErrorMessage`.
pub fn is_media_size_error_message(msg_content: &str) -> bool {
    is_media_size_error(msg_content)
}

/// `services/api/errors.ts` `getInvalidApiKeyErrorMessage`.
pub fn get_invalid_api_key_error_message(is_external: bool) -> &'static str {
    if is_external {
        INVALID_API_KEY_ERROR_MESSAGE_EXTERNAL
    } else {
        INVALID_API_KEY_ERROR_MESSAGE
    }
}

/// `services/api/errors.ts` `getAuthenticationFailedErrorMessage`.
pub fn get_authentication_failed_error_message(ctx: &ErrorMessageContext) -> String {
    if ctx.is_ccr_mode {
        return CCR_AUTH_ERROR_MESSAGE.to_string();
    }
    if ctx.is_hosted_subscriber {
        return "Hosted subscriber authentication failed · Run /login to reauthenticate".to_string();
    }
    INVALID_API_KEY_ERROR_MESSAGE.to_string()
}

/// `services/api/errors.ts` `getTokenRevokedErrorMessage`.
pub fn get_token_revoked_error_message() -> &'static str {
    TOKEN_REVOKED_ERROR_MESSAGE
}

/// `services/api/errors.ts` `getOauthOrgNotAllowedErrorMessage`.
pub fn get_oauth_org_not_allowed_error_message() -> &'static str {
    OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE
}

/// `services/api/errors.ts` `getPdfTooLargeErrorMessage`.
pub fn get_pdf_too_large_error_message_pub(ctx: &ErrorMessageContext) -> String {
    get_pdf_too_large_error_message(ctx)
}

/// `services/api/errors.ts` `getPdfPasswordProtectedErrorMessage`.
pub fn get_pdf_password_protected_error_message_pub(ctx: &ErrorMessageContext) -> String {
    get_pdf_password_protected_error_message(ctx)
}

/// `services/api/errors.ts` `getPdfInvalidErrorMessage`.
pub fn get_pdf_invalid_error_message_pub(ctx: &ErrorMessageContext) -> String {
    get_pdf_invalid_error_message(ctx)
}

/// `services/api/errors.ts` `getImageTooLargeErrorMessage`.
pub fn get_image_too_large_error_message_pub(ctx: &ErrorMessageContext) -> String {
    get_image_too_large_error_message(ctx)
}

/// `services/api/errors.ts` `getRequestTooLargeErrorMessage`.
pub fn get_request_too_large_error_message_pub(ctx: &ErrorMessageContext) -> String {
    get_request_too_large_error_message(ctx)
}

/// `services/api/errors.ts` `getErrorMessageIfRefusal`.
/// Detects model refusal responses; returns the user-facing message when the
/// response embeds a `stop_reason: refusal`.
pub fn get_error_message_if_refusal(raw: &str) -> Option<String> {
    if raw.contains("\"stop_reason\":\"refusal\"")
        || raw.contains("\"stop_reason\": \"refusal\"")
    {
        Some("The model refused this request.".to_string())
    } else {
        None
    }
}
