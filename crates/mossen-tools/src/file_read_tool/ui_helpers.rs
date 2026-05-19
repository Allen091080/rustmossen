//! Text-mode mirror of `tools/FileReadTool/UI.tsx`.

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(file_path: Option<&str>) -> String {
    format!("Read {}", file_path.unwrap_or("<unknown file>"))
}

/// `UI.tsx` `renderToolUseTag`.
pub fn render_tool_use_tag(file_path: Option<&str>) -> String {
    format!("Read({})", file_path.unwrap_or(""))
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(file_path: Option<&str>, line_count: usize) -> String {
    format!(
        "Read {} line{} from {}",
        line_count,
        if line_count == 1 { "" } else { "s" },
        file_path.unwrap_or("<unknown file>")
    )
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(message: &str) -> String {
    format!("Read error: {}", message)
}

/// `UI.tsx` `renderToolUseRejectedMessage`.
pub fn render_tool_use_rejected_message(file_path: Option<&str>) -> String {
    format!("(Rejected) Read {}", file_path.unwrap_or("<unknown file>"))
}
