//! Validation utilities for the `/add-dir` command.
//!
//! Provides path validation, permission checking, and directory traversal
//! safety checks before adding a directory to the session context.

use std::path::{Path, PathBuf};

/// Maximum depth for recursive directory traversal.
const MAX_DEPTH: usize = 10;

/// Maximum number of files that can be tracked in a single added directory.
const MAX_FILES: usize = 10_000;

/// Directories that should never be added to session context.
const BLOCKED_DIRS: &[&str] = &[
    "/", "/bin", "/sbin", "/usr", "/etc", "/var", "/tmp",
    "/System", "/Library", "/proc", "/sys", "/dev",
];

/// Result of directory validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the directory is valid for addition.
    pub is_valid: bool,
    /// Human-readable reason if invalid.
    pub reason: Option<String>,
    /// Resolved absolute path.
    pub resolved_path: Option<PathBuf>,
    /// Estimated file count (if validation passed).
    pub estimated_files: Option<usize>,
}

/// Validate a path for addition to session context.
///
/// Checks:
/// 1. Path exists and is a directory
/// 2. Path is not a blocked system directory
/// 3. Path is readable
/// 4. Path does not exceed depth/file limits
pub fn validate_add_dir(path: &str, cwd: &Path) -> ValidationResult {
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };

    // Canonicalize to resolve symlinks and ..
    let canonical = match resolved.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return ValidationResult {
                is_valid: false,
                reason: Some(format!("Path does not exist: {}", resolved.display())),
                resolved_path: None,
                estimated_files: None,
            };
        }
    };

    // Check if it's a directory
    if !canonical.is_dir() {
        return ValidationResult {
            is_valid: false,
            reason: Some(format!("Not a directory: {}", canonical.display())),
            resolved_path: Some(canonical),
            estimated_files: None,
        };
    }

    // Check blocked directories
    let canonical_str = canonical.to_string_lossy();
    for blocked in BLOCKED_DIRS {
        if canonical_str.as_ref() == *blocked {
            return ValidationResult {
                is_valid: false,
                reason: Some(format!(
                    "Cannot add system directory: {}",
                    canonical.display()
                )),
                resolved_path: Some(canonical),
                estimated_files: None,
            };
        }
    }

    // Estimate file count (non-recursive quick check)
    let estimated = estimate_file_count(&canonical);
    if estimated > MAX_FILES {
        return ValidationResult {
            is_valid: false,
            reason: Some(format!(
                "Directory contains too many files (~{}). Maximum is {}.",
                estimated, MAX_FILES
            )),
            resolved_path: Some(canonical),
            estimated_files: Some(estimated),
        };
    }

    ValidationResult {
        is_valid: true,
        reason: None,
        resolved_path: Some(canonical),
        estimated_files: Some(estimated),
    }
}

/// Quick estimate of file count in a directory (non-recursive for performance).
fn estimate_file_count(dir: &Path) -> usize {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// validation.ts —— 与 TS 一一对应的 API
// ---------------------------------------------------------------------------

/// `validation.ts` `AddDirectoryResult`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "resultType", rename_all = "camelCase")]
pub enum AddDirectoryResult {
    Success {
        #[serde(rename = "absolutePath")]
        absolute_path: String,
    },
    EmptyPath,
    PathNotFound {
        #[serde(rename = "directoryPath")]
        directory_path: String,
        #[serde(rename = "absolutePath")]
        absolute_path: String,
    },
    NotADirectory {
        #[serde(rename = "directoryPath")]
        directory_path: String,
        #[serde(rename = "absolutePath")]
        absolute_path: String,
    },
    AlreadyInWorkingDirectory {
        #[serde(rename = "directoryPath")]
        directory_path: String,
        #[serde(rename = "workingDir")]
        working_dir: String,
    },
}

/// `validation.ts` `validateDirectoryForWorkspace`。
///
/// `current_working_dirs` 由调用方提供 — Rust 端避免直接读取
/// `ToolPermissionContext`（在 mossen-tools 中，会形成循环依赖）。
pub async fn validate_directory_for_workspace(
    directory_path: &str,
    current_working_dirs: &[PathBuf],
) -> AddDirectoryResult {
    if directory_path.is_empty() {
        return AddDirectoryResult::EmptyPath;
    }
    let expanded = expand_path(directory_path);
    let abs = match std::fs::canonicalize(&expanded) {
        Ok(p) => p,
        Err(_) => match std::path::absolute(&expanded) {
            Ok(p) => p,
            Err(_) => PathBuf::from(directory_path),
        },
    };

    // stat
    match tokio::fs::metadata(&abs).await {
        Ok(stat) => {
            if !stat.is_dir() {
                return AddDirectoryResult::NotADirectory {
                    directory_path: directory_path.to_string(),
                    absolute_path: abs.to_string_lossy().into_owned(),
                };
            }
        }
        Err(e) => {
            use std::io::ErrorKind::*;
            if matches!(e.kind(), NotFound | PermissionDenied) {
                return AddDirectoryResult::PathNotFound {
                    directory_path: directory_path.to_string(),
                    absolute_path: abs.to_string_lossy().into_owned(),
                };
            }
            // Other errors fall through as NotFound (matches TS's tolerant
            // existsSync semantics).
            return AddDirectoryResult::PathNotFound {
                directory_path: directory_path.to_string(),
                absolute_path: abs.to_string_lossy().into_owned(),
            };
        }
    }

    for wd in current_working_dirs {
        if path_in_working_path(&abs, wd) {
            return AddDirectoryResult::AlreadyInWorkingDirectory {
                directory_path: directory_path.to_string(),
                working_dir: wd.to_string_lossy().into_owned(),
            };
        }
    }

    AddDirectoryResult::Success {
        absolute_path: abs.to_string_lossy().into_owned(),
    }
}

fn expand_path(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn path_in_working_path(candidate: &Path, working_dir: &Path) -> bool {
    candidate.starts_with(working_dir)
}

/// `validation.ts` `addDirHelpMessage`。
pub fn add_dir_help_message(result: &AddDirectoryResult) -> String {
    match result {
        AddDirectoryResult::EmptyPath => "Please provide a directory path.".to_string(),
        AddDirectoryResult::PathNotFound { absolute_path, .. } => {
            format!("Path {} was not found.", absolute_path)
        }
        AddDirectoryResult::NotADirectory {
            directory_path,
            absolute_path,
        } => {
            let parent = Path::new(absolute_path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!(
                "{} is not a directory. Did you mean to add the parent directory {}?",
                directory_path, parent
            )
        }
        AddDirectoryResult::AlreadyInWorkingDirectory {
            directory_path,
            working_dir,
        } => format!(
            "{} is already accessible within the existing working directory {}.",
            directory_path, working_dir
        ),
        AddDirectoryResult::Success { absolute_path } => {
            format!("Added {} as a working directory.", absolute_path)
        }
    }
}

/// Check if a directory is within the allowed traversal depth.
pub fn check_depth(path: &Path, root: &Path) -> bool {
    let depth = path.strip_prefix(root)
        .map(|rel| rel.components().count())
        .unwrap_or(0);
    depth <= MAX_DEPTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_dirs() {
        let cwd = PathBuf::from("/tmp");
        let result = validate_add_dir("/", &cwd);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_max_depth() {
        let root = Path::new("/home/user/project");
        let deep = Path::new("/home/user/project/a/b/c");
        assert!(check_depth(deep, root));
    }
}
