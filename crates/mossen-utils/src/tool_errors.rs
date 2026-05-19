use regex::Regex;

/// Interrupt message constant
pub const INTERRUPT_MESSAGE_FOR_TOOL_USE: &str =
    "The tool use was interrupted by the user. The tool was not executed.";

/// Shell error with exit code and output
#[derive(Debug)]
pub struct ShellError {
    pub message: String,
    pub code: i32,
    pub stderr: String,
    pub stdout: String,
    pub interrupted: bool,
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShellError(code={}): {}", self.code, self.message)
    }
}

impl std::error::Error for ShellError {}

/// Abort error
#[derive(Debug)]
pub struct AbortError {
    pub message: String,
}

impl std::fmt::Display for AbortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AbortError {}

/// Format error for display in tool results
pub fn format_error(error: &ToolError) -> String {
    match error {
        ToolError::Abort(e) => {
            if e.message.is_empty() {
                INTERRUPT_MESSAGE_FOR_TOOL_USE.to_string()
            } else {
                e.message.clone()
            }
        }
        ToolError::Shell(e) => {
            let parts = get_shell_error_parts(e);
            let full_message = parts
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            let full_message = if full_message.is_empty() {
                "Command failed with no output".to_string()
            } else {
                full_message
            };
            truncate_error_message(&full_message)
        }
        ToolError::Generic(e) => {
            let parts = get_generic_error_parts(e);
            let full_message = parts
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            let full_message = if full_message.is_empty() {
                "Command failed with no output".to_string()
            } else {
                full_message
            };
            truncate_error_message(&full_message)
        }
        ToolError::Other(msg) => msg.clone(),
    }
}

fn truncate_error_message(msg: &str) -> String {
    if msg.len() <= 10000 {
        return msg.to_string();
    }
    let half_length = 5000;
    let start = &msg[..half_length];
    let end = &msg[msg.len() - half_length..];
    format!(
        "{}\n\n... [{} characters truncated] ...\n\n{}",
        start,
        msg.len() - 10000,
        end
    )
}

/// Generic error with optional stderr/stdout
pub struct GenericError {
    pub message: String,
    pub stderr: Option<String>,
    pub stdout: Option<String>,
}

/// Tool error enum encompassing all error types
pub enum ToolError {
    Abort(AbortError),
    Shell(ShellError),
    Generic(GenericError),
    Other(String),
}

/// 对应 TS `getErrorParts`：根据具体错误类型抽取 `[message, stderr, stdout, ...]`
/// 等可用于日志展示的字段。
pub fn get_error_parts(error: &ToolError) -> Vec<String> {
    match error {
        ToolError::Shell(e) => get_shell_error_parts(e),
        ToolError::Generic(e) => get_generic_error_parts(e),
        ToolError::Abort(e) => vec![e.message.clone()],
        ToolError::Other(msg) => vec![msg.clone()],
    }
}

fn get_shell_error_parts(error: &ShellError) -> Vec<String> {
    vec![
        format!("Exit code {}", error.code),
        if error.interrupted {
            INTERRUPT_MESSAGE_FOR_TOOL_USE.to_string()
        } else {
            String::new()
        },
        error.stderr.clone(),
        error.stdout.clone(),
    ]
}

fn get_generic_error_parts(error: &GenericError) -> Vec<String> {
    let mut parts = vec![error.message.clone()];
    if let Some(ref stderr) = error.stderr {
        parts.push(stderr.clone());
    }
    if let Some(ref stdout) = error.stdout {
        parts.push(stdout.clone());
    }
    parts
}

/// Format a validation path into a readable string
/// e.g., ["todos", "0", "activeForm"] => "todos[0].activeForm"
pub fn format_validation_path(path: &[PathSegment]) -> String {
    if path.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    for (index, segment) in path.iter().enumerate() {
        match segment {
            PathSegment::Index(n) => {
                result.push_str(&format!("[{}]", n));
            }
            PathSegment::Key(key) => {
                if index == 0 {
                    result.push_str(key);
                } else {
                    result.push('.');
                    result.push_str(key);
                }
            }
        }
    }
    result
}

/// Path segment can be a numeric index or a string key
#[derive(Debug, Clone)]
pub enum PathSegment {
    Index(usize),
    Key(String),
}

/// Validation issue
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub path: Vec<PathSegment>,
    pub keys: Vec<String>,
    pub expected: Option<String>,
}

/// Format Zod-style validation errors into a human-readable error message
pub fn format_zod_validation_error(tool_name: &str, issues: &[ValidationIssue]) -> String {
    let missing_params: Vec<String> = issues
        .iter()
        .filter(|err| err.code == "invalid_type" && err.message.contains("received undefined"))
        .map(|err| format_validation_path(&err.path))
        .collect();

    let unexpected_params: Vec<String> = issues
        .iter()
        .filter(|err| err.code == "unrecognized_keys")
        .flat_map(|err| err.keys.clone())
        .collect();

    let type_mismatch_params: Vec<(String, String, String)> = issues
        .iter()
        .filter(|err| err.code == "invalid_type" && !err.message.contains("received undefined"))
        .map(|err| {
            let expected = err.expected.clone().unwrap_or_else(|| "unknown".to_string());
            let received_re = Regex::new(r"received (\w+)").unwrap();
            let received = received_re
                .captures(&err.message)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            (format_validation_path(&err.path), expected, received)
        })
        .collect();

    let mut error_parts = Vec::new();

    for param in &missing_params {
        error_parts.push(format!("The required parameter `{}` is missing", param));
    }

    for param in &unexpected_params {
        error_parts.push(format!(
            "An unexpected parameter `{}` was provided",
            param
        ));
    }

    for (param, expected, received) in &type_mismatch_params {
        error_parts.push(format!(
            "The parameter `{}` type is expected as `{}` but provided as `{}`",
            param, expected, received
        ));
    }

    if error_parts.is_empty() {
        // Fall back to joining all issue messages
        issues
            .iter()
            .map(|i| i.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let issue_word = if error_parts.len() > 1 {
            "issues"
        } else {
            "issue"
        };
        format!(
            "{} failed due to the following {}:\n{}",
            tool_name,
            issue_word,
            error_parts.join("\n")
        )
    }
}
