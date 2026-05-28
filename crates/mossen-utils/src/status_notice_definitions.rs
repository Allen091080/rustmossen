//! Status notice definitions — the registry of warnings/info banners
//! shown above the prompt input.
//!
//! Mirrors TS `utils/statusNoticeDefinitions.tsx`. The TSX file binds each
//! notice to a React render callback. The Rust port keeps the same
//! registry shape but renders the notice into plain text — the front-end
//! can decorate / colorize the text later.

use crate::status_notice_helpers::AGENT_DESCRIPTIONS_THRESHOLD;

/// Result of `loadAgentsDir` — the notice registry only reads the cumulative
/// description-tokens count, so we keep the type minimal here.
#[derive(Debug, Clone, Default)]
pub struct AgentDefinitionsResult {
    pub total_description_tokens: usize,
}

fn get_agent_descriptions_total_tokens(
    agent_definitions: Option<&AgentDefinitionsResult>,
) -> usize {
    agent_definitions
        .map(|d| d.total_description_tokens)
        .unwrap_or(0)
}

/// Notice severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusNoticeType {
    Warning,
    Info,
}

/// Memory file info (subset the notices read).
#[derive(Debug, Clone)]
pub struct MemoryFileInfo {
    pub path: String,
    pub display_path: String,
    pub content_len: u64,
}

/// Maximum allowed memory file size (mirrors TS constant). Kept here so
/// callers don't need to import from mossenmd just for this notice.
pub const MAX_MEMORY_CHARACTER_COUNT: u64 = 40_000;

/// Auth source kind (matches the TS string-literal type returned by
/// `getAuthTokenSource` / `getMossenApiKeyWithSource`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSource {
    None,
    Hosted,
    MossenCodeAuthToken,
    MossenCodeApiKey,
    ApiKeyHelper,
    Other(String),
}

impl AuthSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Hosted => "hosted",
            Self::MossenCodeAuthToken => "MOSSEN_CODE_AUTH_TOKEN",
            Self::MossenCodeApiKey => "MOSSEN_CODE_API_KEY",
            Self::ApiKeyHelper => "apiKeyHelper",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Snapshot of state the notice registry inspects.
#[derive(Debug, Clone, Default)]
pub struct StatusNoticeContext {
    pub memory_files: Vec<MemoryFileInfo>,
    pub agent_definitions: Option<AgentDefinitionsResult>,
    pub is_custom_backend_enabled: bool,
    pub auth_token_source: Option<AuthSource>,
    pub api_key_source: Option<AuthSource>,
    pub api_key_from_config_or_keychain: bool,
    pub is_hosted_subscriber: bool,
    /// True when running in a supported JetBrains terminal where the
    /// install hint can be auto-applied.
    pub is_supported_jetbrains_terminal: bool,
    /// Plugin auto-install is allowed.
    pub auto_install_ide_extension: bool,
    /// JetBrains plugin already installed for the detected IDE.
    pub jetbrains_plugin_installed: bool,
    /// Display name of the JetBrains IDE (e.g., "IntelliJ IDEA").
    pub jetbrains_ide_display_name: Option<String>,
}

/// A single notice — its ID, severity, and rendered text.
#[derive(Debug, Clone)]
pub struct StatusNotice {
    pub id: &'static str,
    pub notice_type: StatusNoticeType,
    pub text: String,
}

/// A notice definition — predicate + renderer pair. Mirrors TS
/// `StatusNoticeDefinition`. The renderer returns `Option<StatusNotice>`
/// so the closure can short-circuit if it determines (mid-render) that the
/// notice no longer applies.
pub struct StatusNoticeDefinition {
    pub id: &'static str,
    pub notice_type: StatusNoticeType,
    pub is_active: fn(&StatusNoticeContext) -> bool,
    pub render: fn(&StatusNoticeContext) -> Option<StatusNotice>,
}

fn get_large_memory_files(files: &[MemoryFileInfo]) -> Vec<&MemoryFileInfo> {
    files
        .iter()
        .filter(|f| f.content_len > MAX_MEMORY_CHARACTER_COUNT)
        .collect()
}

fn large_memory_files_active(ctx: &StatusNoticeContext) -> bool {
    !get_large_memory_files(&ctx.memory_files).is_empty()
}

