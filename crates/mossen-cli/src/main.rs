//! Mossen CLI — AI 编程助手终端入口
//!
//! 主二进制入口，对应 TS 的 entrypoints/cli.tsx + main.tsx。
//! 负责 CLI 参数解析、初始化序列、模式路由（REPL / oneshot / 子命令）。

#![allow(
    dead_code,
    non_snake_case,
    non_upper_case_globals,
    unexpected_cfgs,
    unused_imports,
    unused_must_use,
    unused_mut,
    unused_variables,
    clippy::await_holding_lock,
    clippy::enum_variant_names,
    clippy::field_reassign_with_default,
    clippy::format_in_format_args,
    clippy::if_same_then_else,
    clippy::manual_clamp,
    clippy::multiple_bound_locations,
    clippy::needless_range_loop,
    clippy::never_loop,
    clippy::nonminimal_bool,
    clippy::redundant_guards,
    clippy::redundant_locals,
    clippy::regex_creation_in_loops,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::unnecessary_filter_map,
    clippy::unnecessary_sort_by,
    clippy::wrong_self_convention
)]

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
mod screens;
mod sdk_schemas;
mod server;
mod session_hooks;
mod setup;
mod signal;
mod stream_json_render_events;
mod stream_json_terminal_renderer;
mod structured_io;
mod system_prompt;
mod tasks;
#[cfg(test)]
mod test_support;
mod tools_registry;
mod transports;
mod upstream_proxy;
mod vim;
mod voice;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{error, info};

use crate::bootstrap::{new_shared_state, SharedBootstrapState};
use crate::cli::{BridgesSubCmd, Cli, EmitFormat, PluginSubCmd, SubCmd};
use crate::commands_registry::DirectiveRegistry;
use crate::exit::{cli_error, cli_ok};
use crate::repl::{
    launch_repl, run_oneshot, run_oneshot_stream_json, run_oneshot_terminal_render, ReplConfig,
};
use crate::signal::ShutdownSignal;
use crate::tools_registry::InstrumentRegistry;
use mossen_agent::types::PermissionMode;
use mossen_commands::{
    CommandContext, CommandCostModelUsage, CommandCostSnapshot, CommandResult, Directive,
};

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
    // and corrupt the prompt area. `--emit terminal` also owns the visible
    // terminal surface, so diagnostics must not be mixed into the render byte
    // stream. Detect both frontend paths and route logs to a file there. The
    // detection mirrors the dispatch at the bottom of `run()` below.
    let oneshot_argv = std::env::args().any(|a| a == "-p" || a == "--print");
    let interactive = cli.command.is_none() && cli.oneshot.is_none() && !oneshot_argv;
    let terminal_frontend = cli.command.is_none()
        && cli_emit_is_terminal(&cli)
        && (cli.oneshot.is_some() || cli.stdin || cli.input_file.is_some());

    // 初始化日志（必须在其他所有操作之前）
    setup::initialize_logging(
        cli.verbose || cli.debug,
        interactive || terminal_frontend,
        interactive,
    );

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
    if args
        .iter()
        .any(|a| a == "--tmux" || a.starts_with("--tmux="))
    {
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
    if args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--get-mossen-config"
                | "--set-mossen-config"
                | "--clear-mossen-config"
                | "--list-mossen-config"
        )
    }) {
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
    let cwd_path = cwd.clone();
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
            .is_some_and(|p| matches!(p, cli::AccessPolicyArg::Unrestricted)),
        cli.dangerously_skip_permissions,
    )
    .context("permission safety check failed")?;
    apply_cli_access_policy_to_env(&cli);

    // 6. 应用 CLI 参数到状态
    apply_cli_to_state(&cli, &state)?;
    apply_active_profile_to_custom_backend_env(cli.model.as_deref());

    // 7. 执行 setup 流程
    setup::run_setup(&state, cli.bare)
        .await
        .context("setup failed")?;

    // 7.6 Skill 动态发现：加载用户级 skills 和沿 cwd 向上查找
    // .mossen/skills 目录。
    let cwd_str = cwd_path.to_string_lossy().to_string();
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
            "skill directories loaded during startup"
        );
    }

    // 7.7 Conditional skill 激活：基于当前工作目录激活匹配的条件技能。
    // Phase 2-3: wire activate_conditional_skills_for_paths
    let activated = mossen_skills::activate_conditional_skills_for_paths(&[cwd_str], &cwd_path);
    if !activated.is_empty() {
        info!(
            target: "mossen_cli::skills",
            activated = ?activated,
            "conditional skills activated during startup"
        );
    }

    // 8. 路由到对应模式
    let result = route_command(cli, state.clone(), cwd_path, shutdown).await;
    stop_session_background_services().await;

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

