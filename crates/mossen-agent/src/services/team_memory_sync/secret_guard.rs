//! Team memory secret guard — prevents writing secrets into team memory files.

use super::secret_scanner::scan_for_secrets;
use std::path::Path;

/// Check if a team memory path prefix matches (simplified heuristic).
fn is_team_mem_path(file_path: &str) -> bool {
    let path = Path::new(file_path);
    // Check if the path contains a team memory directory marker
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == ".mossen" || s == "team-memory"
    })
}

/// Check if a file write/edit to a team memory path contains secrets.
/// Returns an error message if secrets are detected, or None if safe.
///
/// This is called from FileWriteTool and FileEditTool validateInput to
/// prevent the model from writing secrets into team memory files, which
/// would be synced to all repository collaborators.
pub fn check_team_mem_secrets(file_path: &str, content: &str) -> Option<String> {
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
