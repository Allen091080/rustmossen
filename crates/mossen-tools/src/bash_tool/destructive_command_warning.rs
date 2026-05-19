//! Detects potentially destructive bash commands and returns a warning string
//! for display in the permission dialog. This is purely informational — it
//! doesn't affect permission logic or auto-approval.

use regex::Regex;

/// A pattern that detects a destructive command and its associated warning.
struct DestructivePattern {
    pattern: Regex,
    warning: &'static str,
}

/// Lazily build the list of destructive patterns.
fn destructive_patterns() -> Vec<DestructivePattern> {
    vec![
        // Git — data loss / hard to reverse
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+reset\s+--hard\b").unwrap(),
            warning: "Note: may discard uncommitted changes",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+push\b[^;&|\n]*[ \t](--force|--force-with-lease|-f)\b").unwrap(),
            warning: "Note: may overwrite remote history",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+clean\b(?![^;&|\n]*(?:-[a-zA-Z]*n|--dry-run))[^;&|\n]*-[a-zA-Z]*f").unwrap(),
            warning: "Note: may permanently delete untracked files",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+checkout\s+(--\s+)?\.\s*($|[;&|\n])").unwrap(),
            warning: "Note: may discard all working tree changes",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+restore\s+(--\s+)?\.\s*($|[;&|\n])").unwrap(),
            warning: "Note: may discard all working tree changes",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+stash\s+(drop|clear)\b").unwrap(),
            warning: "Note: may permanently remove stashed changes",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+branch\s+(-D\s|--delete\s+--force|--force\s+--delete)\b").unwrap(),
            warning: "Note: may force-delete a branch",
        },
        // Git — safety bypass
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+(commit|push|merge)\b[^;&|\n]*--no-verify\b").unwrap(),
            warning: "Note: may skip safety hooks",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bgit\s+commit\b[^;&|\n]*--amend\b").unwrap(),
            warning: "Note: may rewrite the last commit",
        },
        // File deletion
        DestructivePattern {
            pattern: Regex::new(r"(^|[;&|\n]\s*)rm\s+-[a-zA-Z]*[rR][a-zA-Z]*f|(^|[;&|\n]\s*)rm\s+-[a-zA-Z]*f[a-zA-Z]*[rR]").unwrap(),
            warning: "Note: may recursively force-remove files",
        },
        DestructivePattern {
            pattern: Regex::new(r"(^|[;&|\n]\s*)rm\s+-[a-zA-Z]*[rR]").unwrap(),
            warning: "Note: may recursively remove files",
        },
        DestructivePattern {
            pattern: Regex::new(r"(^|[;&|\n]\s*)rm\s+-[a-zA-Z]*f").unwrap(),
            warning: "Note: may force-remove files",
        },
        // Database
        DestructivePattern {
            pattern: Regex::new(r"(?i)\b(DROP|TRUNCATE)\s+(TABLE|DATABASE|SCHEMA)\b").unwrap(),
            warning: "Note: may drop or truncate database objects",
        },
        DestructivePattern {
            pattern: Regex::new(r#"(?i)\bDELETE\s+FROM\s+\w+\s*(;|"|\x27|\n|$)"#).unwrap(),
            warning: "Note: may delete all rows from a database table",
        },
        // Infrastructure
        DestructivePattern {
            pattern: Regex::new(r"\bkubectl\s+delete\b").unwrap(),
            warning: "Note: may delete Kubernetes resources",
        },
        DestructivePattern {
            pattern: Regex::new(r"\bterraform\s+destroy\b").unwrap(),
            warning: "Note: may destroy Terraform infrastructure",
        },
    ]
}

/// Checks if a bash command matches known destructive patterns.
/// Returns a human-readable warning string, or `None` if no destructive pattern is detected.
pub fn get_destructive_command_warning(command: &str) -> Option<&'static str> {
    let patterns = destructive_patterns();
    for dp in &patterns {
        if dp.pattern.is_match(command) {
            return Some(dp.warning);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_reset_hard() {
        let w = get_destructive_command_warning("git reset --hard HEAD~1");
        assert_eq!(w, Some("Note: may discard uncommitted changes"));
    }

    #[test]
    fn test_git_push_force() {
        let w = get_destructive_command_warning("git push origin main --force");
        assert_eq!(w, Some("Note: may overwrite remote history"));
    }

    #[test]
    fn test_rm_rf() {
        let w = get_destructive_command_warning("rm -rf /tmp/foo");
        assert_eq!(w, Some("Note: may recursively force-remove files"));
    }

    #[test]
    fn test_safe_command() {
        assert!(get_destructive_command_warning("ls -la").is_none());
        assert!(get_destructive_command_warning("git status").is_none());
    }

    #[test]
    fn test_kubectl_delete() {
        let w = get_destructive_command_warning("kubectl delete pod my-pod");
        assert_eq!(w, Some("Note: may delete Kubernetes resources"));
    }
}
