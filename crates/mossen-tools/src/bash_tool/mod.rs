//! # bash_tool — ShellExecutor tool (complete translation of BashTool/)
//!
//! Translates all 18 TS files from `tools/BashTool/`:
//! - BashTool.tsx (1143 lines) → mod.rs (this file)
//! - bashCommandHelpers.ts → bash_command_helpers.rs
//! - bashPermissions.ts → bash_permissions.rs
//! - bashSecurity.ts → bash_security.rs
//! - commandSemantics.ts → command_semantics.rs
//! - commentLabel.ts → comment_label.rs
//! - destructiveCommandWarning.ts → destructive_command_warning.rs
//! - modeValidation.ts → mode_validation.rs
//! - pathValidation.ts → path_validation.rs
//! - prompt.ts → prompt.rs
//! - readOnlyValidation.ts → read_only_validation.rs
//! - sedEditParser.ts → sed_edit_parser.rs
//! - sedValidation.ts → sed_validation.rs
//! - shouldUseSandbox.ts → should_use_sandbox.rs
//! - toolName.ts → tool_name.rs
//! - utils.ts → utils.rs
//! - UI.tsx / BashToolResultMessage.tsx → (display logic in struct methods)

pub mod bash_command_helpers;
pub mod bash_permissions;
pub mod bash_security;
pub mod command_semantics;
pub mod comment_label;
pub mod destructive_command_warning;
pub mod mode_validation;
pub mod path_validation;
pub mod prompt;
pub mod read_only_validation;
pub mod sed_edit_parser;
pub mod sed_validation;
pub mod should_use_sandbox;
pub mod tool_name;
pub mod ui_helpers;
pub mod utils;

/// `BashTool.tsx` `detectBlockedSleepPattern` — return a description string
/// when the command is `sleep N` (N≥2 seconds) optionally followed by an
/// additional command. `None` means the command is fine.
pub fn detect_blocked_sleep_pattern(command: &str) -> Option<String> {
    let parts = split_command_with_operators(command);
    if parts.is_empty() {
        return None;
    }
    let first = parts[0].trim();
    let re = regex::Regex::new(r"^sleep\s+(\d+)\s*$").unwrap();
    let caps = re.captures(first)?;
    let secs: u64 = caps.get(1)?.as_str().parse().ok()?;
    if secs < 2 {
        return None;
    }
    let rest = parts[1..].join(" ").trim().to_string();
    if rest.is_empty() {
        Some(format!("standalone sleep {}", secs))
    } else {
        Some(format!("sleep {} followed by: {}", secs, rest))
    }
}

/// `BashTool.tsx` `BashTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct BashTool;

impl BashTool {
    pub const TOOL_NAME: &'static str = "Bash";
}

/// `BashTool.tsx` `BashToolInput` alias.
pub type BashToolInput = ShellExecutorInput;

/// `BashTool.tsx` `Out` alias.
pub type Out = ShellExecutorOutput;
pub use ui_helpers::{
    background_hint, render_tool_result_message as render_bash_tool_result_message,
    render_tool_use_error_message as render_bash_tool_use_error_message,
    render_tool_use_message as render_bash_tool_use_message,
    render_tool_use_progress_message as render_bash_tool_use_progress_message,
    render_tool_use_queued_message as render_bash_tool_use_queued_message,
};

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tracing::{info, warn};

use mossen_agent::tool_registry::{Tool, ToolResult, ToolType};
use mossen_types::{ToolDefinition, ToolInputSchema, ToolUseContext};

use self::command_semantics::interpret_command_result;
use self::comment_label::extract_bash_comment_label;
use self::destructive_command_warning::get_destructive_command_warning;
use self::prompt::{get_default_timeout_ms, get_max_timeout_ms};
use self::read_only_validation::{check_read_only_constraints, ReadOnlyResult};
use self::tool_name::BASH_TOOL_NAME;
use self::utils::{format_output, strip_empty_lines};

