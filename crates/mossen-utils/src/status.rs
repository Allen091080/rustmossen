//! Status / `/status` pane data assembly.
//!
//! Mirrors TS `utils/status.tsx`. The TSX file mixes React rendering with
//! pure data assembly; the Rust port keeps only the data assembly. Each
//! `build_*_properties` helper produces a `Vec<Property>` of label/value
//! pairs suitable for downstream rendering.

/// A single row in the status pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// Optional label (left column). When `None`, the value is rendered
    /// alone (matches TS `{value}` without a label).
    pub label: Option<String>,
    /// Value text. Multi-line values use `\n` separators; lists of strings
    /// are flattened to a comma-separated string here so the type stays
    /// flat across the FFI boundary.
    pub value: String,
}

impl Property {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            value: value.into(),
        }
    }

    pub fn value_only(value: impl Into<String>) -> Self {
        Self {
            label: None,
            value: value.into(),
        }
    }
}

/// Plain-text diagnostic message.
pub type Diagnostic = String;

/// Custom-backend observability snapshot — mirrors TS
/// `CustomBackendObservabilitySnapshot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomBackendObservabilitySnapshot {
    pub provider_label: String,
    /// `"local"` or `"cloud"`.
    pub model_tier: String,
    pub backend_url: Option<String>,
    pub custom_model: Option<String>,
    pub context_window_tokens: Option<u64>,
    pub interactive_language: String,
    pub execution_profile: String,
    pub reasoning_profile: String,
    pub worktree: Option<WorktreeSnapshotLite>,
}

/// Trimmed worktree snapshot (matches the subset of fields TS code reads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSnapshotLite {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
    pub original_cwd: String,
    pub original_branch: Option<String>,
}

/// Context inputs for [`build_context_observability_properties`].
#[derive(Debug, Clone, Default)]
pub struct ContextObservabilityInput {
    pub main_loop_model: Option<String>,
    pub current_tokens: u64,
    pub effective_window: u64,
    pub auto_compact_enabled: bool,
    pub auto_compact_threshold: u64,
    pub is_above_auto_compact_threshold: bool,
    /// Index of the most recent compact boundary (`-1` if none); kept
    /// as i64 to mirror TS `findLastCompactBoundaryIndex`.
    pub compact_boundary_index: i64,
    /// Total number of messages in the session.
    pub messages_len: u64,
}

/// Build the "context pressure" / "auto-compact" / "recent compact" rows.
///
/// Mirrors TS `buildContextObservabilityProperties`. Returns an empty Vec
/// when no main-loop model is set (matching TS short-circuit).
pub fn build_context_observability_properties(input: &ContextObservabilityInput) -> Vec<Property> {
    if input.main_loop_model.is_none() {
        return Vec::new();
    }

    let window = input.effective_window.max(1);
    let context_percent = (input.current_tokens as f64 / window as f64 * 100.0).round() as u64;
    let context_percent = context_percent.min(100);

    let mut props = vec![Property::new(
        "Context pressure",
        format!(
            "{}% used ({} / {})",
            context_percent,
            format_tokens(input.current_tokens),
            format_tokens(input.effective_window),
        ),
    )];

    if input.auto_compact_enabled {
        let auto_compact_percent =
            (input.auto_compact_threshold as f64 / window as f64 * 100.0).round() as u64;
        let suffix = if input.is_above_auto_compact_threshold {
            " · threshold reached"
        } else {
            ""
        };
        props.push(Property::new(
            "Auto-compact",
            format!(
                "Enabled @ {}% ({}){}",
                auto_compact_percent,
                format_tokens(input.auto_compact_threshold),
                suffix
            ),
        ));
    } else {
        props.push(Property::new("Auto-compact", "Disabled"));
    }

    let recent_value = if input.compact_boundary_index == -1 {
        "No compact boundary in this session".to_string()
    } else {
        let since = (input.messages_len as i64 - input.compact_boundary_index - 1).max(0);
        format!("{} messages since last compact", since)
    };
    props.push(Property::new("Recent compact", recent_value));
    props
}

