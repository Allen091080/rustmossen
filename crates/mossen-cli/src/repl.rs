//! REPL 循环启动 — TUI 初始化与事件循环。
//!
//! 对应 TS 的 replLauncher.tsx、App 组件和 REPL 主循环。
//! 使用 mossen-tui 提供的 App 和 EventBus 驱动交互式会话。

use anyhow::Result;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::cursor;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use mossen_agent::engine::{submit_prompt, SessionOrchestrator};
use mossen_agent::types::{
    EffortLevel, InteractiveGate, OrchestratorConfig, OriginTag, PermissionDecision,
    PermissionGate, PermissionMode, PermissionRequest, PromptParams, SdkMessage, SubmitOptions,
};
use mossen_mcp::config::{ConfigScope, ScopedMcpServerConfig};
use mossen_mcp::protocol::Implementation;
use mossen_mcp::server::McpServerManager;
use mossen_types::{ContentBlock, Message, Role, TextBlock, ToolDefinition, ToolUseContext};

use crate::bootstrap::SharedBootstrapState;
use crate::commands_registry::DirectiveRegistry;
use crate::stream_json_render_events::{
    StreamJsonRenderEventEmitter, StreamJsonTerminalWidgetControl,
};
use crate::stream_json_terminal_renderer::{
    StreamJsonTerminalDrawRuntime, StreamJsonTerminalViewport, STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
};
use crate::structured_io::{ndjson_safe_stringify, StdoutMessage, StructuredIO};
use crate::tools_registry::InstrumentRegistry;

fn parse_env_bool(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" | "enable" | "enabled" => Some(true),
            "0" | "false" | "off" | "no" | "disable" | "disabled" => Some(false),
            _ => None,
        })
}

fn parse_effort_level(value: &str) -> Option<EffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some(EffortLevel::Low),
        "medium" => Some(EffortLevel::Medium),
        "high" => Some(EffortLevel::High),
        "max" => Some(EffortLevel::Max),
        _ => None,
    }
}

fn normalize_subagent_type(agent_type: &str) -> String {
    let normalized = agent_type
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    match normalized.as_str() {
        "general-purpose" | "generalpurpose" => "general".to_string(),
        _ => normalized,
    }
}

fn resolve_builtin_subagent_definition(
    agent_type: Option<&str>,
) -> Option<mossen_tools::agent_tool::load_agents_dir::AgentDefinition> {
    let requested = agent_type?.trim();
    if requested.is_empty() {
        return None;
    }
    let normalized = normalize_subagent_type(requested);
    mossen_tools::agent_tool::built_in_agents::get_built_in_agents()
        .into_iter()
        .find(|agent| {
            agent.agent_type.eq_ignore_ascii_case(requested)
                || agent.agent_type.eq_ignore_ascii_case(&normalized)
        })
}

fn subagent_system_prompt_block(
    agent: &mossen_tools::agent_tool::load_agents_dir::AgentDefinition,
) -> mossen_agent::types::SystemBlock {
    let role_text = agent
        .system_prompt
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(agent.when_to_use.as_str());
    mossen_agent::types::SystemBlock {
        text: format!(
            "# Subagent role\nYou are running as the `{}` subagent.\n\n{}",
            agent.agent_type,
            role_text.trim()
        ),
        cache_control: None,
    }
}

fn filter_tool_definitions_for_subagent(
    tools: Vec<ToolDefinition>,
    agent: &mossen_tools::agent_tool::load_agents_dir::AgentDefinition,
    is_async_agent: bool,
) -> Vec<ToolDefinition> {
    let tool_names: Vec<String> = tools.iter().map(|tool| tool.name.clone()).collect();
    let resolved = mossen_tools::agent_tool::utils::resolve_agent_tools(
        &tool_names,
        agent.tools.as_deref(),
        agent.disallowed_tools.as_deref(),
        agent.source == "built-in",
        is_async_agent,
        agent.permission_mode.as_ref(),
    );
    let allowed: HashSet<String> = resolved.resolved_tool_names.into_iter().collect();
    tools
        .into_iter()
        .filter(|tool| allowed.contains(&tool.name))
        .collect()
}

fn convert_subagent_permission_mode(
    mode: Option<&mossen_tools::agent_tool::utils::PermissionMode>,
) -> Option<PermissionMode> {
    match mode {
        Some(mossen_tools::agent_tool::utils::PermissionMode::AcceptEdits) => {
            Some(PermissionMode::AcceptEdits)
        }
        Some(mossen_tools::agent_tool::utils::PermissionMode::DontAsk) => {
            Some(PermissionMode::DontAsk)
        }
        Some(mossen_tools::agent_tool::utils::PermissionMode::Plan) => Some(PermissionMode::Plan),
        Some(mossen_tools::agent_tool::utils::PermissionMode::Bubble) => {
            Some(PermissionMode::Default)
        }
        None => None,
    }
}

/// REPL 配置。
#[derive(Clone)]
pub struct ReplConfig {
    /// 初始提示（从 --continue 或 --restore 恢复的消息）。
    pub initial_prompt: Option<String>,
    /// 是否为恢复模式。
    pub restore_mode: bool,
    /// 恢复的会话 ID。
    pub restore_session_id: Option<String>,
    /// 是否启用 MCP。
    pub mcp_enabled: bool,
    /// MCP 配置 JSON。
    pub mcp_config: Option<String>,
    /// 系统提示覆盖。
    pub system_prompt: Option<String>,
    /// 额外系统提示。
    pub extra_prompt: Option<String>,
    /// 模型覆盖。
    pub model_override: Option<String>,
    /// 单次执行最大轮次限制。
    pub max_turns: Option<u32>,
    /// 仅允许的内置工具名；空列表表示不过滤。
    pub allowed_instruments: Vec<String>,
    /// 禁用的内置工具名。
    pub disabled_instruments: Vec<String>,
    /// 关闭信号标志。
    pub shutdown_flag: Arc<AtomicBool>,
}

struct ShutdownCancelBridge {
    token: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl ShutdownCancelBridge {
    fn new(shutdown_flag: Arc<AtomicBool>) -> Self {
        let token = CancellationToken::new();
        if shutdown_flag.load(Ordering::SeqCst) {
            token.cancel();
        }

        let bridge_token = token.clone();
        let task = tokio::spawn(async move {
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    bridge_token.cancel();
                    break;
                }
                if bridge_token.is_cancelled() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        });

        Self { token, task }
    }

    fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for ShutdownCancelBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn instrument_name_matches(list: &[String], name: &str) -> bool {
    list.iter().any(|item| item.eq_ignore_ascii_case(name))
}

fn filtered_builtin_tools(
    options: mossen_tools::ToolRuntimeOptions,
    config: &ReplConfig,
) -> Vec<Box<dyn mossen_agent::tool_registry::Tool>> {
    let mut tools = mossen_tools::all_tools_for_runtime(options);
    if !config.allowed_instruments.is_empty() {
        tools.retain(|tool| instrument_name_matches(&config.allowed_instruments, tool.name()));
    }
    if !config.disabled_instruments.is_empty() {
        tools.retain(|tool| !instrument_name_matches(&config.disabled_instruments, tool.name()));
    }
    tools
}

/// 返回 oneshot / exec 路径的默认 model id。
/// 优先级：MOSSEN_CODE_CUSTOM_MODEL → "custom-backend-model"
fn default_model_for_unset_cli() -> String {
    std::env::var("MOSSEN_CODE_CUSTOM_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "custom-backend-model".to_string())
}

fn session_permission_mode_from_env() -> PermissionMode {
    std::env::var("MOSSEN_PERMISSION_MODE")
        .ok()
        .map(PermissionMode::parse)
        .unwrap_or(PermissionMode::Default)
}

async fn collect_mcp_tool_definitions(manager: &McpServerManager) -> Vec<ToolDefinition> {
    let mut definitions = Vec::new();
    for (server_name, tools) in manager.get_all_tools().await {
        definitions.extend(
            tools
                .iter()
                .map(|tool| mossen_mcp::tools::to_mossen_tool_definition(&server_name, tool)),
        );
    }
    definitions.sort_by(|a, b| a.name.cmp(&b.name));
    definitions
}

fn mcp_configs_from_raw(
    raw: HashMap<String, serde_json::Value>,
    scope: ConfigScope,
) -> HashMap<String, ScopedMcpServerConfig> {
    raw.into_iter()
        .filter_map(|(name, mut value)| {
            if let Some(obj) = value.as_object_mut() {
                obj.entry("scope".to_string())
                    .or_insert_with(|| serde_json::json!(scope));
            }
            match serde_json::from_value::<ScopedMcpServerConfig>(value) {
                Ok(config) => Some((name, config)),
                Err(err) => {
                    warn!(
                        target: "mossen_cli::mcp",
                        server = %name,
                        error = %err,
                        "ignoring invalid MCP server config"
                    );
                    None
                }
            }
        })
        .collect()
}

async fn load_mcp_configs(
    config: &ReplConfig,
    cwd: &Path,
) -> HashMap<String, ScopedMcpServerConfig> {
    if let Some(cfg_json) = config.mcp_config.as_deref() {
        if let Ok(configs) =
            serde_json::from_str::<HashMap<String, ScopedMcpServerConfig>>(cfg_json)
        {
            return configs;
        }
        match mossen_mcp::config_ext::parse_mcp_config(cfg_json) {
            Ok(raw) => return mcp_configs_from_raw(raw, ConfigScope::Local),
            Err(err) => warn!(
                target: "mossen_cli::mcp",
                error = %err,
                "failed to parse --mcp-config as MCP config; using project config if present"
            ),
        }
    }

    let raw = mossen_mcp::config_ext::get_project_mcp_configs_from_cwd(cwd).await;
    mcp_configs_from_raw(raw, ConfigScope::Local)
}

async fn initialize_mcp_manager_for_session(
    config: &ReplConfig,
    cwd: &Path,
    source: &str,
) -> Option<(Arc<McpServerManager>, Vec<ToolDefinition>, usize)> {
    let configs = load_mcp_configs(config, cwd).await;
    if configs.is_empty() {
        info!(
            target: "mossen_cli::mcp",
            source,
            "MCP requested but no server configs were found"
        );
        return None;
    }

    let server_count = configs.len();
    let mcp_manager = Arc::new(McpServerManager::new(Implementation {
        name: "mossen-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }));
    mcp_manager.update_configs(configs).await;
    mcp_manager.connect_all().await;
    let mcp_tool_definitions = collect_mcp_tool_definitions(&mcp_manager).await;
    info!(
        target: "mossen_cli::mcp",
        source,
        connected = mcp_manager.connected_count(),
        total = server_count,
        tool_count = mcp_tool_definitions.len(),
        "MCP server manager initialized"
    );
    Some((mcp_manager, mcp_tool_definitions, server_count))
}

/// 启动交互式 REPL 循环 — 对应 TS 的 launchRepl()。
///
/// 初始化 TUI、创建 Agent 会话、启动事件循环。
pub async fn launch_repl(
    state: SharedBootstrapState,
    _directives: DirectiveRegistry,
    _instruments: InstrumentRegistry,
    config: ReplConfig,
) -> Result<()> {
    let _span = tracing::info_span!("repl").entered();
    info!("launch_repl: initializing interactive session");

    // 1. 标记为交互式模式
    let (model, cwd, cwd_path, system_prompt_override) = {
        let mut s = state
            .write()
            .map_err(|e| anyhow::anyhow!("failed to write state: {}", e))?;
        s.is_interactive = true;
        if let Some(ref model) = config.model_override {
            s.model_override = Some(model.clone());
        }
        let model = s
            .model_override
            .clone()
            .or_else(|| config.model_override.clone())
            .unwrap_or_else(default_model_for_unset_cli);
        let cwd_path = s.cwd.clone();
        let cwd = s.cwd.to_string_lossy().to_string();
        (model, cwd, cwd_path, config.system_prompt.clone())
    };
    let builtin_tool_options = mossen_tools::ToolRuntimeOptions {
        mcp_resources: config.mcp_enabled,
    };

    let session_start_source = if config.restore_mode {
        mossen_utils::session_start::SessionStartSource::Resume
    } else {
        mossen_utils::session_start::SessionStartSource::Startup
    };
    let session_hook_messages = crate::session_hooks::run_session_start_hooks(
        &state,
        session_start_source,
        Some(&model),
        false,
    )
    .await;
    if !session_hook_messages.is_empty() {
        info!(
            target: "mossen_agent::hooks",
            count = session_hook_messages.len(),
            "SessionStart hook messages produced for REPL startup"
        );
    }
    let hook_initial_prompt = mossen_utils::session_start::take_initial_user_message();

    mossen_skills::init_bundled_skills();

    // Skill dynamic discovery: interactive sessions need user-level skills
    // plus project-local `.mossen/skills` from turn one.
    let skill_load_report =
        mossen_skills::load_startup_skill_directories(&cwd_path, ".mossen").await;
    if skill_load_report.user_dir_present
        || skill_load_report.project_dir_count > 0
        || skill_load_report.added_skill_count > 0
    {
        info!(
            target: "mossen_cli::skills",
            user_dir_present = skill_load_report.user_dir_present,
            project_dirs = skill_load_report.project_dir_count,
            added = skill_load_report.added_skill_count,
            "skill directories loaded during REPL startup"
        );
    }

    let activated =
        mossen_skills::activate_conditional_skills_for_paths(std::slice::from_ref(&cwd), &cwd_path);
    if !activated.is_empty() {
        info!(
            target: "mossen_cli::skills",
            activated = ?activated,
            "conditional skills activated during REPL startup"
        );
    }

    let compact_hook_context = match crate::session_hooks::build_hooks_context(&state, false) {
        Ok(context) => Some(Arc::new(context)),
        Err(err) => {
            warn!(
                target: "mossen_agent::hooks",
                error = %err,
                "Hook context unavailable; REPL hooks will continue disabled"
            );
            None
        }
    };
    let _config_change_hook_listener =
        crate::session_hooks::install_config_change_hook_listener(compact_hook_context.clone());

    // Assemble the layered system prompt once per session. If the caller
    // supplied a `--system-prompt` override on `ReplConfig` we honour that
    // verbatim (escape hatch for tests / overrides); otherwise we run the
    // full composer so the model has an identity, env info, language hint
    // and tool-use guidance from turn one. Without this the assistant has
    // no context about cwd / platform / model and behaves like a generic
    // chat completion.
    let system_prompt_blocks = if let Some(text) = system_prompt_override {
        vec![mossen_agent::types::SystemBlock {
            text,
            cache_control: None,
        }]
    } else {
        let is_custom = mossen_utils::custom_backend::is_custom_backend_enabled();
        let is_git = crate::system_prompt::detect_git_repo(&cwd_path);
        let skill_commands = mossen_tools::skill_tool::prompt::get_loaded_skill_tool_commands();
        let skill_commands_text =
            mossen_tools::skill_tool::prompt::format_commands_within_budget(&skill_commands, None);
        // Read project + user memory now (one-shot per session) so the
        // composer has the full instruction surface to inject.
        let memory_text = crate::system_prompt::gather_memory_text_with_hooks(
            &cwd_path,
            compact_hook_context.as_deref(),
        )
        .await;
        let enabled_tools = mossen_tools::all_tool_names_for_runtime(builtin_tool_options);
        let inputs = crate::system_prompt::SystemPromptInputs {
            cwd: &cwd,
            model: &model,
            model_marketing_name: None,
            is_non_interactive: false,
            is_fork_subagent: false,
            is_custom_backend: is_custom,
            is_internal: std::env::var("USER_TYPE").ok().as_deref() == Some("internal"),
            is_git_repo: is_git,
            product_name: "Mossen",
            enabled_tools: &enabled_tools,
            skill_commands_count: skill_commands.len(),
            skill_commands_text: &skill_commands_text,
            are_explore_plan_agents:
                mossen_tools::agent_tool::built_in_agents::are_explore_plan_agents_enabled(),
            explore_agent_type: "explore",
            language_preference: Some("Chinese"),
            memory_text: &memory_text,
        };
        crate::system_prompt::assemble(&inputs)
    };
    info!(
        block_count = system_prompt_blocks.len(),
        "launch_repl: system prompt assembled"
    );

    // 2. 创建 TUI App（内部管理事件总线 + Agent 引擎 + 命令注册表）
    let engine_config = mossen_tui::app::EngineConfig {
        model,
        system_prompt: system_prompt_blocks,
        cwd,
        api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
        api_key: std::env::var("MOSSEN_API_KEY").ok(),
        origin_tag: OriginTag::Repl,
        max_turns: None,
        fast_mode: parse_env_bool("MOSSEN_FAST_MODE"),
        effort: std::env::var("MOSSEN_CODE_EFFORT_LEVEL")
            .ok()
            .and_then(|value| parse_effort_level(&value)),
        extra_body: Default::default(),
        output_style: None,
        compact_hook_context,
    };
    let directives = std::sync::Arc::new(mossen_commands::all_directives());
    let app = mossen_tui::App::with_engine(engine_config, directives)
        .with_startup_hook_messages(session_hook_messages)
        .with_startup_render_session_restore(config.restore_mode);
    info!("launch_repl: TUI app created (engine + directives wired)");

    // Skill registry is built once and handed to the TUI so slash commands,
    // tool execution, and future agent-side hooks can look up skills by id.
    // (Previously this registry was created and immediately dropped, which
    // left the entire skill subsystem unreachable from the running session.)
    let skill_registry = mossen_skills::new_shared_registry();
    let app = app.with_skill_registry(skill_registry);
    info!("launch_repl: skill registry attached to App");

    // Build the executable tool registry from every built-in `mossen_tools`
    // implementation. The registry serves three purposes that all must be
    // wired before the first prompt fires:
    //   1. `App::handle_submit` pulls `ToolDefinition`s from it for the
    //      `tools` field on the OpenAI request body — without this the
    //      model sees no tools and falls back to bash-in-markdown text.
    //   2. The same `Arc<ToolRegistry>` is cloned into
    //      `PromptParams::tool_registry`, forwarded to
    //      `OrchestratorConfig`, and used by `dialogue.rs` to actually
    //      dispatch each `tool_use` block the model emits.
    //   3. Future MCP tools can be merged into the same registry so
    //      tool lookup remains a single code path.
    let mut tool_registry = mossen_agent::tool_registry::ToolRegistry::new();
    tool_registry.register_all(filtered_builtin_tools(builtin_tool_options, &config));
    let tool_registry = Arc::new(tool_registry);
    info!(
        tool_count = tool_registry.len(),
        "launch_repl: tool registry built from runtime-gated mossen tools"
    );
    let app = app.with_tool_registry(tool_registry);

    let app = if config.mcp_enabled {
        let mcp_reload_config = config.clone();
        let mcp_reload_cwd = cwd_path.clone();
        let mcp_reload_callback: mossen_tui::app::McpReloadCallback = Arc::new(move || {
            let config = mcp_reload_config.clone();
            let cwd = mcp_reload_cwd.clone();
            Box::pin(async move {
                if let Some(manager) = crate::repl_mcp::get_manager() {
                    manager.disconnect_all().await;
                }
                crate::repl_mcp::clear_manager();
                mossen_mcp::server::clear_global_manager();

                if let Some((mcp_manager, tool_definitions, server_count)) =
                    initialize_mcp_manager_for_session(&config, &cwd, "reload-plugins").await
                {
                    let connected_count = mcp_manager.connected_count();
                    crate::repl_mcp::set_manager(mcp_manager.clone());
                    mossen_mcp::server::set_global_manager(mcp_manager);
                    Ok(mossen_tui::app::McpRuntimeReloadResult {
                        tool_definitions,
                        server_count,
                        connected_count,
                    })
                } else {
                    Ok(mossen_tui::app::McpRuntimeReloadResult::default())
                }
            }) as mossen_tui::app::McpReloadFuture
        });
        app.with_mcp_reload_callback(mcp_reload_callback)
    } else {
        app
    };

    // Hook the live TaskStore into the TUI with lightweight snapshots so
    // process/status rendering never clones completed task output.
    let task_notification_rx = mossen_tools::task_store::subscribe_task_events();
    let mut app = app
        .with_task_snapshot_provider(std::sync::Arc::new(|| {
            mossen_tools::task_store::list_task_snapshots()
                .into_iter()
                .map(|t| (t.status, t.id, t.subject))
                .collect()
        }))
        .with_task_notification_receiver(task_notification_rx);

    // 4. 初始化 MCP 服务器管理器（如果配置了）
    if config.mcp_enabled {
        if let Some((mcp_manager, mcp_tool_definitions, _server_count)) =
            initialize_mcp_manager_for_session(&config, &cwd_path, "repl").await
        {
            let mcp_tool_count = mcp_tool_definitions.len();
            if mcp_tool_count > 0 {
                app = app.with_extra_tool_definitions(mcp_tool_definitions);
            }
            info!(
                tool_count = mcp_tool_count,
                "launch_repl: MCP tool definitions attached to App"
            );

            // 双重安装：
            //   1. mossen-cli 的 repl_mcp 全局，让 shutdown 路径能 disconnect_all。
            //   2. mossen-mcp 的 OnceLock 全局，让 dialogue.rs::execute_mcp_tool
            //      可以跨 crate 解析 mcp__server__tool 调用而不在 mossen-mcp 和
            //      mossen-cli 之间引入循环依赖。
            crate::repl_mcp::set_manager(mcp_manager.clone());
            mossen_mcp::server::set_global_manager(mcp_manager);
        }
    }

    // 6. 处理恢复模式
    if config.restore_mode {
        if let Some(ref session_id) = config.restore_session_id {
            info!(session_id = %session_id, "launch_repl: restoring session");
            let mut s = state
                .write()
                .map_err(|e| anyhow::anyhow!("failed to write state: {}", e))?;
            s.switch_session(session_id.clone());
        }
    }

    let startup_prompt = config.initial_prompt.clone().or(hook_initial_prompt);
    if let Some(prompt) = startup_prompt {
        app.queue_startup_prompt(prompt);
    }

    // 7. 运行 TUI 事件循环
    info!("launch_repl: starting TUI event loop");
    run_event_loop(app, state, config.shutdown_flag).await?;

    info!("launch_repl: session ended");
    Ok(())
}

/// 执行单次（oneshot）模式 — 对应 TS 的 --print/-p 模式。
///
/// 提交一次提示，等待完成后输出结果并退出。
pub async fn run_oneshot(
    state: SharedBootstrapState,
    prompt: String,
    _instruments: InstrumentRegistry,
    config: ReplConfig,
) -> Result<String> {
    let _span = tracing::info_span!("oneshot").entered();
    info!("run_oneshot: executing single prompt");

    let (mut prompt_params, prompt) =
        build_oneshot_prompt_params(state.clone(), prompt, &config).await?;
    let shutdown_cancel_bridge = ShutdownCancelBridge::new(config.shutdown_flag.clone());
    prompt_params.cancel_token = Some(shutdown_cancel_bridge.token());
    let transcript_history = prompt_params.history_messages.clone();
    let transcript_model = prompt_params.model.clone();
    info!(
        prompt_len = prompt.len(),
        "run_oneshot: dispatching to agent"
    );

    let mut rx = submit_prompt(prompt_params).await;
    let mut result_text = String::new();
    let mut terminal_reason: Option<String> = None;

    // 消费 agent 流，收集 assistant 文本内容
    while let Some(msg) = rx.recv().await {
        match msg {
            SdkMessage::Assistant { message, .. } => {
                for block in &message.content {
                    if let mossen_types::ContentBlock::Text(t) = block {
                        result_text.push_str(&t.text);
                    }
                }
            }
            SdkMessage::Result { terminal, .. } => {
                terminal_reason = Some(terminal);
                break;
            }
            _ => {}
        }
    }

    if result_text.is_empty() {
        // 兜底：至少返回 terminal reason，永不返回完全空的结果
        result_text = format!(
            "(oneshot completed: terminal={}, prompt_chars={})",
            terminal_reason.as_deref().unwrap_or("unknown"),
            prompt.len()
        );
    }
    info!("run_oneshot: completed");
    if let Err(err) = record_oneshot_transcript(
        &state,
        &transcript_history,
        &prompt,
        &result_text,
        &transcript_model,
    )
    .await
    {
        warn!(error = %err, "failed to record oneshot transcript");
    }
    Ok(result_text)
}

/// Stream-json oneshot path: emit every agent `SdkMessage` as NDJSON and keep
/// stdin open for SDK control requests while the turn is running.
pub async fn run_oneshot_stream_json(
    state: SharedBootstrapState,
    prompt: String,
    _instruments: InstrumentRegistry,
    config: ReplConfig,
) -> Result<()> {
    let _span = tracing::info_span!("oneshot_stream_json").entered();
    info!("run_oneshot_stream_json: executing single prompt");

    let (mut prompt_params, prompt) = build_oneshot_prompt_params(state, prompt, &config).await?;
    let shutdown_cancel_bridge = ShutdownCancelBridge::new(config.shutdown_flag.clone());
    prompt_params.cancel_token = Some(shutdown_cancel_bridge.token());
    let render_event_emitter =
        Arc::new(tokio::sync::Mutex::new(StreamJsonRenderEventEmitter::new()));
    let io = Arc::new(StructuredIO::new_with_render_event_emitter(
        false,
        render_event_emitter.clone(),
    ));
    let mut outbound_rx = io
        .take_outbound_rx()
        .await
        .ok_or_else(|| anyhow::anyhow!("stream-json outbound receiver already taken"))?;

    let stdout_task = tokio::spawn(async move {
        let mut stdout = tokio::io::BufWriter::new(tokio::io::stdout());
        while let Some(message) = outbound_rx.recv().await {
            match serde_json::to_string(&message) {
                Ok(json) => {
                    let line = ndjson_safe_stringify(&json);
                    if stdout.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    if stdout.write_all(b"\n").await.is_err() {
                        break;
                    }
                    if stdout.flush().await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    warn!(error = %err, "failed to serialize stream-json stdout message");
                }
            }
        }
        let _ = stdout.flush().await;
    });

    let stdin_io = Arc::clone(&io);
    let stdin_task = tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let reader = tokio::io::BufReader::new(stdin);
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Err(err) = stdin_io.process_line(&line).await {
                        warn!(error = %err, "failed to process stream-json stdin line");
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    warn!(error = %err, "failed to read stream-json stdin line");
                    break;
                }
            }
        }
        stdin_io.mark_input_closed().await;
    });

    info!(
        prompt_len = prompt.len(),
        "run_oneshot_stream_json: dispatching to agent"
    );
    let start = std::time::Instant::now();
    let mut rx = submit_prompt(prompt_params).await;
    let mut saw_result = false;

    while let Some(msg) = rx.recv().await {
        let is_result = matches!(msg, SdkMessage::Result { .. });
        let render_items = {
            let mut emitter = render_event_emitter.lock().await;
            emitter.emit_stream_items_for_sdk_message(&msg)
        };
        let value = serde_json::to_value(&msg)?;
        if io
            .outbound
            .send(StdoutMessage::StreamEvent(value))
            .await
            .is_err()
        {
            break;
        }
        let mut outbound_closed = false;
        for item in render_items {
            if io
                .outbound
                .send(StdoutMessage::StreamEvent(item))
                .await
                .is_err()
            {
                outbound_closed = true;
                break;
            }
        }
        if outbound_closed {
            break;
        }
        if is_result {
            saw_result = true;
            break;
        }
    }

    if !saw_result {
        let fallback = SdkMessage::Result {
            terminal: "stream_ended_without_result".to_string(),
            cost_usd: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            usage: None,
            task_id: None,
        };
        let render_items = {
            let mut emitter = render_event_emitter.lock().await;
            emitter.emit_stream_items_for_sdk_message(&fallback)
        };
        let value = serde_json::to_value(&fallback)?;
        let _ = io.outbound.send(StdoutMessage::StreamEvent(value)).await;
        for item in render_items {
            let _ = io.outbound.send(StdoutMessage::StreamEvent(item)).await;
        }
    }

    stdin_task.abort();
    let _ = stdin_task.await;
    drop(io);
    let _ = stdout_task.await;

    info!("run_oneshot_stream_json: completed");
    Ok(())
}

/// Terminal-render oneshot path: consume the same stream-json render items in
/// process and apply `render_draw_plan` values to the local TTY. This keeps
/// NDJSON transport pure while providing a real Codex-CLI-like frontend path.
pub async fn run_oneshot_terminal_render(
    state: SharedBootstrapState,
    prompt: String,
    _instruments: InstrumentRegistry,
    config: ReplConfig,
) -> Result<()> {
    let _span = tracing::info_span!("oneshot_terminal_render").entered();
    info!("run_oneshot_terminal_render: executing single prompt");

    let (mut prompt_params, prompt) = build_oneshot_prompt_params(state, prompt, &config).await?;
    let (permission_request_tx, mut permission_request_rx) =
        tokio::sync::mpsc::channel::<PermissionRequest>(16);
    let start = std::time::Instant::now();
    let mut saw_result = false;
    let mut render_event_emitter = StreamJsonRenderEventEmitter::new();
    let mut draw_runtime = StreamJsonTerminalDrawRuntime::for_current_terminal();
    let mut approval_bridge = TerminalRenderApprovalBridge::default();
    let mut stdout = io::stdout();
    let mut pending_flush_due_ms: Option<u64> = None;
    let (terminal_event_tx, mut terminal_event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (terminal_priority_event_tx, mut terminal_priority_event_rx) =
        tokio::sync::mpsc::unbounded_channel();
    let terminal_edit_capture_active = Arc::new(AtomicBool::new(false));
    let terminal_resize_event_pending = Arc::new(AtomicBool::new(false));
    let terminal_scroll_event_pending = Arc::new(AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE));
    let mut terminal_priority_events_since_fairness_yield = 0usize;
    let terminal_frontend_event_log = terminal_render_open_frontend_event_log();
    let terminal_input_capture_guard = terminal_render_enable_input_capture(&mut stdout)?;
    let terminal_event_pump_guard = if terminal_input_capture_guard.is_some() {
        terminal_render_spawn_frontend_event_pump(
            terminal_event_tx,
            terminal_priority_event_tx,
            terminal_edit_capture_active.clone(),
            terminal_resize_event_pending.clone(),
            terminal_scroll_event_pending.clone(),
        )
    } else {
        None
    };
    let mut terminal_events_open = terminal_event_pump_guard.is_some();
    let mut permission_requests_open = terminal_events_open;
    if permission_requests_open {
        let permission_gate: Arc<dyn PermissionGate> =
            Arc::new(InteractiveGate::new(permission_request_tx));
        prompt_params.permission_gate = Some(permission_gate);
    }

    info!(
        prompt_len = prompt.len(),
        "run_oneshot_terminal_render: dispatching to agent"
    );
    let shutdown_cancel_bridge = ShutdownCancelBridge::new(config.shutdown_flag.clone());
    let terminal_cancel_token = shutdown_cancel_bridge.token();
    prompt_params.cancel_token = Some(terminal_cancel_token.clone());
    render_event_emitter.seed_terminal_session_model(&prompt_params.model);
    terminal_render_submit_status_heartbeat(
        &mut render_event_emitter,
        &mut draw_runtime,
        &mut stdout,
        start,
        &mut pending_flush_due_ms,
    )?;
    let mut terminal_status_heartbeat_due_ms = Some(TERMINAL_RENDER_STATUS_HEARTBEAT_MS);
    let mut rx = submit_prompt(prompt_params).await;

    loop {
        let loop_now_ms = elapsed_millis_since(start);
        if let Some(due_ms) = pending_flush_due_ms {
            if loop_now_ms >= due_ms {
                let report = draw_runtime.flush_pending_at(loop_now_ms, &mut stdout)?;
                pending_flush_due_ms = terminal_render_next_flush_due(&draw_runtime, &report);
                terminal_render_reset_priority_fairness_budget(
                    &mut terminal_priority_events_since_fairness_yield,
                );
                continue;
            }
        }
        if terminal_render_status_heartbeat_due(loop_now_ms, terminal_status_heartbeat_due_ms) {
            terminal_render_submit_status_heartbeat(
                &mut render_event_emitter,
                &mut draw_runtime,
                &mut stdout,
                start,
                &mut pending_flush_due_ms,
            )?;
            terminal_status_heartbeat_due_ms =
                Some(loop_now_ms.saturating_add(TERMINAL_RENDER_STATUS_HEARTBEAT_MS));
            terminal_render_reset_priority_fairness_budget(
                &mut terminal_priority_events_since_fairness_yield,
            );
            continue;
        }
        let terminal_status_heartbeat_sleep_ms = terminal_render_status_heartbeat_sleep_ms(
            loop_now_ms,
            terminal_status_heartbeat_due_ms,
        );

        if let Some(due_ms) = pending_flush_due_ms {
            let now_ms = elapsed_millis_since(start);
            tokio::select! {
                biased;
                maybe_event = terminal_priority_event_rx.recv(),
                    if terminal_events_open && terminal_render_priority_fairness_allows(terminal_priority_events_since_fairness_yield) => {
                    if let Some(event) = maybe_event {
                        terminal_render_write_frontend_event_stage_log(
                            terminal_frontend_event_log.as_ref(),
                            "priority_recv",
                            &event,
                        );
                        terminal_render_note_priority_frontend_event(
                            &mut terminal_priority_events_since_fairness_yield,
                        );
                        if terminal_render_handle_frontend_event(
                            event,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                            &terminal_edit_capture_active,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                            &terminal_cancel_token,
                        )? {
                            saw_result = true;
                            break;
                        }
                        let drain_report =
                            terminal_render_drain_superseded_low_priority_frontend_events(
                            &mut terminal_event_rx,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                        );
                        terminal_render_submit_follow_up_after_priority_drain(
                            drain_report,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                        )?;
                    } else {
                        terminal_events_open = false;
                        terminal_render_reset_priority_fairness_budget(
                            &mut terminal_priority_events_since_fairness_yield,
                        );
                    }
                }
                maybe_msg = rx.recv() => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    let Some(msg) = maybe_msg else {
                        break;
                    };
                    if terminal_render_handle_sdk_message(
                        &msg,
                        &mut render_event_emitter,
                        &mut draw_runtime,
                        &mut stdout,
                        start,
                        &mut pending_flush_due_ms,
                    )? {
                        saw_result = true;
                        break;
                    }
                }
                maybe_event = terminal_event_rx.recv(), if terminal_events_open => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    if let Some(event) = maybe_event {
                        terminal_render_write_frontend_event_stage_log(
                            terminal_frontend_event_log.as_ref(),
                            "event_recv",
                            &event,
                        );
                        if terminal_render_handle_frontend_event(
                            event,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                            &terminal_edit_capture_active,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                            &terminal_cancel_token,
                        )? {
                            saw_result = true;
                            break;
                        }
                    } else {
                        terminal_events_open = false;
                    }
                }
                maybe_permission = permission_request_rx.recv(), if permission_requests_open => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    if let Some(request) = maybe_permission {
                        terminal_render_handle_permission_request(
                            request,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                        )?;
                    } else {
                        permission_requests_open = false;
                    }
                }
                _ = tokio::task::yield_now(),
                    if terminal_events_open && terminal_render_priority_fairness_yield_due(terminal_priority_events_since_fairness_yield) => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                }
                _ = tokio::time::sleep(Duration::from_millis(terminal_status_heartbeat_sleep_ms.unwrap_or(0))),
                    if terminal_status_heartbeat_sleep_ms.is_some() => {}
                _ = tokio::time::sleep(Duration::from_millis(due_ms.saturating_sub(now_ms))) => {
                    let now_ms = elapsed_millis_since(start);
                    let report = draw_runtime.flush_pending_at(now_ms, &mut stdout)?;
                    pending_flush_due_ms = terminal_render_next_flush_due(&draw_runtime, &report);
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                }
            }
        } else {
            tokio::select! {
                biased;
                maybe_event = terminal_priority_event_rx.recv(),
                    if terminal_events_open && terminal_render_priority_fairness_allows(terminal_priority_events_since_fairness_yield) => {
                    if let Some(event) = maybe_event {
                        terminal_render_write_frontend_event_stage_log(
                            terminal_frontend_event_log.as_ref(),
                            "priority_recv",
                            &event,
                        );
                        terminal_render_note_priority_frontend_event(
                            &mut terminal_priority_events_since_fairness_yield,
                        );
                        if terminal_render_handle_frontend_event(
                            event,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                            &terminal_edit_capture_active,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                            &terminal_cancel_token,
                        )? {
                            saw_result = true;
                            break;
                        }
                        let drain_report =
                            terminal_render_drain_superseded_low_priority_frontend_events(
                            &mut terminal_event_rx,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                        );
                        terminal_render_submit_follow_up_after_priority_drain(
                            drain_report,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                        )?;
                    } else {
                        terminal_events_open = false;
                        terminal_render_reset_priority_fairness_budget(
                            &mut terminal_priority_events_since_fairness_yield,
                        );
                    }
                }
                maybe_msg = rx.recv() => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    let Some(msg) = maybe_msg else {
                        break;
                    };
                    if terminal_render_handle_sdk_message(
                        &msg,
                        &mut render_event_emitter,
                        &mut draw_runtime,
                        &mut stdout,
                        start,
                        &mut pending_flush_due_ms,
                    )? {
                        saw_result = true;
                        break;
                    }
                }
                maybe_event = terminal_event_rx.recv(), if terminal_events_open => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    if let Some(event) = maybe_event {
                        terminal_render_write_frontend_event_stage_log(
                            terminal_frontend_event_log.as_ref(),
                            "event_recv",
                            &event,
                        );
                        if terminal_render_handle_frontend_event(
                            event,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                            &terminal_edit_capture_active,
                            &terminal_resize_event_pending,
                            &terminal_scroll_event_pending,
                            &terminal_cancel_token,
                        )? {
                            saw_result = true;
                            break;
                        }
                    } else {
                        terminal_events_open = false;
                    }
                }
                maybe_permission = permission_request_rx.recv(), if permission_requests_open => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                    if let Some(request) = maybe_permission {
                        terminal_render_handle_permission_request(
                            request,
                            &mut render_event_emitter,
                            &mut draw_runtime,
                            &mut approval_bridge,
                            &mut stdout,
                            start,
                            &mut pending_flush_due_ms,
                        )?;
                    } else {
                        permission_requests_open = false;
                    }
                }
                _ = tokio::task::yield_now(),
                    if terminal_events_open && terminal_render_priority_fairness_yield_due(terminal_priority_events_since_fairness_yield) => {
                    terminal_render_reset_priority_fairness_budget(
                        &mut terminal_priority_events_since_fairness_yield,
                    );
                }
                _ = tokio::time::sleep(Duration::from_millis(terminal_status_heartbeat_sleep_ms.unwrap_or(0))),
                    if terminal_status_heartbeat_sleep_ms.is_some() => {}
            }
        }
    }

    if !saw_result {
        let fallback = SdkMessage::Result {
            terminal: "stream_ended_without_result".to_string(),
            cost_usd: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            usage: None,
            task_id: None,
        };
        let _ = terminal_render_handle_sdk_message(
            &fallback,
            &mut render_event_emitter,
            &mut draw_runtime,
            &mut stdout,
            start,
            &mut pending_flush_due_ms,
        )?;
    }

    if draw_runtime.has_pending_draw() {
        let _ = draw_runtime.release_manual_scroll_for_terminal_teardown();
        let now_ms = elapsed_millis_since(start);
        let flush_ms = pending_flush_due_ms.unwrap_or(now_ms).max(now_ms);
        let _ = draw_runtime.flush_pending_at(flush_ms, &mut stdout)?;
    }
    terminal_render_export_final_diagnostics_if_requested(&draw_runtime);
    drop(terminal_event_pump_guard);
    drop(terminal_input_capture_guard);
    writeln!(stdout)?;
    stdout.flush()?;

    info!("run_oneshot_terminal_render: completed");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalRenderFrontendEvent {
    ManualScrollStart,
    ManualScrollEnd,
    Interrupt,
    Resize,
    ToggleCommandExpansion,
    ToggleBackgroundTaskExpansion,
    ToggleFileChangeExpansion,
    ToggleDiffExpansion,
    ToggleErrorExpansion,
    FocusNextApprovalAction,
    FocusPreviousApprovalAction,
    ActivateFocusedApprovalAction,
    ActivateApprovalActionByKey(char),
    EditCommandInputChar(char),
    EditCommandPaste(String),
    EditCommandBackspace,
    EditCommandSubmit,
    EditCommandCancel,
}

