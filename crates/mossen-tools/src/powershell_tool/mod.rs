//! # powershell_tool — PowerShell tool helper modules
//!
//! Translates all files from `tools/PowerShellTool/`:
//! - PowerShellTool.tsx (core) → already in parent `power_shell.rs`
//! - toolName.ts → tool_name
//! - commonParameters.ts → common_parameters
//! - prompt.ts → prompt
//! - commandSemantics.ts → command_semantics
//! - destructiveCommandWarning.ts → destructive_command_warning
//! - gitSafety.ts → git_safety
//! - clmTypes.ts → clm_types
//! - modeValidation.ts → mode_validation
//! - pathValidation.ts → path_validation
//! - powershellPermissions.ts → permissions
//! - powershellSecurity.ts → security
//! - readOnlyValidation.ts → read_only_validation

pub mod clm_types;
pub mod command_semantics;
pub mod common_parameters;
pub mod destructive_command_warning;
pub mod git_safety;
pub mod mode_validation;
pub mod path_validation;
pub mod permissions;
pub mod prompt;
pub mod read_only_validation;
pub mod security;
pub mod tool_name;
pub mod ui_helpers;
pub use ui_helpers::{
    render_tool_result_message as render_ps_tool_result_message,
    render_tool_use_error_message as render_ps_tool_use_error_message,
    render_tool_use_message as render_ps_tool_use_message,
    render_tool_use_progress_message as render_ps_tool_use_progress_message,
    render_tool_use_queued_message as render_ps_tool_use_queued_message,
};

/// `PowerShellTool.tsx` `detectBlockedSleepPattern` — flag a PowerShell
/// `Start-Sleep -Seconds N` (N≥2) that isn't followed by anything else.
pub fn detect_blocked_sleep_pattern(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    let re = regex::Regex::new(r"start-sleep\s+(?:-seconds\s+)?(\d+)").unwrap();
    let caps = re.captures(&lower)?;
    let secs: u64 = caps.get(1)?.as_str().parse().ok()?;
    if secs < 2 {
        return None;
    }
    Some(format!("Start-Sleep {} seconds", secs))
}

/// `PowerShellTool.tsx` `PowerShellTool` — value-shape constant.
#[derive(Debug, Clone, Default)]
pub struct PowerShellTool;

impl PowerShellTool {
    pub const TOOL_NAME: &'static str = "PowerShell";
}

/// `PowerShellTool.tsx` `PowerShellToolInput`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PowerShellToolInput {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// `PowerShellTool.tsx` `Out`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Out {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}
