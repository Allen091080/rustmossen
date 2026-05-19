//! Consolidation lock — prevents concurrent consolidation runs.

use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;

const LOCK_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// A file-based lock for consolidation.
pub struct ConsolidationLock {
    lock_path: PathBuf,
    acquired: bool,
}

impl ConsolidationLock {
    pub fn new(memory_dir: &std::path::Path) -> Self {
        Self {
            lock_path: memory_dir.join(".consolidation.lock"),
            acquired: false,
        }
    }

    /// Try to acquire the lock. Returns true if successful.
    pub async fn try_acquire(&mut self) -> bool {
        // Check if an existing lock is stale
        if let Ok(metadata) = fs::metadata(&self.lock_path).await {
            if let Ok(modified) = metadata.modified() {
                let age = modified.elapsed().unwrap_or(Duration::from_secs(0));
                if age < Duration::from_secs(LOCK_TIMEOUT_SECS) {
                    // Lock is held and not stale
                    return false;
                }
                // Stale lock — remove it
            }
        }

        // Write our PID to the lock file
        let pid = std::process::id();
        match fs::write(&self.lock_path, pid.to_string()).await {
            Ok(_) => {
                self.acquired = true;
                true
            }
            Err(_) => false,
        }
    }

    /// Release the lock.
    pub async fn release(&mut self) {
        if self.acquired {
            let _ = fs::remove_file(&self.lock_path).await;
            self.acquired = false;
        }
    }

    /// Check if the lock is currently held (by anyone).
    pub async fn is_held(&self) -> bool {
        if let Ok(metadata) = fs::metadata(&self.lock_path).await {
            if let Ok(modified) = metadata.modified() {
                let age = modified.elapsed().unwrap_or(Duration::from_secs(0));
                return age < Duration::from_secs(LOCK_TIMEOUT_SECS);
            }
        }
        false
    }
}

impl Drop for ConsolidationLock {
    fn drop(&mut self) {
        if self.acquired {
            // Synchronous cleanup on drop — best-effort
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/autoDream/consolidationLock.ts` exports.
// ---------------------------------------------------------------------------

/// `consolidationLock.ts` `readLastConsolidatedAt` — last marker timestamp.
pub async fn read_last_consolidated_at() -> u64 {
    let path = consolidation_marker_path();
    let Ok(content) = fs::read_to_string(&path).await else {
        return 0;
    };
    content.trim().parse::<u64>().unwrap_or(0)
}

/// `consolidationLock.ts` `tryAcquireConsolidationLock`.
pub async fn try_acquire_consolidation_lock() -> Option<u64> {
    let path = ts_lock_path();
    if let Ok(metadata) = fs::metadata(&path).await {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                if elapsed.as_secs() < 15 * 60 {
                    return None;
                }
            }
        }
    }
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let _ = fs::write(&path, now.to_string()).await;
    Some(now)
}

/// `consolidationLock.ts` `rollbackConsolidationLock`.
pub async fn rollback_consolidation_lock(_acquired_at: u64) {
    let path = ts_lock_path();
    let _ = fs::remove_file(&path).await;
}

/// `consolidationLock.ts` `listSessionsTouchedSince`.
pub async fn list_sessions_touched_since(since_ms: u64) -> Vec<String> {
    let dir = sessions_dir();
    let Ok(mut entries) = fs::read_dir(&dir).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH) else {
            continue;
        };
        if (epoch.as_millis() as u64) >= since_ms {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// `consolidationLock.ts` `recordConsolidation`.
pub async fn record_consolidation() {
    let path = consolidation_marker_path();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let _ = fs::write(&path, now.to_string()).await;
}

fn ts_lock_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("mossen-consolidation.lock");
    p
}

fn consolidation_marker_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("mossen-last-consolidation.json");
    p
}

fn sessions_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("mossen-sessions");
    p
}
