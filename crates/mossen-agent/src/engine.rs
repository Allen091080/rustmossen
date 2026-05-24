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

        // 构建用户消息：text block + 任何附加 block（图片等）。
        let mut user_content: Vec<ContentBlock> = vec![ContentBlock::Text(TextBlock {
            text: prompt.to_string(),
        })];
        user_content.extend(options.additional_user_blocks.clone());
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
                    content: user_message.content.clone(),
                    uuid: user_message.uuid.clone(),
                    is_meta: None,
                    origin: None,
                    extra: HashMap::new(),
                },
                task_id: None,
            })
            .await;

        self.messages.push(user_message);

        // 创建新的取消令牌，或复用调用方提供的 turn-scoped token。
        self.cancel = options
            .cancel_token
            .clone()
            .unwrap_or_else(CancellationToken::new);

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
            effort: None,
            auto_mode: self.config.auto_mode,
            pre_approved_permissions: Vec::new(),
            permission_mode: self.config.permission_mode,
            permission_gate: self
                .config
                .permission_gate
                .clone()
                .unwrap_or_else(|| std::sync::Arc::new(crate::types::AllowAllGate)),
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
        fast_mode: None,
        api_base_url: params.api_base_url,
        api_key: params.api_key,
        skip_stop_hooks: true,
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
