//! Channel allowlist — approved channel plugins.
//!
//! Translates `services/mcp/channelAllowlist.ts`.

use serde::{Deserialize, Serialize};

/// An entry in the channel allowlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelAllowlistEntry {
    pub marketplace: String,
    pub plugin: String,
}

/// Get the channel allowlist from feature flags.
///
/// In the TS version this reads from GrowthBook. Here we accept the raw value
/// as a parameter to keep the module pure.
pub fn get_channel_allowlist(raw: &serde_json::Value) -> Vec<ChannelAllowlistEntry> {
    match serde_json::from_value::<Vec<ChannelAllowlistEntry>>(raw.clone()) {
        Ok(entries) => entries,
        Err(_) => Vec::new(),
    }
}

/// Overall channels on/off gate.
/// When false, --channels is a no-op and no handlers register.
pub fn is_channels_enabled(feature_value: bool) -> bool {
    feature_value
}

/// Parsed plugin identifier.
pub struct ParsedPluginIdentifier {
    pub name: String,
    pub marketplace: Option<String>,
}

/// Parse a plugin identifier string like "name@marketplace".
pub fn parse_plugin_identifier(source: &str) -> ParsedPluginIdentifier {
    if let Some(at_pos) = source.rfind('@') {
        ParsedPluginIdentifier {
            name: source[..at_pos].to_string(),
            marketplace: Some(source[at_pos + 1..].to_string()),
        }
    } else {
        ParsedPluginIdentifier {
            name: source.to_string(),
            marketplace: None,
        }
    }
}

/// Pure allowlist check keyed off the connection's pluginSource.
///
/// Returns false for None pluginSource (non-plugin server) and for @-less sources.
pub fn is_channel_allowlisted(
    plugin_source: Option<&str>,
    allowlist: &[ChannelAllowlistEntry],
) -> bool {
    let source = match plugin_source {
        Some(s) => s,
        None => return false,
    };
    let parsed = parse_plugin_identifier(source);
    let marketplace = match &parsed.marketplace {
        Some(m) => m,
        None => return false,
    };
    allowlist
        .iter()
        .any(|e| e.plugin == parsed.name && e.marketplace == *marketplace)
}
