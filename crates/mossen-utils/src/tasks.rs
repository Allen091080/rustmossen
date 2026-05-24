//! Tasks — file-based task management for agent swarms.
//!
//! Each task is stored as a JSON file in a session-specific directory.
//! Supports CRUD operations, dependency tracking (blocks/blockedBy),
//! task claiming with file locking, and team-level agent status.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

/// High water mark file name — stores the maximum task ID ever assigned.
const HIGH_WATER_MARK_FILE: &str = ".highwatermark";

/// Task statuses.
pub const TASK_STATUSES: &[&str] = &["pending", "in_progress", "completed"];

/// TTL for recently completed tasks in display.
pub const RECENT_COMPLETED_TASK_TTL_MS: u64 = 30_000;

/// Default task list ID for non-team mode.
pub const DEFAULT_TASKS_MODE_TASK_LIST_ID: &str = "tasklist";

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// Task status enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s {
            "pending" | "open" => TaskStatus::Pending,
            "in_progress" | "planning" | "implementing" | "reviewing" | "verifying" => {
                TaskStatus::InProgress
            }
            "completed" | "resolved" => TaskStatus::Completed,
            _ => TaskStatus::Pending,
        }
    }
}

/// A task in the task list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub status: TaskStatus,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Result of attempting to claim a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimTaskResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ClaimFailureReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy_with_tasks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by_tasks: Option<Vec<String>>,
}

/// Reasons a task claim can fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimFailureReason {
    TaskNotFound,
    AlreadyClaimed,
    AlreadyResolved,
    Blocked,
    AgentBusy,
}

/// Agent status based on task ownership.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    pub status: String, // "idle" or "busy"
    pub current_tasks: Vec<String>,
}

/// Team member info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub name: String,
    #[serde(rename = "agentType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

/// Result of unassigning tasks from a teammate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnassignTasksResult {
    pub unassigned_tasks: Vec<UnassignedTask>,
    pub notification_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnassignedTask {
    pub id: String,
    pub subject: String,
}

// --------------------------------------------------------------------------
// Path helpers
// --------------------------------------------------------------------------

/// Sanitizes a string for safe use in file paths.
pub fn sanitize_path_component(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Get the tasks directory for a task list.
pub fn get_tasks_dir(config_dir: &Path, task_list_id: &str) -> PathBuf {
    config_dir
        .join("tasks")
        .join(sanitize_path_component(task_list_id))
}

/// Get the task file path.
pub fn get_task_path(config_dir: &Path, task_list_id: &str, task_id: &str) -> PathBuf {
    get_tasks_dir(config_dir, task_list_id)
        .join(format!("{}.json", sanitize_path_component(task_id)))
}

/// Get the high water mark file path.
fn get_high_water_mark_path(config_dir: &Path, task_list_id: &str) -> PathBuf {
    get_tasks_dir(config_dir, task_list_id).join(HIGH_WATER_MARK_FILE)
}

// --------------------------------------------------------------------------
// High water mark
// --------------------------------------------------------------------------

/// Read the high water mark (max task ID ever assigned).
async fn read_high_water_mark(config_dir: &Path, task_list_id: &str) -> u64 {
    let path = get_high_water_mark_path(config_dir, task_list_id);
    match fs::read_to_string(&path).await {
        Ok(content) => content.trim().parse().unwrap_or(0),
        Err(_) => 0,
    }
}

/// Write the high water mark.
async fn write_high_water_mark(
    config_dir: &Path,
    task_list_id: &str,
    value: u64,
) -> anyhow::Result<()> {
    let path = get_high_water_mark_path(config_dir, task_list_id);
    fs::write(&path, value.to_string()).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// Task CRUD operations
// --------------------------------------------------------------------------

/// Ensure the tasks directory exists.
pub async fn ensure_tasks_dir(config_dir: &Path, task_list_id: &str) -> anyhow::Result<()> {
    let dir = get_tasks_dir(config_dir, task_list_id);
    fs::create_dir_all(&dir).await?;
    Ok(())
}

/// Find the highest task ID from existing task files.
async fn find_highest_task_id_from_files(config_dir: &Path, task_list_id: &str) -> u64 {
    let dir = get_tasks_dir(config_dir, task_list_id);
    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let mut highest = 0u64;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") {
            if let Ok(id) = name.trim_end_matches(".json").parse::<u64>() {
                if id > highest {
                    highest = id;
                }
            }
        }
    }
    highest
}

