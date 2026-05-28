//! In-process task store shared between TaskCreate/Get/List/Update/Output.
//!
//! The TS tooling backs `TaskCreate` / `TaskList` / `TaskGet` / `TaskUpdate`
//! with a process-wide store so the model can plan, track, and revisit work
//! across turns. The Rust port previously had each tool returning a stub
//! response (random UUID, empty list, fake "pending" record) which made the
//! whole task workflow unusable — TaskGet on a real id always reported the
//! task as empty/pending.
//!
//! This module:
//!   * defines a `TaskRecord` matching what TaskGet/TaskList serialise,
//!   * owns a single `Mutex<HashMap<id, TaskRecord>>` behind a `OnceLock`,
//!   * exposes thin CRUD helpers (`create_task`, `get_task`, `list_tasks`,
//!     `update_task`, `delete_task`) that every tool calls into.
//!
//! Scope: in-process only — tasks live until the process exits, just like
//! the TS implementation. Persistence + cross-process sharing belongs to a
//! later pass.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex, OnceLock};

#[cfg(unix)]
use nix::sys::signal::{killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Mirrors the structure TaskGet / TaskList return. Fields kept flat so
/// `serde_json::to_value(&record)` produces the same wire shape the model
/// already expects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    /// Tasks this one blocks.
    #[serde(default)]
    pub blocks: Vec<String>,
    /// Tasks blocking this one.
    #[serde(rename = "blockedBy", default)]
    pub blocked_by: Vec<String>,
    /// Owner agent name when claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Present-continuous label rendered while the task is in_progress.
    #[serde(
        rename = "activeForm",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub active_form: Option<String>,
    /// Free-form metadata — JSON object.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
    #[serde(
        rename = "completedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<i64>,
    /// Captured stdout from `TaskOutput` (for background-agent tasks).
    #[serde(default)]
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// Lightweight task view for UI/process-list surfaces. This intentionally
/// excludes `TaskRecord::output`; completed agent output can be large enough
/// that cloning it on every render makes keyboard input visibly lag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub id: String,
    pub subject: String,
    pub status: String,
    #[serde(
        rename = "completedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(rename = "outputLen", default)]
    pub output_len: usize,
    #[serde(rename = "taskType", default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskStoreEvent {
    pub id: String,
    pub subject: String,
    pub status: String,
    #[serde(rename = "taskType", default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(
        rename = "completedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl TaskRecord {
    fn new(id: String, subject: String, description: String) -> Self {
        Self {
            id,
            subject,
            description,
            status: "pending".to_string(),
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            owner: None,
            active_form: None,
            metadata: HashMap::new(),
            completed_at: None,
            output: String::new(),
            exit_code: None,
        }
    }
}

impl From<&TaskRecord> for TaskSnapshot {
    fn from(record: &TaskRecord) -> Self {
        Self {
            id: record.id.clone(),
            subject: record.subject.clone(),
            status: record.status.clone(),
            completed_at: record.completed_at,
            exit_code: record.exit_code,
            output_len: record.output.len(),
            task_type: task_type(record),
        }
    }
}

impl From<&TaskRecord> for TaskStoreEvent {
    fn from(record: &TaskRecord) -> Self {
        Self {
            id: record.id.clone(),
            subject: record.subject.clone(),
            status: record.status.clone(),
            task_type: task_type(record),
            completed_at: record.completed_at,
            exit_code: record.exit_code,
        }
    }
}

fn store() -> &'static Mutex<HashMap<String, TaskRecord>> {
    static STORE: OnceLock<Mutex<HashMap<String, TaskRecord>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn event_subscribers() -> &'static Mutex<Vec<mpsc::Sender<TaskStoreEvent>>> {
    static SUBSCRIBERS: OnceLock<Mutex<Vec<mpsc::Sender<TaskStoreEvent>>>> = OnceLock::new();
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn subscribe_task_events() -> mpsc::Receiver<TaskStoreEvent> {
    let (tx, rx) = mpsc::channel();
    event_subscribers().lock().unwrap().push(tx);
    rx
}

fn notify_task_event(record: &TaskRecord) {
    if !is_terminal_status(&record.status) {
        return;
    }
    let event = TaskStoreEvent::from(record);
    event_subscribers()
        .lock()
        .unwrap()
        .retain(|tx| tx.send(event.clone()).is_ok());
}