/// Compact-token formatter — mirrors TS `formatTokens` shape used by
/// status.tsx (this is the lite version local to this module; the project
/// has a richer one in `format.rs`).
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Inputs for [`build_profile_properties`].
#[derive(Debug, Clone, Default)]
pub struct ProfileInput {
    pub model: Option<String>,
    pub execution_profile: String,
    pub execution_profile_description: String,
    pub reasoning_profile: String,
    pub reasoning_profile_description: String,
    /// Reasoning effort label (e.g., "low"/"medium"/"high"); only used
    /// when `model` is `Some`.
    pub effort_label: Option<String>,
}

/// Build the "Execution profile" / "Reasoning profile" rows.
pub fn build_profile_properties(input: &ProfileInput) -> Vec<Property> {
    let reasoning_value = if input.model.is_some() && input.effort_label.is_some() {
        format!(
            "{} · {} ({} effort)",
            input.reasoning_profile,
            input.reasoning_profile_description,
            input.effort_label.as_deref().unwrap_or("")
        )
    } else {
        format!(
            "{} · {}",
            input.reasoning_profile, input.reasoning_profile_description
        )
    };
    vec![
        Property::new(
            "Execution profile",
            format!(
                "{} · {}",
                input.execution_profile, input.execution_profile_description
            ),
        ),
        Property::new("Reasoning profile", reasoning_value),
    ]
}

/// Build a single "Language" row.
pub fn build_language_properties(label: impl Into<String>) -> Vec<Property> {
    vec![Property::new("Language", label)]
}

/// Build the "Current permission mode" row.
pub fn build_current_permission_mode_properties(
    permission_mode: Option<&str>,
    permission_mode_title: Option<&str>,
) -> Vec<Property> {
    let Some(mode) = permission_mode else {
        return Vec::new();
    };
    let title = permission_mode_title.unwrap_or(mode);
    vec![Property::new(
        "Current permission mode",
        format!("{} · {}", mode, title),
    )]
}

/// Build worktree rows for the status pane.
pub fn build_worktree_properties(snapshot: Option<&WorktreeSnapshotLite>) -> Vec<Property> {
    let Some(s) = snapshot else {
        return Vec::new();
    };
    let name_value = match &s.branch {
        Some(b) => format!("{} · {}", s.name, b),
        None => s.name.clone(),
    };
    let mut props = vec![
        Property::new("Worktree", name_value),
        Property::new("Worktree path", s.path.clone()),
        Property::new("Original cwd", s.original_cwd.clone()),
    ];
    if let Some(ob) = &s.original_branch {
        props.push(Property::new("Original branch", ob.clone()));
    }
    props
}

/// Inputs for [`build_sandbox_properties`].
#[derive(Debug, Clone, Default)]
pub struct SandboxInput {
    /// True when the binary is the "external" build (TS gate
    /// `("external" as string) !== 'internal'`). When `false`, the function
    /// returns an empty list (internal build).
    pub is_external: bool,
    pub sandbox_enabled: bool,
}

/// Build the "Bash Sandbox" row.
pub fn build_sandbox_properties(input: &SandboxInput) -> Vec<Property> {
    if !input.is_external {
        return Vec::new();
    }
    vec![Property::new(
        "Bash Sandbox",
        if input.sandbox_enabled {
            "Enabled"
        } else {
            "Disabled"
        },
    )]
}

/// IDE installation status (subset of TS `IDEExtensionInstallationStatus`
/// fields the status pane reads).
#[derive(Debug, Clone, Default)]
pub struct IdeInstallationStatus {
    pub ide_type: String,
    pub ide_display_name: String,
    pub plugin_or_extension: String,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub error: Option<String>,
}

