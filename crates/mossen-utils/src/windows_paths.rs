//! Windows path conversion utilities — POSIX ↔ Windows path translation.

use regex::Regex;

use once_cell::sync::Lazy;

/// Convert a Windows path to a POSIX path using pure Rust.
pub fn windows_path_to_posix_path(windows_path: &str) -> String {
    // Handle UNC paths: \\server\share -> //server/share
    if windows_path.starts_with("\\\\") {
        return windows_path.replace('\\', "/");
    }
    // Handle drive letter paths: C:\Users\foo -> /c/Users/foo
    static DRIVE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([A-Za-z]):[/\\]").unwrap());
    if let Some(caps) = DRIVE_RE.captures(windows_path) {
        let drive_letter = caps[1].to_lowercase();
        let rest = &windows_path[2..];
        return format!("/{}{}", drive_letter, rest.replace('\\', "/"));
    }
    // Already POSIX or relative — just flip slashes
    windows_path.replace('\\', "/")
}

/// Convert a POSIX path to a Windows path using pure Rust.
pub fn posix_path_to_windows_path(posix_path: &str) -> String {
    // Handle UNC paths: //server/share -> \\server\share
    if posix_path.starts_with("//") {
        return posix_path.replace('/', "\\");
    }
    // Handle /cygdrive/c/... format
    static CYGDRIVE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^/cygdrive/([A-Za-z])(/|$)").unwrap());
    if let Some(caps) = CYGDRIVE_RE.captures(posix_path) {
        let drive_letter = caps[1].to_uppercase();
        let prefix_len = "/cygdrive/".len() + 1; // +1 for the drive letter
        let rest = &posix_path[prefix_len..];
        let rest_win = if rest.is_empty() { "\\" } else { rest };
        return format!("{}:{}", drive_letter, rest_win.replace('/', "\\"));
    }
    // Handle /c/... format (MSYS2/Git Bash)
    static DRIVE_MATCH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^/([A-Za-z])(/|$)").unwrap());
    if let Some(caps) = DRIVE_MATCH_RE.captures(posix_path) {
        let drive_letter = caps[1].to_uppercase();
        let rest = &posix_path[2..];
        let rest_win = if rest.is_empty() { "\\" } else { rest };
        return format!("{}:{}", drive_letter, rest_win.replace('/', "\\"));
    }
    // Already Windows or relative — just flip slashes
    posix_path.replace('/', "\\")
}

/// Find the path where `bash.exe` included with git-bash exists.
pub fn find_git_bash_path() -> Option<String> {
    if let Ok(path) = std::env::var("MOSSEN_CODE_GIT_BASH_PATH") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
        eprintln!("Unable to find MOSSEN_CODE_GIT_BASH_PATH path \"{}\"", path);
        return None;
    }

    // Check common git installation locations
    let default_locations = [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ];

    for location in &default_locations {
        if std::path::Path::new(location).exists() {
            // Derive bash path from git path
            let git_dir = std::path::Path::new(location)
                .parent()
                .and_then(|p| p.parent());
            if let Some(base) = git_dir {
                let bash_path = base.join("bin").join("bash.exe");
                if bash_path.exists() {
                    return Some(bash_path.to_string_lossy().to_string());
                }
            }
        }
    }

    // Try where.exe
    let output = std::process::Command::new("where.exe")
        .arg("git")
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let candidate = line.trim();
            if candidate.is_empty() {
                continue;
            }
            let git_dir = std::path::Path::new(candidate)
                .parent()
                .and_then(|p| p.parent());
            if let Some(base) = git_dir {
                let bash_path = base.join("bin").join("bash.exe");
                if bash_path.exists() {
                    return Some(bash_path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// If Windows, set the SHELL environment variable to git-bash path.
pub fn set_shell_if_windows() {
    if cfg!(target_os = "windows") {
        if let Some(bash_path) = find_git_bash_path() {
            std::env::set_var("SHELL", &bash_path);
            tracing::debug!("Using bash path: \"{}\"", bash_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_to_posix() {
        assert_eq!(windows_path_to_posix_path(r"C:\Users\foo"), "/c/Users/foo");
        assert_eq!(
            windows_path_to_posix_path(r"\\server\share\file"),
            "//server/share/file"
        );
        assert_eq!(
            windows_path_to_posix_path(r"relative\path"),
            "relative/path"
        );
    }

    #[test]
    fn test_posix_to_windows() {
        assert_eq!(posix_path_to_windows_path("/c/Users/foo"), r"C:\Users\foo");
        assert_eq!(
            posix_path_to_windows_path("//server/share"),
            r"\\server\share"
        );
        assert_eq!(
            posix_path_to_windows_path("/cygdrive/d/project"),
            r"D:\project"
        );
    }
}
