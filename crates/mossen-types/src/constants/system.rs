//! # System (system.ts)
//!
//! 关键系统常量，用于打破循环依赖。

use std::collections::HashSet;

use once_cell::sync::Lazy;

pub const DEFAULT_PREFIX: &str =
    "You are Mossen, a software engineering assistant running in the Mossen CLI.";
pub const MOSSEN_AGENT_SDK_PRESET_PREFIX: &str =
    "You are Mossen, a software engineering assistant running within the Mossen Agent SDK.";
pub const MOSSEN_AGENT_SDK_PREFIX: &str = "You are a Mossen agent, built on the Mossen Agent SDK.";

/// Custom backend default prefix template.
/// In TS: ``You are ${getProductAssistantName()}, a software engineering assistant running in a local ${getProductDisplayName()} environment.``
pub fn custom_default_prefix(assistant_name: &str, product_name: &str) -> String {
    format!(
        "You are {}, a software engineering assistant running in a local {} environment.",
        assistant_name, product_name
    )
}

/// Custom backend agent SDK preset prefix template.
pub fn custom_agent_sdk_preset_prefix(assistant_name: &str, product_name: &str) -> String {
    format!(
        "You are {} running inside a local {} agent runtime.",
        assistant_name, product_name
    )
}

/// Custom backend agent SDK prefix template.
pub fn custom_agent_sdk_prefix(product_name: &str) -> String {
    format!(
        "You are a software engineering agent operating through {}'s custom model backend.",
        product_name
    )
}

/// All possible CLI sysprompt prefix values (static ones).
/// Used by splitSysPromptPrefix to identify prefix blocks by content.
pub static CLI_SYSPROMPT_PREFIXES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(DEFAULT_PREFIX);
    s.insert(MOSSEN_AGENT_SDK_PRESET_PREFIX);
    s.insert(MOSSEN_AGENT_SDK_PREFIX);
    // Note: custom backend prefixes are dynamic and checked separately at runtime
    s
});

/// Get the CLI sysprompt prefix based on context.
/// `is_custom_backend`: whether custom backend is enabled
/// `is_non_interactive`: whether the session is non-interactive
/// `has_append_system_prompt`: whether there's an appended system prompt
/// `api_provider`: the API provider string (e.g., "vertex")
/// `assistant_name`: product assistant name for custom backend
/// `product_name`: product display name for custom backend
pub fn get_cli_sysprompt_prefix(
    is_custom_backend: bool,
    is_non_interactive: bool,
    has_append_system_prompt: bool,
    api_provider: &str,
    assistant_name: &str,
    product_name: &str,
) -> String {
    if is_custom_backend {
        if is_non_interactive {
            if has_append_system_prompt {
                return custom_agent_sdk_preset_prefix(assistant_name, product_name);
            }
            return custom_agent_sdk_prefix(product_name);
        }
        return custom_default_prefix(assistant_name, product_name);
    }

    if api_provider == "vertex" {
        return DEFAULT_PREFIX.to_string();
    }

    if is_non_interactive {
        if has_append_system_prompt {
            return MOSSEN_AGENT_SDK_PRESET_PREFIX.to_string();
        }
        return MOSSEN_AGENT_SDK_PREFIX.to_string();
    }
    DEFAULT_PREFIX.to_string()
}

/// Check if attribution header is enabled.
/// `env_falsy`: whether MOSSEN_CODE_ATTRIBUTION_HEADER env var is defined and falsy.
/// `growthbook_value`: the GrowthBook `mossen_attribution_header` value (default true).
pub fn is_attribution_header_enabled(env_falsy: bool, growthbook_value: bool) -> bool {
    if env_falsy {
        return false;
    }
    growthbook_value
}

/// Get attribution header for API requests.
/// Returns a header string with cc_version (including fingerprint) and cc_entrypoint.
///
/// When native client attestation is enabled, includes a `cch=00000` placeholder.
/// Before the request is sent, Bun's native HTTP stack finds this placeholder
/// in the request body and overwrites the zeros with a computed hash.
pub fn get_attribution_header(
    is_custom_backend: bool,
    is_enabled: bool,
    version: &str,
    fingerprint: &str,
    entrypoint: &str,
    native_attestation: bool,
    workload: Option<&str>,
) -> String {
    if is_custom_backend {
        return String::new();
    }
    if !is_enabled {
        return String::new();
    }

    let ver = format!("{}.{}", version, fingerprint);
    let cch = if native_attestation {
        " cch=00000;"
    } else {
        ""
    };
    let workload_pair = match workload {
        Some(w) => format!(" cc_workload={};", w),
        None => String::new(),
    };
    format!(
        "x-mossen-billing-header: cc_version={}; cc_entrypoint={};{}{}",
        ver, entrypoint, cch, workload_pair
    )
}
