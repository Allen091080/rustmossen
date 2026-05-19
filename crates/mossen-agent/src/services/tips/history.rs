//! Tip history - tracks which tips have been shown and when

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Persistent tip history record
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TipHistory {
    /// Map of tip_id -> last session number when shown
    pub shown: HashMap<String, u32>,
    /// Current session number
    pub current_session: u32,
}

/// Get the tip history file path
fn get_history_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mossen");
    config_dir.join("tip_history.json")
}

/// Load tip history from disk
pub fn load_tip_history() -> TipHistory {
    let path = get_history_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => TipHistory::default(),
    }
}

/// Save tip history to disk
pub fn save_tip_history(history: &TipHistory) {
    let path = get_history_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(history) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Failed to save tip history: {}", e);
            }
        }
        Err(e) => {
            warn!("Failed to serialize tip history: {}", e);
        }
    }
}

/// Get sessions since a tip was last shown
pub fn get_sessions_since_last_shown(tip_id: &str) -> u32 {
    let history = load_tip_history();
    match history.shown.get(tip_id) {
        Some(&last_session) => history.current_session.saturating_sub(last_session),
        None => u32::MAX, // Never shown
    }
}

/// Record that a tip was shown in the current session
pub fn record_tip_shown_in_history(tip_id: &str) {
    let mut history = load_tip_history();
    history.shown.insert(tip_id.to_string(), history.current_session);
    save_tip_history(&history);
}

/// TS `recordTipShown` — load the persistent tip-history, record the tip as
/// shown, and write the updated history back. Mirrors the higher-level API
/// the TS module exposes (the lower-level helper is
/// `record_tip_shown_in_history`).
pub fn record_tip_shown(tip_id: &str) {
    record_tip_shown_in_history(tip_id);
}
