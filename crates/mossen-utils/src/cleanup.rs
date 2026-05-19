use std::path::{Path, PathBuf};
use tokio::fs;

const DEFAULT_CLEANUP_PERIOD_DAYS: u64 = 30;
const ONE_DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// Result of a cleanup operation.
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    pub messages: u64,
    pub errors: u64,
}

impl CleanupResult {
    pub fn add(&mut self, other: &CleanupResult) {
        self.messages += other.messages;
        self.errors += other.errors;
    }
}

/// Add two cleanup results together.
pub fn add_cleanup_results(a: &CleanupResult, b: &CleanupResult) -> CleanupResult {
    CleanupResult {
        messages: a.messages + b.messages,
        errors: a.errors + b.errors,
    }
}

/// Convert a filename to a date.
pub fn convert_file_name_to_date(filename: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let base = filename.split('.').next()?;
    // Replace T{HH}-{MM}-{SS}-{mmm}Z with T{HH}:{MM}:{SS}.{mmm}Z
    let re = regex::Regex::new(r"T(\d{2})-(\d{2})-(\d{2})-(\d{3})Z").ok()?;
    let iso_str = re.replace(base, "T$1:$2:$3.$4Z");
    chrono::DateTime::parse_from_rfc3339(&iso_str)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Get the cutoff date for cleanup based on settings.
fn get_cutoff_date(cleanup_period_days: Option<u64>) -> chrono::DateTime<chrono::Utc> {
    let days = cleanup_period_days.unwrap_or(DEFAULT_CLEANUP_PERIOD_DAYS);
    let duration = chrono::Duration::days(days as i64);
    chrono::Utc::now() - duration
}

/// Clean up old files in a directory based on filename timestamps.
async fn cleanup_old_files_in_directory(
    dir_path: &Path,
    cutoff_date: chrono::DateTime<chrono::Utc>,
    is_message_path: bool,
) -> CleanupResult {
    let mut result = CleanupResult::default();

    let mut entries = match fs::read_dir(dir_path).await {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Error reading directory {:?}: {}", dir_path, e);
            }
            return result;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if let Some(timestamp) = convert_file_name_to_date(&file_name) {
            if timestamp < cutoff_date {
                match fs::remove_file(entry.path()).await {
                    Ok(_) => {
                        if is_message_path {
                            result.messages += 1;
                        } else {
                            result.errors += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error removing file {:?}: {}", entry.path(), e);
                    }
                }
            }
        }
    }

    result
}

/// Clean up old message files (error logs, MCP logs).
pub async fn cleanup_old_message_files(
    error_path: &Path,
    base_cache_path: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = cleanup_old_files_in_directory(error_path, cutoff_date, false).await;

    // Clean up MCP logs
    let mut entries = match fs::read_dir(base_cache_path).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false)
            && name.starts_with("mcp-logs-")
        {
            let mcp_log_dir = entry.path();
            let sub_result =
                cleanup_old_files_in_directory(&mcp_log_dir, cutoff_date, true).await;
            result.add(&sub_result);
            try_rmdir(&mcp_log_dir).await;
        }
    }

    result
}

/// Try to remove an empty directory.
async fn try_rmdir(dir_path: &Path) {
    let _ = fs::remove_dir(dir_path).await;
}

