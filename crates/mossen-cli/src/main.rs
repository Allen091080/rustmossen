//! Mossen CLI — AI 编程助手终端入口
//!
//! 主二进制入口，对应 TS 的 entrypoints/cli.tsx + main.tsx。
//! 负责 CLI 参数解析、初始化序列、模式路由（REPL / oneshot / 子命令）。

#![allow(dead_code, unused_imports)]

mod app_state;
mod assistant;
mod bootstrap;
mod buddy;
mod cli;
mod commands_registry;
mod cost_tracker;
mod dialog_launchers;
mod entrypoints;
mod exit;
mod handlers;
mod history;
mod interactive;
mod keybindings;
mod memdir;
mod migrations;
mod native_color_diff;
mod native_file_index;
mod native_yoga;
mod output_styles;
mod platform;
mod plugin_handlers;
mod plugins;
mod print_handlers;
mod proactive;
mod project_onboarding;
mod query_engine;
mod remote_io;
mod repl;
mod repl_mcp;
mod root_modules;
mod schemas_hooks;
mod sdk_schemas;
mod screens;
mod server;
mod setup;
mod system_prompt;
mod signal;
mod structured_io;
mod tasks;
mod tools_registry;
mod transports;
mod upstream_proxy;
mod vim;
mod voice;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{error, info};

use crate::bootstrap::{new_shared_state, SharedBootstrapState};
use crate::cli::{BridgesSubCmd, Cli, EmitFormat, PluginSubCmd, SubCmd};
use crate::commands_registry::DirectiveRegistry;
use crate::exit::{cli_error, cli_ok};
use crate::repl::{launch_repl, run_oneshot, ReplConfig};
use crate::signal::ShutdownSignal;
use crate::tools_registry::InstrumentRegistry;
use mossen_commands::{CommandContext, CommandResult, Directive};

/// 程序版本号（从 Cargo.toml 读取）。
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    // Pre-main fast-path sequence — mirrors `entrypoints/cli.tsx`:
    //
    //   1. Apply the USER_TYPE runtime lock so any downstream code that reads
    //      `USER_TYPE` sees the normalized value (`external` unless the
    //      explicit internal-unlock env var is set).
    //   2. Restore the working directory from `MOSSENSRC_LAUNCH_CWD` if a
    //      launcher wrapper passed one through, then clear the variable.
    //   3. Record the `cli_entry` startup profiler checkpoint.
    //   4. Install process output (EPIPE / SIGPIPE) handlers so piping into
    //      `head` / `less` etc. does not kill the process or leak memory.
    //   5. Start capturing keystrokes typed during startup so the REPL can
    //      consume them once it renders. Skipped automatically when stdin
    //      isn't a TTY or `-p`/`--print` is in argv.
    //
    // These must run before any other module loads — they normalise process
    // state the rest of the CLI relies on.
    mossen_utils::user_type_runtime_lock::apply_user_type_runtime_lock();
    mossen_utils::cwd::restore_launch_cwd();
    mossen_utils::startup_profiler::profile_checkpoint("cli_entry");
    mossen_utils::process::register_process_output_error_handlers();
    mossen_utils::early_input::start_capturing_early_input();

    // Pre-parse fast paths — handle CLI flags that must short-circuit before
    // the bootstrap pipeline runs. Each helper returns `(handled, exit_code)`
    // so we can exit cleanly without ever constructing the bootstrap state.
    if run_fast_paths().await {
        std::process::exit(exit::codes::OK);
    }

    // 快速路径：--version / --help 由 clap 自动处理
    let cli = Cli::parse();

    // The TUI owns the alternate screen + raw mode the moment we launch the
    // REPL, so any stderr writes from tracing bleed straight through ratatui
    // and corrupt the prompt area. Detect whether this run will hit the TUI
    // (no subcommand, no --oneshot, no -p/--print short forms) and route
    // logs to a file in that case. The detection mirrors the dispatch at the
    // bottom of `run()` below.
    let oneshot_argv = std::env::args().any(|a| a == "-p" || a == "--print");
    let interactive = cli.command.is_none() && cli.oneshot.is_none() && !oneshot_argv;

    // 初始化日志（必须在其他所有操作之前）
    setup::initialize_logging(cli.verbose || cli.debug, interactive);

    info!(version = VERSION, "mossen starting");

    // 运行主逻辑，捕获所有错误
    let exit_code = match run(cli).await {
        Ok(()) => exit::codes::OK,
        Err(err) => {
            error!("fatal error: {:#}", err);
            exit::codes::ERROR
        }
    };

    std::process::exit(exit_code);
}

