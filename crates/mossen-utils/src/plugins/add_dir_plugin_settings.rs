use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::schemas::SettingsJson;

const SETTINGS_FILES: &[&str] = &["settings.json", "settings.local.json"];

/// Returns a merged record of enabledPlugins from all --add-dir directories.
///
/// Within each directory, settings.local.json is processed after settings.json
/// (local wins within that dir). Across directories, later CLI-order wins on conflict.
pub fn get_add_dir_enabled_plugins(
    additional_dirs: &[PathBuf],
    parse_settings_file: impl Fn(&PathBuf) -> Option<SettingsJson>,
) -> HashMap<String, bool> {
    let mut result = HashMap::new();
    for dir in additional_dirs {
        for file in SETTINGS_FILES {
            let path = dir.join(".mossen").join(file);
            if let Some(settings) = parse_settings_file(&path) {
                if let Some(enabled_plugins) = settings.enabled_plugins {
                    for (key, value) in enabled_plugins {
                        result.insert(key, value);
                    }
                }
            }
        }
    }
    result
}

/// Represents an extra known marketplace entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraKnownMarketplace {
    pub source: Option<super::schemas::MarketplaceSource>,
    #[serde(rename = "autoUpdate")]
    pub auto_update: Option<bool>,
}

/// Returns a merged record of extraKnownMarketplaces from all --add-dir directories.
pub fn get_add_dir_extra_marketplaces(
    additional_dirs: &[PathBuf],
    parse_settings_file: impl Fn(&PathBuf) -> Option<SettingsJson>,
) -> HashMap<String, ExtraKnownMarketplace> {
    let mut result = HashMap::new();
    for dir in additional_dirs {
        for file in SETTINGS_FILES {
            let path = dir.join(".mossen").join(file);
            if let Some(settings) = parse_settings_file(&path) {
                if let Some(extra) = settings.extra_known_marketplaces {
                    for (key, value) in extra {
                        if let Ok(parsed) = serde_json::from_value::<ExtraKnownMarketplace>(value) {
                            result.insert(key, parsed);
                        }
                    }
                }
            }
        }
    }
    result
}
