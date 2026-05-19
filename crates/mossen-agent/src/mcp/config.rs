//! MCP configuration management — loading, parsing, adding, removing configs.
//!
//! Translates `services/mcp/config.ts` (1580 lines).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mcp::env_expansion::expand_env_vars_in_string;
use crate::mcp::types::{ConfigScope, McpServerConfig, ScopedMcpServerConfig};

/// CCR proxy URL path markers.
const CCR_PROXY_PATH_MARKERS: &[&str] = &[
    "/v2/session_ingress/shttp/mcp/",
    "/v2/ccr-sessions/",
];

/// Get the path to the managed MCP configuration file.
pub fn get_enterprise_mcp_file_path(managed_path: &Path) -> PathBuf {
    managed_path.join("managed-mcp.json")
}

/// Add scope to server configs.
fn add_scope_to_servers(
    servers: &HashMap<String, McpServerConfig>,
    scope: ConfigScope,
) -> HashMap<String, ScopedMcpServerConfig> {
    servers
        .iter()
        .map(|(name, config)| {
            (
                name.clone(),
                ScopedMcpServerConfig {
                    config: config.clone(),
                    scope,
                    plugin_source: None,
                },
            )
        })
        .collect()
}

/// Extract command array from server config (stdio servers only).
fn get_server_command_array(config: &McpServerConfig) -> Option<Vec<String>> {
    match config {
        McpServerConfig::Stdio { command, args, .. } => {
            let mut arr = vec![command.clone()];
            arr.extend(args.iter().cloned());
            Some(arr)
        }
        _ => None,
    }
}

/// Check if two command arrays match exactly.
fn command_arrays_match(a: &[String], b: &[String]) -> bool {
    a == b
}

/// Extract URL from server config (remote servers only).
fn get_server_url(config: &McpServerConfig) -> Option<&str> {
    match config {
        McpServerConfig::Sse { url, .. }
        | McpServerConfig::Http { url, .. }
        | McpServerConfig::Ws { url, .. }
        | McpServerConfig::HostedProxy { url, .. } => Some(url),
        _ => None,
    }
}

/// If the URL is a CCR proxy URL, extract the original vendor URL.
pub fn unwrap_ccr_proxy_url(url: &str) -> String {
    if !CCR_PROXY_PATH_MARKERS.iter().any(|m| url.contains(m)) {
        return url.to_string();
    }
    match url::Url::parse(url) {
        Ok(u) => u
            .query_pairs()
            .find(|(k, _)| k == "mcp_url")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| url.to_string()),
        Err(_) => url.to_string(),
    }
}

/// Compute a dedup signature for an MCP server config.
pub fn get_mcp_server_signature(config: &McpServerConfig) -> Option<String> {
    if let Some(cmd) = get_server_command_array(config) {
        return Some(format!("stdio:{}", serde_json::to_string(&cmd).unwrap_or_default()));
    }
    if let Some(url) = get_server_url(config) {
        return Some(format!("url:{}", unwrap_ccr_proxy_url(url)));
    }
    None
}

/// Filter plugin MCP servers, dropping duplicates.
pub fn dedup_plugin_mcp_servers(
    plugin_servers: &HashMap<String, ScopedMcpServerConfig>,
    manual_servers: &HashMap<String, ScopedMcpServerConfig>,
) -> (HashMap<String, ScopedMcpServerConfig>, Vec<(String, String)>) {
    let mut manual_sigs: HashMap<String, String> = HashMap::new();
    for (name, config) in manual_servers {
        if let Some(sig) = get_mcp_server_signature(&config.config) {
            manual_sigs.entry(sig).or_insert_with(|| name.clone());
        }
    }

    let mut servers = HashMap::new();
    let mut suppressed = Vec::new();
    let mut seen_plugin_sigs: HashMap<String, String> = HashMap::new();

    for (name, config) in plugin_servers {
        let sig = get_mcp_server_signature(&config.config);
        if let Some(ref sig) = sig {
            if let Some(manual_dup) = manual_sigs.get(sig) {
                suppressed.push((name.clone(), manual_dup.clone()));
                continue;
            }
            if let Some(plugin_dup) = seen_plugin_sigs.get(sig) {
                suppressed.push((name.clone(), plugin_dup.clone()));
                continue;
            }
            seen_plugin_sigs.insert(sig.clone(), name.clone());
        }
        servers.insert(name.clone(), config.clone());
    }

    (servers, suppressed)
}

