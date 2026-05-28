//! Logic helpers from `tools/BashTool/UI.tsx` — plain-text/text-only mirror.

/// `UI.tsx` `BackgroundHint` — produce the inline hint that explains how to
/// background a long-running command.
pub fn background_hint() -> &'static str {
    "Use run_in_background=true to keep this running in the background."
}

/// `UI.tsx` `renderToolUseMessage`.
pub fn render_tool_use_message(command: Option<&str>) -> String {
    match command {
        Some(c) if !c.is_empty() => format!("$ {}", c),
        _ => "(empty command)".to_string(),
    }
}

/// `UI.tsx` `renderToolUseProgressMessage`.
pub fn render_tool_use_progress_message(elapsed_ms: u64) -> String {
    if elapsed_ms < 1000 {
        format!("Running... ({}ms)", elapsed_ms)
    } else {
        format!("Running... ({:.1}s)", elapsed_ms as f64 / 1000.0)
    }
}

/// `UI.tsx` `renderToolUseQueuedMessage`.
pub fn render_tool_use_queued_message() -> &'static str {
    "Queued (waiting for an earlier command to finish)"
}

/// `UI.tsx` `renderToolResultMessage`.
pub fn render_tool_result_message(stdout: Option<&str>, exit_code: i32) -> String {
    let body = stdout.unwrap_or("");
    if body.is_empty() {
        format!("(exit {})", exit_code)
    } else {
        format!("{}\n(exit {})", body, exit_code)
    }
}

/// `UI.tsx` `renderToolUseErrorMessage`.
pub fn render_tool_use_error_message(message: &str) -> String {
    format!("Bash error: {}", message)
}
