//! Notifications — priority queue notification system.
//!
//! Translates: context/notifications.tsx
//! React hooks → struct-based state with callbacks.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tokio::time::{sleep, Duration};

/// Notification priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Priority {
    Immediate,
    High,
    Medium,
    Low,
}

impl Priority {
    fn rank(&self) -> u8 {
        match self {
            Priority::Immediate => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::Low => 3,
        }
    }
}

/// A notification item.
#[derive(Debug, Clone)]
pub struct Notification {
    pub key: String,
    pub text: String,
    pub priority: Priority,
    pub timeout_ms: Option<u64>,
    /// Keys of notifications that this notification invalidates.
    pub invalidates: Vec<String>,
}

const DEFAULT_TIMEOUT_MS: u64 = 8000;

/// Internal notification state.
#[derive(Debug, Clone)]
struct NotificationState {
    queue: Vec<Notification>,
    current: Option<Notification>,
}

/// Notification manager — manages a priority queue of notifications.
///
/// Translates the React `useNotifications` hook and its internal state
/// into a struct-based async manager.
pub struct NotificationManager {
    state: Arc<Mutex<NotificationState>>,
    change_tx: watch::Sender<u64>,
    change_rx: watch::Receiver<u64>,
    seq: Arc<std::sync::atomic::AtomicU64>,
}

impl NotificationManager {
    pub fn new() -> Self {
        let (change_tx, change_rx) = watch::channel(0);
        Self {
            state: Arc::new(Mutex::new(NotificationState {
                queue: Vec::new(),
                current: None,
            })),
            change_tx,
            change_rx,
            seq: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Add a notification to the queue.
    pub async fn add_notification(&self, notif: Notification) {
        let timeout_ms = notif.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        if notif.priority == Priority::Immediate {
            // Immediate: show right away, re-queue current if non-immediate
            let mut state = self.state.lock().await;
            let old_current = state.current.take();
            // Filter invalidated from queue
            state.queue.retain(|n| {
                n.priority != Priority::Immediate && !notif.invalidates.contains(&n.key)
            });
            // Re-queue old current if non-immediate
            if let Some(old) = old_current {
                if old.priority != Priority::Immediate && !notif.invalidates.contains(&old.key) {
                    state.queue.push(old);
                }
            }
            state.current = Some(notif.clone());
            drop(state);
            self.notify_change();

            // Schedule timeout
            let state_ref = self.state.clone();
            let key = notif.key.clone();
            let self_seq = self.seq.clone();
            let change_tx = self.change_tx.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(timeout_ms)).await;
                let mut s = state_ref.lock().await;
                if s.current.as_ref().map(|c| &c.key) == Some(&key) {
                    s.current = None;
                }
                drop(s);
                let _ = change_tx.send(self_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            });
            return;
        }

        // Non-immediate: add to queue if not duplicate
        let mut state = self.state.lock().await;
        let queued_keys: HashSet<&str> = state.queue.iter().map(|n| n.key.as_str()).collect();
        let current_key = state.current.as_ref().map(|c| c.key.as_str());

        if queued_keys.contains(notif.key.as_str()) || current_key == Some(notif.key.as_str()) {
            return;
        }

        // Check if it invalidates current
        let invalidates_current = state
            .current
            .as_ref()
            .map(|c| notif.invalidates.contains(&c.key))
            .unwrap_or(false);

        if invalidates_current {
            state.current = None;
        }

        state
            .queue
            .retain(|n| n.priority != Priority::Immediate && !notif.invalidates.contains(&n.key));
        state.queue.push(notif);
        drop(state);

        self.notify_change();
        self.process_queue().await;
    }

    /// Remove a notification by key.
    pub async fn remove_notification(&self, key: &str) {
        let mut state = self.state.lock().await;
        let is_current = state.current.as_ref().map(|c| c.key.as_str()) == Some(key);
        let in_queue = state.queue.iter().any(|n| n.key == key);

        if !is_current && !in_queue {
            return;
        }

        if is_current {
            state.current = None;
        }
        state.queue.retain(|n| n.key != key);
        drop(state);
        self.notify_change();
        self.process_queue().await;
    }

    /// Get the current notification being displayed.
    pub async fn current(&self) -> Option<Notification> {
        self.state.lock().await.current.clone()
    }

    /// Subscribe to notification state changes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.change_rx.clone()
    }

    /// Process the queue: show the highest-priority queued notification if none is current.
    async fn process_queue(&self) {
        let mut state = self.state.lock().await;
        if state.current.is_some() || state.queue.is_empty() {
            return;
        }

        let next = get_next(&state.queue);
        if let Some(next_notif) = next {
            let key = next_notif.key.clone();
            let timeout_ms = next_notif.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
            state.queue.retain(|n| n.key != key);
            state.current = Some(next_notif);
            drop(state);

            let state_ref = self.state.clone();
            let self_seq = self.seq.clone();
            let change_tx = self.change_tx.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(timeout_ms)).await;
                let mut s = state_ref.lock().await;
                if s.current.as_ref().map(|c| &c.key) == Some(&key) {
                    s.current = None;
                }
                drop(s);
                let _ = change_tx.send(self_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            });
        }
    }

    fn notify_change(&self) {
        let _ = self
            .change_tx
            .send(self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the highest-priority notification from the queue.
pub fn get_next(queue: &[Notification]) -> Option<Notification> {
    if queue.is_empty() {
        return None;
    }
    queue.iter().min_by_key(|n| n.priority.rank()).cloned()
}

/// React `useNotifications()` snapshot of the notifications manager. Returns
/// the most recently-observed current notification (if any).
pub async fn use_notifications(mgr: &NotificationManager) -> Option<Notification> {
    mgr.current().await
}
