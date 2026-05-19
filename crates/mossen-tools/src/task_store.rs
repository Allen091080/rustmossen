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

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    #[serde(rename = "activeForm", default, skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    /// Free-form metadata — JSON object.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
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
            output: String::new(),
            exit_code: None,
        }
    }
}

fn store() -> &'static Mutex<HashMap<String, TaskRecord>> {
    static STORE: OnceLock<Mutex<HashMap<String, TaskRecord>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Insert a new task. Returns the freshly-created record (cloned).
pub fn create_task(subject: String, description: String) -> TaskRecord {
    let id = uuid::Uuid::new_v4().to_string();
    let record = TaskRecord::new(id.clone(), subject, description);
    store().lock().unwrap().insert(id, record.clone());
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

/// Used by tests + `/reset` paths.
pub fn clear() {
    store().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The store is process-global, so unit tests have to run mutually
    /// exclusively or `clear()` from one test races another's writes.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn create_then_get_returns_same_record() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        let r = create_task("write docs".into(), "section: install".into());
        let got = get_task(&r.id).expect("task should be retrievable");
        assert_eq!(got.subject, "write docs");
        assert_eq!(got.status, "pending");
    }

    #[test]
    fn update_mutates_status_and_owner() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
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
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        create_task("a".into(), "".into());
        create_task("b".into(), "".into());
        assert_eq!(list_tasks().len(), 2);
    }
}
