//! # history — 消息历史管理
//!
//! 对应 TS `history.ts`，负责消息历史的维护、查询和序列化。

use serde::{Deserialize, Serialize};

use mossen_types::{ContentBlock, Message, Role};

// ---------------------------------------------------------------------------
// 消息历史
// ---------------------------------------------------------------------------

/// 消息历史管理器。
#[derive(Debug, Clone)]
pub struct MessageHistory {
    /// 消息列表。
    messages: Vec<Message>,
    /// 最大保留消息数。
    max_messages: Option<usize>,
}

impl MessageHistory {
    /// 创建空的消息历史。
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            max_messages: None,
        }
    }

    /// 创建带初始消息的历史。
    pub fn with_messages(messages: Vec<Message>) -> Self {
        Self {
            messages,
            max_messages: None,
        }
    }

    /// 设置最大保留消息数。
    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = Some(max);
        self
    }

    /// 追加消息。
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.enforce_limit();
    }

    /// 追加多条消息。
    pub fn extend(&mut self, messages: impl IntoIterator<Item = Message>) {
        self.messages.extend(messages);
        self.enforce_limit();
    }

    /// 获取所有消息。
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// 获取可变消息列表。
    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        &mut self.messages
    }

    /// 消息数量。
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 获取最后一条消息。
    pub fn last(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// 获取最后一条指定角色的消息。
    pub fn last_with_role(&self, role: Role) -> Option<&Message> {
        self.messages.iter().rev().find(|m| m.role == role)
    }

    /// 清空历史。
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// 替换所有消息。
    pub fn replace_all(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.enforce_limit();
    }

    /// 按 UUID 查找消息。
    pub fn find_by_uuid(&self, uuid: &str) -> Option<&Message> {
        self.messages
            .iter()
            .find(|m| m.uuid.as_deref() == Some(uuid))
    }

    /// 按 UUID 移除消息。
    pub fn remove_by_uuid(&mut self, uuid: &str) -> Option<Message> {
        if let Some(pos) = self
            .messages
            .iter()
            .position(|m| m.uuid.as_deref() == Some(uuid))
        {
            Some(self.messages.remove(pos))
        } else {
            None
        }
    }

    /// 获取助手消息数量。
    pub fn assistant_message_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .count()
    }

    /// 获取用户消息数量。
    pub fn user_message_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == Role::User)
            .count()
    }

    /// 获取包含工具调用的消息列表。
    pub fn tool_use_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolUse(_)))
            })
            .collect()
    }

    /// 获取所有工具调用 ID。
    pub fn all_tool_use_ids(&self) -> Vec<String> {
        self.messages
            .iter()
            .flat_map(|m| {
                m.content.iter().filter_map(|b| {
                    if let ContentBlock::ToolUse(tu) = b {
                        Some(tu.id.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// 强制执行消息数量限制。
    fn enforce_limit(&mut self) {
        if let Some(max) = self.max_messages {
            while self.messages.len() > max {
                self.messages.remove(0);
            }
        }
    }
}

impl Default for MessageHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 序列化支持
// ---------------------------------------------------------------------------

/// 消息历史快照（用于序列化到 JSON）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySnapshot {
    pub messages: Vec<Message>,
    pub message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl From<&MessageHistory> for HistorySnapshot {
    fn from(history: &MessageHistory) -> Self {
        Self {
            message_count: history.len(),
            messages: history.messages.clone(),
            session_id: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `history.ts` exports.
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// `history.ts` `getPastedTextRefNumLines` — count line separators
/// (`\r\n` | `\r` | `\n`) in the pasted blob.
pub fn get_pasted_text_ref_num_lines(text: &str) -> usize {
    let re = regex::Regex::new(r"\r\n|\r|\n").unwrap();
    re.find_iter(text).count()
}

/// `history.ts` `formatPastedTextRef`.
pub fn format_pasted_text_ref(id: u64, num_lines: usize) -> String {
    if num_lines == 0 {
        format!("[Pasted text #{}]", id)
    } else {
        format!("[Pasted text #{} +{} lines]", id, num_lines)
    }
}

/// `history.ts` `formatImageRef`.
pub fn format_image_ref(id: u64) -> String {
    format!("[Image #{}]", id)
}

/// Parsed paste/image reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRef {
    pub id: u64,
    pub matched: String,
    pub index: usize,
}

/// `history.ts` `parseReferences`.
pub fn parse_references(input: &str) -> Vec<ParsedRef> {
    let re = regex::Regex::new(
        r"\[(Pasted text|Image|\.\.\.Truncated text) #(\d+)(?: \+\d+ lines)?(\.)*\]",
    )
    .unwrap();
    let mut out = Vec::new();
    for caps in re.captures_iter(input) {
        let id: u64 = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        if id == 0 {
            continue;
        }
        let m = caps.get(0).unwrap();
        out.push(ParsedRef {
            id,
            matched: m.as_str().to_string(),
            index: m.start(),
        });
    }
    out
}

/// Inline pasted-content variant for `expandPastedTextRefs`.
#[derive(Debug, Clone)]
pub struct PastedContent {
    pub id: u64,
    pub content_type: String, // "text" | "image"
    pub content: String,
}

/// `history.ts` `expandPastedTextRefs` — splice paste content into the
/// input string, leaving image references untouched.
pub fn expand_pasted_text_refs(
    input: &str,
    pasted_contents: &HashMap<u64, PastedContent>,
) -> String {
    let refs = parse_references(input);
    let mut expanded = input.to_string();
    for r in refs.iter().rev() {
        let Some(content) = pasted_contents.get(&r.id) else {
            continue;
        };
        if content.content_type != "text" {
            continue;
        }
        let end = r.index + r.matched.len();
        if end <= expanded.len() {
            expanded.replace_range(r.index..end, &content.content);
        }
    }
    expanded
}

/// `history.ts` `HistoryEntry` — minimal mirror for in-process history.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryEntry {
    pub display: String,
    #[serde(default)]
    pub pasted_contents: HashMap<u64, PastedContentSerial>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PastedContentSerial {
    pub id: u64,
    #[serde(rename = "type")]
    pub content_type: String,
    pub content: String,
}

/// `history.ts` `TimestampedHistoryEntry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedHistoryEntry {
    pub display: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LogEntry {
    pub display: String,
    pub timestamp: i64,
    pub project: String,
    pub session_id: Option<String>,
    pub pasted_contents: HashMap<u64, PastedContentSerial>,
}

static PENDING_ENTRIES: Lazy<Mutex<Vec<LogEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));
static LAST_ADDED: Lazy<Mutex<Option<LogEntry>>> = Lazy::new(|| Mutex::new(None));
static SKIPPED_TIMESTAMPS: Lazy<Mutex<HashSet<i64>>> = Lazy::new(|| Mutex::new(HashSet::new()));

const MAX_HISTORY_ITEMS: usize = 100;

fn project_root() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn session_id() -> String {
    std::env::var("MOSSEN_SESSION_ID").unwrap_or_default()
}

/// `history.ts` `addToHistory`.
pub fn add_to_history(command: HistoryEntry) {
    if matches!(
        std::env::var("MOSSEN_CODE_SKIP_PROMPT_HISTORY").as_deref(),
        Ok("1" | "true" | "TRUE")
    ) {
        return;
    }
    let entry = LogEntry {
        display: command.display,
        timestamp: chrono::Utc::now().timestamp_millis(),
        project: project_root(),
        session_id: Some(session_id()),
        pasted_contents: command.pasted_contents,
    };
    let mut last = LAST_ADDED.lock().unwrap();
    *last = Some(entry.clone());
    PENDING_ENTRIES.lock().unwrap().push(entry);
}

/// `history.ts` `addToHistory` string overload convenience.
pub fn add_to_history_str(display: &str) {
    add_to_history(HistoryEntry {
        display: display.to_string(),
        pasted_contents: HashMap::new(),
    });
}

/// `history.ts` `clearPendingHistoryEntries`.
pub fn clear_pending_history_entries() {
    PENDING_ENTRIES.lock().unwrap().clear();
    *LAST_ADDED.lock().unwrap() = None;
    SKIPPED_TIMESTAMPS.lock().unwrap().clear();
}

/// `history.ts` `removeLastFromHistory`.
pub fn remove_last_from_history() {
    let entry = {
        let mut last = LAST_ADDED.lock().unwrap();
        last.take()
    };
    let Some(entry) = entry else { return };
    let mut pending = PENDING_ENTRIES.lock().unwrap();
    if let Some(pos) = pending.iter().rposition(|e| e.timestamp == entry.timestamp) {
        pending.remove(pos);
    } else {
        SKIPPED_TIMESTAMPS.lock().unwrap().insert(entry.timestamp);
    }
}

/// `history.ts` `makeHistoryReader` — returns pending entries (newest-first)
/// as a vector. Disk-bound history is left to the persistence backend.
pub fn make_history_reader() -> Vec<HistoryEntry> {
    let pending = PENDING_ENTRIES.lock().unwrap();
    pending
        .iter()
        .rev()
        .map(|e| HistoryEntry {
            display: e.display.clone(),
            pasted_contents: e.pasted_contents.clone(),
        })
        .collect()
}

/// `history.ts` `getTimestampedHistory` — current-project, deduped by display.
pub fn get_timestamped_history() -> Vec<TimestampedHistoryEntry> {
    let current_project = project_root();
    let mut seen: HashSet<String> = HashSet::new();
    let pending = PENDING_ENTRIES.lock().unwrap();
    let mut out = Vec::new();
    for e in pending.iter().rev() {
        if e.project != current_project {
            continue;
        }
        if !seen.insert(e.display.clone()) {
            continue;
        }
        out.push(TimestampedHistoryEntry {
            display: e.display.clone(),
            timestamp: e.timestamp,
        });
        if out.len() >= MAX_HISTORY_ITEMS {
            break;
        }
    }
    out
}

/// `history.ts` `getHistory` — current-session entries first, then others.
pub fn get_history() -> Vec<HistoryEntry> {
    let current_project = project_root();
    let current_session = session_id();
    let pending = PENDING_ENTRIES.lock().unwrap();
    let mut current: Vec<HistoryEntry> = Vec::new();
    let mut others: Vec<HistoryEntry> = Vec::new();
    for e in pending.iter().rev() {
        if e.project != current_project {
            continue;
        }
        let h = HistoryEntry {
            display: e.display.clone(),
            pasted_contents: e.pasted_contents.clone(),
        };
        if e.session_id.as_deref() == Some(&current_session) {
            current.push(h);
        } else {
            others.push(h);
        }
        if current.len() + others.len() >= MAX_HISTORY_ITEMS {
            break;
        }
    }
    current.extend(others);
    current.truncate(MAX_HISTORY_ITEMS);
    current
}
