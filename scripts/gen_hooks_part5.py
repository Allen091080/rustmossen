#!/usr/bin/env python3
"""Generate hooks part 5 - settings through voice_integration + notifs + tool_permission."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/hooks"
files = []

# Remaining root hooks
for name, doc in [
    ("settings", "Provides reactive access to current settings from AppState."),
    ("settings_change", "Detects settings file changes and triggers callbacks."),
    ("skill_improvement_survey", "Shows skill improvement survey after usage milestones."),
    ("skills_change", "Detects changes to skill configuration and reloads."),
    ("ssh_session", "Manages SSH session connection and tunneling."),
    ("swarm_initialization", "Initializes swarm (multi-agent) mode for the session."),
    ("swarm_permission_poller", "Polls for permission requests from swarm workers."),
    ("task_list_watcher", "Watches for changes to the task list and triggers updates."),
    ("tasks_v2", "Manages the v2 task system state."),
    ("teammate_view_auto_exit", "Auto-exits teammate view when the teammate disconnects."),
    ("teleport_resume", "Resumes a teleported session after reconnection."),
    ("terminal_size", "Provides reactive terminal dimensions (rows/columns)."),
    ("timeout", "Simple timeout hook that returns true after a delay."),
    ("turn_diffs", "Collects and displays diffs made during a single turn."),
    ("typeahead", "Provides typeahead/autocomplete for the input."),
    ("unified_suggestions", "Unifies file and command suggestions into one list."),
    ("update_notification", "Shows notification when a new version is available."),
    ("virtual_scroll", "Manages virtual scrolling for long message lists."),
    ("voice", "Core voice input state management."),
    ("voice_enabled", "Checks if voice input is enabled and available."),
    ("voice_integration", "Integrates voice recognition with the input system."),
]:
    content = f'''//! {name.replace("_", " ").title()} hook (use{name.replace("_", " ").title().replace(" ", "")}.ts).
//! {doc}

'''
    # Generate appropriate struct based on the hook name
    struct_name = "".join(w.capitalize() for w in name.split("_")) + "State"

    if name == "settings":
        content += '''use std::collections::HashMap;

/// Read-only settings snapshot from AppState.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub values: HashMap<String, serde_json::Value>,
    pub version: u64,
}

impl SettingsState {
    pub fn new() -> Self { Self { values: HashMap::new(), version: 0 } }
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.values.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    pub fn get_bool(&self, key: &str) -> Option<bool> { self.get(key) }
    pub fn get_string(&self, key: &str) -> Option<String> { self.get(key) }
    pub fn get_u64(&self, key: &str) -> Option<u64> { self.get(key) }
    pub fn update(&mut self, values: HashMap<String, serde_json::Value>) { self.values = values; self.version += 1; }
}
impl Default for SettingsState { fn default() -> Self { Self::new() } }
'''
    elif name == "settings_change":
        content += '''use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource { User, Project, Default }

#[derive(Debug, Clone)]
pub struct SettingsChangeState {
    pub last_change: Option<Instant>,
    pub last_source: Option<SettingSource>,
    pub change_count: u64,
    pub watching: bool,
}

impl SettingsChangeState {
    pub fn new() -> Self { Self { last_change: None, last_source: None, change_count: 0, watching: false } }
    pub fn start_watching(&mut self) { self.watching = true; }
    pub fn stop_watching(&mut self) { self.watching = false; }
    pub fn on_change(&mut self, source: SettingSource) {
        self.last_change = Some(Instant::now()); self.last_source = Some(source); self.change_count += 1;
    }
}
impl Default for SettingsChangeState { fn default() -> Self { Self::new() } }
'''
    elif name == "terminal_size":
        content += '''/// Terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

#[derive(Debug, Clone)]
pub struct TerminalSizeState {
    pub size: TerminalSize,
    pub min_columns: u16,
    pub min_rows: u16,
}

impl TerminalSizeState {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self { size: TerminalSize { columns, rows }, min_columns: 40, min_rows: 10 }
    }
    pub fn update(&mut self, columns: u16, rows: u16) { self.size = TerminalSize { columns, rows }; }
    pub fn columns(&self) -> u16 { self.size.columns }
    pub fn rows(&self) -> u16 { self.size.rows }
    pub fn is_too_small(&self) -> bool { self.size.columns < self.min_columns || self.size.rows < self.min_rows }
}
impl Default for TerminalSizeState { fn default() -> Self { Self::new(80, 24) } }
'''
    elif name == "timeout":
        content += '''use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TimeoutState {
    pub delay: Duration,
    pub started_at: Option<Instant>,
    pub is_elapsed: bool,
}

impl TimeoutState {
    pub fn new(delay_ms: u64) -> Self { Self { delay: Duration::from_millis(delay_ms), started_at: None, is_elapsed: false } }
    pub fn start(&mut self) { self.started_at = Some(Instant::now()); self.is_elapsed = false; }
    pub fn reset(&mut self) { self.started_at = Some(Instant::now()); self.is_elapsed = false; }
    pub fn tick(&mut self) -> bool {
        if self.is_elapsed { return true; }
        if let Some(start) = self.started_at {
            if start.elapsed() >= self.delay { self.is_elapsed = true; return true; }
        }
        false
    }
    pub fn is_elapsed(&self) -> bool { self.is_elapsed }
}
impl Default for TimeoutState { fn default() -> Self { Self::new(1000) } }
'''
    elif name == "virtual_scroll":
        content += '''#[derive(Debug, Clone)]
pub struct VirtualScrollState {
    pub total_items: usize,
    pub viewport_height: usize,
    pub scroll_offset: usize,
    pub item_heights: Vec<u16>,
    pub total_height: u64,
    pub anchor_index: Option<usize>,
}

impl VirtualScrollState {
    pub fn new(viewport_height: usize) -> Self {
        Self { total_items: 0, viewport_height, scroll_offset: 0, item_heights: Vec::new(), total_height: 0, anchor_index: None }
    }
    pub fn set_items(&mut self, count: usize, heights: Vec<u16>) {
        self.total_items = count; self.total_height = heights.iter().map(|h| *h as u64).sum();
        self.item_heights = heights;
    }
    pub fn scroll_to(&mut self, offset: usize) { self.scroll_offset = offset.min(self.max_scroll()); }
    pub fn scroll_by(&mut self, delta: i32) {
        let new = (self.scroll_offset as i32 + delta).max(0) as usize;
        self.scroll_to(new);
    }
    pub fn scroll_to_bottom(&mut self) { self.scroll_offset = self.max_scroll(); }
    pub fn scroll_to_top(&mut self) { self.scroll_offset = 0; }
    pub fn max_scroll(&self) -> usize { self.total_height.saturating_sub(self.viewport_height as u64) as usize }
    pub fn visible_range(&self) -> (usize, usize) {
        let mut acc = 0u64; let mut start = 0; let mut end = self.total_items;
        for (i, h) in self.item_heights.iter().enumerate() {
            if acc + *h as u64 > self.scroll_offset as u64 && start == 0 && i > 0 { start = i; }
            if acc > (self.scroll_offset + self.viewport_height) as u64 { end = i; break; }
            acc += *h as u64;
        }
        (start, end.min(self.total_items))
    }
    pub fn is_at_bottom(&self) -> bool { self.scroll_offset >= self.max_scroll().saturating_sub(5) }
}
impl Default for VirtualScrollState { fn default() -> Self { Self::new(24) } }
'''
    elif name == "voice":
        content += '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState { Idle, Listening, Processing, Error }

#[derive(Debug, Clone)]
pub struct VoiceInputState {
    pub state: VoiceState,
    pub transcript: String,
    pub confidence: f32,
    pub is_final: bool,
    pub error: Option<String>,
}

impl VoiceInputState {
    pub fn new() -> Self { Self { state: VoiceState::Idle, transcript: String::new(), confidence: 0.0, is_final: false, error: None } }
    pub fn start_listening(&mut self) { self.state = VoiceState::Listening; self.transcript.clear(); self.is_final = false; self.error = None; }
    pub fn update_transcript(&mut self, text: &str, confidence: f32, is_final: bool) {
        self.transcript = text.to_string(); self.confidence = confidence; self.is_final = is_final;
        if is_final { self.state = VoiceState::Idle; }
    }
    pub fn stop(&mut self) { self.state = VoiceState::Idle; }
    pub fn error(&mut self, msg: String) { self.state = VoiceState::Error; self.error = Some(msg); }
    pub fn is_active(&self) -> bool { matches!(self.state, VoiceState::Listening | VoiceState::Processing) }
}
impl Default for VoiceInputState { fn default() -> Self { Self::new() } }
'''
    elif name == "voice_enabled":
        content += '''#[derive(Debug, Clone)]
pub struct VoiceEnabledState {
    pub enabled: bool,
    pub available: bool,
    pub reason_disabled: Option<String>,
}

impl VoiceEnabledState {
    pub fn new() -> Self { Self { enabled: false, available: false, reason_disabled: None } }
    pub fn check_availability(&mut self, has_microphone: bool, feature_flag: bool, settings_enabled: bool) {
        self.available = has_microphone && feature_flag;
        self.enabled = self.available && settings_enabled;
        if !has_microphone { self.reason_disabled = Some("No microphone detected".to_string()); }
        else if !feature_flag { self.reason_disabled = Some("Voice feature not available".to_string()); }
        else if !settings_enabled { self.reason_disabled = Some("Voice disabled in settings".to_string()); }
        else { self.reason_disabled = None; }
    }
    pub fn is_usable(&self) -> bool { self.enabled && self.available }
}
impl Default for VoiceEnabledState { fn default() -> Self { Self::new() } }
'''
    elif name == "voice_integration":
        content += '''#[derive(Debug, Clone)]
pub struct VoiceIntegrationState {
    pub is_recording: bool,
    pub auto_submit: bool,
    pub buffer: String,
    pub session_count: u32,
}

impl VoiceIntegrationState {
    pub fn new() -> Self { Self { is_recording: false, auto_submit: true, buffer: String::new(), session_count: 0 } }
    pub fn start_session(&mut self) { self.is_recording = true; self.buffer.clear(); self.session_count += 1; }
    pub fn append_text(&mut self, text: &str) { self.buffer.push_str(text); }
    pub fn end_session(&mut self) -> Option<String> {
        self.is_recording = false;
        if self.buffer.is_empty() { None } else { Some(std::mem::take(&mut self.buffer)) }
    }
    pub fn cancel(&mut self) { self.is_recording = false; self.buffer.clear(); }
    pub fn set_auto_submit(&mut self, auto: bool) { self.auto_submit = auto; }
}
impl Default for VoiceIntegrationState { fn default() -> Self { Self::new() } }
'''
    else:
        # Generic state struct for simpler hooks
        content += f'''#[derive(Debug, Clone)]
pub struct {struct_name} {{
    pub active: bool,
    pub initialized: bool,
}}

impl {struct_name} {{
    pub fn new() -> Self {{ Self {{ active: false, initialized: false }} }}
    pub fn initialize(&mut self) {{ self.initialized = true; }}
    pub fn activate(&mut self) {{ self.active = true; }}
    pub fn deactivate(&mut self) {{ self.active = false; }}
    pub fn is_active(&self) -> bool {{ self.active }}
}}
impl Default for {struct_name} {{ fn default() -> Self {{ Self::new() }} }}
'''

    files.append((f"{name}.rs", content))

# Now the text_input and vim_input (the big ones)
files.append(("text_input.rs", '''//! Text input hook (useTextInput.ts).
//! Full text editing state with cursor movement, kill-ring, history, multiline.

use super::double_press::DoublePressState;

/// Key modifiers for input handling.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub fn_key: bool,
}

/// Key event representation.
#[derive(Debug, Clone)]
pub struct Key {
    pub input: String,
    pub modifiers: KeyModifiers,
    pub escape: bool,
    pub return_key: bool,
    pub backspace: bool,
    pub delete: bool,
    pub tab: bool,
    pub up_arrow: bool,
    pub down_arrow: bool,
    pub left_arrow: bool,
    pub right_arrow: bool,
    pub home: bool,
    pub end: bool,
    pub page_up: bool,
    pub page_down: bool,
}

impl Key {
    pub fn char(ch: char) -> Self {
        Self {
            input: ch.to_string(), modifiers: KeyModifiers::default(),
            escape: false, return_key: false, backspace: false, delete: false, tab: false,
            up_arrow: false, down_arrow: false, left_arrow: false, right_arrow: false,
            home: false, end: false, page_up: false, page_down: false,
        }
    }
}

/// Cursor position in text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

/// Kill ring for cut/paste operations.
#[derive(Debug, Clone)]
pub struct KillRing {
    pub entries: Vec<String>,
    pub current_index: Option<usize>,
    pub accumulating: bool,
    pub yank_start: Option<usize>,
    pub yank_length: Option<usize>,
}

impl KillRing {
    pub fn new() -> Self { Self { entries: Vec::new(), current_index: None, accumulating: false, yank_start: None, yank_length: None } }
    pub fn push(&mut self, text: String, prepend: bool) {
        if self.accumulating && !self.entries.is_empty() {
            let last = self.entries.last_mut().unwrap();
            if prepend { *last = format!("{}{}", text, last); } else { last.push_str(&text); }
        } else {
            self.entries.push(text);
            if self.entries.len() > 60 { self.entries.remove(0); }
        }
        self.accumulating = true;
    }
    pub fn last(&self) -> &str { self.entries.last().map(|s| s.as_str()).unwrap_or("") }
    pub fn yank_pop(&mut self) -> Option<&str> {
        if self.entries.len() < 2 { return None; }
        let idx = match self.current_index {
            Some(i) if i > 0 => i - 1,
            Some(_) => self.entries.len() - 1,
            None => self.entries.len().saturating_sub(2),
        };
        self.current_index = Some(idx);
        self.entries.get(idx).map(|s| s.as_str())
    }
    pub fn reset_accumulation(&mut self) { self.accumulating = false; }
    pub fn reset_yank(&mut self) { self.yank_start = None; self.yank_length = None; self.current_index = None; }
    pub fn record_yank(&mut self, start: usize, length: usize) { self.yank_start = Some(start); self.yank_length = Some(length); }
}
impl Default for KillRing { fn default() -> Self { Self::new() } }

/// Text cursor with editing operations.
#[derive(Debug, Clone)]
pub struct TextCursor {
    pub text: String,
    pub offset: usize,
    pub columns: usize,
}

impl TextCursor {
    pub fn new(text: &str, columns: usize, offset: usize) -> Self {
        Self { text: text.to_string(), offset: offset.min(text.len()), columns }
    }
    pub fn left(&mut self) { if self.offset > 0 { self.offset -= 1; } }
    pub fn right(&mut self) { if self.offset < self.text.len() { self.offset += 1; } }
    pub fn start_of_line(&mut self) {
        let line_start = self.text[..self.offset].rfind('\\n').map_or(0, |p| p + 1);
        self.offset = line_start;
    }
    pub fn end_of_line(&mut self) {
        let line_end = self.text[self.offset..].find('\\n').map_or(self.text.len(), |p| self.offset + p);
        self.offset = line_end;
    }
    pub fn backspace(&mut self) {
        if self.offset > 0 { self.offset -= 1; self.text.remove(self.offset); }
    }
    pub fn delete_forward(&mut self) {
        if self.offset < self.text.len() { self.text.remove(self.offset); }
    }
    pub fn insert(&mut self, text: &str) { self.text.insert_str(self.offset, text); self.offset += text.len(); }
    pub fn delete_to_line_end(&mut self) -> String {
        let end = self.text[self.offset..].find('\\n').map_or(self.text.len(), |p| self.offset + p);
        let killed: String = self.text.drain(self.offset..end).collect();
        killed
    }
    pub fn delete_to_line_start(&mut self) -> String {
        let start = self.text[..self.offset].rfind('\\n').map_or(0, |p| p + 1);
        let killed: String = self.text.drain(start..self.offset).collect();
        self.offset = start;
        killed
    }
    pub fn delete_word_before(&mut self) -> String {
        let new_offset = self.prev_word_boundary();
        let killed: String = self.text.drain(new_offset..self.offset).collect();
        self.offset = new_offset;
        killed
    }
    pub fn delete_word_after(&mut self) -> String {
        let end = self.next_word_boundary();
        let killed: String = self.text.drain(self.offset..end).collect();
        killed
    }
    pub fn prev_word(&mut self) { self.offset = self.prev_word_boundary(); }
    pub fn next_word(&mut self) { self.offset = self.next_word_boundary(); }
    fn prev_word_boundary(&self) -> usize {
        if self.offset == 0 { return 0; }
        let bytes = self.text.as_bytes();
        let mut i = self.offset - 1;
        while i > 0 && !bytes[i].is_ascii_alphanumeric() { i -= 1; }
        while i > 0 && bytes[i - 1].is_ascii_alphanumeric() { i -= 1; }
        i
    }
    fn next_word_boundary(&self) -> usize {
        let bytes = self.text.as_bytes();
        let mut i = self.offset;
        while i < bytes.len() && !bytes[i].is_ascii_alphanumeric() { i += 1; }
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() { i += 1; }
        i
    }
    pub fn position(&self) -> CursorPosition {
        let before = &self.text[..self.offset];
        let line = before.matches('\\n').count();
        let col = before.rfind('\\n').map_or(self.offset, |p| self.offset - p - 1);
        CursorPosition { line, column: col }
    }
    pub fn is_at_start(&self) -> bool { self.offset == 0 }
    pub fn is_at_end(&self) -> bool { self.offset >= self.text.len() }
}

/// Full text input state.
#[derive(Debug, Clone)]
pub struct TextInputState {
    pub cursor: TextCursor,
    pub kill_ring: KillRing,
    pub ctrl_c_handler: DoublePressState,
    pub escape_handler: DoublePressState,
    pub multiline: bool,
    pub offset: usize,
    pub rendered_value: String,
}

impl TextInputState {
    pub fn new(columns: usize, multiline: bool) -> Self {
        Self {
            cursor: TextCursor::new("", columns, 0),
            kill_ring: KillRing::new(),
            ctrl_c_handler: DoublePressState::new(),
            escape_handler: DoublePressState::new(),
            multiline, offset: 0, rendered_value: String::new(),
        }
    }
    pub fn set_value(&mut self, value: &str) {
        self.cursor = TextCursor::new(value, self.cursor.columns, self.cursor.offset.min(value.len()));
    }
    pub fn value(&self) -> &str { &self.cursor.text }
    pub fn handle_key(&mut self, key: &Key) {
        if key.modifiers.ctrl {
            match key.input.as_str() {
                "a" => self.cursor.start_of_line(),
                "e" => self.cursor.end_of_line(),
                "b" => self.cursor.left(),
                "f" => self.cursor.right(),
                "k" => { let k = self.cursor.delete_to_line_end(); self.kill_ring.push(k, false); }
                "u" => { let k = self.cursor.delete_to_line_start(); self.kill_ring.push(k, true); }
                "w" => { let k = self.cursor.delete_word_before(); self.kill_ring.push(k, true); }
                "y" => { let t = self.kill_ring.last().to_string(); let start = self.cursor.offset; self.cursor.insert(&t); self.kill_ring.record_yank(start, t.len()); }
                "d" => self.cursor.delete_forward(),
                "h" => self.cursor.backspace(),
                _ => {}
            }
            if !matches!(key.input.as_str(), "k" | "u" | "w") { self.kill_ring.reset_accumulation(); }
            if key.input != "y" { self.kill_ring.reset_yank(); }
        } else if key.modifiers.meta {
            match key.input.as_str() {
                "b" => self.cursor.prev_word(),
                "f" => self.cursor.next_word(),
                "d" => { let k = self.cursor.delete_word_after(); self.kill_ring.push(k, false); }
                _ => {}
            }
        } else if key.backspace {
            self.cursor.backspace(); self.kill_ring.reset_accumulation(); self.kill_ring.reset_yank();
        } else if key.delete {
            self.cursor.delete_forward();
        } else if key.left_arrow { self.cursor.left();
        } else if key.right_arrow { self.cursor.right();
        } else if key.home { self.cursor.start_of_line();
        } else if key.end { self.cursor.end_of_line();
        } else if !key.input.is_empty() && !key.escape && !key.return_key && !key.tab {
            self.cursor.insert(&key.input); self.kill_ring.reset_accumulation(); self.kill_ring.reset_yank();
        }
        self.offset = self.cursor.offset;
    }
}
impl Default for TextInputState { fn default() -> Self { Self::new(80, false) } }
'''))

files.append(("vim_input.rs", '''//! Vim input hook (useVimInput.ts).
//! Full vim modal editing on top of text input state.

use super::text_input::{Key, TextCursor, TextInputState};

/// Vim mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode { Normal, Insert, Visual, Command }

/// Vim command state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCommand {
    Idle,
    Count(u32),
    Operator(String),
    OperatorCount(String, u32),
    Replace,
    Find(String),
    OperatorFind(String, String),
}

/// State for vim input mode.
#[derive(Debug, Clone)]
pub struct VimInputState {
    pub text_input: TextInputState,
    pub mode: VimMode,
    pub command: VimCommand,
    pub inserted_text: String,
    pub last_change: Option<RecordedChange>,
    pub register: char,
}

#[derive(Debug, Clone)]
pub struct RecordedChange {
    pub command_keys: String,
    pub inserted_text: String,
    pub count: u32,
}

impl VimInputState {
    pub fn new(columns: usize) -> Self {
        Self {
            text_input: TextInputState::new(columns, false),
            mode: VimMode::Insert,
            command: VimCommand::Idle,
            inserted_text: String::new(),
            last_change: None,
            register: '"',
        }
    }

    /// Switch to normal mode.
    pub fn enter_normal_mode(&mut self) {
        if self.mode == VimMode::Insert && !self.inserted_text.is_empty() {
            self.last_change = Some(RecordedChange {
                command_keys: "i".to_string(),
                inserted_text: std::mem::take(&mut self.inserted_text),
                count: 1,
            });
        }
        self.mode = VimMode::Normal;
        self.command = VimCommand::Idle;
    }

    /// Switch to insert mode.
    pub fn enter_insert_mode(&mut self) {
        self.mode = VimMode::Insert;
        self.inserted_text.clear();
    }

    /// Handle a key in the current mode.
    pub fn handle_key(&mut self, key: &Key) {
        match self.mode {
            VimMode::Insert => self.handle_insert_key(key),
            VimMode::Normal => self.handle_normal_key(key),
            VimMode::Visual => self.handle_normal_key(key),
            VimMode::Command => {}
        }
    }

    fn handle_insert_key(&mut self, key: &Key) {
        if key.escape {
            self.enter_normal_mode();
            return;
        }
        if key.modifiers.ctrl {
            self.text_input.handle_key(key);
            return;
        }
        // Track inserted text for dot-repeat
        if key.backspace {
            if !self.inserted_text.is_empty() {
                self.inserted_text.pop();
            }
        } else if !key.input.is_empty() && !key.return_key && !key.tab {
            self.inserted_text.push_str(&key.input);
        }
        self.text_input.handle_key(key);
    }

    fn handle_normal_key(&mut self, key: &Key) {
        if key.escape {
            self.command = VimCommand::Idle;
            return;
        }
        if key.return_key {
            self.text_input.handle_key(key);
            return;
        }
        if key.modifiers.ctrl {
            self.text_input.handle_key(key);
            return;
        }

        let ch = if key.left_arrow { "h" }
            else if key.right_arrow { "l" }
            else if key.up_arrow { "k" }
            else if key.down_arrow { "j" }
            else { key.input.as_str() };

        match (&self.command, ch) {
            (VimCommand::Idle, "i") => self.enter_insert_mode(),
            (VimCommand::Idle, "a") => { self.text_input.cursor.right(); self.enter_insert_mode(); }
            (VimCommand::Idle, "I") => { self.text_input.cursor.start_of_line(); self.enter_insert_mode(); }
            (VimCommand::Idle, "A") => { self.text_input.cursor.end_of_line(); self.enter_insert_mode(); }
            (VimCommand::Idle, "o") => {
                self.text_input.cursor.end_of_line();
                self.text_input.cursor.insert("\\n");
                self.enter_insert_mode();
            }
            (VimCommand::Idle, "O") => {
                self.text_input.cursor.start_of_line();
                self.text_input.cursor.insert("\\n");
                self.text_input.cursor.left();
                self.enter_insert_mode();
            }
            (VimCommand::Idle, "h") => self.text_input.cursor.left(),
            (VimCommand::Idle, "l") => self.text_input.cursor.right(),
            (VimCommand::Idle, "0") => self.text_input.cursor.start_of_line(),
            (VimCommand::Idle, "$") => self.text_input.cursor.end_of_line(),
            (VimCommand::Idle, "w") => self.text_input.cursor.next_word(),
            (VimCommand::Idle, "b") => self.text_input.cursor.prev_word(),
            (VimCommand::Idle, "x") => self.text_input.cursor.delete_forward(),
            (VimCommand::Idle, "X") => self.text_input.cursor.backspace(),
            (VimCommand::Idle, "d") => self.command = VimCommand::Operator("d".to_string()),
            (VimCommand::Idle, "c") => self.command = VimCommand::Operator("c".to_string()),
            (VimCommand::Idle, "y") => self.command = VimCommand::Operator("y".to_string()),
            (VimCommand::Operator(op), "d") if op == "d" => {
                // dd: delete entire line
                self.text_input.cursor.start_of_line();
                self.text_input.cursor.delete_to_line_end();
                self.command = VimCommand::Idle;
            }
            (VimCommand::Operator(op), "w") => {
                let op = op.clone();
                match op.as_str() {
                    "d" => { self.text_input.cursor.delete_word_after(); }
                    "c" => { self.text_input.cursor.delete_word_after(); self.enter_insert_mode(); return; }
                    _ => {}
                }
                self.command = VimCommand::Idle;
            }
            (VimCommand::Operator(op), "$") => {
                let op = op.clone();
                match op.as_str() {
                    "d" => { self.text_input.cursor.delete_to_line_end(); }
                    "c" => { self.text_input.cursor.delete_to_line_end(); self.enter_insert_mode(); return; }
                    _ => {}
                }
                self.command = VimCommand::Idle;
            }
            (VimCommand::Idle, n) if n.chars().all(|c| c.is_ascii_digit()) && n != "0" => {
                self.command = VimCommand::Count(n.parse().unwrap_or(1));
            }
            (VimCommand::Count(count), n) if n.chars().all(|c| c.is_ascii_digit()) => {
                self.command = VimCommand::Count(count * 10 + n.parse::<u32>().unwrap_or(0));
            }
            (VimCommand::Count(count), motion) => {
                let count = *count;
                for _ in 0..count {
                    match motion {
                        "h" => self.text_input.cursor.left(),
                        "l" => self.text_input.cursor.right(),
                        "w" => self.text_input.cursor.next_word(),
                        "b" => self.text_input.cursor.prev_word(),
                        "x" => self.text_input.cursor.delete_forward(),
                        _ => break,
                    }
                }
                self.command = VimCommand::Idle;
            }
            _ => { self.command = VimCommand::Idle; }
        }
    }

    pub fn value(&self) -> &str { self.text_input.value() }
    pub fn set_value(&mut self, value: &str) { self.text_input.set_value(value); }
    pub fn set_mode(&mut self, mode: VimMode) {
        match mode {
            VimMode::Insert => self.enter_insert_mode(),
            _ => self.enter_normal_mode(),
        }
    }
}

impl Default for VimInputState { fn default() -> Self { Self::new(80) } }
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