/// Pre-clap fast-path dispatcher — mirrors `runFastPaths` from
/// `entrypoints/cli.tsx`. Each handler runs before the main bootstrap pipeline
/// and exits the process directly if it matches. Returns `true` when a fast
/// path handled the invocation, in which case the caller exits with status 0.
async fn run_fast_paths() -> bool {
    let args: Vec<String> = std::env::args().collect();

    // --tmux / --tmux=<mode>: launch into a git-worktree-backed tmux session.
    if args.iter().any(|a| a == "--tmux" || a.starts_with("--tmux=")) {
        let result = mossen_utils::worktree::exec_into_tmux_worktree(&args).await;
        if result.handled {
            return true;
        }
        if let Some(err) = result.error {
            mossen_utils::process::exit_with_error(&err);
        }
    }

    // --get/set/clear/list-mossen-config: internal debug flags, hidden from
    // --help, used by support tooling to poke at the merged config tree.
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--get-mossen-config" | "--set-mossen-config" | "--clear-mossen-config" | "--list-mossen-config"))
    {
        let (handled, exit_code) =
            mossen_agent::services::config::profile_cli::handle_config_cli_flag(&args).await;
        if handled {
            std::process::exit(exit_code);
        }
    }

    // Multi-profile flags (--list/get/set/add/update/delete/test-model-profile,
    // --migrate-fallback-profile). User-facing — listed in --help.
    if mossen_agent::services::config::profile_cli::is_model_profile_flag_present(&args) {
        let (handled, exit_code) =
            mossen_agent::services::config::profile_cli::handle_model_profile_cli_flag(&args).await;
        if handled {
            std::process::exit(exit_code);
        }
    }

    false
}

/// 主运行逻辑。
async fn run(cli: Cli) -> Result<()> {
    // 1. 确定工作目录
    let cwd = cli
        .effective_cwd()
        .context("failed to determine working directory")?;
    info!(cwd = %cwd.display(), "working directory resolved");

    // 2. 创建共享启动状态
    let state = new_shared_state(cwd);

    // 3. 安装信号处理器
    let shutdown = ShutdownSignal::new();
    shutdown.install_handlers();

    // 4. 运行初始化序列
    setup::run_init_sequence(&state)
        .await
        .context("initialization failed")?;

    // 5. 安全检查
    setup::validate_permission_safety(
        cli.access_policy
            .as_ref()
            .map_or(false, |p| matches!(p, cli::AccessPolicyArg::Unrestricted)),
        cli.dangerously_skip_permissions,
    )
    .context("permission safety check failed")?;

    // 6. 应用 CLI 参数到状态
    apply_cli_to_state(&cli, &state)?;

    // 7. 执行 setup 流程
    setup::run_setup(&state, cli.bare)
        .await
        .context("setup failed")?;

    // 8. 路由到对应模式
    let result = route_command(cli, state.clone(), shutdown).await;

    // 9. 清理退出
    match result {
        Ok(()) => {
            cli_ok(&state).await?;
            Ok(())
        }
        Err(err) => {
            let code = cli_error(&state, &err).await;
            if code != 0 {
                anyhow::bail!("exiting with code {}", code);
            }
            Ok(())
        }
    }
}

/// 将 CLI 参数应用到启动状态。
fn apply_cli_to_state(cli: &Cli, state: &SharedBootstrapState) -> Result<()> {
    let mut s = state
        .write()
        .map_err(|e| anyhow::anyhow!("failed to write state: {}", e))?;

    s.is_interactive = !cli.is_non_interactive();
    s.bare_mode = cli.bare;
    s.remote_mode = cli.remote;

    if let Some(ref model) = cli.model {
        s.model_override = Some(model.clone());
    }

    if !cli.include_dir.is_empty() {
        s.additional_dirs = cli.include_dir.clone();
    }

    if let Some(ref agent) = cli.agent {
        s.main_agent_type = Some(agent.clone());
    }

    Ok(())
}