/// Unlink a file if it's older than the cutoff date.
async fn unlink_if_old(
    file_path: &Path,
    cutoff_date: chrono::DateTime<chrono::Utc>,
) -> Result<bool, std::io::Error> {
    let metadata = fs::metadata(file_path).await?;
    let modified = metadata.modified()?;
    let modified_chrono = chrono::DateTime::<chrono::Utc>::from(modified);
    if modified_chrono < cutoff_date {
        fs::remove_file(file_path).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Clean up old session files (transcripts, tool results).
pub async fn cleanup_old_session_files(
    projects_dir: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = CleanupResult::default();

    let mut project_entries = match fs::read_dir(projects_dir).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(project_entry)) = project_entries.next_entry().await {
        if !project_entry
            .file_type()
            .await
            .map(|t| t.is_dir())
            .unwrap_or(false)
        {
            continue;
        }

        let project_dir = project_entry.path();
        let mut entries = match fs::read_dir(&project_dir).await {
            Ok(entries) => entries,
            Err(_) => {
                result.errors += 1;
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_type = entry.file_type().await.unwrap_or(std::fs::FileType::from(
                std::fs::metadata(entry.path())
                    .unwrap_or_else(|_| std::fs::metadata(".").unwrap())
                    .file_type(),
            ));

            if file_type.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".jsonl") && !name.ends_with(".cast") {
                    continue;
                }
                match unlink_if_old(&entry.path(), cutoff_date).await {
                    Ok(true) => result.messages += 1,
                    Ok(false) => {}
                    Err(_) => result.errors += 1,
                }
            } else if file_type.is_dir() {
                let session_dir = entry.path();
                let tool_results_dir = session_dir.join("tool-results");
                match cleanup_tool_results_dir(&tool_results_dir, cutoff_date).await {
                    Ok(sub_result) => result.add(&sub_result),
                    Err(_) => {
                        try_rmdir(&session_dir).await;
                        continue;
                    }
                }
                try_rmdir(&tool_results_dir).await;
                try_rmdir(&session_dir).await;
            }
        }

        try_rmdir(&project_dir).await;
    }

    result
}

/// Clean up tool results directory.
async fn cleanup_tool_results_dir(
    tool_results_dir: &Path,
    cutoff_date: chrono::DateTime<chrono::Utc>,
) -> Result<CleanupResult, std::io::Error> {
    let mut result = CleanupResult::default();
    let mut entries = fs::read_dir(tool_results_dir).await?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_type = entry.file_type().await.unwrap_or(
            std::fs::metadata(entry.path())
                .map(|m| m.file_type())
                .unwrap_or(std::fs::metadata(".").unwrap().file_type()),
        );

        if file_type.is_file() {
            match unlink_if_old(&entry.path(), cutoff_date).await {
                Ok(true) => result.messages += 1,
                Ok(false) => {}
                Err(_) => result.errors += 1,
            }
        } else if file_type.is_dir() {
            let tool_dir = entry.path();
            let mut sub_entries = match fs::read_dir(&tool_dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            while let Ok(Some(sub_entry)) = sub_entries.next_entry().await {
                if sub_entry
                    .file_type()
                    .await
                    .map(|t| t.is_file())
                    .unwrap_or(false)
                {
                    match unlink_if_old(&sub_entry.path(), cutoff_date).await {
                        Ok(true) => result.messages += 1,
                        Ok(false) => {}
                        Err(_) => result.errors += 1,
                    }
                }
            }
            try_rmdir(&tool_dir).await;
        }
    }

    Ok(result)
}

/// Clean up a single directory by extension and cutoff date.
async fn cleanup_single_directory(
    dir_path: &Path,
    extension: &str,
    remove_empty_dir: bool,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = CleanupResult::default();

    let mut entries = match fs::read_dir(dir_path).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|t| t.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(extension) {
            continue;
        }
        match unlink_if_old(&entry.path(), cutoff_date).await {
            Ok(true) => result.messages += 1,
            Ok(false) => {}
            Err(_) => result.errors += 1,
        }
    }

    if remove_empty_dir {
        try_rmdir(dir_path).await;
    }

    result
}

/// Clean up old plan files.
pub async fn cleanup_old_plan_files(
    config_home_dir: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let plans_dir = config_home_dir.join("plans");
    cleanup_single_directory(&plans_dir, ".md", true, cleanup_period_days).await
}

