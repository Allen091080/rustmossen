//! # channels — 频道通知与许可
//!
//! 对应 TypeScript:
//! - `services/mcp/channelNotification.ts`
//! - `services/mcp/channelAllowlist.ts`
//! - `services/mcp/channelPermissions.ts`
//!
//! 频道把 MCP 服务器变成可注入用户消息到对话流的通道（Discord/Slack/SMS
//! 等）。每个频道是一个标准 MCP server + `mossen/channel` 通知通道 +
//! 可选 `mossen/channel/permission` 通道（用于把权限对话回放给手机端）。

use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value as JsonValue};

// ---------------------------------------------------------------------------
// 常量 / 协议方法
// ---------------------------------------------------------------------------

/// `channelNotification.ts` `CHANNEL_MESSAGE_METHOD`。
pub const CHANNEL_MESSAGE_METHOD: &str = "notifications/mossen/channel";

/// `channelNotification.ts` `CHANNEL_CAPABILITY_KEY`。
pub const CHANNEL_CAPABILITY_KEY: &str = "mossen/channel";

/// `channelNotification.ts` `CHANNEL_PERMISSION_CAPABILITY_KEY`。
pub const CHANNEL_PERMISSION_CAPABILITY_KEY: &str = "mossen/channel/permission";

/// `channelNotification.ts` `CHANNEL_PERMISSION_METHOD`。
pub const CHANNEL_PERMISSION_METHOD: &str = "notifications/mossen/channel/permission";

/// `channelNotification.ts` `CHANNEL_PERMISSION_REQUEST_METHOD`。
pub const CHANNEL_PERMISSION_REQUEST_METHOD: &str =
    "notifications/mossen/channel/permission_request";

static SAFE_META_KEY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap());

/// 频道许可请求参数 — 对应 TS `ChannelPermissionRequestParams`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelPermissionRequestParams {
    pub request_id: String,
    pub tool_name: String,
    pub description: String,
    /// JSON 化的工具输入预览，截断到 200 字符。
    pub input_preview: String,
}

// ---------------------------------------------------------------------------
// wrapChannelMessage
// ---------------------------------------------------------------------------

/// `channelNotification.ts` `wrapChannelMessage`。
///
/// 把入站频道内容包装到 `<channel>` XML 标签里，附带 `source` 与
/// 元数据属性（仅接受形如 ident 的键名以避免属性注入）。
pub fn wrap_channel_message(
    server_name: &str,
    content: &str,
    meta: Option<&HashMap<String, String>>,
) -> String {
    let mut attrs = String::new();
    if let Some(map) = meta {
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for k in keys {
            if SAFE_META_KEY.is_match(k) {
                attrs.push_str(&format!(" {}=\"{}\"", k, escape_xml_attr(&map[k])));
            }
        }
    }
    format!(
        "<channel source=\"{}\"{}>\n{}\n</channel>",
        escape_xml_attr(server_name),
        attrs,
        content
    )
}

fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// channel allowlist & gate
// ---------------------------------------------------------------------------

/// `channelAllowlist.ts` `ChannelAllowlistEntry`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChannelAllowlistEntry {
    pub plugin: String,
    pub marketplace: String,
}

/// `channelNotification.ts` `ChannelEntry`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelEntry {
    Server {
        name: String,
        /// 开发模式：跳过允许列表校验。
        #[serde(default)]
        dev: bool,
    },
    Plugin {
        name: String,
        marketplace: String,
        #[serde(default)]
        dev: bool,
    },
}

impl ChannelEntry {
    pub fn name(&self) -> &str {
        match self {
            Self::Server { name, .. } | Self::Plugin { name, .. } => name,
        }
    }
    pub fn dev(&self) -> bool {
        match self {
            Self::Server { dev, .. } | Self::Plugin { dev, .. } => *dev,
        }
    }
}

