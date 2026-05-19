//! Logic helpers from `tools/FileWriteTool/UI.tsx` — the JSX render fns are
//! ported in `mossen-tui`. Here we expose the data-shaping helpers the model
//! invokes during tool-use bookkeeping.

/// `UI.tsx` `countLines` — count visible lines, treating a trailing newline
/// as a terminator (not a new empty line).
pub fn count_lines(content: &str) -> usize {
    if content.is_empty() {
        return 0;
    }
    let parts = content.split('\n').count();
    if content.ends_with('\n') {
        parts.saturating_sub(1)
    } else {
        parts
    }
}

/// `UI.tsx` `isResultTruncated` — checks whether a rendered FileWrite output
/// has been collapsed by the UI.
pub fn is_result_truncated(rendered: &str) -> bool {
    rendered.contains("...truncated") || rendered.contains("…truncated")
}

/// `UI.tsx` `userFacingName` — returns the display name for the FileWrite
/// tool, accounting for plan-mode hint variants.
pub fn user_facing_name(file_path: Option<&str>) -> String {
    if let Some(path) = file_path {
        if path.starts_with('/') || path.starts_with('~') {
            return "Write".to_string();
        }
    }
    "Write".to_string()
}

/// `UI.tsx` `getToolUseSummary` — short one-line summary of a tool invocation.
pub fn get_tool_use_summary(file_path: Option<&str>, content: Option<&str>) -> String {
    let path = file_path.unwrap_or("?");
    let lines = content.map(count_lines).unwrap_or(0);
    format!("Wrote {} lines to {}", lines, path)
}

/// `UI.tsx` `renderToolUseMessage` — produce plain-text equivalent.
pub fn render_tool_use_message(file_path: Option<&str>, content: Option<&str>) -> String {
    get_tool_use_summary(file_path, content)
}

/// `UI.tsx` `renderToolUseRejectedMessage`.
pub fn render_tool_use_rejected_message(file_path: Option<&str>) -> String {
    format!(
        "(Rejected) Write to {}",
        file_path.unwrap_or("<unknown file>")
    )
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(error: &str) -> String {
    format!("Error: {}", error)
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(file_path: Option<&str>, lines_written: usize) -> String {
    format!(
        "Wrote {} lines to {}",
        lines_written,
        file_path.unwrap_or("<unknown file>")
    )
}