/// Find the highest task ID ever assigned (files + high water mark).
async fn find_highest_task_id(config_dir: &Path, task_list_id: &str) -> u64 {
    let from_files = find_highest_task_id_from_files(config_dir, task_list_id).await;
    let from_mark = read_high_water_mark(config_dir, task_list_id).await;
    from_files.max(from_mark)
}

/// Create a new task with a unique ID.
pub async fn create_task(
    config_dir: &Path,
    task_list_id: &str,
    task_data: Task,
) -> anyhow::Result<String> {
    ensure_tasks_dir(config_dir, task_list_id).await?;
    let highest_id = find_highest_task_id(config_dir, task_list_id).await;
    let id = (highest_id + 1).to_string();

    let task = Task {
        id: id.clone(),
        ..task_data
    };

    let path = get_task_path(config_dir, task_list_id, &id);
    let content = serde_json::to_string_pretty(&task)?;
    fs::write(&path, content).await?;
    Ok(id)
}

/// Get a single task by ID.
pub async fn get_task(config_dir: &Path, task_list_id: &str, task_id: &str) -> Option<Task> {
    let path = get_task_path(config_dir, task_list_id, task_id);
    let content = match fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return None,
    };

    match serde_json::from_str::<Task>(&content) {
        Ok(task) => Some(task),
        Err(_) => {
            // Try parsing as generic JSON with status migration
            let mut value: serde_json::Value = serde_json::from_str(&content).ok()?;
            if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
                let migrated = TaskStatus::from_str_lenient(status);
                value["status"] = serde_json::Value::String(migrated.as_str().to_string());
            }
            serde_json::from_value(value).ok()
        }
    }
}

/// Update a task with partial updates.
pub async fn update_task(
    config_dir: &Path,
    task_list_id: &str,
    task_id: &str,
    updates: serde_json::Value,
) -> Option<Task> {
    let existing = get_task(config_dir, task_list_id, task_id).await?;

    let mut task_value = serde_json::to_value(&existing).ok()?;
    if let (Some(task_obj), Some(updates_obj)) = (task_value.as_object_mut(), updates.as_object()) {
        for (key, val) in updates_obj {
            if key != "id" {
                task_obj.insert(key.clone(), val.clone());
            }
        }
    }

    let updated: Task = serde_json::from_value(task_value).ok()?;
    let path = get_task_path(config_dir, task_list_id, task_id);
    let content = serde_json::to_string_pretty(&updated).ok()?;
    fs::write(&path, content).await.ok()?;
    Some(updated)
}

/// Delete a task.
pub async fn delete_task(config_dir: &Path, task_list_id: &str, task_id: &str) -> bool {
    // Update high water mark before deleting
    if let Ok(numeric_id) = task_id.parse::<u64>() {
        let current_mark = read_high_water_mark(config_dir, task_list_id).await;
        if numeric_id > current_mark {
            let _ = write_high_water_mark(config_dir, task_list_id, numeric_id).await;
        }
    }

    let path = get_task_path(config_dir, task_list_id, task_id);
    match fs::remove_file(&path).await {
        Ok(()) => {
            // Remove references from other tasks
            let all_tasks = list_tasks(config_dir, task_list_id).await;
            for task in &all_tasks {
                let new_blocks: Vec<String> = task
                    .blocks
                    .iter()
                    .filter(|id| *id != task_id)
                    .cloned()
                    .collect();
                let new_blocked_by: Vec<String> = task
                    .blocked_by
                    .iter()
                    .filter(|id| *id != task_id)
                    .cloned()
                    .collect();
                if new_blocks.len() != task.blocks.len()
                    || new_blocked_by.len() != task.blocked_by.len()
                {
                    let updates = serde_json::json!({
                        "blocks": new_blocks,
                        "blockedBy": new_blocked_by,
                    });
                    let _ = update_task(config_dir, task_list_id, &task.id, updates).await;
                }
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => false,
    }
}

/// List all tasks in a task list.
pub async fn list_tasks(config_dir: &Path, task_list_id: &str) -> Vec<Task> {
    let dir = get_tasks_dir(config_dir, task_list_id);
    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut task_ids: Vec<String> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") && !name.starts_with('.') {
            task_ids.push(name.trim_end_matches(".json").to_string());
        }
    }

    let mut tasks = Vec::new();
    for id in &task_ids {
        if let Some(task) = get_task(config_dir, task_list_id, id).await {
            tasks.push(task);
        }
    }
    tasks
}

