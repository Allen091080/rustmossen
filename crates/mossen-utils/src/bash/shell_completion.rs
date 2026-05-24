//! Shell completion utilities.
//!
//! Translated from `shellCompletion.ts` (260 lines).

use regex::Regex;

use crate::bash::shell_quote::{quote, try_parse_shell_command};

const MAX_SHELL_COMPLETIONS: usize = 15;
const SHELL_COMPLETION_TIMEOUT_MS: u64 = 1000;

/// Shell completion type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCompletionType {
    Command,
    Variable,
    File,
}

/// Input context for completions.
#[derive(Debug, Clone)]
pub struct InputContext {
    pub prefix: String,
    pub completion_type: ShellCompletionType,
}

/// A suggestion item for shell completions.
#[derive(Debug, Clone)]
pub struct SuggestionItem {
    pub id: String,
    pub display_text: String,
    pub description: Option<String>,
    pub completion_type: ShellCompletionType,
    pub input_snapshot: Option<String>,
}

/// Check if a parsed token is a command operator.
fn is_command_operator(token: &str) -> bool {
    matches!(token, "|" | "||" | "&&" | ";")
}

/// Determine completion type based solely on prefix characteristics.
fn get_completion_type_from_prefix(prefix: &str) -> ShellCompletionType {
    if prefix.starts_with('$') {
        return ShellCompletionType::Variable;
    }
    if prefix.contains('/') || prefix.starts_with('~') || prefix.starts_with('.') {
        return ShellCompletionType::File;
    }
    ShellCompletionType::Command
}

/// Find the last string token and its index in parsed tokens.
fn find_last_string_token(tokens: &[String]) -> Option<(String, usize)> {
    for (i, token) in tokens.iter().enumerate().rev() {
        // In our simplified parser, all tokens are strings
        // We need to distinguish operators from values
        if !is_command_operator(token) && !matches!(token.as_str(), ">" | ">>" | "<" | "<<" | ">&")
        {
            return Some((token.clone(), i));
        }
    }
    None
}

/// Check if we're in a context that expects a new command.
fn is_new_command_context(tokens: &[String], current_token_index: usize) -> bool {
    if current_token_index == 0 {
        return true;
    }
    if let Some(prev_token) = tokens.get(current_token_index - 1) {
        return is_command_operator(prev_token);
    }
    false
}

/// Parse input to extract completion context.
pub fn parse_input_context(input: &str, cursor_offset: usize) -> InputContext {
    let before_cursor = &input[..std::cmp::min(cursor_offset, input.len())];

    // Check if it's a variable prefix
    let var_re = Regex::new(r"\$[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    if let Some(m) = var_re.find(before_cursor) {
        return InputContext {
            prefix: m.as_str().to_string(),
            completion_type: ShellCompletionType::Variable,
        };
    }

    // Parse with shell-quote
    let parse_result = try_parse_shell_command(before_cursor);
    if parse_result.is_none() {
        // Fallback to simple parsing
        let tokens: Vec<&str> = before_cursor.split_whitespace().collect();
        let prefix = tokens.last().map(|s| s.to_string()).unwrap_or_default();
        let is_first_token = tokens.len() == 1 && !before_cursor.contains(' ');
        let completion_type = if is_first_token {
            ShellCompletionType::Command
        } else {
            get_completion_type_from_prefix(&prefix)
        };
        return InputContext {
            prefix,
            completion_type,
        };
    }

    let tokens = parse_result.unwrap();

    // Extract current token
    let last_token = find_last_string_token(&tokens);
    if last_token.is_none() {
        let completion_type = if let Some(last) = tokens.last() {
            if is_command_operator(last) {
                ShellCompletionType::Command
            } else {
                ShellCompletionType::Command
            }
        } else {
            ShellCompletionType::Command
        };
        return InputContext {
            prefix: String::new(),
            completion_type,
        };
    }

    let (token, index) = last_token.unwrap();

    // If there's a trailing space, the user is starting a new argument
    if before_cursor.ends_with(' ') {
        return InputContext {
            prefix: String::new(),
            completion_type: ShellCompletionType::File,
        };
    }

    // Determine completion type from context
    let base_type = get_completion_type_from_prefix(&token);
    if base_type == ShellCompletionType::Variable || base_type == ShellCompletionType::File {
        return InputContext {
            prefix: token,
            completion_type: base_type,
        };
    }

    let completion_type = if is_new_command_context(&tokens, index) {
        ShellCompletionType::Command
    } else {
        ShellCompletionType::File
    };

    InputContext {
        prefix: token,
        completion_type,
    }
}