/// `channelNotification.ts` `getEffectiveChannelAllowlist`。
///
/// 管理订阅（team/enterprise）+ 设置了 orgList 时优先使用 org 列表；
/// 否则用 GrowthBook ledger（由调用方注入）。
pub fn get_effective_channel_allowlist(
    sub: &str,
    org_list: Option<Vec<ChannelAllowlistEntry>>,
    ledger: Vec<ChannelAllowlistEntry>,
) -> (Vec<ChannelAllowlistEntry>, &'static str) {
    let managed = sub == "team" || sub == "enterprise";
    if managed {
        if let Some(list) = org_list {
            return (list, "org");
        }
    }
    (ledger, "ledger")
}

/// `channelNotification.ts` `findChannelEntry`。
pub fn find_channel_entry<'a>(
    server_name: &str,
    channels: &'a [ChannelEntry],
) -> Option<&'a ChannelEntry> {
    let parts: Vec<&str> = server_name.split(':').collect();
    channels.iter().find(|c| match c {
        ChannelEntry::Server { name, .. } => server_name == name,
        ChannelEntry::Plugin { name, .. } => parts.first() == Some(&"plugin") && parts.get(1) == Some(&name.as_str()),
    })
}

/// `channelNotification.ts` `ChannelGateResult`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelGateResult {
    Register,
    Skip { kind: SkipKind, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    Capability,
    Disabled,
    Auth,
    Policy,
    Session,
    Marketplace,
    Allowlist,
}

/// `gateChannelServer` 入口的不可注入参数集合 — 调用方注入 `is_channels_enabled`
/// 等环境状态。
pub struct GateInputs<'a> {
    pub server_name: &'a str,
    pub capabilities: Option<&'a JsonValue>,
    pub plugin_source: Option<&'a str>,
    pub channels_enabled: bool,
    pub has_hosted_oauth: bool,
    pub subscription: &'a str,
    pub policy_channels_enabled: Option<bool>,
    pub allowed_channels: &'a [ChannelEntry],
    pub effective_allowlist: Vec<ChannelAllowlistEntry>,
    pub allowlist_source: &'a str,
}

/// `channelNotification.ts` `gateChannelServer`。
pub fn gate_channel_server(inputs: GateInputs<'_>) -> ChannelGateResult {
    // capability gate
    let has_capability = inputs
        .capabilities
        .and_then(|c| c.get("experimental"))
        .and_then(|e| e.get(CHANNEL_CAPABILITY_KEY))
        .map(|v| !v.is_null() && !matches!(v, JsonValue::Bool(false)))
        .unwrap_or(false);
    if !has_capability {
        return ChannelGateResult::Skip {
            kind: SkipKind::Capability,
            reason: "server did not declare mossen/channel capability".into(),
        };
    }

    if !inputs.channels_enabled {
        return ChannelGateResult::Skip {
            kind: SkipKind::Disabled,
            reason: "channels feature is not currently available".into(),
        };
    }

    if !inputs.has_hosted_oauth {
        return ChannelGateResult::Skip {
            kind: SkipKind::Auth,
            reason: "channels requires a hosted session on the current backend".into(),
        };
    }

    let managed = inputs.subscription == "team" || inputs.subscription == "enterprise";
    if managed && inputs.policy_channels_enabled != Some(true) {
        return ChannelGateResult::Skip {
            kind: SkipKind::Policy,
            reason:
                "channels not enabled by org policy (set channelsEnabled: true in managed settings)"
                    .into(),
        };
    }

    let entry = match find_channel_entry(inputs.server_name, inputs.allowed_channels) {
        Some(e) => e,
        None => {
            return ChannelGateResult::Skip {
                kind: SkipKind::Session,
                reason: format!(
                    "server {} not in --channels list for this session",
                    inputs.server_name
                ),
            }
        }
    };

    match entry {
        ChannelEntry::Plugin {
            name,
            marketplace,
            dev,
        } => {
            // Marketplace verification
            let actual_marketplace = inputs
                .plugin_source
                .and_then(|src| src.split('@').nth(1))
                .map(|s| s.to_string());
            if actual_marketplace.as_deref() != Some(marketplace) {
                return ChannelGateResult::Skip {
                    kind: SkipKind::Marketplace,
                    reason: format!(
                        "you asked for plugin:{}@{} but the installed {} plugin is from {}",
                        name,
                        marketplace,
                        name,
                        actual_marketplace.as_deref().unwrap_or("an unknown source")
                    ),
                };
            }
            if !dev
                && !inputs.effective_allowlist.iter().any(|e| {
                    e.plugin == *name && e.marketplace == *marketplace
                })
            {
                let reason = if inputs.allowlist_source == "org" {
                    format!(
                        "plugin {}@{} is not on your org's approved channels list (set allowedChannelPlugins in managed settings)",
                        name, marketplace
                    )
                } else {
                    format!(
                        "plugin {}@{} is not on the approved channels allowlist (use --dangerously-load-development-channels for local dev)",
                        name, marketplace
                    )
                };
                return ChannelGateResult::Skip {
                    kind: SkipKind::Allowlist,
                    reason,
                };
            }
        }
        ChannelEntry::Server { name, dev } => {
            if !dev {
                return ChannelGateResult::Skip {
                    kind: SkipKind::Allowlist,
                    reason: format!(
                        "server {} is not on the approved channels allowlist (use --dangerously-load-development-channels for local dev)",
                        name
                    ),
                };
            }
        }
    }

    ChannelGateResult::Register
}

