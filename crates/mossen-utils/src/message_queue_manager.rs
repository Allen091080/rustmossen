use serde::Serialize;
use std::collections::HashSet;

/// Priority levels for queued commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QueuePriority {
    Now,
    #[default]
    Next,
    Later,
}

impl QueuePriority {
    fn order(&self) -> u8 {
        match self {
            QueuePriority::Now => 0,
            QueuePriority::Next => 1,
            QueuePriority::Later => 2,
        }
    }
}

/// Prompt input mode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PromptInputMode {
    Default,
    TaskNotification,
    Custom(String),
}

/// Editable prompt input modes (everything except TaskNotification).
pub fn is_prompt_input_mode_editable(mode: &PromptInputMode) -> bool {
    !matches!(mode, PromptInputMode::TaskNotification)
}

/// Origin of a queued command.
#[derive(Debug, Clone)]
pub struct CommandOrigin {
    pub kind: String,
}

/// Pasted content (images, etc.).
#[derive(Debug, Clone)]
pub struct PastedContent {
    pub id: u64,
    pub content_type: String,
    pub content: String,
    pub media_type: String,
    pub filename: String,
}

/// A command queued for processing.
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    pub value: CommandValue,
    pub mode: PromptInputMode,
    pub priority: QueuePriority,
    pub is_meta: bool,
    pub agent_id: Option<String>,
    pub skip_slash_commands: bool,
    pub origin: Option<CommandOrigin>,
    pub pasted_contents: Option<Vec<PastedContent>>,
}