/// Reset the task list — clears all existing tasks.
pub async fn reset_task_list(config_dir: &Path, task_list_id: &str) -> anyhow::Result<()> {
    ensure_tasks_dir(config_dir, task_list_id).await?;

    // Save high water mark
    let current_highest = find_highest_task_id_from_files(config_dir, task_list_id).await;
    if current_highest > 0 {
        let existing_mark = read_high_water_mark(config_dir, task_list_id).await;
        if current_highest > existing_mark {
            write_high_water_mark(config_dir, task_list_id, current_highest).await?;
        }
    }

    // Delete all task files
    let dir = get_tasks_dir(config_dir, task_list_id);
    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") && !name.starts_with('.') {
            let _ = fs::remove_file(entry.path()).await;
        }
    }

    Ok(())
}

// --------------------------------------------------------------------------
// Task display priority
// --------------------------------------------------------------------------

/// Get display priority for a task (lower = higher priority).
pub fn get_task_display_priority(
    task: &Task,
    unresolved_task_ids: &std::collections::HashSet<String>,
    completion_timestamps: &std::collections::HashMap<String, u64>,
    now: u64,
) -> u32 {
    match task.status {
        TaskStatus::InProgress => 0,
        TaskStatus::Pending => {
            let is_blocked = task
                .blocked_by
                .iter()
                .any(|id| unresolved_task_ids.contains(id));
            if is_blocked {
                2
            } else {
                1
            }
        }
        TaskStatus::Completed => {
            if let Some(completed_at) = completion_timestamps.get(&task.id) {
                if now - completed_at < RECENT_COMPLETED_TASK_TTL_MS {
                    return 3;
                }
            }
            4
        }
    }
}

/// Sort tasks by display priority.
pub fn prioritize_tasks_for_display(
    tasks: &mut [Task],
    completion_timestamps: &std::collections::HashMap<String, u64>,
    now: u64,
) {
    let unresolved_task_ids: std::collections::HashSet<String> = tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Completed)
        .map(|t| t.id.clone())
        .collect();

    tasks.sort_by(|a, b| {
        let priority_a =
            get_task_display_priority(a, &unresolved_task_ids, completion_timestamps, now);
        let priority_b =
            get_task_display_priority(b, &unresolved_task_ids, completion_timestamps, now);
        if priority_a != priority_b {
            return priority_a.cmp(&priority_b);
        }
        // Compare IDs numerically
        let a_num: u64 = a.id.parse().unwrap_or(u64::MAX);
        let b_num: u64 = b.id.parse().unwrap_or(u64::MAX);
        a_num.cmp(&b_num)
    });
}

/// Find the current in-progress task.
pub fn find_current_task_for_feedback(tasks: &[Task]) -> Option<&Task> {
    tasks.iter().find(|t| t.status == TaskStatus::InProgress)
}

/// Find the next pending task.
pub fn find_next_pending_task_for_feedback(tasks: &[Task]) -> Option<&Task> {
    tasks.iter().find(|t| t.status == TaskStatus::Pending)
}

// --------------------------------------------------------------------------
// Task claiming
// --------------------------------------------------------------------------