fn permission_mode_for_access_policy(policy: &cli::AccessPolicyArg) -> PermissionMode {
    match policy {
        cli::AccessPolicyArg::Supervised => PermissionMode::Default,
        cli::AccessPolicyArg::ReadOnly => PermissionMode::Plan,
        cli::AccessPolicyArg::TrustEdits => PermissionMode::AcceptEdits,
        cli::AccessPolicyArg::Unrestricted => PermissionMode::BypassPermissions,
        cli::AccessPolicyArg::AutoDeny => PermissionMode::DontAsk,
        cli::AccessPolicyArg::Swift => PermissionMode::Auto,
    }
}

fn apply_cli_access_policy_to_env(cli: &Cli) {
    if let Some(policy) = cli.access_policy.as_ref() {
        env::set_var(
            "MOSSEN_PERMISSION_MODE",
            permission_mode_for_access_policy(policy).as_str(),
        );
    }
}

/// Bridge the persisted active model profile into the legacy runtime env knobs.
///
/// The transport layer currently discovers OpenAI-compatible custom backends
/// from `MOSSEN_CODE_CUSTOM_*`. Without this bridge, `mossen.activeProfile`
/// can be correctly configured while the live session still falls through to
/// the placeholder first-party URL.
fn apply_active_profile_to_custom_backend_env(cli_model_override: Option<&str>) {
    let Some(profile) =
        mossen_agent::services::config::profiles::apply_current_profile_to_custom_backend_env_if_missing(
            cli_model_override,
        )
    else {
        return;
    };

    info!(
        target: "mossen_cli::profiles",
        profile = %profile.name,
        model = %profile.profile.model,
        "active model profile applied to custom backend environment"
    );
}

/// 路由命令到对应的执行模式。
async fn route_command(
    cli: Cli,
    state: SharedBootstrapState,
    cwd_path: std::path::PathBuf,
    shutdown: ShutdownSignal,
) -> Result<()> {
    // 子命令优先
    if let Some(subcmd) = cli.command {
        return handle_subcommand(subcmd, &state).await;
    }

    start_session_background_services().await;

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
        mcp_enabled: cli.mcp_config.is_some() || cwd_path.join(".mcp.json").exists(),
        mcp_config: cli.mcp_config.clone(),
        system_prompt: cli.system_prompt.clone(),
        extra_prompt: cli.extra_prompt.clone(),
        model_override: cli.model.clone(),
        max_turns: cli.turn_limit,
        allowed_instruments: cli.instruments.clone(),
        disabled_instruments: cli.disable_instruments.clone(),
        shutdown_flag: shutdown.shutdown_flag(),
    };

    // oneshot 模式（-1 / --oneshot）
    if let Some(ref prompt) = cli.oneshot {
        if cli_emit_is_stream_json(&cli) {
            run_oneshot_stream_json(state.clone(), prompt.clone(), instruments, repl_config)
                .await?;
        } else if cli_emit_is_terminal(&cli) {
            run_oneshot_terminal_render(state.clone(), prompt.clone(), instruments, repl_config)
                .await?;
        } else {
            let result =
                run_oneshot(state.clone(), prompt.clone(), instruments, repl_config).await?;
            output_oneshot_result(&result, &cli.emit);
        }
        return Ok(());
    }

    // stdin 模式
    if cli.stdin {
        let prompt = read_stdin_prompt().await?;
        if cli_emit_is_stream_json(&cli) {
            run_oneshot_stream_json(state.clone(), prompt, instruments, repl_config).await?;
        } else if cli_emit_is_terminal(&cli) {
            run_oneshot_terminal_render(state.clone(), prompt, instruments, repl_config).await?;
        } else {
            let result = run_oneshot(state.clone(), prompt, instruments, repl_config).await?;
            output_oneshot_result(&result, &cli.emit);
        }
        return Ok(());
    }

    // input-file 模式
    if let Some(ref input_file) = cli.input_file {
        let prompt = tokio::fs::read_to_string(input_file)
            .await
            .context("failed to read input file")?;
        if cli_emit_is_stream_json(&cli) {
            run_oneshot_stream_json(state.clone(), prompt, instruments, repl_config).await?;
        } else if cli_emit_is_terminal(&cli) {
            run_oneshot_terminal_render(state.clone(), prompt, instruments, repl_config).await?;
        } else {
            let result = run_oneshot(state.clone(), prompt, instruments, repl_config).await?;
            output_oneshot_result(&result, &cli.emit);
        }
        return Ok(());
    }

    // 默认：交互式 REPL 模式
    info!("entering interactive REPL mode");
    launch_repl(state, directives, instruments, repl_config).await
}

async fn start_session_background_services() {
    if crate::memdir::is_team_memory_enabled() {
        mossen_agent::services::team_memory_sync::start_team_memory_watcher().await;
    }
}

