//! Beta header management for API requests.
//!
//! Translated from utils/betas.ts

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;

// Beta header constants (matching constants/betas.ts)
const MOSSEN_CODE_20250219_BETA_HEADER: &str = "mossen-code-2025-02-19";
const CLI_INTERNAL_BETA_HEADER: Option<&str> = Some("cli-internal-2025-04-01");
const CONTEXT_1M_BETA_HEADER: &str = "context-1m-2025-01-01";
const CONTEXT_MANAGEMENT_BETA_HEADER: &str = "context-management-2025-05-14";
const INTERLEAVED_THINKING_BETA_HEADER: &str = "interleaved-thinking-2025-05-14";
const PROMPT_CACHING_SCOPE_BETA_HEADER: &str = "prompt-caching-scope-2025-06-01";
const REDACT_THINKING_BETA_HEADER: &str = "redact-thinking-2025-05-14";
const STRUCTURED_OUTPUTS_BETA_HEADER: &str = "structured-outputs-2025-05-01";
const SUMMARIZE_CONNECTOR_TEXT_BETA_HEADER: Option<&str> =
    Some("summarize-connector-text-2025-06-01");
const TOKEN_EFFICIENT_TOOLS_BETA_HEADER: &str = "token-efficient-tools-2026-03-28";
const TOOL_SEARCH_BETA_HEADER_1P: &str = "advanced-tool-use-2025-11-20";
const TOOL_SEARCH_BETA_HEADER_3P: &str = "tool-search-tool-2025-10-19";
const WEB_SEARCH_BETA_HEADER: &str = "web-search-2025-03-05";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-01";

/// SDK-provided betas that are allowed for API key users.
static ALLOWED_SDK_BETAS: &[&str] = &[CONTEXT_1M_BETA_HEADER];

/// Bedrock extra params headers that are routed differently.
static BEDROCK_EXTRA_PARAMS_HEADERS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(INTERLEAVED_THINKING_BETA_HEADER);
    s.insert(CONTEXT_MANAGEMENT_BETA_HEADER);
    s
});

/// Partition result for beta allowlist check.
pub struct PartitionResult {
    pub allowed: Vec<String>,
    pub disallowed: Vec<String>,
}

/// Filter betas to only include those in the allowlist.
/// Returns allowed and disallowed betas separately.
fn partition_betas_by_allowlist(betas: &[String]) -> PartitionResult {
    let mut allowed = Vec::new();
    let mut disallowed = Vec::new();
    for beta in betas {
        if ALLOWED_SDK_BETAS.contains(&beta.as_str()) {
            allowed.push(beta.clone());
        } else {
            disallowed.push(beta.clone());
        }
    }
    PartitionResult { allowed, disallowed }
}

/// Filter SDK betas to only include allowed ones.
/// Warns about disallowed betas and subscriber restrictions.
/// Returns None if no valid betas remain or if user is a subscriber.
pub fn filter_allowed_sdk_betas(
    sdk_betas: Option<&[String]>,
    is_hosted_subscriber: bool,
) -> Option<Vec<String>> {
    let betas = match sdk_betas {
        Some(b) if !b.is_empty() => b,
        _ => return None,
    };

    if is_hosted_subscriber {
        eprintln!("Warning: Custom betas are only available for API key users. Ignoring provided betas.");
        return None;
    }

    let PartitionResult { allowed, disallowed } = partition_betas_by_allowlist(betas);
    for beta in &disallowed {
        eprintln!(
            "Warning: Beta header '{}' is not allowed. Only the following betas are supported: {}",
            beta,
            ALLOWED_SDK_BETAS.join(", ")
        );
    }
    if allowed.is_empty() {
        None
    } else {
        Some(allowed)
    }
}

/// Check if a model supports interleaved structured predictions (ISP).
pub fn model_supports_isp(model: &str, provider: &str) -> bool {
    let canonical = get_canonical_name(model);
    if provider == "foundry" {
        return true;
    }
    if provider == "firstParty" {
        return !canonical.contains("mossen-3-");
    }
    canonical.contains("mossen-opus-4") || canonical.contains("mossen-sonnet-4")
}

/// Check if a Vertex model supports web search.
fn vertex_model_supports_web_search(model: &str) -> bool {
    let canonical = get_canonical_name(model);
    canonical.contains("mossen-opus-4")
        || canonical.contains("mossen-sonnet-4")
        || canonical.contains("mossen-haiku-4")
}

