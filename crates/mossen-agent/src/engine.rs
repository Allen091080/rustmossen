//! # engine — SessionOrchestrator（会话编排器）
//!
//! 对应 TS `QueryEngine.ts`，管理会话生命周期：
//! 消息提交、中断、状态查询等。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::api_client::ApiClientConfig;
use crate::dialogue::{self};
use crate::stop_hooks::StopHookManager;
use crate::tool_registry::ToolRegistry;
use crate::types::*;
use mossen_types::{ContentBlock, Message, Role, TextBlock};
use mossen_utils::hooks_utils::{
    execute_user_prompt_submit_hooks, get_user_prompt_submit_hook_blocking_message,
    AggregatedHookResult,
};

fn collect_user_prompt_hook_contexts(results: &[AggregatedHookResult]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| result.additional_contexts.as_ref())
        .flat_map(|contexts| contexts.iter())
        .filter_map(|context| {
            let trimmed = context.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn user_prompt_submit_blocking_message(
    results: &[AggregatedHookResult],
    prompt: &str,
) -> Option<String> {
    results.iter().find_map(|result| {
        result.blocking_error.as_ref().map(|blocking_error| {
            format!(
                "{}\n\nOriginal prompt: {}",
                get_user_prompt_submit_hook_blocking_message(blocking_error),
                prompt
            )
        })
    })
}

fn user_prompt_submit_prevent_message(results: &[AggregatedHookResult]) -> Option<String> {
    results
        .iter()
        .find(|result| result.prevent_continuation == Some(true))
        .map(|result| match result.stop_reason.as_deref() {
            Some(reason) if !reason.trim().is_empty() => {
                format!("Operation stopped by UserPromptSubmit hook: {reason}")
            }
            _ => "Operation stopped by UserPromptSubmit hook".to_string(),
        })
}

async fn send_user_prompt_hook_stop(tx: &mpsc::Sender<SdkMessage>, text: String) {
    let _ = tx
        .send(SdkMessage::Assistant {
            message: mossen_types::AssistantMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text(TextBlock { text })],
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                model: None,
                stop_reason: Some("hook_stopped".to_string()),
                extra: HashMap::new(),
            },
            usage: None,
            task_id: None,
        })
        .await;
    let _ = tx
        .send(SdkMessage::Result {
            terminal: format!("{:?}", TerminalReason::HookStopped),
            cost_usd: None,
            duration_ms: None,
            usage: None,
            task_id: None,
        })
        .await;
}

// ---------------------------------------------------------------------------
// SessionOrchestrator
// ---------------------------------------------------------------------------

/// 会话编排器——管理一次完整的 Agent 会话。
///
/// 对应 TS `QueryEngine` 类。
pub struct SessionOrchestrator {
    /// 配置。
    config: OrchestratorConfig,
    /// 当前消息列表。
    messages: Vec<Message>,
    /// 取消令牌。
    cancel: CancellationToken,
    /// 权限拒绝记录。
    permission_denials: Vec<SdkPermissionDenial>,
    /// 累计用量。
    total_usage: NonNullableUsage,
    /// 是否已处理孤立权限。
    has_handled_orphaned_permission: bool,
    /// 文件状态缓存。
    read_file_state: FileStateCache,
    /// 已加载的嵌套记忆路径。
    loaded_nested_memory_paths: HashSet<String>,
    /// 工具注册表。
    tool_registry: Arc<ToolRegistry>,
    /// Stop hook 管理器。
    stop_hook_manager: Arc<StopHookManager>,
    /// API 客户端配置。
    api_config: Option<ApiClientConfig>,
}

impl SessionOrchestrator {
    /// 创建新的会话编排器。
    pub fn new(config: OrchestratorConfig) -> Self {
        // If the caller supplied an executable tool registry (the TUI/CLI
        // path always does), use it directly. Otherwise fall back to an
        // empty registry — keeps the SDK/test path working without forcing
        // every entry point to know about concrete tool implementations.
        let tool_registry = config
            .tool_registry
            .clone()
            .unwrap_or_else(|| Arc::new(ToolRegistry::new()));
        Self {
            config,
            messages: Vec::new(),
            cancel: CancellationToken::new(),
            permission_denials: Vec::new(),
            total_usage: NonNullableUsage::default(),
            has_handled_orphaned_permission: false,
            read_file_state: FileStateCache::default(),
            loaded_nested_memory_paths: HashSet::new(),
            tool_registry,
            stop_hook_manager: Arc::new(StopHookManager::new()),
            api_config: None,
        }
    }

