//! ModelMigration notification (model_migration.ts).
//! Notifications for model migration/deprecation events.

#[derive(Debug, Clone)]
pub struct ModelMigrationNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl ModelMigrationNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "model-migration".to_string(),
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

impl Default for ModelMigrationNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Migration-timestamp snapshot from the global config. Timestamps are
/// milliseconds since epoch.
#[derive(Debug, Clone, Default)]
pub struct ModelMigrationConfigSnapshot {
    pub balanced_45_to_46_migration_timestamp: Option<u64>,
    pub legacy_max_migration_timestamp: Option<u64>,
    pub max_pro_migration_timestamp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMigrationNotificationEvent {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
    pub timeout_ms: u64,
}

fn recent(ts: Option<u64>, now: u64) -> bool {
    match ts {
        Some(t) => now.saturating_sub(t) < 3000,
        None => false,
    }
}

/// `useModelMigrationNotifications` — pure-logic translation. Returns the
/// notifications that should fire on startup.
///
/// TS source: `useModelMigrationNotifications()`. The TS version is a
/// thin wrapper around `useStartupNotification` that runs each migration
/// callback once on startup and emits notifications for recent writes.
pub fn use_model_migration_notifications(
    config: &ModelMigrationConfigSnapshot,
    now_ms: u64,
) -> Vec<ModelMigrationNotificationEvent> {
    let mut out = Vec::new();

    if recent(config.balanced_45_to_46_migration_timestamp, now_ms) {
        out.push(ModelMigrationNotificationEvent {
            key: "balanced-46-update".to_string(),
            text: "Model updated to Balanced 4.6".to_string(),
            color: "suggestion".to_string(),
            priority: "high".to_string(),
            timeout_ms: 3000,
        });
    }

    let is_legacy_remap = config.legacy_max_migration_timestamp.is_some();
    let max_ts = config
        .legacy_max_migration_timestamp
        .or(config.max_pro_migration_timestamp);
    if recent(max_ts, now_ms) {
        let text = if is_legacy_remap {
            "Model updated to Max 4.6 · Set MOSSEN_CODE_DISABLE_LEGACY_MODEL_REMAP=1 to opt out"
                .to_string()
        } else {
            "Model updated to Max 4.6".to_string()
        };
        out.push(ModelMigrationNotificationEvent {
            key: "max-pro-update".to_string(),
            text,
            color: "suggestion".to_string(),
            priority: "high".to_string(),
            timeout_ms: if is_legacy_remap { 8000 } else { 3000 },
        });
    }
    out
}