/// The value of a queued command (either a string or structured content blocks).
#[derive(Debug, Clone)]
pub enum CommandValue {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

impl CommandValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            CommandValue::Text(s) => Some(s),
            CommandValue::Blocks(_) => None,
        }
    }

    pub fn extract_text(&self) -> String {
        match self {
            CommandValue::Text(s) => s.clone(),
            CommandValue::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// Queue operation types for logging.
#[derive(Debug, Clone, Copy)]
pub enum QueueOperation {
    Enqueue,
    Dequeue,
    Remove,
    PopAll,
}

/// Queue operation message for recording.
#[derive(Debug, Clone, Serialize)]
pub struct QueueOperationMessage {
    pub operation_type: String,
    pub operation: String,
    pub timestamp: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Signal for notifying subscribers of queue changes.
type Subscriber = Box<dyn Fn() + Send + Sync>;

/// The unified command queue manager.
pub struct MessageQueueManager {
    command_queue: Vec<QueuedCommand>,
    snapshot: Vec<QueuedCommand>,
    subscribers: Vec<Subscriber>,
}

impl MessageQueueManager {
    pub fn new() -> Self {
        Self {
            command_queue: Vec::new(),
            snapshot: Vec::new(),
            subscribers: Vec::new(),
        }
    }

    fn notify_subscribers(&mut self) {
        self.snapshot = self.command_queue.clone();
        for subscriber in &self.subscribers {
            subscriber();
        }
    }

    /// Subscribe to command queue changes.
    pub fn subscribe(&mut self, callback: Subscriber) {
        self.subscribers.push(callback);
    }

    /// Get current snapshot of the command queue.
    pub fn get_command_queue_snapshot(&self) -> &[QueuedCommand] {
        &self.snapshot
    }

    /// Get a copy of the current queue.
    pub fn get_command_queue(&self) -> Vec<QueuedCommand> {
        self.command_queue.clone()
    }

    /// Get the current queue length without copying.
    pub fn get_command_queue_length(&self) -> usize {
        self.command_queue.len()
    }

    /// Get the number of commands still actionable by the main thread.
    pub fn get_main_thread_command_queue_length(&self) -> usize {
        self.command_queue
            .iter()
            .filter(|cmd| cmd.agent_id.is_none())
            .count()
    }

    /// Check if there are commands in the queue.
    pub fn has_commands_in_queue(&self) -> bool {
        !self.command_queue.is_empty()
    }

    /// Trigger a re-check by notifying subscribers.
    pub fn recheck_command_queue(&mut self) {
        if !self.command_queue.is_empty() {
            self.notify_subscribers();
        }
    }

    /// Add a command to the queue.
    pub fn enqueue(&mut self, mut command: QueuedCommand) {
        if command.priority == QueuePriority::default() {
            command.priority = QueuePriority::Next;
        }
        self.command_queue.push(command);
        self.notify_subscribers();
    }

    /// Add a task notification to the queue (defaults priority to 'later').
    pub fn enqueue_pending_notification(&mut self, mut command: QueuedCommand) {
        command.priority = QueuePriority::Later;
        self.command_queue.push(command);
        self.notify_subscribers();
    }

    /// Remove and return the highest-priority command, or None if empty.
    pub fn dequeue(
        &mut self,
        filter: Option<&dyn Fn(&QueuedCommand) -> bool>,
    ) -> Option<QueuedCommand> {
        if self.command_queue.is_empty() {
            return None;
        }

        let mut best_idx: Option<usize> = None;
        let mut best_priority = u8::MAX;

        for (i, cmd) in self.command_queue.iter().enumerate() {
            if let Some(f) = filter {
                if !f(cmd) {
                    continue;
                }
            }
            let priority = cmd.priority.order();
            if priority < best_priority {
                best_idx = Some(i);
                best_priority = priority;
            }
        }

        best_idx.map(|idx| {
            let dequeued = self.command_queue.remove(idx);
            self.notify_subscribers();
            dequeued
        })
    }

    /// Remove and return all commands from the queue.
    pub fn dequeue_all(&mut self) -> Vec<QueuedCommand> {
        if self.command_queue.is_empty() {
            return Vec::new();
        }
        let commands: Vec<QueuedCommand> = self.command_queue.drain(..).collect();
        self.notify_subscribers();
        commands
    }

    /// Return the highest-priority command without removing it.
    pub fn peek(&self, filter: Option<&dyn Fn(&QueuedCommand) -> bool>) -> Option<&QueuedCommand> {
        if self.command_queue.is_empty() {
            return None;
        }
        let mut best_idx: Option<usize> = None;
        let mut best_priority = u8::MAX;

        for (i, cmd) in self.command_queue.iter().enumerate() {
            if let Some(f) = filter {
                if !f(cmd) {
                    continue;
                }
            }
            let priority = cmd.priority.order();
            if priority < best_priority {
                best_idx = Some(i);
                best_priority = priority;
            }
        }

        best_idx.map(|idx| &self.command_queue[idx])
    }

    /// Remove and return all commands matching a predicate.
    pub fn dequeue_all_matching(
        &mut self,
        predicate: impl Fn(&QueuedCommand) -> bool,
    ) -> Vec<QueuedCommand> {
        let mut matched = Vec::new();
        let mut remaining = Vec::new();

        for cmd in self.command_queue.drain(..) {
            if predicate(&cmd) {
                matched.push(cmd);
            } else {
                remaining.push(cmd);
            }
        }

        if matched.is_empty() {
            self.command_queue = remaining;
            return Vec::new();
        }

        self.command_queue = remaining;
        self.notify_subscribers();
        matched
    }

    /// Remove specific commands from the queue by index.
    pub fn remove(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }

        let indices_set: HashSet<usize> = indices.iter().copied().collect();
        let before = self.command_queue.len();
        let mut new_queue = Vec::new();
        for (i, cmd) in self.command_queue.drain(..).enumerate() {
            if !indices_set.contains(&i) {
                new_queue.push(cmd);
            }
        }
        self.command_queue = new_queue;

        if self.command_queue.len() != before {
            self.notify_subscribers();
        }
    }

    /// Remove commands matching a predicate. Returns the removed commands.
    pub fn remove_by_filter(
        &mut self,
        predicate: impl Fn(&QueuedCommand) -> bool,
    ) -> Vec<QueuedCommand> {
        let mut removed = Vec::new();
        let mut remaining = Vec::new();

        for cmd in self.command_queue.drain(..) {
            if predicate(&cmd) {
                removed.push(cmd);
            } else {
                remaining.push(cmd);
            }
        }

        self.command_queue = remaining;

        if !removed.is_empty() {
            self.notify_subscribers();
        }

        removed
    }

    /// Clear all commands from the queue.
    pub fn clear_command_queue(&mut self) {
        if self.command_queue.is_empty() {
            return;
        }
        self.command_queue.clear();
        self.notify_subscribers();
    }

    /// Clear all commands and reset snapshot.
    pub fn reset_command_queue(&mut self) {
        self.command_queue.clear();
        self.snapshot = Vec::new();
    }

    /// Get commands at or above a given priority level without removing them.
    pub fn get_commands_by_max_priority(&self, max_priority: QueuePriority) -> Vec<&QueuedCommand> {
        let threshold = max_priority.order();
        self.command_queue
            .iter()
            .filter(|cmd| cmd.priority.order() <= threshold)
            .collect()
    }

    /// Returns true if the command is a slash command.
    pub fn is_slash_command(cmd: &QueuedCommand) -> bool {
        if cmd.skip_slash_commands {
            return false;
        }
        if let CommandValue::Text(ref text) = cmd.value {
            text.trim().starts_with('/')
        } else {
            false
        }
    }
}

impl Default for MessageQueueManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether this queued command can be pulled into the input buffer.
pub fn is_queued_command_editable(cmd: &QueuedCommand) -> bool {
    is_prompt_input_mode_editable(&cmd.mode) && !cmd.is_meta
}

/// Whether this queued command should render in the queue preview.
pub fn is_queued_command_visible(cmd: &QueuedCommand) -> bool {
    if let Some(ref origin) = cmd.origin {
        if origin.kind == "channel" {
            return true;
        }
    }
    is_queued_command_editable(cmd)
}

/// Result of popping all editable commands.
#[derive(Debug, Clone)]
pub struct PopAllEditableResult {
    pub text: String,
    pub cursor_offset: usize,
    pub images: Vec<PastedContent>,
}

/// Pop all editable commands and combine them with current input for editing.
pub fn pop_all_editable(
    queue: &mut MessageQueueManager,
    current_input: &str,
    current_cursor_offset: usize,
) -> Option<PopAllEditableResult> {
    if !queue.has_commands_in_queue() {
        return None;
    }

    let all_commands: Vec<QueuedCommand> = queue.command_queue.drain(..).collect();
    let mut editable = Vec::new();
    let mut non_editable = Vec::new();

    for cmd in all_commands {
        if is_queued_command_editable(&cmd) {
            editable.push(cmd);
        } else {
            non_editable.push(cmd);
        }
    }

    if editable.is_empty() {
        queue.command_queue = non_editable;
        return None;
    }

    let queued_texts: Vec<String> = editable
        .iter()
        .map(|cmd| cmd.value.extract_text())
        .collect();
    let new_input_parts: Vec<&str> = queued_texts
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(current_input))
        .filter(|s| !s.is_empty())
        .collect();
    let new_input = new_input_parts.join("\n");

    let cursor_offset = queued_texts.join("\n").len() + 1 + current_cursor_offset;

    // Extract images from queued commands
    let mut images = Vec::new();
    let mut next_image_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    for cmd in &editable {
        if let Some(ref pasted) = cmd.pasted_contents {
            for content in pasted {
                if content.content_type == "image" {
                    images.push(content.clone());
                }
            }
        }
        // Extract images from block values
        if let CommandValue::Blocks(ref blocks) = cmd.value {
            for block in blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("image") {
                    if let Some(source) = block.get("source") {
                        if source.get("type").and_then(|t| t.as_str()) == Some("base64") {
                            images.push(PastedContent {
                                id: next_image_id,
                                content_type: "image".to_string(),
                                content: source
                                    .get("data")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                media_type: source
                                    .get("media_type")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                filename: format!("image{}", images.len() + 1),
                            });
                            next_image_id += 1;
                        }
                    }
                }
            }
        }
    }

    // Replace queue contents with only the non-editable commands
    queue.command_queue = non_editable;
    queue.notify_subscribers();

    Some(PopAllEditableResult {
        text: new_input,
        cursor_offset,
        images,
    })
}

