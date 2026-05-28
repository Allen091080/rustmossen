use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::schemas::LspServerConfig;

/// Scoped LSP server config with additional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedLspServerConfig {
    #[serde(flatten)]
    pub config: LspServerConfig,
    pub scope: String,
    pub source: String,
}

/// Plugin error related to LSP configuration.
#[derive(Debug, Clone)]
pub struct LspPluginError {
    pub error_type: String,
    pub plugin: String,
    pub server_name: String,
    pub validation_error: String,
    pub source: String,
}

/// Validate that a resolved path stays within the plugin directory.
/// Prevents path traversal attacks via .. or absolute paths.
fn validate_path_within_plugin(plugin_path: &Path, relative_path: &str) -> Option<PathBuf> {
    let resolved_plugin = plugin_path
        .canonicalize()
        .unwrap_or_else(|_| plugin_path.to_path_buf());
    let resolved_file = plugin_path.join(relative_path);
    let resolved_file = resolved_file
        .canonicalize()
        .unwrap_or_else(|_| resolved_file.to_path_buf());

    if let Ok(rel) = resolved_file.strip_prefix(&resolved_plugin) {
        let rel_str = rel.to_string_lossy();
        if rel_str.starts_with("..") {
            return None;
        }
        Some(resolved_file)
    } else {
        None
    }
}

/// Load LSP server configurations from a plugin.
/// Checks for:
/// 1. .lsp.json file in plugin directory
/// 2. manifest.lspServers field
pub async fn load_plugin_lsp_servers(
    plugin_path: &Path,
    plugin_name: &str,
    manifest_lsp_servers: Option<&LspServersDeclaration>,
    errors: &mut Vec<LspPluginError>,
) -> Option<HashMap<String, LspServerConfig>> {
    let mut servers: HashMap<String, LspServerConfig> = HashMap::new();

    // 1. Check for .lsp.json file
    let lsp_json_path = plugin_path.join(".lsp.json");
    match tokio::fs::read_to_string(&lsp_json_path).await {
        Ok(content) => match serde_json::from_str::<HashMap<String, LspServerConfig>>(&content) {
            Ok(parsed) => {
                servers.extend(parsed);
            }
            Err(e) => {
                let error_msg = format!(
                    "LSP config validation failed for .lsp.json in plugin {}: {}",
                    plugin_name, e
                );
                debug!("{}", error_msg);
                errors.push(LspPluginError {
                    error_type: "lsp-config-invalid".to_string(),
                    plugin: plugin_name.to_string(),
                    server_name: ".lsp.json".to_string(),
                    validation_error: e.to_string(),
                    source: "plugin".to_string(),
                });
            }
        },
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                let error_msg = format!(
                    "Failed to read/parse .lsp.json in plugin {}: {}",
                    plugin_name, e
                );
                debug!("{}", error_msg);
                errors.push(LspPluginError {
                    error_type: "lsp-config-invalid".to_string(),
                    plugin: plugin_name.to_string(),
                    server_name: ".lsp.json".to_string(),
                    validation_error: format!("Failed to parse JSON: {}", e),
                    source: "plugin".to_string(),
                });
            }
        }
    }

    // 2. Check manifest.lspServers field
    if let Some(declaration) = manifest_lsp_servers {
        let manifest_servers =
            load_lsp_servers_from_manifest(declaration, plugin_path, plugin_name, errors).await;
        if let Some(ms) = manifest_servers {
            servers.extend(ms);
        }
    }

    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

