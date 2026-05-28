//! Pending clear-conversation request buffer.
//!
//! Single-slot buffer for stream-json `/clear --confirm` control requests.
//! The control_request handler enqueues here; the dialogue loop safe point
//! dequeues and executes.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Timeout for queued clear requests.
pub const CLEAR_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// A pending clear-conversation request.
#[derive(Debug, Clone)]
pub struct PendingClearRequest {
    pub request_id: String,
    pub dry_run: bool,
    pub enqueued_at: Instant,
}

/// Global pending request slot.
static PENDING_REQUEST: Mutex<Option<PendingClearRequest>> = Mutex::new(None);

/// Enqueue a clear request. Returns Err if a request is already pending.
pub fn enqueue_pending_clear_request(request_id: String, dry_run: bool) -> Result<(), String> {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    if slot.is_some() {
        return Err("another clear request is already pending".to_string());
    }
    *slot = Some(PendingClearRequest {
        request_id,
        dry_run,
        enqueued_at: Instant::now(),
    });
    Ok(())
}

/// Dequeue the pending request. Returns None if none pending.
/// If timed out, still returns the request (caller checks the enqueue time).
pub fn dequeue_pending_clear_request() -> Option<PendingClearRequest> {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    slot.take()
}

/// Peek at the pending request without dequeuing.
pub fn get_pending_clear_request() -> Option<PendingClearRequest> {
    let slot = PENDING_REQUEST.lock().unwrap();
    slot.clone()
}

/// Check whether a pending request exists.
pub fn has_pending_clear_request() -> bool {
    let slot = PENDING_REQUEST.lock().unwrap();
    slot.is_some()
}

/// Check whether the pending request has timed out.
pub fn has_clear_request_timed_out() -> bool {
    let slot = PENDING_REQUEST.lock().unwrap();
    match &*slot {
        Some(req) => req.enqueued_at.elapsed() > CLEAR_REQUEST_TIMEOUT,
        None => false,
    }
}

/// Clear the pending request unconditionally.
pub fn clear_pending_clear_request() {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    *slot = None;
}