/// Check if a model supports context management.
pub fn model_supports_context_management(model: &str, provider: &str) -> bool {
    let canonical = get_canonical_name(model);
    if provider == "foundry" {
        return true;
    }
    if provider == "firstParty" {
        return !canonical.contains("mossen-3-");
    }
    canonical.contains("mossen-opus-4")
        || canonical.contains("mossen-sonnet-4")
        || canonical.contains("mossen-haiku-4")
}

/// Check if a model supports structured outputs.
pub fn model_supports_structured_outputs(model: &str, provider: &str) -> bool {
    let canonical = get_canonical_name(model);
    if provider != "firstParty" && provider != "foundry" {
        return false;
    }
    canonical.contains("mossen-sonnet-4-6")
        || canonical.contains("mossen-sonnet-4-5")
        || canonical.contains("mossen-opus-4-1")
        || canonical.contains("mossen-opus-4-5")
        || canonical.contains("mossen-opus-4-6")
        || canonical.contains("mossen-haiku-4-5")
}

/// Check if a model supports auto mode.
pub fn model_supports_auto_mode(model: &str, provider: &str, user_type: Option<&str>) -> bool {
    let m = get_canonical_name(model);

    // External: firstParty-only at launch
    if user_type != Some("ant") && provider != "firstParty" {
        return false;
    }

    if user_type == Some("ant") {
        // Denylist: block known-unsupported Mossen models
        if m.contains("mossen-3-") {
            return false;
        }
        let re = regex::Regex::new(r"mossen-(opus|sonnet|haiku)-4(?!-[6-9])").unwrap();
        if re.is_match(&m) {
            return false;
        }
        return true;
    }

    // External allowlist
    let external_re = regex::Regex::new(r"^mossen-(opus|sonnet)-4-6").unwrap();
    external_re.is_match(&m)
}

/// Get the correct tool search beta header for the current API provider.
pub fn get_tool_search_beta_header(provider: &str) -> &'static str {
    if provider == "vertex" || provider == "bedrock" {
        TOOL_SEARCH_BETA_HEADER_3P
    } else {
        TOOL_SEARCH_BETA_HEADER_1P
    }
}

/// Check if experimental betas should be included.
pub fn should_include_first_party_only_betas(provider: &str) -> bool {
    (provider == "firstParty" || provider == "foundry")
        && std::env::var("MOSSEN_CODE_DISABLE_EXPERIMENTAL_BETAS")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true)
}

/// Global-scope prompt caching is firstParty only.
pub fn should_use_global_cache_scope(provider: &str) -> bool {
    provider == "firstParty"
        && std::env::var("MOSSEN_CODE_DISABLE_EXPERIMENTAL_BETAS")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true)
}

/// Get all model beta headers for a given model.
pub fn get_all_model_betas(
    model: &str,
    provider: &str,
    user_type: Option<&str>,
    is_hosted_subscriber: bool,
    has_1m_context: bool,
    is_non_interactive: bool,
    show_thinking_summaries: bool,
    entrypoint: Option<&str>,
) -> Vec<String> {
    let mut beta_headers: Vec<String> = Vec::new();
    let canonical = get_canonical_name(model);
    let is_haiku = canonical.contains("haiku");
    let include_first_party_only = should_include_first_party_only_betas(provider);

    if !is_haiku {
        beta_headers.push(MOSSEN_CODE_20250219_BETA_HEADER.to_string());
        if user_type == Some("ant") && entrypoint == Some("cli") {
            if let Some(header) = CLI_INTERNAL_BETA_HEADER {
                beta_headers.push(header.to_string());
            }
        }
    }

    if is_hosted_subscriber {
        beta_headers.push(OAUTH_BETA_HEADER.to_string());
    }

    if has_1m_context {
        beta_headers.push(CONTEXT_1M_BETA_HEADER.to_string());
    }

    let disable_isp = std::env::var("DISABLE_INTERLEAVED_THINKING")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if !disable_isp && model_supports_isp(model, provider) {
        beta_headers.push(INTERLEAVED_THINKING_BETA_HEADER.to_string());
    }

    // Redact thinking for interactive sessions
    if include_first_party_only
        && model_supports_isp(model, provider)
        && !is_non_interactive
        && !show_thinking_summaries
    {
        beta_headers.push(REDACT_THINKING_BETA_HEADER.to_string());
    }

    // Summarize connector text (ant-only)
    if let Some(header) = SUMMARIZE_CONNECTOR_TEXT_BETA_HEADER {
        if user_type == Some("ant") && include_first_party_only {
            let force_off = std::env::var("USE_CONNECTOR_TEXT_SUMMARIZATION")
                .map(|v| v == "0" || v.to_lowercase() == "false")
                .unwrap_or(false);
            let force_on = std::env::var("USE_CONNECTOR_TEXT_SUMMARIZATION")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);
            if !force_off && force_on {
                beta_headers.push(header.to_string());
            }
        }
    }

    // Context management beta
    let ant_opted_into_tool_clearing = std::env::var("USE_API_CONTEXT_MANAGEMENT")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
        && user_type == Some("ant");

    let thinking_preservation_enabled = model_supports_context_management(model, provider);

    if should_include_first_party_only_betas(provider)
        && (ant_opted_into_tool_clearing || thinking_preservation_enabled)
    {
        beta_headers.push(CONTEXT_MANAGEMENT_BETA_HEADER.to_string());
    }

    // Web search beta for Vertex
    if provider == "vertex" && vertex_model_supports_web_search(model) {
        beta_headers.push(WEB_SEARCH_BETA_HEADER.to_string());
    }
    if provider == "foundry" {
        beta_headers.push(WEB_SEARCH_BETA_HEADER.to_string());
    }

    // Prompt caching scope
    if include_first_party_only {
        beta_headers.push(PROMPT_CACHING_SCOPE_BETA_HEADER.to_string());
    }

    // Custom user betas from environment
    if let Ok(custom_betas) = std::env::var("MOSSEN_CODE_BETAS") {
        for beta in custom_betas.split(',') {
            let trimmed = beta.trim();
            if !trimmed.is_empty() {
                beta_headers.push(trimmed.to_string());
            }
        }
    }

    beta_headers
}

