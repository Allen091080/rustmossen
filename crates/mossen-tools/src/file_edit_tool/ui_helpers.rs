//! Logic helpers from `tools/FileEditTool/UI.tsx` — JSX rendering lives in
//! the TUI crate; these are the plain-text/serialization-friendly variants
//! used by tool-use bookkeeping.

/// `UI.tsx` `getToolUseSummary`.
pub fn get_tool_use_summary(file_path: Option<&str>) -> String {
    format!("Edit {}", file_path.unwrap_or("<unknown file>"))
}

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(file_path: Option<&str>) -> String {
    get_tool_use_summary(file_path)
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(file_path: Option<&str>, lines_changed: usize) -> String {
    format!(
        "Edited {} ({} line{} changed)",
        file_path.unwrap_or("<unknown file>"),
        lines_changed,
        if lines_changed == 1 { "" } else { "s" }
    )
}

/// `UI.tsx` `renderToolUseRejectedMessage`.
pub fn render_tool_use_rejected_message(file_path: Option<&str>) -> String {
    format!(
        "(Rejected) Edit to {}",
        file_path.unwrap_or("<unknown file>")
    )
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(message: &str) -> String {
    format!("Error: {}", message)
}
