//! # session_state — 会话状态管理
//!
//! 对应 TypeScript `utils/sessionState.ts`。
//! 提供会话状态变更通知、元数据变更通知和权限模式变更通知功能。

use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 会话运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Running,
    RequiresAction,
}

/// 需要动作时的详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiresActionDetails {
    pub tool_name: String,
    /// Human-readable summary, e.g. "Editing src/foo.ts", "Running npm test"
    pub action_description: String,
    pub tool_use_id: String,
    pub request_id: String,
    /// Raw tool input for frontend parsing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<HashMap<String, serde_json::Value>>,
}

/// 会话外部元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionExternalMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ultraplan_mode: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<Option<RequiresActionDetails>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_turn_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_summary: Option<Option<String>>,
}

/// 权限模式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Default,
    Plan,
    AutoAccept,
    BypassPermissions,
}

type SessionStateChangedListener =
    Arc<dyn Fn(SessionState, Option<&RequiresActionDetails>) + Send + Sync>;
type SessionMetadataChangedListener =
    Arc<dyn Fn(&SessionExternalMetadata) + Send + Sync>;
type PermissionModeChangedListener = Arc<dyn Fn(&PermissionMode) + Send + Sync>;

/// 全局会话状态管理器
pub struct SessionStateManager {
    state_listener: Mutex<Option<SessionStateChangedListener>>,
    metadata_listener: Mutex<Option<SessionMetadataChangedListener>>,
    permission_mode_listener: Mutex<Option<PermissionModeChangedListener>>,
    has_pending_action: Mutex<bool>,
    current_state: Mutex<SessionState>,
    emit_state_events: bool,
}

impl SessionStateManager {
    pub fn new(emit_state_events: bool) -> Self {
        Self {
            state_listener: Mutex::new(None),
            metadata_listener: Mutex::new(None),
            permission_mode_listener: Mutex::new(None),
            has_pending_action: Mutex::new(false),
            current_state: Mutex::new(SessionState::Idle),
            emit_state_events,
        }
    }

    pub fn set_session_state_changed_listener(
        &self,
        cb: Option<SessionStateChangedListener>,
    ) {
        *self.state_listener.lock().unwrap() = cb;
    }

    pub fn set_session_metadata_changed_listener(
        &self,
        cb: Option<SessionMetadataChangedListener>,
    ) {
        *self.metadata_listener.lock().unwrap() = cb;
    }

    /// Register a listener for permission-mode changes from onChangeAppState.
    /// Wired by print.ts to emit an SDK system:status message so CCR/IDE clients
    /// see mode transitions in real time.
    pub fn set_permission_mode_changed_listener(
        &self,
        cb: Option<PermissionModeChangedListener>,
    ) {
        *self.permission_mode_listener.lock().unwrap() = cb;
    }

    pub fn get_session_state(&self) -> SessionState {
        *self.current_state.lock().unwrap()
    }

    pub fn notify_session_state_changed(
        &self,
        state: SessionState,
        details: Option<&RequiresActionDetails>,
    ) {
        *self.current_state.lock().unwrap() = state;

        if let Some(listener) = self.state_listener.lock().unwrap().as_ref() {
            listener(state, details);
        }

        // Mirror details into external_metadata so GetSession carries the
        // pending-action context without proto changes.
        let mut has_pending = self.has_pending_action.lock().unwrap();
        if state == SessionState::RequiresAction {
            if let Some(d) = details {
                *has_pending = true;
                if let Some(meta_listener) =
                    self.metadata_listener.lock().unwrap().as_ref()
                {
                    let meta = SessionExternalMetadata {
                        pending_action: Some(Some(d.clone())),
                        ..Default::default()
                    };
                    meta_listener(&meta);
                }
            }
        } else if *has_pending {
            *has_pending = false;
            if let Some(meta_listener) =
                self.metadata_listener.lock().unwrap().as_ref()
            {
                let meta = SessionExternalMetadata {
                    pending_action: Some(None),
                    ..Default::default()
                };
                meta_listener(&meta);
            }
        }

        // task_summary is written mid-turn by the forked summarizer; clear it at
        // idle so the next turn doesn't briefly show the previous turn's progress.
        if state == SessionState::Idle {
            if let Some(meta_listener) =
                self.metadata_listener.lock().unwrap().as_ref()
            {
                let meta = SessionExternalMetadata {
                    task_summary: Some(None),
                    ..Default::default()
                };
                meta_listener(&meta);
            }
        }

        // Mirror to the SDK event stream if enabled
        if self.emit_state_events {
            // In production, this would call enqueueSdkEvent
            let _ = state; // event emission handled elsewhere
        }
    }

    pub fn notify_session_metadata_changed(&self, metadata: &SessionExternalMetadata) {
        if let Some(listener) = self.metadata_listener.lock().unwrap().as_ref() {
            listener(metadata);
        }
    }

    /// Fired by onChangeAppState when toolPermissionContext.mode changes.
    pub fn notify_permission_mode_changed(&self, mode: &PermissionMode) {
        if let Some(listener) =
            self.permission_mode_listener.lock().unwrap().as_ref()
        {
            listener(mode);
        }
    }
}
