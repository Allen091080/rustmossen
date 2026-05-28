//! Managed environment constants — provider-managed env vars and safe env var allowlists.
//!
//! When MOSSEN_CODE_PROVIDER_MANAGED_BY_HOST is truthy in the spawn env, these
//! are stripped from settings-sourced env so the host's routing config isn't
//! overridden by a user's settings.

use std::collections::HashSet;

use once_cell::sync::Lazy;

/// Provider-managed environment variables that control inference routing.
static PROVIDER_MANAGED_ENV_VARS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("MOSSEN_CODE_PROVIDER_MANAGED_BY_HOST");
    s.insert("MOSSEN_CODE_CUSTOM_MAX_INPUT_TOKENS");
    s.insert("MOSSEN_CODE_USE_BEDROCK");
    s.insert("MOSSEN_CODE_USE_VERTEX");
    s.insert("MOSSEN_CODE_USE_FOUNDRY");
    s.insert("MOSSEN_CODE_API_BASE_URL");
    s.insert("MOSSEN_CODE_BEDROCK_BASE_URL");
    s.insert("MOSSEN_CODE_VERTEX_BASE_URL");
    s.insert("MOSSEN_CODE_FOUNDRY_BASE_URL");
    s.insert("MOSSEN_CODE_FOUNDRY_RESOURCE");
    s.insert("MOSSEN_CODE_VERTEX_PROJECT_ID");
    s.insert("CLOUD_ML_REGION");
    s.insert("MOSSEN_CODE_API_KEY");
    s.insert("MOSSEN_CODE_AUTH_TOKEN");
    s.insert("AWS_BEARER_TOKEN_BEDROCK");
    s.insert("MOSSEN_CODE_FOUNDRY_API_KEY");
    s.insert("MOSSEN_CODE_SKIP_BEDROCK_AUTH");
    s.insert("MOSSEN_CODE_SKIP_VERTEX_AUTH");
    s.insert("MOSSEN_CODE_SKIP_FOUNDRY_AUTH");
    s.insert("MOSSEN_CODE_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_SMALL_FAST_MODEL");
    s.insert("MOSSEN_CODE_SMALL_FAST_MODEL_AWS_REGION");
    s.insert("MOSSEN_CODE_SUBAGENT_MODEL");
    s
});

/// Prefixes for provider-managed env vars (prefix-matched).
const PROVIDER_MANAGED_ENV_PREFIXES: &[&str] = &["VERTEX_REGION_MOSSEN_"];

/// Check if a key is a provider-managed environment variable.
pub fn is_provider_managed_env_var(key: &str) -> bool {
    let upper = key.to_uppercase();
    PROVIDER_MANAGED_ENV_VARS.contains(upper.as_str())
        || PROVIDER_MANAGED_ENV_PREFIXES
            .iter()
            .any(|p| upper.starts_with(p))
}

/// Dangerous shell settings that can execute arbitrary shell code.
pub const DANGEROUS_SHELL_SETTINGS: &[&str] = &[
    "apiKeyHelper",
    "awsAuthRefresh",
    "awsCredentialExport",
    "gcpAuthRefresh",
    "customHeadersHelper",
    "statusLine",
];