const TERMINAL_RENDER_SCROLL_EVENT_NONE: u8 = 0;
const TERMINAL_RENDER_SCROLL_EVENT_START: u8 = 1;
const TERMINAL_RENDER_SCROLL_EVENT_END: u8 = 2;
const TERMINAL_RENDER_LOW_PRIORITY_DRAIN_LIMIT: usize = 64;
const TERMINAL_RENDER_PRIORITY_FAIRNESS_BURST_LIMIT: usize = 8;
const TERMINAL_RENDER_STATUS_HEARTBEAT_MS: u64 = 1_000;
const TERMINAL_RENDER_DIAGNOSTICS_PATH_ENV: &str = "MOSSEN_TERMINAL_RENDER_DIAGNOSTICS_PATH";
const TERMINAL_RENDER_FRONTEND_EVENT_LOG_PATH_ENV: &str =
    "MOSSEN_TERMINAL_RENDER_FRONTEND_EVENT_LOG_PATH";

struct TerminalRenderEventPump {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for TerminalRenderEventPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct TerminalRenderInputCaptureGuard {
    raw_mode_enabled: bool,
    bracketed_paste_enabled: bool,
    mouse_capture_enabled: bool,
}

impl Drop for TerminalRenderInputCaptureGuard {
    fn drop(&mut self) {
        if self.bracketed_paste_enabled {
            let _ = execute!(io::stdout(), DisableBracketedPaste);
            self.bracketed_paste_enabled = false;
        }
        if self.mouse_capture_enabled {
            let _ = execute!(io::stdout(), DisableMouseCapture);
            self.mouse_capture_enabled = false;
        }
        if self.raw_mode_enabled {
            let _ = disable_raw_mode();
            self.raw_mode_enabled = false;
        }
    }
}

fn terminal_render_enable_input_capture<W: Write>(
    writer: &mut W,
) -> Result<Option<TerminalRenderInputCaptureGuard>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(None);
    }

    // The pre-main early-input reader also polls crossterm. Stop it before the
    // terminal-render frontend starts, otherwise approval/edit keystrokes can
    // be split between two independent readers.
    mossen_utils::early_input::stop_capturing_early_input();

    enable_raw_mode().map_err(|e| anyhow::anyhow!("failed to enable raw mode: {}", e))?;
    let mut guard = TerminalRenderInputCaptureGuard {
        raw_mode_enabled: true,
        bracketed_paste_enabled: false,
        mouse_capture_enabled: false,
    };

    match execute!(writer, EnableBracketedPaste) {
        Ok(()) => {
            guard.bracketed_paste_enabled = true;
            let _ = writer.flush();
        }
        Err(err) => {
            warn!(
                error = %err,
                "terminal render bracketed paste unavailable; paste falls back to key events"
            );
        }
    }

    if terminal_render_should_capture_mouse() {
        match execute!(writer, EnableMouseCapture) {
            Ok(()) => {
                guard.mouse_capture_enabled = true;
                let _ = writer.flush();
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "terminal render mouse capture unavailable; keyboard controls remain enabled"
                );
            }
        }
    }

    Ok(Some(guard))
}

fn terminal_render_spawn_frontend_event_pump(
    tx: tokio::sync::mpsc::UnboundedSender<TerminalRenderFrontendEvent>,
    priority_tx: tokio::sync::mpsc::UnboundedSender<TerminalRenderFrontendEvent>,
    edit_capture_active: Arc<AtomicBool>,
    resize_event_pending: Arc<AtomicBool>,
    scroll_event_pending: Arc<AtomicU8>,
) -> Option<TerminalRenderEventPump> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let event_log = terminal_render_open_frontend_event_log();
    let handle = std::thread::Builder::new()
        .name("mossen-terminal-render-events".to_string())
        .spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match event::poll(Duration::from_millis(50)) {
                    Ok(true) => if let Ok(event) = event::read() {
                        if let Some(frontend_event) =
                            terminal_render_frontend_event_from_crossterm_with_edit_capture(
                                &event,
                                edit_capture_active.load(Ordering::SeqCst),
                            )
                        {
                            terminal_render_write_frontend_event_log(
                                event_log.as_ref(),
                                &event,
                                Some(&frontend_event),
                            );
                            let _ = terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                                &tx,
                                &priority_tx,
                                frontend_event,
                                &resize_event_pending,
                                &scroll_event_pending,
                            );
                        } else {
                            terminal_render_write_frontend_event_log(
                                event_log.as_ref(),
                                &event,
                                None,
                            );
                        }
                    },
                    Ok(false) => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        })
        .ok()?;

    Some(TerminalRenderEventPump {
        stop,
        handle: Some(handle),
    })
}

fn terminal_render_open_frontend_event_log() -> Option<Arc<Mutex<std::fs::File>>> {
    let path = std::env::var_os(TERMINAL_RENDER_FRONTEND_EVENT_LOG_PATH_ENV)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(|file| Arc::new(Mutex::new(file)))
}

fn terminal_render_write_frontend_event_log(
    event_log: Option<&Arc<Mutex<std::fs::File>>>,
    event: &Event,
    frontend_event: Option<&TerminalRenderFrontendEvent>,
) {
    let Some(event_log) = event_log else {
        return;
    };
    if let Ok(mut file) = event_log.lock() {
        let _ = writeln!(file, "event={event:?} frontend_event={frontend_event:?}");
    }
}

fn terminal_render_write_frontend_event_stage_log(
    event_log: Option<&Arc<Mutex<std::fs::File>>>,
    stage: &str,
    frontend_event: &TerminalRenderFrontendEvent,
) {
    let Some(event_log) = event_log else {
        return;
    };
    if let Ok(mut file) = event_log.lock() {
        let _ = writeln!(file, "stage={stage} frontend_event={frontend_event:?}");
    }
}

