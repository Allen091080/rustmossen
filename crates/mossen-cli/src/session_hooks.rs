//! Session hook runtime adapters for CLI entrypoints.
//!
//! The hook execution primitives live in `mossen-utils`; this module keeps the
//! CLI-side bootstrap state conversion in one place so REPL and oneshot paths
//! do not each grow their own partial hook context.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use mossen_utils::hooks_utils::{AggregatedHookResult, HookMatcher, HooksContext};
use mossen_utils::plugins::load_plugin_hooks::{
    self as plugin_hooks, HookEvent as PluginHookEvent, HookMatcherConfig, LoadedPluginForHooks,
    PluginHookMatcher, PluginsResult,
};
use mossen_utils::session_start::{
    self, HookExecutionResult, HookResultMessage, SessionStartHooksOptions, SessionStartSource,
    SetupTrigger,
};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::bootstrap::SharedBootstrapState;

const SESSION_START_HOOK_TIMEOUT_MS: u64 =
    mossen_utils::hooks_utils::TOOL_HOOK_EXECUTION_TIMEOUT_MS;

struct CliPluginHookLoader;

#[async_trait::async_trait]
impl session_start::AsyncLoadPluginHooks for CliPluginHookLoader {
    async fn load(&self) -> Result<(), anyhow::Error> {
        load_cli_plugin_hooks().await
    }
}

async fn load_cli_plugin_hooks() -> Result<()> {
    let enabled = collect_enabled_plugin_hooks().await;
    let enabled_count = enabled.len();
    plugin_hooks::load_plugin_hooks(
        async move {
            PluginsResult {
                enabled,
                errors: Vec::new(),
            }
        },
        crate::bootstrap::clear_registered_plugin_hooks,
        |hooks| {
            crate::bootstrap::register_plugin_hook_callbacks(
                plugin_hook_matchers_to_registered_hooks(hooks),
            );
        },
    )
    .await;
    info!(
        target: "mossen_agent::hooks",
        enabled_plugin_count = enabled_count,
        "plugin hooks loaded for SessionStart"
    );
    Ok(())
}

async fn collect_enabled_plugin_hooks() -> Vec<LoadedPluginForHooks> {
    let mut plugins = Vec::new();
    plugins.extend(collect_builtin_cli_plugin_hooks());
    plugins.extend(collect_builtin_skill_plugin_hooks());
    plugins.extend(collect_inline_plugin_hooks().await);
    plugins
}

fn collect_builtin_cli_plugin_hooks() -> Vec<LoadedPluginForHooks> {
    let split = crate::plugins::get_builtin_plugins();
    split
        .enabled
        .into_iter()
        .filter_map(|plugin| {
            let hooks_config = plugin
                .hooks_config
                .and_then(|config| serde_json::to_value(config).ok())
                .and_then(|value| parse_plugin_hooks_value(&value));
            Some(LoadedPluginForHooks {
                name: plugin.name,
                path: plugin.path,
                source: plugin.source,
                hooks_config: Some(hooks_config?),
            })
        })
        .collect()
}

fn collect_builtin_skill_plugin_hooks() -> Vec<LoadedPluginForHooks> {
    let enabled_plugins = enabled_plugin_settings();
    let split = mossen_skills::plugin::get_builtin_plugins(&enabled_plugins);
    split
        .enabled
        .into_iter()
        .filter_map(|plugin| {
            let hooks_config = plugin
                .hooks_config
                .as_ref()
                .and_then(parse_plugin_hooks_value);
            Some(LoadedPluginForHooks {
                name: plugin.name,
                path: plugin.path,
                source: plugin.source,
                hooks_config: Some(hooks_config?),
            })
        })
        .collect()
}

async fn collect_inline_plugin_hooks() -> Vec<LoadedPluginForHooks> {
    let mut plugins = Vec::new();
    for plugin_dir in inline_plugin_dirs() {
        match load_inline_plugin_hooks_from_dir(&plugin_dir).await {
            Ok(Some(plugin)) => plugins.push(plugin),
            Ok(None) => {}
            Err(err) => {
                warn!(
                    target: "mossen_agent::hooks",
                    path = %plugin_dir.display(),
                    error = %err,
                    "failed to load inline plugin hooks"
                );
            }
        }
    }
    plugins
}

fn enabled_plugin_settings() -> HashMap<String, bool> {
    mossen_utils::settings::load_settings_from_disk()
        .settings
        .enabled_plugins
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, value)| value.as_bool().map(|enabled| (name, enabled)))
        .collect()
}

