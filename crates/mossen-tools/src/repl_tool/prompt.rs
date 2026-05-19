//! REPLTool constants.
//!
//! Translated from tools/REPLTool/constants.ts

pub const REPL_TOOL_NAME: &str = "REPL";

/// REPL mode is default-on for Mossen users in the interactive CLI.
pub fn is_repl_mode_enabled() -> bool {
    // Check env vars
    if let Ok(val) = std::env::var("MOSSEN_CODE_REPL") {
        if val == "0" || val.eq_ignore_ascii_case("false") {
            return false;
        }
    }
    if let Ok(val) = std::env::var("MOSSEN_REPL_MODE") {
        if val == "1" || val.eq_ignore_ascii_case("true") {
            return true;
        }
    }
    let user_type = std::env::var("USER_TYPE").unwrap_or_default();
    let entrypoint = std::env::var("MOSSEN_CODE_ENTRYPOINT").unwrap_or_default();
    user_type == "mossen" && entrypoint == "cli"
}

/// Tools that are only accessible via REPL when REPL mode is enabled.
pub const REPL_ONLY_TOOLS: &[&str] = &[
    "Read",      // FILE_READ_TOOL_NAME
    "Write",     // FILE_WRITE_TOOL_NAME
    "Edit",      // FILE_EDIT_TOOL_NAME
    "Glob",      // GLOB_TOOL_NAME
    "Grep",      // GREP_TOOL_NAME
    "Bash",      // BASH_TOOL_NAME
    "NotebookEdit", // NOTEBOOK_EDIT_TOOL_NAME
    "Agent",     // AGENT_TOOL_NAME
];
