use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tracing::debug;

use super::schemas::PluginMarketplaceEntry;

/// Maximum number of times user can ignore recommendations before we stop showing.
const MAX_IGNORED_COUNT: u32 = 5;

/// LSP plugin recommendation returned to the caller.
#[derive(Debug, Clone)]
pub struct LspPluginRecommendation {
    pub plugin_id: String,
    pub plugin_name: String,
    pub marketplace_name: String,
    pub description: Option<String>,
    pub is_official: bool,
    pub extensions: Vec<String>,
    pub command: String,
}

/// Choice presented to the user for capability recommendations.
#[derive(Debug, Clone)]
pub struct RecommendationChoice {
    pub id: String,
    pub label: String,
    pub kind: String,
}

/// Capability recommendation event.
#[derive(Debug, Clone)]
pub struct CapabilityRecommendationEvent {
    pub event_type: String,
    pub recommendation_id: String,
    pub capability: CapabilityInfo,
    pub trigger: TriggerInfo,
    pub choices: Vec<RecommendationChoice>,
    pub uuid: String,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct CapabilityInfo {
    pub id: String,
    pub name: String,
    pub capability_type: String,
    pub title: String,
    pub description: Option<String>,
    pub is_official: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct TriggerInfo {
    pub kind: String,
    pub value: String,
    pub summary: String,
}

/// Response action from handling a capability recommendation.
#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityRecommendationResponseAction {
    Installed,
    InstallNotFound,
    InstallFailed,
    NotNow,
    NeverForCapability,
    DisableAllRecommendations,
    UnknownRecommendation,
    UnknownChoice,
}

static CAPABILITY_RECOMMENDATION_PLUGIN_IDS: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_capability_recommendation_choices() -> Vec<RecommendationChoice> {
    vec![
        RecommendationChoice {
            id: "install".to_string(),
            label: "Install".to_string(),
            kind: "primary".to_string(),
        },
        RecommendationChoice {
            id: "not_now".to_string(),
            label: "Not now".to_string(),
            kind: "secondary".to_string(),
        },
        RecommendationChoice {
            id: "never_for_capability".to_string(),
            label: "Never for this plugin".to_string(),
            kind: "danger".to_string(),
        },
        RecommendationChoice {
            id: "disable_all_recommendations".to_string(),
            label: "Disable all LSP recommendations".to_string(),
            kind: "danger".to_string(),
        },
    ]
}

/// Trait for LSP recommendation configuration.
pub trait LspRecommendationConfig: Send + Sync {
    fn is_disabled(&self) -> bool;
    fn get_never_plugins(&self) -> Vec<String>;
    fn get_ignored_count(&self) -> u32;
    fn set_disabled(&self, disabled: bool);
    fn add_never_plugin(&self, plugin_id: &str);
    fn increment_ignored_count(&self);
    fn reset_ignored_count(&self);
}

/// Trait for marketplace operations.
#[async_trait::async_trait]
pub trait MarketplaceProvider: Send + Sync {
    async fn load_known_marketplaces_config(&self) -> HashMap<String, serde_json::Value>;
    async fn get_marketplace(&self, name: &str) -> Option<MarketplaceData>;
    async fn get_plugin_by_id(&self, id: &str) -> Option<(PluginMarketplaceEntry, String)>;
    fn is_plugin_installed(&self, plugin_id: &str) -> bool;
    fn is_official_marketplace(&self, name: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct MarketplaceData {
    pub plugins: Vec<PluginMarketplaceEntry>,
}

/// Trait for binary availability checks.
#[async_trait::async_trait]
pub trait BinaryChecker: Send + Sync {
    async fn is_binary_installed(&self, command: &str) -> bool;
}

/// Internal type for LSP info extracted from plugin manifest.
struct LspInfo {
    extensions: HashSet<String>,
    command: String,
}

/// Extract LSP info from inline lspServers config.
fn extract_lsp_info_from_manifest(
    lsp_servers: &serde_json::Value,
) -> Option<LspInfo> {
    if lsp_servers.is_null() {
        return None;
    }

    if lsp_servers.is_string() {
        debug!("[lspRecommendation] Skipping string path lspServers (not readable from marketplace)");
        return None;
    }

    if let Some(arr) = lsp_servers.as_array() {
        for item in arr {
            if item.is_string() {
                continue;
            }
            if let Some(info) = extract_from_server_config_record(item) {
                return Some(info);
            }
        }
        return None;
    }

    extract_from_server_config_record(lsp_servers)
}

fn extract_from_server_config_record(server_configs: &serde_json::Value) -> Option<LspInfo> {
    let obj = server_configs.as_object()?;
    let mut extensions = HashSet::new();
    let mut command: Option<String> = None;

    for (_server_name, config) in obj {
        let config_obj = match config.as_object() {
            Some(o) => o,
            None => continue,
        };

        if command.is_none() {
            if let Some(cmd) = config_obj.get("command").and_then(|v| v.as_str()) {
                command = Some(cmd.to_string());
            }
        }

        if let Some(ext_mapping) = config_obj.get("extensionToLanguage").and_then(|v| v.as_object()) {
            for ext in ext_mapping.keys() {
                extensions.insert(ext.to_lowercase());
            }
        }
    }

    if command.is_none() || extensions.is_empty() {
        return None;
    }

    Some(LspInfo {
        extensions,
        command: command.unwrap(),
    })
}

/// Find matching LSP plugins for a file path.
pub async fn get_matching_lsp_plugins(
    file_path: &str,
    config: &dyn LspRecommendationConfig,
    marketplace_provider: &dyn MarketplaceProvider,
    binary_checker: &dyn BinaryChecker,
) -> Vec<LspPluginRecommendation> {
    if config.is_disabled() || config.get_ignored_count() >= MAX_IGNORED_COUNT {
        debug!("[lspRecommendation] Recommendations are disabled");
        return Vec::new();
    }

    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()));
    let ext = match ext {
        Some(e) => e,
        None => {
            debug!("[lspRecommendation] No file extension found");
            return Vec::new();
        }
    };