fn inline_plugin_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = crate::bootstrap::get_inline_plugins()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    if let Ok(raw) = std::env::var("MOSSEN_CODE_INLINE_PLUGIN_DIRS") {
        dirs.extend(
            raw.split(':')
                .filter(|part| !part.trim().is_empty())
                .map(PathBuf::from),
        );
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

async fn load_inline_plugin_hooks_from_dir(path: &Path) -> Result<Option<LoadedPluginForHooks>> {
    let manifest_path = path.join("plugin.json");
    let manifest = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(content) => serde_json::from_str::<Value>(&content)?,
        Err(_) => return Ok(None),
    };
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "inline-plugin".to_string());

    let hooks_path = path.join("hooks").join("hooks.json");
    let raw_hooks = match tokio::fs::read_to_string(&hooks_path).await {
        Ok(content) => serde_json::from_str::<Value>(&content)?,
        Err(_) => return Ok(None),
    };
    let hooks_config = match parse_plugin_hooks_value(&raw_hooks) {
        Some(config) => config,
        None => return Ok(None),
    };

    Ok(Some(LoadedPluginForHooks {
        name: name.clone(),
        path: path.to_string_lossy().to_string(),
        source: format!("{name}@inline"),
        hooks_config: Some(hooks_config),
    }))
}

fn parse_plugin_hooks_value(
    raw: &Value,
) -> Option<HashMap<PluginHookEvent, Vec<HookMatcherConfig>>> {
    let hooks = raw.get("hooks").unwrap_or(raw);
    let object = hooks.as_object()?;
    let mut parsed = HashMap::new();

    for (event_name, matchers_value) in object {
        let Some(event) =
            serde_json::from_value::<PluginHookEvent>(Value::String(event_name.clone())).ok()
        else {
            continue;
        };
        let Some(matchers) = matchers_value.as_array() else {
            continue;
        };
        let mut parsed_matchers = Vec::new();
        for matcher_value in matchers {
            if let Some(matcher) = parse_plugin_hook_matcher(matcher_value) {
                parsed_matchers.push(matcher);
            }
        }
        if !parsed_matchers.is_empty() {
            parsed.insert(event, parsed_matchers);
        }
    }

    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

fn parse_plugin_hook_matcher(value: &Value) -> Option<HookMatcherConfig> {
    let matcher = value
        .get("matcher")
        .and_then(Value::as_str)
        .filter(|matcher| !matcher.is_empty())
        .map(str::to_string);
    let hooks = if let Some(hooks) = value.get("hooks").and_then(Value::as_array) {
        hooks.clone()
    } else if value.get("type").is_some() {
        vec![value.clone()]
    } else {
        Vec::new()
    };
    if hooks.is_empty() {
        return None;
    }
    Some(HookMatcherConfig { matcher, hooks })
}

fn plugin_hook_matchers_to_registered_hooks(
    hooks: HashMap<PluginHookEvent, Vec<PluginHookMatcher>>,
) -> HashMap<String, Vec<Value>> {
    let mut converted = HashMap::new();
    for (event, matchers) in hooks {
        let event_matchers = matchers
            .into_iter()
            .filter_map(|matcher| {
                serde_json::to_value(HookMatcher {
                    matcher: matcher.matcher,
                    hooks: matcher.hooks,
                    plugin_root: Some(matcher.plugin_root),
                    plugin_id: Some(matcher.plugin_id),
                    plugin_name: Some(matcher.plugin_name),
                    skill_root: None,
                    skill_name: None,
                })
                .ok()
            })
            .collect::<Vec<_>>();
        if !event_matchers.is_empty() {
            converted.insert(event.as_str().to_string(), event_matchers);
        }
    }
    converted
}

struct SessionStartHookExecutor {
    snapshot: SessionHookStateSnapshot,
}

#[async_trait::async_trait]
impl session_start::AsyncExecuteHooks for SessionStartHookExecutor {
    async fn execute(
        &self,
        source: SessionStartSource,
        session_id: Option<&str>,
        agent_type: &str,
        model: Option<&str>,
        force_sync: bool,
    ) -> Vec<HookExecutionResult> {
        let context = self.snapshot.to_hooks_context();
        let results = mossen_utils::hooks_utils::execute_session_start_hooks(
            &context,
            source.as_str(),
            session_id,
            Some(agent_type),
            model,
            None,
            SESSION_START_HOOK_TIMEOUT_MS,
            force_sync,
        )
        .await;
        aggregated_to_session_start_results(results)
    }
}

struct SetupHookExecutor {
    snapshot: SessionHookStateSnapshot,
}

#[async_trait::async_trait]
impl session_start::AsyncExecuteSetupHooks for SetupHookExecutor {
    async fn execute(&self, trigger: SetupTrigger, force_sync: bool) -> Vec<HookExecutionResult> {
        let context = self.snapshot.to_hooks_context();
        let results = mossen_utils::hooks_utils::execute_setup_hooks(
            &context,
            trigger.as_str(),
            None,
            SESSION_START_HOOK_TIMEOUT_MS,
            force_sync,
        )
        .await;
        aggregated_to_session_start_results(results)
    }
}

/// Execute SessionStart hooks for a CLI session.
pub async fn run_session_start_hooks(
    state: &SharedBootstrapState,
    source: SessionStartSource,
    model: Option<&str>,
    is_non_interactive: bool,
) -> Vec<HookResultMessage> {
    let snapshot = match SessionHookStateSnapshot::from_state(state, is_non_interactive) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!(
                target: "mossen_agent::hooks",
                error = %err,
                "SessionStart hook context unavailable; continuing without hooks"
            );
            return Vec::new();
        }
    };

    let options = SessionStartHooksOptions {
        session_id: Some(snapshot.session_id.clone()),
        agent_type: snapshot
            .main_thread_agent_type
            .clone()
            .or_else(|| Some("main".to_string())),
        model: model.map(str::to_string),
        force_sync_execution: Some(is_non_interactive),
    };

    let messages = session_start::process_session_start_hooks(
        source,
        options,
        snapshot.bare_mode,
        mossen_utils::hooks_dir::should_allow_managed_hooks_only(),
        snapshot.main_thread_agent_type.as_deref(),
        CliPluginHookLoader,
        SessionStartHookExecutor {
            snapshot: snapshot.clone(),
        },
        |watch_paths| {
            info!(
                target: "mossen_agent::hooks",
                count = watch_paths.len(),
                paths = ?watch_paths,
                "SessionStart hook requested watch paths"
            );
        },
    )
    .await;

    info!(
        target: "mossen_agent::hooks",
        source = source.as_str(),
        message_count = messages.len(),
        "SessionStart hooks processed"
    );
    messages
}

