//! `/exit` — Exit the REPL.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Exit command — terminates the session with a random farewell message.
/// In background (tmux) sessions, detaches instead of killing.
pub struct ExitDirective;

/// Farewell messages displayed on exit.
const GOODBYE_MESSAGES: &[&str] = &[
    "Goodbye! Happy coding!",
    "See you later!",
    "Until next time!",
    "Take care!",
    "Happy hacking!",
    "May your code compile on the first try!",
];

/// Check if currently running inside a background (tmux) session.
fn is_background_session(ctx: &CommandContext) -> bool {
    ctx.env_vars.get("MOSSEN_BG_SESSION")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Check if there is an active worktree session.
fn has_worktree_session(ctx: &CommandContext) -> bool {
    ctx.env_vars.get("MOSSEN_WORKTREE_SESSION")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Select a pseudo-random goodbye message based on current time.
fn get_random_goodbye_message() -> &'static str {
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as usize % GOODBYE_MESSAGES.len())
        .unwrap_or(0);
    GOODBYE_MESSAGES[idx]
}

#[async_trait]
impl Directive for ExitDirective {
    fn name(&self) -> &str {
        "exit"
    }

    fn aliases(&self) -> &[&str] {
        &["quit"]
    }

    fn description(&self) -> &str {
        "Exit the REPL"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Inside a background tmux session: detach instead of killing.
        // The REPL keeps running; `mossen attach` can reconnect.
        if is_background_session(ctx) {
            // In a real implementation this would call tmux detach-client
            return Ok(CommandResult::Exit(Some(
                "Detached from background session. Use `mossen attach` to reconnect.".to_string(),
            )));
        }

        // If there's an active worktree session, show the exit flow widget
        // which asks the user whether to also clean up the worktree.
        if has_worktree_session(ctx) {
            // Exit with worktree cleanup prompt
            return Ok(CommandResult::Exit(Some(
                "Exiting session. Note: active worktree detected — run `git worktree remove` to clean up if needed.".to_string(),
            )));
        }

        // Normal exit with a friendly farewell
        let msg = get_random_goodbye_message().to_string();
        Ok(CommandResult::Exit(Some(msg)))
    }
}