fn large_memory_files_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let large = get_large_memory_files(&ctx.memory_files);
    if large.is_empty() {
        return None;
    }
    let mut lines = Vec::with_capacity(large.len());
    for f in large {
        lines.push(format!(
            "Large {} will impact performance ({} chars > {}) · /memory to edit",
            f.display_path,
            format_number(f.content_len),
            format_number(MAX_MEMORY_CHARACTER_COUNT),
        ));
    }
    Some(StatusNotice {
        id: "large-memory-files",
        notice_type: StatusNoticeType::Warning,
        text: lines.join("\n"),
    })
}

fn hosted_subscriber_external_token_active(ctx: &StatusNoticeContext) -> bool {
    if ctx.is_custom_backend_enabled {
        return false;
    }
    if !ctx.is_hosted_subscriber {
        return false;
    }
    matches!(
        ctx.auth_token_source,
        Some(AuthSource::MossenCodeAuthToken) | Some(AuthSource::ApiKeyHelper)
    )
}

fn hosted_subscriber_external_token_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let source = ctx.auth_token_source.as_ref()?.as_str().to_string();
    Some(StatusNotice {
        id: "hosted-external-token",
        notice_type: StatusNoticeType::Warning,
        text: format!(
            "Auth conflict: Using {source} instead of the hosted session token. Either unset {source}, or run `mossen /logout`."
        ),
    })
}

fn api_key_conflict_active(ctx: &StatusNoticeContext) -> bool {
    if ctx.is_custom_backend_enabled {
        return false;
    }
    if !ctx.api_key_from_config_or_keychain {
        return false;
    }
    matches!(
        ctx.api_key_source,
        Some(AuthSource::MossenCodeApiKey) | Some(AuthSource::ApiKeyHelper)
    )
}

fn api_key_conflict_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let source = ctx.api_key_source.as_ref()?.as_str().to_string();
    Some(StatusNotice {
        id: "api-key-conflict",
        notice_type: StatusNoticeType::Warning,
        text: format!(
            "Auth conflict: Using {source} instead of the configured backend API key. Either unset {source}, or run `mossen /logout`."
        ),
    })
}

fn both_auth_methods_active(ctx: &StatusNoticeContext) -> bool {
    if ctx.is_custom_backend_enabled {
        return false;
    }
    let api_key_set = ctx
        .api_key_source
        .as_ref()
        .map(|s| !matches!(s, AuthSource::None))
        .unwrap_or(false);
    let token_set = ctx
        .auth_token_source
        .as_ref()
        .map(|s| !matches!(s, AuthSource::None))
        .unwrap_or(false);
    if !(api_key_set && token_set) {
        return false;
    }
    // Both being apiKeyHelper does NOT trigger the conflict (TS exception).
    let both_helper = matches!(ctx.api_key_source, Some(AuthSource::ApiKeyHelper))
        && matches!(ctx.auth_token_source, Some(AuthSource::ApiKeyHelper));
    !both_helper
}

fn both_auth_methods_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let api_key_source = ctx.api_key_source.as_ref()?.as_str().to_string();
    let token_source = ctx.auth_token_source.as_ref()?.as_str().to_string();

    let try_use_token_hint = match &api_key_source[..] {
        "MOSSEN_CODE_API_KEY" => "Unset the MOSSEN_CODE_API_KEY environment variable.".to_string(),
        "apiKeyHelper" => "Unset the apiKeyHelper setting.".to_string(),
        _ => "mossen /logout".to_string(),
    };

    let label_token_source = if token_source == "hosted" {
        "the hosted session".to_string()
    } else {
        token_source.clone()
    };

    let try_use_apikey_hint = if token_source == "hosted" {
        "mossen /logout to sign out of the hosted session.".to_string()
    } else {
        format!("Unset the {} environment variable.", token_source)
    };

    let text = format!(
        "Auth conflict: Both a token ({token_source}) and an API key ({api_key_source}) are set. This may lead to unexpected behavior.\n  · Trying to use {label_token_source}? {try_use_token_hint}\n  · Trying to use {api_key_source}? {try_use_apikey_hint}"
    );

    Some(StatusNotice {
        id: "both-auth-methods",
        notice_type: StatusNoticeType::Warning,
        text,
    })
}

fn large_agent_descriptions_active(ctx: &StatusNoticeContext) -> bool {
    let total = get_agent_descriptions_total_tokens(ctx.agent_definitions.as_ref());
    total > AGENT_DESCRIPTIONS_THRESHOLD
}

