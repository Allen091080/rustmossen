//! Pending compact request buffer.
//!
//! Single-slot buffer for stream-json compact_conversation control requests.
//! The control_request handler enqueues here; the query loop safe point
//! dequeues and executes.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Timeout for queued compact requests.
pub const COMPACT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// A pending compact request.
#[derive(Debug, Clone)]
pub struct PendingCompactRequest {
    pub request_id: String,
    pub mode: CompactMode,
    pub dry_run: bool,
    pub custom_instructions: Option<String>,
    pub enqueued_at: Instant,
}

/// Compact mode (currently only manual).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactMode {
    Manual,
}

/// Global pending request slot.
static PENDING_REQUEST: Mutex<Option<PendingCompactRequest>> = Mutex::new(None);

/// Enqueue a compact request. Returns Err if a request is already pending.
pub fn enqueue_pending_compact_request(
    request_id: String,
    mode: CompactMode,
    dry_run: bool,
    custom_instructions: Option<String>,
) -> Result<(), String> {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    if slot.is_some() {
        return Err("another compact request is already pending".to_string());
    }
    *slot = Some(PendingCompactRequest {
        request_id,
        mode,
        dry_run,
        custom_instructions,
        enqueued_at: Instant::now(),
    });
    Ok(())
}

/// Dequeue the pending request. Returns None if none pending.
/// If timed out, still returns the request (caller checks via `has_compact_request_timed_out`).
pub fn dequeue_pending_compact_request() -> Option<PendingCompactRequest> {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    slot.take()
}

/// Peek at the pending request without dequeuing.
pub fn get_pending_compact_request() -> Option<PendingCompactRequest> {
    let slot = PENDING_REQUEST.lock().unwrap();
    slot.clone()
}

/// Check whether a pending request exists.
pub fn has_pending_compact_request() -> bool {
    let slot = PENDING_REQUEST.lock().unwrap();
    slot.is_some()
}

/// Check whether the pending request has timed out.
pub fn has_compact_request_timed_out() -> bool {
    let slot = PENDING_REQUEST.lock().unwrap();
    match &*slot {
        Some(req) => req.enqueued_at.elapsed() > COMPACT_REQUEST_TIMEOUT,
        None => false,
    }
}

/// Clear the pending request unconditionally.
pub fn clear_pending_compact_request() {
    let mut slot = PENDING_REQUEST.lock().unwrap();
    *slot = None;
}
