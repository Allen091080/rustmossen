//! Plugin identifier parsing and building.
//!
//! Translated from `utils/plugins/pluginIdentifier.ts` (123 lines).

use super::schemas::{PluginScope, ALLOWED_OFFICIAL_MARKETPLACE_NAMES};

/// Extended scope type that includes 'flag' for session-only plugins.
/// 'flag' scope is NOT persisted to installed_plugins.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtendedPluginScope {
    Managed,
    User,
    Project,
    Local,
    Flag,
}

impl std::fmt::Display for ExtendedPluginScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtendedPluginScope::Managed => write!(f, "managed"),
            ExtendedPluginScope::User => write!(f, "user"),
            ExtendedPluginScope::Project => write!(f, "project"),
            ExtendedPluginScope::Local => write!(f, "local"),
            ExtendedPluginScope::Flag => write!(f, "flag"),
        }
    }
}

impl From<PluginScope> for ExtendedPluginScope {
    fn from(s: PluginScope) -> Self {
        match s {
            PluginScope::Managed => ExtendedPluginScope::Managed,
            PluginScope::User => ExtendedPluginScope::User,
            PluginScope::Project => ExtendedPluginScope::Project,
            PluginScope::Local => ExtendedPluginScope::Local,
        }
    }
}

/// Scopes that are persisted to installed_plugins.json.
pub type PersistablePluginScope = PluginScope;

/// Setting source identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingSource {
    PolicySettings,
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
}

/// Editable setting sources (excludes policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditableSettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
}

/// Map from SettingSource to ExtendedPluginScope.
pub fn setting_source_to_extended_scope(source: SettingSource) -> ExtendedPluginScope {
    match source {
        SettingSource::PolicySettings => ExtendedPluginScope::Managed,
        SettingSource::UserSettings => ExtendedPluginScope::User,
        SettingSource::ProjectSettings => ExtendedPluginScope::Project,
        SettingSource::LocalSettings => ExtendedPluginScope::Local,
        SettingSource::FlagSettings => ExtendedPluginScope::Flag,
    }
}

/// Parsed plugin identifier with name and optional marketplace.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPluginIdentifier {
    pub name: String,
    pub marketplace: Option<String>,
}

/// Parse a plugin identifier string into name and marketplace components.
///
/// Only the first '@' is used as separator. If the input contains multiple '@' symbols
/// (e.g., "plugin@market@place"), everything after the second '@' is ignored.
pub fn parse_plugin_identifier(plugin: &str) -> ParsedPluginIdentifier {
    if plugin.contains('@') {
        let parts: Vec<&str> = plugin.splitn(2, '@').collect();
        ParsedPluginIdentifier {
            name: parts.first().unwrap_or(&"").to_string(),
            marketplace: parts.get(1).map(|s| s.to_string()),
        }
    } else {
        ParsedPluginIdentifier {
            name: plugin.to_string(),
            marketplace: None,
        }
    }
}

/// Build a plugin ID from name and marketplace.
pub fn build_plugin_id(name: &str, marketplace: Option<&str>) -> String {
    match marketplace {
        Some(m) => format!("{}@{}", name, m),
        None => name.to_string(),
    }
}

/// Check if a marketplace name is an official (Mossen-controlled) marketplace.
pub fn is_official_marketplace_name(marketplace: Option<&str>) -> bool {
    match marketplace {
        Some(m) => ALLOWED_OFFICIAL_MARKETPLACE_NAMES.contains(m.to_lowercase().as_str()),
        None => false,
    }
}

/// Convert a plugin scope to its corresponding editable setting source.
/// Returns Err if scope is 'managed' (cannot install plugins to managed scope).
pub fn scope_to_setting_source(scope: PluginScope) -> Result<EditableSettingSource, String> {
    match scope {
        PluginScope::Managed => Err("Cannot install plugins to managed scope".into()),
        PluginScope::User => Ok(EditableSettingSource::UserSettings),
        PluginScope::Project => Ok(EditableSettingSource::ProjectSettings),
        PluginScope::Local => Ok(EditableSettingSource::LocalSettings),
    }
}

/// Convert an editable setting source to its corresponding plugin scope.
pub fn editable_setting_source_to_scope(source: EditableSettingSource) -> PluginScope {
    match source {
        EditableSettingSource::UserSettings => PluginScope::User,
        EditableSettingSource::ProjectSettings => PluginScope::Project,
        EditableSettingSource::LocalSettings => PluginScope::Local,
    }
}