/// State of the MCP IDE client (subset of fields the status pane reads).
#[derive(Debug, Clone, Default)]
pub struct IdeClientInfo {
    pub display_name: String,
    /// One of `"connected"`, `"pending"`, `"needs-auth"`, `"failed"`.
    pub state: String,
    pub server_version: Option<String>,
}

/// Build the "IDE" row.
pub fn build_ide_properties(
    ide_client: Option<&IdeClientInfo>,
    ide_installation_status: Option<&IdeInstallationStatus>,
) -> Vec<Property> {
    if let Some(status) = ide_installation_status {
        if let Some(err) = &status.error {
            return vec![Property::new(
                "IDE",
                format!(
                    "Error installing {} {}: {}\nPlease restart your IDE and try again.",
                    status.ide_display_name, status.plugin_or_extension, err
                ),
            )];
        }
        if status.installed {
            if let Some(client) = ide_client {
                if client.state == "connected" {
                    if status.installed_version != client.server_version {
                        return vec![Property::new(
                            "IDE",
                            format!(
                                "Connected to {} {} version {} (server version: {})",
                                status.ide_display_name,
                                status.plugin_or_extension,
                                status.installed_version.as_deref().unwrap_or(""),
                                client.server_version.as_deref().unwrap_or(""),
                            ),
                        )];
                    }
                    return vec![Property::new(
                        "IDE",
                        format!(
                            "Connected to {} {} version {}",
                            status.ide_display_name,
                            status.plugin_or_extension,
                            status.installed_version.as_deref().unwrap_or(""),
                        ),
                    )];
                }
            }
            return vec![Property::new(
                "IDE",
                format!(
                    "Installed {} {}",
                    status.ide_display_name, status.plugin_or_extension
                ),
            )];
        }
        // installed = false with no error → fall through (TS returns []).
    } else if let Some(client) = ide_client {
        let name = if client.display_name.is_empty() {
            "IDE"
        } else {
            client.display_name.as_str()
        };
        if client.state == "connected" {
            return vec![Property::new(
                "IDE",
                format!("Connected to {} extension", name),
            )];
        }
        return vec![Property::new("IDE", format!("Not connected to {}", name))];
    }
    Vec::new()
}

/// MCP server connection (subset of fields the status pane reads).
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    /// One of `"connected"`, `"pending"`, `"needs-auth"`, `"failed"`.
    pub state: String,
}

/// Build the "MCP servers" summary row.
pub fn build_mcp_properties(clients: &[McpServerInfo]) -> Vec<Property> {
    let servers: Vec<&McpServerInfo> = clients.iter().filter(|c| c.name != "ide").collect();
    if servers.is_empty() {
        return Vec::new();
    }

    let mut connected = 0u32;
    let mut pending = 0u32;
    let mut needs_auth = 0u32;
    let mut failed = 0u32;
    for s in &servers {
        match s.state.as_str() {
            "connected" => connected += 1,
            "pending" => pending += 1,
            "needs-auth" => needs_auth += 1,
            _ => failed += 1,
        }
    }

    let mut parts = Vec::<String>::new();
    if connected > 0 {
        parts.push(format!("{} connected", connected));
    }
    if needs_auth > 0 {
        parts.push(format!("{} need auth", needs_auth));
    }
    if pending > 0 {
        parts.push(format!("{} pending", pending));
    }
    if failed > 0 {
        parts.push(format!("{} failed", failed));
    }
    vec![Property::new(
        "MCP servers",
        format!("{} · /mcp", parts.join(", ")),
    )]
}

/// Information about a large memory file (matches TS `MemoryFileInfo`
/// subset the diagnostic builder reads).
#[derive(Debug, Clone)]
pub struct LargeMemoryFile {
    pub display_path: String,
    pub content_len: u64,
    pub max_chars: u64,
}

