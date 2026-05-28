//! Session-local approval queue for MCP channel allowlist gates.
//!
//! Enterprise/org allowlists still win. This module covers the interactive
//! TUI path for local/development channel entries: when a channel server is
//! blocked by the allowlist gate, the protocol layer enqueues a request; the
//! TUI can approve it for the current process and then retry `/mcp`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelApprovalRequest {
    pub id: String,
    pub server_name: String,
    pub plugin: Option<String>,
    pub marketplace: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelApprovalDecision {
    Allow,
    Deny,
}

#[derive(Default)]
struct Store {
    pending: VecDeque<ChannelApprovalRequest>,
    decisions: HashMap<String, ChannelApprovalDecision>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

pub fn approval_id(server_name: &str, plugin: Option<&str>, marketplace: Option<&str>) -> String {
    match (plugin, marketplace) {
        (Some(plugin), Some(marketplace)) => {
            format!("plugin:{}@{}:{}", plugin, marketplace, server_name)
        }
        (Some(plugin), None) => format!("plugin:{}:{}", plugin, server_name),
        _ => format!("server:{}", server_name),
    }
}

pub fn enqueue(request: ChannelApprovalRequest) {
    let mut guard = store().lock().unwrap();
    if guard.decisions.contains_key(&request.id)
        || guard.pending.iter().any(|pending| pending.id == request.id)
    {
        return;
    }
    guard.pending.push_back(request);
}

pub fn drain_pending() -> Vec<ChannelApprovalRequest> {
    let mut guard = store().lock().unwrap();
    guard.pending.drain(..).collect()
}

pub fn pop_pending() -> Option<ChannelApprovalRequest> {
    store().lock().unwrap().pending.pop_front()
}

pub fn submit_decision(id: &str, decision: ChannelApprovalDecision) {
    let mut guard = store().lock().unwrap();
    guard.decisions.insert(id.to_string(), decision);
}

pub fn is_allowed(id: &str) -> bool {
    matches!(
        store().lock().unwrap().decisions.get(id),
        Some(ChannelApprovalDecision::Allow)
    )
}

#[cfg(test)]
pub fn clear_for_tests() {
    let mut guard = store().lock().unwrap();
    guard.pending.clear();
    guard.decisions.clear();
}
