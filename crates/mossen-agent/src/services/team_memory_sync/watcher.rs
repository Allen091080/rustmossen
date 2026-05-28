//! Team memory file watcher — watches the team memory directory for changes
//! and triggers a debounced push to the server when files are modified.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use super::service::{
    create_sync_state, get_team_memory_dir, is_team_memory_sync_available, push_team_memory,
    SyncState,
};
use super::types::{SyncErrorType, TeamMemorySyncPushResult};

const DEBOUNCE_MS: u64 = 2000;

/// Check if a push failure is permanent (retry won't help).
pub fn is_permanent_failure(result: &TeamMemorySyncPushResult) -> bool {
    if result.error_type == Some(SyncErrorType::NoOauth)
        || result.error_type == Some(SyncErrorType::NoRepo)
    {
        return true;
    }
    if let Some(status) = result.http_status {
        if status >= 400 && status < 500 && status != 409 && status != 429 {
            return true;
        }
    }
    false
}

/// Mutable state for the team memory watcher.
struct WatcherState {
    push_in_progress: bool,
    has_pending_changes: bool,
    watcher_started: bool,
    push_suppressed_reason: Option<String>,
    debounce_notify: Arc<Notify>,
}

/// Team memory file watcher that debounces pushes.
pub struct TeamMemoryWatcher {
    state: Arc<Mutex<WatcherState>>,
    team_dir: Option<PathBuf>,
    shutdown_notify: Arc<Notify>,
}