/// Clean up old file history backups.
pub async fn cleanup_old_file_history_backups(
    config_home_dir: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = CleanupResult::default();
    let file_history_dir = config_home_dir.join("file-history");

    let mut entries = match fs::read_dir(&file_history_dir).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|t| t.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        match fs::metadata(entry.path()).await {
            Ok(metadata) => {
                if let Ok(modified) = metadata.modified() {
                    let modified_chrono = chrono::DateTime::<chrono::Utc>::from(modified);
                    if modified_chrono < cutoff_date {
                        match fs::remove_dir_all(entry.path()).await {
                            Ok(_) => result.messages += 1,
                            Err(_) => result.errors += 1,
                        }
                    }
                }
            }
            Err(_) => result.errors += 1,
        }
    }

    try_rmdir(&file_history_dir).await;
    result
}

/// Clean up old session environment directories.
pub async fn cleanup_old_session_env_dirs(
    config_home_dir: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = CleanupResult::default();
    let session_env_dir = config_home_dir.join("session-env");

    let mut entries = match fs::read_dir(&session_env_dir).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|t| t.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        match fs::metadata(entry.path()).await {
            Ok(metadata) => {
                if let Ok(modified) = metadata.modified() {
                    let modified_chrono = chrono::DateTime::<chrono::Utc>::from(modified);
                    if modified_chrono < cutoff_date {
                        match fs::remove_dir_all(entry.path()).await {
                            Ok(_) => result.messages += 1,
                            Err(_) => result.errors += 1,
                        }
                    }
                }
            }
            Err(_) => result.errors += 1,
        }
    }

    try_rmdir(&session_env_dir).await;
    result
}

/// Clean up old debug log files.
pub async fn cleanup_old_debug_logs(
    config_home_dir: &Path,
    cleanup_period_days: Option<u64>,
) -> CleanupResult {
    let cutoff_date = get_cutoff_date(cleanup_period_days);
    let mut result = CleanupResult::default();
    let debug_dir = config_home_dir.join("debug");

    let mut entries = match fs::read_dir(&debug_dir).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|t| t.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".txt") || name == "latest" {
            continue;
        }
        match unlink_if_old(&entry.path(), cutoff_date).await {
            Ok(true) => result.messages += 1,
            Ok(false) => {}
            Err(_) => result.errors += 1,
        }
    }

    result
}

/// Run all cleanup operations in background.
pub async fn cleanup_old_message_files_in_background(
    config_home_dir: &Path,
    error_path: &Path,
    base_cache_path: &Path,
    projects_dir: &Path,
    cleanup_period_days: Option<u64>,
) {
    let _ = cleanup_old_message_files(error_path, base_cache_path, cleanup_period_days).await;
    let _ = cleanup_old_session_files(projects_dir, cleanup_period_days).await;
    let _ = cleanup_old_plan_files(config_home_dir, cleanup_period_days).await;
    let _ = cleanup_old_file_history_backups(config_home_dir, cleanup_period_days).await;
    let _ = cleanup_old_session_env_dirs(config_home_dir, cleanup_period_days).await;
    let _ = cleanup_old_debug_logs(config_home_dir, cleanup_period_days).await;
}

/// 对应 TS `cleanupNpmCacheForProviderPackages`：清理特定 provider 包的 npm 缓存。
pub async fn cleanup_npm_cache_for_provider_packages(provider_packages: &[String]) {
    for pkg in provider_packages {
        let _ = tokio::process::Command::new("npm")
            .args(["cache", "delete", pkg])
            .output()
            .await;
    }
}

/// 对应 TS `cleanupOldVersionsThrottled`：节流式清理旧版本目录。
pub async fn cleanup_old_versions_throttled(base_dir: &Path, max_keep: usize) {
    let Ok(mut entries) = fs::read_dir(base_dir).await else { return; };
    let mut versions: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_dir() {
                if let Ok(modified) = meta.modified() {
                    versions.push((entry.path(), modified));
                }
            }
        }
    }
    versions.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in versions.into_iter().skip(max_keep) {
        let _ = fs::remove_dir_all(path).await;
    }
}
