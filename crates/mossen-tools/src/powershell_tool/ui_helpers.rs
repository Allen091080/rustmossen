//! Text-mode mirror of `tools/PowerShellTool/UI.tsx`.

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(command: Option<&str>) -> String {
    match command {
        Some(c) if !c.is_empty() => format!("PS> {}", c),
        _ => "(empty command)".to_string(),
    }
}

/// `UI.tsx` `renderToolUseProgressMessage`.
pub fn render_tool_use_progress_message(elapsed_ms: u64) -> String {
    if elapsed_ms < 1000 {
        format!("PowerShell running... ({}ms)", elapsed_ms)
    } else {
        format!("PowerShell running... ({:.1}s)", elapsed_ms as f64 / 1000.0)
    }
}

/// `UI.tsx` `renderToolUseQueuedMessage`.
pub fn render_tool_use_queued_message() -> &'static str {
    "PowerShell queued"
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(stdout: Option<&str>, exit_code: i32) -> String {
    let body = stdout.unwrap_or("");
    if body.is_empty() {
        format!("(PowerShell exit {})", exit_code)
    } else {
        format!("{}\n(PowerShell exit {})", body, exit_code)
    }
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(message: &str) -> String {
    format!("PowerShell error: {}", message)
}