/// Progress display threshold (ms).
const PROGRESS_THRESHOLD_MS: u64 = 2000;
/// In assistant mode, blocking bash auto-backgrounds after this many ms.
const ASSISTANT_BLOCKING_BUDGET_MS: u64 = 15_000;

/// Search commands for collapsible display.
fn bash_search_commands() -> HashSet<&'static str> {
    [
        "find", "grep", "rg", "ag", "ack", "locate", "which", "whereis",
    ]
    .iter()
    .copied()
    .collect()
}

/// Read/view commands for collapsible display.
fn bash_read_commands() -> HashSet<&'static str> {
    [
        "cat", "head", "tail", "less", "more", "wc", "stat", "file", "strings", "jq", "awk", "cut",
        "sort", "uniq", "tr",
    ]
    .iter()
    .copied()
    .collect()
}

/// Directory-listing commands for collapsible display.
fn bash_list_commands() -> HashSet<&'static str> {
    ["ls", "tree", "du"].iter().copied().collect()
}

/// Commands that are semantic-neutral (pure output/status).
fn bash_semantic_neutral_commands() -> HashSet<&'static str> {
    ["echo", "printf", "true", "false", ":"]
        .iter()
        .copied()
        .collect()
}

/// Commands that typically produce no stdout on success.
fn bash_silent_commands() -> HashSet<&'static str> {
    [
        "mv", "cp", "rm", "mkdir", "rmdir", "chmod", "chown", "chgrp", "touch", "ln", "cd",
        "export", "unset", "wait",
    ]
    .iter()
    .copied()
    .collect()
}

/// Checks if a bash command is a search or read operation.
/// Used to determine if the command should be collapsed in the UI.
pub fn is_search_or_read_bash_command(command: &str) -> SearchReadResult {
    let parts = split_command_with_operators(command);
    let search_cmds = bash_search_commands();
    let read_cmds = bash_read_commands();
    let list_cmds = bash_list_commands();
    let neutral_cmds = bash_semantic_neutral_commands();

    let mut is_search = true;
    let mut is_read = true;
    let mut is_list = true;

    for part in &parts {
        let base = part.split_whitespace().next().unwrap_or("");
        if neutral_cmds.contains(base) {
            continue;
        }
        if !search_cmds.contains(base) {
            is_search = false;
        }
        if !read_cmds.contains(base) {
            is_read = false;
        }
        if !list_cmds.contains(base) {
            is_list = false;
        }
    }

    SearchReadResult {
        is_search,
        is_read,
        is_list,
    }
}

/// Result of search/read classification.
#[derive(Debug, Clone)]
pub struct SearchReadResult {
    pub is_search: bool,
    pub is_read: bool,
    pub is_list: bool,
}

/// Split command with operators (simplified pipe/operator split).
fn split_command_with_operators(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    for c in command.chars() {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }
        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            continue;
        }
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            continue;
        }
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            continue;
        }
        if !in_single_quote && !in_double_quote && (c == '|' || c == ';') {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current.clear();
            continue;
        }
        current.push(c);
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Shell Executor — the main tool struct.
pub struct ShellExecutor;

/// Tool input.
#[derive(Debug, Clone, Deserialize)]
pub struct ShellExecutorInput {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub run_in_background: bool,
    #[serde(default)]
    pub dangerously_disable_sandbox: bool,
}

fn default_timeout() -> u64 {
    get_default_timeout_ms()
}

/// Tool output.
#[derive(Debug, Clone, Serialize)]
pub struct ShellExecutorOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub interrupted: bool,
}