// =============================================================================
// Pending notification API — TS exports the `Signal` subscribe handles and a
// few helpers; Rust 端用一个 `Mutex<Vec<...>>` + 一个 [`Signal`] 提供同等语义。
// =============================================================================

use crate::signal::Signal;
use std::sync::Mutex as StdMutex;

/// 单条挂起通知。
#[derive(Debug, Clone)]
pub struct PendingNotification {
    pub id: String,
    pub content: String,
    pub created_at_ms: u128,
}

static PENDING_NOTIFICATIONS: once_cell::sync::Lazy<StdMutex<Vec<PendingNotification>>> =
    once_cell::sync::Lazy::new(|| StdMutex::new(Vec::new()));
static PENDING_NOTIFICATIONS_SIGNAL: once_cell::sync::Lazy<Signal> =
    once_cell::sync::Lazy::new(Signal::new);
static COMMAND_QUEUE_SIGNAL: once_cell::sync::Lazy<Signal> =
    once_cell::sync::Lazy::new(Signal::new);
static PENDING_HINT_SIGNAL: once_cell::sync::Lazy<Signal> = once_cell::sync::Lazy::new(Signal::new);

/// 对应 TS `subscribeToCommandQueue`：返回命令队列订阅入口。
pub fn subscribe_to_command_queue() -> &'static Signal {
    &COMMAND_QUEUE_SIGNAL
}