/// Safe environment variables that can be applied before trust dialog.
/// These are Mossen-specific settings that don't pose security risks.
static SAFE_ENV_VARS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("MOSSEN_CODE_CUSTOM_HEADERS");
    s.insert("MOSSEN_CODE_CUSTOM_MODEL_OPTION");
    s.insert("MOSSEN_CODE_CUSTOM_MODEL_OPTION_DESCRIPTION");
    s.insert("MOSSEN_CODE_CUSTOM_MODEL_OPTION_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_FAST_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_MAX_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_DESCRIPTION");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_NAME");
    s.insert("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_SUPPORTED_CAPABILITIES");
    s.insert("MOSSEN_CODE_FOUNDRY_API_KEY");
    s.insert("MOSSEN_CODE_MODEL");
    s.insert("MOSSEN_CODE_SMALL_FAST_MODEL_AWS_REGION");
    s.insert("MOSSEN_CODE_SMALL_FAST_MODEL");
    s.insert("AWS_DEFAULT_REGION");
    s.insert("AWS_PROFILE");
    s.insert("AWS_REGION");
    s.insert("BASH_DEFAULT_TIMEOUT_MS");
    s.insert("BASH_MAX_OUTPUT_LENGTH");
    s.insert("BASH_MAX_TIMEOUT_MS");
    s.insert("MOSSEN_BASH_MAINTAIN_PROJECT_WORKING_DIR");
    s.insert("MOSSEN_CODE_API_KEY_HELPER_TTL_MS");
    s.insert("MOSSEN_CODE_CUSTOM_MAX_INPUT_TOKENS");
    s.insert("MOSSEN_CODE_DISABLE_EXPERIMENTAL_BETAS");
    s.insert("MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC");
    s.insert("MOSSEN_CODE_DISABLE_TERMINAL_TITLE");
    s.insert("MOSSEN_CODE_ENABLE_TELEMETRY");
    s.insert("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS");
    s.insert("MOSSEN_CODE_IDE_SKIP_AUTO_INSTALL");
    s.insert("MOSSEN_CODE_MAX_OUTPUT_TOKENS");
    s.insert("MOSSEN_CODE_SKIP_BEDROCK_AUTH");
    s.insert("MOSSEN_CODE_SKIP_FOUNDRY_AUTH");
    s.insert("MOSSEN_CODE_SKIP_VERTEX_AUTH");
    s.insert("MOSSEN_CODE_SUBAGENT_MODEL");
    s.insert("MOSSEN_CODE_USE_BEDROCK");
    s.insert("MOSSEN_CODE_USE_FOUNDRY");
    s.insert("MOSSEN_CODE_USE_VERTEX");
    s.insert("DISABLE_AUTOUPDATER");
    s.insert("DISABLE_BUG_COMMAND");
    s.insert("DISABLE_COST_WARNINGS");
    s.insert("DISABLE_ERROR_REPORTING");
    s.insert("DISABLE_FEEDBACK_COMMAND");
    s.insert("DISABLE_TELEMETRY");
    s.insert("ENABLE_TOOL_SEARCH");
    s.insert("MAX_MCP_OUTPUT_TOKENS");
    s.insert("MAX_THINKING_TOKENS");
    s.insert("MCP_TIMEOUT");
    s.insert("MCP_TOOL_TIMEOUT");
    s.insert("OTEL_EXPORTER_OTLP_HEADERS");
    s.insert("OTEL_EXPORTER_OTLP_LOGS_HEADERS");
    s.insert("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL");
    s.insert("OTEL_EXPORTER_OTLP_METRICS_CLIENT_CERTIFICATE");
    s.insert("OTEL_EXPORTER_OTLP_METRICS_CLIENT_KEY");
    s.insert("OTEL_EXPORTER_OTLP_METRICS_HEADERS");
    s.insert("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL");
    s.insert("OTEL_EXPORTER_OTLP_PROTOCOL");
    s.insert("OTEL_EXPORTER_OTLP_TRACES_HEADERS");
    s.insert("OTEL_LOG_TOOL_DETAILS");
    s.insert("OTEL_LOG_USER_PROMPTS");
    s.insert("OTEL_LOGS_EXPORT_INTERVAL");
    s.insert("OTEL_LOGS_EXPORTER");
    s.insert("OTEL_METRIC_EXPORT_INTERVAL");
    s.insert("OTEL_METRICS_EXPORTER");
    s.insert("OTEL_METRICS_INCLUDE_ACCOUNT_UUID");
    s.insert("OTEL_METRICS_INCLUDE_SESSION_ID");
    s.insert("OTEL_METRICS_INCLUDE_VERSION");
    s.insert("OTEL_RESOURCE_ATTRIBUTES");
    s.insert("USE_BUILTIN_RIPGREP");
    s.insert("VERTEX_REGION_MOSSEN_3_5_FAST");
    s.insert("VERTEX_REGION_MOSSEN_3_5_BALANCED");
    s.insert("VERTEX_REGION_MOSSEN_3_7_BALANCED");
    s.insert("VERTEX_REGION_MOSSEN_4_0_MAX");
    s.insert("VERTEX_REGION_MOSSEN_4_0_BALANCED");
    s.insert("VERTEX_REGION_MOSSEN_4_1_MAX");
    s.insert("VERTEX_REGION_MOSSEN_4_5_BALANCED");
    s.insert("VERTEX_REGION_MOSSEN_4_6_BALANCED");
    s.insert("VERTEX_REGION_MOSSEN_FAST_4_5");
    s
});

/// Check if an env var is in the safe list.
pub fn is_safe_env_var(key: &str) -> bool {
    SAFE_ENV_VARS.contains(key)
}

/// Check if an env var is dangerous (not in safe list and not empty).
pub fn is_dangerous_env_var(key: &str) -> bool {
    !SAFE_ENV_VARS.contains(key)
}