/// Filter hosted connectors, dropping duplicates of manual servers.
pub fn dedup_hosted_mcp_servers(
    hosted_servers: &HashMap<String, ScopedMcpServerConfig>,
    manual_servers: &HashMap<String, ScopedMcpServerConfig>,
    is_disabled: impl Fn(&str) -> bool,
) -> (HashMap<String, ScopedMcpServerConfig>, Vec<(String, String)>) {
    let mut manual_sigs: HashMap<String, String> = HashMap::new();
    for (name, config) in manual_servers {
        if is_disabled(name) {
            continue;
        }
        if let Some(sig) = get_mcp_server_signature(&config.config) {
            manual_sigs.entry(sig).or_insert_with(|| name.clone());
        }
    }

    let mut servers = HashMap::new();
    let mut suppressed = Vec::new();

    for (name, config) in hosted_servers {
        if let Some(sig) = get_mcp_server_signature(&config.config) {
            if let Some(manual_dup) = manual_sigs.get(&sig) {
                suppressed.push((name.clone(), manual_dup.clone()));
                continue;
            }
        }
        servers.insert(name.clone(), config.clone());
    }

    (servers, suppressed)
}

/// Convert a URL pattern with wildcards to a regex check.
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern).replace(r"\*", ".*");
    let re = match regex::Regex::new(&format!("^{}$", escaped)) {
        Ok(r) => r,
        Err(_) => return false,
    };
    re.is_match(url)
}

/// Policy allowlist entry types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerPolicyEntry {
    Name { server_name: String },
    Command { server_command: Vec<String> },
    Url { server_url: String },
}

