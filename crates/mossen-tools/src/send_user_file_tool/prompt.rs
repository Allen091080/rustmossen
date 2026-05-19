//! SendUserFileTool prompt.
pub const SEND_USER_FILE_TOOL_NAME: &str = "SendUserFile";
pub const DESCRIPTION: &str = "Send files to the user";
pub const SEND_USER_FILE_TOOL_PROMPT: &str = r#"Deliver one or more files the user should receive.

`attachments` is required and takes absolute or cwd-relative file paths.
`message` is optional context shown alongside the delivered files when the user needs a short explanation.

Use this when the user needs the actual file artifact, not just a textual summary."#;
