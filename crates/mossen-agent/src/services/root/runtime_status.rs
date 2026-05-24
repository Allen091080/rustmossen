//! Runtime status snapshot for stream-json slash `/status`.
//!
//! This module intentionally keeps a small, non-blocking process-local view of
//! the agent loop. It is observability data only; execution must never depend
//! on these counters.

use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeStatusSnapshot {
    pub active_dialogues: u64,
    pub total_dialogues_started: u64,
    pub total_dialogues_completed: u64,
    pub total_dialogues_failed: u64,
    pub total_tool_calls_started: u64,
    pub total_tool_calls_completed: u64,
    pub total_tool_calls_failed: u64,
    pub total_tool_calls_denied: u64,
    pub total_permission_decisions: u64,
    pub permission_mode_decisions: u64,
    pub permission_gate_decisions: u64,
    pub permission_not_required_decisions: u64,
    pub permission_allows: u64,
    pub permission_allow_always: u64,
    pub permission_denies: u64,
    pub last_session_id: Option<String>,
    pub active_session_id: Option<String>,
    pub last_model: Option<String>,
    pub active_model: Option<String>,
    pub last_tool_name: Option<String>,
    pub last_tool_status: Option<String>,
    pub last_tool_started_at_ms: Option<u64>,
    pub last_tool_finished_at_ms: Option<u64>,
    pub last_permission_tool_name: Option<String>,
    pub last_permission_source: Option<String>,
    pub last_permission_decision: Option<String>,
    pub last_started_at_ms: Option<u64>,
    pub last_finished_at_ms: Option<u64>,
    pub last_terminal_reason: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct AgentRuntimeStatus {
    active_dialogues: u64,
    total_dialogues_started: u64,
    total_dialogues_completed: u64,
    total_dialogues_failed: u64,
    total_tool_calls_started: u64,
    total_tool_calls_completed: u64,
    total_tool_calls_failed: u64,
    total_tool_calls_denied: u64,
    total_permission_decisions: u64,
    permission_mode_decisions: u64,
    permission_gate_decisions: u64,
    permission_not_required_decisions: u64,
    permission_allows: u64,
    permission_allow_always: u64,
    permission_denies: u64,
    last_session_id: Option<String>,
    active_session_id: Option<String>,
    last_model: Option<String>,
    active_model: Option<String>,
    last_tool_name: Option<String>,
    last_tool_status: Option<String>,
    last_tool_started_at_ms: Option<u64>,
    last_tool_finished_at_ms: Option<u64>,
    last_permission_tool_name: Option<String>,
    last_permission_source: Option<String>,
    last_permission_decision: Option<String>,
    last_started_at_ms: Option<u64>,
    last_finished_at_ms: Option<u64>,
    last_terminal_reason: Option<String>,
    last_error: Option<String>,
}

static STATUS: Lazy<Mutex<AgentRuntimeStatus>> =
    Lazy::new(|| Mutex::new(AgentRuntimeStatus::default()));

pub fn record_agent_dialogue_start(session_id: &str, model: &str) {
    let mut status = STATUS.lock().unwrap();
    status.active_dialogues = status.active_dialogues.saturating_add(1);
    status.total_dialogues_started = status.total_dialogues_started.saturating_add(1);
    status.last_session_id = Some(session_id.to_string());
    status.active_session_id = Some(session_id.to_string());
    status.last_model = Some(model.to_string());
    status.active_model = Some(model.to_string());
    status.last_started_at_ms = Some(now_ms());
    status.last_terminal_reason = None;
    status.last_error = None;
}

pub fn record_agent_dialogue_finish(terminal_reason: Option<&str>, error: Option<&str>) {
    let mut status = STATUS.lock().unwrap();
    status.active_dialogues = status.active_dialogues.saturating_sub(1);
    if error.is_some() {
        status.total_dialogues_failed = status.total_dialogues_failed.saturating_add(1);
    } else {
        status.total_dialogues_completed = status.total_dialogues_completed.saturating_add(1);
    }
    if status.active_dialogues == 0 {
        status.active_session_id = None;
        status.active_model = None;
    }
    status.last_finished_at_ms = Some(now_ms());
    status.last_terminal_reason = terminal_reason.map(str::to_string);
    status.last_error = error.map(str::to_string);
}

pub fn record_tool_call_start(tool_name: &str) {
    let mut status = STATUS.lock().unwrap();
    status.total_tool_calls_started = status.total_tool_calls_started.saturating_add(1);
    status.last_tool_name = Some(tool_name.to_string());
    status.last_tool_status = Some("running".to_string());
    status.last_tool_started_at_ms = Some(now_ms());
}

pub fn record_tool_call_finish(tool_name: &str, outcome: &str) {
    let mut status = STATUS.lock().unwrap();
    match outcome {
        "completed" => {
            status.total_tool_calls_completed = status.total_tool_calls_completed.saturating_add(1);
        }
        "denied" => {
            status.total_tool_calls_denied = status.total_tool_calls_denied.saturating_add(1);
        }
        _ => {
            status.total_tool_calls_failed = status.total_tool_calls_failed.saturating_add(1);
        }
    }
    status.last_tool_name = Some(tool_name.to_string());
    status.last_tool_status = Some(outcome.to_string());
    status.last_tool_finished_at_ms = Some(now_ms());
}

pub fn record_tool_permission_decision(tool_name: &str, source: &str, decision: &str) {
    let mut status = STATUS.lock().unwrap();
    status.total_permission_decisions = status.total_permission_decisions.saturating_add(1);
    match source {
        "permission_mode" => {
            status.permission_mode_decisions = status.permission_mode_decisions.saturating_add(1);
        }
        "permission_gate" => {
            status.permission_gate_decisions = status.permission_gate_decisions.saturating_add(1);
        }
        "not_required" => {
            status.permission_not_required_decisions =
                status.permission_not_required_decisions.saturating_add(1);
        }
        _ => {}
    }
    match decision {
        "allowAlways" => {
            status.permission_allow_always = status.permission_allow_always.saturating_add(1);
        }
        "deny" => {
            status.permission_denies = status.permission_denies.saturating_add(1);
        }
        _ => {
            status.permission_allows = status.permission_allows.saturating_add(1);
        }
    }
    status.last_permission_tool_name = Some(tool_name.to_string());
    status.last_permission_source = Some(source.to_string());
    status.last_permission_decision = Some(decision.to_string());
}

pub fn snapshot_agent_runtime_status() -> AgentRuntimeStatusSnapshot {
    let status = STATUS.lock().unwrap();
    AgentRuntimeStatusSnapshot {
        active_dialogues: status.active_dialogues,
        total_dialogues_started: status.total_dialogues_started,
        total_dialogues_completed: status.total_dialogues_completed,
        total_dialogues_failed: status.total_dialogues_failed,
        total_tool_calls_started: status.total_tool_calls_started,
        total_tool_calls_completed: status.total_tool_calls_completed,
        total_tool_calls_failed: status.total_tool_calls_failed,
        total_tool_calls_denied: status.total_tool_calls_denied,
        total_permission_decisions: status.total_permission_decisions,
        permission_mode_decisions: status.permission_mode_decisions,
        permission_gate_decisions: status.permission_gate_decisions,
        permission_not_required_decisions: status.permission_not_required_decisions,
        permission_allows: status.permission_allows,
        permission_allow_always: status.permission_allow_always,
        permission_denies: status.permission_denies,
        last_session_id: status.last_session_id.clone(),
        active_session_id: status.active_session_id.clone(),
        last_model: status.last_model.clone(),
        active_model: status.active_model.clone(),
        last_tool_name: status.last_tool_name.clone(),
        last_tool_status: status.last_tool_status.clone(),
        last_tool_started_at_ms: status.last_tool_started_at_ms,
        last_tool_finished_at_ms: status.last_tool_finished_at_ms,
        last_permission_tool_name: status.last_permission_tool_name.clone(),
        last_permission_source: status.last_permission_source.clone(),
        last_permission_decision: status.last_permission_decision.clone(),
        last_started_at_ms: status.last_started_at_ms,
        last_finished_at_ms: status.last_finished_at_ms,
        last_terminal_reason: status.last_terminal_reason.clone(),
        last_error: status.last_error.clone(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
fn reset_agent_runtime_status_for_tests() {
    let mut status = STATUS.lock().unwrap();
    *status = AgentRuntimeStatus::default();
}

#[cfg(test)]
mod tests {
    use super::{
        record_agent_dialogue_finish, record_agent_dialogue_start, record_tool_call_finish,
        record_tool_call_start, record_tool_permission_decision,
        reset_agent_runtime_status_for_tests, snapshot_agent_runtime_status,
    };

    #[test]
    fn runtime_status_tracks_start_and_finish() {
        reset_agent_runtime_status_for_tests();

        record_agent_dialogue_start("session-1", "model-a");
        let running = snapshot_agent_runtime_status();
        assert_eq!(running.active_dialogues, 1);
        assert_eq!(running.total_dialogues_started, 1);
        assert_eq!(running.active_session_id.as_deref(), Some("session-1"));
        assert_eq!(running.active_model.as_deref(), Some("model-a"));

        record_agent_dialogue_finish(Some("Completed"), None);
        let finished = snapshot_agent_runtime_status();
        assert_eq!(finished.active_dialogues, 0);
        assert_eq!(finished.total_dialogues_completed, 1);
        assert_eq!(finished.total_dialogues_failed, 0);
        assert_eq!(finished.active_session_id, None);
        assert_eq!(finished.last_terminal_reason.as_deref(), Some("Completed"));
    }

    #[test]
    fn runtime_status_tracks_tool_and_permission_decisions() {
        reset_agent_runtime_status_for_tests();

        record_tool_call_start("Bash");
        record_tool_permission_decision("Bash", "permission_gate", "allowAlways");
        record_tool_call_finish("Bash", "completed");
        record_tool_call_start("Edit");
        record_tool_permission_decision("Edit", "permission_mode", "deny");
        record_tool_call_finish("Edit", "denied");

        let snapshot = snapshot_agent_runtime_status();
        assert_eq!(snapshot.total_tool_calls_started, 2);
        assert_eq!(snapshot.total_tool_calls_completed, 1);
        assert_eq!(snapshot.total_tool_calls_denied, 1);
        assert_eq!(snapshot.total_permission_decisions, 2);
        assert_eq!(snapshot.permission_gate_decisions, 1);
        assert_eq!(snapshot.permission_mode_decisions, 1);
        assert_eq!(snapshot.permission_allow_always, 1);
        assert_eq!(snapshot.permission_denies, 1);
        assert_eq!(snapshot.last_tool_name.as_deref(), Some("Edit"));
        assert_eq!(snapshot.last_tool_status.as_deref(), Some("denied"));
        assert_eq!(
            snapshot.last_permission_source.as_deref(),
            Some("permission_mode")
        );
    }
}
