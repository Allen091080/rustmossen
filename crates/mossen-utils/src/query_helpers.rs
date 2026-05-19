use std::collections::{HashMap, HashSet};
use regex::Regex;

/// Check if the result should be considered successful based on the last message.
/// Returns true if:
/// - Last message is assistant with text/thinking content
/// - Last message is user with only tool_result blocks
/// - Last message is the user prompt but the API completed with end_turn
pub fn is_result_successful(
    message: Option<&Message>,
    stop_reason: Option<&str>,
) -> bool {
    let Some(msg) = message else {
        return false;
    };

    match &msg.message_type {
        MessageType::Assistant { content } => {
            if let Some(last_content) = content.last() {
                matches!(
                    last_content,
                    ContentBlock::Text { .. }
                        | ContentBlock::Thinking { .. }
                        | ContentBlock::RedactedThinking { .. }
                )
            } else {
                stop_reason == Some("end_turn")
            }
        }
        MessageType::User { content } => {
            if content
                .iter()
                .all(|block| matches!(block, ContentBlock::ToolResult { .. }))
                && !content.is_empty()
            {
                return true;
            }
            stop_reason == Some("end_turn")
        }
        _ => stop_reason == Some("end_turn"),
    }
}

/// Message types for the query helper system.
#[derive(Debug, Clone)]
pub enum MessageType {
    Assistant { content: Vec<ContentBlock> },
    User { content: Vec<ContentBlock> },
    Progress { data: ProgressData },
    System,
}

/// Content block within a message.
#[derive(Debug, Clone, serde::Serialize)]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String },
    RedactedThinking,
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageSource {
    pub source_type: String,
    pub data: String,
    pub media_type: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressData {
    pub progress_type: String,
    pub elapsed_time_seconds: Option<f64>,
    pub task_id: Option<String>,
}

/// A message in the conversation.
#[derive(Debug, Clone)]
pub struct Message {
    pub message_type: MessageType,
    pub uuid: String,
    pub timestamp: Option<String>,
    pub is_meta: bool,
    pub error: Option<String>,
}

/// SDK message output.
#[derive(Debug, Clone)]
pub struct SdkMessage {
    pub message_type: String,
    pub session_id: String,
    pub parent_tool_use_id: Option<String>,
    pub uuid: String,
    pub content: serde_json::Value,
}

/// Track last sent time for tool progress messages per tool use ID.
const MAX_TOOL_PROGRESS_TRACKING_ENTRIES: usize = 100;
const TOOL_PROGRESS_THROTTLE_MS: u64 = 30000;

/// Normalize a message to SDK format.
pub fn normalize_message(
    message: &Message,
    session_id: &str,
    tool_progress_last_sent: &mut HashMap<String, u64>,
) -> Vec<SdkMessage> {
    let mut results = Vec::new();

    match &message.message_type {
        MessageType::Assistant { content } => {
            if content.is_empty() {
                return results;
            }
            results.push(SdkMessage {
                message_type: "assistant".to_string(),
                session_id: session_id.to_string(),
                parent_tool_use_id: None,
                uuid: message.uuid.clone(),
                content: serde_json::to_value(content).unwrap_or_default(),
            });
        }
        MessageType::User { content } => {
            results.push(SdkMessage {
                message_type: "user".to_string(),
                session_id: session_id.to_string(),
                parent_tool_use_id: None,
                uuid: message.uuid.clone(),
                content: serde_json::to_value(content).unwrap_or_default(),
            });
        }
        MessageType::Progress { data } => {
            // Throttle progress messages
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let tracking_key = message.uuid.clone();
            let last_sent = tool_progress_last_sent.get(&tracking_key).copied().unwrap_or(0);
            let time_since_last = now - last_sent;

            if time_since_last >= TOOL_PROGRESS_THROTTLE_MS {
                // LRU eviction
                if tool_progress_last_sent.len() >= MAX_TOOL_PROGRESS_TRACKING_ENTRIES {
                    if let Some(first_key) = tool_progress_last_sent.keys().next().cloned() {
                        tool_progress_last_sent.remove(&first_key);
                    }
                }
                tool_progress_last_sent.insert(tracking_key, now);

                results.push(SdkMessage {
                    message_type: "tool_progress".to_string(),
                    session_id: session_id.to_string(),
                    parent_tool_use_id: None,
                    uuid: message.uuid.clone(),
                    content: serde_json::to_value(data).unwrap_or_default(),
                });
            }
        }
        MessageType::System => {}
    }

    results
}

/// File state entry in the cache.
#[derive(Debug, Clone)]
pub struct FileStateEntry {
    pub content: String,
    pub timestamp: u64,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// A size-limited file state cache.
#[derive(Debug, Clone)]
pub struct FileStateCache {
    entries: HashMap<String, FileStateEntry>,
    max_size: usize,
}

impl FileStateCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size,
        }
    }

    pub fn set(&mut self, path: String, entry: FileStateEntry) {
        if self.entries.len() >= self.max_size && !self.entries.contains_key(&path) {
            // Evict oldest entry
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.timestamp)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }
        self.entries.insert(path, entry);
    }

    pub fn get(&self, path: &str) -> Option<&FileStateEntry> {
        self.entries.get(path)
    }
}

