//! iTerm2 设置备份与恢复工具。
//!
//! 翻译自 `utils/iTermBackup.ts`。

use std::path::PathBuf;

use crate::config::{get_global_config, save_global_config};
use crate::log::log_error;

/// 简单错误包装类型。
#[derive(Debug)]
struct ItermError(String);

impl std::fmt::Display for ItermError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ItermError {}

/// 标记 iTerm2 设置安装完成。
pub fn mark_iterm2_setup_complete() {
    save_global_config(|current| {
        let mut updated = current.clone();
        updated.iterm2_setup_in_progress = Some(false);
        updated
    });
}

/// iTerm2 恢复信息。
struct Iterm2RecoveryInfo {
    in_progress: bool,
    backup_path: Option<String>,
}

/// 获取 iTerm2 恢复信息。
fn get_iterm2_recovery_info() -> Iterm2RecoveryInfo {
    let config = get_global_config();
    Iterm2RecoveryInfo {
        in_progress: config.iterm2_setup_in_progress.unwrap_or(false),
        backup_path: config.iterm2_backup_path.clone().filter(|s| !s.is_empty()),
    }
}

/// 获取 iTerm2 plist 文件路径。
fn get_iterm2_plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    home.join("Library")
        .join("Preferences")
        .join("com.googlecode.iterm2.plist")
}

/// 恢复结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreResult {
    /// 成功恢复。
    Restored,
    /// 没有备份需要恢复。
    NoBackup,
    /// 恢复失败，返回备份路径。
    Failed { backup_path: String },
}

/// 检查并恢复 iTerm2 备份。
pub async fn check_and_restore_iterm2_backup() -> RestoreResult {
    let info = get_iterm2_recovery_info();

    if !info.in_progress {
        return RestoreResult::NoBackup;
    }

    let backup_path = match info.backup_path {
        Some(p) => p,
        None => {
            mark_iterm2_setup_complete();
            return RestoreResult::NoBackup;
        }
    };

    // 检查备份文件是否存在
    if tokio::fs::metadata(&backup_path).await.is_err() {
        mark_iterm2_setup_complete();
        return RestoreResult::NoBackup;
    }

    // 尝试恢复
    match tokio::fs::copy(&backup_path, get_iterm2_plist_path()).await {
        Ok(_) => {
            mark_iterm2_setup_complete();
            RestoreResult::Restored
        }
        Err(restore_error) => {
            let err = ItermError(format!(
                "Failed to restore iTerm2 settings with: {}",
                restore_error
            ));
            log_error(&err);
            mark_iterm2_setup_complete();
            RestoreResult::Failed { backup_path }
        }
    }
}
