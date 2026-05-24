//! # command_semantics — Exit code interpretation for PowerShell
//!
//! Translates `tools/PowerShellTool/commandSemantics.ts`.
//! Provides exit code semantic rules for external executables invoked from PowerShell.

/// Result of interpreting a command's exit code.
#[derive(Debug, Clone)]
pub struct CommandInterpretation {
    pub is_error: bool,
    pub message: Option<String>,
}

/// Interpret the result of a command based on its exit code and semantic rules.
pub fn interpret_command_result(
    command: &str,
    exit_code: i32,
    _stdout: &str,
    _stderr: &str,
) -> CommandInterpretation {
    let base_command = heuristically_extract_base_command(command);

    match base_command.as_str() {
        // grep / ripgrep: 0 = matches found, 1 = no matches, 2+ = error
        "grep" | "rg" | "findstr" => CommandInterpretation {
            is_error: exit_code >= 2,
            message: if exit_code == 1 {
                Some("No matches found".to_string())
            } else {
                None
            },
        },

        // robocopy: 0-7 = success, 8+ = error
        "robocopy" => CommandInterpretation {
            is_error: exit_code >= 8,
            message: if exit_code == 0 {
                Some("No files copied (already in sync)".to_string())
            } else if exit_code >= 1 && exit_code < 8 {
                if exit_code & 1 != 0 {
                    Some("Files copied successfully".to_string())
                } else {
                    Some("Robocopy completed (no errors)".to_string())
                }
            } else {
                None
            },
        },

        // Default: treat only 0 as success
        _ => CommandInterpretation {
            is_error: exit_code != 0,
            message: if exit_code != 0 {
                Some(format!("Command failed with exit code {}", exit_code))
            } else {
                None
            },
        },
    }
}

/// Extract the base command name from a PowerShell command line.
/// Takes the LAST pipeline segment since that determines the exit code.
fn heuristically_extract_base_command(command: &str) -> String {
    let segments: Vec<&str> = command
        .split([';', '|'])
        .filter(|s| !s.trim().is_empty())
        .collect();
    let last = segments.last().copied().unwrap_or(command);
    extract_base_command(last)
}

/// Extract the command name from a single pipeline segment.
/// Strips leading `&` / `.` call operators and `.exe` suffix, lowercases.
fn extract_base_command(segment: &str) -> String {
    let trimmed = segment.trim();

    // Strip PowerShell call operators: & "cmd", . "cmd"
    let stripped = if trimmed.starts_with("& ") || trimmed.starts_with(". ") {
        &trimmed[2..]
    } else {
        trimmed
    };

    let first_token = stripped.split_whitespace().next().unwrap_or("");

    // Strip surrounding quotes
    let unquoted = first_token.trim_matches(|c| c == '"' || c == '\'');

    // Strip path: C:\bin\grep.exe → grep.exe
    let basename = unquoted.rsplit(['/', '\\']).next().unwrap_or(unquoted);

    // Strip .exe suffix and lowercase
    let lower = basename.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}