/// 对应 TS `subscribeToPendingNotifications`：返回通知队列订阅入口。
pub fn subscribe_to_pending_notifications() -> &'static Signal {
    &PENDING_NOTIFICATIONS_SIGNAL
}

/// 对应 TS `subscribeToPendingHint`：pending 提示订阅入口。
pub fn subscribe_to_pending_hint() -> &'static Signal {
    &PENDING_HINT_SIGNAL
}

/// 是否存在挂起通知。
pub fn has_pending_notifications() -> bool {
    !PENDING_NOTIFICATIONS.lock().unwrap().is_empty()
}

/// 挂起通知数量。
pub fn get_pending_notifications_count() -> usize {
    PENDING_NOTIFICATIONS.lock().unwrap().len()
}

/// 重新检查挂起通知，触发订阅。
pub fn recheck_pending_notifications() {
    PENDING_NOTIFICATIONS_SIGNAL.emit();
}

/// 重置（清空）挂起通知。对应 TS `resetPendingNotifications`。
pub fn reset_pending_notifications() {
    PENDING_NOTIFICATIONS.lock().unwrap().clear();
    PENDING_NOTIFICATIONS_SIGNAL.emit();
}

/// 清空挂起通知。对应 TS `clearPendingNotifications`。
pub fn clear_pending_notifications() {
    reset_pending_notifications();
}

/// 获取当前所有挂起通知（快照）。
pub fn get_pending_notifications_snapshot() -> Vec<PendingNotification> {
    PENDING_NOTIFICATIONS.lock().unwrap().clone()
}

/// 出队一条挂起通知，没有则返回 `None`。
pub fn dequeue_pending_notification() -> Option<PendingNotification> {
    let popped = {
        let mut guard = PENDING_NOTIFICATIONS.lock().unwrap();
        if guard.is_empty() {
            None
        } else {
            Some(guard.remove(0))
        }
    };
    if popped.is_some() {
        PENDING_NOTIFICATIONS_SIGNAL.emit();
    }
    popped
}

/// 类型别名：对应 TS `SetAppState`。Rust 端用 `dyn FnMut` 表示状态更新回调签名。
pub type SetAppState =
    Box<dyn FnMut(Box<dyn FnOnce(serde_json::Value) -> serde_json::Value>) + Send + Sync>;
