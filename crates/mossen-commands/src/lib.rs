//! # mossen-commands
//!
//! Mossen 命令系统 — 注册并执行内置斜杠命令（/help、/compact、/model 等）
//! 及 CLI 子命令。
//!
//! ## 模块结构
//!
//! - [`context`] — Directive trait 定义、CommandContext、CommandResult
//! - 各命令模块 — 每个文件实现一个 Directive

#![allow(
    dead_code,
    unused_assignments,
    unused_imports,
    unused_mut,
    unused_variables,
    clippy::await_holding_lock,
    clippy::if_same_then_else,
    clippy::manual_checked_ops,
    clippy::needless_range_loop,
    clippy::ptr_arg,
    clippy::should_implement_trait,
    clippy::unnecessary_sort_by,
    clippy::vec_init_then_push
)]

pub mod command_extras;
pub mod component_logic;
pub mod context;
pub mod plugin_helpers;

// ── 核心命令组（Core Directives） ─────────────────────────────────────
pub mod auth; // /login
pub mod color; // /color
pub mod color_index;
pub mod config; // /config, /settings
pub mod config_index;
pub mod deauth; // /logout
pub mod desktop; // /desktop, /app
pub mod desktop_index;
pub mod diagnose; // /doctor
pub mod env; // /env
pub mod evolve; // /upgrade
pub mod guide; // /help
pub mod ide; // /ide
pub mod install; // /install
pub mod keybindings; // /keybindings
pub mod lang; // /lang
pub mod lang_index;
pub mod login_index;
pub mod logout_index;
pub mod model_index;
pub mod onboarding; // /onboarding
pub mod palette; // /theme
pub mod profile; // /profile
pub mod switch_model; // /model
pub mod terminal_setup; // /terminal-setup
pub mod turbo;
pub mod version; // /version // /fast

// ── 开发命令组（Dev Directives） ─────────────────────────────────────
pub mod advisor; // /advisor
pub mod branch; // /branch
pub mod branch_index;
pub mod brief; // /brief
pub mod commit; // /commit
pub mod feedback; // /feedback
pub mod insights; // /insights
pub mod plan; // /plan
pub mod proactive;
pub mod project; // /project
pub mod review; // /review
pub mod security_review; // /security-review
pub mod ship; // /ship (commit-push-pr) // /proactive

// ── 系统命令组（System Directives） ──────────────────────────────────
pub mod access; // /permissions
pub mod bridges; // /mcp
pub mod chrome; // /chrome
pub mod chrome_index;
pub mod crafts; // /skills
pub mod delegates; // /agents
pub mod extra_usage; // /extra-usage
pub mod mcp_add_command;
pub mod mcp_index; // /mcp
pub mod mcp_parse_args;
pub mod metrics; // /stats
pub mod mobile; // /mobile
pub mod mobile_index;
pub mod passes; // /passes
pub mod plugin; // /plugin
pub mod plugin_parse_args;
pub mod privacy; // /privacy-settings
pub mod rate_limit; // /rate-limit-options
pub mod recall; // /memory
pub mod release_notes; // /release-notes
pub mod remote_env; // /remote-env
pub mod remote_setup;
pub mod sandbox; // /sandbox-toggle
pub mod stickers; // /stickers
pub mod stickers_index;
pub mod usage; // /usage
pub mod vim; // /vim
pub mod vim_index;
pub mod voice; // /voice
pub mod voice_index;
pub mod watchers; // /hooks
pub mod workitems; // /tasks // /remote-setup

// ── 其他命令（Misc Directives） ─────────────────────────────────────
pub mod agents_index;
pub mod assistant; // /assistant
pub mod assistant_index;
pub mod btw; // /btw
pub mod btw_index;
pub mod diff_index;
pub mod doctor_index;
pub mod effort_index;
pub mod exit_index;
pub mod export_index;
pub mod feedback_index;
pub mod github_app; // /github-app
pub mod init;
pub mod init_verifiers;
pub mod reload_plugins;
pub mod slack_app; // /slack-app
pub mod statusline; // /statusline // /reload-plugins

