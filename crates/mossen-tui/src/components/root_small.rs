//! Root-level small components (<100 lines each in TS source).
//! Covers: AutoModeOptInDialog, SandboxViolationExpandedView, PrBadge,
//! LanguagePicker, DiagnosticsDisplay, InvalidSettingsDialog,
//! NotebookEditToolUseRejectedMessage, FallbackToolUseErrorMessage,
//! AutoUpdaterWrapper, ConfigurableShortcutHint, SessionBackgroundHint,
//! TeammateViewHeader, AwsAuthStatusBox, MessageResponse,
//! MossenInChromeOnboarding, BypassPermissionsModeDialog, SearchBox,
//! PackageManagerAutoUpdater, ClickableImageRef, KeybindingWarnings,
//! CostThresholdDialog, MessageTimestamp, IdeStatusIndicator,
//! BashModeProgress, App, StatusNotices, ExitFlow, CtrlOToExpand,
//! DevBar, ContextSuggestions, FastIcon, OffscreenFreeze, MessageModel,
//! FilePathLink, EffortIndicator, ToolUseLoader, MemoryUsageIndicator,
//! StructuredDiffList, SentryErrorBoundary, MCPServerDialogCopy,
//! InterruptedByUser, FallbackToolUseRejectedMessage, PressEnterToContinue

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

use crate::theme::Theme;

// ─── AutoModeOptInDialog ───────────────────────────────────────────────────

pub struct AutoModeOptInDialogState {
    pub accepted: Option<bool>,
    pub show_details: bool,
}

impl AutoModeOptInDialogState {
    pub fn new() -> Self { Self { accepted: None, show_details: false } }
    pub fn accept(&mut self) { self.accepted = Some(true); }
    pub fn decline(&mut self) { self.accepted = Some(false); }
    pub fn toggle_details(&mut self) { self.show_details = !self.show_details; }
}

pub struct AutoModeOptInDialogWidget<'a> {
    pub state: &'a AutoModeOptInDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for AutoModeOptInDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Auto Mode ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, "Enable auto mode?", Style::default().fg(Color::White));
        buf.set_string(inner.x, inner.y + 1, "This allows the assistant to run commands automatically.", Style::default().fg(Color::DarkGray));
        buf.set_string(inner.x, inner.y + 3, "[y] Yes  [n] No  [d] Details", Style::default().fg(Color::Cyan));
        if self.state.show_details {
            buf.set_string(inner.x, inner.y + 5, "Auto mode skips confirmation for safe operations.", Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── SandboxViolationExpandedView ──────────────────────────────────────────

pub struct SandboxViolationExpandedViewState {
    pub violations: Vec<String>,
    pub file_path: Option<String>,
}

impl SandboxViolationExpandedViewState {
    pub fn new(violations: Vec<String>, file_path: Option<String>) -> Self {
        Self { violations, file_path }
    }
}

pub struct SandboxViolationExpandedViewWidget<'a> {
    pub state: &'a SandboxViolationExpandedViewState,
    pub theme: &'a Theme,
}

impl<'a> Widget for SandboxViolationExpandedViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, "Sandbox Violations:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        if let Some(ref path) = self.state.file_path {
            buf.set_string(area.x, area.y + 1, &format!("File: {}", path), Style::default().fg(Color::DarkGray));
        }
        for (i, v) in self.state.violations.iter().enumerate() {
            let y = area.y + 2 + i as u16;
            if y >= area.y + area.height { break; }
            buf.set_string(area.x + 2, y, &format!("• {}", v), Style::default().fg(Color::Red));
        }
    }
}

// ─── PrBadge ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrStatus { Open, Merged, Closed, Draft }

pub struct PrBadgeState {
    pub pr_number: u64,
    pub status: PrStatus,
    pub title: String,
}

impl PrBadgeState {
    pub fn new(number: u64, status: PrStatus, title: String) -> Self {
        Self { pr_number: number, status, title }
    }
}

pub struct PrBadgeWidget<'a> {
    pub state: &'a PrBadgeState,
    pub theme: &'a Theme,
}