/// Execute Setup hooks during CLI setup.
pub async fn run_setup_hooks(
    state: &SharedBootstrapState,
    trigger: SetupTrigger,
    is_non_interactive: bool,
) -> Vec<HookResultMessage> {
    let snapshot = match SessionHookStateSnapshot::from_state(state, is_non_interactive) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!(
                target: "mossen_agent::hooks",
                error = %err,
                "Setup hook context unavailable; continuing without hooks"
            );
            return Vec::new();
        }
    };

    let messages = session_start::process_setup_hooks(
        trigger,
        snapshot.bare_mode,
        mossen_utils::hooks_dir::should_allow_managed_hooks_only(),
        true,
        CliPluginHookLoader,
        SetupHookExecutor { snapshot },
    )
    .await;

    info!(
        target: "mossen_agent::hooks",
        trigger = trigger.as_str(),
        message_count = messages.len(),
        "Setup hooks processed"
    );
    messages
}

#[derive(Clone)]
struct SessionHookStateSnapshot {
    session_id: String,
    cwd: PathBuf,
    bare_mode: bool,
    trust_accepted: bool,
    main_thread_agent_type: Option<String>,
    is_non_interactive: bool,
}

impl SessionHookStateSnapshot {
    fn from_state(state: &SharedBootstrapState, is_non_interactive: bool) -> Result<Self> {
        let state = state
            .read()
            .map_err(|err| anyhow::anyhow!("failed to read bootstrap state: {}", err))?;
        Ok(Self {
            session_id: state.session_id.clone(),
            cwd: state.cwd.clone(),
            bare_mode: state.bare_mode,
            trust_accepted: crate::bootstrap::get_session_trust_accepted(),
            main_thread_agent_type: state
                .main_agent_type
                .clone()
                .or_else(crate::bootstrap::get_main_thread_agent_type),
            is_non_interactive,
        })
    }

