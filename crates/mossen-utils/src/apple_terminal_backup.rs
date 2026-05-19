use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;

/// Mark terminal setup in progress in global config
pub fn mark_terminal_setup_in_progress(
    backup_path: &str,
    save_config: impl FnOnce(Option<&str>),
) {
    save_config(Some(backup_path));
}

/// Mark terminal setup complete in global config
pub fn mark_terminal_setup_complete(save_config: impl FnOnce(Option<&str>)) {
    save_config(None);
}

/// Terminal recovery info
#[derive(Debug, Clone)]
pub struct TerminalRecoveryInfo {
    pub in_progress: bool,
    pub backup_path: Option<String>,
}

/// Get the Apple Terminal plist path
pub fn get_terminal_plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    home.join("Library")
        .join("Preferences")
        .join("com.apple.Terminal.plist")
}

/// Backup terminal preferences
pub async fn backup_terminal_preferences(
    mark_in_progress: impl FnOnce(&str),
) -> Option<String> {
    let terminal_plist_path = get_terminal_plist_path();
    let backup_path = format!("{}.bak", terminal_plist_path.to_string_lossy());

    // Export current defaults
    let result = Command::new("defaults")
        .args(["export", "com.apple.Terminal"])
        .arg(&terminal_plist_path)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {}
        _ => return None,
    }

    // Verify the file exists
    if fs::metadata(&terminal_plist_path).await.is_err() {
        return None;
    }

    // Export to backup path
    let backup_result = Command::new("defaults")
        .args(["export", "com.apple.Terminal"])
        .arg(&backup_path)
        .output()
        .await;

    if let Err(_) = backup_result {
        return None;
    }

    mark_in_progress(&backup_path);

    Some(backup_path)
}

/// Result of restore operation
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreResult {
    Restored,
    NoBackup,
    Failed { backup_path: String },
}

/// Check and restore terminal backup if needed
pub async fn check_and_restore_terminal_backup(
    recovery_info: TerminalRecoveryInfo,
    mark_complete: impl FnOnce(),
) -> RestoreResult {
    if !recovery_info.in_progress {
        return RestoreResult::NoBackup;
    }

    let backup_path = match recovery_info.backup_path {
        Some(p) => p,
        None => {
            mark_complete();
            return RestoreResult::NoBackup;
        }
    };

    // Verify backup file exists
    if fs::metadata(&backup_path).await.is_err() {
        mark_complete();
        return RestoreResult::NoBackup;
    }

    // Attempt restore
    let result = Command::new("defaults")
        .args(["import", "com.apple.Terminal"])
        .arg(&backup_path)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {
            // Kill cfprefsd to apply changes
            let _ = Command::new("killall")
                .arg("cfprefsd")
                .output()
                .await;

            mark_complete();
            RestoreResult::Restored
        }
        _ => {
            mark_complete();
            RestoreResult::Failed {
                backup_path: backup_path.clone(),
            }
        }
    }
}