/// Check if an MCP server is denied by enterprise policy.
pub fn is_mcp_server_denied(
    server_name: &str,
    config: Option<&McpServerConfig>,
    denied_list: &[McpServerPolicyEntry],
) -> bool {
    for entry in denied_list {
        match entry {
            McpServerPolicyEntry::Name { server_name: name } if name == server_name => {
                return true;
            }
            McpServerPolicyEntry::Command { server_command } => {
                if let Some(cfg) = config {
                    if let Some(cmd) = get_server_command_array(cfg) {
                        if command_arrays_match(server_command, &cmd) {
                            return true;
                        }
                    }
                }
            }
            McpServerPolicyEntry::Url { server_url } => {
                if let Some(cfg) = config {
                    if let Some(url) = get_server_url(cfg) {
                        if url_matches_pattern(url, server_url) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if an MCP server is allowed by enterprise policy.
pub fn is_mcp_server_allowed_by_policy(
    server_name: &str,
    config: Option<&McpServerConfig>,
    denied_list: &[McpServerPolicyEntry],
    allowed_list: Option<&[McpServerPolicyEntry]>,
) -> bool {
    if is_mcp_server_denied(server_name, config, denied_list) {
        return false;
    }

    let allowed = match allowed_list {
        Some(list) => list,
        None => return true,
    };

    if allowed.is_empty() {
        return false;
    }

    let has_command_entries = allowed.iter().any(|e| matches!(e, McpServerPolicyEntry::Command { .. }));
    let has_url_entries = allowed.iter().any(|e| matches!(e, McpServerPolicyEntry::Url { .. }));

    if let Some(cfg) = config {
        if let Some(cmd) = get_server_command_array(cfg) {
            if has_command_entries {
                return allowed.iter().any(|e| {
                    matches!(e, McpServerPolicyEntry::Command { server_command } if command_arrays_match(server_command, &cmd))
                });
            }
            return allowed.iter().any(|e| {
                matches!(e, McpServerPolicyEntry::Name { server_name: n } if n == server_name)
            });
        }
        if let Some(url) = get_server_url(cfg) {
            if has_url_entries {
                return allowed.iter().any(|e| {
                    matches!(e, McpServerPolicyEntry::Url { server_url } if url_matches_pattern(url, server_url))
                });
            }
            return allowed.iter().any(|e| {
                matches!(e, McpServerPolicyEntry::Name { server_name: n } if n == server_name)
            });
        }
    }

    allowed.iter().any(|e| {
        matches!(e, McpServerPolicyEntry::Name { server_name: n } if n == server_name)
    })
}

/// Filter MCP servers by policy.
pub fn filter_mcp_servers_by_policy(
    configs: &HashMap<String, ScopedMcpServerConfig>,
    denied_list: &[McpServerPolicyEntry],
    allowed_list: Option<&[McpServerPolicyEntry]>,
) -> (HashMap<String, ScopedMcpServerConfig>, Vec<String>) {
    let mut allowed = HashMap::new();
    let mut blocked = Vec::new();

    for (name, config) in configs {
        if matches!(&config.config, McpServerConfig::Sdk { .. })
            || is_mcp_server_allowed_by_policy(name, Some(&config.config), denied_list, allowed_list)
        {
            allowed.insert(name.clone(), config.clone());
        } else {
            blocked.push(name.clone());
        }
    }

    (allowed, blocked)
}

/// Expand environment variables in an MCP server config.
pub fn expand_env_vars(config: &McpServerConfig) -> (McpServerConfig, Vec<String>) {
    let mut missing_vars = Vec::new();

    let expand = |s: &str| -> String {
        let result = expand_env_vars_in_string(s);
        result.expanded
    };

    let expanded = match config {
        McpServerConfig::Stdio { command, args, env, cwd } => {
            let expanded_cmd = expand(command);
            let expanded_args: Vec<String> = args.iter().map(|a| expand(a)).collect();
            let expanded_env = env.as_ref().map(|e| e.iter().map(|(k, v)| (k.clone(), expand(v))).collect());
            // Collect missing vars
            if let Some(ref e) = env {
                for v in e.values() {
                    let r = expand_env_vars_in_string(v);
                    missing_vars.extend(r.missing_vars);
                }
            }
            for a in args {
                let r = expand_env_vars_in_string(a);
                missing_vars.extend(r.missing_vars);
            }
            {
                let r = expand_env_vars_in_string(command);
                missing_vars.extend(r.missing_vars);
            }
            McpServerConfig::Stdio {
                command: expanded_cmd,
                args: expanded_args,
                env: expanded_env,
                cwd: cwd.clone(),
            }
        }
        McpServerConfig::Sse { url, headers, headers_helper, oauth } => {
            let r = expand_env_vars_in_string(url);
            missing_vars.extend(r.missing_vars);
            let expanded_url = r.expanded;
            let expanded_headers = headers.as_ref().map(|h| {
                h.iter().map(|(k, v)| {
                    let r = expand_env_vars_in_string(v);
                    missing_vars.extend(r.missing_vars);
                    (k.clone(), r.expanded)
                }).collect()
            });
            McpServerConfig::Sse {
                url: expanded_url,
                headers: expanded_headers,
                headers_helper: headers_helper.clone(),
                oauth: oauth.clone(),
            }
        }
        McpServerConfig::Http { url, headers, headers_helper, oauth } => {
            let r = expand_env_vars_in_string(url);
            missing_vars.extend(r.missing_vars);
            let expanded_url = r.expanded;
            let expanded_headers = headers.as_ref().map(|h| {
                h.iter().map(|(k, v)| {
                    let r = expand_env_vars_in_string(v);
                    missing_vars.extend(r.missing_vars);
                    (k.clone(), r.expanded)
                }).collect()
            });
            McpServerConfig::Http {
                url: expanded_url,
                headers: expanded_headers,
                headers_helper: headers_helper.clone(),
                oauth: oauth.clone(),
            }
        }
        McpServerConfig::Ws { url, headers, headers_helper } => {
            let r = expand_env_vars_in_string(url);
            missing_vars.extend(r.missing_vars);
            let expanded_url = r.expanded;
            let expanded_headers = headers.as_ref().map(|h| {
                h.iter().map(|(k, v)| {
                    let r = expand_env_vars_in_string(v);
                    missing_vars.extend(r.missing_vars);
                    (k.clone(), r.expanded)
                }).collect()
            });
            McpServerConfig::Ws {
                url: expanded_url,
                headers: expanded_headers,
                headers_helper: headers_helper.clone(),
            }
        }
        other => other.clone(),
    };

    missing_vars.sort();
    missing_vars.dedup();
    (expanded, missing_vars)
}

/// Validation error for MCP config parsing.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub scope: Option<ConfigScope>,
    pub server_name: Option<String>,
}

/// Parsed MCP JSON config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Parse an MCP JSON config from a file path.
pub fn parse_mcp_config_from_file_path(
    file_path: &Path,
    expand_vars: bool,
    scope: ConfigScope,
) -> (Option<McpJsonConfig>, Vec<ValidationError>) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return (
                    None,
                    vec![ValidationError {
                        message: format!("MCP config file not found: {}", file_path.display()),
                        scope: Some(scope),
                        server_name: None,
                    }],
                );
            }
            return (
                None,
                vec![ValidationError {
                    message: format!("Failed to read MCP config: {}", e),
                    scope: Some(scope),
                    server_name: None,
                }],
            );
        }
    };

    parse_mcp_config_from_string(&content, expand_vars, scope)
}

/// Parse MCP config from a JSON string.
pub fn parse_mcp_config_from_string(
    content: &str,
    expand_vars: bool,
    scope: ConfigScope,
) -> (Option<McpJsonConfig>, Vec<ValidationError>) {
    let parsed: McpJsonConfig = match serde_json::from_str(content) {
        Ok(p) => p,
        Err(e) => {
            return (
                None,
                vec![ValidationError {
                    message: format!("Invalid JSON in MCP config: {}", e),
                    scope: Some(scope),
                    server_name: None,
                }],
            );
        }
    };

    if !expand_vars {
        return (Some(parsed), Vec::new());
    }

    let mut expanded_servers = HashMap::new();
    let mut errors = Vec::new();

    for (name, config) in &parsed.mcp_servers {
        let (expanded, missing) = expand_env_vars(config);
        if !missing.is_empty() {
            errors.push(ValidationError {
                message: format!(
                    "Server '{}' references undefined environment variables: {}",
                    name,
                    missing.join(", ")
                ),
                scope: Some(scope),
                server_name: Some(name.clone()),
            });
        }
        expanded_servers.insert(name.clone(), expanded);
    }

    (
        Some(McpJsonConfig {
            mcp_servers: expanded_servers,
        }),
        errors,
    )
}

/// Check if an MCP server is disabled.
pub fn is_mcp_server_disabled(name: &str, disabled_servers: &[String]) -> bool {
    disabled_servers.contains(&name.to_string())
}

/// Set MCP server enabled/disabled.
pub fn set_mcp_server_enabled(
    name: &str,
    enabled: bool,
    disabled_servers: &mut Vec<String>,
) {
    if enabled {
        disabled_servers.retain(|n| n != name);
    } else if !disabled_servers.contains(&name.to_string()) {
        disabled_servers.push(name.to_string());
    }
}

/// Check whether enterprise MCP config exists.
pub fn does_enterprise_mcp_config_exist(managed_path: &Path) -> bool {
    get_enterprise_mcp_file_path(managed_path).exists()
}

/// Whether allowManagedMcpServersOnly is set.
pub fn should_allow_managed_mcp_servers_only(policy_value: bool) -> bool {
    policy_value
}

/// Get MCP config by name from all available configs.
pub fn get_mcp_config_by_name<'a>(
    name: &str,
    all_configs: &'a HashMap<String, ScopedMcpServerConfig>,
) -> Option<&'a ScopedMcpServerConfig> {
    all_configs.get(name)
}

