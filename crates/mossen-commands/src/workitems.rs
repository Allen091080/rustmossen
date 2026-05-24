//! `/tasks` — List and manage background tasks (local-widget).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Workitems directive — list and manage background tasks.
pub struct WorkitemsDirective;

/// Represents a background task.
#[derive(Debug, Clone)]
struct BackgroundTask {
    id: String,
    name: String,
    status: TaskStatus,
    progress: Option<String>,
}

/// Status of a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            TaskStatus::Running => "⟳",
            TaskStatus::Completed => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Cancelled => "○",
        }
    }
}

/// Get the list of current background tasks.
fn get_background_tasks(_ctx: &CommandContext) -> Vec<BackgroundTask> {
    // In the full implementation, this reads from the app state's task manager.
    // Background tasks are managed by the agent runtime.
    Vec::new()
}

/// Format background tasks for display.
fn format_tasks_display(tasks: &[BackgroundTask]) -> String {
    if tasks.is_empty() {
        return "No background tasks running.\n\n\
                Background tasks are created when you ask me to do something \
                that runs in the background (e.g., long-running builds, deployments)."
            .to_string();
    }

    let mut output = String::from("Background Tasks\n");
    output.push_str("================\n\n");

    for task in tasks {
        let progress_str = task
            .progress
            .as_deref()
            .map(|p| format!(" ({})", p))
            .unwrap_or_default();

        output.push_str(&format!(
            "  {} [{}] {}{}\n",
            task.status.icon(),
            task.status.as_str(),
            task.name,
            progress_str,
        ));
    }

    let running_count = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    output.push_str(&format!(
        "\n{} task(s) total, {} running.",
        tasks.len(),
        running_count
    ));

    output
}

#[async_trait]
impl Directive for WorkitemsDirective {
    fn name(&self) -> &str {
        "tasks"
    }

    fn aliases(&self) -> &[&str] {
        &["bashes"]
    }

    fn description(&self) -> &str {
        "List and manage background tasks"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let tasks = get_background_tasks(ctx);
        let display = format_tasks_display(&tasks);
        Ok(CommandResult::Text(display))
    }
}