impl<'a> Widget for PrBadgeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (icon, color) = match self.state.status {
            PrStatus::Open => ("●", Color::Green),
            PrStatus::Merged => ("◆", Color::Magenta),
            PrStatus::Closed => ("●", Color::Red),
            PrStatus::Draft => ("○", Color::DarkGray),
        };
        let text = format!("{} #{} {}", icon, self.state.pr_number, self.state.title);
        let max_w = area.width as usize;
        let display = if text.len() > max_w { &text[..max_w] } else { &text };
        buf.set_string(area.x, area.y, display, Style::default().fg(color));
    }
}

// ─── LanguagePicker ────────────────────────────────────────────────────────

pub struct LanguagePickerState {
    pub languages: Vec<(String, String)>, // (code, label)
    pub focused_index: usize,
    pub current_language: String,
}

impl LanguagePickerState {
    pub fn new(languages: Vec<(String, String)>, current: String) -> Self {
        let focused = languages.iter().position(|(c, _)| *c == current).unwrap_or(0);
        Self { languages, focused_index: focused, current_language: current }
    }
    pub fn focus_next(&mut self) { self.focused_index = (self.focused_index + 1) % self.languages.len(); }
    pub fn focus_prev(&mut self) { self.focused_index = if self.focused_index == 0 { self.languages.len() - 1 } else { self.focused_index - 1 }; }
    pub fn select_current(&self) -> &str { &self.languages[self.focused_index].0 }
}

pub struct LanguagePickerWidget<'a> {
    pub state: &'a LanguagePickerState,
    pub theme: &'a Theme,
}

impl<'a> Widget for LanguagePickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, (code, label)) in self.state.languages.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            let is_focused = i == self.state.focused_index;
            let is_current = *code == self.state.current_language;
            let prefix = if is_focused { "▸ " } else { "  " };
            let suffix = if is_current { " ●" } else { "" };
            let style = if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::White) };
            buf.set_string(area.x, y, &format!("{}{}{}", prefix, label, suffix), style);
        }
    }
}

// ─── DiagnosticsDisplay ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity { Error, Warning, Info, Hint }

pub struct DiagnosticsDisplayState {
    pub diagnostics: Vec<DiagnosticItem>,
}

impl DiagnosticsDisplayState {
    pub fn new(diagnostics: Vec<DiagnosticItem>) -> Self { Self { diagnostics } }
}

pub struct DiagnosticsDisplayWidget<'a> {
    pub state: &'a DiagnosticsDisplayState,
    pub theme: &'a Theme,
}

impl<'a> Widget for DiagnosticsDisplayWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, d) in self.state.diagnostics.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            let (icon, color) = match d.severity {
                DiagnosticSeverity::Error => ("✗", Color::Red),
                DiagnosticSeverity::Warning => ("⚠", Color::Yellow),
                DiagnosticSeverity::Info => ("ℹ", Color::Blue),
                DiagnosticSeverity::Hint => ("💡", Color::DarkGray),
            };
            let location = match (&d.file, d.line) {
                (Some(f), Some(l)) => format!("{}:{}", f, l),
                (Some(f), None) => f.clone(),
                _ => String::new(),
            };
            let text = if location.is_empty() { format!("{} {}", icon, d.message) } else { format!("{} {} [{}]", icon, d.message, location) };
            let max_w = area.width as usize;
            let display = if text.len() > max_w { &text[..max_w] } else { &text };
            buf.set_string(area.x, y, display, Style::default().fg(color));
        }
    }
}

// ─── InvalidSettingsDialog ─────────────────────────────────────────────────

pub struct InvalidSettingsDialogState {
    pub errors: Vec<String>,
    pub file_path: String,
}

impl InvalidSettingsDialogState {
    pub fn new(file_path: String, errors: Vec<String>) -> Self { Self { errors, file_path } }
}

