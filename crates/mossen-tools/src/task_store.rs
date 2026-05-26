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
use std::sync::{Mutex, OnceLock};

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

fn store() -> &'static Mutex<HashMap<String, TaskRecord>> {
    static STORE: OnceLock<Mutex<HashMap<String, TaskRecord>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
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
    let id = uuid::Uuid::new_v4().to_string();
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
    let subject = description
        .clone()
        .unwrap_or_else(|| format!("bash: {}", command.chars().take(80).collect::<String>()));
    let mut record = create_task(subject, command.clone());
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

    update_task(task_id, |r| {
        if !is_terminal_status(&r.status) {
            r.status = "cancelled".to_string();
            r.completed_at = Some(now_ms());
            r.output = append_output_note(&r.output, "Task stopped by request.");
        }
        r.metadata.insert("stoppedAt".to_string(), json!(now_ms()));
    })
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
    update_task(task_id, |r| {
        if !r.status.eq("cancelled") && !r.status.eq("deleted") {
            r.status = status.to_string();
            r.completed_at = Some(now_ms());
        } else if r.completed_at.is_none() {
            r.completed_at = Some(now_ms());
        }
        r.output = output;
        r.exit_code = exit_code;
        r.metadata
            .insert("completedAt".to_string(), json!(now_ms()));
        r.metadata.insert("timedOut".to_string(), json!(timed_out));
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
}