/// Get model betas filtered for the current provider.
pub fn get_model_betas(
    model: &str,
    provider: &str,
    user_type: Option<&str>,
    is_hosted_subscriber: bool,
    has_1m_context: bool,
    is_non_interactive: bool,
    show_thinking_summaries: bool,
    entrypoint: Option<&str>,
) -> Vec<String> {
    let all = get_all_model_betas(
        model,
        provider,
        user_type,
        is_hosted_subscriber,
        has_1m_context,
        is_non_interactive,
        show_thinking_summaries,
        entrypoint,
    );
    if provider == "bedrock" {
        all.into_iter()
            .filter(|b| !BEDROCK_EXTRA_PARAMS_HEADERS.contains(b.as_str()))
            .collect()
    } else {
        all
    }
}

/// Get Bedrock extra body params betas.
pub fn get_bedrock_extra_body_params_betas(
    model: &str,
    provider: &str,
    user_type: Option<&str>,
    is_hosted_subscriber: bool,
    has_1m_context: bool,
    is_non_interactive: bool,
    show_thinking_summaries: bool,
    entrypoint: Option<&str>,
) -> Vec<String> {
    let all = get_all_model_betas(
        model,
        provider,
        user_type,
        is_hosted_subscriber,
        has_1m_context,
        is_non_interactive,
        show_thinking_summaries,
        entrypoint,
    );
    all.into_iter()
        .filter(|b| BEDROCK_EXTRA_PARAMS_HEADERS.contains(b.as_str()))
        .collect()
}

/// Merge SDK-provided betas with auto-detected model betas.
pub fn get_merged_betas(
    model_betas: &[String],
    sdk_betas: Option<&[String]>,
    is_agentic_query: bool,
    user_type: Option<&str>,
    entrypoint: Option<&str>,
) -> Vec<String> {
    let mut base = model_betas.to_vec();

    // Agentic queries always need mossen-code and cli-internal beta headers.
    if is_agentic_query {
        if !base.iter().any(|b| b == MOSSEN_CODE_20250219_BETA_HEADER) {
            base.push(MOSSEN_CODE_20250219_BETA_HEADER.to_string());
        }
        if user_type == Some("ant") && entrypoint == Some("cli") {
            if let Some(header) = CLI_INTERNAL_BETA_HEADER {
                if !base.iter().any(|b| b == header) {
                    base.push(header.to_string());
                }
            }
        }
    }

    match sdk_betas {
        Some(betas) if !betas.is_empty() => {
            for b in betas {
                if !base.contains(b) {
                    base.push(b.clone());
                }
            }
            base
        }
        _ => base,
    }
}

/// Clear all cached betas (no-op in Rust as we don't use memoization cache).
pub fn clear_betas_caches() {
    // In the TS version, this clears lodash memoize caches.
    // In Rust, we compute betas fresh each time, so this is a no-op.
}

// Helper: get canonical model name (simplified)
fn get_canonical_name(model: &str) -> String {
    model.to_lowercase()
}
