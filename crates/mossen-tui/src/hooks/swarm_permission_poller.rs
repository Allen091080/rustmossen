//! Swarm Permission Poller hook (useSwarmPermissionPoller.ts).
//! Polls for permission requests from swarm workers.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct SwarmPermissionPollerState {
    pub active: bool,
    pub initialized: bool,
}

impl SwarmPermissionPollerState {
    pub fn new() -> Self { Self { active: false, initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }
}
impl Default for SwarmPermissionPollerState { fn default() -> Self { Self::new() } }

// ============================================================================
// Module-level permission/sandbox callback registries.
// ============================================================================

/// Permission decision from the leader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Approved,
    Rejected,
}

/// Validated permission update — the TS code parses an arbitrary value
/// against a Zod schema, dropping malformed entries. We use an opaque
/// wrapper here; callers convert from their JSON shape via
/// `parse_permission_updates`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionUpdate {
    pub raw: String,
}

/// Decision payload delivered to a callback's onAllow.
#[derive(Debug, Clone)]
pub struct AllowParams {
    pub updated_input: Option<String>,
    pub permission_updates: Vec<PermissionUpdate>,
    pub feedback: Option<String>,
}

/// Callback bundle for one pending permission request. Translated from
/// `PermissionResponseCallback` in TS — uses boxed closures because Rust
/// doesn't support React-style closures-with-captured-state directly.
pub struct PermissionResponseCallback {
    pub request_id: String,
    pub tool_use_id: String,
    pub on_allow: Box<dyn Fn(AllowParams) + Send + Sync>,
    pub on_reject: Box<dyn Fn(Option<String>) + Send + Sync>,
}

impl std::fmt::Debug for PermissionResponseCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionResponseCallback")
            .field("request_id", &self.request_id)
            .field("tool_use_id", &self.tool_use_id)
            .finish()
    }
}

/// Callback bundle for one pending sandbox permission request.
pub struct SandboxPermissionResponseCallback {
    pub request_id: String,
    pub host: String,
    pub resolve: Box<dyn Fn(bool) + Send + Sync>,
}

impl std::fmt::Debug for SandboxPermissionResponseCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxPermissionResponseCallback")
            .field("request_id", &self.request_id)
            .field("host", &self.host)
            .finish()
    }
}

/// Registries (module-level — persist across React renders in TS).
static PENDING_CALLBACKS: Mutex<Option<HashMap<String, PermissionResponseCallback>>> = Mutex::new(None);
static PENDING_SANDBOX_CALLBACKS: Mutex<Option<HashMap<String, SandboxPermissionResponseCallback>>> = Mutex::new(None);

fn with_pending<R>(f: impl FnOnce(&mut HashMap<String, PermissionResponseCallback>) -> R) -> R {
    let mut guard = PENDING_CALLBACKS.lock().expect("PENDING_CALLBACKS poisoned");
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

fn with_pending_sandbox<R>(f: impl FnOnce(&mut HashMap<String, SandboxPermissionResponseCallback>) -> R) -> R {
    let mut guard = PENDING_SANDBOX_CALLBACKS.lock().expect("PENDING_SANDBOX_CALLBACKS poisoned");
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

/// Register a callback for a pending permission request.
///
/// TS source: `registerPermissionCallback(callback)`.
pub fn register_permission_callback(callback: PermissionResponseCallback) {
    with_pending(|m| {
        m.insert(callback.request_id.clone(), callback);
    });
}

/// Unregister a callback (e.g. when the request is resolved locally or
/// times out).
///
/// TS source: `unregisterPermissionCallback(requestId)`.
pub fn unregister_permission_callback(request_id: &str) {
    with_pending(|m| {
        m.remove(request_id);
    });
}

/// Check if a request has a registered callback.
///
/// TS source: `hasPermissionCallback(requestId)`.
pub fn has_permission_callback(request_id: &str) -> bool {
    with_pending(|m| m.contains_key(request_id))
}

/// Clear all pending callbacks (both permission and sandbox).
///
/// TS source: `clearAllPendingCallbacks()`.
pub fn clear_all_pending_callbacks() {
    with_pending(|m| m.clear());
    with_pending_sandbox(|m| m.clear());
}

/// Validate raw permission updates from external sources.
///
/// TS source: `parsePermissionUpdates(raw)`. The TS version validates via
/// Zod; we accept already-serialized JSON strings and drop entries that
/// don't look like a valid object (start with `{`). Callers that have
/// their own deserialization can call this with verified strings to get
/// a typed list back.
pub fn parse_permission_updates(raw: &[String]) -> Vec<PermissionUpdate> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let trimmed = entry.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            continue;
        }
        out.push(PermissionUpdate { raw: entry.clone() });
    }
    out
}

