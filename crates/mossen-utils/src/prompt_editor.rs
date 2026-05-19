//! Prompt editor — open prompt text in an external editor for editing.

use std::path::Path;
use std::process::Command;

/// Result of editing a file in an external editor.
pub struct EditorResult {
    pub content: Option<String>,
    pub error: Option<String>,
}

/// Map of editor command overrides (e.g., to add wait flags).
fn get_editor_override(editor: &str) -> Option<&'static str> {
    match editor {
        "code" => Some("code -w"),
        "subl" => Some("subl --wait"),
        _ => None,
    }
}

/// Edit a file in the user's external editor (synchronous, blocking).
pub fn edit_file_in_editor(file_path: &Path, editor: &str) -> EditorResult {
    if !file_path.exists() {
        return EditorResult {
            content: None,
            error: None,
        };
    }

    let editor_command = get_editor_override(editor).unwrap_or(editor);
    let file_str = file_path.to_string_lossy();

    let status = Command::new("sh")
        .args(["-c", &format!("{} \"{}\"", editor_command, file_str)])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(exit_status) => {
            if !exit_status.success() {
                let code = exit_status.code().unwrap_or(-1);
                return EditorResult {
                    content: None,
                    error: Some(format!("{} exited with code {}", editor, code)),
                };
            }
            match std::fs::read_to_string(file_path) {
                Ok(content) => EditorResult {
                    content: Some(content),
                    error: None,
                },
                Err(_) => EditorResult {
                    content: None,
                    error: None,
                },
            }
        }
        Err(_) => EditorResult {
            content: None,
            error: None,
        },
    }
}

/// Re-collapse expanded pasted text by finding content that matches and replacing with refs.
fn recollapse_pasted_content(
    edited_prompt: &str,
    _original_prompt: &str,
    pasted_contents: &[(usize, String)],
) -> String {
    let mut collapsed = edited_prompt.to_string();

    for (id, content) in pasted_contents {
        if let Some(idx) = collapsed.find(content.as_str()) {
            let num_lines = content.lines().count();
            let reference = format!("[pasted text #{} ({} lines)]", id, num_lines);
            collapsed = format!(
                "{}{}{}",
                &collapsed[..idx],
                reference,
                &collapsed[idx + content.len()..]
            );
        }
    }

    collapsed
}

/// Edit a prompt in the user's external editor with support for pasted content expansion.
pub fn edit_prompt_in_editor(
    current_prompt: &str,
    editor: &str,
    pasted_contents: Option<&[(usize, String)]>,
) -> EditorResult {
    let temp_file = std::env::temp_dir().join(format!(
        "mossen-prompt-{}.txt",
        uuid::Uuid::new_v4()
    ));

    // Expand pasted text references before editing
    let expanded_prompt = if let Some(contents) = pasted_contents {
        let mut expanded = current_prompt.to_string();
        for (id, content) in contents {
            let num_lines = content.lines().count();
            let reference = format!("[pasted text #{} ({} lines)]", id, num_lines);
            expanded = expanded.replace(&reference, content);
        }
        expanded
    } else {
        current_prompt.to_string()
    };

    // Write expanded prompt to temp file
    if let Err(e) = std::fs::write(&temp_file, &expanded_prompt) {
        return EditorResult {
            content: None,
            error: Some(format!("Failed to write temp file: {}", e)),
        };
    }

    // Edit the file
    let result = edit_file_in_editor(&temp_file, editor);

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);

    match result.content {
        None => result,
        Some(ref content) => {
            // Trim a single trailing newline if present (common editor behavior)
            let mut final_content = content.clone();
            if final_content.ends_with('\n') && !final_content.ends_with("\n\n") {
                final_content.pop();
            }

            // Re-collapse pasted content if it wasn't edited
            if let Some(contents) = pasted_contents {
                final_content =
                    recollapse_pasted_content(&final_content, current_prompt, contents);
            }

            EditorResult {
                content: Some(final_content),
                error: None,
            }
        }
    }
}