fn terminal_render_append_frontend_event_log_line(line: String) {
    let Some(path) = std::env::var_os(TERMINAL_RENDER_FRONTEND_EVENT_LOG_PATH_ENV) else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

fn terminal_render_frontend_event_snapshot_label(
    render_event_emitter: &StreamJsonRenderEventEmitter,
) -> String {
    let snapshot = render_event_emitter.snapshot_value();
    let activity_kind = snapshot
        .get("activity")
        .and_then(|activity| activity.get("kind"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("none");
    let approval_blocking = snapshot
        .get("terminal")
        .and_then(|terminal| terminal.get("approval"))
        .and_then(|approval| approval.get("blocking"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let focused_action = snapshot
        .get("terminal")
        .and_then(|terminal| terminal.get("approval"))
        .and_then(|approval| approval.get("actionModel"))
        .and_then(|model| model.get("focusedAction"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("none");
    format!(
        "activity={activity_kind} approval_blocking={approval_blocking} focused_action={focused_action}"
    )
}

fn terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
    tx: &tokio::sync::mpsc::UnboundedSender<TerminalRenderFrontendEvent>,
    priority_tx: &tokio::sync::mpsc::UnboundedSender<TerminalRenderFrontendEvent>,
    event: TerminalRenderFrontendEvent,
    resize_event_pending: &AtomicBool,
    scroll_event_pending: &AtomicU8,
) -> bool {
    let is_resize = matches!(event, TerminalRenderFrontendEvent::Resize);
    if is_resize
        && resize_event_pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        return false;
    }
    let scroll_state = terminal_render_scroll_event_pending_state(&event);
    if let Some(scroll_state) = scroll_state {
        let previous_scroll_state = scroll_event_pending.swap(scroll_state, Ordering::SeqCst);
        if previous_scroll_state != TERMINAL_RENDER_SCROLL_EVENT_NONE {
            return false;
        }
    }

    let priority = terminal_render_frontend_event_is_priority(&event);
    let send_result = if priority {
        priority_tx.send(event)
    } else {
        tx.send(event)
    };

    if send_result.is_err() {
        if is_resize {
            resize_event_pending.store(false, Ordering::SeqCst);
        }
        if let Some(scroll_state) = scroll_state {
            let _ = scroll_event_pending.compare_exchange(
                scroll_state,
                TERMINAL_RENDER_SCROLL_EVENT_NONE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
        return false;
    }
    true
}

fn terminal_render_frontend_event_is_priority(event: &TerminalRenderFrontendEvent) -> bool {
    !matches!(event, TerminalRenderFrontendEvent::Resize)
}

fn terminal_render_priority_fairness_allows(priority_events_since_yield: usize) -> bool {
    priority_events_since_yield < TERMINAL_RENDER_PRIORITY_FAIRNESS_BURST_LIMIT
}

fn terminal_render_priority_fairness_yield_due(priority_events_since_yield: usize) -> bool {
    !terminal_render_priority_fairness_allows(priority_events_since_yield)
}

fn terminal_render_note_priority_frontend_event(priority_events_since_yield: &mut usize) {
    *priority_events_since_yield = priority_events_since_yield.saturating_add(1);
}

fn terminal_render_reset_priority_fairness_budget(priority_events_since_yield: &mut usize) {
    *priority_events_since_yield = 0;
}

fn terminal_render_release_resize_frontend_event(
    event: &TerminalRenderFrontendEvent,
    resize_event_pending: &AtomicBool,
) {
    if matches!(event, TerminalRenderFrontendEvent::Resize) {
        resize_event_pending.store(false, Ordering::SeqCst);
    }
}

fn terminal_render_scroll_event_pending_state(event: &TerminalRenderFrontendEvent) -> Option<u8> {
    match event {
        TerminalRenderFrontendEvent::ManualScrollStart => Some(TERMINAL_RENDER_SCROLL_EVENT_START),
        TerminalRenderFrontendEvent::ManualScrollEnd => Some(TERMINAL_RENDER_SCROLL_EVENT_END),
        _ => None,
    }
}

fn terminal_render_release_scroll_frontend_event(
    event: &TerminalRenderFrontendEvent,
    scroll_event_pending: &AtomicU8,
) {
    let Some(scroll_state) = terminal_render_scroll_event_pending_state(event) else {
        return;
    };
    let _ = scroll_event_pending.compare_exchange(
        scroll_state,
        TERMINAL_RENDER_SCROLL_EVENT_NONE,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

fn terminal_render_take_scroll_frontend_event_state(
    event: &TerminalRenderFrontendEvent,
    scroll_event_pending: &AtomicU8,
) -> Option<u8> {
    let event_state = terminal_render_scroll_event_pending_state(event)?;
    let pending_state =
        scroll_event_pending.swap(TERMINAL_RENDER_SCROLL_EVENT_NONE, Ordering::SeqCst);
    if pending_state == TERMINAL_RENDER_SCROLL_EVENT_NONE {
        Some(event_state)
    } else {
        Some(pending_state)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TerminalRenderLowPriorityDrainReport {
    drained_count: usize,
    drained_resize_event: bool,
    drained_scroll_event: bool,
    last_drained_scroll_state: u8,
}

fn terminal_render_drain_superseded_low_priority_frontend_events(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<TerminalRenderFrontendEvent>,
    resize_event_pending: &AtomicBool,
    scroll_event_pending: &AtomicU8,
) -> TerminalRenderLowPriorityDrainReport {
    let mut report = TerminalRenderLowPriorityDrainReport::default();
    while report.drained_count < TERMINAL_RENDER_LOW_PRIORITY_DRAIN_LIMIT {
        let Ok(event) = rx.try_recv() else {
            break;
        };
        if matches!(event, TerminalRenderFrontendEvent::Resize) {
            report.drained_resize_event = true;
        }
        if let Some(scroll_state) =
            terminal_render_take_scroll_frontend_event_state(&event, scroll_event_pending)
        {
            report.drained_scroll_event = true;
            report.last_drained_scroll_state = scroll_state;
        }
        terminal_render_release_resize_frontend_event(&event, resize_event_pending);
        report.drained_count = report.drained_count.saturating_add(1);
    }
    report
}

fn terminal_render_frontend_event_from_crossterm(
    event: &Event,
) -> Option<TerminalRenderFrontendEvent> {
    terminal_render_frontend_event_from_crossterm_with_edit_capture(event, false)
}

fn terminal_render_frontend_event_from_crossterm_with_edit_capture(
    event: &Event,
    edit_capture_active: bool,
) -> Option<TerminalRenderFrontendEvent> {
    match event {
        Event::Resize(_, _) => Some(TerminalRenderFrontendEvent::Resize),
        Event::Paste(text) => {
            if edit_capture_active {
                Some(TerminalRenderFrontendEvent::EditCommandPaste(text.clone()))
            } else {
                None
            }
        }
        Event::Key(key) => {
            if key.kind == KeyEventKind::Release {
                return None;
            }

            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            {
                return Some(TerminalRenderFrontendEvent::Interrupt);
            }

            if edit_capture_active {
                return match key.code {
                    KeyCode::Enter => Some(TerminalRenderFrontendEvent::EditCommandSubmit),
                    KeyCode::Esc => Some(TerminalRenderFrontendEvent::EditCommandCancel),
                    KeyCode::Backspace => Some(TerminalRenderFrontendEvent::EditCommandBackspace),
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(TerminalRenderFrontendEvent::EditCommandInputChar(c))
                    }
                    _ => None,
                };
            }

            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('l') | KeyCode::Char('L'))
            {
                return Some(TerminalRenderFrontendEvent::ManualScrollEnd);
            }

            match key.code {
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    Some(TerminalRenderFrontendEvent::ToggleCommandExpansion)
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    Some(TerminalRenderFrontendEvent::ToggleBackgroundTaskExpansion)
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    Some(TerminalRenderFrontendEvent::ToggleFileChangeExpansion)
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    Some(TerminalRenderFrontendEvent::ToggleDiffExpansion)
                }
                KeyCode::Char('x') | KeyCode::Char('X') => {
                    Some(TerminalRenderFrontendEvent::ToggleErrorExpansion)
                }
                KeyCode::Tab | KeyCode::Right => {
                    Some(TerminalRenderFrontendEvent::FocusNextApprovalAction)
                }
                KeyCode::BackTab | KeyCode::Left => {
                    Some(TerminalRenderFrontendEvent::FocusPreviousApprovalAction)
                }
                KeyCode::Enter => Some(TerminalRenderFrontendEvent::ActivateFocusedApprovalAction),
                KeyCode::Char('y') | KeyCode::Char('Y') => Some(
                    TerminalRenderFrontendEvent::ActivateApprovalActionByKey('y'),
                ),
                KeyCode::Char('n') | KeyCode::Char('N') => Some(
                    TerminalRenderFrontendEvent::ActivateApprovalActionByKey('n'),
                ),
                KeyCode::Char('e') | KeyCode::Char('E') => Some(
                    TerminalRenderFrontendEvent::ActivateApprovalActionByKey('e'),
                ),
                KeyCode::Char('a') | KeyCode::Char('A') => Some(
                    TerminalRenderFrontendEvent::ActivateApprovalActionByKey('a'),
                ),
                KeyCode::PageUp | KeyCode::Home | KeyCode::Up => {
                    Some(TerminalRenderFrontendEvent::ManualScrollStart)
                }
                KeyCode::PageDown | KeyCode::End | KeyCode::Down => {
                    Some(TerminalRenderFrontendEvent::ManualScrollEnd)
                }
                _ => None,
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(TerminalRenderFrontendEvent::ManualScrollStart),
            MouseEventKind::ScrollDown => Some(TerminalRenderFrontendEvent::ManualScrollEnd),
            _ => None,
        },
        _ => None,
    }
}

const TERMINAL_APPROVAL_ACTION_APPROVE_ONCE: &str = "approve_once";
const TERMINAL_APPROVAL_ACTION_REJECT: &str = "reject";
const TERMINAL_APPROVAL_ACTION_EDIT_COMMAND: &str = "edit_command";
const TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION: &str = "approve_for_session";
const TERMINAL_RENDER_CAPTURE_MOUSE_ENV: &str = "MOSSEN_TERMINAL_RENDER_CAPTURE_MOUSE";

fn terminal_render_should_capture_mouse() -> bool {
    std::env::var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
}

struct TerminalRenderPendingPermission {
    tool_name: String,
    input: serde_json::Value,
    responder: tokio::sync::oneshot::Sender<PermissionDecision>,
}

#[derive(Default)]
struct TerminalRenderApprovalBridge {
    pending: Option<TerminalRenderPendingPermission>,
    edit_command_buffer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalRenderApprovalBridgeResult {
    bridge_status: &'static str,
    submitted: bool,
    requires_decision_bridge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalRenderApprovalEditCommandResult {
    bridge_status: &'static str,
    editing: bool,
    submitted: bool,
    requires_decision_bridge: bool,
    command: Option<String>,
}

impl TerminalRenderApprovalBridge {
    fn set_pending(&mut self, request: PermissionRequest) {
        if let Some(previous) = self.pending.take() {
            let _ = previous.responder.send(PermissionDecision::Deny);
        }

        self.edit_command_buffer = None;
        self.pending = Some(TerminalRenderPendingPermission {
            tool_name: request.tool_name,
            input: request.input,
            responder: request.responder,
        });
    }

    fn pending_permission_context(&self) -> Option<(&str, &serde_json::Value)> {
        self.pending
            .as_ref()
            .map(|pending| (pending.tool_name.as_str(), &pending.input))
    }

    fn has_pending_permission(&self) -> bool {
        self.pending.is_some()
    }

    fn edit_command_is_active(&self) -> bool {
        self.edit_command_buffer.is_some()
    }

    fn begin_edit_command(&mut self) -> TerminalRenderApprovalEditCommandResult {
        let Some(pending) = self.pending.as_ref() else {
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "no_pending_permission",
                editing: false,
                submitted: false,
                requires_decision_bridge: true,
                command: None,
            };
        };

        let Some(command) =
            terminal_render_shell_command_for_permission(&pending.tool_name, &pending.input)
        else {
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "unsupported",
                editing: false,
                submitted: false,
                requires_decision_bridge: true,
                command: None,
            };
        };

        self.edit_command_buffer = Some(command.clone());
        TerminalRenderApprovalEditCommandResult {
            bridge_status: "editing",
            editing: true,
            submitted: false,
            requires_decision_bridge: true,
            command: Some(command),
        }
    }

    fn push_edit_command_char(&mut self, c: char) -> Option<String> {
        let buffer = self.edit_command_buffer.as_mut()?;
        if !c.is_control() {
            buffer.push(c);
        }
        Some(buffer.clone())
    }

    fn paste_edit_command_text(&mut self, text: &str) -> Option<String> {
        let buffer = self.edit_command_buffer.as_mut()?;
        let normalized = terminal_render_normalize_pasted_edit_command_text(text);
        if normalized.is_empty() {
            return None;
        }
        buffer.push_str(&normalized);
        Some(buffer.clone())
    }

    fn backspace_edit_command(&mut self) -> Option<String> {
        let buffer = self.edit_command_buffer.as_mut()?;
        buffer.pop();
        Some(buffer.clone())
    }

    fn cancel_edit_command(&mut self) -> TerminalRenderApprovalEditCommandResult {
        self.edit_command_buffer = None;
        TerminalRenderApprovalEditCommandResult {
            bridge_status: "cancelled",
            editing: false,
            submitted: false,
            requires_decision_bridge: true,
            command: None,
        }
    }

    fn cancel_pending_permission(&mut self) -> bool {
        self.edit_command_buffer = None;
        let Some(request) = self.pending.take() else {
            return false;
        };
        let _ = request.responder.send(PermissionDecision::Deny);
        true
    }

    fn submit_edited_command(&mut self) -> TerminalRenderApprovalEditCommandResult {
        let Some(command) = self.edit_command_buffer.take() else {
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "no_edit_command",
                editing: false,
                submitted: false,
                requires_decision_bridge: true,
                command: None,
            };
        };
        let command = command.trim().to_string();
        if command.is_empty() {
            self.edit_command_buffer = Some(String::new());
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "empty_command",
                editing: true,
                submitted: false,
                requires_decision_bridge: true,
                command: Some(String::new()),
            };
        }

        let Some(pending) = self.pending.take() else {
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "no_pending_permission",
                editing: false,
                submitted: false,
                requires_decision_bridge: true,
                command: Some(command),
            };
        };

        let Some(updated_input) = terminal_render_updated_input_for_edited_command(
            &pending.tool_name,
            &pending.input,
            &command,
        ) else {
            self.pending = Some(pending);
            return TerminalRenderApprovalEditCommandResult {
                bridge_status: "unsupported",
                editing: false,
                submitted: false,
                requires_decision_bridge: true,
                command: Some(command),
            };
        };

        let submitted = pending
            .responder
            .send(PermissionDecision::AllowWithUpdatedInput { updated_input })
            .is_ok();
        TerminalRenderApprovalEditCommandResult {
            bridge_status: if submitted {
                "submitted"
            } else {
                "send_failed"
            },
            editing: false,
            submitted,
            requires_decision_bridge: !submitted,
            command: Some(command),
        }
    }

    fn submit_action(&mut self, action_id: &str) -> TerminalRenderApprovalBridgeResult {
        let Some(pending) = self.pending.take() else {
            return TerminalRenderApprovalBridgeResult {
                bridge_status: "no_pending_permission",
                submitted: false,
                requires_decision_bridge: true,
            };
        };

        let decision = match action_id {
            TERMINAL_APPROVAL_ACTION_APPROVE_ONCE => PermissionDecision::Allow,
            TERMINAL_APPROVAL_ACTION_REJECT => PermissionDecision::Deny,
            TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION => PermissionDecision::AllowAlways,
            TERMINAL_APPROVAL_ACTION_EDIT_COMMAND => {
                self.pending = Some(pending);
                return TerminalRenderApprovalBridgeResult {
                    bridge_status: "unsupported",
                    submitted: false,
                    requires_decision_bridge: true,
                };
            }
            _ => {
                self.pending = Some(pending);
                return TerminalRenderApprovalBridgeResult {
                    bridge_status: "unsupported",
                    submitted: false,
                    requires_decision_bridge: true,
                };
            }
        };

        self.edit_command_buffer = None;
        let submitted = pending.responder.send(decision).is_ok();
        TerminalRenderApprovalBridgeResult {
            bridge_status: if submitted {
                "submitted"
            } else {
                "send_failed"
            },
            submitted,
            requires_decision_bridge: !submitted,
        }
    }
}

fn terminal_render_shell_command_for_permission(
    tool_name: &str,
    input: &serde_json::Value,
) -> Option<String> {
    if !terminal_render_tool_accepts_command_edit(tool_name) {
        return None;
    }
    input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn terminal_render_updated_input_for_edited_command(
    tool_name: &str,
    input: &serde_json::Value,
    command: &str,
) -> Option<serde_json::Value> {
    if !terminal_render_tool_accepts_command_edit(tool_name) || command.trim().is_empty() {
        return None;
    }
    let mut updated = input.clone();
    let object = updated.as_object_mut()?;
    object.insert(
        "command".to_string(),
        serde_json::Value::String(command.trim().to_string()),
    );
    Some(updated)
}

fn terminal_render_tool_accepts_command_edit(tool_name: &str) -> bool {
    matches!(tool_name, "Bash" | "PowerShell" | "Execute")
}

fn terminal_render_normalize_pasted_edit_command_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

fn terminal_render_handle_permission_request<W: Write>(
    request: PermissionRequest,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    approval_bridge: &mut TerminalRenderApprovalBridge,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    draw_runtime.set_manual_scroll_active(false);
    draw_runtime.set_viewport(StreamJsonTerminalViewport::current());
    approval_bridge.set_pending(request);
    let (tool_name, input) = approval_bridge
        .pending_permission_context()
        .map(|(tool_name, input)| (tool_name.to_string(), input.clone()))
        .unwrap_or_else(|| ("Tool".to_string(), serde_json::Value::Null));
    let render_items =
        render_event_emitter.emit_terminal_permission_request_draw_plan_items(&tool_name, &input);
    terminal_render_submit_stream_items(
        render_items,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )
}

fn terminal_render_submit_or_begin_approval_action<W: Write>(
    action_id: &str,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    approval_bridge: &mut TerminalRenderApprovalBridge,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
    edit_capture_active: &AtomicBool,
) -> Result<()> {
    let render_items = if action_id == TERMINAL_APPROVAL_ACTION_EDIT_COMMAND {
        let result = approval_bridge.begin_edit_command();
        edit_capture_active.store(result.editing, Ordering::SeqCst);
        render_event_emitter.emit_terminal_approval_edit_command_draw_plan_items(
            result.bridge_status,
            result.command.as_deref(),
            result.editing,
        )
    } else {
        edit_capture_active.store(false, Ordering::SeqCst);
        let result = approval_bridge.submit_action(action_id);
        render_event_emitter.emit_terminal_approval_bridge_status_draw_plan_items(
            result.bridge_status,
            result.submitted,
            result.requires_decision_bridge,
        )
    };
    terminal_render_submit_stream_items(
        render_items,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )
}

fn terminal_render_handle_frontend_event<W: Write>(
    event: TerminalRenderFrontendEvent,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    approval_bridge: &mut TerminalRenderApprovalBridge,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
    edit_capture_active: &AtomicBool,
    resize_event_pending: &AtomicBool,
    scroll_event_pending: &AtomicU8,
    cancel_token: &CancellationToken,
) -> Result<bool> {
    draw_runtime.set_viewport(StreamJsonTerminalViewport::current());
    terminal_render_append_frontend_event_log_line(format!(
        "handle_start event={event:?} snapshot={}",
        terminal_render_frontend_event_snapshot_label(render_event_emitter)
    ));
    match event {
        TerminalRenderFrontendEvent::Interrupt => {
            terminal_render_handle_interrupt(
                render_event_emitter,
                draw_runtime,
                approval_bridge,
                writer,
                start,
                pending_flush_due_ms,
                edit_capture_active,
                cancel_token,
            )?;
            return Ok(true);
        }
        TerminalRenderFrontendEvent::ManualScrollStart
        | TerminalRenderFrontendEvent::ManualScrollEnd => {
            let scroll_state =
                terminal_render_take_scroll_frontend_event_state(&event, scroll_event_pending);
            match scroll_state {
                Some(TERMINAL_RENDER_SCROLL_EVENT_START) => {
                    draw_runtime.set_manual_scroll_active(true);
                }
                Some(TERMINAL_RENDER_SCROLL_EVENT_END) => {
                    draw_runtime.set_manual_scroll_active(false);
                    if draw_runtime.has_pending_draw() {
                        let now_ms = elapsed_millis_since(start);
                        let report = draw_runtime.flush_pending_at(now_ms, writer)?;
                        *pending_flush_due_ms =
                            terminal_render_next_flush_due(draw_runtime, &report);
                    }
                }
                _ => {}
            }
        }
        TerminalRenderFrontendEvent::ToggleCommandExpansion => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ToggleCommandExpansion,
            );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::ToggleBackgroundTaskExpansion => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ToggleBackgroundTaskExpansion,
            );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::ToggleFileChangeExpansion => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ToggleFileChangeExpansion,
            );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::ToggleDiffExpansion => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ToggleDiffExpansion,
            );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::ToggleErrorExpansion => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ToggleErrorExpansion,
            );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::FocusNextApprovalAction => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::FocusNextApprovalAction,
            );
            terminal_render_append_frontend_event_log_line(format!(
                "handle_focus_next render_items={} snapshot={}",
                render_items.len(),
                terminal_render_frontend_event_snapshot_label(render_event_emitter)
            ));
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::FocusPreviousApprovalAction => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::FocusPreviousApprovalAction,
            );
            terminal_render_append_frontend_event_log_line(format!(
                "handle_focus_previous render_items={} snapshot={}",
                render_items.len(),
                terminal_render_frontend_event_snapshot_label(render_event_emitter)
            ));
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::ActivateFocusedApprovalAction => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ActivateFocusedApprovalAction,
            );
            let action_id = render_event_emitter.pending_terminal_approval_action_id();
            terminal_render_append_frontend_event_log_line(format!(
                "handle_activate_focused render_items={} action_id={action_id:?} snapshot={}",
                render_items.len(),
                terminal_render_frontend_event_snapshot_label(render_event_emitter)
            ));
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
            if let Some(action_id) = action_id {
                terminal_render_submit_or_begin_approval_action(
                    &action_id,
                    render_event_emitter,
                    draw_runtime,
                    approval_bridge,
                    writer,
                    start,
                    pending_flush_due_ms,
                    edit_capture_active,
                )?;
            }
        }
        TerminalRenderFrontendEvent::ActivateApprovalActionByKey(key) => {
            draw_runtime.set_manual_scroll_active(false);
            let render_items = render_event_emitter.emit_terminal_widget_control_draw_plan_items(
                StreamJsonTerminalWidgetControl::ActivateApprovalActionByKey(key),
            );
            let action_id = render_event_emitter.pending_terminal_approval_action_id();
            terminal_render_append_frontend_event_log_line(format!(
                "handle_activate_key key={key} render_items={} action_id={action_id:?} snapshot={}",
                render_items.len(),
                terminal_render_frontend_event_snapshot_label(render_event_emitter)
            ));
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
            if let Some(action_id) = action_id {
                terminal_render_submit_or_begin_approval_action(
                    &action_id,
                    render_event_emitter,
                    draw_runtime,
                    approval_bridge,
                    writer,
                    start,
                    pending_flush_due_ms,
                    edit_capture_active,
                )?;
            }
        }
        TerminalRenderFrontendEvent::EditCommandInputChar(c) => {
            draw_runtime.set_manual_scroll_active(false);
            if let Some(command) = approval_bridge.push_edit_command_char(c) {
                let render_items = render_event_emitter
                    .emit_terminal_approval_edit_command_draw_plan_items(
                        "editing",
                        Some(&command),
                        true,
                    );
                terminal_render_submit_stream_items(
                    render_items,
                    draw_runtime,
                    writer,
                    start,
                    pending_flush_due_ms,
                )?;
            }
        }
        TerminalRenderFrontendEvent::EditCommandPaste(text) => {
            draw_runtime.set_manual_scroll_active(false);
            if let Some(command) = approval_bridge.paste_edit_command_text(&text) {
                let render_items = render_event_emitter
                    .emit_terminal_approval_edit_command_draw_plan_items(
                        "editing",
                        Some(&command),
                        true,
                    );
                terminal_render_submit_stream_items(
                    render_items,
                    draw_runtime,
                    writer,
                    start,
                    pending_flush_due_ms,
                )?;
            }
        }
        TerminalRenderFrontendEvent::EditCommandBackspace => {
            draw_runtime.set_manual_scroll_active(false);
            if let Some(command) = approval_bridge.backspace_edit_command() {
                let render_items = render_event_emitter
                    .emit_terminal_approval_edit_command_draw_plan_items(
                        "editing",
                        Some(&command),
                        true,
                    );
                terminal_render_submit_stream_items(
                    render_items,
                    draw_runtime,
                    writer,
                    start,
                    pending_flush_due_ms,
                )?;
            }
        }
        TerminalRenderFrontendEvent::EditCommandSubmit => {
            draw_runtime.set_manual_scroll_active(false);
            let result = approval_bridge.submit_edited_command();
            edit_capture_active.store(result.editing, Ordering::SeqCst);
            let render_items = if result.submitted {
                render_event_emitter.emit_terminal_approval_bridge_status_draw_plan_items(
                    result.bridge_status,
                    result.submitted,
                    result.requires_decision_bridge,
                )
            } else {
                render_event_emitter.emit_terminal_approval_edit_command_draw_plan_items(
                    result.bridge_status,
                    result.command.as_deref(),
                    result.editing,
                )
            };
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::EditCommandCancel => {
            draw_runtime.set_manual_scroll_active(false);
            let result = approval_bridge.cancel_edit_command();
            edit_capture_active.store(false, Ordering::SeqCst);
            let render_items = render_event_emitter
                .emit_terminal_approval_edit_command_draw_plan_items(
                    result.bridge_status,
                    result.command.as_deref(),
                    result.editing,
                );
            terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            )?;
        }
        TerminalRenderFrontendEvent::Resize => {
            let render_items = render_event_emitter.emit_terminal_resize_draw_plan_items();
            let submit_result = terminal_render_submit_stream_items(
                render_items,
                draw_runtime,
                writer,
                start,
                pending_flush_due_ms,
            );
            terminal_render_release_resize_frontend_event(
                &TerminalRenderFrontendEvent::Resize,
                resize_event_pending,
            );
            submit_result?;
        }
    }
    Ok(false)
}

