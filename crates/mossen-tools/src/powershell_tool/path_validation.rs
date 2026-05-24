//! # path_validation — PowerShell path validation
//!
//! Translates `tools/PowerShellTool/pathValidation.ts`.
//! Validates that PowerShell commands don't write to dangerous paths.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::git_safety::is_git_internal_path_ps;

/// Dangerous file names that should never be written to.
static DANGEROUS_FILES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "autorun.inf",
        "desktop.ini",
        ".bashrc",
        ".bash_profile",
        ".zshrc",
        ".profile",
        ".login",
        ".cshrc",
        ".tcshrc",
        ".kshrc",
        ".bash_login",
        ".bash_logout",
        ".zlogin",
        ".zlogout",
        ".zprofile",
        ".zshenv",
        ".xsession",
        ".xinitrc",
        ".xprofile",
        "authorized_keys",
        "known_hosts",
        "id_rsa",
        "id_ed25519",
        "config", // SSH config
    ]
    .iter()
    .copied()
    .collect()
});

/// Dangerous directory patterns.
static DANGEROUS_DIRECTORIES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        ".git",
        ".ssh",
        ".gnupg",
        ".config/systemd",
        "system32",
        "syswow64",
        "windows",
        "/etc",
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
        "program files",
        "programdata",
    ]
});

/// Write cmdlets and their aliases.
static WRITE_CMDLETS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "set-content",
        "sc",
        "out-file",
        "add-content",
        "ac",
        "new-item",
        "ni",
        "mkdir",
        "md",
        "copy-item",
        "cp",
        "copy",
        "cpi",
        "move-item",
        "mv",
        "move",
        "mi",
        "rename-item",
        "ren",
        "rni",
        "remove-item",
        "rm",
        "del",
        "rd",
        "rmdir",
        "ri",
        "clear-content",
        "clc",
        "invoke-webrequest",
        "iwr",
    ]
    .iter()
    .copied()
    .collect()
});

/// Result of path validation.
#[derive(Debug, Clone)]
pub enum PathValidationResult {
    /// Path is safe to use.
    Safe,
    /// Path is dangerous and should be blocked.
    Dangerous { reason: String },
}

/// Validate a command's path arguments for dangerous writes.
pub fn validate_paths(command: &str) -> PathValidationResult {
    // Parse the command to extract paths being written to
    let paths = extract_write_paths(command);

    for path in &paths {
        let lower = path.to_lowercase();
        let normalized = lower.replace('\\', "/");

        // Check for dangerous files
        let filename = normalized.rsplit('/').next().unwrap_or(&normalized);
        if DANGEROUS_FILES.contains(filename) {
            return PathValidationResult::Dangerous {
                reason: format!("Writing to sensitive file '{}' is not allowed", filename),
            };
        }

        // Check for dangerous directories
        for dir in DANGEROUS_DIRECTORIES.iter() {
            if normalized.contains(dir) {
                return PathValidationResult::Dangerous {
                    reason: format!("Writing to sensitive directory '{}' is not allowed", dir),
                };
            }
        }

        // Check for git-internal paths
        if is_git_internal_path_ps(path) {
            return PathValidationResult::Dangerous {
                reason:
                    "Writing to git-internal paths (.git/, hooks/, refs/, objects/) is not allowed"
                        .to_string(),
            };
        }

        // Check for parent traversal to escape cwd
        if has_dangerous_traversal(&normalized) {
            return PathValidationResult::Dangerous {
                reason: "Path traversal outside the working directory is not allowed".to_string(),
            };
        }
    }

    PathValidationResult::Safe
}

/// Extract paths that are being written to from a command.
fn extract_write_paths(command: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Split into statements
    let statements: Vec<&str> = command.split([';', '\n']).collect();

    for stmt in statements {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();

        // Check if this statement uses a write cmdlet
        let first_token = lower.split_whitespace().next().unwrap_or("");
        let is_write = WRITE_CMDLETS.contains(first_token) || first_token.starts_with("& ");

        if !is_write {
            // Check for redirection operators
            if let Some(path) = extract_redirect_path(trimmed) {
                paths.push(path);
            }
            continue;
        }

        // Extract path argument (first non-parameter argument after cmdlet)
        if let Some(path) = extract_cmdlet_path_arg(trimmed) {
            paths.push(path);
        }
    }

    paths
}

/// Extract path from output redirection (> or >>).
fn extract_redirect_path(command: &str) -> Option<String> {
    // Look for > or >> followed by a path
    let re = Regex::new(r">{1,2}\s*([^\s;|]+)").ok()?;
    re.captures(command).and_then(|c| c.get(1)).map(|m| {
        m.as_str()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string()
    })
}

/// Extract the path argument from a write cmdlet invocation.
fn extract_cmdlet_path_arg(command: &str) -> Option<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    // Look for -Path, -LiteralPath, or -Destination parameter
    for i in 1..parts.len() {
        let lower = parts[i].to_lowercase();
        if lower == "-path" || lower == "-literalpath" || lower == "-destination" {
            if i + 1 < parts.len() {
                return Some(
                    parts[i + 1]
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_string(),
                );
            }
        }
    }

    // If no named parameter, take the first positional argument
    // (skip parameters that start with -)
    for i in 1..parts.len() {
        if !parts[i].starts_with('-') {
            return Some(parts[i].trim_matches(|c| c == '"' || c == '\'').to_string());
        }
    }

    None
}

/// Check if a normalized path has dangerous parent traversal.
fn has_dangerous_traversal(path: &str) -> bool {
    // Count .. segments
    let mut depth: i32 = 0;
    for component in path.split('/') {
        match component {
            ".." => {
                depth -= 1;
                if depth < -2 {
                    // More than 2 levels up is suspicious
                    return true;
                }
            }
            "" | "." => {}
            _ => {
                depth += 1;
            }
        }
    }
    // If we end up negative, we've escaped the working directory
    depth < 0
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/PowerShellTool/pathValidation.ts` additional exports.
// ---------------------------------------------------------------------------

const DANGEROUS_REMOVAL_PATHS: &[&str] = &[
    "/",
    "C:\\",
    "C:/",
    "/etc",
    "/usr",
    "/var",
    "/bin",
    "/sbin",
    "/System",
    "/Library",
    "C:\\Windows",
    "C:\\Program Files",
];

/// `pathValidation.ts` `isDangerousRemovalRawPath`.
pub fn is_dangerous_removal_raw_path(file_path: &str) -> bool {
    let normalized = file_path.trim().trim_matches(&['"', '\''] as &[char]);
    let normalized = normalized.trim_end_matches('/').trim_end_matches('\\');
    DANGEROUS_REMOVAL_PATHS
        .iter()
        .any(|p| normalized.eq_ignore_ascii_case(p))
        || normalized == "~"
        || normalized.is_empty()
}

/// Permission-result shape used by `dangerousRemovalDeny`.
#[derive(Debug, Clone)]
pub struct DangerousRemovalDenyResult {
    pub behavior: &'static str,
    pub message: String,
}

/// `pathValidation.ts` `dangerousRemovalDeny`.
pub fn dangerous_removal_deny(path: &str) -> DangerousRemovalDenyResult {
    DangerousRemovalDenyResult {
        behavior: "deny",
        message: format!("Removal of `{}` is denied for safety.", path),
    }
}