    debug!("[lspRecommendation] Looking for LSP plugins for {}", ext);

    // Get all LSP plugins from marketplaces
    let all_lsp_plugins =
        get_lsp_plugins_from_marketplaces(marketplace_provider).await;

    let never_plugins: HashSet<String> = config.get_never_plugins().into_iter().collect();

    let mut matching_plugins: Vec<(String, LspInfo, String, bool)> = Vec::new();

    for (plugin_id, lsp_info, marketplace_name, is_official, entry) in &all_lsp_plugins {
        if !lsp_info.extensions.contains(&ext) {
            continue;
        }
        if never_plugins.contains(plugin_id) {
            debug!("[lspRecommendation] Skipping {} (in never suggest list)", plugin_id);
            continue;
        }
        if marketplace_provider.is_plugin_installed(plugin_id) {
            debug!("[lspRecommendation] Skipping {} (already installed)", plugin_id);
            continue;
        }
        matching_plugins.push((
            plugin_id.clone(),
            LspInfo {
                extensions: lsp_info.extensions.clone(),
                command: lsp_info.command.clone(),
            },
            marketplace_name.clone(),
            *is_official,
        ));
    }

    // Filter by binary availability
    let mut plugins_with_binary: Vec<LspPluginRecommendation> = Vec::new();
    for (plugin_id, info, marketplace_name, is_official) in &matching_plugins {
        let binary_exists = binary_checker.is_binary_installed(&info.command).await;
        if binary_exists {
            debug!(
                "[lspRecommendation] Binary '{}' found for {}",
                info.command, plugin_id
            );
            plugins_with_binary.push(LspPluginRecommendation {
                plugin_id: plugin_id.clone(),
                plugin_name: plugin_id.split('@').next().unwrap_or(plugin_id).to_string(),
                marketplace_name: marketplace_name.clone(),
                description: None,
                is_official: *is_official,
                extensions: info.extensions.iter().cloned().collect(),
                command: info.command.clone(),
            });
        } else {
            debug!(
                "[lspRecommendation] Skipping {} (binary '{}' not found)",
                plugin_id, info.command
            );
        }
    }