fn build_input_schema() -> ToolInputSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "The shell command to execute. Can be multiple commands separated by && or ;."
        }),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "A clear, concise description of what this command does (2-10 words, no technical jargon)."
        }),
    );
    properties.insert(
        "timeout".to_string(),
        serde_json::json!({
            "type": "number",
            "description": format!("Optional timeout in milliseconds (max {}ms). Default: {}ms.", get_max_timeout_ms(), get_default_timeout_ms()),
            "default": get_default_timeout_ms()
        }),
    );
    properties.insert(
        "run_in_background".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Set to true to run this command in the background. You'll be notified when it completes.",
            "default": false
        }),
    );
    properties.insert(
        "dangerouslyDisableSandbox".to_string(),
        serde_json::json!({
            "type": "boolean",
            "description": "Set to true to run this command outside the sandbox.",
            "default": false
        }),
    );

    ToolInputSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: Some(vec!["command".to_string()]),
        extra: HashMap::new(),
    }
}

#[async_trait]
impl Tool for ShellExecutor {
    fn name(&self) -> &str {
        BASH_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Execute a shell command"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: build_input_schema(),
            cache_control: None,
        }
    }

    fn tool_type(&self) -> ToolType {
        ToolType::Builtin
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, context: &ToolUseContext) -> anyhow::Result<ToolResult> {
        let inp: ShellExecutorInput = serde_json::from_value(input)?;
        let start = std::time::Instant::now();

        let timeout_ms = inp.timeout.min(get_max_timeout_ms());
        let duration = std::time::Duration::from_millis(timeout_ms);

        info!(
            command = %inp.command,
            timeout_ms = timeout_ms,
            background = inp.run_in_background,
            "ShellExecutor: running command"
        );

        // Background mode: spawn and return immediately
        if inp.run_in_background {
            let cmd = inp.command.clone();
            let cwd = context.cwd.clone();
            tokio::spawn(async move {
                let _ = Command::new("bash")
                    .arg("-c")
                    .arg(&cmd)
                    .current_dir(&cwd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn();
            });

            let output = ShellExecutorOutput {
                stdout: Some("Command started in background.".to_string()),
                stderr: None,
                exit_code: None,
                timed_out: false,
                interrupted: false,
            };
            return Ok(ToolResult {
                output: serde_json::to_string(&output)?,
                is_error: false,
                duration_ms: start.elapsed().as_millis() as u64,
                metadata: HashMap::new(),
            });
        }

        // Foreground mode: execute with timeout
        let child = Command::new("bash")
            .arg("-c")
            .arg(&inp.command)
            .current_dir(&context.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let result = tokio::select! {
            res = child.wait_with_output() => {
                match res {
                    Ok(o) => {
                        let stdout_raw = String::from_utf8_lossy(&o.stdout).to_string();
                        let stderr_raw = String::from_utf8_lossy(&o.stderr).to_string();
                        let stdout = strip_empty_lines(&stdout_raw);
                        let stderr = strip_empty_lines(&stderr_raw);

                        ShellExecutorOutput {
                            stdout: if stdout.is_empty() { None } else { Some(stdout) },
                            stderr: if stderr.is_empty() { None } else { Some(stderr) },
                            exit_code: o.status.code(),
                            timed_out: false,
                            interrupted: false,
                        }
                    }
                    Err(e) => ShellExecutorOutput {
                        stdout: None,
                        stderr: Some(format!("Failed to execute command: {}", e)),
                        exit_code: None,
                        timed_out: false,
                        interrupted: false,
                    },
                }
            }
            _ = tokio::time::sleep(duration) => {
                warn!(command = %inp.command, "ShellExecutor: command timed out");
                ShellExecutorOutput {
                    stdout: None,
                    stderr: Some("Command timed out".to_string()),
                    exit_code: None,
                    timed_out: true,
                    interrupted: false,
                }
            }
        };

        let elapsed = start.elapsed().as_millis() as u64;

        // Use semantic exit code interpretation
        let is_error = if let Some(code) = result.exit_code {
            let interpretation = interpret_command_result(
                &inp.command,
                code,
                result.stdout.as_deref().unwrap_or(""),
                result.stderr.as_deref().unwrap_or(""),
            );
            interpretation.is_error
        } else {
            result.timed_out
        };

        Ok(ToolResult {
            output: serde_json::to_string(&result)?,
            is_error,
            duration_ms: elapsed,
            metadata: HashMap::new(),
        })
    }
}