fn large_agent_descriptions_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let total = get_agent_descriptions_total_tokens(ctx.agent_definitions.as_ref());
    Some(StatusNotice {
        id: "large-agent-descriptions",
        notice_type: StatusNoticeType::Warning,
        text: format!(
            "Large cumulative agent descriptions will impact performance (~{} tokens > {}) · /agents to manage",
            format_number(total as u64),
            format_number(AGENT_DESCRIPTIONS_THRESHOLD as u64),
        ),
    })
}

fn jetbrains_plugin_active(ctx: &StatusNoticeContext) -> bool {
    if !ctx.is_supported_jetbrains_terminal {
        return false;
    }
    if !ctx.auto_install_ide_extension {
        return false;
    }
    !ctx.jetbrains_plugin_installed
}

fn jetbrains_plugin_render(ctx: &StatusNoticeContext) -> Option<StatusNotice> {
    let ide_name = ctx
        .jetbrains_ide_display_name
        .clone()
        .unwrap_or_else(|| "JetBrains IDE".to_string());
    Some(StatusNotice {
        id: "jetbrains-plugin-install",
        notice_type: StatusNoticeType::Info,
        text: format!("Install the {ide_name} plugin from the JetBrains Marketplace."),
    })
}

/// The full registry of notice definitions. Order matches TS
/// `statusNoticeDefinitions`.
pub fn status_notice_definitions() -> Vec<StatusNoticeDefinition> {
    vec![
        StatusNoticeDefinition {
            id: "large-memory-files",
            notice_type: StatusNoticeType::Warning,
            is_active: large_memory_files_active,
            render: large_memory_files_render,
        },
        StatusNoticeDefinition {
            id: "large-agent-descriptions",
            notice_type: StatusNoticeType::Warning,
            is_active: large_agent_descriptions_active,
            render: large_agent_descriptions_render,
        },
        StatusNoticeDefinition {
            id: "hosted-external-token",
            notice_type: StatusNoticeType::Warning,
            is_active: hosted_subscriber_external_token_active,
            render: hosted_subscriber_external_token_render,
        },
        StatusNoticeDefinition {
            id: "api-key-conflict",
            notice_type: StatusNoticeType::Warning,
            is_active: api_key_conflict_active,
            render: api_key_conflict_render,
        },
        StatusNoticeDefinition {
            id: "both-auth-methods",
            notice_type: StatusNoticeType::Warning,
            is_active: both_auth_methods_active,
            render: both_auth_methods_render,
        },
        StatusNoticeDefinition {
            id: "jetbrains-plugin-install",
            notice_type: StatusNoticeType::Info,
            is_active: jetbrains_plugin_active,
            render: jetbrains_plugin_render,
        },
    ]
}

/// Return the rendered set of notices that apply to the given context.
pub fn get_active_notices(context: &StatusNoticeContext) -> Vec<StatusNotice> {
    status_notice_definitions()
        .into_iter()
        .filter(|d| (d.is_active)(context))
        .filter_map(|d| (d.render)(context))
        .collect()
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_notices_when_state_clean() {
        let ctx = StatusNoticeContext::default();
        assert!(get_active_notices(&ctx).is_empty());
    }

    #[test]
    fn large_memory_triggers_notice() {
        let ctx = StatusNoticeContext {
            memory_files: vec![MemoryFileInfo {
                path: "/tmp/MOSSEN.md".into(),
                display_path: "MOSSEN.md".into(),
                content_len: MAX_MEMORY_CHARACTER_COUNT + 10,
            }],
            ..Default::default()
        };
        let notices = get_active_notices(&ctx);
        assert!(notices.iter().any(|n| n.id == "large-memory-files"));
    }

    #[test]
    fn both_auth_skip_when_both_helper() {
        let ctx = StatusNoticeContext {
            api_key_source: Some(AuthSource::ApiKeyHelper),
            auth_token_source: Some(AuthSource::ApiKeyHelper),
            ..Default::default()
        };
        assert!(!both_auth_methods_active(&ctx));
    }

    #[test]
    fn both_auth_triggers_for_mixed_sources() {
        let ctx = StatusNoticeContext {
            api_key_source: Some(AuthSource::MossenCodeApiKey),
            auth_token_source: Some(AuthSource::MossenCodeAuthToken),
            ..Default::default()
        };
        assert!(both_auth_methods_active(&ctx));
    }
}
