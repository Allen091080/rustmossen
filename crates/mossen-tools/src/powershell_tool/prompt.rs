//! # prompt — PowerShell tool prompt generation
//!
//! Translates `tools/PowerShellTool/prompt.ts`.

use std::env;

use super::tool_name::POWERSHELL_TOOL_NAME;

const FILE_EDIT_TOOL_NAME: &str = "Edit";
const FILE_READ_TOOL_NAME: &str = "Read";
const FILE_WRITE_TOOL_NAME: &str = "Write";
const GLOB_TOOL_NAME: &str = "Glob";
const GREP_TOOL_NAME: &str = "Grep";

/// PowerShell edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerShellEdition {
    Desktop, // Windows PowerShell 5.1
    Core,    // PowerShell 7+
}

/// Default timeout in milliseconds.
pub fn get_default_timeout_ms() -> u64 {
    120_000
}

/// Maximum timeout in milliseconds.
pub fn get_max_timeout_ms() -> u64 {
    600_000
}

/// Maximum output length in characters.
fn get_max_output_length() -> usize {
    100_000
}

fn is_env_truthy(key: &str) -> bool {
    env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn get_background_usage_note() -> Option<String> {
    if is_env_truthy("MOSSEN_CODE_DISABLE_BACKGROUND_TASKS") {
        return None;
    }
    Some(
        "  - You can use the `run_in_background` parameter to run the command in the background. \
         Only use this if you don't need the result immediately and are OK being notified when \
         the command completes later."
            .to_string(),
    )
}

fn get_sleep_guidance() -> Option<String> {
    if is_env_truthy("MOSSEN_CODE_DISABLE_BACKGROUND_TASKS") {
        return None;
    }
    Some(
        "  - Avoid unnecessary `Start-Sleep` commands:\n\
         \x20   - Do not sleep between commands that can run immediately — just run them.\n\
         \x20   - If your command is long running, use `run_in_background`.\n\
         \x20   - Do not retry failing commands in a sleep loop — diagnose the root cause.\n\
         \x20   - If waiting for a background task, you will be notified — do not poll.\n\
         \x20   - If you must sleep, keep the duration short (1-5 seconds)."
            .to_string(),
    )
}

/// Get edition-specific syntax guidance.
fn get_edition_section(edition: Option<PowerShellEdition>) -> String {
    match edition {
        Some(PowerShellEdition::Desktop) => {
            "PowerShell edition: Windows PowerShell 5.1 (powershell.exe)\n\
             \x20  - Pipeline chain operators `&&` and `||` are NOT available. \
               To run B only if A succeeds: `A; if ($?) { B }`.\n\
             \x20  - Ternary (`?:`), null-coalescing (`??`), and null-conditional (`?.`) are NOT available.\n\
             \x20  - Avoid `2>&1` on native executables (wraps stderr in ErrorRecord).\n\
             \x20  - Default file encoding is UTF-16 LE. Use `-Encoding utf8` for interop.\n\
             \x20  - `ConvertFrom-Json` returns PSCustomObject, not hashtable."
                .to_string()
        }
        Some(PowerShellEdition::Core) => {
            "PowerShell edition: PowerShell 7+ (pwsh)\n\
             \x20  - Pipeline chain operators `&&` and `||` ARE available.\n\
             \x20  - Ternary, null-coalescing (`??`), and null-conditional (`?.`) are available.\n\
             \x20  - Default file encoding is UTF-8 without BOM."
                .to_string()
        }
        None => {
            "PowerShell edition: unknown — assume Windows PowerShell 5.1 for compatibility\n\
             \x20  - Do NOT use `&&`, `||`, ternary `?:`, null-coalescing `??`, or null-conditional `?.`.\n\
             \x20  - To chain commands conditionally: `A; if ($?) { B }`. Unconditionally: `A; B`."
                .to_string()
        }
    }
}

/// Generate the full PowerShell tool prompt.
pub fn get_prompt(edition: Option<PowerShellEdition>) -> String {
    let background_note = get_background_usage_note().unwrap_or_default();
    let sleep_guidance = get_sleep_guidance().unwrap_or_default();
    let max_timeout = get_max_timeout_ms();
    let default_timeout = get_default_timeout_ms();
    let max_output = get_max_output_length();
    let edition_section = get_edition_section(edition);

    format!(
        r#"Executes a given PowerShell command with optional timeout. Working directory persists between commands; shell state (variables, functions) does not.

IMPORTANT: This tool is for terminal operations via PowerShell: git, npm, docker, and PS cmdlets. DO NOT use it for file operations - use the specialized tools instead.

{edition}

Before executing the command, please follow these steps:

1. Directory Verification:
   - If the command will create new directories or files, first use `Get-ChildItem` to verify the parent directory exists

2. Command Execution:
   - Always quote file paths that contain spaces with double quotes
   - Capture the output of the command.

PowerShell Syntax Notes:
   - Variables use $ prefix: $myVar = "value"
   - Escape character is backtick (`), not backslash
   - Use Verb-Noun cmdlet naming: Get-ChildItem, Set-Location, New-Item, Remove-Item
   - Pipe operator | passes objects, not text
   - String interpolation: "Hello $name" or "Hello $($obj.Property)"
   - Registry access: `HKLM:\SOFTWARE\...`, `HKCU:\...`
   - Environment variables: `$env:NAME`
   - Call native exe: `& "C:\Program Files\App\app.exe" arg1 arg2`

Interactive commands (will hang - runs with -NonInteractive):
   - NEVER use `Read-Host`, `Get-Credential`, `Out-GridView`, `$Host.UI.PromptForChoice`, or `pause`
   - Destructive cmdlets may prompt — add `-Confirm:$false` when intended

Usage notes:
  - The command argument is required.
  - Optional timeout up to {max_timeout}ms ({max_min} minutes). Default: {default_timeout}ms ({default_min} minutes).
  - Output truncated at {max_output} characters.
{background}
  - Avoid using PowerShell for tasks with dedicated tools:
    - File search: Use {glob} (NOT Get-ChildItem -Recurse)
    - Content search: Use {grep} (NOT Select-String)
    - Read files: Use {read} (NOT Get-Content)
    - Edit files: Use {edit}
    - Write files: Use {write} (NOT Set-Content/Out-File)
  - When issuing multiple commands:
    - Independent commands: make multiple {ps} tool calls in a single message.
    - Dependent commands: chain in a single {ps} call.
  - Do NOT prefix commands with `cd` — the working directory is already set correctly.
{sleep}
  - For git commands:
    - Prefer creating a new commit rather than amending.
    - Before destructive operations, consider safer alternatives.
    - Never skip hooks (--no-verify) unless explicitly asked."#,
        edition = edition_section,
        max_timeout = max_timeout,
        max_min = max_timeout / 60000,
        default_timeout = default_timeout,
        default_min = default_timeout / 60000,
        max_output = max_output,
        background = background_note,
        sleep = sleep_guidance,
        glob = GLOB_TOOL_NAME,
        grep = GREP_TOOL_NAME,
        read = FILE_READ_TOOL_NAME,
        edit = FILE_EDIT_TOOL_NAME,
        write = FILE_WRITE_TOOL_NAME,
        ps = POWERSHELL_TOOL_NAME,
    )
}