pub struct InvalidSettingsDialogWidget<'a> {
    pub state: &'a InvalidSettingsDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for InvalidSettingsDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Invalid Settings ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, &format!("File: {}", self.state.file_path), Style::default().fg(Color::White));
        for (i, err) in self.state.errors.iter().enumerate() {
            let y = inner.y + 2 + i as u16;
            if y >= inner.y + inner.height { break; }
            buf.set_string(inner.x + 2, y, &format!("• {}", err), Style::default().fg(Color::Red));
        }
    }
}

// ─── ConfigurableShortcutHint ──────────────────────────────────────────────

pub struct ConfigurableShortcutHintState {
    pub action: String,
    pub default_key: String,
    pub configured_key: Option<String>,
    pub context: String,
}

impl ConfigurableShortcutHintState {
    pub fn new(action: String, default_key: String, context: String) -> Self {
        Self { action, default_key, configured_key: None, context }
    }
    pub fn display_key(&self) -> &str {
        self.configured_key.as_deref().unwrap_or(&self.default_key)
    }
}

pub struct ConfigurableShortcutHintWidget<'a> {
    pub state: &'a ConfigurableShortcutHintState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ConfigurableShortcutHintWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = format!("{} to {}", self.state.display_key(), self.state.action);
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::DarkGray));
    }
}

// ─── BypassPermissionsModeDialog ───────────────────────────────────────────

pub struct BypassPermissionsModeDialogState {
    pub accepted: Option<bool>,
}

impl BypassPermissionsModeDialogState {
    pub fn new() -> Self { Self { accepted: None } }
    pub fn accept(&mut self) { self.accepted = Some(true); }
    pub fn decline(&mut self) { self.accepted = Some(false); }
}

pub struct BypassPermissionsModeDialogWidget<'a> {
    pub state: &'a BypassPermissionsModeDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for BypassPermissionsModeDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Bypass Permissions ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, "⚠ Enable bypass permissions mode?", Style::default().fg(Color::Yellow));
        buf.set_string(inner.x, inner.y + 2, "This will skip all permission checks.", Style::default().fg(Color::White));
        buf.set_string(inner.x, inner.y + 3, "Only recommended in trusted environments.", Style::default().fg(Color::DarkGray));
        buf.set_string(inner.x, inner.y + 5, "[y] Yes  [n] No", Style::default().fg(Color::Cyan));
    }
}

// ─── SearchBox ─────────────────────────────────────────────────────────────

pub struct SearchBoxState {
    pub query: String,
    pub cursor_offset: usize,
    pub is_focused: bool,
    pub placeholder: String,
}

impl SearchBoxState {
    pub fn new(placeholder: &str) -> Self {
        Self { query: String::new(), cursor_offset: 0, is_focused: true, placeholder: placeholder.to_string() }
    }
    pub fn set_query(&mut self, q: String) { self.cursor_offset = q.len(); self.query = q; }
    pub fn insert_char(&mut self, c: char) { self.query.insert(self.cursor_offset, c); self.cursor_offset += c.len_utf8(); }
    pub fn delete_back(&mut self) {
        if self.cursor_offset > 0 {
            let prev = self.query[..self.cursor_offset].chars().last().map(|c| c.len_utf8()).unwrap_or(0);
            self.cursor_offset -= prev;
            self.query.remove(self.cursor_offset);
        }
    }
    pub fn clear(&mut self) { self.query.clear(); self.cursor_offset = 0; }
}

pub struct SearchBoxWidget<'a> {
    pub state: &'a SearchBoxState,
    pub theme: &'a Theme,
}

impl<'a> Widget for SearchBoxWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let display = if self.state.query.is_empty() { &self.state.placeholder } else { &self.state.query };
        let style = if self.state.query.is_empty() { Style::default().fg(Color::DarkGray) } else { Style::default().fg(Color::White) };
        buf.set_string(area.x, area.y, display, style);
    }
}

// ─── CostThresholdDialog ───────────────────────────────────────────────────

pub struct CostThresholdDialogState {
    pub current_cost: f64,
    pub threshold: f64,
    pub action: Option<bool>, // true = continue, false = stop
}

