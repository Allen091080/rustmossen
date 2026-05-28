//! # notification — AlertDispatcher 工具
//!
//! 对应 TS `PushNotificationTool`（84 行）。发送系统通知。

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};
use mossen_utils::hooks_utils::{execute_notification_hooks, TOOL_HOOK_EXECUTION_TIMEOUT_MS};

/// 告警分发器 — 向用户发送系统通知。
pub struct AlertDispatcher;

/// 工具输入。
#[derive(Debug, Clone, Deserialize)]
pub struct AlertDispatcherInput {
    /// 通知标题（1-120 字符）。
    pub title: String,
    /// 通知正文（1-500 字符）。
    pub body: String,
}

/// 工具输出。
#[derive(Debug, Clone, Serialize)]
pub struct AlertDispatcherOutput {
    pub delivered: bool,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "title".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Notification title (max 120 chars)",
            "maxLength": 120
        }),
    );
    properties.insert(
        "body".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Notification body (max 500 chars)",
            "maxLength": 500
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["title".to_string(), "body".to_string()]),
        extra: HashMap::new(),
    }
}

fn parse_input(input: Value) -> Result<AlertDispatcherInput, String> {
    match input {
        Value::Null => Err(
            "PushNotification requires a JSON object with `title` and `body`; received null."
                .to_string(),
        ),
        Value::Object(_) => serde_json::from_value(input).map_err(|error| {
            format!("PushNotification received invalid input: {error}. Expected object: {{\"title\":\"...\",\"body\":\"...\"}}.")
        }),
        other => Err(format!(
            "PushNotification requires a JSON object with `title` and `body`; received {}.",
            other
        )),
    }
}

fn error_result(message: impl Into<String>) -> anyhow::Result<ToolResult> {
    let output = AlertDispatcherOutput {
        delivered: false,
        title: String::new(),
        body: String::new(),
        error: Some(message.into()),
    };
    Ok(ToolResult {
        output: serde_json::to_string(&output)?,
        is_error: true,
        duration_ms: 0,
        metadata: HashMap::new(),
    })
}

async fn execute_configured_notification_hook(context: &ToolUseContext, title: &str, body: &str) {
    let Some(hooks_context) = crate::task_hooks::runtime_hook_context(context) else {
        return;
    };
    execute_notification_hooks(
        hooks_context.as_ref(),
        body,
        Some(title),
        "PushNotification",
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;
}

#[async_trait]
impl Tool for AlertDispatcher {
    fn name(&self) -> &str {
        "PushNotification"
    }

    fn description(&self) -> &str {
        "Send a push notification to the user"
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp = match parse_input(input) {
            Ok(input) => input,
            Err(message) => return error_result(message),
        };
        if inp.title.trim().is_empty() {
            return error_result("PushNotification requires a non-empty `title` string.");
        }
        if inp.body.trim().is_empty() {
            return error_result("PushNotification requires a non-empty `body` string.");
        }
        execute_configured_notification_hook(context, &inp.title, &inp.body).await;
        let (delivered, error) = deliver_local_notification(&inp.title, &inp.body);

        let output = AlertDispatcherOutput {
            delivered,
            title: inp.title,
            body: inp.body,
            error,
        };

        Ok(ToolResult {
            output: serde_json::to_string(&output)?,
            is_error: !delivered,
            duration_ms: 0,
            metadata: HashMap::new(),
        })
    }
}

fn deliver_local_notification(title: &str, body: &str) -> (bool, Option<String>) {
    if matches!(
        std::env::var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    ) {
        return (
            false,
            Some("Local notifications disabled by MOSSEN_DISABLE_LOCAL_NOTIFICATIONS".to_string()),
        );
    }

    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "display notification {} with title {}",
                osascript_literal(body),
                osascript_literal(title)
            ))
            .status()
    } else if cfg!(target_os = "linux") {
        std::process::Command::new("notify-send")
            .arg(title)
            .arg(body)
            .status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "New-BurntToastNotification",
                "-Text",
                title,
                body,
            ])
            .status()
    } else {
        return (
            false,
            Some("Local notifications are not supported on this platform".to_string()),
        );
    };

    match result {
        Ok(status) if status.success() => (true, None),
        Ok(status) => (
            false,
            Some(format!("Local notification command exited with {}", status)),
        ),
        Err(error) => (
            false,
            Some(format!("Local notification command failed: {}", error)),
        ),
    }
}

