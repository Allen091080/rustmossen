use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::official_marketplace::OFFICIAL_MARKETPLACE_NAME;
use super::schemas::MarketplaceSource;

/// A single marketplace source status entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSourceStatusEntry {
    pub name: String,
    pub declared: bool,
    pub known: bool,
    pub source_display: String,
    pub install_location: Option<String>,
    pub auto_update: Option<bool>,
    pub is_official: bool,
    pub source_is_fallback: Option<bool>,
}

/// The full plugin source status report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSourceStatus {
    pub plugin_root: String,
    pub marketplace_cache_dir: String,
    pub seed_dirs: Vec<String>,
    pub official_marketplace: OfficialMarketplaceStatus,
    pub entries: Vec<PluginSourceStatusEntry>,
    pub suggested_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialMarketplaceStatus {
    pub name: String,
    pub source_display: String,
    pub declared: bool,
    pub known: bool,
}

/// Describes the current state of all plugin marketplace sources.
///
/// Takes dependencies as parameters to avoid circular dependencies.
pub async fn describe_plugin_sources(
    get_declared_marketplaces: impl Fn() -> HashMap<String, DeclaredMarketplaceInfo>,
    load_known_marketplaces: impl std::future::Future<Output = HashMap<String, KnownMarketplaceInfo>>,
    get_marketplace_source_display: impl Fn(&MarketplaceSource) -> String,
    get_plugins_directory: impl Fn() -> String,
    get_marketplaces_cache_dir: impl Fn() -> String,
    get_plugin_seed_dirs: impl Fn() -> Vec<String>,
    official_source: &MarketplaceSource,
) -> PluginSourceStatus {
    let declared = get_declared_marketplaces();
    let known = load_known_marketplaces.await;

    let names: Vec<String> = {
        let mut set = HashSet::new();
        set.insert(OFFICIAL_MARKETPLACE_NAME.to_string());
        for k in declared.keys() {
            set.insert(k.clone());
        }
        for k in known.keys() {
            set.insert(k.clone());
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };

    let entries: Vec<PluginSourceStatusEntry> = names
        .iter()
        .map(|name| {
            let declared_entry = declared.get(name);
            let known_entry = known.get(name);
            let source = known_entry
                .and_then(|k| k.source.as_ref())
                .or_else(|| declared_entry.and_then(|d| d.source.as_ref()));
            let source_display = source
                .map(|s| get_marketplace_source_display(s))
                .unwrap_or_else(|| "(unknown)".to_string());
            let auto_update = known_entry
                .and_then(|k| k.auto_update)
                .or_else(|| declared_entry.and_then(|d| d.auto_update));

            PluginSourceStatusEntry {
                name: name.clone(),
                declared: declared_entry.is_some(),
                known: known_entry.is_some(),
                source_display,
                install_location: known_entry.and_then(|k| k.install_location.clone()),
                auto_update,
                is_official: name == OFFICIAL_MARKETPLACE_NAME,
                source_is_fallback: declared_entry.and_then(|d| d.source_is_fallback),
            }
        })
        .collect();

    let official = entries.iter().find(|e| e.name == OFFICIAL_MARKETPLACE_NAME);

    PluginSourceStatus {
        plugin_root: get_plugins_directory(),
        marketplace_cache_dir: get_marketplaces_cache_dir(),
        seed_dirs: get_plugin_seed_dirs(),
        official_marketplace: OfficialMarketplaceStatus {
            name: OFFICIAL_MARKETPLACE_NAME.to_string(),
            source_display: get_marketplace_source_display(official_source),
            declared: official.map(|o| o.declared).unwrap_or(false),
            known: official.map(|o| o.known).unwrap_or(false),
        },
        entries,
        suggested_commands: vec![
            "/plugin marketplace list".to_string(),
            "/plugin install <plugin>@<marketplace>".to_string(),
            format!("/plugin install <plugin>@{}", OFFICIAL_MARKETPLACE_NAME),
            "/plugin validate <path>".to_string(),
            "/plugin status".to_string(),
        ],
    }
}

/// Info about a declared marketplace from settings.
#[derive(Debug, Clone)]
pub struct DeclaredMarketplaceInfo {
    pub source: Option<MarketplaceSource>,
    pub auto_update: Option<bool>,
    pub source_is_fallback: Option<bool>,
}

/// Info about a known marketplace from the config file.
#[derive(Debug, Clone)]
pub struct KnownMarketplaceInfo {
    pub source: Option<MarketplaceSource>,
    pub install_location: Option<String>,
    pub auto_update: Option<bool>,
}
