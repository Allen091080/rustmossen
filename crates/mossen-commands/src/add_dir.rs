//! `/add-dir` — Add a new working directory to the session.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Add directory command — adds a path to the session's working directories,
/// making files in that directory accessible to tools.
pub struct AddDirDirective;

/// Validate that a path exists and is a directory.
fn validate_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }
    Ok(())
}

/// Resolve a path argument relative to the cwd.
fn resolve_path(path_str: &str, cwd: &Path) -> std::path::PathBuf {
    let path = Path::new(path_str);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path_str)
    }
}

#[async_trait]
impl Directive for AddDirDirective {
    fn name(&self) -> &str {
        "add-dir"
    }

    fn description(&self) -> &str {
        "Add a new working directory"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "<path>"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            return Ok(CommandResult::Text(
                "Usage: /add-dir <path>\n\nAdd a directory to the session's working directories."
                    .to_string(),
            ));
        }

        let path_str = args.join(" ");
        let resolved = resolve_path(path_str.trim(), &ctx.cwd);

        // Validate the directory exists
        if let Err(e) = validate_directory(&resolved) {
            return Ok(CommandResult::Error(e));
        }

        Ok(CommandResult::Error(format!(
            "Cannot add working directory from this command runner. Use /add-dir {} in the interactive TUI so the live tool context is updated.",
            resolved.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
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
    fn add_dir_directive_does_not_claim_live_tool_context_update() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = tokio_test::block_on(AddDirDirective.execute(
            &[dir.path().to_string_lossy().as_ref()],
            &test_context(dir.path().to_path_buf()),
        ))
        .expect("add-dir command");

        let CommandResult::Error(text) = output else {
            panic!("add-dir should not claim success outside TUI state");
        };
        assert!(text.contains("Cannot add working directory"), "{text}");
        assert!(!text.contains("Added working directory"), "{text}");
    }
}