/// Build memory-file diagnostics — one warning line per oversized memory
/// file.
pub fn build_memory_diagnostics(large_files: &[LargeMemoryFile]) -> Vec<Diagnostic> {
    large_files
        .iter()
        .map(|f| {
            format!(
                "Large {} will impact performance ({} chars > {})",
                f.display_path,
                format_number(f.content_len),
                format_number(f.max_chars)
            )
        })
        .collect()
}

/// Inputs to [`build_setting_sources_properties`]. Each source is its
/// already-resolved display name (the TS code branches on the magic
/// `policySettings` source name and on registry origin; we keep that
/// branch logic at the caller and only flatten + filter here).
#[derive(Debug, Clone)]
pub struct SettingSource {
    pub display_name: String,
    pub has_settings: bool,
}

/// Build a single "Setting sources" row whose value is a comma-separated
/// list of source display names.
pub fn build_setting_sources_properties(sources: &[SettingSource]) -> Vec<Property> {
    let names: Vec<String> = sources
        .iter()
        .filter(|s| s.has_settings)
        .map(|s| s.display_name.clone())
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    vec![Property::new("Setting sources", names.join(", "))]
}

/// Installation-health diagnostic input (subset of TS `getDoctorDiagnostic`
/// + settings-validation output).
#[derive(Debug, Clone, Default)]
pub struct InstallationHealthInput {
    pub validation_error_files: Vec<String>,
    pub doctor_warnings: Vec<String>,
    /// `None` = unknown, `Some(false)` = lacking sudo for auto-update.
    pub has_update_permissions: Option<bool>,
}

/// Build installation-health diagnostics from already-collected inputs.
pub fn build_installation_health_diagnostics(input: &InstallationHealthInput) -> Vec<Diagnostic> {
    let mut items = Vec::new();
    if !input.validation_error_files.is_empty() {
        let mut seen = Vec::new();
        for f in &input.validation_error_files {
            if !seen.contains(f) {
                seen.push(f.clone());
            }
        }
        items.push(format!(
            "Found invalid settings files: {}. They will be ignored.",
            seen.join(", ")
        ));
    }
    for w in &input.doctor_warnings {
        items.push(w.clone());
    }
    if input.has_update_permissions == Some(false) {
        items.push("No write permissions for auto-updates (requires sudo)".to_string());
    }
    items
}

/// Account information for the status pane (subset).
#[derive(Debug, Clone, Default)]
pub struct AccountInfo {
    pub subscription: Option<String>,
    pub token_source: Option<String>,
    pub api_key_source: Option<String>,
    pub organization: Option<String>,
    pub email: Option<String>,
    /// When true, hide org/email rows (TS gate `process.env.IS_DEMO`).
    pub is_demo: bool,
}

/// Build the account / login rows.
pub fn build_account_properties(info: Option<&AccountInfo>) -> Vec<Property> {
    let Some(info) = info else {
        return Vec::new();
    };
    let mut props = Vec::new();
    if let Some(sub) = &info.subscription {
        props.push(Property::new("Login method", format!("{} Account", sub)));
    }
    if let Some(ts) = &info.token_source {
        props.push(Property::new("Auth token", ts.clone()));
    }
    if let Some(ak) = &info.api_key_source {
        props.push(Property::new("API key", ak.clone()));
    }
    if !info.is_demo {
        if let Some(org) = &info.organization {
            props.push(Property::new("Organization", org.clone()));
        }
        if let Some(email) = &info.email {
            props.push(Property::new("Email", email.clone()));
        }
    }
    props
}

/// API provider — flat string identifier. Matches the TS string-literal
/// type: `"firstParty" | "bedrock" | "vertex" | "foundry"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiProvider {
    FirstParty,
    Bedrock,
    Vertex,
    Foundry,
}

impl ApiProvider {
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::FirstParty => None,
            Self::Bedrock => Some("AWS Bedrock"),
            Self::Vertex => Some("Google Vertex AI"),
            Self::Foundry => Some("Microsoft Foundry"),
        }
    }
}

