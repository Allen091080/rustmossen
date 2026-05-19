use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::process::Command;

/// Shell info for completion setup.
#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub name: String,
    pub rc_file: PathBuf,
    pub cache_file: PathBuf,
    pub completion_line: String,
    pub shell_flag: String,
}

/// Detect the current shell.
pub fn detect_shell() -> Option<ShellInfo> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let mossen_dir = home.join(".mossen");

    if shell.ends_with("/zsh") || shell.ends_with("/zsh.exe") {
        let cache_file = mossen_dir.join("completion.zsh");
        let cache_file_str = cache_file.to_string_lossy().to_string();
        return Some(ShellInfo {
            name: "zsh".to_string(),
            rc_file: home.join(".zshrc"),
            cache_file: cache_file.clone(),
            completion_line: format!("[[ -f \"{}\" ]] && source \"{}\"", cache_file_str, cache_file_str),
            shell_flag: "zsh".to_string(),
        });
    }
    if shell.ends_with("/bash") || shell.ends_with("/bash.exe") {
        let cache_file = mossen_dir.join("completion.bash");
        let cache_file_str = cache_file.to_string_lossy().to_string();
        return Some(ShellInfo {
            name: "bash".to_string(),
            rc_file: home.join(".bashrc"),
            cache_file: cache_file.clone(),
            completion_line: format!("[ -f \"{}\" ] && source \"{}\"", cache_file_str, cache_file_str),
            shell_flag: "bash".to_string(),
        });
    }
    if shell.ends_with("/fish") || shell.ends_with("/fish.exe") {
        let xdg = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"));
        let cache_file = mossen_dir.join("completion.fish");
        let cache_file_str = cache_file.to_string_lossy().to_string();
        return Some(ShellInfo {
            name: "fish".to_string(),
            rc_file: xdg.join("fish").join("config.fish"),
            cache_file: cache_file.clone(),
            completion_line: format!("[ -f \"{}\" ] && source \"{}\"", cache_file_str, cache_file_str),
            shell_flag: "fish".to_string(),
        });
    }
    None
}

/// Generate and cache the completion script, then add a source line to the shell's rc file.
pub async fn setup_shell_completion(mossen_bin: &str) -> Result<String, String> {
    let shell = match detect_shell() {
        Some(s) => s,
        None => return Ok(String::new()),
    };

    // Ensure the cache directory exists
    if let Some(parent) = shell.cache_file.parent() {
        if let Err(_) = fs::create_dir_all(parent).await {
            return Ok(format!(
                "\nCould not write {} completion cache\nRun manually: mossen completion {} > {}\n",
                shell.name,
                shell.shell_flag,
                shell.cache_file.display()
            ));
        }
    }

    // Generate the completion script
    let result = Command::new(mossen_bin)
        .args(["completion", &shell.shell_flag, "--output"])
        .arg(&shell.cache_file)
        .output()
        .await;

    match result {
        Ok(output) if !output.status.success() => {
            return Ok(format!(
                "\nCould not generate {} shell completions\nRun manually: mossen completion {} > {}\n",
                shell.name,
                shell.shell_flag,
                shell.cache_file.display()
            ));
        }
        Err(_) => {
            return Ok(format!(
                "\nCould not generate {} shell completions\nRun manually: mossen completion {} > {}\n",
                shell.name,
                shell.shell_flag,
                shell.cache_file.display()
            ));
        }
        _ => {}
    }

    // Check if rc file already sources completions
    let existing = match fs::read_to_string(&shell.rc_file).await {
        Ok(content) => {
            if content.contains("mossen completion")
                || content.contains(&shell.cache_file.to_string_lossy().to_string())
            {
                return Ok(format!(
                    "\nShell completions updated for {}\nSee {}\n",
                    shell.name,
                    shell.rc_file.display()
                ));
            }
            content
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Ok(format!(
                    "\nCould not install {} shell completions\nAdd this to {}:\n{}\n",
                    shell.name,
                    shell.rc_file.display(),
                    shell.completion_line
                ));
            }
            String::new()
        }
    };

    // Append source line to rc file
    if let Some(config_dir) = shell.rc_file.parent() {
        let _ = fs::create_dir_all(config_dir).await;
    }

    let separator = if !existing.is_empty() && !existing.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let content = format!(
        "{}{}\n# Mossen shell completions\n{}\n",
        existing, separator, shell.completion_line
    );

    match fs::write(&shell.rc_file, &content).await {
        Ok(_) => Ok(format!(
            "\nInstalled {} shell completions\nAdded to {}\nRun: source {}\n",
            shell.name,
            shell.rc_file.display(),
            shell.rc_file.display()
        )),
        Err(_) => Ok(format!(
            "\nCould not install {} shell completions\nAdd this to {}:\n{}\n",
            shell.name,
            shell.rc_file.display(),
            shell.completion_line
        )),
    }
}

/// Regenerate cached shell completion scripts.
pub async fn regenerate_completion_cache(mossen_bin: &str) {
    let shell = match detect_shell() {
        Some(s) => s,
        None => return,
    };

    let _ = Command::new(mossen_bin)
        .args(["completion", &shell.shell_flag, "--output"])
        .arg(&shell.cache_file)
        .output()
        .await;
}
