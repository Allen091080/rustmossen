//! `/project` — Manage project settings and directories.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Project directive — manages project-level configuration, displays project status,
/// lists projects, or purges project data.
pub struct ProjectDirective;

/// Subcommands for the project command.
enum ProjectAction {
    /// Show project status (default)
    Status,
    /// List all known projects
    List,
    /// Purge project data
    Purge(Option<String>),
    /// Show project info
    Info,
}

/// Parse the project subcommand.
fn parse_project_action(args: &[&str]) -> ProjectAction {
    if args.is_empty() {
        return ProjectAction::Status;
    }

    match args[0].to_lowercase().as_str() {
        "list" | "ls" => ProjectAction::List,
        "purge" | "delete" => {
            let target = if args.len() > 1 {
                Some(args[1..].join(" "))
            } else {
                None
            };
            ProjectAction::Purge(target)
        }
        "info" | "show" => ProjectAction::Info,
        _ => ProjectAction::Status,
    }
}

#[async_trait]
impl Directive for ProjectDirective {
    fn name(&self) -> &str {
        "project"
    }

    fn description(&self) -> &str {
        "Manage project settings and directories"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[status|list|purge|info]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let action = parse_project_action(args);

        match action {
            ProjectAction::Status => {
                // Show current project status
                let project_dir = ctx.cwd.join(".mossen");
                let has_project = project_dir.exists();
                if has_project {
                    Ok(CommandResult::Text(format!(
                        "Project: {}\nConfig dir: {}\nStatus: active",
                        ctx.cwd.display(),
                        project_dir.display()
                    )))
                } else {
                    Ok(CommandResult::Text(format!(
                        "No project configuration found in {}\nRun commands to auto-initialize.",
                        ctx.cwd.display()
                    )))
                }
            }
            ProjectAction::List => {
                // List known project directories
                Ok(CommandResult::Text(
                    "Known Projects\n\
                     ==============\n\n\
                     No projects registered yet.\n\
                     Navigate to a project directory and use /project init to register it."
                        .to_string(),
                ))
            }
            ProjectAction::Purge(target) => {
                let target = target.unwrap_or_else(|| ctx.cwd.display().to_string());
                Ok(CommandResult::Error(format!(
                    "Cannot purge project data from this command runner. Requested target: {}. Use an explicit filesystem command after reviewing the files to delete.",
                    target
                )))
            }
            ProjectAction::Info => Ok(CommandResult::Text(format!(
                "Project root: {}\nProduct: {}\nVersion: {}",
                ctx.cwd.display(),
                ctx.product_name,
                ctx.version
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("/tmp/example-project"),
            is_non_interactive: true,
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

    #[test]
    fn project_purge_does_not_claim_deletion_when_unwired() {
        let output = tokio_test::block_on(ProjectDirective.execute(&["purge"], &test_context()))
            .expect("project command");

        let CommandResult::Error(text) = output else {
            panic!("project purge should fail closed");
        };
        assert!(text.contains("Cannot purge project data"), "{text}");
        assert!(!text.contains("Purging project data"), "{text}");
    }
}