fn osascript_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, OnceLock};

    use mossen_utils::hooks_utils::{
        register_runtime_hooks_context, unregister_runtime_hooks_context, HookMatcher, HooksContext,
    };

    struct EnvRestore {
        previous: Option<String>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS", previous);
            } else {
                std::env::remove_var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS");
            }
        }
    }

    async fn notification_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    struct HookRegistration {
        id: String,
    }

    impl Drop for HookRegistration {
        fn drop(&mut self) {
            unregister_runtime_hooks_context(&self.id);
        }
    }

    fn context() -> ToolUseContext {
        ToolUseContext {
            cwd: ".".to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        }
    }

    fn hooked_context(
        cwd: &std::path::Path,
        command: String,
    ) -> (ToolUseContext, HookRegistration) {
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            "Notification".to_string(),
            vec![HookMatcher {
                matcher: Some("PushNotification".to_string()),
                hooks: vec![serde_json::json!({
                    "type": "command",
                    "command": command,
                    "timeout": 1
                })],
                plugin_root: None,
                plugin_id: None,
                plugin_name: None,
                skill_root: None,
                skill_name: None,
            }],
        );
        let hooks_context = Arc::new(HooksContext {
            session_id: "notification-test".to_string(),
            original_cwd: cwd.to_string_lossy().to_string(),
            project_root: cwd.to_string_lossy().to_string(),
            is_non_interactive: true,
            trust_accepted: true,
            hooks_config_snapshot: None,
            registered_hooks: Some(registered_hooks),
            disable_all_hooks: false,
            managed_hooks_only: false,
            main_thread_agent_type: Some("main".to_string()),
            custom_backend_enabled: false,
            simple_mode: false,
            get_transcript_path: Arc::new(|session_id| format!("/tmp/{session_id}.jsonl")),
            get_agent_transcript_path: Arc::new(|agent_id| format!("/tmp/agent-{agent_id}.jsonl")),
            log_debug: Arc::new(|_| {}),
            log_error: Arc::new(|_| {}),
            log_event: Arc::new(|_, _| {}),
            get_settings: Arc::new(|| None),
            get_settings_for_source: Arc::new(|_| None),
            invalidate_session_env_cache: Arc::new(|| {}),
            dynamic_hook_executor: None,
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        });
        let id = register_runtime_hooks_context(hooks_context);
        let mut context = ToolUseContext {
            cwd: cwd.to_string_lossy().to_string(),
            additional_working_directories: None,
            extra: HashMap::new(),
        };
        context.extra.insert(
            crate::task_hooks::HOOK_CONTEXT_ID_EXTRA_KEY.to_string(),
            serde_json::json!(id.clone()),
        );
        (context, HookRegistration { id })
    }

    #[tokio::test]
    async fn push_notification_reports_disabled_delivery_as_error() {
        let _guard = notification_env_lock().await;
        let _restore = EnvRestore {
            previous: std::env::var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS").ok(),
        };
        std::env::set_var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS", "1");

        let result = AlertDispatcher
            .execute(
                serde_json::json!({
                    "title": "Build",
                    "body": "Finished"
                }),
                &context(),
            )
            .await
            .expect("push notification");
        assert!(result.is_error);
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("output json");
        assert_eq!(output["delivered"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("disabled"));
    }

    #[tokio::test]
    async fn push_notification_null_input_returns_structured_tool_error() {
        let result = AlertDispatcher
            .execute(serde_json::Value::Null, &context())
            .await
            .expect("push notification");
        let output: serde_json::Value = serde_json::from_str(&result.output).expect("output json");

        assert!(result.is_error);
        assert_eq!(output["delivered"], false);
        assert!(output["error"]
            .as_str()
            .unwrap_or_default()
            .contains("title"));
    }

    #[tokio::test]
    async fn push_notification_executes_configured_notification_hook() {
        let _guard = notification_env_lock().await;
        let _restore = EnvRestore {
            previous: std::env::var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS").ok(),
        };
        std::env::set_var("MOSSEN_DISABLE_LOCAL_NOTIFICATIONS", "1");
        let cwd = tempfile::tempdir().expect("tempdir");
        let marker_path = cwd.path().join("notification_hook_marker");
        let marker_arg = marker_path.to_string_lossy().replace('\'', "'\\''");
        let (context, _registration) =
            hooked_context(cwd.path(), format!("printf notified > '{marker_arg}'"));

        let result = AlertDispatcher
            .execute(
                serde_json::json!({
                    "title": "Build",
                    "body": "Finished"
                }),
                &context,
            )
            .await
            .expect("push notification");

        assert!(result.is_error);
        let marker = tokio::fs::read_to_string(&marker_path)
            .await
            .expect("Notification hook should write marker");
        assert_eq!(marker, "notified");
    }
}
