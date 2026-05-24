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
    unused_variables
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
pub use context::{BoxedDirective, CommandContext, CommandResult, Directive, DirectiveType};

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
    use std::collections::HashMap;
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
        }
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
        assert!(text.contains("Description: List model options or switch session model"));
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
}