fn terminal_render_submit_resize_redraw_after_priority_drain<W: Write>(
    drain_report: TerminalRenderLowPriorityDrainReport,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    if !drain_report.drained_resize_event {
        return Ok(());
    }
    draw_runtime.set_viewport(StreamJsonTerminalViewport::current());
    let render_items = render_event_emitter.emit_terminal_resize_draw_plan_items();
    terminal_render_submit_stream_items(
        render_items,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )
}

fn terminal_render_submit_follow_up_after_priority_drain<W: Write>(
    drain_report: TerminalRenderLowPriorityDrainReport,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    terminal_render_release_manual_scroll_end_after_priority_drain(
        drain_report,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )?;
    terminal_render_submit_resize_redraw_after_priority_drain(
        drain_report,
        render_event_emitter,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )
}

fn terminal_render_release_manual_scroll_end_after_priority_drain<W: Write>(
    drain_report: TerminalRenderLowPriorityDrainReport,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    if drain_report.last_drained_scroll_state != TERMINAL_RENDER_SCROLL_EVENT_END {
        return Ok(());
    }
    draw_runtime.set_manual_scroll_active(false);
    if draw_runtime.has_pending_draw() {
        let now_ms = elapsed_millis_since(start);
        let report = draw_runtime.flush_pending_at(now_ms, writer)?;
        *pending_flush_due_ms = terminal_render_next_flush_due(draw_runtime, &report);
    }
    Ok(())
}

fn terminal_render_handle_interrupt<W: Write>(
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    approval_bridge: &mut TerminalRenderApprovalBridge,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
    edit_capture_active: &AtomicBool,
    cancel_token: &CancellationToken,
) -> Result<()> {
    cancel_token.cancel();
    edit_capture_active.store(false, Ordering::SeqCst);
    draw_runtime.set_manual_scroll_active(false);

    if approval_bridge.cancel_pending_permission() {
        let render_items = render_event_emitter
            .emit_terminal_approval_bridge_status_draw_plan_items("interrupted", true, false);
        terminal_render_submit_stream_items(
            render_items,
            draw_runtime,
            writer,
            start,
            pending_flush_due_ms,
        )?;
    }

    let cancelled = SdkMessage::Result {
        terminal: "cancelled".to_string(),
        cost_usd: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        usage: None,
        task_id: None,
    };
    let _ = terminal_render_handle_sdk_message(
        &cancelled,
        render_event_emitter,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )?;
    Ok(())
}

fn terminal_render_status_heartbeat_due(now_ms: u64, due_ms: Option<u64>) -> bool {
    due_ms.is_some_and(|due_ms| now_ms >= due_ms)
}

fn terminal_render_status_heartbeat_sleep_ms(now_ms: u64, due_ms: Option<u64>) -> Option<u64> {
    due_ms.and_then(|due_ms| (due_ms > now_ms).then_some(due_ms.saturating_sub(now_ms)))
}

fn terminal_render_submit_status_heartbeat<W: Write>(
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    if draw_runtime.has_pending_draw() || draw_runtime.runtime_snapshot().manual_scroll_active {
        return Ok(());
    }
    draw_runtime.set_viewport(StreamJsonTerminalViewport::current());
    let render_items = render_event_emitter.emit_terminal_status_heartbeat_draw_plan_items();
    terminal_render_submit_stream_items(
        render_items,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )
}

