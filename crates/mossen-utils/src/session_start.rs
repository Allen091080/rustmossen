//! Session start hook processing.
//!
//! Processes SessionStart and Setup hooks during session initialization,
//! loading plugin hooks and executing registered hook handlers.

/// Source/trigger for session start processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStartSource {
    Startup,
    Resume,
    Clear,
    Compact,
}

impl SessionStartSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Resume => "resume",
            Self::Clear => "clear",
            Self::Compact => "compact",
        }
    }
}

/// Trigger for setup hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupTrigger {
    Init,
    Maintenance,
}

impl SetupTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Maintenance => "maintenance",
        }
    }
}

/// Options for session start hook processing.
#[derive(Debug, Clone, Default)]
pub struct SessionStartHooksOptions {
    pub session_id: Option<String>,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub force_sync_execution: Option<bool>,
}

/// A message produced by a hook execution.
#[derive(Debug, Clone)]
pub struct HookResultMessage {
    pub content: String,
    pub hook_name: String,
    pub message_type: String,
}

/// Result from a single hook execution.
#[derive(Debug, Clone, Default)]
pub struct HookExecutionResult {
    pub message: Option<HookResultMessage>,
    pub additional_contexts: Vec<String>,
    pub initial_user_message: Option<String>,
    pub watch_paths: Vec<String>,
}

/// Attachment message for hook additional context.
#[derive(Debug, Clone)]
pub struct AttachmentMessage {
    pub attachment_type: String,
    pub content: Vec<String>,
    pub hook_name: String,
    pub tool_use_id: String,
    pub hook_event: String,
}

/// Module-level pending initial user message (set by hooks, consumed once).
static PENDING_INITIAL_USER_MESSAGE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Take (consume) the initial user message set by a hook, if any.
pub fn take_initial_user_message() -> Option<String> {
    PENDING_INITIAL_USER_MESSAGE
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
}

/// Set the pending initial user message (called by hook processing).
fn set_pending_initial_user_message(msg: String) {
    if let Ok(mut guard) = PENDING_INITIAL_USER_MESSAGE.lock() {
        *guard = Some(msg);
    }
}

/// User guidance for plugin hook loading errors.
fn get_user_guidance(error_message: &str) -> &'static str {
    if error_message.contains("Failed to clone")
        || error_message.contains("network")
        || error_message.contains("ETIMEDOUT")
        || error_message.contains("ENOTFOUND")
    {
        "This appears to be a network issue. Check your internet connection and try again."
    } else if error_message.contains("Permission denied")
        || error_message.contains("EACCES")
        || error_message.contains("EPERM")
    {
        "This appears to be a permissions issue. Check file permissions on ~/.mossen/plugins/"
    } else if error_message.contains("Invalid")
        || error_message.contains("parse")
        || error_message.contains("JSON")
        || error_message.contains("schema")
    {
        "This appears to be a configuration issue. Check your plugin settings in .mossen/settings.json"
    } else {
        "Please fix the plugin configuration or remove problematic plugins from your settings."
    }
}

