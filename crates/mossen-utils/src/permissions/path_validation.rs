//! Path validation for file system permissions.
//!
//! Handles path expansion, glob patterns, dangerous removal paths,
//! and comprehensive path validation for read/write/create operations.

use std::path::Path;

use super::permission_result::{PermissionDecisionReason, ToolPermissionContext};

const MAX_DIRS_TO_LIST: usize = 5;

/// File operation type for permission checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperationType {
    Read,
    Write,
    Create,
}

/// Result of a path permission check.
#[derive(Debug, Clone)]
pub struct PathCheckResult {
    pub allowed: bool,
    pub decision_reason: Option<PermissionDecisionReason>,
}

/// Result of a resolved path permission check.
#[derive(Debug, Clone)]
pub struct ResolvedPathCheckResult {
    pub allowed: bool,
    pub resolved_path: String,
    pub decision_reason: Option<PermissionDecisionReason>,
}

/// Format a list of directories for display.
pub fn format_directory_list(directories: &[String]) -> String {
    let dir_count = directories.len();

    if dir_count <= MAX_DIRS_TO_LIST {
        return directories
            .iter()
            .map(|dir| format!("'{}'", dir))
            .collect::<Vec<_>>()
            .join(", ");
    }

    let first_dirs: String = directories[..MAX_DIRS_TO_LIST]
        .iter()
        .map(|dir| format!("'{}'", dir))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}, and {} more", first_dirs, dir_count - MAX_DIRS_TO_LIST)
}

/// Extracts the base directory from a glob pattern for validation.
/// For example: "/path/to/*.txt" returns "/path/to"
pub fn get_glob_base_directory(path: &str) -> &str {
    let glob_chars = ['*', '?', '[', ']', '{', '}'];
    let glob_pos = path.find(|c| glob_chars.contains(&c));

    match glob_pos {
        None => path,
        Some(pos) => {
            let before_glob = &path[..pos];
            match before_glob.rfind('/') {
                None => ".",
                Some(0) => "/",
                Some(idx) => &before_glob[..idx],
            }
        }
    }
}

/// Expands tilde (~) at the start of a path to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    // Windows variant
    if cfg!(windows) && path.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}

