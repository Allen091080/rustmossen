//! `/goal` — set or view a persistent thread goal.

use anyhow::Result;
use async_trait::async_trait;

use crate::{CommandContext, CommandResult, Directive, DirectiveType};

pub struct GoalDirective;

#[async_trait]
impl Directive for GoalDirective {
    fn name(&self) -> &str {
        "goal"
    }

    fn description(&self) -> &str {
        "Set or view the goal for a long-running task"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[objective|edit|pause|resume|clear]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        execute_goal_command(args, ctx)
    }
}

pub fn execute_goal_command(args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
    let thread_id = mossen_agent::goal::thread_id_from_command_env(&ctx.env_vars);
    let store = mossen_agent::goal::GoalStore::default();

    let first = args.first().copied().unwrap_or("");
    match first {
        "" => match store.get(&thread_id)? {
            Some(goal) => Ok(CommandResult::Text(mossen_agent::goal::format_goal_summary(
                &goal,
            ))),
            None => Ok(CommandResult::Text(
                "No goal is set.\nUsage: /goal <objective>\nCommands: /goal edit <objective>, /goal pause, /goal resume, /goal clear"
                    .to_string(),
            )),
        },
        "clear" => {
            let removed = store.clear(&thread_id)?;
            if removed {
                Ok(CommandResult::Text("Goal cleared".to_string()))
            } else {
                Ok(CommandResult::Text("No goal is set".to_string()))
            }
        }
        "pause" => update_status(&store, &thread_id, mossen_agent::goal::ThreadGoalStatus::Paused),
        "resume" => update_status(&store, &thread_id, mossen_agent::goal::ThreadGoalStatus::Active),
        "edit" => {
            let objective = args.iter().skip(1).copied().collect::<Vec<_>>().join(" ");
            if objective.trim().is_empty() {
                return Ok(CommandResult::Error(
                    "Usage: /goal edit <objective>".to_string(),
                ));
            }
            let existing = store.get(&thread_id)?;
            let status = existing
                .as_ref()
                .map(|goal| mossen_agent::goal::edited_goal_status(goal.status))
                .unwrap_or(mossen_agent::goal::ThreadGoalStatus::Active);
            let token_budget = existing.and_then(|goal| goal.token_budget);
            let goal = store.set_or_replace(&thread_id, &objective, status, token_budget)?;
            Ok(CommandResult::Text(mossen_agent::goal::format_goal_summary(
                &goal,
            )))
        }
        _ => {
            let objective = args.join(" ");
            let goal = store.replace(&thread_id, &objective, None)?;
            Ok(CommandResult::Text(mossen_agent::goal::format_goal_summary(
                &goal,
            )))
        }
    }
}

fn update_status(
    store: &mossen_agent::goal::GoalStore,
    thread_id: &str,
    status: mossen_agent::goal::ThreadGoalStatus,
) -> Result<CommandResult> {
    let goal = store.update_status(thread_id, status)?;
    Ok(CommandResult::Text(
        mossen_agent::goal::format_goal_summary(&goal),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    struct EnvRestore {
        previous_config: Option<String>,
    }

    impl EnvRestore {
        fn new(config_dir: &str) -> Self {
            let previous_config = std::env::var("MOSSEN_CONFIG_DIR").ok();
            std::env::set_var("MOSSEN_CONFIG_DIR", config_dir);
            Self { previous_config }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(value) = self.previous_config.take() {
                std::env::set_var("MOSSEN_CONFIG_DIR", value);
            } else {
                std::env::remove_var("MOSSEN_CONFIG_DIR");
            }
        }
    }

    fn test_context(thread_id: &str) -> CommandContext {
        let mut env_vars = HashMap::new();
        env_vars.insert(
            mossen_agent::goal::GOAL_THREAD_ID_ENV.to_string(),
            thread_id.to_string(),
        );
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars,
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    #[tokio::test]
    async fn goal_directive_sets_views_pauses_resumes_and_clears_goal() {
        let _lock = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = EnvRestore::new(temp.path().to_string_lossy().as_ref());
        let ctx = test_context("command-thread");
        let directive = GoalDirective;

        let created = directive
            .execute(&["release", "v1"], &ctx)
            .await
            .expect("set goal");
        assert!(
            matches!(created, CommandResult::Text(ref text) if text.contains("Objective: release v1"))
        );

        let paused = directive.execute(&["pause"], &ctx).await.expect("pause");
        assert!(matches!(paused, CommandResult::Text(ref text) if text.contains("Status: paused")));

        let resumed = directive.execute(&["resume"], &ctx).await.expect("resume");
        assert!(
            matches!(resumed, CommandResult::Text(ref text) if text.contains("Status: active"))
        );

        let cleared = directive.execute(&["clear"], &ctx).await.expect("clear");
        assert!(matches!(cleared, CommandResult::Text(ref text) if text.contains("Goal cleared")));
    }
}