// ---------------------------------------------------------------------------
// channelPermissions.ts — 权限中继
// ---------------------------------------------------------------------------

/// `channelPermissions.ts` `isChannelPermissionRelayEnabled`。
///
/// Rust 端依赖调用方注入 feature/policy flag（避免循环依赖到 utils）。
pub fn is_channel_permission_relay_enabled(feature_enabled: bool) -> bool {
    feature_enabled
}

/// `channelPermissions.ts` `shortRequestId`。
///
/// 输入一个 request id，输出后 6 位（便于显示）。
pub fn short_request_id(request_id: &str) -> String {
    let len = request_id.chars().count();
    if len <= 6 {
        return request_id.to_string();
    }
    request_id
        .chars()
        .skip(len.saturating_sub(6))
        .collect::<String>()
}

/// `channelPermissions.ts` `truncateForPreview`。
///
/// 把字符串截断到 max_len 字符，超过部分用 `…` 替代。
pub fn truncate_for_preview(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        return s.to_string();
    }
    let head: String = chars.iter().take(max_len.saturating_sub(1)).collect();
    format!("{}…", head)
}

/// `channelPermissions.ts` `filterPermissionRelayClients`。
///
/// 过滤一组 MCP 客户端，仅保留声明 `mossen/channel/permission` 能力的项。
pub fn filter_permission_relay_clients(clients: &[JsonValue]) -> Vec<JsonValue> {
    clients
        .iter()
        .filter(|c| {
            c.get("capabilities")
                .and_then(|caps| caps.get("experimental"))
                .and_then(|e| e.get(CHANNEL_PERMISSION_CAPABILITY_KEY))
                .map(|v| !v.is_null() && !matches!(v, JsonValue::Bool(false)))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// `channelPermissions.ts` `buildPermissionRequestParams`。
pub fn build_permission_request_params(
    request_id: &str,
    tool_name: &str,
    description: &str,
    raw_input: &JsonValue,
) -> ChannelPermissionRequestParams {
    let preview_raw = serde_json::to_string(raw_input).unwrap_or_default();
    ChannelPermissionRequestParams {
        request_id: request_id.to_string(),
        tool_name: tool_name.to_string(),
        description: description.to_string(),
        input_preview: truncate_for_preview(&preview_raw, 200),
    }
}

/// `channelPermissions.ts` `relayPermissionDecision`。
///
/// 在内存中跟踪 request_id -> 决策；返回先前是否已记录过相同决策。
pub fn relay_permission_decision(request_id: &str, behavior: &str) -> bool {
    let mut s = relay_state().write().unwrap();
    if let Some(prev) = s.decisions.get(request_id) {
        if prev == behavior {
            return true;
        }
    }
    s.decisions.insert(request_id.to_string(), behavior.to_string());
    false
}

/// `channelPermissions.ts` `clearRelayState`。
pub fn clear_permission_relay_state() {
    let mut s = relay_state().write().unwrap();
    s.decisions.clear();
    s.pending.clear();
}

#[derive(Default)]
struct RelayState {
    decisions: HashMap<String, String>,
    pending: HashSet<String>,
}

fn relay_state() -> &'static std::sync::RwLock<RelayState> {
    use std::sync::{OnceLock, RwLock};
    static S: OnceLock<RwLock<RelayState>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(RelayState::default()))
}

/// `channelPermissions.ts` `markPendingPermissionRelay`。
pub fn mark_pending_permission_relay(request_id: &str) {
    relay_state()
        .write()
        .unwrap()
        .pending
        .insert(request_id.to_string());
}

/// `channelPermissions.ts` `isPendingPermissionRelay`。
pub fn is_pending_permission_relay(request_id: &str) -> bool {
    relay_state().read().unwrap().pending.contains(request_id)
}

// ---------------------------------------------------------------------------
// channelAllowlist.ts — 全局允许列表
// ---------------------------------------------------------------------------

static ALLOWLIST: once_cell::sync::Lazy<std::sync::RwLock<Vec<ChannelAllowlistEntry>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(Vec::new()));

