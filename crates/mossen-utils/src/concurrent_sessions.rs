//! Concurrent session management — PID file registry for `mossen ps`.
//!
//! When multiple Mossen sessions run, each registers a PID file.
//! `mossen ps` enumerates them. Stale PID files from crashed sessions are swept.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Session kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Interactive,
    Bg,
    Daemon,
    #[serde(rename = "daemon-worker")]
    DaemonWorker,
}

/// Session status for `mossen ps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Busy,
    Idle,
    Waiting,
}

/// PID file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PidFileContent {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub started_at: u64,
    pub kind: SessionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messaging_socket_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_for: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
}

/// Get the sessions directory path.
pub fn get_sessions_dir(config_home: &Path) -> PathBuf {
    config_home.join("sessions")
}

/// Get the session kind from environment.
pub fn env_session_kind(bg_sessions_enabled: bool) -> Option<SessionKind> {
    if !bg_sessions_enabled {
        return None;
    }
    match std::env::var("MOSSEN_CODE_SESSION_KIND")
        .ok()
        .as_deref()
    {
        Some("bg") => Some(SessionKind::Bg),
        Some("daemon") => Some(SessionKind::Daemon),
        Some("daemon-worker") => Some(SessionKind::DaemonWorker),
        _ => None,
    }
}

/// True when running inside a `mossen --bg` tmux session.
pub fn is_bg_session(bg_sessions_enabled: bool) -> bool {
    env_session_kind(bg_sessions_enabled) == Some(SessionKind::Bg)
}

/// Register a session PID file.
///
/// Returns true if registered, false if skipped (e.g., subagent).
pub async fn register_session(
    config_home: &Path,
    session_id: &str,
    cwd: &str,
    pid: u32,
    agent_id: Option<&str>,
    bg_sessions_enabled: bool,
    uds_inbox_enabled: bool,
) -> Result<bool, String> {
    // Skip teammates/subagents
    if agent_id.is_some() {
        return Ok(false);
    }

    let kind = env_session_kind(bg_sessions_enabled).unwrap_or(SessionKind::Interactive);
    let dir = get_sessions_dir(config_home);
    let pid_file = dir.join(format!("{pid}.json"));

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Failed to create sessions dir: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).await;
    }

    let mut content = PidFileContent {
        pid,
        session_id: session_id.to_string(),
        cwd: cwd.to_string(),
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        kind,
        entrypoint: std::env::var("MOSSEN_CODE_ENTRYPOINT").ok(),
        messaging_socket_path: if uds_inbox_enabled {
            std::env::var("MOSSEN_CODE_MESSAGING_SOCKET").ok()
        } else {
            None
        },
        name: if bg_sessions_enabled {
            std::env::var("MOSSEN_CODE_SESSION_NAME").ok()
        } else {
            None
        },
        log_path: if bg_sessions_enabled {
            std::env::var("MOSSEN_CODE_SESSION_LOG").ok()
        } else {
            None
        },
        agent: if bg_sessions_enabled {
            std::env::var("MOSSEN_CODE_AGENT").ok()
        } else {
            None
        },
        bridge_session_id: None,
        status: None,
        waiting_for: None,
        updated_at: None,
    };

    let json = serde_json::to_string(&content)
        .map_err(|e| format!("Failed to serialize PID file: {e}"))?;

    tokio::fs::write(&pid_file, json)
        .await
        .map_err(|e| format!("Failed to write PID file: {e}"))?;

    Ok(true)
}

/// Update this session's PID file with a patch.
pub async fn update_pid_file(
    config_home: &Path,
    pid: u32,
    patch: serde_json::Value,
) -> Result<(), String> {
    let pid_file = get_sessions_dir(config_home).join(format!("{pid}.json"));
    let raw = tokio::fs::read_to_string(&pid_file)
        .await
        .map_err(|e| format!("Failed to read PID file: {e}"))?;

    let mut data: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse PID file: {e}"))?;

    if let (Some(obj), Some(patch_obj)) = (data.as_object_mut(), patch.as_object()) {
        for (k, v) in patch_obj {
            obj.insert(k.clone(), v.clone());
        }
    }

    let updated =
        serde_json::to_string(&data).map_err(|e| format!("Failed to serialize PID file: {e}"))?;

    tokio::fs::write(&pid_file, updated)
        .await
        .map_err(|e| format!("Failed to write PID file: {e}"))?;

    Ok(())
}

/// Update session name.
pub async fn update_session_name(
    config_home: &Path,
    pid: u32,
    name: &str,
) -> Result<(), String> {
    update_pid_file(config_home, pid, serde_json::json!({ "name": name })).await
}

/// Update session bridge ID.
pub async fn update_session_bridge_id(
    config_home: &Path,
    pid: u32,
    bridge_session_id: Option<&str>,
) -> Result<(), String> {
    update_pid_file(
        config_home,
        pid,
        serde_json::json!({ "bridgeSessionId": bridge_session_id }),
    )
    .await
}

/// Push live activity state for `mossen ps`.
pub async fn update_session_activity(
    config_home: &Path,
    pid: u32,
    status: Option<SessionStatus>,
    waiting_for: Option<&str>,
    bg_sessions_enabled: bool,
) -> Result<(), String> {
    if !bg_sessions_enabled {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut patch = serde_json::json!({ "updatedAt": now });
    if let Some(s) = status {
        patch["status"] = serde_json::to_value(s).unwrap_or_default();
    }
    if let Some(wf) = waiting_for {
        patch["waitingFor"] = serde_json::json!(wf);
    }
    update_pid_file(config_home, pid, patch).await
}

/// Check if a process is running by PID.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // Conservative: assume running
        let _ = pid;
        true
    }
}

/// Count live concurrent CLI sessions.
/// Filters out stale PID files (crashed sessions) and deletes them.
/// Returns 0 on any error.
pub async fn count_concurrent_sessions(
    config_home: &Path,
    current_pid: u32,
    is_wsl: bool,
) -> u32 {
    let dir = get_sessions_dir(config_home);
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let pid_file_re = regex::Regex::new(r"^\d+\.json$").unwrap();
    let mut count = 0u32;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !pid_file_re.is_match(&file_name) {
            continue;
        }
        let pid_str = &file_name[..file_name.len() - 5];
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        if pid == current_pid {
            count += 1;
            continue;
        }

        if is_process_running(pid) {
            count += 1;
        } else if !is_wsl {
            // Stale file — sweep it
            let _ = tokio::fs::remove_file(dir.join(&file_name)).await;
        }
    }

    count
}