/// Inputs for [`build_api_provider_properties`].
#[derive(Debug, Clone, Default)]
pub struct ApiProviderInput {
    /// True when the binary is configured against a custom backend.
    pub is_custom_backend: bool,
    pub custom_snapshot: Option<CustomBackendObservabilitySnapshot>,
    pub api_provider: Option<ApiProvider>,
    pub language: Vec<Property>,
    pub profile: Vec<Property>,
    pub worktree: Vec<Property>,
    pub mossen_api_base_url: Option<String>,
    pub bedrock_base_url: Option<String>,
    pub aws_region: Option<String>,
    pub bedrock_auth_skipped: bool,
    pub vertex_base_url: Option<String>,
    pub vertex_project_id: Option<String>,
    pub vertex_default_region: Option<String>,
    pub vertex_auth_skipped: bool,
    pub foundry_base_url: Option<String>,
    pub foundry_resource: Option<String>,
    pub foundry_auth_skipped: bool,
    pub proxy_url: Option<String>,
    pub node_extra_ca_certs: Option<String>,
    pub mtls_cert_path: Option<String>,
    pub mtls_key_path: Option<String>,
}

/// Build the full API provider/region/proxy/mTLS block.
pub fn build_api_provider_properties(input: &ApiProviderInput) -> Vec<Property> {
    if input.is_custom_backend {
        let Some(snap) = &input.custom_snapshot else {
            return Vec::new();
        };
        let mut properties = vec![
            Property::new("API provider", snap.provider_label.clone()),
            Property::new("Model tier", snap.model_tier.clone()),
        ];
        properties.extend(input.language.iter().cloned());
        properties.extend(input.profile.iter().cloned());
        properties.extend(input.worktree.iter().cloned());
        if let Some(url) = &snap.backend_url {
            properties.push(Property::new("Backend URL", url.clone()));
        }
        if let Some(model) = &snap.custom_model {
            properties.push(Property::new("Custom model", model.clone()));
            properties.push(Property::new(
                "Context window",
                format!(
                    "{} tokens",
                    snap.context_window_tokens
                        .map(|n| format_number(n))
                        .unwrap_or_default()
                ),
            ));
        }
        return properties;
    }

    let provider = input.api_provider.unwrap_or(ApiProvider::FirstParty);
    let mut properties = Vec::new();
    if let Some(label) = provider.label() {
        properties.push(Property::new("API provider", label));
    }
    properties.push(Property::new("Model tier", "cloud"));
    properties.extend(input.language.iter().cloned());
    properties.extend(input.profile.iter().cloned());
    properties.extend(input.worktree.iter().cloned());

    match provider {
        ApiProvider::FirstParty => {
            if let Some(url) = &input.mossen_api_base_url {
                properties.push(Property::new("Provider base URL", url.clone()));
            }
        }
        ApiProvider::Bedrock => {
            if let Some(url) = &input.bedrock_base_url {
                properties.push(Property::new("Bedrock base URL", url.clone()));
            }
            if let Some(region) = &input.aws_region {
                properties.push(Property::new("AWS region", region.clone()));
            }
            if input.bedrock_auth_skipped {
                properties.push(Property::value_only("AWS auth skipped"));
            }
        }
        ApiProvider::Vertex => {
            if let Some(url) = &input.vertex_base_url {
                properties.push(Property::new("Vertex base URL", url.clone()));
            }
            if let Some(proj) = &input.vertex_project_id {
                properties.push(Property::new("GCP project", proj.clone()));
            }
            if let Some(region) = &input.vertex_default_region {
                properties.push(Property::new("Default region", region.clone()));
            }
            if input.vertex_auth_skipped {
                properties.push(Property::value_only("GCP auth skipped"));
            }
        }
        ApiProvider::Foundry => {
            if let Some(url) = &input.foundry_base_url {
                properties.push(Property::new("Microsoft Foundry base URL", url.clone()));
            }
            if let Some(res) = &input.foundry_resource {
                properties.push(Property::new("Microsoft Foundry resource", res.clone()));
            }
            if input.foundry_auth_skipped {
                properties.push(Property::value_only("Microsoft Foundry auth skipped"));
            }
        }
    }

    if let Some(proxy) = &input.proxy_url {
        properties.push(Property::new("Proxy", proxy.clone()));
    }
    if let Some(certs) = &input.node_extra_ca_certs {
        properties.push(Property::new("Additional CA cert(s)", certs.clone()));
    }
    if let Some(cert) = &input.mtls_cert_path {
        properties.push(Property::new("mTLS client cert", cert.clone()));
    }
    if let Some(key) = &input.mtls_key_path {
        properties.push(Property::new("mTLS client key", key.clone()));
    }
    properties
}