/// 路由命令到对应的执行模式。
async fn route_command(
    cli: Cli,
    state: SharedBootstrapState,
    shutdown: ShutdownSignal,
) -> Result<()> {
    // 子命令优先
    if let Some(subcmd) = cli.command {
        return handle_subcommand(subcmd, &state).await;
    }

    // 注册命令和工具
    let directives = DirectiveRegistry::new();
    let instruments = InstrumentRegistry::new();
    info!(
        directives = directives.len(),
        instruments = instruments.len(),
        "registries initialized"
    );

    // 构建 REPL 配置
    let repl_config = ReplConfig {
        initial_prompt: cli.continue_prompt.clone(),
        restore_mode: cli.should_restore(),
        restore_session_id: cli.restore_id.clone(),
        mcp_enabled: cli.mcp_config.is_some(),
        mcp_config: cli.mcp_config.clone(),
        system_prompt: cli.system_prompt.clone(),
        extra_prompt: cli.extra_prompt.clone(),
        model_override: cli.model.clone(),
        shutdown_flag: shutdown.shutdown_flag(),
    };

    // oneshot 模式（-1 / --oneshot）
    if let Some(ref prompt) = cli.oneshot {
        let result = run_oneshot(state.clone(), prompt.clone(), instruments, repl_config).await?;
        output_oneshot_result(&result, &cli.emit);
        return Ok(());
    }

    // stdin 模式
    if cli.stdin {
        let prompt = read_stdin_prompt().await?;
        let result = run_oneshot(state.clone(), prompt, instruments, repl_config).await?;
        output_oneshot_result(&result, &cli.emit);
        return Ok(());
    }

    // input-file 模式
    if let Some(ref input_file) = cli.input_file {
        let prompt = tokio::fs::read_to_string(input_file)
            .await
            .context("failed to read input file")?;
        let result = run_oneshot(state.clone(), prompt, instruments, repl_config).await?;
        output_oneshot_result(&result, &cli.emit);
        return Ok(());
    }

    // 默认：交互式 REPL 模式
    info!("entering interactive REPL mode");
    launch_repl(state, directives, instruments, repl_config).await
}

/// 构建子命令执行所需的 CommandContext。
fn build_command_context(state: &SharedBootstrapState) -> Result<CommandContext> {
    let s = state
        .read()
        .map_err(|e| anyhow::anyhow!("failed to read state: {}", e))?;
    let env_vars: HashMap<String, String> = std::env::vars().collect();
    Ok(CommandContext {
        cwd: s.cwd.clone(),
        is_non_interactive: !s.is_interactive,
        is_remote_mode: s.remote_mode,
        is_custom_backend: std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").is_ok(),
        user_type: std::env::var("MOSSEN_CODE_USER_TYPE").ok(),
        env_vars,
        product_name: "Mossen".to_string(),
        cli_name: "mossen".to_string(),
        version: VERSION.to_string(),
        build_time: option_env!("BUILD_TIME").map(|s| s.to_string()),
    })
}

/// 渲染 CommandResult 到 stdout/stderr 并返回 exit-OK 标志。
fn render_command_result(result: CommandResult) -> Result<()> {
    match result {
        CommandResult::Text(s) | CommandResult::System(s) => {
            println!("{}", s);
            Ok(())
        }
        CommandResult::Empty | CommandResult::Widget => Ok(()),
        CommandResult::Exit(msg) => {
            if let Some(m) = msg {
                println!("{}", m);
            }
            Ok(())
        }
        CommandResult::Error(s) => {
            eprintln!("{}", s);
            anyhow::bail!("command failed");
        }
    }
}

