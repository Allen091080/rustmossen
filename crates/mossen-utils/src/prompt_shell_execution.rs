//! Prompt shell execution — parse and execute embedded shell commands in prompt text.

use regex::Regex;

use once_cell::sync::Lazy;

/// Pattern for code blocks: ```! command ```
static BLOCK_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"```!\s*\n?([\s\S]*?)\n?```").unwrap());

/// Pattern for inline: !`command`
static INLINE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)(?:^|\s)!`([^`]+)`").unwrap());

/// Shell execution error types.
#[derive(Debug, thiserror::Error)]
pub enum ShellExecError {
    #[error("Shell command permission check failed for pattern \"{pattern}\": {message}")]
    PermissionDenied { pattern: String, message: String },
    #[error("Shell command interrupted for pattern \"{pattern}\": [Command interrupted]")]
    Interrupted { pattern: String },
    #[error("Shell command failed for pattern \"{pattern}\": {output}")]
    CommandFailed { pattern: String, output: String },
    #[error("{0}")]
    Malformed(String),
}

/// Shell output from a command execution.
#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub interrupted: bool,
}

/// Trait for shell command execution (allows mocking in tests).
#[async_trait::async_trait]
pub trait ShellExecutor: Send + Sync {
    async fn execute(&self, command: &str) -> Result<ShellOutput, ShellExecError>;
    async fn check_permission(&self, command: &str) -> Result<bool, String>;
}

/// Parse prompt text and execute any embedded shell commands.
///
/// Supports two syntaxes:
/// - Code blocks: ```! command ```
/// - Inline: !`command`
pub async fn execute_shell_commands_in_prompt(
    text: &str,
    executor: &dyn ShellExecutor,
    slash_command_name: &str,
) -> Result<String, ShellExecError> {
    let mut result = text.to_string();

    // Collect all matches (block and inline)
    let mut matches: Vec<(String, String)> = Vec::new(); // (full_match, command)

    for caps in BLOCK_PATTERN.captures_iter(text) {
        let full_match = caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
        let command = caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        if !command.is_empty() {
            matches.push((full_match, command));
        }
    }

    // Only scan for inline pattern if text contains !`
    if text.contains("!`") {
        for caps in INLINE_PATTERN.captures_iter(text) {
            let full_match = caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
            let command = caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            if !command.is_empty() {
                matches.push((full_match, command));
            }
        }
    }

    for (pattern, command) in matches {
        // Check permissions before executing
        match executor.check_permission(&command).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::debug!(
                    "Shell command permission check failed for command in {}: {}",
                    slash_command_name,
                    command
                );
                return Err(ShellExecError::PermissionDenied {
                    pattern: pattern.clone(),
                    message: "Permission denied".to_string(),
                });
            }
            Err(msg) => {
                return Err(ShellExecError::PermissionDenied {
                    pattern: pattern.clone(),
                    message: msg,
                });
            }
        }

        let shell_output = executor.execute(&command).await?;

        if shell_output.interrupted {
            return Err(ShellExecError::Interrupted {
                pattern: pattern.clone(),
            });
        }

        let output = format_bash_output(&shell_output.stdout, &shell_output.stderr, false);
        result = result.replacen(&pattern, &output, 1);
    }

    Ok(result)
}

/// Format shell output combining stdout and stderr.
fn format_bash_output(stdout: &str, stderr: &str, inline: bool) -> String {
    let mut parts = Vec::new();

    let stdout_trimmed = stdout.trim();
    if !stdout_trimmed.is_empty() {
        parts.push(stdout_trimmed.to_string());
    }

    let stderr_trimmed = stderr.trim();
    if !stderr_trimmed.is_empty() {
        if inline {
            parts.push(format!("[stderr: {}]", stderr_trimmed));
        } else {
            parts.push(format!("[stderr]\n{}", stderr_trimmed));
        }
    }

    if inline {
        parts.join(" ")
    } else {
        parts.join("\n")
    }
}
