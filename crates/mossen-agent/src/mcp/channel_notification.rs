//! Channel notifications — lets an MCP server push user messages into the conversation.
//!
//! Translates `services/mcp/channelNotification.ts`.

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::mcp::channel_allowlist::{
    get_channel_allowlist, is_channels_enabled, parse_plugin_identifier, ChannelAllowlistEntry,
};

pub const CHANNEL_MESSAGE_METHOD: &str = "notifications/mossen/channel";
pub const CHANNEL_CAPABILITY_KEY: &str = "mossen/channel";
pub const CHANNEL_PERMISSION_CAPABILITY_KEY: &str = "mossen/channel/permission";
pub const CHANNEL_PERMISSION_METHOD: &str = "notifications/mossen/channel/permission";
pub const CHANNEL_PERMISSION_REQUEST_METHOD: &str = "notifications/mossen/channel/permission_request";
pub const CHANNEL_TAG: &str = "channel";

/// Channel message notification params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessageParams {
    pub content: String,
    pub meta: Option<HashMap<String, String>>,
}

/// Channel permission notification params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPermissionParams {
    pub request_id: String,
    pub behavior: PermissionBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
}

/// Outbound permission request params (CC -> server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPermissionRequestParams {
    pub request_id: String,
    pub tool_name: String,
    pub description: String,
    pub input_preview: String,
}

/// Safe meta key regex — only accept keys that look like plain identifiers.
fn is_safe_meta_key(key: &str) -> bool {
    lazy_static::lazy_static! {
        static ref SAFE_META_KEY: Regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    }
    SAFE_META_KEY.is_match(key)
}

/// Escape XML attribute value.
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Wrap a channel message in XML tags.
pub fn wrap_channel_message(
    server_name: &str,
    content: &str,
    meta: Option<&HashMap<String, String>>,
) -> String {
    let attrs: String = meta
        .map(|m| {
            m.iter()
                .filter(|(k, _)| is_safe_meta_key(k))
                .map(|(k, v)| format!(" {}=\"{}\"", k, escape_xml_attr(v)))
                .collect::<String>()
        })
        .unwrap_or_default();
    format!(
        "<{} source=\"{}\"{}>\n{}\n</{}>",
        CHANNEL_TAG,
        escape_xml_attr(server_name),
        attrs,
        content,
        CHANNEL_TAG
    )
}

/// Subscription type for org policy checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionType {
    Individual,
    Team,
    Enterprise,
}