// ── 内部/调试命令（Internal Directives） ────────────────────────────
pub mod heapdump; // /heapdump
pub mod pr_comments;
pub mod teleport; // /teleport
pub mod thinkback; // /thinkback
pub mod thinkback_play; // /thinkback-play // /pr-comments

// ── 会话命令组（Session Directives） ──────────────────────────────────
pub mod add_dir; // /add-dir
pub mod add_dir_index;
pub mod add_dir_validation;
pub mod changes; // /diff
pub mod clear_index;
pub mod compact_index;
pub mod condense; // /compact
pub mod context_index;
pub mod copy; // /copy
pub mod copy_index;
pub mod cost_index;
pub mod effort; // /effort
pub mod environ; // /context
pub mod exit; // /exit, /quit
pub mod extract; // /export
pub mod files; // /files
pub mod files_index;
pub mod meter; // /cost
pub mod output_style; // /output-style
pub mod rename; // /rename
pub mod rename_index;
pub mod restore; // /resume, /continue
pub mod session; // /session, /remote
pub mod session_index;
pub mod share; // /share
pub mod status; // /status
pub mod summary; // /summary
pub mod tag;
pub mod undo; // /rewind, /checkpoint
pub mod wipe; // /clear, /reset, /new // /tag

// Re-export core types
pub use context::{
    BoxedDirective, CommandContext, CommandCostModelUsage, CommandCostSnapshot, CommandResult,
    Directive, DirectiveType,
};

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("command env test lock poisoned")
    }

    pub(crate) fn skill_state_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("command skill state test lock poisoned")
    }
}

/// Build the full registry of all built-in directives.
pub fn all_directives() -> Vec<BoxedDirective> {
    vec![
        // Core commands
        Box::new(auth::AuthDirective),
        Box::new(deauth::DeauthDirective),
        Box::new(config::ConfigDirective),
        Box::new(switch_model::SwitchModelDirective),
        Box::new(guide::GuideDirective),
        Box::new(diagnose::DiagnoseDirective),
        Box::new(version::VersionDirective),
        Box::new(evolve::EvolveDirective),
        Box::new(lang::LangDirective),
        Box::new(keybindings::KeybindingsDirective),
        Box::new(color::ColorDirective),
        Box::new(palette::PaletteDirective),
        Box::new(desktop::DesktopDirective),
        Box::new(ide::IdeDirective),
        Box::new(install::InstallDirective),
        Box::new(onboarding::OnboardingDirective),
        Box::new(terminal_setup::TerminalSetupDirective),
        Box::new(profile::ProfileDirective),
        Box::new(env::EnvDirective),
        Box::new(turbo::TurboDirective),
        // Session commands
        Box::new(session::SessionDirective),
        Box::new(restore::RestoreDirective),
        Box::new(extract::ExtractDirective),
        Box::new(share::ShareDirective),
        Box::new(wipe::WipeDirective),
        Box::new(condense::CondenseDirective),
        Box::new(rename::RenameDirective),
        Box::new(copy::CopyDirective),
        Box::new(changes::ChangesDirective),
        Box::new(files::FilesDirective),
        Box::new(environ::EnvironDirective),
        Box::new(effort::EffortDirective),
        Box::new(output_style::OutputStyleDirective),
        Box::new(exit::ExitDirective),
        Box::new(meter::MeterDirective),
        Box::new(status::StatusDirective),
        Box::new(add_dir::AddDirDirective),
        Box::new(undo::UndoDirective),
        Box::new(summary::SummaryDirective),
        Box::new(tag::TagDirective),
        // Dev commands
        Box::new(review::ReviewDirective),
        Box::new(commit::CommitDirective),
        Box::new(ship::ShipDirective),
        Box::new(branch::BranchDirective),
        Box::new(plan::PlanDirective),
        Box::new(project::ProjectDirective),
        Box::new(feedback::FeedbackDirective),
        Box::new(insights::InsightsDirective),
        Box::new(advisor::AdvisorDirective),
        Box::new(brief::BriefDirective),
        Box::new(security_review::SecurityReviewDirective),
        Box::new(proactive::ProactiveDirective),
        // System commands
        Box::new(access::AccessDirective),
        Box::new(privacy::PrivacyDirective),
        Box::new(sandbox::SandboxDirective),
        Box::new(bridges::BridgesDirective),
        Box::new(recall::RecallDirective),
        Box::new(crafts::CraftsDirective),
        Box::new(workitems::WorkitemsDirective),
        Box::new(delegates::DelegatesDirective),
        Box::new(plugin::PluginDirective),
        Box::new(stickers::StickersDirective),
        Box::new(metrics::MetricsDirective),
        Box::new(usage::UsageDirective),
        Box::new(passes::PassesDirective),
        Box::new(extra_usage::ExtraUsageDirective),
        Box::new(rate_limit::RateLimitDirective),
        Box::new(watchers::WatchersDirective),
        Box::new(mobile::MobileDirective),
        Box::new(chrome::ChromeDirective),
        Box::new(voice::VoiceDirective),
        Box::new(vim::VimDirective),
        Box::new(release_notes::ReleaseNotesDirective),
        Box::new(remote_env::RemoteEnvDirective),
        Box::new(remote_setup::RemoteSetupDirective),
        // Misc commands
        Box::new(btw::BtwDirective),
        Box::new(assistant::AssistantDirective),
        Box::new(statusline::StatuslineDirective),
        Box::new(github_app::GithubAppDirective),
        Box::new(slack_app::SlackAppDirective),
        Box::new(reload_plugins::ReloadPluginsDirective),
        // Internal/debug commands
        Box::new(thinkback::ThinkbackDirective),
        Box::new(thinkback_play::ThinkbackPlayDirective),
        Box::new(heapdump::HeapdumpDirective),
        Box::new(teleport::TeleportDirective),
        Box::new(pr_comments::PrCommentsDirective),
        // Init commands
        Box::new(init::InitDirective),
        Box::new(init_verifiers::InitVerifiersDirective),
    ]
}