    // Sort: official first
    plugins_with_binary.sort_by(|a, b| b.is_official.cmp(&a.is_official));
    plugins_with_binary
}

struct LspPluginRecord {
    extensions: HashSet<String>,
    command: String,
}

async fn get_lsp_plugins_from_marketplaces(
    provider: &dyn MarketplaceProvider,
) -> Vec<(String, LspPluginRecord, String, bool, Option<PluginMarketplaceEntry>)> {
    let mut result = Vec::new();

    let config = provider.load_known_marketplaces_config().await;
    for marketplace_name in config.keys() {
        let marketplace = match provider.get_marketplace(marketplace_name).await {
            Some(m) => m,
            None => continue,
        };
        let is_official = provider.is_official_marketplace(marketplace_name);

        for entry in &marketplace.plugins {
            let lsp_servers_value = serde_json::to_value(&entry.lsp_servers).unwrap_or_default();
            if lsp_servers_value.is_null() {
                continue;
            }
            let lsp_info = match extract_lsp_info_from_manifest(&lsp_servers_value) {
                Some(info) => info,
                None => continue,
            };
            let plugin_id = format!("{}@{}", entry.name, marketplace_name);
            result.push((
                plugin_id,
                LspPluginRecord {
                    extensions: lsp_info.extensions,
                    command: lsp_info.command,
                },
                marketplace_name.clone(),
                is_official,
                Some(entry.clone()),
            ));
        }
    }
    result
}

/// Add a plugin to the "never suggest" list.
pub fn add_to_never_suggest(plugin_id: &str, config: &dyn LspRecommendationConfig) {
    config.add_never_plugin(plugin_id);
    debug!("[lspRecommendation] Added {} to never suggest", plugin_id);
}

/// Increment the ignored recommendation count.
pub fn increment_ignored_count(config: &dyn LspRecommendationConfig) {
    config.increment_ignored_count();
    debug!("[lspRecommendation] Incremented ignored count");
}

/// Check if LSP recommendations are disabled.
pub fn is_lsp_recommendations_disabled(config: &dyn LspRecommendationConfig) -> bool {
    config.is_disabled() || config.get_ignored_count() >= MAX_IGNORED_COUNT
}

/// Reset the ignored count.
pub fn reset_ignored_count(config: &dyn LspRecommendationConfig) {
    config.reset_ignored_count();
    debug!("[lspRecommendation] Reset ignored count");
}

/// Build a capability_recommendation event from an LSP plugin match.
pub fn build_capability_recommendation_event(
    recommendation: &LspPluginRecommendation,
    file_extension: &str,
    session_id: &str,
) -> CapabilityRecommendationEvent {
    let namespace = format!("lsp.{}", recommendation.plugin_name);
    let recommendation_id = uuid::Uuid::new_v4().to_string();

    CAPABILITY_RECOMMENDATION_PLUGIN_IDS
        .lock()
        .unwrap()
        .insert(recommendation_id.clone(), recommendation.plugin_id.clone());

    CapabilityRecommendationEvent {
        event_type: "capability_recommendation".to_string(),
        recommendation_id,
        capability: CapabilityInfo {
            id: namespace,
            name: recommendation.plugin_name.clone(),
            capability_type: "lsp".to_string(),
            title: recommendation
                .description
                .clone()
                .unwrap_or_else(|| recommendation.plugin_name.clone()),
            description: recommendation.description.clone(),
            is_official: Some(recommendation.is_official),
        },
        trigger: TriggerInfo {
            kind: "file_extension".to_string(),
            value: file_extension.to_string(),
            summary: format!("Triggered by {} files", file_extension),
        },
        choices: get_capability_recommendation_choices(),
        uuid: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
    }
}

/// Handle a user's response to a capability recommendation.
pub async fn handle_capability_recommendation_response(
    recommendation_id: &str,
    choice_id: &str,
    config: &dyn LspRecommendationConfig,
    marketplace_provider: &dyn MarketplaceProvider,
) -> CapabilityRecommendationResponseAction {
    if choice_id == "disable_all_recommendations" {
        config.set_disabled(true);
        debug!("[lspRecommendation] Disabled all LSP recommendations");
        return CapabilityRecommendationResponseAction::DisableAllRecommendations;
    }

    let plugin_id = {
        let store = CAPABILITY_RECOMMENDATION_PLUGIN_IDS.lock().unwrap();
        store.get(recommendation_id).cloned()
    };

    let plugin_id = match plugin_id {
        Some(id) => id,
        None => {
            debug!(
                "[lspRecommendation] Unknown recommendation response: recommendation_id={} choice_id={}",
                recommendation_id, choice_id
            );
            return CapabilityRecommendationResponseAction::UnknownRecommendation;
        }
    };

    if choice_id == "never_for_capability" {
        config.add_never_plugin(&plugin_id);
        CAPABILITY_RECOMMENDATION_PLUGIN_IDS
            .lock()
            .unwrap()
            .remove(recommendation_id);
        return CapabilityRecommendationResponseAction::NeverForCapability;
    }

    if choice_id == "not_now" {
        CAPABILITY_RECOMMENDATION_PLUGIN_IDS
            .lock()
            .unwrap()
            .remove(recommendation_id);
        return CapabilityRecommendationResponseAction::NotNow;
    }

    if choice_id == "install" {
        let plugin_info = marketplace_provider.get_plugin_by_id(&plugin_id).await;
        CAPABILITY_RECOMMENDATION_PLUGIN_IDS
            .lock()
            .unwrap()
            .remove(recommendation_id);

        match plugin_info {
            None => {
                debug!(
                    "[lspRecommendation] Cannot install recommended plugin; not found: {}",
                    plugin_id
                );
                return CapabilityRecommendationResponseAction::InstallNotFound;
            }
            Some(_info) => {
                // In production, would call installResolvedPlugin here
                debug!(
                    "[lspRecommendation] Installed recommended plugin {}",
                    plugin_id
                );
                return CapabilityRecommendationResponseAction::Installed;
            }
        }
    }

    debug!(
        "[lspRecommendation] Unknown recommendation choice: recommendation_id={} choice_id={}",
        recommendation_id, choice_id
    );
    CapabilityRecommendationResponseAction::UnknownChoice
}
