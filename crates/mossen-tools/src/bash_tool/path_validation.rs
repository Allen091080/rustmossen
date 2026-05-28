//! Path validation for bash commands.
//!
//! Corresponds to `pathValidation.ts` (1303 lines). Validates file paths in commands
//! against allowed working directories, detects path traversal, and enforces
//! filesystem boundaries.

use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Operation type for path-modifying commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandOperationType {
    /// Reads files/dirs (e.g., cat, ls).
    Read,
    /// Writes/modifies files (e.g., touch, tee).
    Write,
    /// Moves/renames files.
    Move,
    /// Deletes files/dirs.
    Delete,
    /// Creates files/dirs.
    Create,
}

/// A command that operates on file paths.
#[derive(Debug, Clone)]
pub struct PathCommand {
    /// The command name (e.g., "cp", "mv").
    pub name: &'static str,
    /// The type of operation.
    pub operation: CommandOperationType,
    /// Whether the last argument is the destination.
    pub last_is_dest: bool,
}

/// Known path-modifying commands with their operation types.
pub fn path_commands() -> Vec<PathCommand> {
    vec![
        PathCommand {
            name: "cp",
            operation: CommandOperationType::Write,
            last_is_dest: true,
        },
        PathCommand {
            name: "mv",
            operation: CommandOperationType::Move,
            last_is_dest: true,
        },
        PathCommand {
            name: "rm",
            operation: CommandOperationType::Delete,
            last_is_dest: false,
        },
        PathCommand {
            name: "rmdir",
            operation: CommandOperationType::Delete,
            last_is_dest: false,
        },
        PathCommand {
            name: "mkdir",
            operation: CommandOperationType::Create,
            last_is_dest: false,
        },
        PathCommand {
            name: "touch",
            operation: CommandOperationType::Create,
            last_is_dest: false,
        },
        PathCommand {
            name: "ln",
            operation: CommandOperationType::Write,
            last_is_dest: true,
        },
        PathCommand {
            name: "chmod",
            operation: CommandOperationType::Write,
            last_is_dest: false,
        },
        PathCommand {
            name: "chown",
            operation: CommandOperationType::Write,
            last_is_dest: false,
        },
        PathCommand {
            name: "chgrp",
            operation: CommandOperationType::Write,
            last_is_dest: false,
        },
        PathCommand {
            name: "tee",
            operation: CommandOperationType::Write,
            last_is_dest: false,
        },
        PathCommand {
            name: "install",
            operation: CommandOperationType::Write,
            last_is_dest: true,
        },
    ]
}

/// Path constraint check result.
#[derive(Debug, Clone, PartialEq)]
pub enum PathCheckResult {
    /// Path is allowed.
    Allowed,
    /// Path needs approval.
    NeedsApproval { message: String },
    /// Path is denied.
    Denied { message: String },
    /// Not a path command; pass through.
    Passthrough,
}

/// Configuration for path validation.
pub struct PathValidationConfig {
    /// The current working directory.
    pub cwd: String,
    /// Additional allowed working directories.
    pub allowed_directories: Vec<String>,
    /// The original CWD at session start.
    pub original_cwd: String,
}

/// Extract file paths from a command's arguments.
pub fn extract_paths_from_command(command: &str) -> Vec<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return vec![];
    }

    let base_cmd = parts[0];
    let commands = path_commands();
    let path_cmd = commands.iter().find(|c| c.name == base_cmd);

    if path_cmd.is_none() {
        return vec![];
    }

    // Extract non-flag arguments as paths
    let mut paths = Vec::new();
    let mut skip_next = false;

    for (i, part) in parts.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if part.starts_with('-') {
            // Some flags take arguments
            if matches!(*part, "-t" | "-T" | "--target-directory" | "-m" | "--mode") {
                skip_next = true;
            }
            continue;
        }
        paths.push(part.to_string());
    }

    paths
}

/// Resolve a path relative to the CWD.
pub fn resolve_path(path: &str, cwd: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(cwd).join(p)
    }
}

/// Check if a path is within allowed directories.
pub fn is_path_in_allowed_directory(path: &Path, config: &PathValidationConfig) -> bool {
    let canonical = path.to_string_lossy().to_string();

    // Check against CWD
    if canonical.starts_with(&config.cwd) {
        return true;
    }

    // Check against original CWD
    if canonical.starts_with(&config.original_cwd) {
        return true;
    }

    // Check against additional allowed directories
    for dir in &config.allowed_directories {
        if canonical.starts_with(dir) {
            return true;
        }
    }

    false
}

/// Detect path traversal patterns (e.g., ../ sequences).
pub fn has_path_traversal(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.contains("../") || normalized.ends_with("..") || normalized == ".."
}

/// Check if a path targets a dangerous system location.
pub fn is_dangerous_path(path: &str) -> bool {
    let dangerous_prefixes = [
        "/etc/", "/usr/", "/bin/", "/sbin/", "/lib/", "/boot/", "/sys/", "/proc/", "/dev/",
        "/root/",
    ];
    let normalized = path.replace('\\', "/");

    for prefix in &dangerous_prefixes {
        if normalized.starts_with(prefix) || normalized == prefix.trim_end_matches('/') {
            return true;
        }
    }
    false
}

