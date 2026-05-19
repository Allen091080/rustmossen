use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;

/// Cache states: None = not yet loaded, Some(None) = checked, no files, Some(Some(s)) = cached.
static SESSION_ENV_SCRIPT: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));

/// Get the session environment directory path.
pub async fn get_session_env_dir_path(config_home: &Path, session_id: &str) -> std::io::Result<PathBuf> {
    let session_env_dir = config_home.join("session-env").join(session_id);
    fs::create_dir_all(&session_env_dir).await?;
    Ok(session_env_dir)
}

/// Get the file path for a hook environment file.
pub async fn get_hook_env_file_path(
    config_home: &Path,
    session_id: &str,
    hook_event: &str,
    hook_index: usize,
) -> std::io::Result<PathBuf> {
    let prefix = hook_event.to_lowercase();
    let dir = get_session_env_dir_path(config_home, session_id).await?;
    Ok(dir.join(format!("{}-hook-{}.sh", prefix, hook_index)))
}

static HOOK_ENV_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(setup|sessionstart|cwdchanged|filechanged)-hook-(\d+)\.sh$").unwrap());

/// Clear CWD-related environment files.
pub async fn clear_cwd_env_files(config_home: &Path, session_id: &str) {
    let dir = match get_session_env_dir_path(config_home, session_id).await {
        Ok(d) => d,
        Err(_) => return,
    };

    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if (name.starts_with("filechanged-hook-") || name.starts_with("cwdchanged-hook-"))
            && HOOK_ENV_REGEX.is_match(&name)
        {
            let _ = fs::write(dir.join(&name), "").await;
        }
    }
}

/// Invalidate the session environment cache.
pub fn invalidate_session_env_cache() {
    let mut cache = SESSION_ENV_SCRIPT.lock().unwrap();
    *cache = None;
}

/// Hook event priority order.
fn hook_env_priority(event_type: &str) -> u32 {
    match event_type {
        "setup" => 0,
        "sessionstart" => 1,
        "cwdchanged" => 2,
        "filechanged" => 3,
        _ => 99,
    }
}

/// Sort hook environment files by event priority then index.
fn sort_hook_env_files(a: &str, b: &str) -> std::cmp::Ordering {
    let a_match = HOOK_ENV_REGEX.captures(a);
    let b_match = HOOK_ENV_REGEX.captures(b);

    let a_type = a_match.as_ref().and_then(|c| c.get(1)).map(|m| m.as_str()).unwrap_or("");
    let b_type = b_match.as_ref().and_then(|c| c.get(1)).map(|m| m.as_str()).unwrap_or("");

    if a_type != b_type {
        return hook_env_priority(a_type).cmp(&hook_env_priority(b_type));
    }

    let a_index: u32 = a_match
        .as_ref()
        .and_then(|c| c.get(2))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let b_index: u32 = b_match
        .as_ref()
        .and_then(|c| c.get(2))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    a_index.cmp(&b_index)
}

/// Get the session environment script.
pub async fn get_session_environment_script(
    config_home: &Path,
    session_id: &str,
    platform: &str,
) -> Option<String> {
    if platform == "windows" {
        return None;
    }

    // Check cache
    {
        let cache = SESSION_ENV_SCRIPT.lock().unwrap();
        if let Some(ref cached) = *cache {
            return cached.clone();
        }
    }

    let mut scripts: Vec<String> = Vec::new();

    // Check for MOSSEN_ENV_FILE
    if let Ok(env_file) = std::env::var("MOSSEN_ENV_FILE") {
        if let Ok(env_script) = fs::read_to_string(&env_file).await {
            let trimmed = env_script.trim().to_string();
            if !trimmed.is_empty() {
                scripts.push(trimmed);
            }
        }
    }

    // Load hook environment files from session directory
    let session_env_dir = match get_session_env_dir_path(config_home, session_id).await {
        Ok(d) => d,
        Err(_) => {
            let result = if scripts.is_empty() { None } else { Some(scripts.join("\n")) };
            let mut cache = SESSION_ENV_SCRIPT.lock().unwrap();
            *cache = Some(result.clone());
            return result;
        }
    };

    let mut hook_files: Vec<String> = Vec::new();
    if let Ok(mut entries) = fs::read_dir(&session_env_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if HOOK_ENV_REGEX.is_match(&name) {
                hook_files.push(name);
            }
        }
    }

    hook_files.sort_by(|a, b| sort_hook_env_files(a, b));

    for file in &hook_files {
        let file_path = session_env_dir.join(file);
        if let Ok(content) = fs::read_to_string(&file_path).await {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                scripts.push(trimmed);
            }
        }
    }

    let result = if scripts.is_empty() {
        None
    } else {
        Some(scripts.join("\n"))
    };

    // Cache the result
    let mut cache = SESSION_ENV_SCRIPT.lock().unwrap();
    *cache = Some(result.clone());

    result
}
