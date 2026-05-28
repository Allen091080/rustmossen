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
                .map(&get_marketplace_source_display)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn describe_plugin_sources_merges_declared_known_and_official_status() {
        let official_source = MarketplaceSource::GitHub {
            repo: "mossen/mossen-plugins-official".to_string(),
            git_ref: None,
            path: None,
            sparse_paths: None,
        };
        let custom_source = MarketplaceSource::Directory {
            path: "/tmp/custom-market".to_string(),
        };

        let status = describe_plugin_sources(
            || {
                HashMap::from([(
                    "custom".to_string(),
                    DeclaredMarketplaceInfo {
                        source: Some(custom_source.clone()),
                        auto_update: Some(false),
                        source_is_fallback: Some(true),
                    },
                )])
            },
            async {
                HashMap::from([
                    (
                        OFFICIAL_MARKETPLACE_NAME.to_string(),
                        KnownMarketplaceInfo {
                            source: Some(official_source.clone()),
                            install_location: Some("/tmp/official-cache".to_string()),
                            auto_update: Some(true),
                        },
                    ),
                    (
                        "custom".to_string(),
                        KnownMarketplaceInfo {
                            source: Some(custom_source.clone()),
                            install_location: Some("/tmp/custom-cache".to_string()),
                            auto_update: Some(false),
                        },
                    ),
                ])
            },
            |source| match source {
                MarketplaceSource::GitHub { repo, .. } => repo.clone(),
                MarketplaceSource::Directory { path } => path.clone(),
                other => format!("{other:?}"),
            },
            || "/tmp/plugins".to_string(),
            || "/tmp/plugins/marketplaces".to_string(),
            || vec!["/tmp/seed".to_string()],
            &official_source,
        )
        .await;

        assert_eq!(status.plugin_root, "/tmp/plugins");
        assert_eq!(status.marketplace_cache_dir, "/tmp/plugins/marketplaces");
        assert_eq!(status.seed_dirs, vec!["/tmp/seed".to_string()]);
        assert_eq!(status.official_marketplace.name, OFFICIAL_MARKETPLACE_NAME);
        assert_eq!(
            status.official_marketplace.source_display,
            "mossen/mossen-plugins-official"
        );
        assert!(status.official_marketplace.known);

        let custom = status
            .entries
            .iter()
            .find(|entry| entry.name == "custom")
            .expect("custom marketplace entry");
        assert!(custom.declared);
        assert!(custom.known);
        assert_eq!(custom.source_display, "/tmp/custom-market");
        assert_eq!(
            custom.install_location.as_deref(),
            Some("/tmp/custom-cache")
        );
        assert_eq!(custom.auto_update, Some(false));
        assert_eq!(custom.source_is_fallback, Some(true));

        assert!(status
            .suggested_commands
            .iter()
            .any(|command| command == "/plugin status"));
    }
}