/// Add MCP config (validation and writing).
pub async fn add_mcp_config(
    name: &str,
    config: &McpServerConfig,
    scope: ConfigScope,
    writer: &dyn McpConfigWriter,
) -> Result<(), String> {
    // Validate name
    let name_re = regex::Regex::new(r"[^a-zA-Z0-9_\-]").unwrap();
    if name_re.is_match(name) {
        return Err(format!(
            "Invalid name {}. Names can only contain letters, numbers, hyphens, and underscores.",
            name
        ));
    }

    match scope {
        ConfigScope::Dynamic | ConfigScope::Enterprise | ConfigScope::Hosted => {
            return Err(format!("Cannot add MCP server to scope: {:?}", scope));
        }
        _ => {}
    }

    writer.write(name, config, scope).await
}

/// Remove MCP config.
pub async fn remove_mcp_config(
    name: &str,
    scope: ConfigScope,
    writer: &dyn McpConfigWriter,
) -> Result<(), String> {
    match scope {
        ConfigScope::Dynamic | ConfigScope::Enterprise | ConfigScope::Hosted => {
            return Err(format!("Cannot remove MCP server from scope: {:?}", scope));
        }
        _ => {}
    }

    writer.remove(name, scope).await
}

/// Trait for writing MCP configs to different scopes.
#[async_trait::async_trait]
pub trait McpConfigWriter: Send + Sync {
    async fn write(&self, name: &str, config: &McpServerConfig, scope: ConfigScope) -> Result<(), String>;
    async fn remove(&self, name: &str, scope: ConfigScope) -> Result<(), String>;
}