/// Find a directive by name or alias.
pub fn find_directive<'a>(
    directives: &'a [BoxedDirective],
    name: &str,
) -> Option<&'a dyn Directive> {
    directives.iter().find_map(|d| {
        if d.name() == name || d.aliases().contains(&name) {
            Some(d.as_ref())
        } else {
            None
        }
    })
}

/// Get all enabled directives for the given context.
pub fn enabled_directives<'a>(
    directives: &'a [BoxedDirective],
    ctx: &CommandContext,
) -> Vec<&'a dyn Directive> {
    directives
        .iter()
        .filter(|d| d.is_enabled(ctx))
        .map(|d| d.as_ref())
        .collect()
}

/// Get all visible (non-hidden, enabled) directives for help display.
pub fn visible_directives<'a>(
    directives: &'a [BoxedDirective],
    ctx: &CommandContext,
) -> Vec<&'a dyn Directive> {
    directives
        .iter()
        .filter(|d| d.is_enabled(ctx) && !d.is_hidden())
        .map(|d| d.as_ref())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    fn internal_test_context() -> CommandContext {
        let mut ctx = test_context();
        ctx.user_type = Some("internal".to_string());
        ctx
    }

    fn help_contexts() -> [(&'static str, CommandContext); 2] {
        [
            ("standard", test_context()),
            ("internal", internal_test_context()),
        ]
    }

    fn visible_command_names(ctx: &CommandContext) -> Vec<String> {
        let directives = all_directives();
        visible_directives(&directives, ctx)
            .into_iter()
            .map(|directive| directive.name().to_string())
            .collect()
    }

    #[test]
    fn personal_help_excludes_hosted_remote_and_team_only_surfaces() {
        let ctx = test_context();
        let directives = all_directives();
        let visible = visible_directives(&directives, &ctx);
        let names: Vec<&str> = visible.iter().map(|directive| directive.name()).collect();
        for hidden in [
            "session",
            "mobile",
            "chrome",
            "remote-env",
            "remote-setup",
            "install-slack-app",
            "install-github-app",
            "passes",
            "rate-limit-options",
            "stickers",
            "branch",
            "install",
            "keybindings",
            "privacy-settings",
            "feedback",
            "desktop",
            "logout",
        ] {
            assert!(
                !names.contains(&hidden),
                "personal /help should not expose /{hidden}"
            );
        }
        for directive in visible {
            assert!(
                !directive.aliases().contains(&"team"),
                "personal /help should not expose /team alias"
            );
        }
    }

    #[test]
    fn hosted_opt_in_exposes_platform_commands_without_custom_provider_coupling() {
        let mut ctx = test_context();
        ctx.env_vars
            .insert("MOSSEN_ENABLE_HOSTED_COMMANDS".to_string(), "1".to_string());
        let names = visible_command_names(&ctx);
        for visible in [
            "mobile",
            "chrome",
            "remote-setup",
            "install-slack-app",
            "install-github-app",
            "passes",
            "rate-limit-options",
        ] {
            assert!(
                names.iter().any(|name| name == visible),
                "hosted opt-in should expose /{visible}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn agents_spawn_wires_to_background_agent_task() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::{Mutex, OnceLock};
        use std::time::Duration;

        struct EnvTaskStoreRestore {
            bin: Option<String>,
            depth: Option<String>,
        }
        impl Drop for EnvTaskStoreRestore {
            fn drop(&mut self) {
                if let Some(value) = self.bin.take() {
                    std::env::set_var("MOSSEN_AGENT_SUBPROCESS_BIN", value);
                } else {
                    std::env::remove_var("MOSSEN_AGENT_SUBPROCESS_BIN");
                }
                if let Some(value) = self.depth.take() {
                    std::env::set_var("MOSSEN_AGENT_SUBPROCESS_DEPTH", value);
                } else {
                    std::env::remove_var("MOSSEN_AGENT_SUBPROCESS_DEPTH");
                }
                mossen_tools::task_store::clear();
            }
        }

        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _lock = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _restore = EnvTaskStoreRestore {
            bin: std::env::var("MOSSEN_AGENT_SUBPROCESS_BIN").ok(),
            depth: std::env::var("MOSSEN_AGENT_SUBPROCESS_DEPTH").ok(),
        };
        mossen_tools::task_store::clear();

        let temp = tempfile::tempdir().expect("tempdir");
        let fake_bin = temp.path().join("fake-mossen-subagent");
        std::fs::write(
            &fake_bin,
            "#!/bin/sh\nprintf 'agent-command-output\\n'\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done\n",
        )
        .expect("write fake agent");
        let mut perms = std::fs::metadata(&fake_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_bin, perms).unwrap();
        std::env::set_var("MOSSEN_AGENT_SUBPROCESS_BIN", &fake_bin);
        std::env::remove_var("MOSSEN_AGENT_SUBPROCESS_DEPTH");

        let ctx = test_context();
        let result = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let result = delegates::DelegatesDirective
                    .execute(&["spawn", "find", "agent-smoke-marker"], &ctx)
                    .await
                    .expect("spawn agent");
                let CommandResult::System(text) = result else {
                    panic!("expected system launch result");
                };
                let task_id = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Task: "))
                    .expect("task id in result")
                    .to_string();
                for _ in 0..250 {
                    if let Some(task) = mossen_tools::task_store::get_task(&task_id) {
                        if mossen_tools::task_store::is_task_ready_status(&task.status) {
                            return task;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                panic!("agent task did not finish");
            });

        assert_eq!(result.status, "completed");
        assert!(result.output.contains("agent-smoke-marker"));
    }

    #[test]
    fn help_for_specific_command_uses_registered_metadata() {
        let ctx = test_context();
        let result = tokio_test::block_on(guide::GuideDirective.execute(&["model"], &ctx))
            .expect("help command should execute");
        let CommandResult::Text(text) = result else {
            panic!("expected text help");
        };

        assert!(text.contains("Help for /model"));
        assert!(text.contains("Usage: /model [profile-name]"));
        assert!(
            text.contains("Description: List configured model profiles or switch session profile")
        );
        assert!(!text.contains("registry is fully connected"));
    }

    #[test]
    fn representative_index_directives_execute_real_commands() {
        let ctx = test_context();
        let cases: Vec<(&str, BoxedDirective, Vec<&str>)> = vec![
            ("model", Box::new(model_index::ModelIndexDirective), vec![]),
            (
                "config",
                Box::new(config_index::ConfigIndexDirective),
                vec!["list"],
            ),
            ("mcp", Box::new(mcp_index::McpDirective), vec!["help"]),
            ("clear", Box::new(clear_index::ClearIndexDirective), vec![]),
            (
                "compact",
                Box::new(compact_index::CompactIndexDirective),
                vec![],
            ),
        ];

        for (name, directive, args) in cases {
            let result = tokio_test::block_on(directive.execute(&args, &ctx))
                .unwrap_or_else(|error| panic!("/{name} failed: {error}"));
            assert!(
                !matches!(result, CommandResult::Empty),
                "/{name} index directive returned Empty"
            );
        }
    }

    #[test]
    fn help_resolves_every_visible_command_and_alias() {
        for (ctx_name, ctx) in help_contexts() {
            let directives = all_directives();
            let visible = visible_directives(&directives, &ctx);
            assert!(
                visible.len() > 50,
                "{ctx_name}: expected a populated help registry"
            );

            for directive in visible {
                let names =
                    std::iter::once(directive.name()).chain(directive.aliases().iter().copied());
                for name in names {
                    let result = tokio_test::block_on(guide::GuideDirective.execute(&[name], &ctx))
                        .unwrap_or_else(|error| panic!("{ctx_name}: /help {name} failed: {error}"));
                    let CommandResult::Text(text) = result else {
                        panic!("{ctx_name}: /help {name} returned a non-text result");
                    };
                    assert!(
                        text.contains(&format!("Help for /{}", directive.name())),
                        "{ctx_name}: /help {name} did not render registered metadata for /{}",
                        directive.name()
                    );
                }
            }
        }
    }

    #[test]
    fn help_rejects_hidden_or_disabled_commands_by_name() {
        let ctx = test_context();
        for hidden in [
            "remote-env",
            "session",
            "install",
            "keybindings",
            "passes",
            "mobile",
        ] {
            let result = tokio_test::block_on(guide::GuideDirective.execute(&[hidden], &ctx))
                .unwrap_or_else(|error| panic!("/help {hidden} failed: {error}"));
            let CommandResult::Error(text) = result else {
                panic!("/help {hidden} should return an error in personal default context");
            };
            assert!(
                text.contains("Unknown command"),
                "/help {hidden} exposed hidden command help: {text}"
            );
        }
    }

    #[test]
    fn help_visible_directives_have_usable_safe_entrypoints() {
        let mut failures = Vec::new();
        for (ctx_name, ctx) in help_contexts() {
            let directives = all_directives();
            let visible = visible_directives(&directives, &ctx);
            assert!(
                visible.len() > 50,
                "{ctx_name}: expected a populated help registry"
            );

            for directive in visible {
                let name = directive.name();
                let args = smoke_args(name);
                let result =
                    tokio_test::block_on(directive.execute(args, &ctx)).unwrap_or_else(|error| {
                        panic!("{ctx_name}: /{name} failed to execute: {error}")
                    });

                match result {
                    CommandResult::Text(text) | CommandResult::System(text) => {
                        validate_smoke_output(ctx_name, name, &text, &mut failures);
                    }
                    CommandResult::Error(text) => {
                        failures.push(format!(
                            "{ctx_name}: /{name} returned Error for safe args {:?}: {}",
                            args,
                            first_line(&text)
                        ));
                    }
                    CommandResult::Empty => {
                        failures.push(format!(
                            "{ctx_name}: /{name} returned Empty for safe args {:?}",
                            args
                        ));
                    }
                    CommandResult::Widget => {
                        failures.push(format!(
                            "{ctx_name}: /{name} returned Widget for safe args {:?}",
                            args
                        ));
                    }
                    CommandResult::Exit(message) => {
                        if let Some(message) = message {
                            validate_smoke_output(ctx_name, name, &message, &mut failures);
                        }
                    }
                }
            }
        }

        assert!(
            failures.is_empty(),
            "visible /help commands must have usable safe entrypoints:\n{}",
            failures.join("\n")
        );
    }

    #[derive(Serialize)]
    struct CommandMatrix {
        total: usize,
        by_category_counts: HashMap<String, usize>,
        by_category_names: HashMap<String, Vec<String>>,
        entries: Vec<CommandMatrixEntry>,
    }

    #[derive(Serialize)]
    struct CommandMatrixEntry {
        command: String,
        description: String,
        aliases: Vec<String>,
        is_hidden: bool,
        is_enabled: bool,
        visible: bool,
        argument_hint: String,
        directive_type: &'static str,
        is_immediate: bool,
        supports_non_interactive: bool,
        category: &'static str,
        side_effect: &'static str,
        test_mode: &'static str,
        expected: &'static str,
        script: Option<&'static str>,
    }

    #[test]
    fn command_inventory_matrix_covers_current_personal_registry() {
        let ctx = test_context();
        let matrix = command_inventory_matrix(&ctx);

        assert!(
            matrix.total >= 50,
            "expected a populated current Rust command registry"
        );
        assert!(
            matrix
                .by_category_counts
                .get("no_side_effect")
                .copied()
                .unwrap_or(0)
                >= 5,
            "expected read-only commands in matrix"
        );

        let names: std::collections::HashSet<&str> = matrix
            .entries
            .iter()
            .map(|entry| entry.command.as_str())
            .collect();
        for must_have in [
            "help",
            "clear",
            "compact",
            "context",
            "model",
            "mcp",
            "memory",
            "status",
            "permissions",
            "skills",
            "plugin",
            "lang",
            "resume",
            "agents",
        ] {
            assert!(
                names.contains(must_have),
                "current Rust slash registry is missing /{must_have}"
            );
        }

        if let Ok(path) = std::env::var("MOSSEN_COMMAND_MATRIX_JSON") {
            let json = serde_json::to_string_pretty(&matrix).expect("serialize command matrix");
            fs::write(path, json).expect("write command matrix json");
        }
    }

    fn command_inventory_matrix(ctx: &CommandContext) -> CommandMatrix {
        let directives = all_directives();
        let mut entries = directives
            .iter()
            .map(|directive| command_matrix_entry(directive.as_ref(), ctx))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.command.cmp(&right.command));

        let mut by_category_names: HashMap<String, Vec<String>> = HashMap::new();
        for entry in &entries {
            by_category_names
                .entry(entry.category.to_string())
                .or_default()
                .push(entry.command.clone());
        }
        let by_category_counts = by_category_names
            .iter()
            .map(|(category, names)| (category.clone(), names.len()))
            .collect();

        CommandMatrix {
            total: entries.len(),
            by_category_counts,
            by_category_names,
            entries,
        }
    }

    fn command_matrix_entry(directive: &dyn Directive, ctx: &CommandContext) -> CommandMatrixEntry {
        let is_hidden = directive.is_hidden();
        let is_enabled = directive.is_enabled(ctx);
        let visible = is_enabled && !is_hidden;
        let (category, side_effect, test_mode) = categorize_for_matrix(directive.name(), visible);

        CommandMatrixEntry {
            command: directive.name().to_string(),
            description: directive.description().to_string(),
            aliases: directive
                .aliases()
                .iter()
                .map(|alias| alias.to_string())
                .collect(),
            is_hidden,
            is_enabled,
            visible,
            argument_hint: directive.argument_hint().to_string(),
            directive_type: directive_type_label_for_matrix(directive.directive_type()),
            is_immediate: directive.is_immediate(),
            supports_non_interactive: directive.supports_non_interactive(),
            category,
            side_effect,
            test_mode,
            expected: "see-category-rule",
            script: None,
        }
    }

    fn directive_type_label_for_matrix(directive_type: DirectiveType) -> &'static str {
        match directive_type {
            DirectiveType::Local => "local",
            DirectiveType::LocalWidget => "local_widget",
            DirectiveType::Prompt => "prompt",
        }
    }

    fn categorize_for_matrix(
        name: &str,
        visible_in_personal_default: bool,
    ) -> (&'static str, &'static str, &'static str) {
        if !visible_in_personal_default {
            return (
                "temporarily_unsupported",
                "hidden_or_disabled_in_personal_default",
                "hidden_or_opt_in",
            );
        }

        if matches!(
            name,
            "share"
                | "github-app"
                | "slack-app"
                | "desktop"
                | "mobile"
                | "chrome"
                | "remote-env"
                | "remote-setup"
                | "teleport"
                | "feedback"
                | "release-notes"
                | "pr-comments"
        ) {
            return (
                "external_service",
                "calls_external_api_or_platform_service",
                "mock_or_hidden",
            );
        }

        if matches!(
            name,
            "compact"
                | "clear"
                | "exit"
                | "rewind"
                | "commit"
                | "ship"
                | "review"
                | "security-review"
                | "branch"
                | "init"
                | "init-verifiers"
                | "doctor"
                | "heapdump"
        ) {
            return (
                "high_risk_tool",
                "modifies_session_repo_or_debug_state",
                "fixture_with_permission",
            );
        }

        if matches!(
            name,
            "config"
                | "model"
                | "profile"
                | "lang"
                | "permissions"
                | "mcp"
                | "memory"
                | "skills"
                | "plugin"
                | "statusline"
                | "hooks"
                | "vim"
                | "theme"
                | "color"
                | "output-style"
                | "keybindings"
                | "add-dir"
                | "rename"
                | "tag"
                | "reload-plugins"
                | "sandbox-toggle"
                | "onboarding"
                | "terminal-setup"
                | "effort"
        ) {
            return (
                "writes_config",
                "writes_user_project_or_session_config",
                "fixture_home",
            );
        }

        ("no_side_effect", "read_only_or_prompt_preview", "real_run")
    }

    fn smoke_args(name: &str) -> &'static [&'static str] {
        match name {
            "access" | "bridges" | "crafts" | "delegates" | "metrics" | "passes" | "plugin"
            | "privacy" | "rate_limit" | "sandbox" | "usage" => &["help"],
            "branch" => &["help"],
            "changes" => &["summary"],
            "config" => &["list"],
            "deauth" => &["status"],
            "heapdump" => &["help"],
            "ide" => &["status"],
            "install" => &["status"],
            "keybindings" => &["help"],
            "output_style" => &["list"],
            "pr_comments" => &["help"],
            "project" => &["info"],
            "remote_env" => &["help"],
            "remote_setup" => &["status"],
            "stickers" => &["help"],
            "turbo" => &["status"],
            "vim" => &["status"],
            _ => &[],
        }
    }

    fn validate_smoke_output(ctx_name: &str, name: &str, text: &str, failures: &mut Vec<String>) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            failures.push(format!("{ctx_name}: /{name} returned empty output"));
            return;
        }

        let lowered = trimmed.to_ascii_lowercase();
        let forbidden_terms = [
            "placeholder",
            "stub",
            "not implemented",
            "unimplemented",
            "not wired",
            "phase 5 tui",
            "phase 5 implementation",
            "hosted",
            "team memory",
            "direct-connect",
            "ssh remote",
            "remote attach",
            "hosted bridge",
            "hosted workflow",
        ];
        for term in forbidden_terms {
            if lowered.contains(term) {
                failures.push(format!(
                    "{ctx_name}: /{name} surfaced unfinished text `{term}`: {}",
                    first_line(trimmed)
                ));
            }
        }
    }

    fn first_line(text: &str) -> &str {
        text.lines().next().unwrap_or("").trim()
    }
}
