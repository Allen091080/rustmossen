//! Channel permission prompts over channels (Telegram, iMessage, Discord).
//!
//! Translates `services/mcp/channelPermissions.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use regex::Regex;

use mossen_utils::string_utils::truncate_chars;

/// GrowthBook runtime gate for channel permission relay.
pub fn is_channel_permission_relay_enabled(feature_value: bool) -> bool {
    feature_value
}

/// Response from a channel permission event.
#[derive(Debug, Clone)]
pub struct ChannelPermissionResponse {
    pub behavior: PermissionBehavior,
    pub from_server: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
}

/// Callbacks for channel permission handling.
pub struct ChannelPermissionCallbacks {
    pending: Arc<Mutex<HashMap<String, Box<dyn Fn(ChannelPermissionResponse) + Send>>>>,
}

impl ChannelPermissionCallbacks {
    /// Register a resolver for a request ID. Returns an unsubscribe function handle.
    pub fn on_response(
        &self,
        request_id: &str,
        handler: Box<dyn Fn(ChannelPermissionResponse) + Send>,
    ) -> UnsubscribeHandle {
        let key = request_id.to_lowercase();
        self.pending.lock().unwrap().insert(key.clone(), handler);
        UnsubscribeHandle {
            pending: Arc::clone(&self.pending),
            key,
        }
    }

    /// Resolve a pending request from a structured channel event.
    /// Returns true if the ID was pending.
    pub fn resolve(
        &self,
        request_id: &str,
        behavior: PermissionBehavior,
        from_server: &str,
    ) -> bool {
        let key = request_id.to_lowercase();
        let resolver = self.pending.lock().unwrap().remove(&key);
        match resolver {
            Some(f) => {
                f(ChannelPermissionResponse {
                    behavior,
                    from_server: from_server.to_string(),
                });
                true
            }
            None => false,
        }
    }
}

/// Handle to unsubscribe from a pending permission response.
pub struct UnsubscribeHandle {
    pending: Arc<Mutex<HashMap<String, Box<dyn Fn(ChannelPermissionResponse) + Send>>>>,
    key: String,
}

impl UnsubscribeHandle {
    pub fn unsubscribe(self) {
        self.pending.lock().unwrap().remove(&self.key);
    }
}

impl Drop for UnsubscribeHandle {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.key);
    }
}

/// Reply format spec regex for channel servers.
pub fn permission_reply_regex() -> Regex {
    Regex::new(r"(?i)^\s*(y|yes|n|no)\s+([a-km-z]{5})\s*$").unwrap()
}

/// 25-letter alphabet: a-z minus 'l' (looks like 1/I).
const ID_ALPHABET: &[u8; 25] = b"abcdefghijkmnopqrstuvwxyz";

/// Blocklist of substrings to avoid in generated IDs.
const ID_AVOID_SUBSTRINGS: &[&str] = &[
    "fuck", "shit", "cunt", "cock", "dick", "twat", "piss", "crap", "bitch", "whore", "ass", "tit",
    "cum", "fag", "dyke", "nig", "kike", "rape", "nazi", "damn", "poo", "pee", "wank", "anus",
];

/// FNV-1a hash to 5-char ID.
fn hash_to_id(input: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for b in input.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    let mut s = String::with_capacity(5);
    for _ in 0..5 {
        s.push(ID_ALPHABET[(h % 25) as usize] as char);
        h /= 25;
    }
    s
}

/// Short ID from a toolUseID. 5 letters from 25-char alphabet.
/// Re-hashes with salt if result contains blocklisted substring.
pub fn short_request_id(tool_use_id: &str) -> String {
    let mut candidate = hash_to_id(tool_use_id);
    for salt in 0..10 {
        if !ID_AVOID_SUBSTRINGS
            .iter()
            .any(|bad| candidate.contains(bad))
        {
            return candidate;
        }
        candidate = hash_to_id(&format!("{}:{}", tool_use_id, salt));
    }
    candidate
}

/// Truncate tool input to a phone-sized JSON preview (200 chars).
pub fn truncate_for_preview(input: &serde_json::Value) -> String {
    match serde_json::to_string(input) {
        Ok(s) => truncate_chars(&s, 200),
        Err(_) => "(unserializable)".to_string(),
    }
}

/// Filter MCP clients down to those that can relay permission prompts.
/// Three conditions: connected + in allowlist + declares BOTH capabilities.
pub fn filter_permission_relay_clients<'a, T>(
    clients: &'a [T],
    is_in_allowlist: impl Fn(&str) -> bool,
) -> Vec<&'a T>
where
    T: PermissionRelayCandidate,
{
    clients
        .iter()
        .filter(|c| {
            c.connection_type() == "connected"
                && is_in_allowlist(c.name())
                && c.has_experimental_capability(
                    super::channel_notification::CHANNEL_CAPABILITY_KEY,
                )
                && c.has_experimental_capability(
                    super::channel_notification::CHANNEL_PERMISSION_CAPABILITY_KEY,
                )
        })
        .collect()
}

/// Trait for types that can be checked as permission relay candidates.
pub trait PermissionRelayCandidate {
    fn connection_type(&self) -> &str;
    fn name(&self) -> &str;
    fn has_experimental_capability(&self, key: &str) -> bool;
}

/// Factory for the callbacks object.
pub fn create_channel_permission_callbacks() -> ChannelPermissionCallbacks {
    ChannelPermissionCallbacks {
        pending: Arc::new(Mutex::new(HashMap::new())),
    }
}

/// TS `PERMISSION_REPLY_RE` — regex matching a permission-reply notification
/// method name. The regex is built lazily via `permission_reply_regex()`.
pub static PERMISSION_REPLY_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(permission_reply_regex);
