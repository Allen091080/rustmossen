//! `/agents` — Manage AI agent delegates.
//!
//! Provides management for agent delegates that can work in parallel
//! on subtasks. Agents can be spawned, monitored, and terminated
//! through this command.

use anyhow::Result;
use async_trait::async_trait;
use mossen_agent::tool_registry::Tool;
use mossen_types::ToolUseContext;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Agents/Delegates management command.
///
/// Allows users to:
/// - View active agents and their current tasks
/// - Spawn new agent delegates for parallel work
/// - Monitor agent progress and resource usage
/// - Terminate agents that are no longer needed
pub struct DelegatesDirective;

/// Agent management subcommands.
const AGENT_SUBCOMMANDS: &[(&str, &str)] = &[
    ("list", "List all active agents"),
    ("spawn", "Create a new agent delegate"),
    ("status", "Show detailed status of an agent"),
    ("stop", "Terminate an agent"),
    ("logs", "View agent execution logs"),
];

fn is_agent_task(task: &mossen_tools::task_store::TaskRecord) -> bool {
    task.metadata.get("type").and_then(|value| value.as_str()) == Some("background_agent")
}

fn agent_id(task: &mossen_tools::task_store::TaskRecord) -> Option<&str> {
    task.metadata
        .get("agentId")
        .and_then(|value| value.as_str())
}

fn list_agent_tasks() -> Vec<mossen_tools::task_store::TaskRecord> {
    mossen_tools::task_store::list_tasks()
        .into_iter()
        .filter(is_agent_task)
        .collect()
}

fn find_agent_task(id: &str) -> Option<mossen_tools::task_store::TaskRecord> {
    list_agent_tasks()
        .into_iter()
        .find(|task| task.id == id || agent_id(task) == Some(id))
}

fn format_agent_list() -> String {
    let tasks = list_agent_tasks();
    if tasks.is_empty() {
        return "Active agents: (none)\n\nUse /agents spawn <task> to create one.".to_string();
    }

    let mut lines = vec!["Agent Delegates".to_string(), String::new()];
    for task in tasks {
        let agent = agent_id(&task).unwrap_or(&task.id);
        lines.push(format!(
            "{}  {}  {}",
            short_id(agent),
            task.status,
            task.subject
        ));
    }
    lines.push(String::new());
    lines.push("Use /agents status <agent-id> or /agents logs <agent-id> for details.".to_string());
    lines.join("\n")
}

fn format_agent_status(task: &mossen_tools::task_store::TaskRecord) -> String {
    let agent = agent_id(task).unwrap_or(&task.id);
    let agent_type = task
        .metadata
        .get("agentType")
        .and_then(|value| value.as_str())
        .unwrap_or("general-purpose");
    let cwd = task
        .metadata
        .get("cwd")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let exit_code = task
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "-".to_string());

    format!(
        "Agent: {agent}\nTask: {task_id}\nType: {agent_type}\nStatus: {status}\nExit code: {exit_code}\nCWD: {cwd}\nSubject: {subject}",
        task_id = task.id,
        status = task.status,
        subject = task.subject,
    )
}

fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

async fn launch_background_agent(task: &str, ctx: &CommandContext) -> Result<String> {
    let context = ToolUseContext {
        cwd: ctx.cwd.to_string_lossy().to_string(),
        additional_working_directories: None,
        extra: std::collections::HashMap::new(),
    };
    let output = mossen_tools::agent::SubagentLauncher
        .execute(
            serde_json::json!({
                "description": task.chars().take(48).collect::<String>(),
                "prompt": task,
                "subagent_type": "general-purpose",
                "run_in_background": true,
                "cwd": ctx.cwd.to_string_lossy().to_string(),
            }),
            &context,
        )
        .await?;
    let value: serde_json::Value = serde_json::from_str(&output.output)?;
    let task_id = value
        .get("task_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let agent = value
        .get("agent_id")
        .and_then(|value| value.as_str())
        .unwrap_or(task_id);
    Ok(format!(
        "Launched agent {agent}\nTask: {task_id}\nUse /agents status {agent} or TaskOutput with task_id={task_id}."
    ))
}

#[async_trait]
impl Directive for DelegatesDirective {
    fn name(&self) -> &str {
        "agents"
    }

    fn aliases(&self) -> &[&str] {
        &["delegates"]
    }

    fn description(&self) -> &str {
        "Manage AI agent delegates"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[list|spawn|status|stop]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let subcommand = args.first().map(|s| s.to_lowercase());

        match subcommand.as_deref() {
            None => Ok(CommandResult::Text(format_agent_list())),
            Some("list") => Ok(CommandResult::Text(format_agent_list())),
            Some("spawn") => {
                let task = args[1..].join(" ");
                if task.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents spawn <task description>".to_string(),
                    ));
                }
                let result = launch_background_agent(&task, ctx).await?;
                Ok(CommandResult::System(result))
            }
            Some("status") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents status <agent-id>".to_string(),
                    ));
                }
                Ok(CommandResult::Text(
                    find_agent_task(agent_id)
                        .map(|task| format_agent_status(&task))
                        .unwrap_or_else(|| format!("Agent {}: not found", agent_id)),
                ))
            }
            Some("stop" | "kill") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents stop <agent-id>".to_string(),
                    ));
                }
                let Some(task) = find_agent_task(agent_id) else {
                    return Ok(CommandResult::Error(format!(
                        "Agent {}: not found",
                        agent_id
                    )));
                };
                let updated = mossen_tools::task_store::stop_background_task(&task.id);
                Ok(CommandResult::System(match updated {
                    Some(task) => format!("Stopped agent task: {}", task.id),
                    None => format!("Agent {}: not found", agent_id),
                }))
            }
            Some("logs") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents logs <agent-id>".to_string(),
                    ));
                }
                Ok(CommandResult::Text(
                    find_agent_task(agent_id)
                        .map(|task| {
                            if task.output.trim().is_empty() {
                                format!("Agent {} has no output yet.", agent_id)
                            } else {
                                task.output
                            }
                        })
                        .unwrap_or_else(|| format!("Agent {}: not found", agent_id)),
                ))
            }
            Some("help" | "-h" | "--help") => {
                let mut help = String::from("Usage: /agents [subcommand]\n\nSubcommands:\n");
                for (cmd, desc) in AGENT_SUBCOMMANDS {
                    help.push_str(&format!("  {:10} {}\n", cmd, desc));
                }
                Ok(CommandResult::Text(help))
            }
            Some(unknown) => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /agents help for available commands.",
                unknown
            ))),
        }
    }
}