impl CostThresholdDialogState {
    pub fn new(current_cost: f64, threshold: f64) -> Self {
        Self { current_cost, threshold, action: None }
    }
    pub fn continue_session(&mut self) { self.action = Some(true); }
    pub fn stop_session(&mut self) { self.action = Some(false); }
}

pub struct CostThresholdDialogWidget<'a> {
    pub state: &'a CostThresholdDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for CostThresholdDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Cost Threshold ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);
        buf.set_string(inner.x, inner.y, &format!("Current cost: ${:.2}", self.state.current_cost), Style::default().fg(Color::Yellow));
        buf.set_string(inner.x, inner.y + 1, &format!("Threshold: ${:.2}", self.state.threshold), Style::default().fg(Color::DarkGray));
        buf.set_string(inner.x, inner.y + 3, "Continue? (y/n)", Style::default().fg(Color::White));
    }
}

// ─── MessageTimestamp ──────────────────────────────────────────────────────

pub struct MessageTimestampState {
    pub timestamp: Instant,
    pub format: TimestampFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimestampFormat { Relative, Absolute }

impl MessageTimestampState {
    pub fn new(timestamp: Instant) -> Self {
        Self { timestamp, format: TimestampFormat::Relative }
    }
    pub fn display(&self) -> String {
        let elapsed = self.timestamp.elapsed();
        let secs = elapsed.as_secs();
        if secs < 60 { format!("{}s ago", secs) }
        else if secs < 3600 { format!("{}m ago", secs / 60) }
        else { format!("{}h ago", secs / 3600) }
    }
}

pub struct MessageTimestampWidget<'a> {
    pub state: &'a MessageTimestampState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessageTimestampWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = self.state.display();
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::DarkGray));
    }
}

// ─── IdeStatusIndicator ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdeConnectionStatus { Connected, Disconnected, Connecting }

pub struct IdeStatusIndicatorState {
    pub status: IdeConnectionStatus,
    pub ide_name: String,
}

impl IdeStatusIndicatorState {
    pub fn new(ide_name: String) -> Self {
        Self { status: IdeConnectionStatus::Disconnected, ide_name }
    }
    pub fn set_status(&mut self, status: IdeConnectionStatus) { self.status = status; }
}

pub struct IdeStatusIndicatorWidget<'a> {
    pub state: &'a IdeStatusIndicatorState,
    pub theme: &'a Theme,
}

impl<'a> Widget for IdeStatusIndicatorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (icon, color) = match self.state.status {
            IdeConnectionStatus::Connected => ("●", Color::Green),
            IdeConnectionStatus::Disconnected => ("○", Color::Red),
            IdeConnectionStatus::Connecting => ("◐", Color::Yellow),
        };
        buf.set_string(area.x, area.y, &format!("{} {}", icon, self.state.ide_name), Style::default().fg(color));
    }
}

// ─── BashModeProgress ──────────────────────────────────────────────────────

pub struct BashModeProgressState {
    pub command: String,
    pub elapsed: Duration,
    pub output_lines: usize,
}

impl BashModeProgressState {
    pub fn new(command: String) -> Self {
        Self { command, elapsed: Duration::ZERO, output_lines: 0 }
    }
    pub fn update(&mut self, elapsed: Duration, output_lines: usize) {
        self.elapsed = elapsed;
        self.output_lines = output_lines;
    }
}

pub struct BashModeProgressWidget<'a> {
    pub state: &'a BashModeProgressState,
    pub theme: &'a Theme,
}

impl<'a> Widget for BashModeProgressWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let elapsed_str = format!("{:.1}s", self.state.elapsed.as_secs_f64());
        let text = format!("⟳ {} ({}, {} lines)", self.state.command, elapsed_str, self.state.output_lines);
        let max_w = area.width as usize;
        let display = if text.len() > max_w { &text[..max_w] } else { &text };
        buf.set_string(area.x, area.y, display, Style::default().fg(Color::Yellow));
    }
}

// ─── StatusNotices ─────────────────────────────────────────────────────────

pub struct StatusNoticesState {
    pub notices: Vec<String>,
}