#[derive(Debug, Clone)]
struct BackgroundShellProcess {
    #[cfg(unix)]
    pgid: i32,
    #[cfg(not(unix))]
    pid: u32,
}

fn background_shells() -> &'static Mutex<HashMap<String, BackgroundShellProcess>> {
    static STORE: OnceLock<Mutex<HashMap<String, BackgroundShellProcess>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "deleted" | "failed" | "cancelled" | "canceled"
    )
}

fn task_type(record: &TaskRecord) -> Option<String> {
    record
        .metadata
        .get("type")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

#[cfg(unix)]
fn terminate_background_process(process: &BackgroundShellProcess) {
    let pgid = Pid::from_raw(process.pgid);
    let _ = killpg(pgid, Signal::SIGTERM);
    let _ = killpg(pgid, Signal::SIGKILL);
}

#[cfg(not(unix))]
fn terminate_background_process(process: &BackgroundShellProcess) {
    let _ = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(process.pid.to_string())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(process.pid.to_string())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Insert a new task. Returns the freshly-created record (cloned).
pub fn create_task(subject: String, description: String) -> TaskRecord {
    create_task_with_id(uuid::Uuid::new_v4().to_string(), subject, description)
}

/// Insert a new task with a caller-provided id.
pub fn create_task_with_id(id: String, subject: String, description: String) -> TaskRecord {
    let record = TaskRecord::new(id.clone(), subject, description);
    store().lock().unwrap().insert(id, record.clone());
    record
}

/// Insert a task record for a background Bash command and mark it running.
pub fn create_background_shell_task(
    command: String,
    cwd: String,
    description: Option<String>,
    timeout_ms: u64,
) -> TaskRecord {
    create_background_shell_task_with_id(
        uuid::Uuid::new_v4().to_string(),
        command,
        cwd,
        description,
        timeout_ms,
    )
}

/// Insert a task record for a background Bash command with a caller-provided id.
pub fn create_background_shell_task_with_id(
    task_id: String,
    command: String,
    cwd: String,
    description: Option<String>,
    timeout_ms: u64,
) -> TaskRecord {
    let subject = description
        .clone()
        .unwrap_or_else(|| format!("bash: {}", command.chars().take(80).collect::<String>()));
    let mut record = create_task_with_id(task_id, subject, command.clone());
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), json!("background_shell"));
    metadata.insert("kind".to_string(), json!("bash"));
    metadata.insert("command".to_string(), json!(command));
    metadata.insert("cwd".to_string(), json!(cwd));
    metadata.insert("timeoutMs".to_string(), json!(timeout_ms));
    metadata.insert("startedAt".to_string(), json!(now_ms()));
    update_task(&record.id, |r| {
        r.status = "in_progress".to_string();
        r.active_form = Some("Running command".to_string());
        r.metadata = metadata.clone();
    });
    record.status = "in_progress".to_string();
    record.active_form = Some("Running command".to_string());
    record.metadata = metadata;
    record
}

/// Insert a task record for a background sub-agent and mark it running.
pub fn create_background_agent_task(
    task_id: String,
    agent_id: String,
    agent_type: String,
    description: String,
    prompt: String,
    cwd: String,
) -> TaskRecord {
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), json!("background_agent"));
    metadata.insert("kind".to_string(), json!("agent"));
    metadata.insert("agentId".to_string(), json!(agent_id));
    metadata.insert("agentType".to_string(), json!(agent_type));
    metadata.insert("prompt".to_string(), json!(prompt.clone()));
    metadata.insert("cwd".to_string(), json!(cwd));
    metadata.insert("startedAt".to_string(), json!(now_ms()));

    let mut record = TaskRecord::new(task_id.clone(), description, prompt);
    record.status = "in_progress".to_string();
    record.active_form = Some("Running agent".to_string());
    record.metadata = metadata;
    store().lock().unwrap().insert(task_id, record.clone());
    record
}

/// Lookup by id.
pub fn get_task(id: &str) -> Option<TaskRecord> {
    store().lock().unwrap().get(id).cloned()
}