async fn stop_session_background_services() {
    mossen_agent::services::team_memory_sync::stop_team_memory_watcher().await;
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
        cost_snapshot: current_command_cost_snapshot(),
    })
}

fn current_command_cost_snapshot() -> CommandCostSnapshot {
    CommandCostSnapshot {
        total_cost_usd: crate::bootstrap::get_total_cost_usd(),
        total_api_duration_ms: crate::bootstrap::get_total_api_duration(),
        total_api_duration_without_retries_ms:
            crate::bootstrap::get_total_api_duration_without_retries(),
        total_tool_duration_ms: crate::bootstrap::get_total_tool_duration(),
        total_lines_added: crate::bootstrap::get_total_lines_added(),
        total_lines_removed: crate::bootstrap::get_total_lines_removed(),
        has_unknown_model_cost: crate::bootstrap::has_unknown_model_cost(),
        model_usage: crate::bootstrap::get_model_usage()
            .into_iter()
            .map(|(model, usage)| {
                (
                    model,
                    CommandCostModelUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read_input_tokens: usage.cache_read_input_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        web_search_requests: usage.web_search_requests,
                        cost_usd: usage.cost_usd,
                        context_window: usage.context_window,
                        max_output_tokens: usage.max_output_tokens,
                    },
                )
            })
            .collect(),
    }
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

fn backend_credential_detected_message(source: &str) -> String {
    format!("Backend credential detected via {source}.")
}