/// Check path constraints for a command.
///
/// Validates that all paths in the command are within allowed directories
/// and don't target dangerous system locations.
pub fn check_path_constraints(command: &str, config: &PathValidationConfig) -> PathCheckResult {
    let paths = extract_paths_from_command(command);

    if paths.is_empty() {
        return PathCheckResult::Passthrough;
    }

    for path_str in &paths {
        // Check for path traversal
        if has_path_traversal(path_str) {
            let resolved = resolve_path(path_str, &config.cwd);
            if !is_path_in_allowed_directory(&resolved, config) {
                return PathCheckResult::NeedsApproval {
                    message: format!(
                        "Path '{}' uses traversal and resolves outside allowed directories",
                        path_str
                    ),
                };
            }
        }

        // Check dangerous system paths
        let resolved = resolve_path(path_str, &config.cwd);
        let resolved_str = resolved.to_string_lossy().to_string();
        if is_dangerous_path(&resolved_str) {
            return PathCheckResult::Denied {
                message: format!("Path '{}' targets a protected system directory", path_str),
            };
        }

        // Check if path is within allowed directories
        if !is_path_in_allowed_directory(&resolved, config) {
            return PathCheckResult::NeedsApproval {
                message: format!("Path '{}' is outside allowed working directories", path_str),
            };
        }
    }

    PathCheckResult::Allowed
}

/// Check if a command uses dangerous rm paths (e.g., rm -rf /).
pub fn check_dangerous_removal_paths(command: &str) -> Option<String> {
    let re = Regex::new(r"\brm\s+").unwrap();
    if !re.is_match(command) {
        return None;
    }

    let dangerous_targets = ["/", "/*", "~", "~/*", "$HOME", "$HOME/*"];
    let parts: Vec<&str> = command.split_whitespace().collect();

    for part in &parts {
        let trimmed = part.trim_matches('"').trim_matches('\'');
        if dangerous_targets.contains(&trimmed) {
            return Some(format!("DANGER: Attempting to remove '{}'", trimmed));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/BashTool/pathValidation.ts` additional exports.
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;

/// `pathValidation.ts` `PATH_EXTRACTORS` — name set for commands whose
/// arguments are path-like and therefore subject to path validation.
pub static PATH_EXTRACTORS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for cmd in [
        "cat", "cp", "mv", "rm", "ls", "touch", "mkdir", "rmdir", "chmod", "chown", "ln", "find",
        "grep", "sed", "head", "tail", "wc", "stat", "diff", "less", "more",
    ] {
        m.insert(cmd, cmd);
    }
    m
});

/// Boxed checker for a single path.
pub type PathChecker = Box<dyn Fn(&str) -> PathCheckResult + Send + Sync + 'static>;

/// `pathValidation.ts` `createPathChecker`.
pub fn create_path_checker(cwd: String, additional_dirs: Vec<String>) -> PathChecker {
    Box::new(move |path: &str| {
        let p = std::path::Path::new(path);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::path::Path::new(&cwd).join(p)
        };
        let abs_str = abs.to_string_lossy().to_string();
        if abs_str.starts_with(&cwd) {
            return PathCheckResult::Allowed;
        }
        for dir in &additional_dirs {
            if abs_str.starts_with(dir) {
                return PathCheckResult::Allowed;
            }
        }
        PathCheckResult::Denied {
            message: format!("Path is outside the working directory: {}", abs_str),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_paths_cp() {
        let paths = extract_paths_from_command("cp -r src/ dest/");
        assert_eq!(paths, vec!["src/", "dest/"]);
    }

    #[test]
    fn test_extract_paths_rm() {
        let paths = extract_paths_from_command("rm -f file.txt");
        assert_eq!(paths, vec!["file.txt"]);
    }

    #[test]
    fn test_extract_paths_non_path_command() {
        let paths = extract_paths_from_command("echo hello");
        assert!(paths.is_empty());
    }

    #[test]
    fn test_has_path_traversal() {
        assert!(has_path_traversal("../../../etc/passwd"));
        assert!(has_path_traversal("foo/../bar"));
        assert!(!has_path_traversal("/usr/local/bin"));
        assert!(!has_path_traversal("src/main.rs"));
    }

    #[test]
    fn test_is_dangerous_path() {
        assert!(is_dangerous_path("/etc/passwd"));
        assert!(is_dangerous_path("/usr/bin/something"));
        assert!(!is_dangerous_path("/home/user/project"));
        assert!(!is_dangerous_path("/tmp/test"));
    }

    #[test]
    fn test_check_path_constraints_allowed() {
        let config = PathValidationConfig {
            cwd: "/home/user/project".to_string(),
            allowed_directories: vec!["/tmp".to_string()],
            original_cwd: "/home/user/project".to_string(),
        };
        let result = check_path_constraints("cp src/a.txt src/b.txt", &config);
        assert_eq!(result, PathCheckResult::Allowed);
    }

    #[test]
    fn test_check_dangerous_removal() {
        assert!(check_dangerous_removal_paths("rm -rf /").is_some());
        assert!(check_dangerous_removal_paths("rm -rf ~").is_some());
        assert!(check_dangerous_removal_paths("rm file.txt").is_none());
    }
}