fn terminal_render_handle_sdk_message<W: Write>(
    msg: &SdkMessage,
    render_event_emitter: &mut StreamJsonRenderEventEmitter,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<bool> {
    draw_runtime.set_viewport(StreamJsonTerminalViewport::current());
    let render_items = render_event_emitter.emit_terminal_draw_plan_items_for_sdk_message(msg);
    terminal_render_submit_stream_items(
        render_items,
        draw_runtime,
        writer,
        start,
        pending_flush_due_ms,
    )?;
    Ok(matches!(msg, SdkMessage::Result { .. }))
}

fn terminal_render_submit_stream_items<W: Write>(
    render_items: Vec<serde_json::Value>,
    draw_runtime: &mut StreamJsonTerminalDrawRuntime,
    writer: &mut W,
    start: std::time::Instant,
    pending_flush_due_ms: &mut Option<u64>,
) -> Result<()> {
    let now_ms = elapsed_millis_since(start);
    for item in render_items {
        let is_draw_plan = item.get("type").and_then(serde_json::Value::as_str)
            == Some(STREAM_JSON_RENDER_DRAW_PLAN_TYPE);
        if is_draw_plan {
            let report = draw_runtime.submit_draw_plan_value_at(item, now_ms, writer)?;
            terminal_render_append_frontend_event_log_line(format!(
                "submit_draw_plan applied={} queued={} skipped={} skip_reason={:?} flushed={} next_flush_due_ms={:?}",
                report.applied,
                report.queued,
                report.skipped,
                report.skip_reason,
                report
                    .execution
                    .as_ref()
                    .map(|execution| execution.flushed)
                    .unwrap_or(false),
                report.next_flush_due_ms
            ));
            *pending_flush_due_ms = terminal_render_next_flush_due(draw_runtime, &report);
        }
    }
    Ok(())
}

fn terminal_render_next_flush_due(
    draw_runtime: &StreamJsonTerminalDrawRuntime,
    report: &crate::stream_json_terminal_renderer::StreamJsonTerminalDrawRuntimeReport,
) -> Option<u64> {
    if draw_runtime.has_pending_draw() {
        report.next_flush_due_ms
    } else {
        None
    }
}

fn elapsed_millis_since(start: std::time::Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn terminal_render_export_final_diagnostics_if_requested(
    draw_runtime: &StreamJsonTerminalDrawRuntime,
) {
    let Some(path) = terminal_render_diagnostics_path_from_env() else {
        return;
    };
    let diagnostics = draw_runtime.runtime_diagnostics_value();
    if let Err(error) = terminal_render_write_diagnostics_snapshot_to_path(&path, &diagnostics) {
        warn!(
            path = %path,
            error = %error,
            "failed to write terminal-render diagnostics snapshot"
        );
    }
}

fn terminal_render_diagnostics_path_from_env() -> Option<String> {
    std::env::var(TERMINAL_RENDER_DIAGNOSTICS_PATH_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

fn terminal_render_write_diagnostics_snapshot_to_path(
    path: &str,
    diagnostics: &serde_json::Value,
) -> Result<()> {
    let path = Path::new(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut payload = serde_json::to_vec_pretty(diagnostics)?;
    payload.push(b'\n');
    std::fs::write(path, payload)?;
    Ok(())
}

async fn build_oneshot_prompt_params(
    state: SharedBootstrapState,
    mut prompt: String,
    config: &ReplConfig,
) -> Result<(PromptParams, String)> {
    // 标记为非交互式，并捕获 model/cwd
    let (mut model, cwd, main_agent_type) = {
        let mut s = state
            .write()
            .map_err(|e| anyhow::anyhow!("failed to write state: {}", e))?;
        s.is_interactive = false;
        if let Some(ref model) = config.model_override {
            s.model_override = Some(model.clone());
        }
        let m = s
            .model_override
            .clone()
            .unwrap_or_else(default_model_for_unset_cli);
        (
            m,
            s.cwd.to_string_lossy().to_string(),
            s.main_agent_type.clone(),
        )
    };
    let subagent_definition = resolve_builtin_subagent_definition(main_agent_type.as_deref());
    let is_agent_subprocess = std::env::var("MOSSEN_AGENT_SUBPROCESS_DEPTH").is_ok();

    let restore_history = load_restore_history(config, &cwd, &model, &state).await?;
    if let Ok(state) = state.read() {
        if let Some(restored_model) = state.model_override.clone() {
            model = restored_model;
        }
    }
    let session_start_source = if restore_history.is_empty() || !config.restore_mode {
        mossen_utils::session_start::SessionStartSource::Startup
    } else {
        mossen_utils::session_start::SessionStartSource::Resume
    };
    let session_hook_messages = crate::session_hooks::run_session_start_hooks(
        &state,
        session_start_source,
        Some(&model),
        true,
    )
    .await;
    if !session_hook_messages.is_empty() {
        info!(
            target: "mossen_agent::hooks",
            count = session_hook_messages.len(),
            "SessionStart hook messages produced for oneshot startup"
        );
    }
    let hook_initial_prompt = mossen_utils::session_start::take_initial_user_message();
    let mut startup_additional_blocks: Vec<ContentBlock> = session_hook_messages
        .iter()
        .filter(|message| message.message_type == "hook_additional_context")
        .map(|message| {
            ContentBlock::Text(TextBlock {
                text: message.content.clone(),
            })
        })
        .collect();
    if let Some(initial_prompt) = hook_initial_prompt {
        if prompt.trim().is_empty() {
            prompt = initial_prompt;
        } else {
            startup_additional_blocks.push(ContentBlock::Text(TextBlock {
                text: format!("Startup hook initial user message:\n{}", initial_prompt),
            }));
        }
    }
    let hook_context = match crate::session_hooks::build_hooks_context(&state, true) {
        Ok(context) => Some(Arc::new(context)),
        Err(err) => {
            warn!(
                target: "mossen_agent::hooks",
                error = %err,
                "Hook context unavailable; oneshot dialogue hooks will continue disabled"
            );
            None
        }
    };

    mossen_skills::init_bundled_skills();

    // Build the executable tool registry the same way `launch_repl` does so
    // --oneshot / --print mode has the same tool surface as the TUI. Without
    // this the model is forced to fall back to writing bash inside markdown
    // code blocks (or worse, hallucinating XML-style `<tool_call>` tags).
    let mut oneshot_registry = mossen_agent::tool_registry::ToolRegistry::new();
    let builtin_tool_options = mossen_tools::ToolRuntimeOptions {
        mcp_resources: config.mcp_enabled,
    };
    oneshot_registry.register_all(filtered_builtin_tools(builtin_tool_options, config));
    let oneshot_registry = Arc::new(oneshot_registry);
    let mut oneshot_tools = oneshot_registry.definitions();
    if config.mcp_enabled {
        let cwd_path = std::path::PathBuf::from(&cwd);
        if let Some((mcp_manager, mcp_tool_definitions, _server_count)) =
            initialize_mcp_manager_for_session(config, &cwd_path, "oneshot").await
        {
            oneshot_tools.extend(mcp_tool_definitions);
            crate::repl_mcp::set_manager(mcp_manager.clone());
            mossen_mcp::server::set_global_manager(mcp_manager);
        }
    }
    if let Some(agent) = subagent_definition.as_ref() {
        oneshot_tools =
            filter_tool_definitions_for_subagent(oneshot_tools, agent, is_agent_subprocess);
    }
    let oneshot_tool_names: Vec<String> = oneshot_tools.iter().map(|t| t.name.clone()).collect();

    // Compose the same layered system prompt the REPL uses; without it,
    // oneshot calls go to the model with zero identity / env context and
    // the assistant treats them like raw chat completions.
    let mut system_prompt_blocks = if let Some(text) = config.system_prompt.clone() {
        vec![mossen_agent::types::SystemBlock {
            text,
            cache_control: None,
        }]
    } else {
        let is_custom = mossen_utils::custom_backend::is_custom_backend_enabled();
        let cwd_path = std::path::PathBuf::from(&cwd);
        let skill_load_report =
            mossen_skills::load_startup_skill_directories(&cwd_path, ".mossen").await;
        if skill_load_report.user_dir_present
            || skill_load_report.project_dir_count > 0
            || skill_load_report.added_skill_count > 0
        {
            info!(
                target: "mossen_cli::skills",
                user_dir_present = skill_load_report.user_dir_present,
                project_dirs = skill_load_report.project_dir_count,
                added = skill_load_report.added_skill_count,
                "skill directories loaded during oneshot startup"
            );
        }
        let activated = mossen_skills::activate_conditional_skills_for_paths(
            std::slice::from_ref(&cwd),
            &cwd_path,
        );
        if !activated.is_empty() {
            info!(
                target: "mossen_cli::skills",
                activated = ?activated,
                "conditional skills activated during oneshot startup"
            );
        }
        let skill_commands = mossen_tools::skill_tool::prompt::get_loaded_skill_tool_commands();
        let skill_commands_text =
            mossen_tools::skill_tool::prompt::format_commands_within_budget(&skill_commands, None);
        let memory_text =
            crate::system_prompt::gather_memory_text_with_hooks(&cwd_path, hook_context.as_deref())
                .await;
        let inputs = crate::system_prompt::SystemPromptInputs {
            cwd: &cwd,
            model: &model,
            model_marketing_name: None,
            is_non_interactive: true,
            is_fork_subagent: std::env::var("MOSSEN_AGENT_SUBPROCESS_TYPE")
                .ok()
                .is_some_and(|value| value.eq_ignore_ascii_case("fork")),
            is_custom_backend: is_custom,
            is_internal: std::env::var("USER_TYPE").ok().as_deref() == Some("internal"),
            is_git_repo: crate::system_prompt::detect_git_repo(&cwd_path),
            product_name: "Mossen",
            enabled_tools: &oneshot_tool_names,
            skill_commands_count: skill_commands.len(),
            skill_commands_text: &skill_commands_text,
            are_explore_plan_agents:
                mossen_tools::agent_tool::built_in_agents::are_explore_plan_agents_enabled(),
            explore_agent_type: "explore",
            language_preference: None,
            memory_text: &memory_text,
        };
        crate::system_prompt::assemble(&inputs)
    };
    if let Some(agent) = subagent_definition.as_ref() {
        let insert_at = usize::from(!system_prompt_blocks.is_empty());
        system_prompt_blocks.insert(insert_at, subagent_system_prompt_block(agent));
    }

    // 构造 PromptParams 并通过 mossen-agent submit_prompt 执行真实查询
    let prompt_params = PromptParams {
        prompt: prompt.clone(),
        history_messages: restore_history,
        additional_blocks: startup_additional_blocks,
        model,
        system_prompt: system_prompt_blocks,
        tools: oneshot_tools,
        tool_use_context: ToolUseContext {
            cwd,
            additional_working_directories: None,
            extra: Default::default(),
        },
        origin_tag: OriginTag::Sdk,
        // Allow the full tool/response round-trip while keeping unattended
        // oneshot runs bounded. CLI --turn-limit overrides the default cap.
        max_turns: Some(config.max_turns.unwrap_or(12)),
        cancel_token: None,
        api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
        api_key: std::env::var("MOSSEN_API_KEY").ok(),
        extra_body: Default::default(),
        fast_mode: None,
        effort: None,
        permission_mode: convert_subagent_permission_mode(
            subagent_definition
                .as_ref()
                .and_then(|agent| agent.permission_mode.as_ref()),
        )
        .unwrap_or_else(session_permission_mode_from_env),
        // Oneshot mode is unattended — there's nothing to prompt the user
        // with — so leave the gate open.
        permission_gate: None,
        tool_registry: Some(oneshot_registry),
        hook_context,
    };

    Ok((prompt_params, prompt))
}

async fn load_restore_history(
    config: &ReplConfig,
    cwd: &str,
    model: &str,
    state: &SharedBootstrapState,
) -> Result<Vec<Message>> {
    if !config.restore_mode && config.restore_session_id.is_none() {
        return Ok(Vec::new());
    }

    use mossen_agent::transcript::{default_transcript_dir, list_transcripts, TranscriptManager};

    let transcript = if let Some(session_id) = config.restore_session_id.as_deref() {
        let manager = TranscriptManager::new(session_id.to_string(), default_transcript_dir());
        match manager.load().await? {
            Some(transcript) => Some(transcript),
            None => {
                warn!(
                    session_id = %session_id,
                    "restore-id requested but no transcript file was found"
                );
                None
            }
        }
    } else {
        list_transcripts(&default_transcript_dir())
            .await?
            .into_iter()
            .find(|transcript| transcript.cwd.as_deref() == Some(cwd))
    };

    let Some(transcript) = transcript else {
        return Ok(Vec::new());
    };

    if let Ok(mut state) = state.write() {
        state.switch_session(transcript.session_id.clone());
        if state.model_override.is_none() {
            if let Some(model) = transcript.model.clone() {
                state.model_override = Some(model);
            }
        }
    }
    info!(
        session_id = %transcript.session_id,
        message_count = transcript.messages.len(),
        model = transcript.model.as_deref().unwrap_or(model),
        "loaded restore transcript for oneshot"
    );
    Ok(transcript.messages)
}

async fn record_oneshot_transcript(
    state: &SharedBootstrapState,
    history_messages: &[Message],
    prompt: &str,
    result_text: &str,
    model: &str,
) -> Result<()> {
    use mossen_agent::transcript::{default_transcript_dir, TranscriptManager};

    let (session_id, persistence_disabled, cwd) = {
        let state = state
            .read()
            .map_err(|e| anyhow::anyhow!("failed to read state: {}", e))?;
        (
            state.session_id.clone(),
            state.session_persistence_disabled,
            state.cwd.to_string_lossy().to_string(),
        )
    };
    if persistence_disabled {
        return Ok(());
    }

    let mut messages = history_messages.to_vec();
    messages.push(oneshot_text_message(Role::User, prompt));
    messages.push(oneshot_text_message(Role::Assistant, result_text));

    let mut manager = TranscriptManager::new(session_id, default_transcript_dir());
    manager.record(&messages, Some(model), Some(&cwd)).await
}

fn oneshot_text_message(role: Role, text: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text(TextBlock {
            text: text.to_string(),
        })],
        uuid: None,
        is_meta: None,
        origin: None,
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
        extra: HashMap::new(),
    }
}

/// TUI 事件循环。
///
/// 使用 crossterm 进入 raw mode + alternate screen，
/// 构建 ratatui Terminal，并调用 App::run 进入消息循环。
/// 也持续观察 shutdown_flag 以支持外部中断。
async fn run_event_loop(
    mut app: mossen_tui::App,
    state: SharedBootstrapState,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    info!("event_loop: starting");

    // 0. Shut down the early-input reader BEFORE we enter raw mode. The early
    //    capture thread spawned at pre-main also polls crossterm key events;
    //    leaving it alive would mean two threads racing for the same stdin
    //    keystrokes, with each input event delivered to whichever reader
    //    happened to poll first. CJK input then loses every other character.
    //    Consume whatever was typed during boot and seed it into the prompt so
    //    nothing the user pressed is dropped.
    let early = mossen_utils::early_input::consume_early_input();
    if !early.is_empty() {
        app.prompt.input.insert_str(&early);
    }

    // 1. 启用 crossterm raw mode + alternate screen. Mouse capture stays
    // opt-in so normal terminal text selection/copy keeps working.
    enable_raw_mode().map_err(|e| anyhow::anyhow!("failed to enable raw mode: {}", e))?;
    let mut stdout = io::stdout();
    let mouse_capture_enabled = terminal_render_should_capture_mouse();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| anyhow::anyhow!("failed to enter terminal UI mode: {}", e))?;
    if mouse_capture_enabled {
        execute!(stdout, EnableMouseCapture)
            .map_err(|e| anyhow::anyhow!("failed to enable mouse capture: {}", e))?;
    }

    // 2. 构建 ratatui Terminal
    let backend = CrosstermBackend::new(stdout);
    let terminal =
        Terminal::new(backend).map_err(|e| anyhow::anyhow!("failed to create terminal: {}", e))?;

    // 3. 启动 shutdown 监视器：将外部 shutdown_flag 转换为 app.should_quit
    let shutdown_watch = {
        let shutdown_flag = shutdown_flag.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if shutdown_flag.load(Ordering::SeqCst) {
                    break;
                }
            }
        })
    };

    // 4. 在 select 中并行运行 App 主循环与 shutdown 监视器
    let app_result = tokio::select! {
        r = app.run(terminal) => r,
        _ = shutdown_watch => {
            info!("event_loop: shutdown_flag triggered, exiting");
            Ok(())
        }
    };

    // 5. 恢复终端状态（即使主循环报错也要执行）
    let _ = disable_raw_mode();
    let mut cleanup_stdout = io::stdout();
    if mouse_capture_enabled {
        let _ = execute!(cleanup_stdout, DisableMouseCapture);
    }
    let _ = execute!(cleanup_stdout, LeaveAlternateScreen, cursor::Show);
    let _ = cleanup_stdout.flush();

    // 6. 同步会话统计回 BootstrapState
    if let Ok(mut s) = state.write() {
        s.touch_interaction();
    }

    info!("event_loop: ended");
    app_result
}

/// 单次提交便捷入口（绕过 TUI），主要供测试 / 子命令复用。
///
/// 这是 `run_oneshot` 的更直接形式：直接构造 SessionOrchestrator，
/// 提交一条 prompt，收集所有 assistant 文本内容并返回。
pub async fn submit_once(
    prompt: &str,
    model: Option<String>,
    cwd: Option<String>,
) -> Result<String> {
    let model = model.unwrap_or_else(default_model_for_unset_cli);
    let cwd = cwd.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| ".".to_string())
    });

    let config = OrchestratorConfig {
        system_prompt: Vec::new(),
        tools: Vec::new(),
        tool_use_context: ToolUseContext {
            cwd,
            additional_working_directories: None,
            extra: Default::default(),
        },
        model,
        user_specified_model: None,
        max_output_tokens: None,
        origin_tag: OriginTag::Sdk,
        fast_mode: None,
        effort: None,
        api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
        api_key: std::env::var("MOSSEN_API_KEY").ok(),
        skip_stop_hooks: true,
        auto_mode: false,
        extra_body: Default::default(),
        permission_mode: session_permission_mode_from_env(),
        // `submit_once` is the non-interactive test/SDK shortcut — no UI to
        // drive a permission modal — so we leave the gate at the
        // `AllowAllGate` default.
        permission_gate: None,
        tool_registry: None,
        hook_context: None,
    };

    let mut orchestrator = SessionOrchestrator::new(config);
    let mut rx = orchestrator
        .dispatch_turn(
            prompt,
            Some(SubmitOptions {
                max_turns: Some(1),
                ..Default::default()
            }),
        )
        .await;

    let mut result = String::new();
    while let Some(msg) = rx.recv().await {
        match msg {
            SdkMessage::Assistant { message, .. } => {
                for block in &message.content {
                    if let mossen_types::ContentBlock::Text(t) = block {
                        result.push_str(&t.text);
                    }
                }
            }
            SdkMessage::Result { .. } => break,
            _ => {}
        }
    }

    if result.is_empty() {
        // 永远不返回空 — 至少给出结构化兜底（带 prompt 元信息），便于上层判断。
        result = format!("[no-content] prompt_chars={}", prompt.len());
    }
    Ok(result)
}