const ASK_READ_FILE_STATE_CACHE_SIZE: usize = 10;
const FILE_READ_TOOL_NAME: &str = "Read";
const FILE_WRITE_TOOL_NAME: &str = "Write";
const FILE_EDIT_TOOL_NAME: &str = "Edit";
const BASH_TOOL_NAME: &str = "Bash";
const FILE_UNCHANGED_STUB: &str = "(file content unchanged)";

/// Create a function to extract read files from messages.
pub fn extract_read_files_from_messages(
    messages: &[Message],
    cwd: &str,
    max_size: usize,
) -> FileStateCache {
    let mut cache = FileStateCache::new(if max_size == 0 {
        ASK_READ_FILE_STATE_CACHE_SIZE
    } else {
        max_size
    });

    // First pass: find all FileReadTool/FileWriteTool/FileEditTool uses in assistant messages
    let mut file_read_tool_use_ids: HashMap<String, String> = HashMap::new();
    let mut file_write_tool_use_ids: HashMap<String, (String, String)> = HashMap::new();
    let mut file_edit_tool_use_ids: HashMap<String, String> = HashMap::new();

    for message in messages {
        if let MessageType::Assistant { content } = &message.message_type {
            for block in content {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    match name.as_str() {
                        n if n == FILE_READ_TOOL_NAME => {
                            if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str())
                            {
                                let offset = input.get("offset");
                                let limit = input.get("limit");
                                if offset.is_none() && limit.is_none() {
                                    let abs_path = expand_path(file_path, cwd);
                                    file_read_tool_use_ids.insert(id.clone(), abs_path);
                                }
                            }
                        }
                        n if n == FILE_WRITE_TOOL_NAME => {
                            if let (Some(file_path), Some(content_val)) = (
                                input.get("file_path").and_then(|v| v.as_str()),
                                input.get("content").and_then(|v| v.as_str()),
                            ) {
                                let abs_path = expand_path(file_path, cwd);
                                file_write_tool_use_ids
                                    .insert(id.clone(), (abs_path, content_val.to_string()));
                            }
                        }
                        n if n == FILE_EDIT_TOOL_NAME => {
                            if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str())
                            {
                                let abs_path = expand_path(file_path, cwd);
                                file_edit_tool_use_ids.insert(id.clone(), abs_path);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Second pass: find corresponding tool results and extract content
    for message in messages {
        if let MessageType::User { content } = &message.message_type {
            for block in content {
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content: result_content,
                    is_error,
                } = block
                {
                    // Handle Read tool results
                    if let Some(read_file_path) = file_read_tool_use_ids.get(tool_use_id) {
                        if !result_content.starts_with(FILE_UNCHANGED_STUB) {
                            // Remove system-reminder blocks
                            let processed = remove_system_reminder_blocks(result_content);
                            let file_content: String = processed
                                .lines()
                                .map(|line| strip_line_number_prefix(line))
                                .collect::<Vec<_>>()
                                .join("\n")
                                .trim()
                                .to_string();

                            if let Some(ref timestamp) = message.timestamp {
                                if let Ok(ts) = parse_timestamp_ms(timestamp) {
                                    cache.set(
                                        read_file_path.clone(),
                                        FileStateEntry {
                                            content: file_content,
                                            timestamp: ts,
                                            offset: None,
                                            limit: None,
                                        },
                                    );
                                }
                            }
                        }
                    }

                    // Handle Write tool results
                    if let Some((write_path, write_content)) =
                        file_write_tool_use_ids.get(tool_use_id)
                    {
                        if let Some(ref timestamp) = message.timestamp {
                            if let Ok(ts) = parse_timestamp_ms(timestamp) {
                                cache.set(
                                    write_path.clone(),
                                    FileStateEntry {
                                        content: write_content.clone(),
                                        timestamp: ts,
                                        offset: None,
                                        limit: None,
                                    },
                                );
                            }
                        }
                    }

                    // Handle Edit tool results
                    if let Some(edit_file_path) = file_edit_tool_use_ids.get(tool_use_id) {
                        if !is_error {
                            if let Ok(disk_content) = std::fs::read_to_string(edit_file_path) {
                                if let Ok(metadata) = std::fs::metadata(edit_file_path) {
                                    if let Ok(modified) = metadata.modified() {
                                        let ts = modified
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                            as u64;
                                        cache.set(
                                            edit_file_path.clone(),
                                            FileStateEntry {
                                                content: disk_content,
                                                timestamp: ts,
                                                offset: None,
                                                limit: None,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    cache
}

/// Extract the top-level CLI tools used in BashTool calls from message history.
pub fn extract_bash_tools_from_messages(messages: &[Message]) -> HashSet<String> {
    let mut tools = HashSet::new();
    for message in messages {
        if let MessageType::Assistant { content } = &message.message_type {
            for block in content {
                if let ContentBlock::ToolUse { name, input, .. } = block {
                    if name == BASH_TOOL_NAME {
                        if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
                            if let Some(cmd) = extract_cli_name(Some(command)) {
                                tools.insert(cmd);
                            }
                        }
                    }
                }
            }
        }
    }
    tools
}

const STRIPPED_COMMANDS: &[&str] = &["sudo"];

/// Extract the actual CLI name from a bash command string, skipping
/// env var assignments and prefixes in STRIPPED_COMMANDS.
pub fn extract_cli_name(command: Option<&str>) -> Option<String> {
    let command = command?;
    let tokens: Vec<&str> = command.trim().split_whitespace().collect();
    let env_var_re = Regex::new(r"^[A-Za-z_]\w*=").unwrap();

    for token in tokens {
        if env_var_re.is_match(token) {
            continue;
        }
        if STRIPPED_COMMANDS.contains(&token) {
            continue;
        }
        return Some(token.to_string());
    }
    None
}

/// Expand a file path to absolute, resolving relative paths against cwd.
fn expand_path(path: &str, cwd: &str) -> String {
    if std::path::Path::new(path).is_absolute() {
        path.to_string()
    } else if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            format!("{}{}", home.display(), &path[1..])
        } else {
            path.to_string()
        }
    } else {
        std::path::Path::new(cwd)
            .join(path)
            .to_string_lossy()
            .to_string()
    }
}

/// Strip line number prefix from a single line (e.g., "   123→content" -> "content").
fn strip_line_number_prefix(line: &str) -> &str {
    let re = Regex::new(r"^\s*\d+[\u{2192}\t](.*)$").unwrap();
    if let Some(caps) = re.captures(line) {
        caps.get(1).map_or(line, |m| m.as_str())
    } else {
        line
    }
}

/// Remove <system-reminder>...</system-reminder> blocks from content.
fn remove_system_reminder_blocks(content: &str) -> String {
    let re = Regex::new(r"<system-reminder>[\s\S]*?</system-reminder>").unwrap();
    re.replace_all(content, "").to_string()
}

/// Parse a timestamp string to milliseconds since epoch.
fn parse_timestamp_ms(timestamp: &str) -> Result<u64, ()> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt.timestamp_millis() as u64)
    } else {
        Err(())
    }
}

/// 对应 TS `PermissionPromptTool`：权限交互专用的 Tool 标记类型。
#[derive(Debug, Clone)]
pub struct PermissionPromptTool {
    pub name: String,
    pub description: String,
}