/// 处理子命令。
async fn handle_subcommand(subcmd: SubCmd, state: &SharedBootstrapState) -> Result<()> {
    let ctx = build_command_context(state)?;
    match subcmd {
        SubCmd::Evolve => {
            info!("subcommand: evolve (version upgrade)");
            setup::check_for_updates().await?;
            let directive = mossen_commands::evolve::EvolveDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
        SubCmd::Auth => {
            info!("subcommand: auth (login)");
            // 首先尝试通过 mossen-utils auth 模块的环境变量快速路径
            if std::env::var("MOSSEN_CODE_API_KEY").is_ok() {
                println!("Sign-in successful (via MOSSEN_CODE_API_KEY environment variable).");
                return Ok(());
            }
            if std::env::var("MOSSEN_CODE_AUTH_TOKEN").is_ok() {
                println!("Sign-in successful (via MOSSEN_CODE_AUTH_TOKEN environment variable).");
                return Ok(());
            }
            // 检查是否已经存在 oauth tokens
            if mossen_utils::auth::get_hosted_oauth_tokens().is_some() {
                println!("Already signed in via stored OAuth tokens.");
                return Ok(());
            }
            // 否则委托给 AuthDirective（显示如何配置 backend 凭据的说明）
            let directive = mossen_commands::auth::AuthDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
        SubCmd::Deauth => {
            info!("subcommand: deauth (logout)");
            // 清除 API key（如果存在）
            if mossen_utils::auth::has_mossen_api_key_auth() {
                mossen_utils::auth::remove_api_key().await;
            }
            // 清除 oauth token 缓存
            mossen_utils::auth::clear_oauth_token_cache();
            // 委托给 DeauthDirective 输出用户消息
            let directive = mossen_commands::deauth::DeauthDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
        SubCmd::Diagnose => {
            info!("subcommand: diagnose (doctor)");
            // 调用 mossen-utils 的 doctor 诊断器并展示结果
            let diag = mossen_utils::doctor_diagnostic::get_doctor_diagnostic().await;
            println!("Mossen Doctor");
            println!("=============\n");
            println!("Version: {}", diag.version);
            println!("Installation type: {}", diag.installation_type);
            println!("Installation path: {}", diag.installation_path);
            println!("Invoked binary: {}", diag.invoked_binary);
            println!("Auto-updates: {}", diag.auto_updates);
            println!(
                "Ripgrep: {} ({})",
                if diag.ripgrep_status.working { "ok" } else { "broken" },
                diag.ripgrep_status.mode
            );
            if !diag.multiple_installations.is_empty() {
                println!("\nMultiple installations detected:");
                for inst in &diag.multiple_installations {
                    println!("  - {} ({})", inst.path, inst.install_type);
                }
            }
            if !diag.warnings.is_empty() {
                println!("\nWarnings:");
                for w in &diag.warnings {
                    println!("  - {}", w.issue);
                    println!("    fix: {}", w.fix);
                }
            } else {
                println!("\nAll checks passed.");
            }
            // 同时调用 DiagnoseDirective 显示基本环境信息
            let directive = mossen_commands::diagnose::DiagnoseDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
        SubCmd::Config { action, key, value } => {
            info!(?action, ?key, ?value, "subcommand: config");
            match action.as_deref() {
                Some("get") => {
                    let target_key = key
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("config get requires KEY argument"))?;
                    let cfg = mossen_utils::config::get_global_config();
                    let json = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
                    match json.get(&target_key) {
                        Some(v) => println!("{}", v),
                        None => println!("(unset)"),
                    }
                }
                Some("set") => {
                    let target_key = key
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("config set requires KEY and VALUE"))?;
                    let target_value = value
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("config set requires VALUE"))?;
                    // 通过 save_global_config 写入；对 GlobalConfig 的字段以 JSON merge 形式更新
                    let parsed: serde_json::Value =
                        serde_json::from_str(&target_value).unwrap_or_else(|_| {
                            serde_json::Value::String(target_value.clone())
                        });
                    let key_for_update = target_key.clone();
                    mossen_utils::config::save_global_config(move |current| {
                        let mut as_json =
                            serde_json::to_value(current).unwrap_or(serde_json::Value::Null);
                        if let Some(obj) = as_json.as_object_mut() {
                            obj.insert(key_for_update.clone(), parsed.clone());
                        }
                        serde_json::from_value(as_json).unwrap_or_else(|_| current.clone())
                    });
                    println!("Configuration updated: {} = {}", target_key, target_value);
                }
                Some("list") | None => {
                    let cfg = mossen_utils::config::get_global_config();
                    let json = serde_json::to_string_pretty(&cfg).unwrap_or_default();
                    println!("{}", json);
                }
                Some("remove") | Some("unset") => {
                    let target_key = key
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("config remove requires KEY"))?;
                    let key_for_update = target_key.clone();
                    mossen_utils::config::save_global_config(move |current| {
                        let mut as_json =
                            serde_json::to_value(current).unwrap_or(serde_json::Value::Null);
                        if let Some(obj) = as_json.as_object_mut() {
                            obj.remove(&key_for_update);
                        }
                        serde_json::from_value(as_json).unwrap_or_else(|_| current.clone())
                    });
                    println!("Configuration removed: {}", target_key);
                }
                Some(other) => {
                    // 通过 ConfigDirective 处理 list/help 等
                    let mut args: Vec<String> = vec![other.to_string()];
                    if let Some(ref k) = key {
                        args.push(k.clone());
                    }
                    if let Some(ref v) = value {
                        args.push(v.clone());
                    }
                    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    let directive = mossen_commands::config::ConfigDirective;
                    let result = directive.execute(&arg_refs, &ctx).await?;
                    render_command_result(result)?;
                }
            }
        }
        SubCmd::Bridges { action } => {
            info!(?action, "subcommand: bridges (MCP)");
            let directive = mossen_commands::bridges::BridgesDirective;
            match action {
                Some(BridgesSubCmd::List) | None => {
                    // 加载已合并的 MCP 配置
                    let global_dir = std::path::PathBuf::from(
                        mossen_utils::config::get_user_mossen_rules_dir(),
                    );
                    let cwd = ctx.cwd.clone();
                    match mossen_mcp::config::load_merged_configs(&cwd, &global_dir).await {
                        Ok(configs) => {
                            if configs.is_empty() {
                                println!("No MCP servers configured.");
                                println!("Use `mossen bridges add <name> <uri>` to add a server.");
                            } else {
                                println!("Configured MCP servers ({}):", configs.len());
                                for (name, sc) in configs {
                                    let transport = match &sc.config {
                                        mossen_mcp::config::McpServerConfig::Stdio(_) => "stdio",
                                        mossen_mcp::config::McpServerConfig::Sse(_) => "sse",
                                        mossen_mcp::config::McpServerConfig::SseIde(_) => "sse-ide",
                                        mossen_mcp::config::McpServerConfig::WsIde(_) => "ws-ide",
                                        mossen_mcp::config::McpServerConfig::Http(_) => "http",
                                        mossen_mcp::config::McpServerConfig::Ws(_) => "ws",
                                        mossen_mcp::config::McpServerConfig::Sdk(_) => "sdk",
                                        mossen_mcp::config::McpServerConfig::HostedProxy(_) => "hosted-proxy",
                                    };
                                    println!(
                                        "  - {} [{:?}] transport={}",
                                        name, sc.scope, transport
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to load MCP configs: {}", e);
                            let result = directive.execute(&[], &ctx).await?;
                            render_command_result(result)?;
                        }
                    }
                }
                Some(BridgesSubCmd::Add { name, uri }) => {
                    // 写入项目级 .mcp.json
                    let mcp_path =
                        mossen_mcp::config::get_project_mcp_file_path(&ctx.cwd);
                    let mut existing: mossen_mcp::config::McpJsonConfig =
                        if mcp_path.exists() {
                            let txt = tokio::fs::read_to_string(&mcp_path)
                                .await
                                .unwrap_or_default();
                            serde_json::from_str(&txt).unwrap_or_default()
                        } else {
                            Default::default()
                        };
                    // 构造一个最小 server 配置：URL 使用 http/ws，否则视为 stdio command
                    let server_value: serde_json::Value = if uri.starts_with("http") {
                        serde_json::json!({
                            "type": "http",
                            "url": uri,
                        })
                    } else if uri.starts_with("ws") {
                        serde_json::json!({
                            "type": "ws",
                            "url": uri,
                        })
                    } else {
                        serde_json::json!({
                            "type": "stdio",
                            "command": uri,
                            "args": [],
                        })
                    };
                    existing.mcp_servers.insert(name.clone(), server_value);
                    mossen_mcp::config::save_project_mcp_config(&ctx.cwd, &existing).await?;
                    println!("MCP server '{}' added: {}", name, uri);
                }
                Some(BridgesSubCmd::Remove { name }) => {
                    let mcp_path =
                        mossen_mcp::config::get_project_mcp_file_path(&ctx.cwd);
                    if mcp_path.exists() {
                        let txt = tokio::fs::read_to_string(&mcp_path)
                            .await
                            .unwrap_or_default();
                        let mut existing: mossen_mcp::config::McpJsonConfig =
                            serde_json::from_str(&txt).unwrap_or_default();
                        existing.mcp_servers.remove(&name);
                        mossen_mcp::config::save_project_mcp_config(&ctx.cwd, &existing)
                            .await?;
                        println!("MCP server '{}' removed.", name);
                    } else {
                        println!("No project .mcp.json found; nothing to remove.");
                    }
                }
                Some(BridgesSubCmd::Status) => {
                    let result = directive.execute(&["status"], &ctx).await?;
                    render_command_result(result)?;
                }
            }
        }
        SubCmd::Plugin { action } => {
            info!(?action, "subcommand: plugin");
            let directive = mossen_commands::plugin::PluginDirective;
            match action {
                Some(PluginSubCmd::List) | None => {
                    // 显示内置插件 + 通过 mossen-skills 注册的
                    let builtin = mossen_skills::plugin::get_builtin_plugins(
                        &std::collections::HashMap::new(),
                    );
                    if builtin.enabled.is_empty() && builtin.disabled.is_empty() {
                        let result = directive.execute(&[], &ctx).await?;
                        render_command_result(result)?;
                    } else {
                        println!(
                            "Installed plugins (enabled: {}, disabled: {}):",
                            builtin.enabled.len(),
                            builtin.disabled.len()
                        );
                        for p in &builtin.enabled {
                            println!("  - {} [enabled] (source: {})", p.name, p.source);
                        }
                        for p in &builtin.disabled {
                            println!("  - {} [disabled] (source: {})", p.name, p.source);
                        }
                    }
                }
                Some(PluginSubCmd::Install { name }) => {
                    let result =
                        directive.execute(&["install", &name], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Uninstall { name }) => {
                    let result =
                        directive.execute(&["remove", &name], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Enable { name }) => {
                    println!("Plugin '{}' enabled.", name);
                }
                Some(PluginSubCmd::Disable { name }) => {
                    println!("Plugin '{}' disabled.", name);
                }
            }
        }
        SubCmd::Install => {
            info!("subcommand: install");
            let directive = mossen_commands::install::InstallDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
        SubCmd::Init => {
            info!("subcommand: init");
            let directive = mossen_commands::init::InitDirective;
            let result = directive.execute(&[], &ctx).await?;
            render_command_result(result)?;
        }
    }
    Ok(())
}

/// 输出 oneshot 结果。
fn output_oneshot_result(result: &str, format: &EmitFormat) {
    match format {
        EmitFormat::Text => {
            println!("{}", result);
        }
        EmitFormat::Json => {
            let json = serde_json::json!({
                "result": result,
                "type": "text",
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_default()
            );
        }
        EmitFormat::StreamJson => {
            let json = serde_json::json!({
                "type": "result",
                "data": result,
            });
            println!("{}", serde_json::to_string(&json).unwrap_or_default());
        }
    }
}

/// 从 stdin 读取提示。
async fn read_stdin_prompt() -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut buf = String::new();
    tokio::io::stdin()
        .read_to_string(&mut buf)
        .await
        .context("failed to read from stdin")?;
    if buf.trim().is_empty() {
        anyhow::bail!("empty prompt received from stdin");
    }
    Ok(buf)
}
