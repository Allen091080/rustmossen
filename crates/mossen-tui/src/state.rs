//! Global TUI state management.
//!
//! Translates the React Context + AppStateStore pattern into a shared state
//! struct with watch-based change notification.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

use crate::theme::ThemeName;

/// Expanded view mode for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandedView {
    #[default]
    None,
    Tasks,
    Teammates,
}

/// View selection mode for footer navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewSelectionMode {
    #[default]
    Normal,
    Footer,
    Expanded,
}

/// Connection status for remote sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Input mode for the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Bash,
    Vim,
    Search,
    Command,
}

/// Global application state — mirrors the TS AppState store fields.
#[derive(Debug, Clone)]
pub struct AppState {
    // --- Basic settings ---
    pub verbose: bool,
    pub theme: ThemeName,

    // --- UI state ---
    pub expanded_view: ExpandedView,
    pub is_brief_only: bool,
    pub view_selection_mode: ViewSelectionMode,
    pub input_mode: InputMode,

    // --- Session ---
    pub remote_connection_status: ConnectionStatus,
    pub remote_session_url: Option<String>,

    // --- Model ---
    pub current_model: Option<String>,
    pub fast_mode: bool,
    pub thinking_enabled: bool,

    // --- Agent ---
    pub agent_name: Option<String>,

    // --- Notifications ---
    pub notification_count: usize,
    pub active_overlays: HashSet<String>,

    // --- Task state ---
    pub foreground_task_id: Option<String>,
    pub background_task_count: usize,

    // --- Messages ---
    pub message_count: usize,
    pub is_streaming: bool,
    pub is_waiting_for_response: bool,

    // --- Terminal ---
    pub terminal_width: u16,
    pub terminal_height: u16,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            verbose: false,
            theme: ThemeName::default(),
            expanded_view: ExpandedView::default(),
            is_brief_only: false,
            view_selection_mode: ViewSelectionMode::default(),
            input_mode: InputMode::default(),
            remote_connection_status: ConnectionStatus::default(),
            remote_session_url: None,
            current_model: None,
            fast_mode: false,
            thinking_enabled: true,
            agent_name: None,
            notification_count: 0,
            active_overlays: HashSet::new(),
            foreground_task_id: None,
            background_task_count: 0,
            message_count: 0,
            is_streaming: false,
            is_waiting_for_response: false,
            terminal_width: 80,
            terminal_height: 24,
        }
    }
}

/// Thread-safe state store with change notification.
///
/// Translates the TS `Store<T>` pattern (getState/setState/subscribe).
#[derive(Clone)]
pub struct AppStore {
    state: Arc<RwLock<AppState>>,
    notify_tx: Arc<watch::Sender<u64>>,
    notify_rx: watch::Receiver<u64>,
    version: Arc<std::sync::atomic::AtomicU64>,
}

impl AppStore {
    pub fn new(initial: AppState) -> Self {
        let (notify_tx, notify_rx) = watch::channel(0u64);
        Self {
            state: Arc::new(RwLock::new(initial)),
            notify_tx: Arc::new(notify_tx),
            notify_rx,
            version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Read current state.
    pub async fn get_state(&self) -> AppState {
        self.state.read().await.clone()
    }

    /// Update state with a closure and notify subscribers.
    pub async fn set_state(&self, f: impl FnOnce(&mut AppState)) {
        let mut state = self.state.write().await;
        f(&mut state);
        let v = self
            .version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.notify_tx.send(v + 1);
    }

    /// Subscribe to state changes. Returns a receiver that wakes on each update.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify_rx.clone()
    }
}

impl Default for AppStore {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}
