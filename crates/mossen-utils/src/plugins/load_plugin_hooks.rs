//! Load plugin hooks — register hooks from all enabled plugins.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

/// Hook events that plugins can register for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PostSampling,
    PermissionDenied,
    Notification,
    UserPromptSubmit,
    SessionStart,
    SessionEnd,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    PreCompact,
    PostCompact,
    PermissionRequest,
    Setup,
    TeammateIdle,
    TaskCreated,
    TaskCompleted,
    Elicitation,
    ElicitationResult,
    ConfigChange,
    WorktreeCreate,
    WorktreeRemove,
    InstructionsLoaded,
    CwdChanged,
    FileChanged,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::PostSampling => "PostSampling",
            Self::PermissionDenied => "PermissionDenied",
            Self::Notification => "Notification",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::Stop => "Stop",
            Self::StopFailure => "StopFailure",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::PreCompact => "PreCompact",
            Self::PostCompact => "PostCompact",
            Self::PermissionRequest => "PermissionRequest",
            Self::Setup => "Setup",
            Self::TeammateIdle => "TeammateIdle",
            Self::TaskCreated => "TaskCreated",
            Self::TaskCompleted => "TaskCompleted",
            Self::Elicitation => "Elicitation",
            Self::ElicitationResult => "ElicitationResult",
            Self::ConfigChange => "ConfigChange",
            Self::WorktreeCreate => "WorktreeCreate",
            Self::WorktreeRemove => "WorktreeRemove",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::CwdChanged => "CwdChanged",
            Self::FileChanged => "FileChanged",
        }
    }
}

/// A plugin hook matcher entry.
#[derive(Debug, Clone)]
pub struct PluginHookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    pub plugin_root: String,
    pub plugin_name: String,
    pub plugin_id: String,
}

/// Loaded plugin info needed for hook loading.
#[derive(Debug, Clone)]
pub struct LoadedPluginForHooks {
    pub name: String,
    pub path: String,
    pub source: String,
    pub hooks_config: Option<HashMap<HookEvent, Vec<HookMatcherConfig>>>,
}

#[derive(Debug, Clone)]
pub struct HookMatcherConfig {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
}

static HOT_RELOAD_SUBSCRIBED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
static LAST_PLUGIN_SETTINGS_SNAPSHOT: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Convert plugin hooks configuration to native matchers with plugin context.
pub fn convert_plugin_hooks_to_matchers(
    plugin: &LoadedPluginForHooks,
) -> HashMap<HookEvent, Vec<PluginHookMatcher>> {
    let mut plugin_matchers: HashMap<HookEvent, Vec<PluginHookMatcher>> = HashMap::new();
    for event in all_hook_events() {
        plugin_matchers.insert(event, Vec::new());
    }

    let hooks_config = match &plugin.hooks_config {
        Some(c) => c,
        None => return plugin_matchers,
    };

    for (event, matchers) in hooks_config {
        if let Some(list) = plugin_matchers.get_mut(event) {
            for matcher in matchers {
                if !matcher.hooks.is_empty() {
                    list.push(PluginHookMatcher {
                        matcher: matcher.matcher.clone(),
                        hooks: matcher.hooks.clone(),
                        plugin_root: plugin.path.clone(),
                        plugin_name: plugin.name.clone(),
                        plugin_id: plugin.source.clone(),
                    });
                }
            }
        }
    }

    plugin_matchers
}

/// Load and register hooks from all enabled plugins.
pub async fn load_plugin_hooks(
    load_all_plugins_cache_only: impl std::future::Future<Output = PluginsResult>,
    clear_registered_plugin_hooks: impl Fn(),
    register_hook_callbacks: impl Fn(HashMap<HookEvent, Vec<PluginHookMatcher>>),
) {
    let result = load_all_plugins_cache_only.await;
    let mut all_plugin_hooks: HashMap<HookEvent, Vec<PluginHookMatcher>> = HashMap::new();
    for event in all_hook_events() {
        all_plugin_hooks.insert(event, Vec::new());
    }

    for plugin in &result.enabled {
        if plugin.hooks_config.is_none() {
            continue;
        }
        debug!("Loading hooks from plugin: {}", plugin.name);
        let plugin_matchers = convert_plugin_hooks_to_matchers(plugin);
        for (event, matchers) in plugin_matchers {
            all_plugin_hooks.entry(event).or_default().extend(matchers);
        }
    }

    clear_registered_plugin_hooks();
    let total_hooks: usize = all_plugin_hooks
        .values()
        .map(|matchers| matchers.iter().map(|m| m.hooks.len()).sum::<usize>())
        .sum();

    register_hook_callbacks(all_plugin_hooks);
    debug!(
        "Registered {} hooks from {} plugins",
        total_hooks,
        result.enabled.len()
    );
}

