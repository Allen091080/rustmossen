//! If the first line of a bash command is a `# comment` (not a `#!` shebang),
//! return the comment text stripped of the `#` prefix. Otherwise None.
//!
//! Under fullscreen mode this is the non-verbose tool-use label AND the
//! collapse-group hint — it's what Mossen wrote for the human to read.

/// Extract comment label from the first line of a bash command.
/// Returns `None` if the first line is not a `# comment` or is a shebang.
pub fn extract_bash_comment_label(command: &str) -> Option<String> {
    let first_line = match command.find('\n') {
        Some(pos) => &command[..pos],
        None => command,
    };
    let trimmed = first_line.trim();
    if !trimmed.starts_with('#') || trimmed.starts_with("#!") {
        return None;
    }
    let stripped = trimmed.trim_start_matches('#').trim_start();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_label() {
        assert_eq!(
            extract_bash_comment_label("# Build the project\nmake"),
            Some("Build the project".to_string())
        );
        assert_eq!(extract_bash_comment_label("#!/bin/bash\necho hi"), None);
        assert_eq!(extract_bash_comment_label("echo hello"), None);
        assert_eq!(extract_bash_comment_label("#\necho"), None);
        assert_eq!(
            extract_bash_comment_label("## Multi hash\nfoo"),
            Some("Multi hash".to_string())
        );
    }
}