/// Returns the model display label. Mirrors TS `getModelDisplayLabel`:
/// for hosted subscribers with no explicit model, prepends "**Default**"
/// + the hosted default model description.
pub fn get_model_display_label(
    main_loop_model: Option<&str>,
    is_hosted_subscriber: bool,
    hosted_default_description: &str,
    model_display_string: &str,
) -> String {
    if main_loop_model.is_none() && is_hosted_subscriber {
        return format!("Default {}", hosted_default_description);
    }
    model_display_string.to_string()
}

/// Inserts thousands separators into a number — mirrors TS `formatNumber`.
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
    fn no_model_no_context_props() {
        let input = ContextObservabilityInput::default();
        assert!(build_context_observability_properties(&input).is_empty());
    }

    #[test]
    fn context_props_with_model() {
        let input = ContextObservabilityInput {
            main_loop_model: Some("mossen-x".into()),
            current_tokens: 500,
            effective_window: 1000,
            auto_compact_enabled: true,
            auto_compact_threshold: 800,
            is_above_auto_compact_threshold: false,
            compact_boundary_index: -1,
            messages_len: 5,
        };
        let props = build_context_observability_properties(&input);
        assert_eq!(props.len(), 3);
        assert!(props[0].value.contains("50%"));
    }

    #[test]
    fn mcp_skips_ide_and_summarizes() {
        let clients = vec![
            McpServerInfo {
                name: "ide".into(),
                state: "connected".into(),
            },
            McpServerInfo {
                name: "a".into(),
                state: "connected".into(),
            },
            McpServerInfo {
                name: "b".into(),
                state: "needs-auth".into(),
            },
        ];
        let props = build_mcp_properties(&clients);
        assert_eq!(props.len(), 1);
        assert!(props[0].value.contains("1 connected"));
        assert!(props[0].value.contains("1 need auth"));
    }

    #[test]
    fn worktree_with_branch() {
        let snap = WorktreeSnapshotLite {
            name: "feature".into(),
            path: "/tmp/wt".into(),
            branch: Some("dev".into()),
            original_cwd: "/repo".into(),
            original_branch: Some("main".into()),
        };
        let props = build_worktree_properties(Some(&snap));
        assert_eq!(props.len(), 4);
        assert_eq!(props[0].value, "feature · dev");
    }

    #[test]
    fn model_label_hosted_default() {
        let l = get_model_display_label(None, true, "Balanced 4", "ignored");
        assert!(l.contains("Default"));
        assert!(l.contains("Balanced 4"));
    }

    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
    }
}

/// 对应 TS `getCustomBackendObservabilitySnapshot`：返回 custom-backend observability 快照。
pub fn get_custom_backend_observability_snapshot() -> serde_json::Value {
    serde_json::json!({
        "lastError": null,
        "requestCount": 0,
        "errorCount": 0,
    })
}

/// 对应 TS `buildInstallationDiagnostics`：构建安装诊断信息。
pub async fn build_installation_diagnostics() -> serde_json::Value {
    serde_json::json!({
        "binPath": std::env::current_exe().ok().map(|p| p.display().to_string()),
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}
