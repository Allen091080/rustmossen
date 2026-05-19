use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::plugin_identifier::parse_plugin_identifier;
use super::plugin_policy::is_plugin_blocked_by_policy;
use super::schemas::MossenHint;

const MAX_SHOWN_PLUGINS: usize = 100;

/// A plugin hint recommendation ready for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHintRecommendation {
    pub plugin_id: String,
    pub plugin_name: String,
    pub marketplace_name: String,
    pub plugin_description: Option<String>,
    pub source_command: String,
}

/// Session-scoped set of plugin IDs that have been tried.
static TRIED_THIS_SESSION: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

/// Pre-store gate called by shell tools when a `type="plugin"` hint is detected.
/// Returns true if the hint should be stored (pending), false if dropped.
pub fn maybe_record_plugin_hint(
    hint: &MossenHint,
    feature_enabled: bool,
    has_shown_hint_this_session: bool,
    get_global_hints_state: impl Fn() -> HintsState,
    is_plugin_installed: impl Fn(&str) -> bool,
    is_official_marketplace_name: impl Fn(&str) -> bool,
    set_pending_hint: impl Fn(&MossenHint),
) -> bool {
    if !feature_enabled {
        return false;
    }
    if has_shown_hint_this_session {
        return false;
    }

    let state = get_global_hints_state();
    if state.disabled {
        return false;
    }

    if state.plugin_shown.len() >= MAX_SHOWN_PLUGINS {
        return false;
    }

    let plugin_id = match hint.value.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return false,
    };
    let parsed = parse_plugin_identifier(plugin_id);
    let name = match parsed.name.as_str() {
        "" => return false,
        n => n,
    };
    let marketplace = match parsed.marketplace.as_deref() {
        Some(m) if !m.is_empty() => m,
        _ => return false,
    };

    if !is_official_marketplace_name(marketplace) {
        return false;
    }
    if state.plugin_shown.contains(&plugin_id.to_string()) {
        return false;
    }
    if is_plugin_installed(plugin_id) {
        return false;
    }
    if is_plugin_blocked_by_policy(plugin_id) {
        return false;
    }

    // Bound repeat lookups on the same slug
    {
        let mut tried = TRIED_THIS_SESSION.lock().unwrap();
        if tried.contains(&plugin_id.to_string()) {
            return false;
        }
        tried.insert(plugin_id.to_string());
    }

    set_pending_hint(hint);
    true
}

/// Test-only reset.
pub fn reset_hint_recommendation_for_testing() {
    let mut tried = TRIED_THIS_SESSION.lock().unwrap();
    tried.clear();
}

/// Resolve the pending hint to a renderable recommendation.
pub async fn resolve_plugin_hint(
    hint: &MossenHint,
    get_plugin_by_id: impl std::future::Future<Output = Option<PluginData>>,
) -> Option<PluginHintRecommendation> {
    let plugin_id = hint.value.as_deref().unwrap_or("");
    let parsed = parse_plugin_identifier(plugin_id);

    let plugin_data = get_plugin_by_id.await;

    if let Some(data) = plugin_data {
        Some(PluginHintRecommendation {
            plugin_id: plugin_id.to_string(),
            plugin_name: data.name,
            marketplace_name: parsed.marketplace.unwrap_or_default(),
            plugin_description: data.description,
            source_command: hint.source_command.clone().unwrap_or_default(),
        })
    } else {
        debug!(
            "[hintRecommendation] {} not found in marketplace cache",
            plugin_id
        );
        None
    }
}

/// Record that a prompt for this plugin was surfaced.
pub fn mark_hint_plugin_shown(
    plugin_id: &str,
    get_global_hints_state: impl Fn() -> HintsState,
    save_global_config: impl Fn(HintsState),
) {
    let mut state = get_global_hints_state();
    if state.plugin_shown.contains(&plugin_id.to_string()) {
        return;
    }
    state.plugin_shown.push(plugin_id.to_string());
    save_global_config(state);
}

/// Called when the user picks "don't show plugin installation hints again".
pub fn disable_hint_recommendations(
    get_global_hints_state: impl Fn() -> HintsState,
    save_global_config: impl Fn(HintsState),
) {
    let mut state = get_global_hints_state();
    if state.disabled {
        return;
    }
    state.disabled = true;
    save_global_config(state);
}

/// State for plugin hints stored in global config.
#[derive(Debug, Clone, Default)]
pub struct HintsState {
    pub disabled: bool,
    pub plugin_shown: Vec<String>,
}

/// Plugin data returned from marketplace lookup.
#[derive(Debug, Clone)]
pub struct PluginData {
    pub name: String,
    pub description: Option<String>,
}