/// Effective allowlist for the current session.
pub struct EffectiveAllowlist {
    pub entries: Vec<ChannelAllowlistEntry>,
    pub source: AllowlistSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowlistSource {
    Org,
    Ledger,
}

/// Get effective channel allowlist.
pub fn get_effective_channel_allowlist(
    sub: SubscriptionType,
    org_list: Option<&[ChannelAllowlistEntry]>,
    ledger_raw: &serde_json::Value,
) -> EffectiveAllowlist {
    if (sub == SubscriptionType::Team || sub == SubscriptionType::Enterprise)
        && org_list.is_some()
    {
        EffectiveAllowlist {
            entries: org_list.unwrap().to_vec(),
            source: AllowlistSource::Org,
        }
    } else {
        EffectiveAllowlist {
            entries: get_channel_allowlist(ledger_raw),
            source: AllowlistSource::Ledger,
        }
    }
}

/// Channel entry from --channels CLI argument.
#[derive(Debug, Clone)]
pub struct ChannelEntry {
    pub kind: ChannelEntryKind,
    pub name: String,
    pub marketplace: Option<String>,
    pub dev: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEntryKind {
    Server,
    Plugin,
}

/// Gate result for channel server.
#[derive(Debug, Clone)]
pub enum ChannelGateResult {
    Register,
    Skip {
        kind: ChannelGateSkipKind,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelGateSkipKind {
    Capability,
    Disabled,
    Auth,
    Policy,
    Session,
    Marketplace,
    Allowlist,
}

/// Match a connected MCP server against the user's parsed --channels entries.
pub fn find_channel_entry<'a>(
    server_name: &str,
    channels: &'a [ChannelEntry],
) -> Option<&'a ChannelEntry> {
    let parts: Vec<&str> = server_name.split(':').collect();
    channels.iter().find(|c| match c.kind {
        ChannelEntryKind::Server => server_name == c.name,
        ChannelEntryKind::Plugin => parts.first() == Some(&"plugin") && parts.get(1) == Some(&c.name.as_str()),
    })
}

/// Capabilities structure for gate checking.
pub struct ServerCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
}

/// Gate an MCP server's channel-notification path.
pub fn gate_channel_server(
    server_name: &str,
    capabilities: Option<&ServerCapabilities>,
    plugin_source: Option<&str>,
    channels_enabled: bool,
    has_oauth_token: bool,
    sub: SubscriptionType,
    channels_policy_enabled: Option<bool>,
    allowed_channels: &[ChannelEntry],
    org_allowlist: Option<&[ChannelAllowlistEntry]>,
    ledger_raw: &serde_json::Value,
) -> ChannelGateResult {
    // Check capability
    let has_channel_cap = capabilities
        .and_then(|c| c.experimental.as_ref())
        .map(|exp| exp.contains_key(CHANNEL_CAPABILITY_KEY))
        .unwrap_or(false);

    if !has_channel_cap {
        return ChannelGateResult::Skip {
            kind: ChannelGateSkipKind::Capability,
            reason: "server did not declare mossen/channel capability".to_string(),
        };
    }

    if !channels_enabled {
        return ChannelGateResult::Skip {
            kind: ChannelGateSkipKind::Disabled,
            reason: "channels feature is not currently available".to_string(),
        };
    }

    if !has_oauth_token {
        return ChannelGateResult::Skip {
            kind: ChannelGateSkipKind::Auth,
            reason: "channels requires a hosted session on the current backend".to_string(),
        };
    }

    // Team/Enterprise opt-in
    let managed = sub == SubscriptionType::Team || sub == SubscriptionType::Enterprise;
    if managed && channels_policy_enabled != Some(true) {
        return ChannelGateResult::Skip {
            kind: ChannelGateSkipKind::Policy,
            reason: "channels not enabled by org policy (set channelsEnabled: true in managed settings)".to_string(),
        };
    }

    // Session opt-in
    let entry = match find_channel_entry(server_name, allowed_channels) {
        Some(e) => e,
        None => {
            return ChannelGateResult::Skip {
                kind: ChannelGateSkipKind::Session,
                reason: format!(
                    "server {} not in --channels list for this session",
                    server_name
                ),
            };
        }
    };

    if entry.kind == ChannelEntryKind::Plugin {
        // Marketplace verification
        let actual = plugin_source.map(|s| parse_plugin_identifier(s).marketplace).flatten();
        if actual.as_deref() != entry.marketplace.as_deref() {
            return ChannelGateResult::Skip {
                kind: ChannelGateSkipKind::Marketplace,
                reason: format!(
                    "you asked for plugin:{}@{} but the installed {} plugin is from {}",
                    entry.name,
                    entry.marketplace.as_deref().unwrap_or("?"),
                    entry.name,
                    actual.as_deref().unwrap_or("an unknown source")
                ),
            };
        }

        // Allowlist check
        if !entry.dev {
            let effective = get_effective_channel_allowlist(sub, org_allowlist, ledger_raw);
            let is_allowed = effective.entries.iter().any(|e| {
                e.plugin == entry.name
                    && Some(e.marketplace.as_str()) == entry.marketplace.as_deref()
            });
            if !is_allowed {
                let reason = match effective.source {
                    AllowlistSource::Org => format!(
                        "plugin {}@{} is not on your org's approved channels list (set allowedChannelPlugins in managed settings)",
                        entry.name,
                        entry.marketplace.as_deref().unwrap_or("?")
                    ),
                    AllowlistSource::Ledger => format!(
                        "plugin {}@{} is not on the approved channels allowlist (use --dangerously-load-development-channels for local dev)",
                        entry.name,
                        entry.marketplace.as_deref().unwrap_or("?")
                    ),
                };
                return ChannelGateResult::Skip {
                    kind: ChannelGateSkipKind::Allowlist,
                    reason,
                };
            }
        }
    } else {
        // server-kind: allowlist schema is {marketplace, plugin} — a server entry can never match.
        if !entry.dev {
            return ChannelGateResult::Skip {
                kind: ChannelGateSkipKind::Allowlist,
                reason: format!(
                    "server {} is not on the approved channels allowlist (use --dangerously-load-development-channels for local dev)",
                    entry.name
                ),
            };
        }
    }

    ChannelGateResult::Register
}

// === Notification schemas (TS exports two Zod `lazySchema`s) ===

/// Validator for a channel-message notification payload.
/// Mirrors TS `ChannelMessageNotificationSchema`.
pub struct ChannelMessageNotificationSchema;

impl ChannelMessageNotificationSchema {
    /// Validate that `value` matches a JSON-RPC notification with
    /// `method == CHANNEL_MESSAGE_METHOD` and a `params` object.
    pub fn parse(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        let obj = value.as_object().ok_or("expected object")?;
        let method = obj
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or("missing method")?;
        if method != CHANNEL_MESSAGE_METHOD {
            return Err(format!(
                "expected method={CHANNEL_MESSAGE_METHOD}, got {method}"
            ));
        }
        let params = obj.get("params").ok_or("missing params")?;
        if !params.is_object() {
            return Err("params must be object".to_string());
        }
        Ok(value.clone())
    }
}

/// Validator for a channel-permission notification payload.
/// Mirrors TS `ChannelPermissionNotificationSchema`.
pub struct ChannelPermissionNotificationSchema;

impl ChannelPermissionNotificationSchema {
    pub fn parse(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        let obj = value.as_object().ok_or("expected object")?;
        let method = obj
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or("missing method")?;
        if method != CHANNEL_PERMISSION_METHOD {
            return Err(format!(
                "expected method={CHANNEL_PERMISSION_METHOD}, got {method}"
            ));
        }
        let params = obj.get("params").ok_or("missing params")?;
        if !params.is_object() {
            return Err("params must be object".to_string());
        }
        Ok(value.clone())
    }
}