/// Different formats for lspServers declaration in manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspServersDeclaration {
    FilePath(String),
    Inline(HashMap<String, LspServerConfig>),
    Array(Vec<LspServersDeclarationItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspServersDeclarationItem {
    FilePath(String),
    Inline(HashMap<String, LspServerConfig>),
}

/// Load LSP servers from manifest declaration.
async fn load_lsp_servers_from_manifest(
    declaration: &LspServersDeclaration,
    plugin_path: &Path,
    plugin_name: &str,
    errors: &mut Vec<LspPluginError>,
) -> Option<HashMap<String, LspServerConfig>> {
    let mut servers: HashMap<String, LspServerConfig> = HashMap::new();

    let items: Vec<&LspServersDeclarationItem> = match declaration {
        LspServersDeclaration::FilePath(path) => {
            let item = LspServersDeclarationItem::FilePath(path.clone());
            // Process single file path
            process_lsp_declaration_item(&item, plugin_path, plugin_name, errors, &mut servers)
                .await;
            return if servers.is_empty() {
                None
            } else {
                Some(servers)
            };
        }
        LspServersDeclaration::Inline(configs) => {
            servers.extend(configs.clone());
            return if servers.is_empty() {
                None
            } else {
                Some(servers)
            };
        }
        LspServersDeclaration::Array(items) => items.iter().collect(),
    };

    for item in items {
        process_lsp_declaration_item(item, plugin_path, plugin_name, errors, &mut servers).await;
    }

    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

async fn process_lsp_declaration_item(
    item: &LspServersDeclarationItem,
    plugin_path: &Path,
    plugin_name: &str,
    errors: &mut Vec<LspPluginError>,
    servers: &mut HashMap<String, LspServerConfig>,
) {
    match item {
        LspServersDeclarationItem::FilePath(path) => {
            let validated_path = match validate_path_within_plugin(plugin_path, path) {
                Some(p) => p,
                None => {
                    let msg = format!(
                        "Security: Path traversal attempt blocked in plugin {}: {}",
                        plugin_name, path
                    );
                    debug!("{}", msg);
                    errors.push(LspPluginError {
                        error_type: "lsp-config-invalid".to_string(),
                        plugin: plugin_name.to_string(),
                        server_name: path.clone(),
                        validation_error: "Invalid path: must be relative and within plugin directory".to_string(),
                        source: "plugin".to_string(),
                    });
                    return;
                }
            };

            match tokio::fs::read_to_string(&validated_path).await {
                Ok(content) => {
                    match serde_json::from_str::<HashMap<String, LspServerConfig>>(&content) {
                        Ok(parsed) => {
                            servers.extend(parsed);
                        }
                        Err(e) => {
                            errors.push(LspPluginError {
                                error_type: "lsp-config-invalid".to_string(),
                                plugin: plugin_name.to_string(),
                                server_name: path.clone(),
                                validation_error: e.to_string(),
                                source: "plugin".to_string(),
                            });
                        }
                    }
                }
                Err(e) => {
                    errors.push(LspPluginError {
                        error_type: "lsp-config-invalid".to_string(),
                        plugin: plugin_name.to_string(),
                        server_name: path.clone(),
                        validation_error: format!("Failed to parse JSON: {}", e),
                        source: "plugin".to_string(),
                    });
                }
            }
        }
        LspServersDeclarationItem::Inline(configs) => {
            servers.extend(configs.clone());
        }
    }
}

/// Resolve environment variables for plugin LSP servers.
/// Handles ${MOSSEN_PLUGIN_ROOT}, ${user_config.X}, and general ${VAR} substitution.
pub fn resolve_plugin_lsp_environment(
    config: &LspServerConfig,
    plugin_path: &str,
    plugin_source: &str,
    user_config: Option<&HashMap<String, serde_json::Value>>,
    get_plugin_data_dir: impl Fn(&str) -> String,
    expand_env_vars: impl Fn(&str) -> (String, Vec<String>),
) -> (LspServerConfig, Vec<String>) {
    let mut all_missing_vars: Vec<String> = Vec::new();

    let mut resolve_value = |value: &str| -> String {
        let mut resolved = super::plugin_options_storage::substitute_plugin_variables(
            value,
            plugin_path,
            Some(plugin_source),
            &get_plugin_data_dir,
        );

        if let Some(uc) = user_config {
            if let Ok(r) =
                super::plugin_options_storage::substitute_user_config_variables(&resolved, uc)
            {
                resolved = r;
            }
        }

        let (expanded, missing) = expand_env_vars(&resolved);
        all_missing_vars.extend(missing);
        expanded
    };

    let mut resolved = config.clone();

    resolved.command = resolve_value(&resolved.command);

    if let Some(ref args) = resolved.args {
        resolved.args = Some(args.iter().map(|a| resolve_value(a)).collect());
    }

    // Resolve env and add MOSSEN_PLUGIN_ROOT / MOSSEN_PLUGIN_DATA
    let mut resolved_env: HashMap<String, String> = HashMap::new();
    resolved_env.insert("MOSSEN_PLUGIN_ROOT".to_string(), plugin_path.to_string());
    resolved_env.insert(
        "MOSSEN_PLUGIN_DATA".to_string(),
        get_plugin_data_dir(plugin_source),
    );
    if let Some(ref env) = resolved.env {
        for (key, value) in env {
            if key != "MOSSEN_PLUGIN_ROOT" && key != "MOSSEN_PLUGIN_DATA" {
                resolved_env.insert(key.clone(), resolve_value(value));
            }
        }
    }
    resolved.env = Some(resolved_env);

    if let Some(ref wf) = resolved.workspace_folder {
        resolved.workspace_folder = Some(resolve_value(wf));
    }

    if !all_missing_vars.is_empty() {
        let unique: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            all_missing_vars
                .into_iter()
                .filter(|v| seen.insert(v.clone()))
                .collect()
        };
        debug!(
            "Missing environment variables in plugin LSP config: {}",
            unique.join(", ")
        );
        return (resolved, unique);
    }

    (resolved, Vec::new())
}

/// Add plugin scope to LSP server configs.
pub fn add_plugin_scope_to_lsp_servers(
    servers: &HashMap<String, LspServerConfig>,
    plugin_name: &str,
) -> HashMap<String, ScopedLspServerConfig> {
    let mut scoped = HashMap::new();
    for (name, config) in servers {
        let scoped_name = format!("plugin:{}:{}", plugin_name, name);
        scoped.insert(
            scoped_name,
            ScopedLspServerConfig {
                config: config.clone(),
                scope: "dynamic".to_string(),
                source: plugin_name.to_string(),
            },
        );
    }
    scoped
}

/// Get LSP servers from a specific plugin with environment variable resolution and scoping.
pub async fn get_plugin_lsp_servers(
    plugin_path: &Path,
    plugin_name: &str,
    plugin_source: &str,
    enabled: bool,
    cached_servers: Option<&HashMap<String, LspServerConfig>>,
    manifest_lsp_servers: Option<&LspServersDeclaration>,
    manifest_user_config: bool,
    user_config: Option<&HashMap<String, serde_json::Value>>,
    errors: &mut Vec<LspPluginError>,
    get_plugin_data_dir: impl Fn(&str) -> String + Clone,
    expand_env_vars: impl Fn(&str) -> (String, Vec<String>) + Clone,
) -> Option<HashMap<String, ScopedLspServerConfig>> {
    if !enabled {
        return None;
    }

    let servers = if let Some(cached) = cached_servers {
        cached.clone()
    } else {
        load_plugin_lsp_servers(plugin_path, plugin_name, manifest_lsp_servers, errors).await?
    };

    let uc = if manifest_user_config {
        user_config
    } else {
        None
    };

    let mut resolved_servers: HashMap<String, LspServerConfig> = HashMap::new();
    for (name, config) in &servers {
        let (resolved, _missing) = resolve_plugin_lsp_environment(
            config,
            &plugin_path.to_string_lossy(),
            plugin_source,
            uc,
            get_plugin_data_dir.clone(),
            expand_env_vars.clone(),
        );
        resolved_servers.insert(name.clone(), resolved);
    }

    Some(add_plugin_scope_to_lsp_servers(
        &resolved_servers,
        plugin_name,
    ))
}

/// Extract all LSP servers from loaded plugins.
pub async fn extract_lsp_servers_from_plugins(
    plugins: &[(PathBuf, String, String, bool, Option<LspServersDeclaration>)],
    errors: &mut Vec<LspPluginError>,
) -> HashMap<String, ScopedLspServerConfig> {
    let mut all_servers: HashMap<String, ScopedLspServerConfig> = HashMap::new();

    for (path, name, _source, enabled, manifest_lsp) in plugins {
        if !*enabled {
            continue;
        }

        let servers = load_plugin_lsp_servers(path, name, manifest_lsp.as_ref(), errors).await;
        if let Some(servers) = servers {
            let scoped = add_plugin_scope_to_lsp_servers(&servers, name);
            all_servers.extend(scoped);
            debug!("Loaded {} LSP servers from plugin {}", servers.len(), name);
        }
    }

    all_servers
}