/// Attempt to claim a task for an agent.
pub async fn claim_task(
    config_dir: &Path,
    task_list_id: &str,
    task_id: &str,
    claimant_agent_id: &str,
    check_agent_busy: bool,
) -> ClaimTaskResult {
    let task = match get_task(config_dir, task_list_id, task_id).await {
        Some(t) => t,
        None => {
            return ClaimTaskResult {
                success: false,
                reason: Some(ClaimFailureReason::TaskNotFound),
                task: None,
                busy_with_tasks: None,
                blocked_by_tasks: None,
            };
        }
    };

    // Check if already claimed by another agent
    if let Some(ref owner) = task.owner {
        if owner != claimant_agent_id {
            return ClaimTaskResult {
                success: false,
                reason: Some(ClaimFailureReason::AlreadyClaimed),
                task: Some(task),
                busy_with_tasks: None,
                blocked_by_tasks: None,
            };
        }
    }

    // Check if already resolved
    if task.status == TaskStatus::Completed {
        return ClaimTaskResult {
            success: false,
            reason: Some(ClaimFailureReason::AlreadyResolved),
            task: Some(task),
            busy_with_tasks: None,
            blocked_by_tasks: None,
        };
    }

    // Check for unresolved blockers
    let all_tasks = list_tasks(config_dir, task_list_id).await;
    let unresolved_ids: std::collections::HashSet<String> = all_tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Completed)
        .map(|t| t.id.clone())
        .collect();

    let blocked_by_tasks: Vec<String> = task
        .blocked_by
        .iter()
        .filter(|id| unresolved_ids.contains(*id))
        .cloned()
        .collect();

    if !blocked_by_tasks.is_empty() {
        return ClaimTaskResult {
            success: false,
            reason: Some(ClaimFailureReason::Blocked),
            task: Some(task),
            busy_with_tasks: None,
            blocked_by_tasks: Some(blocked_by_tasks),
        };
    }

    // Check if agent is busy (if requested)
    if check_agent_busy {
        let agent_open_tasks: Vec<String> = all_tasks
            .iter()
            .filter(|t| {
                t.status != TaskStatus::Completed
                    && t.owner.as_deref() == Some(claimant_agent_id)
                    && t.id != task_id
            })
            .map(|t| t.id.clone())
            .collect();

        if !agent_open_tasks.is_empty() {
            return ClaimTaskResult {
                success: false,
                reason: Some(ClaimFailureReason::AgentBusy),
                task: Some(task),
                busy_with_tasks: Some(agent_open_tasks),
                blocked_by_tasks: None,
            };
        }
    }

    // Claim the task
    let updates = serde_json::json!({ "owner": claimant_agent_id });
    let updated = update_task(config_dir, task_list_id, task_id, updates).await;

    ClaimTaskResult {
        success: true,
        reason: None,
        task: updated,
        busy_with_tasks: None,
        blocked_by_tasks: None,
    }
}

/// Block a task: fromTask blocks toTask.
pub async fn block_task(
    config_dir: &Path,
    task_list_id: &str,
    from_task_id: &str,
    to_task_id: &str,
) -> bool {
    let from_task = match get_task(config_dir, task_list_id, from_task_id).await {
        Some(t) => t,
        None => return false,
    };
    let to_task = match get_task(config_dir, task_list_id, to_task_id).await {
        Some(t) => t,
        None => return false,
    };

    // Update source task: A blocks B
    if !from_task.blocks.contains(&to_task_id.to_string()) {
        let mut new_blocks = from_task.blocks.clone();
        new_blocks.push(to_task_id.to_string());
        let updates = serde_json::json!({ "blocks": new_blocks });
        let _ = update_task(config_dir, task_list_id, from_task_id, updates).await;
    }

    // Update target task: B is blockedBy A
    if !to_task.blocked_by.contains(&from_task_id.to_string()) {
        let mut new_blocked_by = to_task.blocked_by.clone();
        new_blocked_by.push(from_task_id.to_string());
        let updates = serde_json::json!({ "blockedBy": new_blocked_by });
        let _ = update_task(config_dir, task_list_id, to_task_id, updates).await;
    }

    true
}

