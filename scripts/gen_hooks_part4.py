#!/usr/bin/env python3
"""Generate hooks part 4 - merged_clients through session_backgrounding."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"
files = []

files.append(("merged_clients.rs", '''//! Merged clients hook (useMergedClients.ts).
//! Combines multiple MCP client connections into a unified client list.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct McpClientEntry {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub connected: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MergedClientsState {
    pub clients: HashMap<String, McpClientEntry>,
    pub merged_order: Vec<String>,
}

impl MergedClientsState {
    pub fn new() -> Self { Self { clients: HashMap::new(), merged_order: Vec::new() } }
    pub fn add_client(&mut self, client: McpClientEntry) {
        let id = client.id.clone();
        self.clients.insert(id.clone(), client);
        if !self.merged_order.contains(&id) { self.merged_order.push(id); }
    }
    pub fn remove_client(&mut self, id: &str) {
        self.clients.remove(id);
        self.merged_order.retain(|i| i != id);
    }
    pub fn get_client(&self, id: &str) -> Option<&McpClientEntry> { self.clients.get(id) }
    pub fn connected_clients(&self) -> Vec<&McpClientEntry> {
        self.merged_order.iter().filter_map(|id| self.clients.get(id)).filter(|c| c.connected).collect()
    }
    pub fn all_clients(&self) -> Vec<&McpClientEntry> {
        self.merged_order.iter().filter_map(|id| self.clients.get(id)).collect()
    }
}
impl Default for MergedClientsState { fn default() -> Self { Self::new() } }
'''))

files.append(("merged_commands.rs", '''//! Merged commands hook (useMergedCommands.ts).
//! Combines built-in commands with plugin-provided commands.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    pub source: CommandSource,
    pub hidden: bool,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource { Builtin, Plugin(String), User }

#[derive(Debug, Clone)]
pub struct MergedCommandsState {
    pub commands: HashMap<String, CommandDef>,
    pub alias_map: HashMap<String, String>,
}

impl MergedCommandsState {
    pub fn new() -> Self { Self { commands: HashMap::new(), alias_map: HashMap::new() } }
    pub fn register(&mut self, cmd: CommandDef) {
        for alias in &cmd.aliases { self.alias_map.insert(alias.clone(), cmd.name.clone()); }
        self.commands.insert(cmd.name.clone(), cmd);
    }
    pub fn unregister(&mut self, name: &str) {
        if let Some(cmd) = self.commands.remove(name) {
            for alias in &cmd.aliases { self.alias_map.remove(alias); }
        }
    }
    pub fn resolve(&self, name: &str) -> Option<&CommandDef> {
        self.commands.get(name).or_else(|| self.alias_map.get(name).and_then(|n| self.commands.get(n)))
    }
    pub fn visible_commands(&self) -> Vec<&CommandDef> {
        self.commands.values().filter(|c| !c.hidden).collect()
    }
}
impl Default for MergedCommandsState { fn default() -> Self { Self::new() } }
'''))

files.append(("merged_tools.rs", '''//! Merged tools hook (useMergedTools.ts).
//! Combines built-in tools with MCP-provided tools.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub source: ToolSource,
    pub input_schema: serde_json::Value,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource { Builtin, Mcp(String), Plugin(String) }

#[derive(Debug, Clone)]
pub struct MergedToolsState {
    pub tools: HashMap<String, ToolDef>,
    pub override_order: Vec<String>,
}

impl MergedToolsState {
    pub fn new() -> Self { Self { tools: HashMap::new(), override_order: Vec::new() } }
    pub fn register(&mut self, tool: ToolDef) {
        self.override_order.push(tool.name.clone());
        self.tools.insert(tool.name.clone(), tool);
    }
    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
        self.override_order.retain(|n| n != name);
    }
    pub fn get_tool(&self, name: &str) -> Option<&ToolDef> { self.tools.get(name) }
    pub fn all_tools(&self) -> Vec<&ToolDef> {
        self.override_order.iter().filter_map(|n| self.tools.get(n)).collect()
    }
    pub fn tools_requiring_approval(&self) -> Vec<&ToolDef> {
        self.tools.values().filter(|t| t.requires_approval).collect()
    }
}
impl Default for MergedToolsState { fn default() -> Self { Self::new() } }
'''))

files.append(("min_display_time.rs", '''//! Min display time hook (useMinDisplayTime.ts).
//! Ensures a UI element is shown for at least a minimum duration.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct MinDisplayTimeState {
    pub min_duration: Duration,
    pub shown_at: Option<Instant>,
    pub content_ready: bool,
    pub force_visible: bool,
}

impl MinDisplayTimeState {
    pub fn new(min_ms: u64) -> Self {
        Self { min_duration: Duration::from_millis(min_ms), shown_at: None, content_ready: false, force_visible: false }
    }
    pub fn show(&mut self) { self.shown_at = Some(Instant::now()); self.force_visible = true; }
    pub fn mark_content_ready(&mut self) { self.content_ready = true; }
    pub fn should_remain_visible(&self) -> bool {
        if !self.force_visible { return false; }
        match self.shown_at {
            Some(t) => t.elapsed() < self.min_duration || !self.content_ready,
            None => false,
        }
    }
    pub fn can_hide(&self) -> bool {
        self.content_ready && self.shown_at.map_or(true, |t| t.elapsed() >= self.min_duration)
    }
    pub fn reset(&mut self) { self.shown_at = None; self.content_ready = false; self.force_visible = false; }
}
impl Default for MinDisplayTimeState { fn default() -> Self { Self::new(500) } }
'''))

files.append(("mossen_hint_recommendation.rs", '''//! Mossen hint recommendation (useMossenHintRecommendation.tsx).
//! Shows contextual hints to help users discover features.

#[derive(Debug, Clone)]
pub struct MossenHint {
    pub id: String,
    pub text: String,
    pub action: Option<String>,
    pub priority: HintPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintPriority { Low, Medium, High }

#[derive(Debug, Clone)]
pub struct MossenHintRecommendationState {
    pub active_hint: Option<MossenHint>,
    pub dismissed_hints: Vec<String>,
    pub shown_count: u32,
    pub max_shown: u32,
}

impl MossenHintRecommendationState {
    pub fn new() -> Self {
        Self { active_hint: None, dismissed_hints: Vec::new(), shown_count: 0, max_shown: 3 }
    }
    pub fn suggest(&mut self, hint: MossenHint) {
        if self.shown_count >= self.max_shown { return; }
        if self.dismissed_hints.contains(&hint.id) { return; }
        self.active_hint = Some(hint);
        self.shown_count += 1;
    }
    pub fn dismiss(&mut self) {
        if let Some(hint) = self.active_hint.take() { self.dismissed_hints.push(hint.id); }
    }
    pub fn clear(&mut self) { self.active_hint = None; }
}
impl Default for MossenHintRecommendationState { fn default() -> Self { Self::new() } }
'''))

files.append(("notify_after_timeout.rs", '''//! Notify after timeout hook (useNotifyAfterTimeout.ts).
//! Shows a notification after a specified delay.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct NotifyAfterTimeoutState {
    pub delay: Duration,
    pub started_at: Option<Instant>,
    pub fired: bool,
    pub notification_key: String,
    pub notification_text: String,
}

impl NotifyAfterTimeoutState {
    pub fn new(delay_ms: u64, key: &str, text: &str) -> Self {
        Self {
            delay: Duration::from_millis(delay_ms),
            started_at: None, fired: false,
            notification_key: key.to_string(), notification_text: text.to_string(),
        }
    }
    pub fn start(&mut self) { self.started_at = Some(Instant::now()); self.fired = false; }
    pub fn should_fire(&self) -> bool {
        !self.fired && self.started_at.map_or(false, |t| t.elapsed() >= self.delay)
    }
    pub fn fire(&mut self) -> Option<(&str, &str)> {
        if self.should_fire() { self.fired = true; Some((&self.notification_key, &self.notification_text)) }
        else { None }
    }
    pub fn reset(&mut self) { self.started_at = None; self.fired = false; }
    pub fn cancel(&mut self) { self.started_at = None; }
}
'''))

files.append(("official_marketplace_notification.rs", '''//! Official marketplace notification (useOfficialMarketplaceNotification.tsx).
//! Shows a notification about the official plugin marketplace.

#[derive(Debug, Clone)]
pub struct OfficialMarketplaceNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub marketplace_url: String,
}

impl OfficialMarketplaceNotificationState {
    pub fn new(url: &str) -> Self {
        Self { shown: false, dismissed: false, marketplace_url: url.to_string() }
    }
    pub fn should_show(&self, has_plugins: bool, seen_before: bool) -> bool {
        !self.shown && !self.dismissed && has_plugins && !seen_before
    }
    pub fn show(&mut self) { self.shown = true; }
    pub fn dismiss(&mut self) { self.dismissed = true; }
}
impl Default for OfficialMarketplaceNotificationState {
    fn default() -> Self { Self::new("https://marketplace.mossen.dev") }
}
'''))

files.append(("paste_handler.rs", '''//! Paste handler hook (usePasteHandler.ts).
//! Handles clipboard paste events including bracketed paste detection.

#[derive(Debug, Clone)]
pub struct PasteHandlerState {
    pub is_pasting: bool,
    pub paste_buffer: String,
    pub bracketed_paste_enabled: bool,
    pub last_paste_length: usize,
}

impl PasteHandlerState {
    pub fn new() -> Self {
        Self { is_pasting: false, paste_buffer: String::new(), bracketed_paste_enabled: true, last_paste_length: 0 }
    }
    pub fn start_paste(&mut self) { self.is_pasting = true; self.paste_buffer.clear(); }
    pub fn append(&mut self, text: &str) { self.paste_buffer.push_str(text); }
    pub fn end_paste(&mut self) -> String {
        self.is_pasting = false;
        self.last_paste_length = self.paste_buffer.len();
        std::mem::take(&mut self.paste_buffer)
    }
    pub fn handle_unbracket_paste(&mut self, text: &str) -> String {
        self.last_paste_length = text.len();
        text.to_string()
    }
    pub fn is_in_paste(&self) -> bool { self.is_pasting }
}
impl Default for PasteHandlerState { fn default() -> Self { Self::new() } }
'''))

files.append(("plugin_recommendation_base.rs", '''//! Plugin recommendation base (usePluginRecommendationBase.tsx).
//! Base logic for recommending plugins based on workspace analysis.

#[derive(Debug, Clone)]
pub struct PluginRecommendationEntry {
    pub plugin_id: String,
    pub reason: String,
    pub priority: u8,
    pub auto_install: bool,
}

#[derive(Debug, Clone)]
pub struct PluginRecommendationBaseState {
    pub recommendations: Vec<PluginRecommendationEntry>,
    pub dismissed: Vec<String>,
    pub auto_installed: Vec<String>,
}

impl PluginRecommendationBaseState {
    pub fn new() -> Self {
        Self { recommendations: Vec::new(), dismissed: Vec::new(), auto_installed: Vec::new() }
    }
    pub fn add_recommendation(&mut self, rec: PluginRecommendationEntry) {
        if !self.dismissed.contains(&rec.plugin_id) && !self.auto_installed.contains(&rec.plugin_id) {
            self.recommendations.push(rec);
        }
    }
    pub fn dismiss(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.dismissed.push(plugin_id.to_string());
    }
    pub fn mark_auto_installed(&mut self, plugin_id: &str) {
        self.recommendations.retain(|r| r.plugin_id != plugin_id);
        self.auto_installed.push(plugin_id.to_string());
    }
    pub fn pending(&self) -> &[PluginRecommendationEntry] { &self.recommendations }
}
impl Default for PluginRecommendationBaseState { fn default() -> Self { Self::new() } }
'''))

files.append(("pr_status.rs", '''//! PR status hook (usePrStatus.ts).
//! Polls PR review status periodically while the session is active.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReviewState { Pending, Approved, ChangesRequested, Commented, Dismissed }

#[derive(Debug, Clone)]
pub struct PrStatusState {
    pub number: Option<u32>,
    pub url: Option<String>,
    pub review_state: Option<PrReviewState>,
    pub last_updated: Option<Instant>,
    pub is_polling: bool,
    pub disabled: bool,
    pub poll_interval: Duration,
    pub last_fetch: Option<Instant>,
}

const POLL_INTERVAL_MS: u64 = 60_000;
const SLOW_GH_THRESHOLD_MS: u64 = 4_000;
const IDLE_STOP_MS: u64 = 3_600_000;

impl PrStatusState {
    pub fn new() -> Self {
        Self {
            number: None, url: None, review_state: None, last_updated: None,
            is_polling: false, disabled: false,
            poll_interval: Duration::from_millis(POLL_INTERVAL_MS), last_fetch: None,
        }
    }
    pub fn should_poll(&self) -> bool {
        if self.disabled || self.is_polling { return false; }
        self.last_fetch.map_or(true, |t| t.elapsed() >= self.poll_interval)
    }
    pub fn start_poll(&mut self) { self.is_polling = true; }
    pub fn complete_poll(&mut self, number: Option<u32>, url: Option<String>, state: Option<PrReviewState>, duration_ms: u64) {
        self.is_polling = false;
        self.last_fetch = Some(Instant::now());
        if duration_ms > SLOW_GH_THRESHOLD_MS { self.disabled = true; return; }
        self.number = number; self.url = url; self.review_state = state;
        self.last_updated = Some(Instant::now());
    }
    pub fn poll_error(&mut self) { self.is_polling = false; }
    pub fn should_stop_idle(&self, last_interaction: Instant) -> bool {
        last_interaction.elapsed() >= Duration::from_millis(IDLE_STOP_MS)
    }
}
impl Default for PrStatusState { fn default() -> Self { Self::new() } }
'''))

files.append(("prompt_suggestion.rs", '''//! Prompt suggestion hook (usePromptSuggestion.ts).
//! Provides autocomplete suggestions for the prompt input.

#[derive(Debug, Clone)]
pub struct PromptSuggestion {
    pub text: String,
    pub description: Option<String>,
    pub source: SuggestionSource,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionSource { History, Command, File, Context }

#[derive(Debug, Clone)]
pub struct PromptSuggestionState {
    pub suggestions: Vec<PromptSuggestion>,
    pub selected_index: Option<usize>,
    pub query: String,
    pub is_active: bool,
}

impl PromptSuggestionState {
    pub fn new() -> Self {
        Self { suggestions: Vec::new(), selected_index: None, query: String::new(), is_active: false }
    }
    pub fn update(&mut self, query: &str, suggestions: Vec<PromptSuggestion>) {
        self.query = query.to_string();
        self.suggestions = suggestions;
        self.selected_index = if self.suggestions.is_empty() { None } else { Some(0) };
        self.is_active = !self.suggestions.is_empty();
    }
    pub fn next(&mut self) {
        if let Some(idx) = &mut self.selected_index {
            *idx = (*idx + 1) % self.suggestions.len().max(1);
        }
    }
    pub fn prev(&mut self) {
        if let Some(idx) = &mut self.selected_index {
            *idx = if *idx == 0 { self.suggestions.len().saturating_sub(1) } else { *idx - 1 };
        }
    }
    pub fn accept(&mut self) -> Option<String> {
        let text = self.selected_index.and_then(|i| self.suggestions.get(i)).map(|s| s.text.clone());
        self.clear();
        text
    }
    pub fn clear(&mut self) { self.suggestions.clear(); self.selected_index = None; self.is_active = false; }
    pub fn selected(&self) -> Option<&PromptSuggestion> {
        self.selected_index.and_then(|i| self.suggestions.get(i))
    }
}
impl Default for PromptSuggestionState { fn default() -> Self { Self::new() } }
'''))

files.append(("prompts_from_mossen_in_chrome.rs", '''//! Prompts from Mossen in Chrome (usePromptsFromMossenInChrome.tsx).
//! Receives and processes prompts forwarded from the Chrome extension.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct ChromePrompt {
    pub id: String,
    pub text: String,
    pub url: Option<String>,
    pub page_title: Option<String>,
    pub selected_text: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct PromptsFromChromeState {
    pub pending_prompts: VecDeque<ChromePrompt>,
    pub processed_ids: Vec<String>,
    pub enabled: bool,
}

impl PromptsFromChromeState {
    pub fn new() -> Self {
        Self { pending_prompts: VecDeque::new(), processed_ids: Vec::new(), enabled: false }
    }
    pub fn receive(&mut self, prompt: ChromePrompt) {
        if !self.enabled { return; }
        if self.processed_ids.contains(&prompt.id) { return; }
        self.pending_prompts.push_back(prompt);
    }
    pub fn take_next(&mut self) -> Option<ChromePrompt> {
        let prompt = self.pending_prompts.pop_front()?;
        self.processed_ids.push(prompt.id.clone());
        Some(prompt)
    }
    pub fn has_pending(&self) -> bool { !self.pending_prompts.is_empty() }
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
}
impl Default for PromptsFromChromeState { fn default() -> Self { Self::new() } }
'''))

files.append(("queue_processor.rs", '''//! Queue processor hook (useQueueProcessor.ts).
//! Processes queued commands when conditions are met (no active query, no UI blocking).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueProcessorStatus { Idle, Processing, Blocked }

#[derive(Debug, Clone)]
pub struct QueueProcessorState {
    pub status: QueueProcessorStatus,
    pub is_query_active: bool,
    pub has_active_local_jsx_ui: bool,
    pub process_count: u64,
}

impl QueueProcessorState {
    pub fn new() -> Self {
        Self { status: QueueProcessorStatus::Idle, is_query_active: false, has_active_local_jsx_ui: false, process_count: 0 }
    }
    pub fn can_process(&self) -> bool {
        !self.is_query_active && !self.has_active_local_jsx_ui && self.status != QueueProcessorStatus::Processing
    }
    pub fn start_processing(&mut self) { self.status = QueueProcessorStatus::Processing; self.process_count += 1; }
    pub fn finish_processing(&mut self) { self.status = QueueProcessorStatus::Idle; }
    pub fn set_query_active(&mut self, active: bool) {
        self.is_query_active = active;
        if active { self.status = QueueProcessorStatus::Blocked; }
    }
    pub fn set_local_jsx_ui_active(&mut self, active: bool) { self.has_active_local_jsx_ui = active; }
}
impl Default for QueueProcessorState { fn default() -> Self { Self::new() } }
'''))

files.append(("remote_session.rs", '''//! Remote session hook (useRemoteSession.ts).
//! Manages connection to a remote/SSH session.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSessionStatus { Disconnected, Connecting, Connected, Error }

#[derive(Debug, Clone)]
pub struct RemoteSessionState {
    pub status: RemoteSessionStatus,
    pub session_id: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub connected_at: Option<Instant>,
    pub error: Option<String>,
    pub reconnect_count: u32,
}

impl RemoteSessionState {
    pub fn new() -> Self {
        Self { status: RemoteSessionStatus::Disconnected, session_id: None, host: None, port: None, connected_at: None, error: None, reconnect_count: 0 }
    }
    pub fn connecting(&mut self, host: &str, port: u16) {
        self.status = RemoteSessionStatus::Connecting; self.host = Some(host.to_string()); self.port = Some(port); self.error = None;
    }
    pub fn connected(&mut self, session_id: String) {
        self.status = RemoteSessionStatus::Connected; self.session_id = Some(session_id); self.connected_at = Some(Instant::now()); self.error = None;
    }
    pub fn disconnected(&mut self) { self.status = RemoteSessionStatus::Disconnected; self.session_id = None; self.connected_at = None; }
    pub fn error(&mut self, msg: String) { self.status = RemoteSessionStatus::Error; self.error = Some(msg); }
    pub fn reconnecting(&mut self) { self.status = RemoteSessionStatus::Connecting; self.reconnect_count += 1; }
    pub fn is_connected(&self) -> bool { self.status == RemoteSessionStatus::Connected }
}
impl Default for RemoteSessionState { fn default() -> Self { Self::new() } }
'''))

files.append(("render_placeholder.rs", '''//! Render placeholder (renderPlaceholder.ts).
//! Provides placeholder text rendering for the input when empty.

#[derive(Debug, Clone)]
pub struct RenderPlaceholderState {
    pub placeholder_text: String,
    pub is_visible: bool,
    pub style: PlaceholderStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderStyle { Dim, Italic, DimItalic }

impl RenderPlaceholderState {
    pub fn new(text: &str) -> Self {
        Self { placeholder_text: text.to_string(), is_visible: true, style: PlaceholderStyle::DimItalic }
    }
    pub fn should_show(&self, input_value: &str, is_focused: bool) -> bool {
        self.is_visible && input_value.is_empty() && is_focused
    }
    pub fn set_text(&mut self, text: String) { self.placeholder_text = text; }
    pub fn set_visible(&mut self, visible: bool) { self.is_visible = visible; }
    pub fn get_text(&self) -> &str { &self.placeholder_text }
}
impl Default for RenderPlaceholderState { fn default() -> Self { Self::new("Type a message...") } }
'''))

files.append(("scheduled_tasks.rs", '''//! Scheduled tasks hook (useScheduledTasks.ts).
//! Manages periodic background tasks that run on a schedule.

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: String,
    pub interval: Duration,
    pub last_run: Option<Instant>,
    pub enabled: bool,
    pub run_count: u64,
}

#[derive(Debug, Clone)]
pub struct ScheduledTasksState {
    pub tasks: HashMap<String, ScheduledTask>,
}

impl ScheduledTasksState {
    pub fn new() -> Self { Self { tasks: HashMap::new() } }
    pub fn register(&mut self, id: &str, interval_ms: u64) {
        self.tasks.insert(id.to_string(), ScheduledTask {
            id: id.to_string(), interval: Duration::from_millis(interval_ms),
            last_run: None, enabled: true, run_count: 0,
        });
    }
    pub fn unregister(&mut self, id: &str) { self.tasks.remove(id); }
    pub fn due_tasks(&self) -> Vec<&str> {
        self.tasks.values().filter(|t| t.enabled && t.last_run.map_or(true, |lr| lr.elapsed() >= t.interval))
            .map(|t| t.id.as_str()).collect()
    }
    pub fn mark_run(&mut self, id: &str) {
        if let Some(task) = self.tasks.get_mut(id) { task.last_run = Some(Instant::now()); task.run_count += 1; }
    }
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(task) = self.tasks.get_mut(id) { task.enabled = enabled; }
    }
}
impl Default for ScheduledTasksState { fn default() -> Self { Self::new() } }
'''))

files.append(("search_input.rs", '''//! Search input hook (useSearchInput.ts).
//! Manages search/filter input with cursor, kill-ring, and keybindings.

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct SearchInputState {
    pub query: String,
    pub cursor_offset: usize,
    pub is_active: bool,
    pub kill_ring: Vec<String>,
    pub kill_ring_index: Option<usize>,
}

const UNHANDLED_SPECIAL_KEYS: &[&str] = &["ArrowUp", "ArrowDown", "PageUp", "PageDown", "F1", "F2", "F3", "F4"];

impl SearchInputState {
    pub fn new() -> Self {
        Self { query: String::new(), cursor_offset: 0, is_active: false, kill_ring: Vec::new(), kill_ring_index: None }
    }
    pub fn activate(&mut self) { self.is_active = true; }
    pub fn deactivate(&mut self) { self.is_active = false; }
    pub fn set_query(&mut self, query: String) { self.cursor_offset = query.len(); self.query = query; }
    pub fn insert(&mut self, text: &str) {
        self.query.insert_str(self.cursor_offset, text);
        self.cursor_offset += text.len();
    }
    pub fn backspace(&mut self) {
        if self.cursor_offset > 0 {
            self.cursor_offset -= 1;
            self.query.remove(self.cursor_offset);
        }
    }
    pub fn delete(&mut self) {
        if self.cursor_offset < self.query.len() { self.query.remove(self.cursor_offset); }
    }
    pub fn move_left(&mut self) { if self.cursor_offset > 0 { self.cursor_offset -= 1; } }
    pub fn move_right(&mut self) { if self.cursor_offset < self.query.len() { self.cursor_offset += 1; } }
    pub fn move_start(&mut self) { self.cursor_offset = 0; }
    pub fn move_end(&mut self) { self.cursor_offset = self.query.len(); }
    pub fn kill_to_end(&mut self) {
        let killed: String = self.query.drain(self.cursor_offset..).collect();
        if !killed.is_empty() { self.kill_ring.push(killed); }
    }
    pub fn kill_to_start(&mut self) {
        let killed: String = self.query.drain(..self.cursor_offset).collect();
        self.cursor_offset = 0;
        if !killed.is_empty() { self.kill_ring.push(killed); }
    }
    pub fn yank(&mut self) {
        if let Some(text) = self.kill_ring.last().cloned() { self.insert(&text); }
    }
    pub fn clear(&mut self) { self.query.clear(); self.cursor_offset = 0; }
    pub fn is_special_key(key: &str) -> bool {
        UNHANDLED_SPECIAL_KEYS.contains(&key)
    }
}
impl Default for SearchInputState { fn default() -> Self { Self::new() } }
'''))

files.append(("session_backgrounding.rs", '''//! Session backgrounding hook (useSessionBackgrounding.ts).
//! Manages the session state when the terminal goes to background.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionForegroundState { Foreground, Background, Returning }

#[derive(Debug, Clone)]
pub struct SessionBackgroundingState {
    pub state: SessionForegroundState,
    pub background_since: Option<Instant>,
    pub total_background_time: std::time::Duration,
    pub background_count: u32,
}

impl SessionBackgroundingState {
    pub fn new() -> Self {
        Self { state: SessionForegroundState::Foreground, background_since: None, total_background_time: std::time::Duration::ZERO, background_count: 0 }
    }
    pub fn go_background(&mut self) {
        self.state = SessionForegroundState::Background;
        self.background_since = Some(Instant::now());
        self.background_count += 1;
    }
    pub fn go_foreground(&mut self) -> std::time::Duration {
        let elapsed = self.background_since.map_or(std::time::Duration::ZERO, |t| t.elapsed());
        self.total_background_time += elapsed;
        self.state = SessionForegroundState::Returning;
        self.background_since = None;
        elapsed
    }
    pub fn finish_return(&mut self) { self.state = SessionForegroundState::Foreground; }
    pub fn is_background(&self) -> bool { self.state == SessionForegroundState::Background }
    pub fn background_elapsed(&self) -> std::time::Duration {
        self.background_since.map_or(std::time::Duration::ZERO, |t| t.elapsed())
    }
}
impl Default for SessionBackgroundingState { fn default() -> Self { Self::new() } }
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