/// Snapshot of all tasks, sorted by id for stable output.
pub fn list_tasks() -> Vec<TaskRecord> {
    let map = store().lock().unwrap();
    let mut v: Vec<TaskRecord> = map.values().cloned().collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Lightweight snapshot of all tasks, sorted by id for stable UI output.
pub fn list_task_snapshots() -> Vec<TaskSnapshot> {
    let map = store().lock().unwrap();
    let mut v: Vec<TaskSnapshot> = map.values().map(TaskSnapshot::from).collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Apply a mutation closure to a task. Returns the post-mutation snapshot
/// when the id exists, `None` otherwise.
pub fn update_task<F>(id: &str, mutate: F) -> Option<TaskRecord>
where
    F: FnOnce(&mut TaskRecord),
{
    let mut map = store().lock().unwrap();
    if let Some(rec) = map.get_mut(id) {
        mutate(rec);
        return Some(rec.clone());
    }
    None
}

/// Register the OS process backing a background shell task.
pub fn register_background_shell_process(task_id: &str, pid: u32) {
    let process = BackgroundShellProcess {
        #[cfg(unix)]
        pgid: pid as i32,
        #[cfg(not(unix))]
        pid,
    };
    background_shells()
        .lock()
        .unwrap()
        .insert(task_id.to_string(), process);
    update_task(task_id, |r| {
        r.metadata.insert("pid".to_string(), json!(pid));
        #[cfg(unix)]
        r.metadata.insert("pgid".to_string(), json!(pid as i32));
    });
}

/// Remove a background shell process handle without changing the public task.
pub fn unregister_background_shell_process(task_id: &str) {
    background_shells().lock().unwrap().remove(task_id);
}

/// Stop a running background task. Shell tasks also get their process group
/// terminated so children do not survive behind the shell parent.
pub fn stop_background_task(task_id: &str) -> Option<TaskRecord> {
    if let Some(process) = background_shells().lock().unwrap().remove(task_id) {
        terminate_background_process(&process);
    }

    let mut should_notify = false;
    let updated = update_task(task_id, |r| {
        if !is_terminal_status(&r.status) {
            r.status = "cancelled".to_string();
            r.completed_at = Some(now_ms());
            r.output = append_output_note(&r.output, "Task stopped by request.");
            should_notify = true;
        }
        r.metadata.insert("stoppedAt".to_string(), json!(now_ms()));
    });
    if should_notify {
        if let Some(record) = updated.as_ref() {
            notify_task_event(record);
        }
    }
    updated
}

/// Finish a background shell task after the process exits.
pub fn finish_background_shell_task(
    task_id: &str,
    status: &str,
    output: String,
    exit_code: Option<i32>,
    timed_out: bool,
) -> Option<TaskRecord> {
    unregister_background_shell_process(task_id);
    let mut should_notify = false;
    let updated = update_task(task_id, |r| {
        if !r.status.eq("cancelled") && !r.status.eq("deleted") {
            r.status = status.to_string();
            r.completed_at = Some(now_ms());
            should_notify = true;
        } else if r.completed_at.is_none() {
            r.completed_at = Some(now_ms());
        }
        r.output = output;
        r.exit_code = exit_code;
        r.metadata
            .insert("completedAt".to_string(), json!(now_ms()));
        r.metadata.insert("timedOut".to_string(), json!(timed_out));
    });
    if should_notify {
        if let Some(record) = updated.as_ref() {
            notify_task_event(record);
        }
    }
    updated
}

/// Finish a background sub-agent task after the child agent process exits.
pub fn finish_background_agent_task(
    task_id: &str,
    status: &str,
    output: String,
    exit_code: Option<i32>,
) -> Option<TaskRecord> {
    let mut should_notify = false;
    let updated = update_task(task_id, |r| {
        if !r.status.eq("cancelled") && !r.status.eq("deleted") {
            r.status = status.to_string();
            r.completed_at = Some(now_ms());
            should_notify = true;
        } else if r.completed_at.is_none() {
            r.completed_at = Some(now_ms());
        }
        r.output = output;
        r.exit_code = exit_code;
        r.metadata
            .insert("completedAt".to_string(), json!(now_ms()));
    });
    if should_notify {
        if let Some(record) = updated.as_ref() {
            notify_task_event(record);
        }
    }
    updated
}

/// Keep a task open when a TaskCompleted hook blocks the terminal transition.
pub fn block_task_completion(
    task_id: &str,
    output: String,
    exit_code: Option<i32>,
    reason: String,
) -> Option<TaskRecord> {
    update_task(task_id, |r| {
        r.output = append_output_note(&output, &reason);
        r.exit_code = exit_code;
        r.metadata
            .insert("completionBlockedAt".to_string(), json!(now_ms()));
        r.metadata
            .insert("completionBlockedReason".to_string(), json!(reason));
    })
}

/// Public helper shared by TaskOutput and tests.
pub fn is_task_ready_status(status: &str) -> bool {
    is_terminal_status(status)
}

fn append_output_note(existing: &str, note: &str) -> String {
    if existing.trim().is_empty() {
        note.to_string()
    } else {
        format!("{existing}\n{note}")
    }
}

/// Used by tests + `/reset` paths.
pub fn clear() {
    store().lock().unwrap().clear();
    background_shells().lock().unwrap().clear();
}

#[cfg(test)]
pub(crate) struct TaskStoreTestGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for TaskStoreTestGuard {
    fn drop(&mut self) {
        clear();
    }
}

#[cfg(test)]
pub(crate) fn test_store_guard() -> TaskStoreTestGuard {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear();
    TaskStoreTestGuard { _guard: guard }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_get_returns_same_record() {
        let _guard = test_store_guard();
        let r = create_task("write docs".into(), "section: install".into());
        let got = get_task(&r.id).expect("task should be retrievable");
        assert_eq!(got.subject, "write docs");
        assert_eq!(got.status, "pending");
    }

    #[test]
    fn update_mutates_status_and_owner() {
        let _guard = test_store_guard();
        let r = create_task("ship".into(), "merge PR".into());
        let updated = update_task(&r.id, |t| {
            t.status = "in_progress".into();
            t.owner = Some("alice".into());
        })
        .expect("update should find task");
        assert_eq!(updated.status, "in_progress");
        assert_eq!(updated.owner.as_deref(), Some("alice"));
    }

    #[test]
    fn list_returns_inserted_tasks() {
        let _guard = test_store_guard();
        create_task("a".into(), "".into());
        create_task("b".into(), "".into());
        assert_eq!(list_tasks().len(), 2);
    }

    #[test]
    fn list_task_snapshots_excludes_large_output_payloads() {
        let _guard = test_store_guard();
        let record = create_background_agent_task(
            "agent-large".to_string(),
            "agent-large".to_string(),
            "general".to_string(),
            "large output task".to_string(),
            "prompt".to_string(),
            ".".to_string(),
        );
        finish_background_agent_task(&record.id, "completed", "x".repeat(1024 * 1024), Some(0));

        let snapshots = list_task_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "agent-large");
        assert_eq!(snapshots[0].output_len, 1024 * 1024);
        assert_eq!(snapshots[0].task_type.as_deref(), Some("background_agent"));
    }

    #[test]
    fn background_agent_finish_notifies_subscribers_without_output_payload() {
        let _guard = test_store_guard();
        let rx = subscribe_task_events();
        create_background_agent_task(
            "agent-done".to_string(),
            "agent-done".to_string(),
            "general".to_string(),
            "scan repo".to_string(),
            "prompt".to_string(),
            ".".to_string(),
        );
        finish_background_agent_task(
            "agent-done",
            "completed",
            "private output".to_string(),
            Some(0),
        );

        let event = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("completion event");
        assert_eq!(event.id, "agent-done");
        assert_eq!(event.status, "completed");
        assert_eq!(event.subject, "scan repo");
        assert_eq!(event.task_type.as_deref(), Some("background_agent"));
        assert_eq!(event.exit_code, Some(0));
    }

    #[test]
    fn stop_then_finish_emits_single_terminal_event() {
        let _guard = test_store_guard();
        let rx = subscribe_task_events();
        create_background_agent_task(
            "agent-cancel".to_string(),
            "agent-cancel".to_string(),
            "general".to_string(),
            "cancel scan".to_string(),
            "prompt".to_string(),
            ".".to_string(),
        );

        stop_background_task("agent-cancel");
        finish_background_agent_task(
            "agent-cancel",
            "completed",
            "late child output".to_string(),
            Some(0),
        );

        let event = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("cancel event");
        assert_eq!(event.id, "agent-cancel");
        assert_eq!(event.status, "cancelled");
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "late child finish should not emit a second terminal event"
        );
    }
}