    fn to_hooks_context(&self) -> HooksContext {
        let cwd = self.cwd.to_string_lossy().to_string();
        let config_home = mossen_utils::naming::get_resolved_config_home_dir();
        let agent_config_home = config_home.clone();
        let session_cwd = self.cwd.clone();
        let agent_cwd = self.cwd.clone();

        HooksContext {
            session_id: self.session_id.clone(),
            original_cwd: cwd.clone(),
            project_root: cwd,
            is_non_interactive: self.is_non_interactive,
            trust_accepted: self.trust_accepted,
            hooks_config_snapshot: snapshot_hooks_config(),
            registered_hooks: registered_hooks_config(crate::bootstrap::get_registered_hooks()),
            disable_all_hooks: mossen_utils::hooks_dir::should_disable_all_hooks_including_managed(
            ),
            managed_hooks_only: mossen_utils::hooks_dir::should_allow_managed_hooks_only(),
            main_thread_agent_type: self.main_thread_agent_type.clone(),
            custom_backend_enabled: mossen_utils::custom_backend::is_custom_backend_enabled(),
            simple_mode: false,
            get_transcript_path: Arc::new(move |session_id| {
                transcript_path_for(&config_home, &session_cwd, session_id)
                    .to_string_lossy()
                    .to_string()
            }),
            get_agent_transcript_path: Arc::new(move |agent_id| {
                agent_transcript_path_for(&agent_config_home, &agent_cwd, agent_id)
                    .to_string_lossy()
                    .to_string()
            }),
            log_debug: Arc::new(|message| debug!(target: "mossen_agent::hooks", "{message}")),
            log_error: Arc::new(|message| warn!(target: "mossen_agent::hooks", "{message}")),
            log_event: Arc::new(|event, data| {
                debug!(target: "mossen_agent::hooks", event, data = %data, "hook analytics event");
            }),
            get_settings: Arc::new(|| {
                serde_json::to_value(mossen_utils::settings::load_settings_from_disk().settings)
                    .ok()
            }),
            get_settings_for_source: Arc::new(|source| {
                use mossen_utils::settings::SettingSource;

                let source = match source {
                    "userSettings" | "UserSettings" => SettingSource::UserSettings,
                    "projectSettings" | "ProjectSettings" => SettingSource::ProjectSettings,
                    "localSettings" | "LocalSettings" => SettingSource::LocalSettings,
                    "policySettings" | "PolicySettings" => SettingSource::PolicySettings,
                    _ => return None,
                };
                mossen_utils::settings::load_settings_for_source(source)
                    .and_then(|settings| serde_json::to_value(settings).ok())
            }),
            invalidate_session_env_cache: Arc::new(|| {}),
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        }
    }
}

/// Build the shared hook execution context used by TUI-side runtime actions
/// such as `/compact`.
pub fn build_hooks_context(
    state: &SharedBootstrapState,
    is_non_interactive: bool,
) -> Result<HooksContext> {
    Ok(SessionHookStateSnapshot::from_state(state, is_non_interactive)?.to_hooks_context())
}

fn transcript_path_for(config_home: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    mossen_utils::session_storage_portable::get_project_dir(
        &config_home.to_string_lossy(),
        &cwd.to_string_lossy(),
    )
    .join(format!("{session_id}.jsonl"))
}

fn agent_transcript_path_for(config_home: &Path, cwd: &Path, agent_id: &str) -> PathBuf {
    mossen_utils::session_storage_portable::get_project_dir(
        &config_home.to_string_lossy(),
        &cwd.to_string_lossy(),
    )
    .join("subagents")
    .join(format!("agent-{agent_id}.jsonl"))
}

fn snapshot_hooks_config() -> Option<HashMap<String, Vec<HookMatcher>>> {
    mossen_utils::hooks_dir::get_hooks_config_from_snapshot().map(|config| {
        config
            .into_iter()
            .map(|(event, matchers)| {
                let converted = matchers
                    .into_iter()
                    .map(|matcher| HookMatcher {
                        matcher: matcher.matcher,
                        hooks: matcher
                            .hooks
                            .into_iter()
                            .filter_map(|hook| serde_json::to_value(hook).ok())
                            .collect(),
                        plugin_root: None,
                        plugin_id: None,
                        plugin_name: None,
                        skill_root: None,
                        skill_name: None,
                    })
                    .collect();
                (event, converted)
            })
            .collect()
    })
}

fn registered_hooks_config(
    raw: Option<HashMap<String, Vec<serde_json::Value>>>,
) -> Option<HashMap<String, Vec<HookMatcher>>> {
    let raw = raw?;
    let mut converted: HashMap<String, Vec<HookMatcher>> = HashMap::new();
    for (event, matchers) in raw {
        let event_matchers = matchers
            .into_iter()
            .filter_map(|matcher| serde_json::from_value::<HookMatcher>(matcher).ok())
            .collect::<Vec<_>>();
        if !event_matchers.is_empty() {
            converted.insert(event, event_matchers);
        }
    }
    Some(converted)
}

