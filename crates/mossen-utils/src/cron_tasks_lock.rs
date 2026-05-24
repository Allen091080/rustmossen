//! Scheduler lease lock for .mossen/scheduled_tasks.json.
//!
//! When multiple Mossen sessions run in the same project directory, only one
//! drives the cron scheduler. Uses O_EXCL atomic create, PID liveness probe,
//! stale-lock recovery, and cleanup-on-exit.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

const LOCK_FILE_REL: &str = ".mossen/scheduled_tasks.lock";

/// Scheduler lock file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerLock {
    pub session_id: String,
    pub pid: u32,
    pub acquired_at: u64,
}

/// Options for out-of-REPL callers (Agent SDK daemon).
#[derive(Debug, Clone, Default)]
pub struct SchedulerLockOptions {
    pub dir: Option<PathBuf>,
    pub lock_identity: Option<String>,
}

/// Module-level state for suppressing repeat log lines.
static LAST_BLOCKED_BY: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

fn get_lock_path(dir: &Path) -> PathBuf {
    dir.join(LOCK_FILE_REL)
}

/// Read and parse the lock file. Returns None if file doesn't exist or is invalid.
async fn read_lock(dir: &Path) -> Option<SchedulerLock> {
    let path = get_lock_path(dir);
    let raw = tokio::fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&raw).ok()
}

/// Try to atomically create the lock file (exclusive).
/// Returns true on success, false if file already exists.
async fn try_create_exclusive(lock: &SchedulerLock, dir: &Path) -> Result<bool, std::io::Error> {
    let path = get_lock_path(dir);
    let body = serde_json::to_string(lock).unwrap_or_default();

    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(_file) => {
            // File was created exclusively, now write the content
            tokio::fs::write(&path, &body).await?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // .mossen/ doesn't exist yet — create it and retry once
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(_file) => {
                    tokio::fs::write(&path, &body).await?;
                    Ok(true)
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Check if a process is running by PID.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true // Conservative: assume running
    }
}

/// Try to acquire the scheduler lock for the current session.
/// Returns true on success, false if another live session holds it.
///
/// Uses O_EXCL ('wx') for atomic test-and-set. If the file exists:
///   - Already ours → true (idempotent re-acquire)
///   - Another live PID → false
///   - Stale (PID dead / corrupt) → unlink and retry exclusive create once
pub async fn try_acquire_scheduler_lock(
    project_root: &Path,
    session_id: &str,
    opts: &SchedulerLockOptions,
) -> bool {
    let dir = opts.dir.as_deref().unwrap_or(project_root);
    let identity = opts.lock_identity.as_deref().unwrap_or(session_id);
    let pid = std::process::id();

    let lock = SchedulerLock {
        session_id: identity.to_string(),
        pid,
        acquired_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };

    match try_create_exclusive(&lock, dir).await {
        Ok(true) => {
            *LAST_BLOCKED_BY.lock().unwrap() = None;
            tracing::debug!("[ScheduledTasks] acquired scheduler lock (PID {})", pid);
            return true;
        }
        Ok(false) => {}
        Err(e) => {
            tracing::debug!("[ScheduledTasks] lock create failed: {}", e);
            return false;
        }
    }

    let existing = read_lock(dir).await;

    // Already ours (idempotent). Update PID if changed after --resume.
    if let Some(ref ex) = existing {
        if ex.session_id == identity {
            if ex.pid != pid {
                let path = get_lock_path(dir);
                let body = serde_json::to_string(&lock).unwrap_or_default();
                let _ = tokio::fs::write(&path, &body).await;
            }
            return true;
        }
    }

    // Another live session — blocked.
    if let Some(ref ex) = existing {
        if is_process_running(ex.pid) {
            let mut last = LAST_BLOCKED_BY.lock().unwrap();
            if last.as_deref() != Some(&ex.session_id) {
                *last = Some(ex.session_id.clone());
                tracing::debug!(
                    "[ScheduledTasks] scheduler lock held by session {} (PID {})",
                    ex.session_id,
                    ex.pid
                );
            }
            return false;
        }
    }

    // Stale — unlink and retry the exclusive create once.
    if let Some(ref ex) = existing {
        tracing::debug!(
            "[ScheduledTasks] recovering stale scheduler lock from PID {}",
            ex.pid
        );
    }
    let _ = tokio::fs::remove_file(get_lock_path(dir)).await;

    match try_create_exclusive(&lock, dir).await {
        Ok(true) => {
            *LAST_BLOCKED_BY.lock().unwrap() = None;
            true
        }
        _ => false, // Another session won the recovery race
    }
}

/// Release the scheduler lock if the current session owns it.
pub async fn release_scheduler_lock(
    project_root: &Path,
    session_id: &str,
    opts: &SchedulerLockOptions,
) {
    *LAST_BLOCKED_BY.lock().unwrap() = None;

    let dir = opts.dir.as_deref().unwrap_or(project_root);
    let identity = opts.lock_identity.as_deref().unwrap_or(session_id);

    let existing = read_lock(dir).await;
    if let Some(ref ex) = existing {
        if ex.session_id != identity {
            return;
        }
    } else {
        return;
    }

    match tokio::fs::remove_file(get_lock_path(dir)).await {
        Ok(_) => {
            tracing::debug!("[ScheduledTasks] released scheduler lock");
        }
        Err(_) => {
            // Already gone
        }
    }
}