#[cfg(test)]
mod terminal_render_frontend_event_tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, MouseEvent};

    fn terminal_render_mouse_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn restore_terminal_render_mouse_env(previous: Option<String>) {
        if let Some(previous) = previous {
            std::env::set_var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV, previous);
        } else {
            std::env::remove_var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV);
        }
    }

    const RESTORE_ENV_KEYS: &[&str] = &[
        "HOME",
        "MOSSEN_CONFIG_HOME",
        "MOSSEN_CONFIG_DIR",
        "XDG_CONFIG_HOME",
        "MOSSEN_CODE_DISABLE_AUTO_MEMORY",
        "MOSSEN_CODE_DISABLE_TEAM_MEMORY",
        "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE",
        "MOSSEN_AGENT_SUBPROCESS_DEPTH",
        "MOSSEN_FEATURE_BUILTIN_EXPLORE_PLAN_AGENTS",
    ];

    struct RestoreEnvGuard(Vec<(&'static str, Option<String>)>);

    impl Drop for RestoreEnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn restore_history_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn isolate_restore_history_env(root: &std::path::Path) -> RestoreEnvGuard {
        let guard = RestoreEnvGuard(
            RESTORE_ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect(),
        );
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("MOSSEN_CONFIG_HOME", root.join("home").join(".mossen"));
        std::env::set_var("MOSSEN_CONFIG_DIR", root.join("home").join(".mossen"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("MOSSEN_CODE_DISABLE_AUTO_MEMORY", "1");
        std::env::set_var("MOSSEN_CODE_DISABLE_TEAM_MEMORY", "1");
        guard
    }

    fn test_repl_config(restore_mode: bool, restore_session_id: Option<String>) -> ReplConfig {
        ReplConfig {
            initial_prompt: None,
            restore_mode,
            restore_session_id,
            mcp_enabled: false,
            mcp_config: None,
            system_prompt: Some("test system prompt".to_string()),
            extra_prompt: None,
            model_override: Some("mossen-balanced-4".to_string()),
            max_turns: None,
            allowed_instruments: Vec::new(),
            disabled_instruments: Vec::new(),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn cli_instrument_filters_apply_to_builtin_tool_registry() {
        let mut config = test_repl_config(false, None);
        config.disabled_instruments = vec!["bash".to_string()];
        let names = filtered_builtin_tools(mossen_tools::ToolRuntimeOptions::default(), &config)
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        assert!(!names.iter().any(|name| name == "Bash"));
        assert!(names.iter().any(|name| name == "Glob"));

        config.allowed_instruments = vec!["glob".to_string(), "read".to_string()];
        config.disabled_instruments = Vec::new();
        let mut names =
            filtered_builtin_tools(mossen_tools::ToolRuntimeOptions::default(), &config)
                .into_iter()
                .map(|tool| tool.name().to_string())
                .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["Glob", "Read"]);
    }

    #[test]
    fn normalizes_legacy_and_prompted_subagent_type_names() {
        assert_eq!(normalize_subagent_type("Explore"), "explore");
        assert_eq!(normalize_subagent_type("general-purpose"), "general");
        assert_eq!(normalize_subagent_type("general purpose"), "general");
    }

    #[tokio::test]
    async fn oneshot_agent_type_reaches_child_prompt_and_tool_surface() {
        let _lock = restore_history_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        std::env::set_var("MOSSEN_FEATURE_BUILTIN_EXPLORE_PLAN_AGENTS", "1");
        std::env::set_var("MOSSEN_AGENT_SUBPROCESS_DEPTH", "1");
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let state = crate::bootstrap::new_shared_state(cwd);
        {
            let mut state = state.write().expect("state write");
            state.main_agent_type = Some("Explore".to_string());
        }
        let config = ReplConfig {
            system_prompt: None,
            ..test_repl_config(false, None)
        };

        let (params, _) =
            build_oneshot_prompt_params(state, "scan the codebase".to_string(), &config)
                .await
                .expect("prompt params");
        let system_prompt = params
            .system_prompt
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let tool_names: HashSet<String> =
            params.tools.iter().map(|tool| tool.name.clone()).collect();

        assert!(
            system_prompt.contains("`explore` subagent"),
            "{system_prompt}"
        );
        assert!(
            !system_prompt.contains("subagent_type=Explore"),
            "{system_prompt}"
        );
        for allowed in ["Bash", "Glob", "Grep", "Read"] {
            assert!(tool_names.contains(allowed), "{tool_names:?}");
        }
        for hidden in ["Agent", "Edit", "Write", "TodoWrite"] {
            assert!(!tool_names.contains(hidden), "{tool_names:?}");
        }
        assert_eq!(params.permission_mode, PermissionMode::DontAsk);
    }

    #[tokio::test]
    async fn oneshot_turn_limit_reaches_prompt_params() {
        let _lock = restore_history_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let state = crate::bootstrap::new_shared_state(cwd);
        let mut config = test_repl_config(false, None);
        config.max_turns = Some(32);

        let (params, _) = build_oneshot_prompt_params(state, "use more turns".to_string(), &config)
            .await
            .expect("prompt params");

        assert_eq!(params.max_turns, Some(32));
    }

    #[tokio::test]
    async fn oneshot_restore_id_loads_history_without_leaking_to_new_session() {
        let _lock = restore_history_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let cwd_string = cwd.to_string_lossy().to_string();
        let marker = "M4_5_RESTORE_HISTORY_MARKER";
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut manager = mossen_agent::transcript::TranscriptManager::new(
            session_id.clone(),
            mossen_agent::transcript::default_transcript_dir(),
        );
        manager
            .record(
                &[oneshot_text_message(Role::User, marker)],
                Some("mossen-balanced-4"),
                Some(&cwd_string),
            )
            .await
            .expect("record transcript");

        let restored_state = crate::bootstrap::new_shared_state(cwd.clone());
        let (restored_params, _) = build_oneshot_prompt_params(
            restored_state.clone(),
            "continue the session".to_string(),
            &test_repl_config(true, Some(session_id.clone())),
        )
        .await
        .expect("restore prompt params");

        assert_eq!(restored_params.history_messages.len(), 1);
        assert!(serde_json::to_string(&restored_params.history_messages)
            .expect("history json")
            .contains(marker));
        assert_eq!(restored_state.read().expect("state").session_id, session_id);

        let fresh_state = crate::bootstrap::new_shared_state(cwd);
        let (fresh_params, _) = build_oneshot_prompt_params(
            fresh_state.clone(),
            "new session".to_string(),
            &test_repl_config(false, None),
        )
        .await
        .expect("fresh prompt params");

        assert!(fresh_params.history_messages.is_empty());
        assert_ne!(
            fresh_state.read().expect("state").session_id,
            session_id,
            "new sessions must not adopt explicit restore-id history"
        );
    }

    #[tokio::test]
    async fn oneshot_transcript_record_appends_turn_to_existing_history() {
        let _lock = restore_history_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let cwd_string = cwd.to_string_lossy().to_string();
        let state = crate::bootstrap::new_shared_state(cwd.clone());
        let session_id = state.read().expect("state").session_id.clone();
        let history = vec![oneshot_text_message(Role::Assistant, "previous answer")];

        record_oneshot_transcript(
            &state,
            &history,
            "new prompt",
            "new answer",
            "mossen-balanced-4",
        )
        .await
        .expect("record oneshot transcript");

        let manager = mossen_agent::transcript::TranscriptManager::new(
            session_id,
            mossen_agent::transcript::default_transcript_dir(),
        );
        let transcript = manager
            .load()
            .await
            .expect("load transcript")
            .expect("transcript exists");

        assert_eq!(transcript.cwd.as_deref(), Some(cwd_string.as_str()));
        assert_eq!(transcript.message_count, 3);
        let transcript_json = serde_json::to_string(&transcript.messages).expect("messages json");
        assert!(transcript_json.contains("previous answer"));
        assert!(transcript_json.contains("new prompt"));
        assert!(transcript_json.contains("new answer"));
    }

    #[tokio::test]
    async fn restore_history_and_project_memory_stay_separate() {
        let _lock = restore_history_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let cwd_string = cwd.to_string_lossy().to_string();
        let project_marker = "MOSSEN_M5_5_PROJECT_MEMORY_MARKER";
        let conversation_marker = "MOSSEN_M5_5_RESUME_HISTORY_MARKER";
        std::fs::write(cwd.join("MOSSEN.md"), project_marker).expect("write project memory");

        let session_id = uuid::Uuid::new_v4().to_string();
        let mut manager = mossen_agent::transcript::TranscriptManager::new(
            session_id.clone(),
            mossen_agent::transcript::default_transcript_dir(),
        );
        manager
            .record(
                &[oneshot_text_message(Role::User, conversation_marker)],
                Some("mossen-balanced-4"),
                Some(&cwd_string),
            )
            .await
            .expect("record transcript");

        let restored_config = ReplConfig {
            system_prompt: None,
            ..test_repl_config(true, Some(session_id.clone()))
        };
        let restored_state = crate::bootstrap::new_shared_state(cwd.clone());
        let (restored_params, _) =
            build_oneshot_prompt_params(restored_state, "continue".to_string(), &restored_config)
                .await
                .expect("restore prompt params");
        let restored_history =
            serde_json::to_string(&restored_params.history_messages).expect("history json");
        let restored_system_prompt = restored_params
            .system_prompt
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(restored_history.contains(conversation_marker));
        assert!(restored_system_prompt.contains(project_marker));
        assert!(!restored_system_prompt.contains(conversation_marker));

        let fresh_config = ReplConfig {
            system_prompt: None,
            ..test_repl_config(false, None)
        };
        let fresh_state = crate::bootstrap::new_shared_state(cwd);
        let (fresh_params, _) =
            build_oneshot_prompt_params(fresh_state, "new session".to_string(), &fresh_config)
                .await
                .expect("fresh prompt params");
        let fresh_system_prompt = fresh_params
            .system_prompt
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(fresh_params.history_messages.is_empty());
        assert!(fresh_system_prompt.contains(project_marker));
        assert!(!fresh_system_prompt.contains(conversation_marker));
    }

    #[tokio::test]
    async fn oneshot_system_prompt_includes_user_config_skill() {
        let _lock = restore_history_env_lock();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_restore_history_env(temp.path());
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).expect("project dir");
        let skill_dir = temp
            .path()
            .join("home")
            .join(".mossen")
            .join("skills")
            .join("m6_user_config_skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: M6 user config skill\n---\nUse this user-level skill.\n",
        )
        .expect("write skill");

        let config = ReplConfig {
            system_prompt: None,
            ..test_repl_config(false, None)
        };
        let state = crate::bootstrap::new_shared_state(cwd);
        let (params, _) = build_oneshot_prompt_params(state, "use a skill".to_string(), &config)
            .await
            .expect("oneshot params");
        let system_prompt = params
            .system_prompt
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(
            system_prompt.contains("# User-invocable skills"),
            "{system_prompt}"
        );
        assert!(
            system_prompt.contains("m6_user_config_skill"),
            "{system_prompt}"
        );
        assert!(
            system_prompt.contains("M6 user config skill"),
            "{system_prompt}"
        );
        mossen_skills::clear_dynamic_skills();
    }

    #[test]
    fn terminal_render_writes_final_diagnostics_snapshot_to_requested_path() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mossen-terminal-render-diagnostics-{}-{unique}.json",
            std::process::id()
        ));
        let diagnostics = serde_json::json!({
            "hasPendingDraw": false,
            "manualScrollActive": false,
            "reportCount": 7,
            "lastReport": {
                "applied": true,
                "execution": {
                    "flushed": true
                }
            }
        });

        terminal_render_write_diagnostics_snapshot_to_path(
            path.to_str().expect("utf8 temp path"),
            &diagnostics,
        )
        .expect("write terminal render diagnostics");

        let written = std::fs::read_to_string(&path).expect("read diagnostics");
        let parsed: serde_json::Value = serde_json::from_str(&written).expect("parse diagnostics");
        assert_eq!(parsed["hasPendingDraw"], false);
        assert_eq!(parsed["reportCount"], 7);
        assert_eq!(parsed["lastReport"]["execution"]["flushed"], true);
        assert!(written.ends_with('\n'));

        let _ = std::fs::remove_file(path);
    }

    fn permission_request_for_test(
        tool_id: &str,
    ) -> (
        PermissionRequest,
        tokio::sync::oneshot::Receiver<PermissionDecision>,
    ) {
        let (responder, response) = tokio::sync::oneshot::channel();
        (
            PermissionRequest {
                tool_id: tool_id.to_string(),
                tool_name: "Bash".to_string(),
                input: serde_json::json!({ "command": "echo ok" }),
                responder,
            },
            response,
        )
    }

    #[test]
    fn terminal_render_mouse_capture_defaults_off_for_native_scroll() {
        let _guard = terminal_render_mouse_env_lock();
        let previous = std::env::var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV).ok();
        std::env::remove_var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV);

        assert!(!terminal_render_should_capture_mouse());

        restore_terminal_render_mouse_env(previous);
    }

    #[test]
    fn terminal_render_mouse_capture_can_be_enabled_by_env() {
        let _guard = terminal_render_mouse_env_lock();
        let previous = std::env::var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV).ok();
        std::env::set_var(TERMINAL_RENDER_CAPTURE_MOUSE_ENV, "1");

        assert!(terminal_render_should_capture_mouse());

        restore_terminal_render_mouse_env(previous);
    }

    #[test]
    fn maps_scroll_keys_to_manual_scroll_state() {
        let page_up = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        let home = Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        let up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        let page_down = Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        let end = Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&page_up),
            Some(TerminalRenderFrontendEvent::ManualScrollStart)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&home),
            Some(TerminalRenderFrontendEvent::ManualScrollStart)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&up),
            Some(TerminalRenderFrontendEvent::ManualScrollStart)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&page_down),
            Some(TerminalRenderFrontendEvent::ManualScrollEnd)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&end),
            Some(TerminalRenderFrontendEvent::ManualScrollEnd)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&down),
            Some(TerminalRenderFrontendEvent::ManualScrollEnd)
        );
    }

    #[test]
    fn maps_resize_mouse_wheel_and_clear_key_to_frontend_events() {
        let resize = Event::Resize(80, 24);
        let scroll_up = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let scroll_down = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let ctrl_l = Event::Key(KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&resize),
            Some(TerminalRenderFrontendEvent::Resize)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&scroll_up),
            Some(TerminalRenderFrontendEvent::ManualScrollStart)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&scroll_down),
            Some(TerminalRenderFrontendEvent::ManualScrollEnd)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&ctrl_l),
            Some(TerminalRenderFrontendEvent::ManualScrollEnd)
        );
    }

    #[test]
    fn coalesces_resize_frontend_events_until_resize_is_handled() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (priority_tx, mut priority_rx) = tokio::sync::mpsc::unbounded_channel();
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE);

        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Resize,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            !terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Resize,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollEnd,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );

        assert_eq!(rx.try_recv().unwrap(), TerminalRenderFrontendEvent::Resize);
        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::ManualScrollEnd
        );
        assert!(rx.try_recv().is_err());
        assert!(priority_rx.try_recv().is_err());

        terminal_render_release_resize_frontend_event(
            &TerminalRenderFrontendEvent::Resize,
            &resize_event_pending,
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Resize,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert_eq!(rx.try_recv().unwrap(), TerminalRenderFrontendEvent::Resize);
    }

    #[test]
    fn coalesces_repeated_manual_scroll_frontend_events_until_state_is_handled() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (priority_tx, mut priority_rx) = tokio::sync::mpsc::unbounded_channel();
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE);

        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollStart,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            !terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollStart,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::ManualScrollStart
        );

        terminal_render_release_scroll_frontend_event(
            &TerminalRenderFrontendEvent::ManualScrollStart,
            &scroll_event_pending,
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollStart,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::ManualScrollStart
        );

        terminal_render_release_scroll_frontend_event(
            &TerminalRenderFrontendEvent::ManualScrollStart,
            &scroll_event_pending,
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollEnd,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            !terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollEnd,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::ManualScrollEnd
        );
        assert!(priority_rx.try_recv().is_err());
    }

    #[test]
    fn coalesces_opposite_manual_scroll_frontend_events_to_latest_state() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (priority_tx, mut priority_rx) = tokio::sync::mpsc::unbounded_channel();
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE);

        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollStart,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            !terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollEnd,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert_eq!(
            scroll_event_pending.load(Ordering::SeqCst),
            TERMINAL_RENDER_SCROLL_EVENT_END
        );

        let queued_event = priority_rx.try_recv().unwrap();
        assert_eq!(queued_event, TerminalRenderFrontendEvent::ManualScrollStart);
        assert_eq!(
            terminal_render_take_scroll_frontend_event_state(&queued_event, &scroll_event_pending),
            Some(TERMINAL_RENDER_SCROLL_EVENT_END)
        );
        assert_eq!(
            scroll_event_pending.load(Ordering::SeqCst),
            TERMINAL_RENDER_SCROLL_EVENT_NONE
        );
        assert!(priority_rx.try_recv().is_err());
    }

    #[test]
    fn routes_priority_frontend_events_ahead_of_low_priority_backlog() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (priority_tx, mut priority_rx) = tokio::sync::mpsc::unbounded_channel();
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE);

        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Resize,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::ManualScrollStart,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Interrupt,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );

        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::ManualScrollStart
        );
        assert_eq!(
            terminal_render_take_scroll_frontend_event_state(
                &TerminalRenderFrontendEvent::ManualScrollStart,
                &scroll_event_pending
            ),
            Some(TERMINAL_RENDER_SCROLL_EVENT_START)
        );
        assert_eq!(
            priority_rx.try_recv().unwrap(),
            TerminalRenderFrontendEvent::Interrupt
        );
        let drain_report = terminal_render_drain_superseded_low_priority_frontend_events(
            &mut rx,
            &resize_event_pending,
            &scroll_event_pending,
        );
        assert_eq!(drain_report.drained_count, 1);
        assert!(drain_report.drained_resize_event);
        assert!(!drain_report.drained_scroll_event);
        assert_eq!(
            drain_report.last_drained_scroll_state,
            TERMINAL_RENDER_SCROLL_EVENT_NONE
        );
        assert!(!resize_event_pending.load(Ordering::SeqCst));
        assert_eq!(
            scroll_event_pending.load(Ordering::SeqCst),
            TERMINAL_RENDER_SCROLL_EVENT_NONE
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn priority_drain_reports_resize_for_follow_up_redraw() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (priority_tx, _priority_rx) = tokio::sync::mpsc::unbounded_channel();
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_NONE);

        assert!(
            terminal_render_try_enqueue_frontend_event_with_resize_coalescing(
                &tx,
                &priority_tx,
                TerminalRenderFrontendEvent::Resize,
                &resize_event_pending,
                &scroll_event_pending,
            )
        );
        let drain_report = terminal_render_drain_superseded_low_priority_frontend_events(
            &mut rx,
            &resize_event_pending,
            &scroll_event_pending,
        );

        assert_eq!(
            drain_report,
            TerminalRenderLowPriorityDrainReport {
                drained_count: 1,
                drained_resize_event: true,
                drained_scroll_event: false,
                last_drained_scroll_state: TERMINAL_RENDER_SCROLL_EVENT_NONE,
            }
        );
        assert!(!resize_event_pending.load(Ordering::SeqCst));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn priority_manual_scroll_end_releases_hold_and_flushes_pending_draw() {
        let resize_event_pending = AtomicBool::new(false);
        let scroll_event_pending = AtomicU8::new(TERMINAL_RENDER_SCROLL_EVENT_END);

        let mut draw_runtime =
            StreamJsonTerminalDrawRuntime::new(StreamJsonTerminalViewport::new(24, 80));
        let mut writer = Vec::new();
        draw_runtime.set_manual_scroll_active(true);
        let pending_plan = serde_json::json!({
            "sequence": 42,
            "schedule": {
                "shouldFlush": true,
                "flushPolicy": "immediate",
                "coalesceSafe": false
            },
            "draw": {
                "skipped": false
            },
            "scroll": {
                "preserveOnActiveUpdate": true,
                "preserveDuringManualScroll": true,
                "historyPolicy": "update_active"
            },
            "terminalOps": [
                { "op": "move_to_row", "row": "top+0" },
                { "op": "write_line", "text": "pending after scroll", "semanticStyle": "plain" }
            ]
        });
        let queued = draw_runtime
            .submit_draw_plan_value_at(pending_plan, 0, &mut writer)
            .expect("queue manual-scroll-held draw plan");
        assert!(queued.queued);
        assert!(draw_runtime.has_pending_draw());
        assert!(writer.is_empty());

        let mut render_event_emitter = StreamJsonRenderEventEmitter::new();
        let mut approval_bridge = TerminalRenderApprovalBridge::default();
        let edit_capture_active = AtomicBool::new(false);
        let cancel_token = CancellationToken::new();
        let mut pending_flush_due_ms = None;
        let saw_result = terminal_render_handle_frontend_event(
            TerminalRenderFrontendEvent::ManualScrollEnd,
            &mut render_event_emitter,
            &mut draw_runtime,
            &mut approval_bridge,
            &mut writer,
            std::time::Instant::now(),
            &mut pending_flush_due_ms,
            &edit_capture_active,
            &resize_event_pending,
            &scroll_event_pending,
            &cancel_token,
        )
        .expect("release manual-scroll end");

        assert!(!saw_result);
        assert!(!draw_runtime.has_pending_draw());
        assert_eq!(pending_flush_due_ms, None);
        assert_eq!(
            scroll_event_pending.load(Ordering::SeqCst),
            TERMINAL_RENDER_SCROLL_EVENT_NONE
        );
        assert!(!writer.is_empty());
    }

    #[test]
    fn priority_frontend_event_fairness_yields_after_burst_limit() {
        let mut priority_events_since_yield = 0usize;

        for _ in 0..TERMINAL_RENDER_PRIORITY_FAIRNESS_BURST_LIMIT {
            assert!(terminal_render_priority_fairness_allows(
                priority_events_since_yield
            ));
            terminal_render_note_priority_frontend_event(&mut priority_events_since_yield);
        }

        assert!(terminal_render_priority_fairness_yield_due(
            priority_events_since_yield
        ));
        assert!(!terminal_render_priority_fairness_allows(
            priority_events_since_yield
        ));

        terminal_render_reset_priority_fairness_budget(&mut priority_events_since_yield);
        assert!(terminal_render_priority_fairness_allows(
            priority_events_since_yield
        ));
        assert_eq!(priority_events_since_yield, 0);
    }

    #[test]
    fn maps_widget_toggle_keys_to_frontend_events() {
        let command_toggle = Event::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        let background_task_toggle =
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        let file_change_toggle = Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        let diff_toggle = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        let error_toggle = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&command_toggle),
            Some(TerminalRenderFrontendEvent::ToggleCommandExpansion)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&background_task_toggle),
            Some(TerminalRenderFrontendEvent::ToggleBackgroundTaskExpansion)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&file_change_toggle),
            Some(TerminalRenderFrontendEvent::ToggleFileChangeExpansion)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&diff_toggle),
            Some(TerminalRenderFrontendEvent::ToggleDiffExpansion)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&error_toggle),
            Some(TerminalRenderFrontendEvent::ToggleErrorExpansion)
        );
    }

    #[test]
    fn maps_approval_focus_keys_to_frontend_events() {
        let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let right = Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        let back_tab = Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        let left = Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&tab),
            Some(TerminalRenderFrontendEvent::FocusNextApprovalAction)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&right),
            Some(TerminalRenderFrontendEvent::FocusNextApprovalAction)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&back_tab),
            Some(TerminalRenderFrontendEvent::FocusPreviousApprovalAction)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&left),
            Some(TerminalRenderFrontendEvent::FocusPreviousApprovalAction)
        );
    }

    #[test]
    fn maps_approval_activation_keys_to_frontend_events() {
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let approve = Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        let reject = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let edit = Event::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        let session = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&enter),
            Some(TerminalRenderFrontendEvent::ActivateFocusedApprovalAction)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&approve),
            Some(TerminalRenderFrontendEvent::ActivateApprovalActionByKey(
                'y'
            ))
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&reject),
            Some(TerminalRenderFrontendEvent::ActivateApprovalActionByKey(
                'n'
            ))
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&edit),
            Some(TerminalRenderFrontendEvent::ActivateApprovalActionByKey(
                'e'
            ))
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&session),
            Some(TerminalRenderFrontendEvent::ActivateApprovalActionByKey(
                'a'
            ))
        );
    }

    #[test]
    fn maps_edit_command_capture_keys_to_input_events() {
        let command_char = Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        let backspace = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let escape = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&command_char, true),
            Some(TerminalRenderFrontendEvent::EditCommandInputChar('y'))
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&backspace, true),
            Some(TerminalRenderFrontendEvent::EditCommandBackspace)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&enter, true),
            Some(TerminalRenderFrontendEvent::EditCommandSubmit)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&escape, true),
            Some(TerminalRenderFrontendEvent::EditCommandCancel)
        );
    }

    #[test]
    fn maps_bracketed_paste_to_edit_command_paste_only_during_edit_capture() {
        let paste = Event::Paste("echo pasted".to_string());

        assert_eq!(terminal_render_frontend_event_from_crossterm(&paste), None);
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&paste, true),
            Some(TerminalRenderFrontendEvent::EditCommandPaste(
                "echo pasted".to_string()
            ))
        );
    }

    #[test]
    fn maps_ctrl_c_to_interrupt_even_during_edit_capture() {
        let ctrl_c = Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&ctrl_c),
            Some(TerminalRenderFrontendEvent::Interrupt)
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(&ctrl_c, true),
            Some(TerminalRenderFrontendEvent::Interrupt)
        );
    }

    #[test]
    fn ignores_key_release_events_to_prevent_duplicate_actions() {
        let release_approve = Event::Key(KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        let release_edit_char = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });
        let repeat_scroll = Event::Key(KeyEvent {
            code: KeyCode::PageUp,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        });

        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&release_approve),
            None
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm_with_edit_capture(
                &release_edit_char,
                true
            ),
            None
        );
        assert_eq!(
            terminal_render_frontend_event_from_crossterm(&repeat_scroll),
            Some(TerminalRenderFrontendEvent::ManualScrollStart)
        );
    }

    #[tokio::test]
    async fn terminal_approval_bridge_submits_allow_reject_and_session_decisions() {
        let mut bridge = TerminalRenderApprovalBridge::default();
        let (request, response) = permission_request_for_test("tool-allow");
        bridge.set_pending(request);
        assert!(bridge.has_pending_permission());
        let result = bridge.submit_action(TERMINAL_APPROVAL_ACTION_APPROVE_ONCE);
        assert_eq!(result.bridge_status, "submitted");
        assert!(result.submitted);
        assert!(!result.requires_decision_bridge);
        assert_eq!(
            response.await.expect("allow response"),
            PermissionDecision::Allow
        );
        assert!(!bridge.has_pending_permission());

        let (request, response) = permission_request_for_test("tool-reject");
        bridge.set_pending(request);
        let result = bridge.submit_action(TERMINAL_APPROVAL_ACTION_REJECT);
        assert_eq!(result.bridge_status, "submitted");
        assert_eq!(
            response.await.expect("reject response"),
            PermissionDecision::Deny
        );

        let (request, response) = permission_request_for_test("tool-session");
        bridge.set_pending(request);
        let result = bridge.submit_action(TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION);
        assert_eq!(result.bridge_status, "submitted");
        assert_eq!(
            response.await.expect("session response"),
            PermissionDecision::AllowAlways
        );
    }

    #[tokio::test]
    async fn terminal_approval_bridge_submits_edited_command_updated_input() {
        let mut bridge = TerminalRenderApprovalBridge::default();
        let (request, response) = permission_request_for_test("tool-edit");
        bridge.set_pending(request);

        let edit_result = bridge.begin_edit_command();
        assert_eq!(edit_result.bridge_status, "editing");
        assert!(edit_result.editing);
        assert_eq!(edit_result.command.as_deref(), Some("echo ok"));
        assert!(bridge.has_pending_permission());

        assert_eq!(
            bridge.push_edit_command_char('!').as_deref(),
            Some("echo ok!")
        );
        let submit_result = bridge.submit_edited_command();
        assert_eq!(submit_result.bridge_status, "submitted");
        assert!(submit_result.submitted);
        assert!(!submit_result.requires_decision_bridge);
        assert!(!bridge.has_pending_permission());

        match response.await.expect("edited command response") {
            PermissionDecision::AllowWithUpdatedInput { updated_input } => {
                assert_eq!(updated_input["command"], "echo ok!");
            }
            other => panic!("expected updated input decision, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn terminal_approval_bridge_pastes_normalized_command_text() {
        let mut bridge = TerminalRenderApprovalBridge::default();
        let (request, response) = permission_request_for_test("tool-paste");
        bridge.set_pending(request);

        assert_eq!(bridge.begin_edit_command().bridge_status, "editing");
        assert_eq!(
            bridge
                .paste_edit_command_text(" && echo pasted\r\nnext\u{1b}[31m")
                .as_deref(),
            Some("echo ok && echo pasted\nnext[31m")
        );

        let submit_result = bridge.submit_edited_command();
        assert_eq!(submit_result.bridge_status, "submitted");
        match response.await.expect("pasted command response") {
            PermissionDecision::AllowWithUpdatedInput { updated_input } => {
                assert_eq!(updated_input["command"], "echo ok && echo pasted\nnext[31m");
            }
            other => panic!("expected updated input decision, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn terminal_approval_bridge_edit_command_empty_stays_pending() {
        let mut bridge = TerminalRenderApprovalBridge::default();
        let (request, _response) = permission_request_for_test("tool-edit-empty");
        bridge.set_pending(request);

        assert_eq!(bridge.begin_edit_command().bridge_status, "editing");
        for _ in 0.."echo ok".len() {
            bridge.backspace_edit_command();
        }
        let edit_result = bridge.submit_edited_command();
        assert_eq!(edit_result.bridge_status, "empty_command");
        assert!(edit_result.editing);
        assert!(!edit_result.submitted);
        assert!(edit_result.requires_decision_bridge);
        assert!(bridge.has_pending_permission());
    }

    #[tokio::test]
    async fn terminal_approval_bridge_interrupt_denies_pending_permission() {
        let mut bridge = TerminalRenderApprovalBridge::default();
        let (request, response) = permission_request_for_test("tool-interrupt");
        bridge.set_pending(request);

        assert!(bridge.begin_edit_command().editing);
        assert!(bridge.edit_command_is_active());
        assert!(bridge.cancel_pending_permission());
        assert!(!bridge.has_pending_permission());
        assert!(!bridge.edit_command_is_active());
        assert_eq!(
            response.await.expect("interrupt denial"),
            PermissionDecision::Deny
        );
        assert!(!bridge.cancel_pending_permission());
    }

    #[test]
    fn terminal_approval_bridge_reports_no_pending_permission() {
        let mut bridge = TerminalRenderApprovalBridge::default();

        let result = bridge.submit_action(TERMINAL_APPROVAL_ACTION_APPROVE_ONCE);

        assert_eq!(result.bridge_status, "no_pending_permission");
        assert!(!result.submitted);
        assert!(result.requires_decision_bridge);
    }

    #[test]
    fn mcp_configs_from_raw_adds_local_scope_for_project_config_shape() {
        let raw = HashMap::from([(
            "slow".to_string(),
            serde_json::json!({
                "type": "stdio",
                "command": "python3",
                "args": ["server.py"]
            }),
        )]);

        let configs = mcp_configs_from_raw(raw, ConfigScope::Local);
        let config = configs.get("slow").expect("scoped config");

        assert_eq!(config.scope, ConfigScope::Local);
        assert!(matches!(
            config.config,
            mossen_mcp::config::McpServerConfig::Stdio(_)
        ));
    }
}
