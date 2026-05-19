#!/usr/bin/env python3
"""Generate hooks part 2 - can_use_tool through file_suggestions."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"
files = []

files.append(("can_use_tool.rs", '''//! Can-use-tool permission check (useCanUseTool.tsx).
//!
//! Determines whether the current user/session can use a specific tool,
//! checking feature flags, permission mode, and tool allowlists.

/// Result of a can-use-tool check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanUseToolResult {
    Allowed,
    Denied { reason: String },
    RequiresApproval,
    FeatureDisabled,
}

/// State for tool usage permission checking.
#[derive(Debug, Clone)]
pub struct CanUseToolState {
    pub tool_name: String,
    pub result: CanUseToolResult,
    pub checked: bool,
}

impl CanUseToolState {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            result: CanUseToolResult::Allowed,
            checked: false,
        }
    }

    /// Check if a tool can be used given the current permission context.
    pub fn check(
        &mut self,
        permission_mode: &str,
        is_auto_mode_available: bool,
        tool_allowlist: &[String],
        feature_transcript_classifier: bool,
    ) {
        self.checked = true;

        // Feature flag check
        if !feature_transcript_classifier && self.tool_name == "auto_approve" {
            self.result = CanUseToolResult::FeatureDisabled;
            return;
        }

        // Allowlist check
        if !tool_allowlist.is_empty() && !tool_allowlist.contains(&self.tool_name) {
            self.result = CanUseToolResult::Denied {
                reason: format!("Tool '{}' not in allowlist", self.tool_name),
            };
            return;
        }

        // Permission mode check
        match permission_mode {
            "auto" if is_auto_mode_available => {
                self.result = CanUseToolResult::Allowed;
            }
            "plan" => {
                self.result = CanUseToolResult::RequiresApproval;
            }
            _ => {
                self.result = CanUseToolResult::Allowed;
            }
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.result, CanUseToolResult::Allowed)
    }
}
'''))

files.append(("cancel_request.rs", '''//! Cancel request hook (useCancelRequest.ts).
//!
//! Manages the state for cancelling an in-flight API request.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cancellation token that can be shared across async boundaries.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// State for the cancel request hook.
#[derive(Debug, Clone)]
pub struct CancelRequestState {
    pub token: CancellationToken,
    pub is_cancelling: bool,
    pub cancel_count: u32,
}

impl CancelRequestState {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            is_cancelling: false,
            cancel_count: 0,
        }
    }

    /// Initiate cancellation of the current request.
    pub fn cancel(&mut self) {
        self.token.cancel();
        self.is_cancelling = true;
        self.cancel_count += 1;
    }

    /// Reset for a new request.
    pub fn reset(&mut self) {
        self.token = CancellationToken::new();
        self.is_cancelling = false;
    }

    /// Check if currently in cancelling state.
    pub fn is_pending_cancel(&self) -> bool {
        self.is_cancelling
    }
}

impl Default for CancelRequestState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("chrome_extension_notification.rs", '''//! Chrome extension notification (useChromeExtensionNotification.tsx).
//!
//! Shows notifications about Chrome extension status: not installed,
//! integration unavailable, or default-enabled.

/// Chrome extension notification level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeNotificationLevel {
    Info,
    Warning,
    Error,
}

/// Chrome extension notification surface.
#[derive(Debug, Clone)]
pub struct ChromeExtensionNotice {
    pub key: String,
    pub level: ChromeNotificationLevel,
    pub message: String,
}

/// State for chrome extension notification.
#[derive(Debug, Clone)]
pub struct ChromeExtensionNotificationState {
    pub notice: Option<ChromeExtensionNotice>,
    pub checked: bool,
    pub extension_installed: bool,
}

impl ChromeExtensionNotificationState {
    pub fn new() -> Self {
        Self {
            notice: None,
            checked: false,
            extension_installed: false,
        }
    }

    /// Determine the notification to show based on chrome integration state.
    pub fn check(
        &mut self,
        chrome_flag: Option<bool>,
        can_use_chrome: bool,
        is_custom_backend: bool,
        has_configured_urls: bool,
        extension_installed: bool,
        is_running_on_homespace: bool,
    ) {
        self.checked = true;
        self.extension_installed = extension_installed;

        // Check if chrome integration should be enabled
        let should_enable = chrome_flag.unwrap_or(true);
        if !should_enable {
            self.notice = None;
            return;
        }

        if !can_use_chrome {
            let message = if is_custom_backend && !has_configured_urls {
                "Chrome integration is not configured. Set MOSSEN_CODE_PLATFORM_BASE_URL or the MOSSEN_CODE_CHROME_* URLs first.".to_string()
            } else {
                "Chrome integration is not enabled for the current provider or backend configuration.".to_string()
            };
            self.notice = Some(ChromeExtensionNotice {
                key: "chrome-integration-unavailable".to_string(),
                level: ChromeNotificationLevel::Error,
                message,
            });
            return;
        }

        if !extension_installed && !is_running_on_homespace {
            self.notice = Some(ChromeExtensionNotice {
                key: "chrome-extension-not-detected".to_string(),
                level: ChromeNotificationLevel::Warning,
                message: "Chrome extension not detected".to_string(),
            });
            return;
        }

        if chrome_flag.is_none() {
            self.notice = Some(ChromeExtensionNotice {
                key: "mossen-in-chrome-default-enabled".to_string(),
                level: ChromeNotificationLevel::Info,
                message: "Chrome integration enabled · /chrome".to_string(),
            });
            return;
        }

        self.notice = None;
    }
}

impl Default for ChromeExtensionNotificationState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("clipboard_image_hint.rs", '''//! Clipboard image hint hook (useClipboardImageHint.ts).
//!
//! Shows a notification when the terminal regains focus and the
//! clipboard contains an image.

use std::time::{Duration, Instant};

const FOCUS_CHECK_DEBOUNCE_MS: u64 = 1000;
const HINT_COOLDOWN_MS: u64 = 30000;

/// State for clipboard image hint notification.
#[derive(Debug, Clone)]
pub struct ClipboardImageHintState {
    pub enabled: bool,
    pub last_focused: bool,
    pub last_hint_time: Option<Instant>,
    pub pending_check: bool,
    pub check_scheduled_at: Option<Instant>,
}

impl ClipboardImageHintState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            last_focused: false,
            last_hint_time: None,
            pending_check: false,
            check_scheduled_at: None,
        }
    }

    /// Update focus state. Returns true if a clipboard check should be triggered.
    pub fn on_focus_change(&mut self, is_focused: bool) -> bool {
        let was_focused = self.last_focused;
        self.last_focused = is_focused;

        if !self.enabled || !is_focused || was_focused {
            return false;
        }

        // Check cooldown
        if let Some(last_hint) = self.last_hint_time {
            if last_hint.elapsed() < Duration::from_millis(HINT_COOLDOWN_MS) {
                return false;
            }
        }

        // Schedule a debounced check
        self.pending_check = true;
        self.check_scheduled_at = Some(Instant::now());
        true
    }

    /// Check if the debounce period has passed.
    pub fn should_check_clipboard(&self) -> bool {
        if !self.pending_check {
            return false;
        }
        if let Some(scheduled) = self.check_scheduled_at {
            scheduled.elapsed() >= Duration::from_millis(FOCUS_CHECK_DEBOUNCE_MS)
        } else {
            false
        }
    }

    /// Mark that the hint was shown.
    pub fn mark_hint_shown(&mut self) {
        self.last_hint_time = Some(Instant::now());
        self.pending_check = false;
        self.check_scheduled_at = None;
    }

    /// Cancel pending check.
    pub fn cancel_check(&mut self) {
        self.pending_check = false;
        self.check_scheduled_at = None;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for ClipboardImageHintState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("command_keybindings.rs", '''//! Command keybindings hook (useCommandKeybindings.tsx).
//!
//! Registers keybinding handlers for command bindings within the
//! keybinding system context.

use std::collections::HashMap;

/// A command keybinding registration.
#[derive(Debug, Clone)]
pub struct CommandKeybinding {
    pub command: String,
    pub key_sequence: String,
    pub context: String,
    pub description: String,
    pub enabled: bool,
}

/// State for managing command keybindings.
#[derive(Debug, Clone)]
pub struct CommandKeybindingsState {
    pub bindings: Vec<CommandKeybinding>,
    pub active_bindings: HashMap<String, String>,
    pub context_active: bool,
}

impl CommandKeybindingsState {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            active_bindings: HashMap::new(),
            context_active: false,
        }
    }

    /// Register a new command keybinding.
    pub fn register(&mut self, binding: CommandKeybinding) {
        if binding.enabled {
            self.active_bindings.insert(
                binding.key_sequence.clone(),
                binding.command.clone(),
            );
        }
        self.bindings.push(binding);
    }

    /// Unregister a command keybinding.
    pub fn unregister(&mut self, command: &str) {
        self.bindings.retain(|b| b.command != command);
        self.active_bindings.retain(|_, v| v != command);
    }

    /// Look up a command for a key sequence.
    pub fn lookup(&self, key_sequence: &str) -> Option<&str> {
        if !self.context_active {
            return None;
        }
        self.active_bindings.get(key_sequence).map(|s| s.as_str())
    }

    /// Set whether the keybinding context is active.
    pub fn set_context_active(&mut self, active: bool) {
        self.context_active = active;
    }

    /// Get all registered bindings for display.
    pub fn all_bindings(&self) -> &[CommandKeybinding] {
        &self.bindings
    }
}

impl Default for CommandKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("command_queue.rs", '''//! Command queue hook (useCommandQueue.ts).
//!
//! Subscribes to the unified command queue store and returns
//! a frozen snapshot that changes only on mutation.

/// Priority level for queued commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandPriority {
    Later,
    Next,
    Now,
}

/// A queued command waiting to be executed.
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    pub id: String,
    pub text: String,
    pub priority: CommandPriority,
    pub source: CommandSource,
    pub timestamp: u64,
}

/// Source of a queued command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource {
    User,
    Task,
    Plugin,
    System,
}

/// State for the command queue.
#[derive(Debug, Clone)]
pub struct CommandQueueState {
    pub commands: Vec<QueuedCommand>,
    pub version: u64,
}

impl CommandQueueState {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            version: 0,
        }
    }

    /// Enqueue a command with the given priority.
    pub fn enqueue(&mut self, command: QueuedCommand) {
        // Insert maintaining priority order
        let pos = self.commands.partition_point(|c| c.priority >= command.priority);
        self.commands.insert(pos, command);
        self.version += 1;
    }

    /// Dequeue the highest priority command.
    pub fn dequeue(&mut self) -> Option<QueuedCommand> {
        if self.commands.is_empty() {
            return None;
        }
        self.version += 1;
        Some(self.commands.remove(0))
    }

    /// Peek at the next command without removing it.
    pub fn peek(&self) -> Option<&QueuedCommand> {
        self.commands.first()
    }

    /// Get current queue length.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Clear all commands.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.version += 1;
    }

    /// Get a snapshot of the current queue.
    pub fn snapshot(&self) -> &[QueuedCommand] {
        &self.commands
    }
}

impl Default for CommandQueueState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("copy_on_select.rs", '''//! Copy on select hook (useCopyOnSelect.ts).
//!
//! Monitors selection changes and copies selected text to clipboard
//! when the copyOnSelect config option is enabled.

/// State for copy-on-select behavior.
#[derive(Debug, Clone)]
pub struct CopyOnSelectState {
    pub enabled: bool,
    pub last_selection: Option<String>,
    pub copy_count: u64,
}

impl CopyOnSelectState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_selection: None,
            copy_count: 0,
        }
    }

    /// Handle a selection change. Returns the text to copy if applicable.
    pub fn on_selection_change(&mut self, selected_text: Option<&str>) -> Option<&str> {
        if !self.enabled {
            return None;
        }

        match selected_text {
            Some(text) if !text.is_empty() => {
                // Only copy if selection changed
                let should_copy = self.last_selection.as_deref() != Some(text);
                self.last_selection = Some(text.to_string());
                if should_copy {
                    self.copy_count += 1;
                    self.last_selection.as_deref()
                } else {
                    None
                }
            }
            _ => {
                self.last_selection = None;
                None
            }
        }
    }

    /// Set whether copy-on-select is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for CopyOnSelectState {
    fn default() -> Self {
        Self::new(false)
    }
}
'''))

files.append(("deferred_hook_messages.rs", '''//! Deferred hook messages (useDeferredHookMessages.ts).
//!
//! Collects messages produced by hooks during render and flushes them
//! to the message list in a deferred effect (to avoid setState during render).

/// A hook-produced message to be added to the message list.
#[derive(Debug, Clone)]
pub struct HookMessage {
    pub id: String,
    pub content: String,
    pub source: String,
    pub timestamp: u64,
}

/// State for deferred hook messages.
#[derive(Debug, Clone)]
pub struct DeferredHookMessagesState {
    pending: Vec<HookMessage>,
    flushed: Vec<HookMessage>,
}

impl DeferredHookMessagesState {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            flushed: Vec::new(),
        }
    }

    /// Queue a message to be flushed after render.
    pub fn push(&mut self, message: HookMessage) {
        self.pending.push(message);
    }

    /// Flush all pending messages. Returns the messages that were flushed.
    pub fn flush(&mut self) -> Vec<HookMessage> {
        let messages = std::mem::take(&mut self.pending);
        self.flushed.extend(messages.clone());
        messages
    }

    /// Check if there are pending messages.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get the count of flushed messages.
    pub fn flushed_count(&self) -> usize {
        self.flushed.len()
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.flushed.clear();
    }
}

impl Default for DeferredHookMessagesState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("diff_data.rs", '''//! Diff data hook (useDiffData.ts).
//!
//! Fetches and caches git diff data for display in the UI.

/// A diff hunk from git.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
    pub header: String,
}

/// A single line in a diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_type: DiffLineType,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    Context,
    Added,
    Removed,
}

/// State for diff data loading and caching.
#[derive(Debug, Clone)]
pub struct DiffDataState {
    pub file_path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub raw_diff: Option<String>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}

impl DiffDataState {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            hunks: Vec::new(),
            is_loading: false,
            error: None,
            raw_diff: None,
            old_content: None,
            new_content: None,
        }
    }

    /// Start loading diff data.
    pub fn start_loading(&mut self) {
        self.is_loading = true;
        self.error = None;
    }

    /// Set the loaded diff data.
    pub fn set_data(&mut self, hunks: Vec<DiffHunk>, raw_diff: String) {
        self.hunks = hunks;
        self.raw_diff = Some(raw_diff);
        self.is_loading = false;
    }

    /// Set file contents for inline diff display.
    pub fn set_contents(&mut self, old_content: String, new_content: String) {
        self.old_content = Some(old_content);
        self.new_content = Some(new_content);
    }

    /// Mark as errored.
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.is_loading = false;
    }

    /// Check if data is available.
    pub fn has_data(&self) -> bool {
        !self.hunks.is_empty()
    }

    /// Get total lines changed.
    pub fn total_changes(&self) -> (u32, u32) {
        let mut added = 0u32;
        let mut removed = 0u32;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line.line_type {
                    DiffLineType::Added => added += 1,
                    DiffLineType::Removed => removed += 1,
                    DiffLineType::Context => {}
                }
            }
        }
        (added, removed)
    }
}
'''))

files.append(("diff_in_ide.rs", '''//! Diff in IDE hook (useDiffInIDE.ts).
//!
//! Opens diffs in the connected IDE for side-by-side comparison,
//! managing temporary files and diff editor lifecycle.

use std::collections::HashMap;
use std::path::PathBuf;

/// A pending diff to show in the IDE.
#[derive(Debug, Clone)]
pub struct IdeDiff {
    pub id: String,
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub title: String,
    pub temp_file: Option<PathBuf>,
}

/// State for diff-in-IDE management.
#[derive(Debug, Clone)]
pub struct DiffInIdeState {
    pub active_diffs: HashMap<String, IdeDiff>,
    pub pending_open: Vec<String>,
    pub ide_connected: bool,
    pub last_opened: Option<String>,
}

impl DiffInIdeState {
    pub fn new() -> Self {
        Self {
            active_diffs: HashMap::new(),
            pending_open: Vec::new(),
            ide_connected: false,
            last_opened: None,
        }
    }

    /// Register a new diff to be shown.
    pub fn register_diff(&mut self, diff: IdeDiff) {
        let id = diff.id.clone();
        self.active_diffs.insert(id.clone(), diff);
        if self.ide_connected {
            self.pending_open.push(id);
        }
    }

    /// Mark a diff as opened in the IDE.
    pub fn mark_opened(&mut self, id: &str) {
        self.pending_open.retain(|i| i != id);
        self.last_opened = Some(id.to_string());
    }

    /// Close a diff (remove temp files).
    pub fn close_diff(&mut self, id: &str) {
        self.active_diffs.remove(id);
        self.pending_open.retain(|i| i != id);
    }

    /// Set IDE connection status.
    pub fn set_ide_connected(&mut self, connected: bool) {
        self.ide_connected = connected;
        // Queue all active diffs for opening on reconnect
        if connected {
            for id in self.active_diffs.keys() {
                if !self.pending_open.contains(id) {
                    self.pending_open.push(id.clone());
                }
            }
        }
    }

    /// Get the next diff to open.
    pub fn next_pending(&self) -> Option<&IdeDiff> {
        self.pending_open.first().and_then(|id| self.active_diffs.get(id))
    }

    /// Clean up all temporary files.
    pub fn cleanup(&mut self) -> Vec<PathBuf> {
        let paths: Vec<PathBuf> = self.active_diffs.values()
            .filter_map(|d| d.temp_file.clone())
            .collect();
        self.active_diffs.clear();
        self.pending_open.clear();
        paths
    }
}

impl Default for DiffInIdeState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("direct_connect.rs", '''//! Direct connect hook (useDirectConnect.ts).
//!
//! Manages direct connection to a remote session, handling permission
//! confirmations and remote permission responses.

use std::collections::VecDeque;

/// A permission request from a remote session.
#[derive(Debug, Clone)]
pub struct RemotePermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub args: String,
    pub session_id: String,
}

/// Response to a remote permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemotePermissionResponse {
    Allow,
    Deny,
    AllowAlways,
}

/// State for direct connection management.
#[derive(Debug, Clone)]
pub struct DirectConnectState {
    pub is_connected: bool,
    pub session_id: Option<String>,
    pub pending_permissions: VecDeque<RemotePermissionRequest>,
    pub permission_history: Vec<(String, RemotePermissionResponse)>,
    pub auto_approve_tools: Vec<String>,
}

impl DirectConnectState {
    pub fn new() -> Self {
        Self {
            is_connected: false,
            session_id: None,
            pending_permissions: VecDeque::new(),
            permission_history: Vec::new(),
            auto_approve_tools: Vec::new(),
        }
    }

    /// Connect to a remote session.
    pub fn connect(&mut self, session_id: String) {
        self.is_connected = true;
        self.session_id = Some(session_id);
    }

    /// Disconnect from the remote session.
    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.session_id = None;
        self.pending_permissions.clear();
    }

    /// Add a permission request to the queue.
    pub fn add_permission_request(&mut self, request: RemotePermissionRequest) {
        // Check auto-approve list first
        if self.auto_approve_tools.contains(&request.tool_name) {
            self.permission_history.push((request.id, RemotePermissionResponse::Allow));
            return;
        }
        self.pending_permissions.push_back(request);
    }

    /// Respond to the current permission request.
    pub fn respond(&mut self, response: RemotePermissionResponse) -> Option<RemotePermissionRequest> {
        if let Some(request) = self.pending_permissions.pop_front() {
            if response == RemotePermissionResponse::AllowAlways {
                self.auto_approve_tools.push(request.tool_name.clone());
            }
            self.permission_history.push((request.id.clone(), response));
            Some(request)
        } else {
            None
        }
    }

    /// Get the current pending permission request.
    pub fn current_permission(&self) -> Option<&RemotePermissionRequest> {
        self.pending_permissions.front()
    }

    /// Check if there are pending permissions.
    pub fn has_pending_permissions(&self) -> bool {
        !self.pending_permissions.is_empty()
    }
}

impl Default for DirectConnectState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("dynamic_config.rs", '''//! Dynamic config hook (useDynamicConfig.ts).
//!
//! Provides access to dynamically-fetched configuration values
//! (from GrowthBook or similar feature flag service).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A cached dynamic config value.
#[derive(Debug, Clone)]
pub struct DynamicConfigEntry {
    pub value: serde_json::Value,
    pub fetched: bool,
}

/// State for dynamic configuration access.
#[derive(Debug, Clone)]
pub struct DynamicConfigState {
    pub configs: HashMap<String, DynamicConfigEntry>,
}

impl DynamicConfigState {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Get a config value, returning the default if not yet fetched.
    pub fn get<T: serde::de::DeserializeOwned>(&self, name: &str, default: T) -> T {
        match self.configs.get(name) {
            Some(entry) if entry.fetched => {
                serde_json::from_value(entry.value.clone()).unwrap_or(default)
            }
            _ => default,
        }
    }

    /// Get a config value as a raw JSON value.
    pub fn get_raw(&self, name: &str) -> Option<&serde_json::Value> {
        self.configs.get(name).filter(|e| e.fetched).map(|e| &e.value)
    }

    /// Set a config value (called when fetch completes).
    pub fn set(&mut self, name: String, value: serde_json::Value) {
        self.configs.insert(name, DynamicConfigEntry {
            value,
            fetched: true,
        });
    }

    /// Check if a config has been fetched.
    pub fn is_fetched(&self, name: &str) -> bool {
        self.configs.get(name).map_or(false, |e| e.fetched)
    }
}

impl Default for DynamicConfigState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared dynamic config store for async access.
pub type SharedDynamicConfig = Arc<RwLock<DynamicConfigState>>;
'''))

files.append(("elapsed_time.rs", '''//! Elapsed time hook (useElapsedTime.ts).
//!
//! Returns formatted elapsed time since a start time, with
//! interval-based updates.

use std::time::{Duration, Instant};

/// State for elapsed time display.
#[derive(Debug, Clone)]
pub struct ElapsedTimeState {
    pub start_time: Instant,
    pub is_running: bool,
    pub update_interval: Duration,
    pub paused_duration: Duration,
    pub end_time: Option<Instant>,
    cached_display: String,
    last_update: Instant,
}

impl ElapsedTimeState {
    pub fn new(start_time: Instant) -> Self {
        Self {
            start_time,
            is_running: true,
            update_interval: Duration::from_secs(1),
            paused_duration: Duration::ZERO,
            end_time: None,
            cached_display: String::new(),
            last_update: Instant::now(),
        }
    }

    /// Set the update interval.
    pub fn with_interval(mut self, ms: u64) -> Self {
        self.update_interval = Duration::from_millis(ms);
        self
    }

    /// Set paused duration to subtract.
    pub fn with_paused(mut self, paused: Duration) -> Self {
        self.paused_duration = paused;
        self
    }

    /// Freeze the timer at a specific end time.
    pub fn with_end_time(mut self, end: Instant) -> Self {
        self.end_time = Some(end);
        self
    }

    /// Get the current elapsed duration.
    pub fn elapsed(&self) -> Duration {
        let end = self.end_time.unwrap_or_else(Instant::now);
        end.saturating_duration_since(self.start_time)
            .saturating_sub(self.paused_duration)
    }

    /// Get formatted duration string (e.g., "1m 23s").
    pub fn formatted(&mut self) -> &str {
        let now = Instant::now();
        if now.duration_since(self.last_update) >= self.update_interval || self.cached_display.is_empty() {
            self.cached_display = format_duration(self.elapsed());
            self.last_update = now;
        }
        &self.cached_display
    }

    /// Check if the timer should tick (update display).
    pub fn should_tick(&self) -> bool {
        self.is_running && self.last_update.elapsed() >= self.update_interval
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        self.is_running = false;
        self.end_time = Some(Instant::now());
    }

    /// Resume the timer.
    pub fn resume(&mut self) {
        self.is_running = true;
        self.end_time = None;
    }
}

/// Format a duration as a human-readable string.
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, s)
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h {}m", h, m)
        }
    }
}
'''))

files.append(("exit_on_ctrl_cd.rs", '''//! Exit on Ctrl+C/D hook (useExitOnCtrlCD.ts).
//!
//! Double-press Ctrl+C or Ctrl+D to exit the application.

use super::double_press::{DoublePressAction, DoublePressState};

/// State for Ctrl+C/D exit behavior.
#[derive(Debug, Clone)]
pub struct ExitOnCtrlCDState {
    pub ctrl_c_press: DoublePressState,
    pub ctrl_d_press: DoublePressState,
    pub show_exit_message: bool,
    pub exit_key: Option<String>,
}

impl ExitOnCtrlCDState {
    pub fn new() -> Self {
        Self {
            ctrl_c_press: DoublePressState::new(),
            ctrl_d_press: DoublePressState::new(),
            show_exit_message: false,
            exit_key: None,
        }
    }

    /// Handle Ctrl+C press. Returns true if should exit.
    pub fn on_ctrl_c(&mut self, has_input: bool) -> ExitAction {
        if has_input {
            // Clear input on first press when there's text
            return ExitAction::ClearInput;
        }
        match self.ctrl_c_press.press() {
            DoublePressAction::FirstPress => {
                self.show_exit_message = true;
                self.exit_key = Some("Ctrl-C".to_string());
                ExitAction::ShowMessage
            }
            DoublePressAction::DoublePress => {
                self.show_exit_message = false;
                ExitAction::Exit
            }
        }
    }

    /// Handle Ctrl+D press. Returns true if should exit.
    pub fn on_ctrl_d(&mut self, input_empty: bool) -> ExitAction {
        if !input_empty {
            return ExitAction::DeleteForward;
        }
        match self.ctrl_d_press.press() {
            DoublePressAction::FirstPress => {
                self.show_exit_message = true;
                self.exit_key = Some("Ctrl-D".to_string());
                ExitAction::ShowMessage
            }
            DoublePressAction::DoublePress => {
                self.show_exit_message = false;
                ExitAction::Exit
            }
        }
    }

    /// Tick timeout state.
    pub fn tick(&mut self) {
        if self.ctrl_c_press.tick() || self.ctrl_d_press.tick() {
            self.show_exit_message = false;
            self.exit_key = None;
        }
    }
}

impl Default for ExitOnCtrlCDState {
    fn default() -> Self {
        Self::new()
    }
}

/// Action resulting from Ctrl+C/D handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitAction {
    ShowMessage,
    Exit,
    ClearInput,
    DeleteForward,
    None,
}
'''))

files.append(("exit_on_ctrl_cd_with_keybindings.rs", '''//! Exit on Ctrl+C/D with keybindings (useExitOnCtrlCDWithKeybindings.ts).
//!
//! Extended version that integrates with the keybinding system,
//! allowing custom exit key sequences.

use super::exit_on_ctrl_cd::{ExitAction, ExitOnCtrlCDState};

/// State for exit with keybinding integration.
#[derive(Debug, Clone)]
pub struct ExitOnCtrlCDWithKeybindingsState {
    pub inner: ExitOnCtrlCDState,
    pub custom_exit_binding: Option<String>,
    pub keybinding_context_active: bool,
}

impl ExitOnCtrlCDWithKeybindingsState {
    pub fn new() -> Self {
        Self {
            inner: ExitOnCtrlCDState::new(),
            custom_exit_binding: None,
            keybinding_context_active: false,
        }
    }

    /// Handle an input key, checking both standard and custom bindings.
    pub fn handle_key(&mut self, key: &str, has_input: bool, input_empty: bool) -> ExitAction {
        // Check custom exit binding first
        if let Some(ref binding) = self.custom_exit_binding {
            if key == binding && self.keybinding_context_active {
                return ExitAction::Exit;
            }
        }

        // Fall through to standard Ctrl+C/D handling
        match key {
            "ctrl+c" | "ctrl-c" => self.inner.on_ctrl_c(has_input),
            "ctrl+d" | "ctrl-d" => self.inner.on_ctrl_d(input_empty),
            _ => ExitAction::None,
        }
    }

    /// Set a custom exit keybinding.
    pub fn set_custom_binding(&mut self, binding: Option<String>) {
        self.custom_exit_binding = binding;
    }

    pub fn tick(&mut self) {
        self.inner.tick();
    }
}

impl Default for ExitOnCtrlCDWithKeybindingsState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("file_history_snapshot_init.rs", '''//! File history snapshot initialization (useFileHistorySnapshotInit.ts).
//!
//! Initializes file history snapshots at session start for rewind support.

use std::collections::HashMap;
use std::path::PathBuf;

/// A file snapshot taken at session start.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content_hash: String,
    pub size: u64,
    pub modified_at: u64,
}

/// State for file history snapshot initialization.
#[derive(Debug, Clone)]
pub struct FileHistorySnapshotInitState {
    pub snapshots: HashMap<PathBuf, FileSnapshot>,
    pub initialized: bool,
    pub initializing: bool,
    pub error: Option<String>,
}

impl FileHistorySnapshotInitState {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            initialized: false,
            initializing: false,
            error: None,
        }
    }

    /// Start initialization.
    pub fn start_init(&mut self) {
        self.initializing = true;
        self.error = None;
    }

    /// Add a snapshot.
    pub fn add_snapshot(&mut self, snapshot: FileSnapshot) {
        self.snapshots.insert(snapshot.path.clone(), snapshot);
    }

    /// Mark initialization as complete.
    pub fn finish_init(&mut self) {
        self.initializing = false;
        self.initialized = true;
    }

    /// Mark initialization as failed.
    pub fn fail_init(&mut self, error: String) {
        self.initializing = false;
        self.error = Some(error);
    }

    /// Get snapshot for a specific file.
    pub fn get_snapshot(&self, path: &PathBuf) -> Option<&FileSnapshot> {
        self.snapshots.get(path)
    }

    /// Check if a file has changed since snapshot.
    pub fn has_file_changed(&self, path: &PathBuf, current_hash: &str) -> bool {
        match self.snapshots.get(path) {
            Some(snapshot) => snapshot.content_hash != current_hash,
            None => true, // New file
        }
    }
}

impl Default for FileHistorySnapshotInitState {
    fn default() -> Self {
        Self::new()
    }
}
'''))

files.append(("file_suggestions.rs", '''//! File suggestions engine (fileSuggestions.ts).
//!
//! Provides file path suggestions for autocomplete based on fuzzy matching.

/// A file suggestion result.
#[derive(Debug, Clone)]
pub struct FileSuggestion {
    pub path: String,
    pub display: String,
    pub score: f64,
    pub is_directory: bool,
}

/// State for the file suggestion engine.
#[derive(Debug, Clone)]
pub struct FileSuggesterState {
    pub suggestions: Vec<FileSuggestion>,
    pub query: String,
    pub max_results: usize,
    pub all_files: Vec<String>,
    pub selected_index: Option<usize>,
}

impl FileSuggesterState {
    pub fn new(max_results: usize) -> Self {
        Self {
            suggestions: Vec::new(),
            query: String::new(),
            max_results,
            all_files: Vec::new(),
            selected_index: None,
        }
    }

    /// Set the full list of available files.
    pub fn set_files(&mut self, files: Vec<String>) {
        self.all_files = files;
        if !self.query.is_empty() {
            self.recompute();
        }
    }

    /// Update the search query and recompute suggestions.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected_index = None;
        self.recompute();
    }

    /// Recompute suggestions based on current query.
    fn recompute(&mut self) {
        if self.query.is_empty() {
            self.suggestions.clear();
            return;
        }

        let query_lower = self.query.to_lowercase();
        let mut scored: Vec<(f64, &String)> = self.all_files
            .iter()
            .filter_map(|path| {
                let score = fuzzy_score(&query_lower, &path.to_lowercase());
                if score > 0.0 {
                    Some((score, path))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.max_results);

        self.suggestions = scored
            .into_iter()
            .map(|(score, path)| {
                let display = path.rsplit('/').next().unwrap_or(path).to_string();
                FileSuggestion {
                    path: path.clone(),
                    display,
                    score,
                    is_directory: path.ends_with('/'),
                }
            })
            .collect();
    }

    /// Select the next suggestion.
    pub fn select_next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => (i + 1) % self.suggestions.len(),
            None => 0,
        });
    }

    /// Select the previous suggestion.
    pub fn select_prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
            None => self.suggestions.len() - 1,
        });
    }

    /// Get the currently selected suggestion.
    pub fn selected(&self) -> Option<&FileSuggestion> {
        self.selected_index.and_then(|i| self.suggestions.get(i))
    }

    /// Clear suggestions.
    pub fn clear(&mut self) {
        self.suggestions.clear();
        self.query.clear();
        self.selected_index = None;
    }
}

impl Default for FileSuggesterState {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Simple fuzzy matching score.
fn fuzzy_score(query: &str, target: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    if target.contains(query) {
        return 1.0 + (query.len() as f64 / target.len() as f64);
    }
    let mut qi = 0;
    let query_chars: Vec<char> = query.chars().collect();
    let mut score = 0.0;
    let mut consecutive = 0.0;

    for ch in target.chars() {
        if qi < query_chars.len() && ch == query_chars[qi] {
            qi += 1;
            consecutive += 1.0;
            score += consecutive;
        } else {
            consecutive = 0.0;
        }
    }

    if qi == query_chars.len() {
        score / target.len() as f64
    } else {
        0.0
    }
}
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
