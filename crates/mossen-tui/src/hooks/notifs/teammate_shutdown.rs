//! TeammateShutdown notification (teammate_shutdown.ts).
//! Notification when a teammate session shuts down.

#[derive(Debug, Clone)]
pub struct TeammateShutdownNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl TeammateShutdownNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "teammate-shutdown".to_string(),
            message: None,
        }
    }

    /// Check conditions and set notification message if needed.
    pub fn check(&mut self, should_show: bool, message: &str) {
        if should_show && !self.shown && !self.dismissed {
            self.shown = true;
            self.message = Some(message.to_string());
        }
    }

    /// Dismiss the notification.
    pub fn dismiss(&mut self) {
        self.dismissed = true;
        self.message = None;
    }

    /// Get the notification message if active.
    pub fn active_message(&self) -> Option<&str> {
        if self.shown && !self.dismissed {
            self.message.as_deref()
        } else {
            None
        }
    }

    /// Reset state for re-evaluation.
    pub fn reset(&mut self) {
        self.shown = false;
        self.message = None;
    }
}

impl Default for TeammateShutdownNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of an in-process teammate task — only `Running` and `Completed`
/// trigger spawn/shutdown notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeammateTaskStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone)]
pub struct TeammateTaskSnapshot {
    pub id: String,
    pub status: TeammateTaskStatus,
    pub is_in_process_teammate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeammateLifecycleEvent {
    pub key: String,
    pub text: String,
    pub priority: String,
    pub timeout_ms: u64,
}

/// In-memory dedupe state — mirrors the TS refs `seenRunningRef` and
/// `seenCompletedRef`. The Rust port asks the caller to thread this
/// through the hook invocation.
#[derive(Debug, Clone, Default)]
pub struct TeammateLifecycleSeen {
    pub seen_running: std::collections::HashSet<String>,
    pub seen_completed: std::collections::HashSet<String>,
}

/// Pluralized spawn / shutdown text. Translated from `makeSpawnNotif` /
/// `makeShutdownNotif` in TS.
fn make_spawn_text(count: u32) -> String {
    if count == 1 {
        "1 agent spawned".to_string()
    } else {
        format!("{} agents spawned", count)
    }
}
fn make_shutdown_text(count: u32) -> String {
    if count == 1 {
        "1 agent shut down".to_string()
    } else {
        format!("{} agents shut down", count)
    }
}

/// `useTeammateLifecycleNotification` — pure-logic translation.
///
/// TS source: `useTeammateLifecycleNotification()`. Returns the list of
/// notifications to fire on this tick. Each spawn / shutdown is reported
/// individually; callers fold consecutive events via `fold` callback on
/// the TS Notification (the Rust port leaves folding to the notification
/// store, but we emit identical text/priority/timeout so the fold logic
/// in the store can dedupe).
pub fn use_teammate_lifecycle_notification(
    is_remote_mode: bool,
    tasks: &[TeammateTaskSnapshot],
    seen: &mut TeammateLifecycleSeen,
) -> Vec<TeammateLifecycleEvent> {
    let mut events = Vec::new();
    if is_remote_mode {
        return events;
    }
    for task in tasks {
        if !task.is_in_process_teammate {
            continue;
        }
        match task.status {
            TeammateTaskStatus::Running if !seen.seen_running.contains(&task.id) => {
                seen.seen_running.insert(task.id.clone());
                events.push(TeammateLifecycleEvent {
                    key: "teammate-spawn".to_string(),
                    text: make_spawn_text(1),
                    priority: "low".to_string(),
                    timeout_ms: 5000,
                });
            }
            TeammateTaskStatus::Completed if !seen.seen_completed.contains(&task.id) => {
                seen.seen_completed.insert(task.id.clone());
                events.push(TeammateLifecycleEvent {
                    key: "teammate-shutdown".to_string(),
                    text: make_shutdown_text(1),
                    priority: "low".to_string(),
                    timeout_ms: 5000,
                });
            }
            _ => {}
        }
    }
    events
}