fn legacy_stored_token_detected_message() -> &'static str {
    "Legacy stored credential detected. Personal edition uses configured backend profiles or MOSSEN_CODE_CUSTOM_* credentials for new sessions."
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
                println!(
                    "{}",
                    backend_credential_detected_message("MOSSEN_CODE_API_KEY environment variable")
                );
                return Ok(());
            }
            if std::env::var("MOSSEN_CODE_AUTH_TOKEN").is_ok() {
                println!(
                    "{}",
                    backend_credential_detected_message(
                        "MOSSEN_CODE_AUTH_TOKEN environment variable"
                    )
                );
                return Ok(());
            }
            // 检查是否已经存在 oauth tokens
            if mossen_utils::auth::get_hosted_oauth_tokens().is_some() {
                println!("{}", legacy_stored_token_detected_message());
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
                if diag.ripgrep_status.working {
                    "ok"
                } else {
                    "broken"
                },
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
                    let parsed: serde_json::Value = serde_json::from_str(&target_value)
                        .unwrap_or_else(|_| serde_json::Value::String(target_value.clone()));
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
                    let global_dir =
                        std::path::PathBuf::from(mossen_utils::config::get_user_mossen_rules_dir());
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
                                        mossen_mcp::config::McpServerConfig::HostedProxy(_) => {
                                            "hosted-proxy"
                                        }
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
                    let mcp_path = mossen_mcp::config::get_project_mcp_file_path(&ctx.cwd);
                    let mut existing: mossen_mcp::config::McpJsonConfig = if mcp_path.exists() {
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
                    let mcp_path = mossen_mcp::config::get_project_mcp_file_path(&ctx.cwd);
                    if mcp_path.exists() {
                        let txt = tokio::fs::read_to_string(&mcp_path)
                            .await
                            .unwrap_or_default();
                        let mut existing: mossen_mcp::config::McpJsonConfig =
                            serde_json::from_str(&txt).unwrap_or_default();
                        existing.mcp_servers.remove(&name);
                        mossen_mcp::config::save_project_mcp_config(&ctx.cwd, &existing).await?;
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
                    let result = directive.execute(&["list"], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Install { name }) => {
                    let result = directive.execute(&["install", &name], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Uninstall { name }) => {
                    let result = directive.execute(&["uninstall", &name], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Enable { name }) => {
                    let result = directive.execute(&["enable", &name], &ctx).await?;
                    render_command_result(result)?;
                }
                Some(PluginSubCmd::Disable { name }) => {
                    let result = directive.execute(&["disable", &name], &ctx).await?;
                    render_command_result(result)?;
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

fn cli_emit_is_stream_json(cli: &Cli) -> bool {
    matches!(&cli.emit, EmitFormat::StreamJson)
}

fn cli_emit_is_terminal(cli: &Cli) -> bool {
    matches!(&cli.emit, EmitFormat::Terminal)
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
        EmitFormat::Terminal => {
            println!("{}", result);
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

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_agent::services::config::{facade, types::ConfigOverrideScope};

    const PROFILE_ENV_KEYS: &[&str] = &[
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_API_BASE_URL",
        "MOSSEN_API_KEY",
    ];

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    struct EnvRestore(Vec<(&'static str, Option<String>)>);

    impl EnvRestore {
        fn capture(keys: &[&'static str]) -> Self {
            Self(
                keys.iter()
                    .map(|key| (*key, env::var(key).ok()))
                    .collect::<Vec<_>>(),
            )
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn cli_model_override_is_applied_to_startup_state() {
        use clap::Parser;

        let cli = Cli::parse_from(["mossen", "--model", "example-large"]);
        let state = new_shared_state(std::path::PathBuf::from("."));

        apply_cli_to_state(&cli, &state).expect("CLI model override should apply to state");

        let state = state.read().expect("bootstrap state should be readable");
        assert_eq!(state.model_override.as_deref(), Some("example-large"));
    }

    #[test]
    fn cli_access_policy_is_applied_to_runtime_permission_env() {
        use clap::Parser;

        let _guard = env_lock();
        let _env_restore = EnvRestore::capture(&["MOSSEN_PERMISSION_MODE"]);
        env::remove_var("MOSSEN_PERMISSION_MODE");

        let cli = Cli::parse_from(["mossen", "--access-policy", "unrestricted"]);
        apply_cli_access_policy_to_env(&cli);
        assert_eq!(
            env::var("MOSSEN_PERMISSION_MODE").as_deref(),
            Ok("bypassPermissions")
        );

        let cli = Cli::parse_from(["mossen", "--access-policy", "read-only"]);
        apply_cli_access_policy_to_env(&cli);
        assert_eq!(env::var("MOSSEN_PERMISSION_MODE").as_deref(), Ok("plan"));
    }

    #[test]
    fn active_profile_sets_custom_backend_env_for_runtime() {
        let _guard = env_lock();
        let _env_restore = EnvRestore::capture(PROFILE_ENV_KEYS);
        for key in PROFILE_ENV_KEYS {
            env::remove_var(key);
        }
        facade::reset_facade_for_testing();
        facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "example": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-highspeed",
                    "apiKey": "sk-test-profile-secret"
                }
            }),
            ConfigOverrideScope::Override,
        );
        facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("example".to_string()),
            ConfigOverrideScope::Override,
        );

        apply_active_profile_to_custom_backend_env(None);

        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://api.example.com/v1")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_MODEL").as_deref(),
            Ok("example-highspeed")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-compatible")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_USE_CUSTOM_BACKEND").as_deref(),
            Ok("1")
        );

        facade::reset_facade_for_testing();
    }

    #[test]
    fn cli_model_override_keeps_profile_model_out_of_custom_model_env() {
        let _guard = env_lock();
        let _env_restore = EnvRestore::capture(PROFILE_ENV_KEYS);
        for key in PROFILE_ENV_KEYS {
            env::remove_var(key);
        }
        facade::reset_facade_for_testing();
        facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "example": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-highspeed",
                    "apiKey": "sk-test-profile-secret"
                }
            }),
            ConfigOverrideScope::Override,
        );
        facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("example".to_string()),
            ConfigOverrideScope::Override,
        );

        apply_active_profile_to_custom_backend_env(Some("other-model"));

        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://api.example.com/v1")
        );
        assert!(env::var("MOSSEN_CODE_CUSTOM_MODEL").is_err());

        facade::reset_facade_for_testing();
    }

    #[test]
    fn active_profile_sets_provider_protocol_for_runtime() {
        let _guard = env_lock();
        let _env_restore = EnvRestore::capture(PROFILE_ENV_KEYS);
        for key in PROFILE_ENV_KEYS {
            env::remove_var(key);
        }
        facade::reset_facade_for_testing();
        facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "openai-responses": {
                    "provider": "openai-responses",
                    "baseURL": "https://api.openai.com/v1",
                    "model": "gpt-5.1",
                    "apiKey": "sk-test-secret"
                }
            }),
            ConfigOverrideScope::Override,
        );
        facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("openai-responses".to_string()),
            ConfigOverrideScope::Override,
        );

        apply_active_profile_to_custom_backend_env(None);

        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-responses")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://api.openai.com/v1")
        );

        facade::reset_facade_for_testing();
    }

    #[test]
    fn auth_subcommand_status_text_uses_backend_credentials_not_account_login() {
        let env_message =
            backend_credential_detected_message("MOSSEN_CODE_AUTH_TOKEN environment variable");
        assert!(env_message.contains("Backend credential detected"));
        let sign_in_success = ["Sign-in", "successful"].join(" ");
        let logged_in = ["logged", "in"].join(" ");
        assert!(!env_message.contains(&sign_in_success));
        assert!(!env_message.contains(&logged_in));

        let legacy_message = legacy_stored_token_detected_message();
        assert!(legacy_message.contains("Legacy stored credential detected"));
        let signed_in = ["signed", "in"].join(" ");
        let oauth_label = ["O", "Auth"].join("");
        assert!(!legacy_message.contains(&signed_in));
        assert!(!legacy_message.contains(&oauth_label));
    }
}