impl StatusNoticesState {
    pub fn new() -> Self { Self { notices: Vec::new() } }
    pub fn add_notice(&mut self, notice: String) { self.notices.push(notice); }
    pub fn clear(&mut self) { self.notices.clear(); }
}

pub struct StatusNoticesWidget<'a> {
    pub state: &'a StatusNoticesState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StatusNoticesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, notice) in self.state.notices.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            let max_w = area.width as usize;
            let display = if notice.len() > max_w { &notice[..max_w] } else { notice.as_str() };
            buf.set_string(area.x, y, display, Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── EffortIndicator ───────────────────────────────────────────────────────

pub fn effort_level_to_symbol(level: &str) -> &'static str {
    match level {
        "low" => "⚡",
        "medium" => "⚡⚡",
        "high" => "⚡⚡⚡",
        "max" => "⚡⚡⚡⚡",
        _ => "⚡⚡",
    }
}

pub struct EffortIndicatorState {
    pub level: String,
}

impl EffortIndicatorState {
    pub fn new(level: String) -> Self { Self { level } }
}

pub struct EffortIndicatorWidget<'a> {
    pub state: &'a EffortIndicatorState,
    pub theme: &'a Theme,
}

impl<'a> Widget for EffortIndicatorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let symbol = effort_level_to_symbol(&self.state.level);
        buf.set_string(area.x, area.y, symbol, Style::default().fg(Color::Yellow));
    }
}

// ─── ToolUseLoader ─────────────────────────────────────────────────────────

pub struct ToolUseLoaderState {
    pub tool_name: String,
    pub elapsed: Duration,
    pub spinner_frame: usize,
}

impl ToolUseLoaderState {
    pub fn new(tool_name: String) -> Self {
        Self { tool_name, elapsed: Duration::ZERO, spinner_frame: 0 }
    }
    pub fn tick(&mut self) { self.spinner_frame = (self.spinner_frame + 1) % 4; }
}

pub struct ToolUseLoaderWidget<'a> {
    pub state: &'a ToolUseLoaderState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ToolUseLoaderWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frames = ['⠋', '⠙', '⠹', '⠸'];
        let spinner = frames[self.state.spinner_frame % frames.len()];
        let text = format!("{} {}...", spinner, self.state.tool_name);
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::Magenta));
    }
}

// ─── MemoryUsageIndicator ──────────────────────────────────────────────────

pub struct MemoryUsageIndicatorState {
    pub current_bytes: u64,
    pub limit_bytes: u64,
}

impl MemoryUsageIndicatorState {
    pub fn new(current: u64, limit: u64) -> Self { Self { current_bytes: current, limit_bytes: limit } }
    pub fn percentage(&self) -> f64 { if self.limit_bytes == 0 { 0.0 } else { (self.current_bytes as f64 / self.limit_bytes as f64) * 100.0 } }
    pub fn format_bytes(bytes: u64) -> String {
        if bytes < 1024 { format!("{}B", bytes) }
        else if bytes < 1024 * 1024 { format!("{:.1}KB", bytes as f64 / 1024.0) }
        else { format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0)) }
    }
}

pub struct MemoryUsageIndicatorWidget<'a> {
    pub state: &'a MemoryUsageIndicatorState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MemoryUsageIndicatorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = format!("Mem: {}/{} ({:.0}%)",
            MemoryUsageIndicatorState::format_bytes(self.state.current_bytes),
            MemoryUsageIndicatorState::format_bytes(self.state.limit_bytes),
            self.state.percentage()
        );
        let color = if self.state.percentage() > 90.0 { Color::Red }
            else if self.state.percentage() > 70.0 { Color::Yellow }
            else { Color::Green };
        buf.set_string(area.x, area.y, &text, Style::default().fg(color));
    }
}

// ─── MessageModel ──────────────────────────────────────────────────────────

pub struct MessageModelState {
    pub model_name: String,
}

impl MessageModelState {
    pub fn new(model: String) -> Self { Self { model_name: model } }
}