    /// 设置工具注册表。
    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = registry;
        self
    }

    /// 设置 stop hook 管理器。
    pub fn with_stop_hook_manager(mut self, manager: Arc<StopHookManager>) -> Self {
        self.stop_hook_manager = manager;
        self
    }

    /// 设置 API 客户端配置。
    pub fn with_api_config(mut self, config: ApiClientConfig) -> Self {
        self.api_config = Some(config);
        self
    }

    /// 提交消息并返回 SDK 消息接收器。
    ///
    /// 对应 TS `submitMessage()`。
    pub async fn dispatch_turn(
        &mut self,
        prompt: &str,
        options: Option<SubmitOptions>,
    ) -> mpsc::Receiver<SdkMessage> {
        let options = options.unwrap_or_default();
        let (tx, rx) = mpsc::channel(256);

        // Restore prior conversation context before adding the current
        // user message. `submit_prompt` constructs a fresh orchestrator per
        // turn, so callers that want multi-turn behavior pass the visible
        // transcript here.
        for msg in options.additional_messages {
            self.messages.push(msg);
        }

        // 创建新的取消令牌，或复用调用方提供的 turn-scoped token。
        self.cancel = options
            .cancel_token
            .clone()
            .unwrap_or_else(CancellationToken::new);

        let user_prompt_hook_results = if let Some(ctx) = self.config.hook_context.as_deref() {
            execute_user_prompt_submit_hooks(
                ctx,
                prompt,
                self.config.permission_mode.as_str(),
                Some(&self.cancel),
            )
            .await
        } else {
            Vec::new()
        };

        if let Some(message) =
            user_prompt_submit_blocking_message(&user_prompt_hook_results, prompt)
        {
            send_user_prompt_hook_stop(&tx, message).await;
            return rx;
        }

        if let Some(message) = user_prompt_submit_prevent_message(&user_prompt_hook_results) {
            send_user_prompt_hook_stop(&tx, message).await;
            return rx;
        }

        // 构建用户消息：text block + 任何附加 block（图片等）。
        let mut visible_user_content: Vec<ContentBlock> = vec![ContentBlock::Text(TextBlock {
            text: prompt.to_string(),
        })];
        visible_user_content.extend(options.additional_user_blocks.clone());

        let mut user_content = visible_user_content.clone();
        let additional_contexts = collect_user_prompt_hook_contexts(&user_prompt_hook_results);
        if !additional_contexts.is_empty() {
            user_content.push(ContentBlock::Text(TextBlock {
                text: format!(
                    "UserPromptSubmit hook additional context:\n{}",
                    additional_contexts.join("\n")
                ),
            }));
        }

        let user_message = Message {
            role: Role::User,
            content: user_content,
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            is_meta: None,
            origin: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            extra: HashMap::new(),
        };

        // 发送用户消息回放
        let _ = tx
            .send(SdkMessage::User {
                message: mossen_types::UserMessage {
                    role: Role::User,
                    content: visible_user_content,
                    uuid: user_message.uuid.clone(),
                    is_meta: None,
                    origin: None,
                    extra: HashMap::new(),
                },
                task_id: None,
            })
            .await;

        self.messages.push(user_message);

        // 构建对话规格
        let spec = DialogueSpec {
            system_prompt: self.config.system_prompt.clone(),
            messages: self.messages.clone(),
            tools: self.config.tools.clone(),
            tool_use_context: self.config.tool_use_context.clone(),
            model: self
                .config
                .user_specified_model
                .clone()
                .unwrap_or_else(|| self.config.model.clone()),
            thinking_enabled: false,
            thinking_budget: None,
            max_output_tokens: self.config.max_output_tokens,
            max_turns: options.max_turns,
            origin_tag: self.config.origin_tag.clone(),
            fast_mode: self.config.fast_mode,
            extra_body: self.config.extra_body.clone(),
            cancel: self.cancel.clone(),
            chain_trace: None,
            skip_stop_hooks: self.config.skip_stop_hooks,
            effort: self.config.effort,
            auto_mode: self.config.auto_mode,
            pre_approved_permissions: Vec::new(),
            permission_mode: self.config.permission_mode,
            permission_gate: self
                .config
                .permission_gate
                .clone()
                .unwrap_or_else(|| std::sync::Arc::new(crate::types::AllowAllGate)),
            hook_context: self.config.hook_context.clone(),
        };

        // 获取 API 配置
        let api_config = self.api_config.clone().unwrap_or_else(|| {
            ApiClientConfig::new(
                self.config.api_key.clone().unwrap_or_default(),
                self.config.api_base_url.clone(),
            )
        });

        let tool_registry = self.tool_registry.clone();
        let stop_hook_manager = self.stop_hook_manager.clone();

        // 在后台任务中执行对话循环
        tokio::spawn(async move {
            let psm =
                std::sync::Arc::new(crate::hooks::post_sampling::PostSamplingHookRegistry::new());
            let result = dialogue::initiate_dialogue(
                spec,
                api_config,
                tool_registry,
                stop_hook_manager,
                psm,
                tx,
            )
            .await;

            if let Err(e) = result {
                tracing::error!(error = %e, "Dialogue error");
            }
        });

        rx
    }

    /// 中断当前对话。
    ///
    /// 对应 TS `interrupt()`。
    pub fn interrupt(&self) {
        info!("Interrupting session");
        self.cancel.cancel();
    }

    /// 获取当前消息列表。
    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    /// 获取文件状态缓存。
    pub fn get_read_file_state(&self) -> &FileStateCache {
        &self.read_file_state
    }

    /// 设置模型。
    pub fn set_model(&mut self, model: String) {
        self.config.user_specified_model = Some(model);
    }

    /// 获取当前模型。
    pub fn current_model(&self) -> &str {
        self.config
            .user_specified_model
            .as_deref()
            .unwrap_or(&self.config.model)
    }

    /// 获取累计用量。
    pub fn total_usage(&self) -> &NonNullableUsage {
        &self.total_usage
    }

    /// 获取权限拒绝记录。
    pub fn permission_denials(&self) -> &[SdkPermissionDenial] {
        &self.permission_denials
    }

    /// 重置会话状态。
    pub fn reset(&mut self) {
        self.messages.clear();
        self.total_usage = NonNullableUsage::default();
        self.permission_denials.clear();
        self.has_handled_orphaned_permission = false;
    }
}