fn aggregated_to_session_start_results(
    results: Vec<AggregatedHookResult>,
) -> Vec<HookExecutionResult> {
    results
        .into_iter()
        .map(|result| HookExecutionResult {
            message: result.message.map(value_to_hook_message),
            additional_contexts: result.additional_contexts.unwrap_or_default(),
            initial_user_message: result.initial_user_message,
            watch_paths: result.watch_paths.unwrap_or_default(),
        })
        .collect()
}

fn value_to_hook_message(value: serde_json::Value) -> HookResultMessage {
    let attachment = value.get("attachment");
    let hook_name = attachment
        .and_then(|attachment| attachment.get("hookName"))
        .and_then(|value| value.as_str())
        .unwrap_or("SessionStart")
        .to_string();
    let message_type = attachment
        .and_then(|attachment| attachment.get("type"))
        .and_then(|value| value.as_str())
        .or_else(|| value.get("type").and_then(|value| value.as_str()))
        .unwrap_or("hook_message")
        .to_string();
    let content = value
        .get("content")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());

    HookResultMessage {
        content,
        hook_name,
        message_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_hooks_config_ignores_malformed_matchers() {
        let mut raw = HashMap::new();
        raw.insert(
            "SessionStart".to_string(),
            vec![
                serde_json::json!({
                    "matcher": "startup",
                    "hooks": [{ "type": "command", "command": "echo hi" }]
                }),
                serde_json::json!("not-a-matcher"),
            ],
        );

        let converted = registered_hooks_config(Some(raw)).expect("converted");
        let matchers = converted.get("SessionStart").expect("session start hooks");
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].matcher.as_deref(), Some("startup"));
        assert_eq!(matchers[0].hooks.len(), 1);
    }

    #[test]
    fn aggregated_results_preserve_session_start_fields() {
        let results = vec![AggregatedHookResult {
            message: Some(serde_json::json!({
                "type": "attachment",
                "attachment": {
                    "type": "hook_success",
                    "hookName": "SessionStart",
                    "hookEvent": "SessionStart"
                }
            })),
            additional_contexts: Some(vec!["context".to_string()]),
            initial_user_message: Some("hello".to_string()),
            watch_paths: Some(vec!["src".to_string()]),
            ..Default::default()
        }];

        let converted = aggregated_to_session_start_results(results);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].additional_contexts, vec!["context"]);
        assert_eq!(converted[0].initial_user_message.as_deref(), Some("hello"));
        assert_eq!(converted[0].watch_paths, vec!["src"]);
        assert_eq!(
            converted[0]
                .message
                .as_ref()
                .map(|message| message.message_type.as_str()),
            Some("hook_success")
        );
    }

    #[test]
    fn parse_plugin_hooks_value_accepts_wrapped_hooks_json() {
        let parsed = parse_plugin_hooks_value(&serde_json::json!({
            "hooks": {
                "SessionStart": [
                    {
                        "matcher": "startup",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "echo plugin-start"
                            }
                        ]
                    }
                ]
            }
        }))
        .expect("plugin hooks parsed");

        let matchers = parsed
            .get(&PluginHookEvent::SessionStart)
            .expect("session start plugin hook");
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].matcher.as_deref(), Some("startup"));
        assert_eq!(
            matchers[0].hooks[0].get("command").and_then(Value::as_str),
            Some("echo plugin-start")
        );
    }

    #[test]
    fn plugin_hook_registration_preserves_plugin_metadata_and_payload() {
        let mut raw = HashMap::new();
        raw.insert(
            PluginHookEvent::SessionStart,
            vec![PluginHookMatcher {
                matcher: Some("startup".to_string()),
                hooks: vec![serde_json::json!({
                    "type": "command",
                    "command": "echo registered"
                })],
                plugin_root: "/tmp/plugin".to_string(),
                plugin_name: "demo".to_string(),
                plugin_id: "demo@inline".to_string(),
            }],
        );

        let converted = plugin_hook_matchers_to_registered_hooks(raw);
        let registered = registered_hooks_config(Some(converted)).expect("registered hooks");
        let matchers = registered
            .get("SessionStart")
            .expect("registered SessionStart hook");

        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].plugin_name.as_deref(), Some("demo"));
        assert_eq!(matchers[0].plugin_root.as_deref(), Some("/tmp/plugin"));
        assert_eq!(
            matchers[0].hooks[0].get("command").and_then(Value::as_str),
            Some("echo registered")
        );
    }
}