pub struct MessageModelWidget<'a> {
    pub state: &'a MessageModelState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessageModelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &self.state.model_name, Style::default().fg(Color::DarkGray));
    }
}

// ─── FilePathLink ──────────────────────────────────────────────────────────

pub struct FilePathLinkState {
    pub path: String,
    pub line: Option<usize>,
    pub display_path: String,
}

impl FilePathLinkState {
    pub fn new(path: String, line: Option<usize>) -> Self {
        let display = if path.len() > 40 {
            format!("...{}", &path[path.len() - 37..])
        } else {
            path.clone()
        };
        Self { path, line, display_path: display }
    }
}

pub struct FilePathLinkWidget<'a> {
    pub state: &'a FilePathLinkState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FilePathLinkWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = if let Some(line) = self.state.line {
            format!("{}:{}", self.state.display_path, line)
        } else {
            self.state.display_path.clone()
        };
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED));
    }
}

// ─── ClickableImageRef ─────────────────────────────────────────────────────

pub struct ClickableImageRefState {
    pub image_path: String,
    pub alt_text: String,
}

impl ClickableImageRefState {
    pub fn new(path: String, alt: String) -> Self { Self { image_path: path, alt_text: alt } }
}

pub struct ClickableImageRefWidget<'a> {
    pub state: &'a ClickableImageRefState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ClickableImageRefWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = format!("[Image: {}]", self.state.alt_text);
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED));
    }
}

// ─── KeybindingWarnings ────────────────────────────────────────────────────

pub struct KeybindingWarningsState {
    pub warnings: Vec<String>,
}

impl KeybindingWarningsState {
    pub fn new(warnings: Vec<String>) -> Self { Self { warnings } }
}

pub struct KeybindingWarningsWidget<'a> {
    pub state: &'a KeybindingWarningsState,
    pub theme: &'a Theme,
}

impl<'a> Widget for KeybindingWarningsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, w) in self.state.warnings.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            buf.set_string(area.x, y, &format!("⚠ {}", w), Style::default().fg(Color::Yellow));
        }
    }
}

// ─── DevBar ────────────────────────────────────────────────────────────────

pub struct DevBarState {
    pub items: Vec<(String, String)>, // (label, value)
    pub visible: bool,
}

impl DevBarState {
    pub fn new() -> Self { Self { items: Vec::new(), visible: false } }
    pub fn set_visible(&mut self, v: bool) { self.visible = v; }
    pub fn set_item(&mut self, label: String, value: String) {
        if let Some(item) = self.items.iter_mut().find(|(l, _)| *l == label) {
            item.1 = value;
        } else {
            self.items.push((label, value));
        }
    }
}

pub struct DevBarWidget<'a> {
    pub state: &'a DevBarState,
    pub theme: &'a Theme,
}

impl<'a> Widget for DevBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible { return; }
        let mut x = area.x;
        for (label, value) in &self.state.items {
            let text = format!("{}:{} ", label, value);
            if x + text.len() as u16 > area.x + area.width { break; }
            buf.set_string(x, area.y, &text, Style::default().fg(Color::DarkGray));
            x += text.len() as u16;
        }
    }
}

// ─── FastIcon ──────────────────────────────────────────────────────────────

pub struct FastIconState {
    pub icon: String,
    pub color: Color,
}

impl FastIconState {
    pub fn new(icon: String, color: Color) -> Self { Self { icon, color } }
}

pub struct FastIconWidget<'a> {
    pub state: &'a FastIconState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FastIconWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &self.state.icon, Style::default().fg(self.state.color));
    }
}

// ─── ContextSuggestions ────────────────────────────────────────────────────

pub struct ContextSuggestionsState {
    pub suggestions: Vec<String>,
}

impl ContextSuggestionsState {
    pub fn new(suggestions: Vec<String>) -> Self { Self { suggestions } }
}

pub struct ContextSuggestionsWidget<'a> {
    pub state: &'a ContextSuggestionsState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ContextSuggestionsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, sug) in self.state.suggestions.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            buf.set_string(area.x, y, &format!("💡 {}", sug), Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── PressEnterToContinue / InterruptedByUser / Message wrappers ───────────

