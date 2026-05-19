//! Command semantics configuration for interpreting exit codes in different contexts.
//!
//! Many commands use exit codes to convey information other than just success/failure.
//! For example, grep returns 1 when no matches are found, which is not an error condition.

/// Result of interpreting a command's exit code.
#[derive(Debug, Clone)]
pub struct CommandInterpretation {
    pub is_error: bool,
    pub message: Option<String>,
}

/// Type alias for a semantic function that interprets exit codes.
type CommandSemantic = fn(exit_code: i32, stdout: &str, stderr: &str) -> CommandInterpretation;

/// Default semantic: treat only 0 as success, everything else as error.
fn default_semantic(exit_code: i32, _stdout: &str, _stderr: &str) -> CommandInterpretation {
    CommandInterpretation {
        is_error: exit_code != 0,
        message: if exit_code != 0 {
            Some(format!("Command failed with exit code {}", exit_code))
        } else {
            None
        },
    }
}

/// grep/rg: 0=matches found, 1=no matches, 2+=error
fn grep_semantic(exit_code: i32, _stdout: &str, _stderr: &str) -> CommandInterpretation {
    CommandInterpretation {
        is_error: exit_code >= 2,
        message: if exit_code == 1 {
            Some("No matches found".to_string())
        } else {
            None
        },
    }
}

/// find: 0=success, 1=partial success (some dirs inaccessible), 2+=error
fn find_semantic(exit_code: i32, _stdout: &str, _stderr: &str) -> CommandInterpretation {
    CommandInterpretation {
        is_error: exit_code >= 2,
        message: if exit_code == 1 {
            Some("Some directories were inaccessible".to_string())
        } else {
            None
        },
    }
}

/// diff: 0=no differences, 1=differences found, 2+=error
fn diff_semantic(exit_code: i32, _stdout: &str, _stderr: &str) -> CommandInterpretation {
    CommandInterpretation {
        is_error: exit_code >= 2,
        message: if exit_code == 1 {
            Some("Files differ".to_string())
        } else {
            None
        },
    }
}

/// test/[: 0=condition true, 1=condition false, 2+=error
fn test_semantic(exit_code: i32, _stdout: &str, _stderr: &str) -> CommandInterpretation {
    CommandInterpretation {
        is_error: exit_code >= 2,
        message: if exit_code == 1 {
            Some("Condition is false".to_string())
        } else {
            None
        },
    }
}

/// Get the semantic interpretation function for a command.
fn get_command_semantic(command: &str) -> CommandSemantic {
    let base_command = heuristically_extract_base_command(command);
    match base_command.as_str() {
        "grep" | "rg" | "ag" | "ack" => grep_semantic,
        "find" => find_semantic,
        "diff" => diff_semantic,
        "test" | "[" => test_semantic,
        _ => default_semantic,
    }
}

/// Extract just the command name (first word) from a single command string.
fn extract_base_command(command: &str) -> String {
    command
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Split a command by `&&`, `||`, and `;` operators (deprecated simple split).
fn split_command_deprecated(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if escape_next {
            current.push(c);
            escape_next = false;
            i += 1;
            continue;
        }

        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if !in_single_quote && !in_double_quote {
            // Check for && or ||
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if (c == '&' && next == '&') || (c == '|' && next == '|') {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        segments.push(trimmed);
                    }
                    current.clear();
                    i += 2;
                    continue;
                }
            }
            // Check for ;
            if c == ';' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current.clear();
                i += 1;
                continue;
            }
        }

        current.push(c);
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Extract the primary command from a complex command line.
/// May get it wrong — don't depend on this for security.
fn heuristically_extract_base_command(command: &str) -> String {
    let segments = split_command_deprecated(command);
    // Take the last command as that determines the exit code
    let last_command = segments.last().map(|s| s.as_str()).unwrap_or(command);
    extract_base_command(last_command)
}

/// Interpret command result based on semantic rules.
pub fn interpret_command_result(
    command: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> CommandInterpretation {
    let semantic = get_command_semantic(command);
    semantic(exit_code, stdout, stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_no_matches() {
        let result = interpret_command_result("grep foo bar.txt", 1, "", "");
        assert!(!result.is_error);
        assert_eq!(result.message.as_deref(), Some("No matches found"));
    }

    #[test]
    fn test_grep_error() {
        let result = interpret_command_result("grep foo bar.txt", 2, "", "No such file");
        assert!(result.is_error);
    }

    #[test]
    fn test_diff_differences() {
        let result = interpret_command_result("diff a.txt b.txt", 1, "< line", "");
        assert!(!result.is_error);
        assert_eq!(result.message.as_deref(), Some("Files differ"));
    }

    #[test]
    fn test_default_failure() {
        let result = interpret_command_result("make build", 2, "", "error");
        assert!(result.is_error);
    }

    #[test]
    fn test_default_success() {
        let result = interpret_command_result("ls", 0, "file.txt", "");
        assert!(!result.is_error);
        assert!(result.message.is_none());
    }

    #[test]
    fn test_compound_command_last_segment() {
        // The last segment determines the semantic
        let result = interpret_command_result("cd dir && grep pattern file", 1, "", "");
        assert!(!result.is_error); // grep semantics apply
    }
}
