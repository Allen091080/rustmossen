//! Team memory secret guard - prevents writing secrets into team memory files.

use super::{secret_scanner::scan_for_secrets, service};
use std::path::Path;

/// Check if a file write/edit to a team memory path contains secrets.
/// Returns an error message if secrets are detected, or None if safe.
///
/// This is called from FileWriteTool and FileEditTool validateInput to
/// prevent the model from writing secrets into team memory files, which
/// would be synced to all repository collaborators.
pub fn check_team_mem_secrets(file_path: impl AsRef<Path>, content: &str) -> Option<String> {
    check_team_mem_secrets_with_detector(file_path.as_ref(), content, |path| {
        service::is_team_memory_file_path(path)
    })
}

fn check_team_mem_secrets_with_detector(
    file_path: &Path,
    content: &str,
    is_team_mem_path: impl FnOnce(&Path) -> bool,
) -> Option<String> {
    if !is_team_mem_path(file_path) {
        return None;
    }

    let matches = scan_for_secrets(content);
    if matches.is_empty() {
        return None;
    }

    let labels: Vec<&str> = matches.iter().map(|m| m.label.as_str()).collect();
    Some(format!(
        "Content contains potential secrets ({}) and cannot be written to team memory. \
         Team memory is shared with all repository collaborators. \
         Remove the sensitive content and try again.",
        labels.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_guard_skips_non_team_memory_paths() {
        let message = check_team_mem_secrets_with_detector(
            Path::new("/workspace/project/notes.md"),
            "token = ghp_1234567890abcdef1234567890abcdef1234",
            |_| false,
        );

        assert!(message.is_none());
    }

    #[test]
    fn secret_guard_blocks_detected_secret_for_team_memory_path() {
        let message = check_team_mem_secrets_with_detector(
            Path::new("/memory/team/notes.md"),
            "token = ghp_1234567890abcdef1234567890abcdef1234",
            |_| true,
        )
        .expect("secret warning");

        assert!(message.contains("GitHub PAT"));
        assert!(message.contains("cannot be written to team memory"));
        assert!(!message.contains("ghp_"));
    }
}