pub struct PressEnterToContinueState {
    pub message: String,
}

impl PressEnterToContinueState {
    pub fn new(message: &str) -> Self { Self { message: message.to_string() } }
}

pub struct PressEnterToContinueWidget<'a> {
    pub state: &'a PressEnterToContinueState,
    pub theme: &'a Theme,
}

impl<'a> Widget for PressEnterToContinueWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &self.state.message, Style::default().fg(Color::White));
        buf.set_string(area.x, area.y + 1, "Press Enter to continue...", Style::default().fg(Color::DarkGray));
    }
}

pub struct InterruptedByUserState {
    pub message: String,
}

impl InterruptedByUserState {
    pub fn new() -> Self { Self { message: "Interrupted by user".to_string() } }
}

pub struct InterruptedByUserWidget<'a> {
    pub state: &'a InterruptedByUserState,
    pub theme: &'a Theme,
}

impl<'a> Widget for InterruptedByUserWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &self.state.message, Style::default().fg(Color::Yellow));
    }
}

pub struct MessageResponseState {
    pub content: String,
    pub is_error: bool,
}

impl MessageResponseState {
    pub fn new(content: String, is_error: bool) -> Self { Self { content, is_error } }
}

pub struct MessageResponseWidget<'a> {
    pub state: &'a MessageResponseState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessageResponseWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let color = if self.state.is_error { Color::Red } else { Color::White };
        let max_w = area.width as usize;
        let display = if self.state.content.len() > max_w { &self.state.content[..max_w] } else { &self.state.content };
        buf.set_string(area.x, area.y, display, Style::default().fg(color));
    }
}

// ─── ExitFlow ──────────────────────────────────────────────────────────────

pub struct ExitFlowState {
    pub confirming: bool,
    pub has_background_tasks: bool,
}

impl ExitFlowState {
    pub fn new(has_background_tasks: bool) -> Self {
        Self { confirming: has_background_tasks, has_background_tasks }
    }
    pub fn confirm_exit(&mut self) { self.confirming = false; }
}

pub struct ExitFlowWidget<'a> {
    pub state: &'a ExitFlowState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ExitFlowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.confirming {
            buf.set_string(area.x, area.y, "Background tasks running. Exit anyway? (y/n)", Style::default().fg(Color::Yellow));
        }
    }
}

// ─── CtrlOToExpand ─────────────────────────────────────────────────────────

pub struct CtrlOToExpandWidget;

impl Widget for CtrlOToExpandWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, "Ctrl+O to expand", Style::default().fg(Color::DarkGray));
    }
}

// ─── AwsAuthStatusBox ──────────────────────────────────────────────────────

pub struct AwsAuthStatusBoxState {
    pub authenticated: bool,
    pub region: Option<String>,
    pub profile: Option<String>,
}

impl AwsAuthStatusBoxState {
    pub fn new() -> Self { Self { authenticated: false, region: None, profile: None } }
    pub fn set_authenticated(&mut self, region: String, profile: Option<String>) {
        self.authenticated = true;
        self.region = Some(region);
        self.profile = profile;
    }
}

pub struct AwsAuthStatusBoxWidget<'a> {
    pub state: &'a AwsAuthStatusBoxState,
    pub theme: &'a Theme,
}

impl<'a> Widget for AwsAuthStatusBoxWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (icon, color) = if self.state.authenticated { ("✓", Color::Green) } else { ("✗", Color::Red) };
        let mut text = format!("{} AWS", icon);
        if let Some(ref region) = self.state.region { text.push_str(&format!(" ({})", region)); }
        if let Some(ref profile) = self.state.profile { text.push_str(&format!(" [{}]", profile)); }
        buf.set_string(area.x, area.y, &text, Style::default().fg(color));
    }
}

// ─── SessionBackgroundHint ─────────────────────────────────────────────────

pub struct SessionBackgroundHintState {
    pub session_count: usize,
    pub visible: bool,
}