// ---------------------------------------------------------------------------
// submit_prompt — 便捷单次对话
// ---------------------------------------------------------------------------

/// 单次对话便捷包装。
///
/// 对应 TS `ask()` 函数。
pub async fn submit_prompt(params: PromptParams) -> mpsc::Receiver<SdkMessage> {
    let config = OrchestratorConfig {
        system_prompt: params.system_prompt,
        tools: params.tools,
        tool_use_context: params.tool_use_context,
        model: params.model,
        user_specified_model: None,
        max_output_tokens: None,
        origin_tag: params.origin_tag,
        fast_mode: params.fast_mode,
        effort: params.effort,
        api_base_url: params.api_base_url,
        api_key: params.api_key,
        skip_stop_hooks: false,
        auto_mode: false,
        extra_body: params.extra_body,
        permission_mode: params.permission_mode,
        // Forward the gate provided by the caller. Most non-interactive
        // entry points leave this `None`, which `SessionOrchestrator` then
        // resolves to `AllowAllGate`. The TUI passes an `InteractiveGate`
        // through `params.permission_gate` so user-facing tool-use prompts
        // round-trip through the modal.
        permission_gate: params.permission_gate,
        tool_registry: params.tool_registry,
        hook_context: params.hook_context,
    };

    let mut orchestrator = SessionOrchestrator::new(config);
    let max_turns = params.max_turns;
    let additional_blocks = params.additional_blocks;
    let history_messages = params.history_messages;

    orchestrator
        .dispatch_turn(
            &params.prompt,
            Some(SubmitOptions {
                max_turns,
                cancel_token: params.cancel_token,
                additional_messages: history_messages,
                additional_user_blocks: additional_blocks,
                ..Default::default()
            }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_types::ToolUseContext;
    use mossen_utils::hooks_utils::{HookMatcher, HooksContext};

    fn test_hooks_context(
        cwd: &std::path::Path,
        registered_hooks: HashMap<String, Vec<HookMatcher>>,
    ) -> HooksContext {
        HooksContext {
            session_id: "engine-test-session".to_string(),
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
        }
    }

    fn command_hook(command: String) -> HookMatcher {
        HookMatcher {
            matcher: None,
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
        }
    }

    fn test_config(
        cwd: &std::path::Path,
        hook_context: Option<Arc<HooksContext>>,
    ) -> OrchestratorConfig {
        OrchestratorConfig {
            system_prompt: Vec::new(),
            tools: Vec::new(),
            tool_use_context: ToolUseContext {
                cwd: cwd.to_string_lossy().to_string(),
                additional_working_directories: None,
                extra: HashMap::new(),
            },
            model: "engine-hook-test".to_string(),
            user_specified_model: None,
            max_output_tokens: None,
            origin_tag: OriginTag::Sdk,
            fast_mode: None,
            effort: None,
            api_base_url: Some("http://127.0.0.1:9".to_string()),
            api_key: Some("sk-test".to_string()),
            skip_stop_hooks: true,
            auto_mode: false,
            extra_body: HashMap::new(),
            permission_mode: PermissionMode::Default,
            permission_gate: Some(Arc::new(AllowAllGate)),
            tool_registry: Some(Arc::new(ToolRegistry::new())),
            hook_context,
        }
    }

    async fn drain_messages(rx: &mut mpsc::Receiver<SdkMessage>) -> Vec<SdkMessage> {
        let mut messages = Vec::new();
        while let Some(message) = rx.recv().await {
            messages.push(message);
        }
        messages
    }

    fn assistant_texts(messages: &[SdkMessage]) -> String {
        messages
            .iter()
            .flat_map(|message| match message {
                SdkMessage::Assistant { message, .. } => message
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(text) => Some(text.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn result_terminals(messages: &[SdkMessage]) -> Vec<&str> {
        messages
            .iter()
            .filter_map(|message| match message {
                SdkMessage::Result { terminal, .. } => Some(terminal.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn user_prompt_submit_blocking_hook_stops_before_dialogue() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            "UserPromptSubmit".to_string(),
            vec![command_hook(
                "printf 'blocked by user prompt hook' >&2; exit 2".to_string(),
            )],
        );
        let hook_context = Arc::new(test_hooks_context(cwd.path(), registered_hooks));
        let mut orchestrator =
            SessionOrchestrator::new(test_config(cwd.path(), Some(hook_context)));

        let mut rx = orchestrator
            .dispatch_turn(
                "scan the repo",
                Some(SubmitOptions {
                    max_turns: Some(1),
                    ..Default::default()
                }),
            )
            .await;
        let messages = drain_messages(&mut rx).await;

        let text = assistant_texts(&messages);
        assert!(
            text.contains("UserPromptSubmit operation blocked by hook"),
            "assistant text should explain hook block: {text}"
        );
        assert!(
            text.contains("Original prompt: scan the repo"),
            "assistant text should include the blocked prompt: {text}"
        );
        assert!(
            result_terminals(&messages)
                .iter()
                .any(|terminal| terminal.contains("HookStopped")),
            "result should report hook stop: {messages:?}"
        );
        assert!(
            orchestrator.get_messages().is_empty(),
            "blocked prompt should not enter dialogue history"
        );
    }

    #[tokio::test]
    async fn user_prompt_submit_prevent_hook_stops_before_dialogue() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            "UserPromptSubmit".to_string(),
            vec![command_hook(
                "printf '%s\n' '{\"continue\":false,\"stopReason\":\"policy gate\"}'".to_string(),
            )],
        );
        let hook_context = Arc::new(test_hooks_context(cwd.path(), registered_hooks));
        let mut orchestrator =
            SessionOrchestrator::new(test_config(cwd.path(), Some(hook_context)));

        let mut rx = orchestrator
            .dispatch_turn(
                "continue check",
                Some(SubmitOptions {
                    max_turns: Some(1),
                    ..Default::default()
                }),
            )
            .await;
        let messages = drain_messages(&mut rx).await;

        let text = assistant_texts(&messages);
        assert!(
            text.contains("Operation stopped by UserPromptSubmit hook: policy gate"),
            "assistant text should explain hook prevent-continuation: {text}"
        );
        assert!(
            result_terminals(&messages)
                .iter()
                .any(|terminal| terminal.contains("HookStopped")),
            "result should report hook stop: {messages:?}"
        );
        assert!(
            orchestrator.get_messages().is_empty(),
            "prevented prompt should not enter dialogue history"
        );
    }

    #[test]
    fn user_prompt_submit_contexts_are_collected_for_model_context() {
        let results = vec![
            AggregatedHookResult {
                additional_contexts: Some(vec![
                    "  repo policy context  ".to_string(),
                    "".to_string(),
                ]),
                ..Default::default()
            },
            AggregatedHookResult {
                additional_contexts: Some(vec!["second context".to_string()]),
                ..Default::default()
            },
        ];

        assert_eq!(
            collect_user_prompt_hook_contexts(&results),
            vec![
                "repo policy context".to_string(),
                "second context".to_string()
            ]
        );
    }
}
