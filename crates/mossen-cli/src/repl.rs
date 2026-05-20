//! REPL 循环启动 — TUI 初始化与事件循环。
//!
//! 对应 TS 的 replLauncher.tsx、App 组件和 REPL 主循环。
//! 使用 mossen-tui 提供的 App 和 EventBus 驱动交互式会话。

use anyhow::Result;
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::{info, warn};
use mossen_types::hooks::HookEvent;

use mossen_agent::engine::{submit_prompt, SessionOrchestrator};
use mossen_agent::types::{
    OrchestratorConfig, OriginTag, PromptParams, SdkMessage, SubmitOptions,
};
use mossen_mcp::protocol::Implementation;
use mossen_mcp::server::McpServerManager;
use mossen_mcp::config::ScopedMcpServerConfig;
use mossen_types::ToolUseContext;

use crate::bootstrap::SharedBootstrapState;
use crate::commands_registry::DirectiveRegistry;
use crate::tools_registry::InstrumentRegistry;

/// REPL 配置。
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
    /// 关闭信号标志。
    pub shutdown_flag: Arc<AtomicBool>,
}

/// 返回 oneshot / exec 路径的默认 model id。
/// 优先级：MOSSEN_CODE_CUSTOM_MODEL → "custom-backend-model"
fn default_model_for_unset_cli() -> String {
    std::env::var("MOSSEN_CODE_CUSTOM_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "custom-backend-model".to_string())
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

    // SessionStart hook stub：通知 watcher 交互式 REPL 会话已启动。
    // 当前为 stub 实现，仅记录日志。后续可替换为正式 HookManager。
    info!(
        target: "mossen_agent::hooks",
        hook_event = ?HookEvent::SessionStart,
        cwd = %cwd,
        is_interactive = true,
        "Session-start hook: interactive REPL session about to start"
    );

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
        // Read project + user memory now (one-shot per session) so the
        // composer has the full instruction surface to inject.
        let memory_text = crate::system_prompt::gather_memory_text(&cwd_path).await;
        let inputs = crate::system_prompt::SystemPromptInputs {
            cwd: &cwd,
            model: &model,
            model_marketing_name: None,
            is_non_interactive: false,
            is_custom_backend: is_custom,
            is_ant: std::env::var("USER_TYPE").ok().as_deref() == Some("ant"),
            is_git_repo: is_git,
            product_name: "Mossen",
            enabled_tools: &[
                "Bash".into(),
                "Read".into(),
                "Edit".into(),
                "Write".into(),
                "Glob".into(),
                "Grep".into(),
                "WebFetch".into(),
                "WebSearch".into(),
                "TodoWrite".into(),
                "TaskCreate".into(),
                "AskUserQuestion".into(),
            ],
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
        api_key: std::env::var("MOSSEN_API_KEY")
            .ok()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
        origin_tag: OriginTag::Repl,
        max_turns: None,
        extra_body: Default::default(),
        output_style: None,
    };
    let directives = std::sync::Arc::new(mossen_commands::all_directives());
    let app = mossen_tui::App::with_engine(engine_config, directives);
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
    tool_registry.register_all(mossen_tools::all_tools());
    let tool_registry = Arc::new(tool_registry);
    info!(
        tool_count = tool_registry.len(),
        "launch_repl: tool registry built from mossen_tools::all_tools()"
    );
    let app = app.with_tool_registry(tool_registry);

    // Hook the live TaskStore into the TUI so Ctrl+T can dump current
    // tasks without mossen-tui having to depend on mossen-tools (which
    // would form a workspace cycle).
    let app = app.with_task_snapshot_provider(std::sync::Arc::new(|| {
        mossen_tools::task_store::list_tasks()
            .into_iter()
            .map(|t| (t.status, t.id, t.subject))
            .collect()
    }));

    // 4. 初始化 MCP 服务器管理器（如果配置了）
    if config.mcp_enabled {
        info!("launch_repl: initializing MCP server manager");
        let mcp_manager = Arc::new(McpServerManager::new(Implementation {
            name: "mossen-cli".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }));

        // 解析 MCP 配置 JSON（若提供）并加载到 manager
        let configs = if let Some(cfg_json) = config.mcp_config.as_deref() {
            match serde_json::from_str::<HashMap<String, ScopedMcpServerConfig>>(cfg_json) {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "launch_repl: failed to parse mcp_config JSON; using empty");
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };
        let server_count = configs.len();
        mcp_manager.update_configs(configs).await;
        mcp_manager.connect_all().await;
        info!(
            connected = mcp_manager.connected_count(),
            total = server_count,
            "launch_repl: MCP server manager initialized"
        );

        // 双重安装：
        //   1. mossen-cli 的 repl_mcp 全局，让 shutdown 路径能 disconnect_all。
        //   2. mossen-mcp 的 OnceLock 全局，让 dialogue.rs::execute_mcp_tool
        //      可以跨 crate 解析 mcp__server__tool 调用而不在 mossen-mcp 和
        //      mossen-cli 之间引入循环依赖。
        crate::repl_mcp::set_manager(mcp_manager.clone());
        mossen_mcp::server::set_global_manager(mcp_manager);
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

    // 标记为非交互式，并捕获 model/cwd
    let (model, cwd) = {
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
        (m, s.cwd.to_string_lossy().to_string())
    };

    // Build the executable tool registry the same way `launch_repl` does so
    // --oneshot / --print mode has the same tool surface as the TUI. Without
    // this the model is forced to fall back to writing bash inside markdown
    // code blocks (or worse, hallucinating XML-style `<tool_call>` tags).
    let mut oneshot_registry = mossen_agent::tool_registry::ToolRegistry::new();
    oneshot_registry.register_all(mossen_tools::all_tools());
    let oneshot_registry = Arc::new(oneshot_registry);
    let oneshot_tools = oneshot_registry.definitions();
    let oneshot_tool_names: Vec<String> = oneshot_tools.iter().map(|t| t.name.clone()).collect();

    // Compose the same layered system prompt the REPL uses; without it,
    // oneshot calls go to the model with zero identity / env context and
    // the assistant treats them like raw chat completions.
    let system_prompt_blocks = if let Some(text) = config.system_prompt.clone() {
        vec![mossen_agent::types::SystemBlock {
            text,
            cache_control: None,
        }]
    } else {
        let is_custom = mossen_utils::custom_backend::is_custom_backend_enabled();
        let cwd_path = std::path::PathBuf::from(&cwd);
        let memory_text = crate::system_prompt::gather_memory_text(&cwd_path).await;
        let inputs = crate::system_prompt::SystemPromptInputs {
            cwd: &cwd,
            model: &model,
            model_marketing_name: None,
            is_non_interactive: true,
            is_custom_backend: is_custom,
            is_ant: std::env::var("USER_TYPE").ok().as_deref() == Some("ant"),
            is_git_repo: crate::system_prompt::detect_git_repo(&cwd_path),
            product_name: "Mossen",
            enabled_tools: &oneshot_tool_names,
            language_preference: None,
            memory_text: &memory_text,
        };
        crate::system_prompt::assemble(&inputs)
    };

    // 构造 PromptParams 并通过 mossen-agent submit_prompt 执行真实查询
    let prompt_params = PromptParams {
        prompt: prompt.clone(),
        additional_blocks: Vec::new(),
        model,
        system_prompt: system_prompt_blocks,
        tools: oneshot_tools,
        tool_use_context: ToolUseContext {
            cwd,
            additional_working_directories: None,
            extra: Default::default(),
        },
        origin_tag: OriginTag::Sdk,
        // Allow the full tool/response round-trip. With max_turns=1 the loop
        // exits the instant the model emits its first tool_use, so the user
        // never sees the assistant's follow-up summary or the tool result.
        // Setting a low double-digit cap keeps unbounded loops safe while
        // letting real multi-tool flows complete (read → think → edit →
        // verify, etc.).
        max_turns: Some(12),
        api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
        api_key: std::env::var("MOSSEN_API_KEY")
            .ok()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
        extra_body: Default::default(),
        // Oneshot mode is unattended — there's nothing to prompt the user
        // with — so leave the gate open.
        permission_gate: None,
        tool_registry: Some(oneshot_registry),
    };

    info!(prompt_len = prompt.len(), "run_oneshot: dispatching to agent");

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
    Ok(result_text)
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

    // 1. 启用 crossterm raw mode + alternate screen
    enable_raw_mode().map_err(|e| anyhow::anyhow!("failed to enable raw mode: {}", e))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| anyhow::anyhow!("failed to enter alternate screen: {}", e))?;

    // 2. 构建 ratatui Terminal
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)
        .map_err(|e| anyhow::anyhow!("failed to create terminal: {}", e))?;

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
    let _ = execute!(io::stdout(), LeaveAlternateScreen);

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
        api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
        api_key: std::env::var("MOSSEN_API_KEY")
            .ok()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
        skip_stop_hooks: true,
        auto_mode: false,
        extra_body: Default::default(),
        // `submit_once` is the non-interactive test/SDK shortcut — no UI to
        // drive a permission modal — so we leave the gate at the
        // `AllowAllGate` default.
        permission_gate: None,
        tool_registry: None,
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