/// Generate bash completion command using compgen.
pub fn get_bash_completion_command(prefix: &str, completion_type: ShellCompletionType) -> String {
    match completion_type {
        ShellCompletionType::Variable => {
            let var_name = if prefix.starts_with('$') {
                &prefix[1..]
            } else {
                prefix
            };
            format!("compgen -v {} 2>/dev/null", quote(&[var_name]))
        }
        ShellCompletionType::File => {
            format!(
                "compgen -f {} 2>/dev/null | head -{} | while IFS= read -r f; do [ -d \"$f\" ] && echo \"$f/\" || echo \"$f \"; done",
                quote(&[prefix]),
                MAX_SHELL_COMPLETIONS
            )
        }
        ShellCompletionType::Command => {
            format!("compgen -c {} 2>/dev/null", quote(&[prefix]))
        }
    }
}

/// Generate zsh completion command.
pub fn get_zsh_completion_command(prefix: &str, completion_type: ShellCompletionType) -> String {
    match completion_type {
        ShellCompletionType::Variable => {
            let var_name = if prefix.starts_with('$') {
                &prefix[1..]
            } else {
                prefix
            };
            format!(
                "print -rl -- ${{(k)parameters[(I){}*]}} 2>/dev/null",
                quote(&[var_name])
            )
        }
        ShellCompletionType::File => {
            format!(
                "for f in {}*(N[1,{}]); do [[ -d \"$f\" ]] && echo \"$f/\" || echo \"$f \"; done",
                quote(&[prefix]),
                MAX_SHELL_COMPLETIONS
            )
        }
        ShellCompletionType::Command => {
            format!(
                "print -rl -- ${{(k)commands[(I){}*]}} 2>/dev/null",
                quote(&[prefix])
            )
        }
    }
}

/// 对应 TS `getShellCompletions`：组合 [`parse_input_context`] 与 shell 内置补
/// 全命令，异步执行并返回候选项。
///
/// `shell_type` 仅支持 `"bash"` / `"zsh"`，与 TS 端 `Shell.exec` 限制一致；
/// 其他值返回空结果。命令执行使用 `tokio::process::Command` + 1s 超时。
pub async fn get_shell_completions(
    input: &str,
    cursor_offset: usize,
    shell_type: &str,
) -> Vec<SuggestionItem> {
    let Some(command) = prepare_shell_completion(input, cursor_offset, shell_type) else {
        return Vec::new();
    };
    let output = tokio::time::timeout(
        std::time::Duration::from_millis(1000),
        tokio::process::Command::new("bash")
            .arg("-lc")
            .arg(&command)
            .output(),
    )
    .await;
    let stdout = match output {
        Ok(Ok(out)) => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Vec::new(),
    };
    let context = parse_input_context(input, cursor_offset);
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(15)
        .map(|line| SuggestionItem {
            id: line.to_string(),
            display_text: line.to_string(),
            description: None,
            completion_type: context.completion_type,
            input_snapshot: Some(input.to_string()),
        })
        .collect()
}

/// Get shell completions for the given input (synchronous data preparation).
/// The actual shell execution must be done by the caller.
pub fn prepare_shell_completion(
    input: &str,
    cursor_offset: usize,
    shell_type: &str,
) -> Option<String> {
    if shell_type != "bash" && shell_type != "zsh" {
        return None;
    }

    let context = parse_input_context(input, cursor_offset);
    if context.prefix.is_empty() {
        return None;
    }

    let command = match shell_type {
        "bash" => get_bash_completion_command(&context.prefix, context.completion_type),
        "zsh" => get_zsh_completion_command(&context.prefix, context.completion_type),
        _ => return None,
    };

    Some(command)
}