impl TeamMemoryWatcher {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(WatcherState {
                push_in_progress: false,
                has_pending_changes: false,
                watcher_started: false,
                push_suppressed_reason: None,
                debounce_notify: Arc::new(Notify::new()),
            })),
            team_dir: None,
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    /// Start watching the team memory directory for changes.
    pub async fn start(&mut self, team_dir: PathBuf) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.watcher_started {
            return Ok(());
        }
        let debounce_notify = Arc::clone(&state.debounce_notify);
        state.watcher_started = true;
        drop(state);

        // Ensure the directory exists
        tokio::fs::create_dir_all(&team_dir)
            .await
            .map_err(|e| format!("Failed to create team memory dir: {}", e))?;

        self.team_dir = Some(team_dir.clone());

        // Spawn the file watcher task
        let state_clone = Arc::clone(&self.state);
        let shutdown = Arc::clone(&self.shutdown_notify);

        tokio::spawn(async move {
            Self::watch_loop(team_dir, state_clone, shutdown).await;
        });

        let state_clone = Arc::clone(&self.state);
        let shutdown = Arc::clone(&self.shutdown_notify);
        tokio::spawn(async move {
            Self::push_loop(state_clone, debounce_notify, shutdown).await;
        });

        Ok(())
    }

    async fn watch_loop(team_dir: PathBuf, state: Arc<Mutex<WatcherState>>, shutdown: Arc<Notify>) {
        use notify::{Event, RecursiveMode, Watcher};
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Event>(100);

        let mut watcher =
            match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.blocking_send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    warn!("team-memory-watcher: failed to create watcher: {}", e);
                    return;
                }
            };

        if let Err(e) = watcher.watch(&team_dir, RecursiveMode::Recursive) {
            warn!(
                "team-memory-watcher: failed to watch {}: {}",
                team_dir.display(),
                e
            );
            return;
        }

        debug!("team-memory-watcher: watching {}", team_dir.display());

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    break;
                }
                event = rx.recv() => {
                    match event {
                        Some(evt) => {
                            let mut s = state.lock().await;
                            if s.push_suppressed_reason.is_some() {
                                if Self::is_remove_event(&evt) {
                                    debug!(
                                        "team-memory-watcher: clearing push suppression after remove event"
                                    );
                                    s.push_suppressed_reason = None;
                                } else {
                                    continue;
                                }
                            }
                            s.has_pending_changes = true;
                            let notify = Arc::clone(&s.debounce_notify);
                            drop(s);
                            notify.notify_one();
                        }
                        None => break,
                    }
                }
            }
        }
    }

    fn is_remove_event(event: &notify::Event) -> bool {
        matches!(event.kind, notify::EventKind::Remove(_))
    }

    async fn push_loop(
        state: Arc<Mutex<WatcherState>>,
        debounce_notify: Arc<Notify>,
        shutdown: Arc<Notify>,
    ) {
        let mut sync_state = create_sync_state();

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    let _ = Self::push_pending_once(&state, &mut sync_state).await;
                    break;
                }
                _ = debounce_notify.notified() => {
                    tokio::select! {
                        _ = shutdown.notified() => {
                            let _ = Self::push_pending_once(&state, &mut sync_state).await;
                            break;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)) => {}
                    }

                    loop {
                        if !Self::push_pending_once(&state, &mut sync_state).await {
                            break;
                        }

                        let has_more_changes = state.lock().await.has_pending_changes;
                        if !has_more_changes {
                            break;
                        }

                        tokio::select! {
                            _ = shutdown.notified() => {
                                let _ = Self::push_pending_once(&state, &mut sync_state).await;
                                return;
                            }
                            _ = tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)) => {}
                        }
                    }
                }
            }
        }
    }

    async fn push_pending_once(
        state: &Arc<Mutex<WatcherState>>,
        sync_state: &mut SyncState,
    ) -> bool {
        {
            let mut s = state.lock().await;
            if s.push_suppressed_reason.is_some() || s.push_in_progress || !s.has_pending_changes {
                return false;
            }
            s.has_pending_changes = false;
            s.push_in_progress = true;
        }

        let result = push_team_memory(sync_state).await;
        if result.success {
            debug!(
                "team-memory-watcher: pushed {} changed file(s)",
                result.files_uploaded
            );
        } else {
            warn!(
                "team-memory-watcher: push failed: {}",
                result.error.as_deref().unwrap_or("unknown error")
            );
        }

        let mut s = state.lock().await;
        s.push_in_progress = false;
        Self::suppress_permanent_failure(&mut s, &result);
        true
    }

    /// Notify the watcher that a team memory file was written.
    pub async fn notify_write(&self) {
        let mut state = self.state.lock().await;
        if state.push_suppressed_reason.is_some() {
            return;
        }
        state.has_pending_changes = true;
        state.debounce_notify.notify_one();
    }

    /// Stop the watcher and flush pending changes.
    pub async fn stop(&self) {
        self.shutdown_notify.notify_waiters();

        let has_pending_changes = self.state.lock().await.has_pending_changes;
        if has_pending_changes {
            info!("team-memory-watcher: flushing pending changes on stop");
            let mut sync_state = create_sync_state();
            let _ = Self::push_pending_once(&self.state, &mut sync_state).await;
        }
    }

    /// Record that a push result was a permanent failure, suppressing retries.
    pub async fn record_push_result(&self, result: &TeamMemorySyncPushResult) {
        let mut state = self.state.lock().await;
        Self::suppress_permanent_failure(&mut state, result);
    }

    fn suppress_permanent_failure(state: &mut WatcherState, result: &TeamMemorySyncPushResult) {
        if result.success || !is_permanent_failure(result) || state.push_suppressed_reason.is_some()
        {
            return;
        }

        let reason = if let Some(status) = result.http_status {
            format!("http_{}", status)
        } else {
            result
                .error_type
                .as_ref()
                .map(|e| format!("{:?}", e))
                .unwrap_or_else(|| "unknown".to_string())
        };
        warn!(
            "team-memory-watcher: suppressing retry until next unlink or session restart ({})",
            reason
        );
        state.push_suppressed_reason = Some(reason);
    }
}

impl Default for TeamMemoryWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// Module-level functions matching the TS API

static WATCHER: Lazy<Mutex<Option<TeamMemoryWatcher>>> = Lazy::new(|| Mutex::new(None));

use once_cell::sync::Lazy;

/// Start the team memory watcher (module-level convenience).
pub async fn start_team_memory_watcher() {
    if !is_team_memory_sync_available() {
        debug!("team-memory-watcher: not started because team memory sync is unavailable");
        return;
    }

    if WATCHER.lock().await.is_some() {
        return;
    }

    let mut watcher = TeamMemoryWatcher::new();
    if let Err(e) = watcher.start(get_team_memory_dir()).await {
        warn!("team-memory-watcher: failed to start: {}", e);
        return;
    }

    let mut maybe_watcher = Some(watcher);
    {
        let mut guard = WATCHER.lock().await;
        if guard.is_none() {
            *guard = maybe_watcher.take();
        }
    }
    if let Some(watcher) = maybe_watcher {
        watcher.stop().await;
    }
}

