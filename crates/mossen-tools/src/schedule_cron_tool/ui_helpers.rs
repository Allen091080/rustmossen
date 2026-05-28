//! Text-mode mirror of `tools/ScheduleCronTool/UI.tsx`.

use mossen_utils::string_utils::truncate_chars;

/// `UI.tsx` `renderCreateToolUseMessage`.
pub fn render_create_tool_use_message(schedule: Option<&str>, prompt: Option<&str>) -> String {
    let sched = schedule.unwrap_or("?");
    let p = prompt.unwrap_or("");
    if p.is_empty() {
        format!("Create cron schedule: {}", sched)
    } else {
        format!("Create cron schedule: {} ({})", sched, summarize(p))
    }
}

/// `UI.tsx` `renderCreateResultMessage`.
pub fn render_create_result_message(routine_id: Option<&str>) -> String {
    match routine_id {
        Some(id) => format!("Created cron schedule {}", id),
        None => "Created cron schedule".to_string(),
    }
}

/// `UI.tsx` `renderDeleteToolUseMessage`.
pub fn render_delete_tool_use_message(routine_id: Option<&str>) -> String {
    format!("Delete cron schedule {}", routine_id.unwrap_or("?"))
}

/// `UI.tsx` `renderDeleteResultMessage`.
pub fn render_delete_result_message(routine_id: Option<&str>) -> String {
    format!("Deleted cron schedule {}", routine_id.unwrap_or("?"))
}

/// `UI.tsx` `renderListToolUseMessage`.
pub fn render_list_tool_use_message() -> &'static str {
    "List cron schedules"
}

/// `UI.tsx` `renderListResultMessage`.
pub fn render_list_result_message(count: usize) -> String {
    format!(
        "Listed {} cron schedule{}",
        count,
        if count == 1 { "" } else { "s" }
    )
}

fn summarize(p: &str) -> String {
    const LIMIT: usize = 60;
    truncate_chars(p, LIMIT)
}