/// Parameters for `process_mailbox_permission_response`.
#[derive(Debug, Clone)]
pub struct MailboxPermissionResponseParams {
    pub request_id: String,
    pub decision: PermissionDecision,
    pub feedback: Option<String>,
    pub updated_input: Option<String>,
    pub permission_updates: Vec<String>,
}

/// Process a permission response from a mailbox message. Returns true if a
/// callback was registered and was invoked.
///
/// TS source: `processMailboxPermissionResponse(params)`.
pub fn process_mailbox_permission_response(params: MailboxPermissionResponseParams) -> bool {
    let callback = with_pending(|m| m.remove(&params.request_id));
    let Some(callback) = callback else {
        return false;
    };
    match params.decision {
        PermissionDecision::Approved => {
            let permission_updates = parse_permission_updates(&params.permission_updates);
            (callback.on_allow)(AllowParams {
                updated_input: params.updated_input,
                permission_updates,
                feedback: params.feedback.clone(),
            });
        }
        PermissionDecision::Rejected => {
            (callback.on_reject)(params.feedback);
        }
    }
    true
}

/// Register a sandbox permission callback.
///
/// TS source: `registerSandboxPermissionCallback(callback)`.
pub fn register_sandbox_permission_callback(callback: SandboxPermissionResponseCallback) {
    with_pending_sandbox(|m| {
        m.insert(callback.request_id.clone(), callback);
    });
}

/// Check if a sandbox request has a registered callback.
///
/// TS source: `hasSandboxPermissionCallback(requestId)`.
pub fn has_sandbox_permission_callback(request_id: &str) -> bool {
    with_pending_sandbox(|m| m.contains_key(request_id))
}

/// Parameters for `process_sandbox_permission_response`.
#[derive(Debug, Clone)]
pub struct SandboxPermissionResponseParams {
    pub request_id: String,
    pub host: String,
    pub allow: bool,
}

/// Process a sandbox permission response. Returns true if a callback was
/// registered and was invoked.
///
/// TS source: `processSandboxPermissionResponse(params)`.
pub fn process_sandbox_permission_response(params: SandboxPermissionResponseParams) -> bool {
    let callback = with_pending_sandbox(|m| m.remove(&params.request_id));
    let Some(callback) = callback else {
        return false;
    };
    (callback.resolve)(params.allow);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[test]
    fn register_and_resolve() {
        clear_all_pending_callbacks();
        let allow_called = Arc::new(AtomicU32::new(0));
        let reject_called = Arc::new(AtomicU32::new(0));
        let a_clone = Arc::clone(&allow_called);
        let r_clone = Arc::clone(&reject_called);
        register_permission_callback(PermissionResponseCallback {
            request_id: "r-1".to_string(),
            tool_use_id: "t-1".to_string(),
            on_allow: Box::new(move |_p| { a_clone.fetch_add(1, Ordering::SeqCst); }),
            on_reject: Box::new(move |_| { r_clone.fetch_add(1, Ordering::SeqCst); }),
        });
        assert!(has_permission_callback("r-1"));
        let ok = process_mailbox_permission_response(MailboxPermissionResponseParams {
            request_id: "r-1".to_string(),
            decision: PermissionDecision::Approved,
            feedback: None,
            updated_input: None,
            permission_updates: vec![],
        });
        assert!(ok);
        assert_eq!(allow_called.load(Ordering::SeqCst), 1);
        assert!(!has_permission_callback("r-1"));
    }

    #[test]
    fn unregister_works() {
        clear_all_pending_callbacks();
        register_permission_callback(PermissionResponseCallback {
            request_id: "r-2".to_string(),
            tool_use_id: "t-2".to_string(),
            on_allow: Box::new(|_| {}),
            on_reject: Box::new(|_| {}),
        });
        assert!(has_permission_callback("r-2"));
        unregister_permission_callback("r-2");
        assert!(!has_permission_callback("r-2"));
    }

    #[test]
    fn sandbox_response_resolves() {
        clear_all_pending_callbacks();
        let resolved = Arc::new(AtomicBool::new(false));
        let clone = Arc::clone(&resolved);
        register_sandbox_permission_callback(SandboxPermissionResponseCallback {
            request_id: "sb-1".to_string(),
            host: "example.com".to_string(),
            resolve: Box::new(move |allow| { clone.store(allow, Ordering::SeqCst); }),
        });
        assert!(has_sandbox_permission_callback("sb-1"));
        let ok = process_sandbox_permission_response(SandboxPermissionResponseParams {
            request_id: "sb-1".to_string(),
            host: "example.com".to_string(),
            allow: true,
        });
        assert!(ok);
        assert!(resolved.load(Ordering::SeqCst));
    }

    #[test]
    fn parse_permission_updates_filters_garbage() {
        let raw = vec!["{\"a\":1}".to_string(), "not-json".to_string(), "{\"b\":2}".to_string()];
        let p = parse_permission_updates(&raw);
        assert_eq!(p.len(), 2);
    }
}