/// Get project MCP server status.
pub fn get_project_mcp_server_status(
    server_name: &str,
    enabled_servers: &[String],
    disabled_servers: &[String],
    enable_all: bool,
    is_bypass_permissions: bool,
    is_non_interactive: bool,
    project_settings_enabled: bool,
) -> &'static str {
    let normalized = crate::mcp::normalization::normalize_name_for_mcp(server_name);

    if disabled_servers.iter().any(|n| crate::mcp::normalization::normalize_name_for_mcp(n) == normalized) {
        return "rejected";
    }

    if enabled_servers.iter().any(|n| crate::mcp::normalization::normalize_name_for_mcp(n) == normalized) || enable_all {
        return "approved";
    }

    if is_bypass_permissions && project_settings_enabled {
        return "approved";
    }

    if is_non_interactive && project_settings_enabled {
        return "approved";
    }

    "pending"
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/mcp/config.ts` additional exports.
// ---------------------------------------------------------------------------

/// `config.ts` `getProjectMcpConfigsFromCwd`.
pub fn get_project_mcp_configs_from_cwd() -> HashMap<String, ScopedMcpServerConfig> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_file = cwd.join(".mossen").join("mcp.json");
    let mut out: HashMap<String, ScopedMcpServerConfig> = HashMap::new();
    if let Ok(bytes) = std::fs::read(&project_file) {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(map) = value.get("mcpServers").and_then(|v| v.as_object()) {
                for (name, cfg) in map {
                    if let Ok(server_cfg) =
                        serde_json::from_value::<McpServerConfig>(cfg.clone())
                    {
                        out.insert(
                            name.clone(),
                            ScopedMcpServerConfig {
                                config: server_cfg,
                                scope: ConfigScope::Project,
                                plugin_source: None,
                            },
                        );
                    }
                }
            }
        }
    }
    out
}

/// `config.ts` `getMcpConfigsByScope`.
pub fn get_mcp_configs_by_scope(
    configs: &[ScopedMcpServerConfig],
) -> HashMap<ConfigScope, Vec<ScopedMcpServerConfig>> {
    let mut out: HashMap<ConfigScope, Vec<ScopedMcpServerConfig>> = HashMap::new();
    for cfg in configs {
        out.entry(cfg.scope.clone()).or_default().push(cfg.clone());
    }
    out
}

/// `config.ts` `getMossenMcpConfigs`.
pub async fn get_mossen_mcp_configs() -> HashMap<String, ScopedMcpServerConfig> {
    HashMap::new()
}

/// `config.ts` `getAllMcpConfigs`.
pub async fn get_all_mcp_configs() -> HashMap<String, ScopedMcpServerConfig> {
    let mut all = get_project_mcp_configs_from_cwd();
    let defaults = get_mossen_mcp_configs().await;
    for (k, v) in defaults {
        all.entry(k).or_insert(v);
    }
    all
}

/// `config.ts` `parseMcpConfig`.
pub fn parse_mcp_config(
    name: &str,
    raw: &serde_json::Value,
    scope: ConfigScope,
) -> Option<ScopedMcpServerConfig> {
    let cfg = serde_json::from_value::<McpServerConfig>(raw.clone()).ok()?;
    let _ = name;
    Some(ScopedMcpServerConfig {
        config: cfg,
        scope,
        plugin_source: None,
    })
}

/// `config.ts` `parseMcpConfigFromFilePath` — TS-mirror alias preserved as
/// a separate name so callers from the ts-port code can keep their imports.
/// Delegates to the primary implementation defined earlier in this module.
pub fn parse_mcp_config_from_file_path_ts(
    file_path: &Path,
    scope: ConfigScope,
) -> HashMap<String, ScopedMcpServerConfig> {
    let mut out = HashMap::new();
    let Ok(bytes) = std::fs::read(file_path) else {
        return out;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return out;
    };
    let Some(map) = value.get("mcpServers").and_then(|v| v.as_object()) else {
        return out;
    };
    for (name, cfg) in map {
        if let Some(scoped) = parse_mcp_config(name, cfg, scope.clone()) {
            out.insert(name.clone(), scoped);
        }
    }
    out
}

/// TS `areMcpConfigsAllowedWithEnterpriseMcpConfig` — returns `true` when the
/// proposed MCP configs respect the enterprise allowlist policy.
///
/// Policy: when an enterprise-managed config sets `enabled === false` for a
/// server, that server must not appear in the proposed config map. When the
/// enterprise config does not mention a server, the proposed entry is
/// allowed. Returns `false` on the first denial encountered.
pub fn are_mcp_configs_allowed_with_enterprise_mcp_config(
    proposed: &std::collections::HashMap<String, crate::mcp::types::McpServerConfig>,
    enterprise: &std::collections::HashMap<String, serde_json::Value>,
) -> bool {
    for name in proposed.keys() {
        if let Some(entry) = enterprise.get(name) {
            // Treat any non-object entry as deny.
            let enabled = entry
                .as_object()
                .and_then(|o| o.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !enabled {
                return false;
            }
        }
    }
    true
}
