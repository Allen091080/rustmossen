//! # i_term_backup — iTerm2 设置备份与恢复
//!
//! 对应 TypeScript `utils/iTermBackup.ts`。

use std::path::PathBuf;
use tokio::fs;
use tracing::error;

use crate::config::{get_global_config, save_global_config};

/// 标记 iTerm2 设置过程已完成。
pub fn mark_iterm2_setup_complete() {
    save_global_config(|current| {
        let mut updated = current.clone();
        updated.iterm2_setup_in_progress = Some(false);
        updated
    });
}

/// 获取 iTerm2 恢复信息。
fn get_iterm2_recovery_info() -> (bool, Option<String>) {
    let config = get_global_config();
    let in_progress = config.iterm2_setup_in_progress.unwrap_or(false);
    let backup_path = config.iterm2_backup_path.clone().filter(|p| !p.is_empty());
    (in_progress, backup_path)
}

/// 获取 iTerm2 plist 文件路径。
fn get_iterm2_plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    home.join("Library")
        .join("Preferences")
        .join("com.googlecode.iterm2.plist")
}

/// 恢复结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreResult {
    Restored,
    NoBackup,
    Failed { backup_path: String },
}

/// 检查并恢复 iTerm2 备份。
pub async fn check_and_restore_iterm2_backup() -> RestoreResult {
    let (in_progress, backup_path) = get_iterm2_recovery_info();

    if !in_progress {
        return RestoreResult::NoBackup;
    }

    let backup_path = match backup_path {
        Some(p) => p,
        None => {
            mark_iterm2_setup_complete();
            return RestoreResult::NoBackup;
        }
    };

    // Check if backup file exists
    if fs::metadata(&backup_path).await.is_err() {
        mark_iterm2_setup_complete();
        return RestoreResult::NoBackup;
    }

    // Try to restore
    match fs::copy(&backup_path, get_iterm2_plist_path()).await {
        Ok(_) => {
            mark_iterm2_setup_complete();
            RestoreResult::Restored
        }
        Err(restore_error) => {
            error!("Failed to restore iTerm2 settings with: {}", restore_error);
            mark_iterm2_setup_complete();
            RestoreResult::Failed { backup_path }
        }
    }
}