/// `channelAllowlist.ts` `getChannelAllowlist`。
pub fn get_channel_allowlist() -> Vec<ChannelAllowlistEntry> {
    ALLOWLIST.read().unwrap().clone()
}

/// `channelAllowlist.ts` `setChannelAllowlist` — 调用方注入。
pub fn set_channel_allowlist(entries: Vec<ChannelAllowlistEntry>) {
    *ALLOWLIST.write().unwrap() = entries;
}

/// `channelAllowlist.ts` `isChannelsEnabled` — 调用方注入。
pub fn is_channels_enabled_default() -> bool {
    std::env::var("MOSSEN_CHANNELS_ENABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 给 channel notification 包一层基础 JSON 结构 — 便于上层直接喂给
/// 传输层。
pub fn build_channel_notification(content: &str, meta: Option<&HashMap<String, String>>) -> JsonValue {
    let mut params = serde_json::Map::new();
    params.insert("content".into(), JsonValue::String(content.to_string()));
    if let Some(m) = meta {
        let mut s = serde_json::Map::new();
        for (k, v) in m {
            s.insert(k.clone(), JsonValue::String(v.clone()));
        }
        params.insert("meta".into(), JsonValue::Object(s));
    }
    json!({
        "method": CHANNEL_MESSAGE_METHOD,
        "params": params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_message_with_attrs() {
        let mut meta = HashMap::new();
        meta.insert("chat_id".to_string(), "abc".to_string());
        let s = wrap_channel_message("slack", "hi", Some(&meta));
        assert!(s.starts_with("<channel source=\"slack\""));
        assert!(s.contains("chat_id=\"abc\""));
        assert!(s.contains(">\nhi\n</channel>"));
    }

    #[test]
    fn skips_unsafe_meta_keys() {
        let mut meta = HashMap::new();
        meta.insert("x=\"\" inj".to_string(), "y".to_string());
        let s = wrap_channel_message("a", "b", Some(&meta));
        assert!(!s.contains("inj"));
    }

    #[test]
    fn truncate_preserves_short_string() {
        assert_eq!(truncate_for_preview("hello", 10), "hello");
        assert_eq!(truncate_for_preview("0123456789", 6), "01234…");
    }

    #[test]
    fn short_request_id_keeps_tail() {
        assert_eq!(short_request_id("abcdef"), "abcdef");
        assert_eq!(short_request_id("0123456789"), "456789");
    }
}
