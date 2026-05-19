use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

/// Reason for session activity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionActivityReason {
    ApiCall,
    ToolExec,
}

impl SessionActivityReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionActivityReason::ApiCall => "api_call",
            SessionActivityReason::ToolExec => "tool_exec",
        }
    }
}

struct SessionActivityState {
    activity_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    refcount: u32,
    active_reasons: HashMap<SessionActivityReason, u32>,
    oldest_activity_started_at: Option<Instant>,
    heartbeat_active: bool,
    idle_timer_active: bool,
    send_keepalives: bool,
}

static STATE: Lazy<Mutex<SessionActivityState>> = Lazy::new(|| {
    Mutex::new(SessionActivityState {
        activity_callback: None,
        refcount: 0,
        active_reasons: HashMap::new(),
        oldest_activity_started_at: None,
        heartbeat_active: false,
        idle_timer_active: false,
        send_keepalives: false,
    })
});

/// Register the session activity callback (keep-alive sender)
pub fn register_session_activity_callback(
    cb: Arc<dyn Fn() + Send + Sync>,
    send_keepalives: bool,
) {
    let mut state = STATE.lock().unwrap();
    state.activity_callback = Some(cb);
    state.send_keepalives = send_keepalives;
    // Restart timer if work is already in progress
    if state.refcount > 0 && !state.heartbeat_active {
        state.heartbeat_active = true;
    }
}

/// Unregister the session activity callback
pub fn unregister_session_activity_callback() {
    let mut state = STATE.lock().unwrap();
    state.activity_callback = None;
    state.heartbeat_active = false;
    state.idle_timer_active = false;
}

/// Send a keepalive signal immediately
pub fn send_session_activity_signal() {
    let state = STATE.lock().unwrap();
    if state.send_keepalives {
        if let Some(ref cb) = state.activity_callback {
            cb();
        }
    }
}

/// Check if session activity tracking is active
pub fn is_session_activity_tracking_active() -> bool {
    let state = STATE.lock().unwrap();
    state.activity_callback.is_some()
}

/// Increment the activity refcount. When it transitions from 0→1 and a callback
/// is registered, start a periodic heartbeat timer.
pub fn start_session_activity(reason: SessionActivityReason) {
    let mut state = STATE.lock().unwrap();
    state.refcount += 1;
    *state.active_reasons.entry(reason).or_insert(0) += 1;
    if state.refcount == 1 {
        state.oldest_activity_started_at = Some(Instant::now());
        if state.activity_callback.is_some() && !state.heartbeat_active {
            state.heartbeat_active = true;
            state.idle_timer_active = false;
        }
    }
}

/// Decrement the activity refcount. When it reaches 0, stop the heartbeat timer
/// and start an idle timer.
pub fn stop_session_activity(reason: SessionActivityReason) {
    let mut state = STATE.lock().unwrap();
    if state.refcount > 0 {
        state.refcount -= 1;
    }
    let n = state.active_reasons.get(&reason).copied().unwrap_or(0);
    if n > 1 {
        state.active_reasons.insert(reason, n - 1);
    } else {
        state.active_reasons.remove(&reason);
    }
    if state.refcount == 0 && state.heartbeat_active {
        state.heartbeat_active = false;
        state.idle_timer_active = true;
    }
}

/// Get current refcount (for diagnostics)
pub fn get_session_activity_refcount() -> u32 {
    let state = STATE.lock().unwrap();
    state.refcount
}

/// Get active reasons as a map of reason -> count (for diagnostics)
pub fn get_active_reasons() -> HashMap<SessionActivityReason, u32> {
    let state = STATE.lock().unwrap();
    state.active_reasons.clone()
}

/// Get duration since oldest activity started (for diagnostics)
pub fn get_oldest_activity_duration() -> Option<Duration> {
    let state = STATE.lock().unwrap();
    if state.refcount > 0 {
        state.oldest_activity_started_at.map(|t| t.elapsed())
    } else {
        None
    }
}
