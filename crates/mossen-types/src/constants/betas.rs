//! # Beta Headers (betas.ts)
//!
//! Beta feature header 常量。

use std::collections::HashSet;

use once_cell::sync::Lazy;

pub const MOSSEN_CODE_20250219_BETA_HEADER: &str = "mossen-code-20250219";
pub const INTERLEAVED_THINKING_BETA_HEADER: &str = "interleaved-thinking-2025-05-14";
pub const CONTEXT_1M_BETA_HEADER: &str = "context-1m-2025-08-07";
pub const CONTEXT_MANAGEMENT_BETA_HEADER: &str = "context-management-2025-06-27";
pub const STRUCTURED_OUTPUTS_BETA_HEADER: &str = "structured-outputs-2025-12-15";
pub const WEB_SEARCH_BETA_HEADER: &str = "web-search-2025-03-05";
/// Tool search beta headers differ by provider:
/// - Mossen API / Foundry: advanced-tool-use-2025-11-20
/// - Vertex AI / Bedrock: tool-search-tool-2025-10-19
pub const TOOL_SEARCH_BETA_HEADER_1P: &str = "advanced-tool-use-2025-11-20";
pub const TOOL_SEARCH_BETA_HEADER_3P: &str = "tool-search-tool-2025-10-19";
pub const EFFORT_BETA_HEADER: &str = "effort-2025-11-24";
pub const TASK_BUDGETS_BETA_HEADER: &str = "task-budgets-2026-03-13";
pub const PROMPT_CACHING_SCOPE_BETA_HEADER: &str = "prompt-caching-scope-2026-01-05";
pub const FAST_MODE_BETA_HEADER: &str = "fast-mode-2026-02-01";
pub const REDACT_THINKING_BETA_HEADER: &str = "redact-thinking-2026-02-12";
pub const TOKEN_EFFICIENT_TOOLS_BETA_HEADER: &str = "token-efficient-tools-2026-03-28";

/// Feature-gated: `feature('CONNECTOR_TEXT')` in TS.
/// Empty string when feature is disabled.
pub const SUMMARIZE_CONNECTOR_TEXT_BETA_HEADER: &str = "summarize-connector-text-2026-03-13";

/// Feature-gated: `feature('TRANSCRIPT_CLASSIFIER')` in TS.
/// Empty string when feature is disabled.
pub const AFK_MODE_BETA_HEADER: &str = "afk-mode-2026-01-31";

/// Internal-only: `process.env.USER_TYPE === 'internal'` in TS.
/// Empty string when not internal.
pub const CLI_INTERNAL_BETA_HEADER: &str = "cli-internal-2026-02-09";

pub const ADVISOR_BETA_HEADER: &str = "advisor-tool-2026-03-01";

/// Bedrock only supports a limited number of beta headers and only through
/// extraBodyParams. This set maintains the beta strings that should be in
/// Bedrock extraBodyParams *and not* in Bedrock headers.
pub static BEDROCK_EXTRA_PARAMS_HEADERS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(INTERLEAVED_THINKING_BETA_HEADER);
    s.insert(CONTEXT_1M_BETA_HEADER);
    s.insert(TOOL_SEARCH_BETA_HEADER_3P);
    s
});

/// Betas allowed on Vertex countTokens API.
/// Other betas will cause 400 errors.
pub static VERTEX_COUNT_TOKENS_ALLOWED_BETAS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(MOSSEN_CODE_20250219_BETA_HEADER);
    s.insert(INTERLEAVED_THINKING_BETA_HEADER);
    s.insert(CONTEXT_MANAGEMENT_BETA_HEADER);
    s
});