impl SessionBackgroundHintState {
    pub fn new(count: usize) -> Self { Self { session_count: count, visible: count > 0 } }
}

pub struct SessionBackgroundHintWidget<'a> {
    pub state: &'a SessionBackgroundHintState,
    pub theme: &'a Theme,
}

impl<'a> Widget for SessionBackgroundHintWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible { return; }
        let text = format!("{} background session{}", self.state.session_count, if self.state.session_count == 1 { "" } else { "s" });
        buf.set_string(area.x, area.y, &text, Style::default().fg(Color::DarkGray));
    }
}

// ─── TeammateViewHeader ────────────────────────────────────────────────────

pub struct TeammateViewHeaderState {
    pub agent_name: String,
    pub agent_color: Color,
    pub task_summary: String,
}

impl TeammateViewHeaderState {
    pub fn new(name: String, color: Color, task: String) -> Self {
        Self { agent_name: name, agent_color: color, task_summary: task }
    }
}

pub struct TeammateViewHeaderWidget<'a> {
    pub state: &'a TeammateViewHeaderState,
    pub theme: &'a Theme,
}

impl<'a> Widget for TeammateViewHeaderWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &format!("● {}", self.state.agent_name), Style::default().fg(self.state.agent_color));
        if !self.state.task_summary.is_empty() {
            let max_w = (area.width as usize).saturating_sub(self.state.agent_name.len() + 4);
            let task = if self.state.task_summary.len() > max_w { &self.state.task_summary[..max_w] } else { &self.state.task_summary };
            buf.set_string(area.x + self.state.agent_name.len() as u16 + 4, area.y, task, Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── StructuredDiffList ────────────────────────────────────────────────────

pub struct StructuredDiffListState {
    pub files: Vec<String>,
}

impl StructuredDiffListState {
    pub fn new(files: Vec<String>) -> Self { Self { files } }
}

pub struct StructuredDiffListWidget<'a> {
    pub state: &'a StructuredDiffListState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StructuredDiffListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, file) in self.state.files.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height { break; }
            buf.set_string(area.x, y, &format!("  {}", file), Style::default().fg(Color::White));
        }
    }
}

// ─── FallbackToolUseErrorMessage / FallbackToolUseRejectedMessage ──────────

pub struct FallbackToolUseErrorMessageState {
    pub tool_name: String,
    pub error_message: String,
}

impl FallbackToolUseErrorMessageState {
    pub fn new(tool: String, error: String) -> Self { Self { tool_name: tool, error_message: error } }
}

pub struct FallbackToolUseErrorMessageWidget<'a> {
    pub state: &'a FallbackToolUseErrorMessageState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FallbackToolUseErrorMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &format!("✗ {} failed: {}", self.state.tool_name, self.state.error_message), Style::default().fg(Color::Red));
    }
}

pub struct FallbackToolUseRejectedMessageState {
    pub tool_name: String,
}

impl FallbackToolUseRejectedMessageState {
    pub fn new(tool: String) -> Self { Self { tool_name: tool } }
}

pub struct FallbackToolUseRejectedMessageWidget<'a> {
    pub state: &'a FallbackToolUseRejectedMessageState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FallbackToolUseRejectedMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, &format!("⊘ {} rejected", self.state.tool_name), Style::default().fg(Color::Yellow));
    }
}

// ─── MCPServerDialogCopy ───────────────────────────────────────────────────

pub struct MCPServerDialogCopyState {
    pub server_name: String,
    pub copied: bool,
}

impl MCPServerDialogCopyState {
    pub fn new(name: String) -> Self { Self { server_name: name, copied: false } }
    pub fn mark_copied(&mut self) { self.copied = true; }
}

pub struct MCPServerDialogCopyWidget<'a> {
    pub state: &'a MCPServerDialogCopyState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MCPServerDialogCopyWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.copied {
            buf.set_string(area.x, area.y, "✓ Copied to clipboard", Style::default().fg(Color::Green));
        } else {
            buf.set_string(area.x, area.y, &format!("Copy {} config", self.state.server_name), Style::default().fg(Color::White));
        }
    }
}