/// Checks if a resolved path is dangerous for removal operations (rm/rmdir).
pub fn is_dangerous_removal_path(resolved_path: &str) -> bool {
    // Collapse runs of slashes
    let forward_slashed: String = resolved_path.chars().fold(String::new(), |mut acc, c| {
        if c == '\\' || c == '/' {
            if !acc.ends_with('/') {
                acc.push('/');
            }
        } else {
            acc.push(c);
        }
        acc
    });

    if forward_slashed == "*" || forward_slashed.ends_with("/*") {
        return true;
    }

    let normalized_path = if forward_slashed == "/" {
        forward_slashed.clone()
    } else {
        forward_slashed.trim_end_matches('/').to_string()
    };

    if normalized_path == "/" {
        return true;
    }

    // Windows drive root (C:/, D:/)
    let windows_drive_root = regex::Regex::new(r"^[A-Za-z]:/?$").unwrap();
    if windows_drive_root.is_match(&normalized_path) {
        return true;
    }

    // Home directory
    if let Some(home) = dirs::home_dir() {
        let normalized_home = home.to_string_lossy().replace('\\', "/");
        if normalized_path == normalized_home {
            return true;
        }
    }

    // Direct children of root: /usr, /tmp, /etc (but not /usr/local)
    let parent = Path::new(&normalized_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    if parent == "/" {
        return true;
    }

    // Windows drive child (C:\Windows, C:\Users)
    let windows_drive_child = regex::Regex::new(r"^[A-Za-z]:/[^/]+$").unwrap();
    if windows_drive_child.is_match(&normalized_path) {
        return true;
    }

    false
}

/// Validates a file system path, handling tilde expansion and glob patterns.
/// Returns whether the path is allowed and the resolved path for error messages.
pub fn validate_path(
    path: &str,
    cwd: &str,
    tool_permission_context: &ToolPermissionContext,
    operation_type: FileOperationType,
    // Functions that the caller provides for path resolution and permission checking
    _contains_path_traversal: impl Fn(&str) -> bool,
    contains_vulnerable_unc_path: impl Fn(&str) -> bool,
    safe_resolve_path: impl Fn(&str) -> (String, bool),
    is_path_allowed: impl Fn(&str, &ToolPermissionContext, FileOperationType) -> PathCheckResult,
) -> ResolvedPathCheckResult {
    // Remove surrounding quotes if present
    let clean_path = expand_tilde(path.trim_matches(|c| c == '\'' || c == '"'));

    // SECURITY: Block UNC paths that could leak credentials
    if contains_vulnerable_unc_path(&clean_path) {
        return ResolvedPathCheckResult {
            allowed: false,
            resolved_path: clean_path,
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "UNC network paths require manual approval".to_string(),
            }),
        };
    }

    // SECURITY: Reject tilde variants
    if clean_path.starts_with('~') {
        return ResolvedPathCheckResult {
            allowed: false,
            resolved_path: clean_path,
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Tilde expansion variants (~user, ~+, ~-) in paths require manual approval"
                    .to_string(),
            }),
        };
    }

    // SECURITY: Reject shell expansion syntax
    if clean_path.contains('$') || clean_path.contains('%') || clean_path.starts_with('=') {
        return ResolvedPathCheckResult {
            allowed: false,
            resolved_path: clean_path,
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Shell expansion syntax in paths requires manual approval".to_string(),
            }),
        };
    }

    // SECURITY: Block glob patterns in write/create operations
    let glob_chars = ['*', '?', '[', ']', '{', '}'];
    let has_glob = clean_path.contains(|c| glob_chars.contains(&c));

    if has_glob {
        if operation_type == FileOperationType::Write || operation_type == FileOperationType::Create
        {
            return ResolvedPathCheckResult {
                allowed: false,
                resolved_path: clean_path,
                decision_reason: Some(PermissionDecisionReason::Other {
                    reason:
                        "Glob patterns are not allowed in write operations. Please specify an exact file path."
                            .to_string(),
                }),
            };
        }

        // For read operations, validate the base directory
        let base_path = get_glob_base_directory(&clean_path);
        let absolute_base = if Path::new(base_path).is_absolute() {
            base_path.to_string()
        } else {
            format!("{}/{}", cwd, base_path)
        };
        let (resolved_path, _is_canonical) = safe_resolve_path(&absolute_base);
        let result = is_path_allowed(&resolved_path, tool_permission_context, operation_type);
        return ResolvedPathCheckResult {
            allowed: result.allowed,
            resolved_path,
            decision_reason: result.decision_reason,
        };
    }

    // Resolve path
    let absolute_path = if Path::new(&clean_path).is_absolute() {
        clean_path.clone()
    } else {
        format!("{}/{}", cwd, clean_path)
    };
    let (resolved_path, _is_canonical) = safe_resolve_path(&absolute_path);

    let result = is_path_allowed(&resolved_path, tool_permission_context, operation_type);
    ResolvedPathCheckResult {
        allowed: result.allowed,
        resolved_path,
        decision_reason: result.decision_reason,
    }
}

/// 对应 TS `isPathInSandboxWriteAllowlist`：判断路径是否在沙箱可写白名单内。
pub fn is_path_in_sandbox_write_allowlist(path: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|allowed| path.starts_with(allowed))
}

/// 对应 TS `isPathAllowed`：判断路径是否被工具允许访问。
pub fn is_path_allowed(path: &str, allowed_roots: &[String]) -> bool {
    if allowed_roots.is_empty() {
        return true;
    }
    allowed_roots.iter().any(|root| path.starts_with(root))
}

/// 对应 TS `validateGlobPattern`：校验 glob pattern 合法性。
pub fn validate_glob_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("empty glob pattern".to_string());
    }
    if pattern.contains("..") {
        return Err("glob pattern must not contain `..`".to_string());
    }
    Ok(())
}