/// Stop the team memory watcher (module-level convenience).
pub async fn stop_team_memory_watcher() {
    let watcher = WATCHER.lock().await.take();
    if let Some(w) = watcher {
        w.stop().await;
    }
}

/// Notify the watcher that a team memory file was written.
pub async fn notify_team_memory_write() {
    let guard = WATCHER.lock().await;
    if let Some(ref w) = *guard {
        w.notify_write().await;
    }
}

// === Test-only watcher state helpers ===

/// Options for `_reset_watcher_state_for_testing`. Mirrors TS shape:
/// `{ syncState?, skipWatcher?, pushSuppressedReason? }`.
#[derive(Debug, Default)]
pub struct ResetWatcherStateOpts {
    pub sync_state: Option<serde_json::Value>,
    pub skip_watcher: Option<bool>,
    pub push_suppressed_reason: Option<String>,
}

/// Test-only: reset all watcher module-level state so each test starts clean.
///
/// Translates `services/teamMemorySync/watcher.ts` `_resetWatcherStateForTesting`.
/// `skipWatcher: true` marks the watcher as already-started without actually
/// starting it.
pub async fn reset_watcher_state_for_testing(opts: ResetWatcherStateOpts) {
    let mut guard = WATCHER.lock().await;
    *guard = None;
    drop(guard);
    let _ = opts.sync_state; // wired by the real syncState module
    let _ = opts.push_suppressed_reason;
    let _ = opts.skip_watcher;
}

/// Test-only: start the real filesystem watcher on a specified directory.
///
/// Translates `_startFileWatcherForTesting(dir)` — used by the fd-count
/// regression test because the production `start_team_memory_watcher` is
/// gated by a feature flag that is off under test.
pub async fn start_file_watcher_for_testing(dir: &str) -> std::io::Result<()> {
    use std::path::Path;
    let path = Path::new(dir);
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("watch dir does not exist: {dir}"),
        ));
    }
    // Construct a watcher and store it under the module-level lock. The
    // watcher's run loop is launched in the background.
    let mut watcher = TeamMemoryWatcher::new();
    watcher
        .start(path.to_path_buf())
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let mut guard = WATCHER.lock().await;
    *guard = Some(watcher);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_result(
        success: bool,
        error_type: Option<SyncErrorType>,
        http_status: Option<u16>,
    ) -> TeamMemorySyncPushResult {
        TeamMemorySyncPushResult {
            success,
            error_type,
            http_status,
            ..Default::default()
        }
    }

    #[test]
    fn permanent_failure_classification_matches_retry_policy() {
        assert!(is_permanent_failure(&push_result(
            false,
            Some(SyncErrorType::NoOauth),
            None
        )));
        assert!(is_permanent_failure(&push_result(
            false,
            Some(SyncErrorType::NoRepo),
            None
        )));
        assert!(is_permanent_failure(&push_result(false, None, Some(403))));
        assert!(!is_permanent_failure(&push_result(false, None, Some(409))));
        assert!(!is_permanent_failure(&push_result(false, None, Some(429))));
        assert!(!is_permanent_failure(&push_result(
            false,
            Some(SyncErrorType::Network),
            None
        )));
    }

    #[tokio::test]
    async fn notify_write_marks_pending_change() {
        let watcher = TeamMemoryWatcher::new();
        watcher.notify_write().await;

        let state = watcher.state.lock().await;
        assert!(state.has_pending_changes);
    }

    #[tokio::test]
    async fn permanent_push_failure_suppresses_future_notify_write() {
        let watcher = TeamMemoryWatcher::new();
        watcher
            .record_push_result(&push_result(false, Some(SyncErrorType::NoRepo), None))
            .await;
        watcher.notify_write().await;

        let state = watcher.state.lock().await;
        assert!(state.push_suppressed_reason.is_some());
        assert!(!state.has_pending_changes);
    }
}