/// Process session start hooks. Returns hook result messages.
///
/// - `source`: the startup trigger
/// - `options`: optional session/agent/model info
/// - `is_bare_mode`: whether running in bare mode (skips all hooks)
/// - `allow_managed_hooks_only`: whether to restrict to managed hooks
/// - `main_thread_agent_type`: fallback agent type from bootstrap state
/// - `load_plugin_hooks_fn`: async function to load plugin hooks
/// - `execute_session_start_hooks_fn`: async generator to execute hooks
/// - `update_watch_paths_fn`: callback to update file watcher paths
pub async fn process_session_start_hooks<F, G, H>(
    source: SessionStartSource,
    options: SessionStartHooksOptions,
    is_bare_mode: bool,
    allow_managed_hooks_only: bool,
    main_thread_agent_type: Option<&str>,
    load_plugin_hooks_fn: F,
    execute_hooks_fn: G,
    update_watch_paths_fn: H,
) -> Vec<HookResultMessage>
where
    F: AsyncLoadPluginHooks,
    G: AsyncExecuteHooks,
    H: Fn(&[String]),
{
    if is_bare_mode {
        return Vec::new();
    }

    let mut hook_messages: Vec<HookResultMessage> = Vec::new();
    let mut additional_contexts: Vec<String> = Vec::new();
    let mut all_watch_paths: Vec<String> = Vec::new();

    // Load plugin hooks if not restricted to managed-only
    if allow_managed_hooks_only {
        tracing::debug!("Skipping plugin hooks - allowManagedHooksOnly is enabled");
    } else {
        match load_plugin_hooks_fn.load().await {
            Ok(_) => {}
            Err(e) => {
                let error_msg = e.to_string();
                let guidance = get_user_guidance(&error_msg);
                tracing::error!(
                    "Failed to load plugin hooks during {}: {}",
                    source.as_str(),
                    error_msg
                );
                tracing::warn!(
                    "Warning: Failed to load plugin hooks. SessionStart hooks from plugins will not execute. Error: {}. {}",
                    error_msg,
                    guidance
                );
            }
        }
    }

    // Execute SessionStart hooks
    let resolved_agent_type = options
        .agent_type
        .as_deref()
        .or(main_thread_agent_type)
        .unwrap_or("default");

    let results = execute_hooks_fn
        .execute(
            source,
            options.session_id.as_deref(),
            resolved_agent_type,
            options.model.as_deref(),
            options.force_sync_execution.unwrap_or(false),
        )
        .await;

    for hook_result in results {
        if let Some(msg) = hook_result.message {
            hook_messages.push(msg);
        }
        if !hook_result.additional_contexts.is_empty() {
            additional_contexts.extend(hook_result.additional_contexts);
        }
        if let Some(initial_msg) = hook_result.initial_user_message {
            set_pending_initial_user_message(initial_msg);
        }
        if !hook_result.watch_paths.is_empty() {
            all_watch_paths.extend(hook_result.watch_paths);
        }
    }

    if !all_watch_paths.is_empty() {
        update_watch_paths_fn(&all_watch_paths);
    }

    // If hooks provided additional context, add it as a message
    if !additional_contexts.is_empty() {
        hook_messages.push(HookResultMessage {
            content: additional_contexts.join("\n"),
            hook_name: "SessionStart".to_string(),
            message_type: "hook_additional_context".to_string(),
        });
    }

    hook_messages
}

/// Process setup hooks. Returns hook result messages.
pub async fn process_setup_hooks<F, G>(
    trigger: SetupTrigger,
    is_bare_mode: bool,
    allow_managed_hooks_only: bool,
    force_sync_execution: bool,
    load_plugin_hooks_fn: F,
    execute_setup_hooks_fn: G,
) -> Vec<HookResultMessage>
where
    F: AsyncLoadPluginHooks,
    G: AsyncExecuteSetupHooks,
{
    if is_bare_mode {
        return Vec::new();
    }

    let mut hook_messages: Vec<HookResultMessage> = Vec::new();
    let mut additional_contexts: Vec<String> = Vec::new();

    if allow_managed_hooks_only {
        tracing::debug!("Skipping plugin hooks - allowManagedHooksOnly is enabled");
    } else {
        if let Err(e) = load_plugin_hooks_fn.load().await {
            let error_msg = e.to_string();
            tracing::warn!(
                "Warning: Failed to load plugin hooks. Setup hooks from plugins will not execute. Error: {}",
                error_msg
            );
        }
    }

    let results = execute_setup_hooks_fn
        .execute(trigger, force_sync_execution)
        .await;

    for hook_result in results {
        if let Some(msg) = hook_result.message {
            hook_messages.push(msg);
        }
        if !hook_result.additional_contexts.is_empty() {
            additional_contexts.extend(hook_result.additional_contexts);
        }
    }

    if !additional_contexts.is_empty() {
        hook_messages.push(HookResultMessage {
            content: additional_contexts.join("\n"),
            hook_name: "Setup".to_string(),
            message_type: "hook_additional_context".to_string(),
        });
    }

    hook_messages
}

/// Trait for async plugin hook loading.
#[async_trait::async_trait]
pub trait AsyncLoadPluginHooks {
    async fn load(&self) -> Result<(), anyhow::Error>;
}

/// Trait for async session start hook execution.
#[async_trait::async_trait]
pub trait AsyncExecuteHooks {
    async fn execute(
        &self,
        source: SessionStartSource,
        session_id: Option<&str>,
        agent_type: &str,
        model: Option<&str>,
        force_sync: bool,
    ) -> Vec<HookExecutionResult>;
}

/// Trait for async setup hook execution.
#[async_trait::async_trait]
pub trait AsyncExecuteSetupHooks {
    async fn execute(&self, trigger: SetupTrigger, force_sync: bool) -> Vec<HookExecutionResult>;
}
