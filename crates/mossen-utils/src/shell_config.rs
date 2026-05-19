use std::collections::HashMap;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;

/// Regex to match mossen alias lines in shell config.
pub static MOSSEN_ALIAS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*alias\s+mossen\s*=").unwrap());

/// Options for shell config operations.
pub struct ShellConfigOptions {
    pub env: Option<HashMap<String, String>>,
    pub homedir: Option<PathBuf>,
}

/// Get the paths to shell configuration files.
/// Respects ZDOTDIR for zsh users.
pub fn get_shell_config_paths(options: Option<&ShellConfigOptions>) -> HashMap<String, PathBuf> {
    let home = options
        .and_then(|o| o.homedir.clone())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")));

    let env_zdotdir = options
        .and_then(|o| o.env.as_ref())
        .and_then(|e| e.get("ZDOTDIR").cloned())
        .or_else(|| std::env::var("ZDOTDIR").ok());

    let zsh_config_dir = env_zdotdir
        .map(PathBuf::from)
        .unwrap_or_else(|| home.clone());

    let mut configs = HashMap::new();
    configs.insert("zsh".to_string(), zsh_config_dir.join(".zshrc"));
    configs.insert("bash".to_string(), home.join(".bashrc"));
    configs.insert(
        "fish".to_string(),
        home.join(".config/fish/config.fish"),
    );
    configs
}

/// Filter out installer-created mossen aliases from lines.
/// Only removes aliases pointing to $HOME/.mossen/local/mossen.
pub fn filter_mossen_aliases(lines: &[String], local_mossen_path: &str) -> (Vec<String>, bool) {
    let mut had_alias = false;
    let alias_target_regex =
        Regex::new(r#"alias\s+mossen\s*=\s*["']([^"']+)["']"#).unwrap();
    let alias_target_noquote_regex =
        Regex::new(r"alias\s+mossen\s*=\s*([^#\n]+)").unwrap();

    let filtered: Vec<String> = lines
        .iter()
        .filter(|line| {
            if !MOSSEN_ALIAS_REGEX.is_match(line) {
                return true;
            }

            let target = alias_target_regex
                .captures(line)
                .and_then(|c| c.get(1))
                .or_else(|| {
                    alias_target_noquote_regex
                        .captures(line)
                        .and_then(|c| c.get(1))
                })
                .map(|m| m.as_str().trim().to_string());

            if let Some(t) = target {
                if t == local_mossen_path {
                    had_alias = true;
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    (filtered, had_alias)
}

/// Read a file and split it into lines.
/// Returns None if file doesn't exist or can't be read.
pub async fn read_file_lines(file_path: &Path) -> Option<Vec<String>> {
    match fs::read_to_string(file_path).await {
        Ok(content) => Some(content.split('\n').map(|s| s.to_string()).collect()),
        Err(e) => {
            if is_fs_inaccessible(&e) {
                None
            } else {
                None // Could re-throw, but matching TS behavior
            }
        }
    }
}

/// Write lines back to a file.
pub async fn write_file_lines(file_path: &Path, lines: &[String]) -> std::io::Result<()> {
    let content = lines.join("\n");
    fs::write(file_path, content).await
}

/// Check if a mossen alias exists in any shell config file.
pub async fn find_mossen_alias(options: Option<&ShellConfigOptions>) -> Option<String> {
    let configs = get_shell_config_paths(options);
    let alias_regex = Regex::new(r#"alias\s+mossen=["']?([^"'\s]+)"#).unwrap();

    for config_path in configs.values() {
        if let Some(lines) = read_file_lines(config_path).await {
            for line in &lines {
                if MOSSEN_ALIAS_REGEX.is_match(line) {
                    if let Some(caps) = alias_regex.captures(line) {
                        if let Some(m) = caps.get(1) {
                            return Some(m.as_str().to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Check if a mossen alias exists and points to a valid executable.
pub async fn find_valid_mossen_alias(options: Option<&ShellConfigOptions>) -> Option<String> {
    let alias_target = find_mossen_alias(options).await?;

    let home = options
        .and_then(|o| o.homedir.clone())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")));

    let expanded_path = if alias_target.starts_with('~') {
        alias_target.replacen('~', &home.to_string_lossy(), 1)
    } else {
        alias_target.clone()
    };

    match fs::metadata(&expanded_path).await {
        Ok(meta) => {
            if meta.is_file() {
                Some(alias_target)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Helper: check if an IO error indicates filesystem inaccessibility.
fn is_fs_inaccessible(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    )
}