/// Prune hooks from plugins no longer in the enabled set.
pub async fn prune_removed_plugin_hooks(
    get_registered_hooks: impl Fn() -> Option<HashMap<HookEvent, Vec<PluginHookMatcher>>>,
    load_all_plugins_cache_only: impl std::future::Future<Output = PluginsResult>,
    clear_registered_plugin_hooks: impl Fn(),
    register_hook_callbacks: impl Fn(HashMap<HookEvent, Vec<PluginHookMatcher>>),
) {
    let current = match get_registered_hooks() {
        Some(h) => h,
        None => return,
    };

    let result = load_all_plugins_cache_only.await;
    let enabled_roots: std::collections::HashSet<String> =
        result.enabled.iter().map(|p| p.path.clone()).collect();

    let mut survivors: HashMap<HookEvent, Vec<PluginHookMatcher>> = HashMap::new();
    for (event, matchers) in current {
        let kept: Vec<PluginHookMatcher> = matchers
            .into_iter()
            .filter(|m| enabled_roots.contains(&m.plugin_root))
            .collect();
        if !kept.is_empty() {
            survivors.insert(event, kept);
        }
    }

    clear_registered_plugin_hooks();
    register_hook_callbacks(survivors);
}

/// Reset hot reload subscription state (test only).
pub fn reset_hot_reload_state() {
    *HOT_RELOAD_SUBSCRIBED.lock().unwrap() = false;
    *LAST_PLUGIN_SETTINGS_SNAPSHOT.lock().unwrap() = None;
}

/// Build a stable string snapshot of the settings that feed into loadAllPluginsCacheOnly().
pub fn get_plugin_affecting_settings_snapshot(
    get_merged_settings: impl Fn() -> PluginAffectingSettings,
    get_policy_settings: impl Fn() -> Option<PolicySettings>,
) -> String {
    let merged = get_merged_settings();
    let policy = get_policy_settings();

    let mut enabled: Vec<(String, bool)> = merged.enabled_plugins.into_iter().collect();
    enabled.sort_by(|a, b| a.0.cmp(&b.0));
    let mut extra: Vec<(String, serde_json::Value)> =
        merged.extra_known_marketplaces.into_iter().collect();
    extra.sort_by(|a, b| a.0.cmp(&b.0));

    serde_json::to_string(&serde_json::json!({
        "enabledPlugins": enabled,
        "extraKnownMarketplaces": extra,
        "strictKnownMarketplaces": policy.as_ref().map(|p| &p.strict_known_marketplaces).cloned().unwrap_or_default(),
        "blockedMarketplaces": policy.as_ref().map(|p| &p.blocked_marketplaces).cloned().unwrap_or_default(),
    })).unwrap_or_default()
}

/// Set up hot reload for plugin hooks when remote settings change.
pub fn setup_plugin_hook_hot_reload(
    subscribe_to_settings_change: impl Fn(Box<dyn Fn(&str) + Send + Sync>),
    get_snapshot: impl Fn() -> String + Send + Sync + 'static,
    clear_plugin_cache: impl Fn(&str) + Send + Sync + 'static,
    reload_hooks: impl Fn() + Send + Sync + 'static,
) {
    let mut subscribed = HOT_RELOAD_SUBSCRIBED.lock().unwrap();
    if *subscribed {
        return;
    }
    *subscribed = true;
    drop(subscribed);

    let initial = get_snapshot();
    *LAST_PLUGIN_SETTINGS_SNAPSHOT.lock().unwrap() = Some(initial);

    subscribe_to_settings_change(Box::new(move |source| {
        if source == "policySettings" {
            let new_snapshot = get_snapshot();
            let mut last = LAST_PLUGIN_SETTINGS_SNAPSHOT.lock().unwrap();
            if last.as_deref() == Some(&new_snapshot) {
                debug!("Plugin hooks: skipping reload, plugin-affecting settings unchanged");
                return;
            }
            *last = Some(new_snapshot);
            drop(last);

            debug!("Plugin hooks: reloading due to plugin-affecting settings change");
            clear_plugin_cache("loadPluginHooks: plugin-affecting settings changed");
            reload_hooks();
        }
    }));
}

#[derive(Debug, Clone, Default)]
pub struct PluginsResult {
    pub enabled: Vec<LoadedPluginForHooks>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PluginAffectingSettings {
    pub enabled_plugins: HashMap<String, bool>,
    pub extra_known_marketplaces: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct PolicySettings {
    pub strict_known_marketplaces: Vec<String>,
    pub blocked_marketplaces: Vec<String>,
}

fn all_hook_events() -> Vec<HookEvent> {
    vec![
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::PostToolUseFailure,
        HookEvent::PostSampling,
        HookEvent::PermissionDenied,
        HookEvent::Notification,
        HookEvent::UserPromptSubmit,
        HookEvent::SessionStart,
        HookEvent::SessionEnd,
        HookEvent::Stop,
        HookEvent::StopFailure,
        HookEvent::SubagentStart,
        HookEvent::SubagentStop,
        HookEvent::PreCompact,
        HookEvent::PostCompact,
        HookEvent::PermissionRequest,
        HookEvent::Setup,
        HookEvent::TeammateIdle,
        HookEvent::TaskCreated,
        HookEvent::TaskCompleted,
        HookEvent::Elicitation,
        HookEvent::ElicitationResult,
        HookEvent::ConfigChange,
        HookEvent::WorktreeCreate,
        HookEvent::WorktreeRemove,
        HookEvent::InstructionsLoaded,
        HookEvent::CwdChanged,
        HookEvent::FileChanged,
    ]
}