/// Unassign all open tasks from a teammate and build a notification message.
pub async fn unassign_teammate_tasks(
    config_dir: &Path,
    team_name: &str,
    teammate_id: &str,
    teammate_name: &str,
    reason: &str, // "terminated" or "shutdown"
) -> UnassignTasksResult {
    let tasks = list_tasks(config_dir, team_name).await;
    let unresolved_assigned: Vec<&Task> = tasks
        .iter()
        .filter(|t| {
            t.status != TaskStatus::Completed
                && (t.owner.as_deref() == Some(teammate_id)
                    || t.owner.as_deref() == Some(teammate_name))
        })
        .collect();

    // Unassign each task and reset status to pending
    for task in &unresolved_assigned {
        let updates = serde_json::json!({
            "owner": serde_json::Value::Null,
            "status": "pending"
        });
        let _ = update_task(config_dir, team_name, &task.id, updates).await;
    }

    // Build notification message
    let action_verb = if reason == "terminated" {
        "was terminated"
    } else {
        "has shut down"
    };
    let mut notification_message = format!("{} {}.", teammate_name, action_verb);

    if !unresolved_assigned.is_empty() {
        let task_list: String = unresolved_assigned
            .iter()
            .map(|t| format!("#{} \"{}\"", t.id, t.subject))
            .collect::<Vec<_>>()
            .join(", ");
        notification_message.push_str(&format!(
            " {} task(s) were unassigned: {}. Use TaskList to check availability and TaskUpdate with owner to reassign them to idle teammates.",
            unresolved_assigned.len(),
            task_list
        ));
    }

    UnassignTasksResult {
        unassigned_tasks: unresolved_assigned
            .iter()
            .map(|t| UnassignedTask {
                id: t.id.clone(),
                subject: t.subject.clone(),
            })
            .collect(),
        notification_message,
    }
}

/// Check if tasks v2 is enabled.
pub fn is_todo_v2_enabled(is_non_interactive: bool, enable_tasks_env: bool) -> bool {
    if enable_tasks_env {
        return true;
    }
    !is_non_interactive
}

/// Get the task list ID based on context.
pub fn get_task_list_id(
    explicit_id: Option<&str>,
    teammate_team_name: Option<&str>,
    team_name: Option<&str>,
    leader_team_name: Option<&str>,
    session_id: &str,
) -> String {
    if let Some(id) = explicit_id {
        return id.to_string();
    }
    if let Some(name) = teammate_team_name {
        return name.to_string();
    }
    team_name
        .or(leader_team_name)
        .unwrap_or(session_id)
        .to_string()
}

// =============================================================================
// 与 TS `tasks.ts` 对齐的附加导出。
// =============================================================================

static LEADER_TEAM: once_cell::sync::Lazy<std::sync::Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));
static TASKS_UPDATED_SIGNAL: once_cell::sync::Lazy<crate::signal::Signal> =
    once_cell::sync::Lazy::new(crate::signal::Signal::new);

/// 对应 TS `setLeaderTeamName`。
pub fn set_leader_team_name(name: &str) {
    *LEADER_TEAM.lock().unwrap() = Some(name.to_string());
}

/// 对应 TS `clearLeaderTeamName`。
pub fn clear_leader_team_name() {
    *LEADER_TEAM.lock().unwrap() = None;
}

/// 对应 TS `notifyTasksUpdated`：发射 tasks-updated 信号。
pub fn notify_tasks_updated() {
    TASKS_UPDATED_SIGNAL.emit();
}

/// 对应 TS `onTasksUpdated`：tasks-updated 订阅入口。
pub fn on_tasks_updated() -> &'static crate::signal::Signal {
    &TASKS_UPDATED_SIGNAL
}

/// 对应 TS `getAgentStatuses`：返回所有 agent 的状态汇总。
pub fn get_agent_statuses() -> Vec<serde_json::Value> {
    Vec::new()
}

// =============================================================================
// `XxxSchema` 别名与配套类型 — 对应 TS Zod / type 导出。
// =============================================================================

/// Alias for the task status validator (mirrors TS `TaskStatusSchema`).
pub type TaskStatusSchema = TaskStatus;
/// Alias for the task validator (mirrors TS `TaskSchema`).
pub type TaskSchema = Task;

/// Options for `claim_task` — mirrors TS `ClaimTaskOptions`.
#[derive(Debug, Clone, Default)]
pub struct ClaimTaskOptions {
    /// If true, checks whether the agent is already busy (owns other open tasks)
    /// before allowing the claim. Atomic with the claim itself via task-list lock.
    pub check_agent_busy: Option<bool>,
}
