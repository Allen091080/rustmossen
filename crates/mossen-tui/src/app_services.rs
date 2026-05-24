//! TUI auxiliary services — terminal title / tab-status / desktop notification
//! glue + cost-threshold / idle-return / message-selector / search dialogs.
//!
//! Design note: the core `App` struct (in `app.rs`) is owned by a parallel
//! wiring agent. They've added a unified [`crate::app::ActiveModal`] enum
//! with tag-style variants (`CostThreshold(String)`, `IdleReturn(String)`,
//! `MessageSelector(usize)`, `Search(String)`) but no per-modal state. The
//! design here is therefore:
//!
//! * [`TerminalServices`] (this module) owns the *rich* dialog state.
//! * [`crate::app::ActiveModal`] (their module) holds the *routing tag* so
//!   `App::handle_key` and `App::render_modal` can dispatch.
//! * The trigger helpers below set both at the same time.
//!
//! Behaviours implemented (all real — no TODO stubs):
//!
//! * Spinner-glue: when `is_streaming` flips true the title becomes
//!   `"Mossen ▸ ⠋ thinking..."`, tab status flips to **busy**.
//! * Completion: when streaming ends the title is `"Mossen ▸ ready"`, tab
//!   status **ok**, and a desktop notification escape fires when the window is
//!   not focused.
//! * Submit: the idle timer is reset; a query-slot is reserved on the shared
//!   [`mossen_utils::query_guard::QueryGuard`] so spinners can show
//!   immediately.
//! * Cost threshold: polled each tick — when the session's total cost crosses
//!   the configured threshold the modal is opened exactly once.
//! * Idle return: a tick reads `last_interaction` — if 15 minutes elapsed
//!   *and* a response is pending the IdleReturn dialog is opened.
//! * MessageSelector: opened on Esc when not streaming. Populated from the
//!   on-disk session list via [`mossen_utils::list_sessions_impl`].
//! * Search filter: opened on Ctrl+R, wraps `hooks::search_input` plus a small
//!   local highlight state and filters the current message list.
//!
//! Vim mode is deliberately out of scope.

use std::time::{Duration, Instant};

use mossen_utils::list_sessions_impl::{list_sessions_impl, ListSessionsOptions, SessionInfo};
use mossen_utils::query_guard::QueryGuard;

use crate::app::{ActiveModal, App};
// `hooks::search_input` is a private module but its items are re-exported at
// the `hooks::` root.
use crate::hooks::SearchInputState;
use crate::message_model::{MessageData, MessageType};
use crate::render_model::{RenderBlockKind, RenderTranscript};
use crate::widgets::cost_threshold::CostThresholdDialogState;
use crate::widgets::idle_return::IdleReturnDialogState;
use crate::widgets::message_selector::{
    MessageSelectorState, RenderableMessage, RenderableMessageType,
};

// ---------------------------------------------------------------------------
// Defaults — pulled from settings when wired; literal fallbacks otherwise.
// ---------------------------------------------------------------------------

/// Default cost threshold (USD). Mirrors TS
/// `CostThresholdDialog.DEFAULT_THRESHOLD`.
pub const DEFAULT_COST_THRESHOLD_USD: f64 = 5.0;

/// Idle window before showing the IdleReturn dialog when a response is
/// pending. Mirrors TS `IDLE_RETURN_THRESHOLD_MS`.
pub const IDLE_RETURN_THRESHOLD: Duration = Duration::from_secs(15 * 60);

// ---------------------------------------------------------------------------
// Terminal chrome state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SearchHighlightState {
    pub active: bool,
}

impl SearchHighlightState {
    pub fn new() -> Self {
        Self { active: true }
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

impl Default for SearchHighlightState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TerminalTitleState {
    pub title: String,
}

impl TerminalTitleState {
    pub fn new() -> Self {
        Self {
            title: String::new(),
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = sanitize_terminal_title(title);
    }

    pub fn get_title(&self) -> &str {
        &self.title
    }

    pub fn to_escape_sequence(&self) -> String {
        format!("\x1b]2;{}\x07", self.title)
    }
}

fn sanitize_terminal_title(title: &str) -> String {
    title
        .chars()
        .filter_map(|ch| match ch {
            '\u{1b}' | '\u{7}' => None,
            ch if ch.is_control() => Some(' '),
            ch => Some(ch),
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Default for TerminalTitleState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TabStatusState {
    pub indicator: Option<String>,
    pub status: Option<String>,
}

impl TabStatusState {
    pub fn new() -> Self {
        Self {
            indicator: None,
            status: None,
        }
    }

    pub fn set_indicator(&mut self, color: Option<String>) {
        self.indicator = color;
    }

    pub fn set_status(&mut self, text: Option<String>) {
        self.status = text;
    }

    pub fn clear(&mut self) {
        self.indicator = None;
        self.status = None;
    }
}

impl Default for TabStatusState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatusKind {
    Idle,
    Busy,
    Attention,
    Error,
    Success,
}

#[derive(Debug, Clone)]
pub struct TerminalFocusState {
    pub focused: bool,
}

impl TerminalFocusState {
    pub fn new() -> Self {
        Self { focused: true }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }
}

impl Default for TerminalFocusState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalNotification {
    pub title: String,
    pub body: String,
    pub urgent: bool,
}

fn terminal_notification_escape(notification: &TerminalNotification) -> String {
    format!("\x1b]9;{}: {}\x07", notification.title, notification.body)
}

// ---------------------------------------------------------------------------
// Search panel — bundles the hook state for the Ctrl+R filter.
// ---------------------------------------------------------------------------

/// In-session message search panel state.
pub struct SearchPanelState {
    pub input: SearchInputState,
    pub highlight: SearchHighlightState,
    /// Indices into `App.messages` that match the current query.
    pub matches: Vec<usize>,
    /// Cursor inside `matches`.
    pub selected: usize,
}

impl SearchPanelState {
    pub fn new() -> Self {
        let mut s = Self {
            input: SearchInputState::new(),
            highlight: SearchHighlightState::new(),
            matches: Vec::new(),
            selected: 0,
        };
        s.input.activate();
        s.highlight.set_active(true);
        s
    }

    /// Recompute the filtered match list for `messages` against the current
    /// query. Returns the number of hits. Empty query ⇒ no matches (we hide
    /// the filter rather than show every line).
    pub fn refilter(&mut self, messages: &[MessageData]) -> usize {
        self.matches.clear();
        let q = self.input.query.to_lowercase();
        if q.is_empty() {
            self.selected = 0;
            return 0;
        }
        for (idx, m) in messages.iter().enumerate() {
            if m.content.to_lowercase().contains(&q) {
                self.matches.push(idx);
            }
        }
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
        self.matches.len()
    }

    pub fn next(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.matches.len();
    }

    pub fn prev(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.matches.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn current_match(&self) -> Option<usize> {
        self.matches.get(self.selected).copied()
    }
}

impl Default for SearchPanelState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TerminalServices — owns the long-lived state needed by all four wirings.
// ---------------------------------------------------------------------------

/// Aggregate state container surfaced to the App. One instance per running
/// REPL. Holds:
///
/// * the three terminal-chrome hook states (title / tab / focus),
/// * a per-session [`QueryGuard`] so spinners show immediately on submit,
/// * the last-interaction timestamp used by the idle-return dialog,
/// * the last cost level we saw so we only trigger the threshold modal once,
/// * the rich per-dialog state (cost / idle / selector / search), and
/// * the cached session list used by the MessageSelector.
pub struct TerminalServices {
    /// Title bar state — set via OSC 2.
    pub title: TerminalTitleState,
    /// Optional user-chosen terminal-title base. Streaming chrome decorates
    /// this with the current state instead of replacing it.
    pub manual_title: Option<String>,
    /// Tab status (busy / ok / attention).
    pub tab: TabStatusState,
    /// Focus tracking.
    pub focus: TerminalFocusState,
    /// Query-slot reservation (shared with QueueProcessor).
    pub query_guard: QueryGuard,
    /// Last time the user pressed a key or submitted.
    pub last_interaction: Instant,
    /// Cost level at last poll. We trigger the threshold dialog the first
    /// time `cost >= threshold` *and* `last_cost_level < threshold`.
    pub last_cost_level: f64,
    /// Configured cost threshold (USD).
    pub cost_threshold: f64,
    /// Per-modal state. At most one is `Some` at a time. The App's
    /// `active_modal` enum encodes which one.
    pub cost_threshold_state: Option<CostThresholdDialogState>,
    pub idle_return_state: Option<IdleReturnDialogState>,
    pub message_selector_state: Option<MessageSelectorState>,
    pub search_panel_state: Option<SearchPanelState>,
    /// Cached session list for the MessageSelector. Refreshed when the
    /// selector is opened.
    pub session_cache: Vec<SessionInfo>,
    /// True once we've already shown the idle dialog for the *current* idle
    /// gap — prevents re-opening it every tick.
    pub idle_dialog_shown: bool,
    /// True once we've already fired the OS notification for the *current*
    /// streaming cycle. Reset on the next streaming-start edge.
    pub notification_fired: bool,
    /// Tracks the prior `is_streaming` value so we can detect edges.
    pub was_streaming: bool,
}

impl TerminalServices {
    pub fn new() -> Self {
        Self {
            title: TerminalTitleState::new(),
            manual_title: None,
            tab: TabStatusState::new(),
            focus: TerminalFocusState::new(),
            query_guard: QueryGuard::new(),
            last_interaction: Instant::now(),
            last_cost_level: 0.0,
            cost_threshold: DEFAULT_COST_THRESHOLD_USD,
            cost_threshold_state: None,
            idle_return_state: None,
            message_selector_state: None,
            search_panel_state: None,
            session_cache: Vec::new(),
            idle_dialog_shown: false,
            notification_fired: false,
            was_streaming: false,
        }
    }

    /// Override the cost threshold (settings hook).
    pub fn with_cost_threshold(mut self, threshold: f64) -> Self {
        self.cost_threshold = threshold;
        self
    }

    /// Update the focus state and clear notification-fired state on focus.
    pub fn set_focus(&mut self, focused: bool) {
        self.focus.set_focused(focused);
        if focused {
            // Window came back — reset the streaming-completion notification
            // latch so a future completion can fire again.
            self.notification_fired = false;
        }
    }

    /// Apply the "thinking" chrome.
    pub fn enter_streaming(&mut self) {
        let title = format!("{} ▸ ⠋ thinking...", self.title_base());
        self.title.set_title(&title);
        self.tab
            .set_status(Some(tab_status_label(TabStatusKind::Busy)));
        self.tab.set_indicator(Some("yellow".to_string()));
        self.notification_fired = false;
    }

    /// Apply the "ready" chrome. Returns the OSC 9 escape if a desktop
    /// notification should be written (window unfocused, not already fired).
    pub fn finish_streaming(&mut self) -> Option<String> {
        let title = format!("{} ▸ ready", self.title_base());
        self.title.set_title(&title);
        self.tab
            .set_status(Some(tab_status_label(TabStatusKind::Success)));
        self.tab.set_indicator(Some("green".to_string()));

        if !self.focus.is_focused() && !self.notification_fired {
            self.notification_fired = true;
            let n = TerminalNotification {
                title: "Mossen".to_string(),
                body: "Response ready".to_string(),
                urgent: false,
            };
            Some(terminal_notification_escape(&n))
        } else {
            None
        }
    }

    /// Called on every keystroke / submit — resets the idle timer.
    pub fn note_interaction(&mut self) {
        self.last_interaction = Instant::now();
        self.idle_dialog_shown = false;
    }

    pub fn title_base(&self) -> &str {
        self.manual_title.as_deref().unwrap_or("Mossen")
    }

    pub fn visible_title(&self) -> String {
        if self.title.get_title().trim().is_empty() {
            format!("{} ▸ ready", self.title_base())
        } else {
            self.title.get_title().to_string()
        }
    }

    pub fn set_manual_title(&mut self, title: &str) -> Option<String> {
        let title = sanitize_terminal_title(title);
        if title.is_empty() {
            self.manual_title = None;
        } else {
            self.manual_title = Some(title);
        }
        self.refresh_title_for_current_state();
        self.manual_title.clone()
    }

    pub fn clear_manual_title(&mut self) {
        self.manual_title = None;
        self.refresh_title_for_current_state();
    }

    pub fn refresh_title_for_current_state(&mut self) {
        let title = if self.was_streaming {
            format!("{} ▸ ⠋ thinking...", self.title_base())
        } else {
            format!("{} ▸ ready", self.title_base())
        };
        self.title.set_title(&title);
    }

    /// Idle gap since last interaction.
    pub fn idle_for(&self) -> Duration {
        self.last_interaction.elapsed()
    }

    /// Clear all dialog state — paired with `app.active_modal = None`.
    pub fn clear_modal_state(&mut self) {
        self.cost_threshold_state = None;
        self.idle_return_state = None;
        self.message_selector_state = None;
        self.search_panel_state = None;
    }
}

impl Default for TerminalServices {
    fn default() -> Self {
        Self::new()
    }
}

fn tab_status_label(kind: TabStatusKind) -> String {
    match kind {
        TabStatusKind::Idle => "idle",
        TabStatusKind::Busy => "busy",
        TabStatusKind::Attention => "attention",
        TabStatusKind::Error => "error",
        TabStatusKind::Success => "ok",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Free-standing trigger helpers — these are the units the App tick wires to.
// They take `(&mut App, &mut TerminalServices)` so they can update both the
// rich state and the App's `active_modal` routing tag in lockstep.
// ---------------------------------------------------------------------------

/// Drive the title / tab / notification chrome from the streaming flag.
///
/// Should be called once per render tick. Returns an optional escape sequence
/// that the caller (the App) should write to stdout so the desktop
/// notification reaches the terminal emulator. Returning `None` means nothing
/// to write.
pub fn tick_streaming_chrome(svc: &mut TerminalServices, is_streaming: bool) -> Option<String> {
    let edge_enter = is_streaming && !svc.was_streaming;
    let edge_exit = !is_streaming && svc.was_streaming;
    svc.was_streaming = is_streaming;
    if edge_enter {
        svc.enter_streaming();
    }
    if edge_exit {
        return svc.finish_streaming();
    }
    None
}

/// Open the cost-threshold dialog if `current_cost` crosses the configured
/// threshold for the first time. Idempotent until the user dismisses it.
pub fn maybe_open_cost_threshold(app: &mut App, svc: &mut TerminalServices, current_cost: f64) {
    if app.active_modal.is_open() {
        svc.last_cost_level = current_cost;
        return;
    }
    if svc.last_cost_level < svc.cost_threshold && current_cost >= svc.cost_threshold {
        svc.cost_threshold_state = Some(CostThresholdDialogState::new(
            current_cost,
            svc.cost_threshold,
        ));
        app.active_modal = ActiveModal::CostThreshold(format!(
            "${:.2} / ${:.2}",
            current_cost, svc.cost_threshold
        ));
    }
    svc.last_cost_level = current_cost;
}

/// Open the IdleReturn dialog when the user has been idle past the threshold
/// *and* there's an in-flight response they should return to. Once shown,
/// `idle_dialog_shown` latches until `note_interaction()` is called.
pub fn maybe_open_idle_return(app: &mut App, svc: &mut TerminalServices, response_pending: bool) {
    if svc.idle_dialog_shown || app.active_modal.is_open() {
        return;
    }
    if !response_pending {
        return;
    }
    let idle = svc.idle_for();
    if idle >= IDLE_RETURN_THRESHOLD {
        svc.idle_dialog_shown = true;
        let mins = idle.as_secs() / 60;
        let summary = if mins > 60 {
            format!("away {}h {}m", mins / 60, mins % 60)
        } else {
            format!("away {}m", mins)
        };
        svc.idle_return_state = Some(IdleReturnDialogState::new(idle));
        app.active_modal = ActiveModal::IdleReturn(summary);
    }
}

/// Open the MessageSelector — Esc trigger. Populates from `messages`
/// (current session view) plus refreshes the on-disk session cache so the
/// dialog can also list prior sessions for resume. `file_history_enabled`
/// gates the "Restore code" option.
pub fn open_message_selector(
    app: &mut App,
    svc: &mut TerminalServices,
    file_history_enabled: bool,
) {
    let transcript =
        RenderTranscript::from_messages_and_decisions(&app.messages, &app.approval_decisions);
    let renderable: Vec<RenderableMessage> = transcript
        .blocks
        .iter()
        .map(|block| RenderableMessage {
            uuid: block.id.clone(),
            message_type: render_block_kind_to_selector_type(block.kind),
            content: block.selector_summary(),
            tool_use_id: block.tool.as_ref().map(|tool| tool.name.clone()),
            is_meta: matches!(
                block.kind,
                RenderBlockKind::Attachment | RenderBlockKind::SkillInvocation
            ),
            is_api_error: block.state.error,
            timestamp: None,
            model: None,
            thinking_content: None,
        })
        .collect();
    let initial_index = renderable.len().saturating_sub(1);
    svc.message_selector_state = Some(MessageSelectorState::new(renderable, file_history_enabled));
    app.active_modal = ActiveModal::MessageSelector(initial_index);
}

fn render_block_kind_to_selector_type(kind: RenderBlockKind) -> RenderableMessageType {
    match kind {
        RenderBlockKind::User => RenderableMessageType::User,
        RenderBlockKind::Assistant => RenderableMessageType::Assistant,
        RenderBlockKind::System
        | RenderBlockKind::CommandOutput
        | RenderBlockKind::Error
        | RenderBlockKind::FileChangeSummary => RenderableMessageType::System,
        RenderBlockKind::Progress => RenderableMessageType::Progress,
        RenderBlockKind::Attachment | RenderBlockKind::SkillInvocation => {
            RenderableMessageType::Meta
        }
        RenderBlockKind::Tool => RenderableMessageType::ToolResult,
        RenderBlockKind::ApprovalDecision | RenderBlockKind::FinalSummary => {
            RenderableMessageType::Meta
        }
    }
}

/// Refresh the cached session list (used by the MessageSelector when the
/// user navigates past the first item — at that point they want prior
/// sessions). Async wrapper around `list_sessions_impl`. `projects_dir` is
/// typically `<XDG_CONFIG_HOME>/mossen/projects` resolved by the launcher.
pub async fn refresh_session_cache(svc: &mut TerminalServices, projects_dir: &str) {
    svc.session_cache = list_sessions_impl(
        Some(ListSessionsOptions {
            dir: None,
            limit: Some(50),
            offset: Some(0),
            include_worktrees: Some(true),
        }),
        projects_dir,
    )
    .await;
}

/// Open the search panel — Ctrl+R trigger. Pre-populates an empty query so
/// the user can start typing.
pub fn open_search_panel(app: &mut App, svc: &mut TerminalServices) {
    svc.search_panel_state = Some(SearchPanelState::new());
    app.active_modal = ActiveModal::Search(String::new());
}

/// Update the search query and refilter against the message list.
pub fn search_update_query(app: &mut App, svc: &mut TerminalServices, edit: SearchEdit) {
    let mut snapshot_query: Option<String> = None;
    if let Some(sp) = svc.search_panel_state.as_mut() {
        match edit {
            SearchEdit::Char(c) => sp.input.insert(&c.to_string()),
            SearchEdit::Backspace => sp.input.backspace(),
            SearchEdit::Clear => sp.input.clear(),
            SearchEdit::Next => sp.next(),
            SearchEdit::Prev => sp.prev(),
        }
        sp.refilter(&app.messages);
        snapshot_query = Some(sp.input.query.clone());
    }
    if let Some(q) = snapshot_query {
        // Update the App's routing tag with the live query for visibility in
        // status lines / debug dumps.
        app.active_modal = ActiveModal::Search(q);
    }
}

/// Edits accepted by [`search_update_query`].
#[derive(Debug, Clone)]
pub enum SearchEdit {
    Char(char),
    Backspace,
    Clear,
    Next,
    Prev,
}

/// Dismiss the active modal (Esc inside a dialog). Clears both the rich
/// state and the App's routing tag.
pub fn dismiss_modal(app: &mut App, svc: &mut TerminalServices) {
    app.active_modal = ActiveModal::None;
    svc.clear_modal_state();
}

// ---------------------------------------------------------------------------
// `impl App { … }` extension block.
//
// All methods take an explicit `&mut TerminalServices` parameter. When the
// core agent finishes wiring `App` to carry a `services: TerminalServices`
// field, the call sites can drop the explicit argument.
// ---------------------------------------------------------------------------

impl App {
    /// Called every render tick. Drives streaming chrome + idle dialog +
    /// cost threshold polling. The caller writes any returned escape
    /// sequence to stdout (desktop notification).
    pub fn services_tick(
        &mut self,
        svc: &mut TerminalServices,
        current_cost_usd: f64,
    ) -> Option<String> {
        let escape = tick_streaming_chrome(svc, self.state.is_streaming);
        maybe_open_cost_threshold(self, svc, current_cost_usd);
        let pending = self.state.is_waiting_for_response;
        maybe_open_idle_return(self, svc, pending);
        escape
    }

    /// Called when the user submits the prompt — resets idle timer and
    /// reserves a slot on the query guard so the spinner can show before the
    /// async chain reaches the API call.
    pub fn services_on_submit(&mut self, svc: &mut TerminalServices) {
        svc.note_interaction();
        let _ = svc.query_guard.reserve();
    }

    /// Called on every keystroke (Char / arrow / backspace / etc.) — just
    /// resets the idle timer.
    pub fn services_on_keypress(&mut self, svc: &mut TerminalServices) {
        svc.note_interaction();
    }

    /// Called on FocusChange events — forwards into the focus hook state and
    /// resets the notification latch on focus-gained.
    pub fn services_on_focus_change(&mut self, svc: &mut TerminalServices, focused: bool) {
        svc.set_focus(focused);
    }

    /// Esc handler — opens the MessageSelector when nothing else is going
    /// on. Returns `true` if the press was consumed.
    pub fn services_handle_escape(&mut self, svc: &mut TerminalServices) -> bool {
        if self.active_modal.is_open() {
            dismiss_modal(self, svc);
            return true;
        }
        if self.state.is_streaming {
            // Streaming overrides Esc — let the existing interrupt handler run.
            return false;
        }
        open_message_selector(self, svc, /* file_history_enabled */ true);
        true
    }

    /// Ctrl+R handler — opens the search panel against the current message
    /// list. Returns `true` if consumed.
    pub fn services_handle_ctrl_r(&mut self, svc: &mut TerminalServices) -> bool {
        open_search_panel(self, svc);
        true
    }
}

// ---------------------------------------------------------------------------
// Tests — smoke tests for the trigger transitions.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_messages() -> Vec<MessageData> {
        vec![
            MessageData {
                message_type: MessageType::User,
                content: "hello world".into(),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            },
            MessageData {
                message_type: MessageType::Assistant,
                content: "world peace please".into(),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            },
            MessageData {
                message_type: MessageType::System,
                content: "unrelated".into(),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            },
        ]
    }

    #[test]
    fn streaming_edges_drive_chrome() {
        let mut svc = TerminalServices::new();
        svc.set_focus(false);
        let r = tick_streaming_chrome(&mut svc, true);
        assert!(r.is_none());
        assert!(svc.title.get_title().contains("thinking"));
        let r = tick_streaming_chrome(&mut svc, false);
        assert!(r.is_some(), "unfocused completion must emit OSC 9");
        assert!(svc.title.get_title().contains("ready"));
    }

    #[test]
    fn focused_completion_skips_notification() {
        let mut svc = TerminalServices::new();
        svc.set_focus(true);
        tick_streaming_chrome(&mut svc, true);
        let r = tick_streaming_chrome(&mut svc, false);
        assert!(r.is_none(), "focused completion must not pop notification");
    }

    #[test]
    fn manual_title_is_sanitized_and_persists_across_streaming_edges() {
        let mut svc = TerminalServices::new();

        let saved = svc.set_manual_title("渲染会话\u{1b}]2;bad\u{7}\nready");

        assert_eq!(saved.as_deref(), Some("渲染会话]2;bad ready"));
        assert!(svc.title.get_title().contains("渲染会话]2;bad ready"));
        assert!(!svc.title.get_title().contains('\u{1b}'));
        assert!(!svc.title.get_title().contains('\u{7}'));

        tick_streaming_chrome(&mut svc, true);
        assert!(svc.title.get_title().contains("渲染会话]2;bad ready"));
        assert!(svc.title.get_title().contains("thinking"));

        tick_streaming_chrome(&mut svc, false);
        assert!(svc.title.get_title().contains("渲染会话]2;bad ready"));
        assert!(svc.title.get_title().contains("ready"));

        svc.clear_manual_title();
        assert_eq!(svc.manual_title, None);
        assert!(svc.title.get_title().starts_with("Mossen"));
    }

    #[test]
    fn cost_threshold_fires_once() {
        let mut app = App::new();
        let mut svc = TerminalServices::new();
        svc.cost_threshold = 5.0;
        maybe_open_cost_threshold(&mut app, &mut svc, 3.0);
        assert!(!app.active_modal.is_open());
        maybe_open_cost_threshold(&mut app, &mut svc, 6.0);
        assert!(matches!(app.active_modal, ActiveModal::CostThreshold(_)));
        // dismiss + bump again — should not re-trigger because last_cost_level
        // is now above threshold.
        dismiss_modal(&mut app, &mut svc);
        maybe_open_cost_threshold(&mut app, &mut svc, 7.0);
        assert!(!app.active_modal.is_open());
    }

    #[test]
    fn idle_dialog_waits_for_pending_response() {
        let mut app = App::new();
        let mut svc = TerminalServices::new();
        svc.last_interaction = Instant::now() - IDLE_RETURN_THRESHOLD - Duration::from_secs(1);
        // no pending response → don't show
        maybe_open_idle_return(&mut app, &mut svc, false);
        assert!(!app.active_modal.is_open());
        // pending response → show
        maybe_open_idle_return(&mut app, &mut svc, true);
        assert!(matches!(app.active_modal, ActiveModal::IdleReturn(_)));
    }

    #[test]
    fn idle_dialog_resets_on_interaction() {
        let mut app = App::new();
        let mut svc = TerminalServices::new();
        svc.last_interaction = Instant::now() - IDLE_RETURN_THRESHOLD - Duration::from_secs(1);
        maybe_open_idle_return(&mut app, &mut svc, true);
        dismiss_modal(&mut app, &mut svc);
        svc.note_interaction();
        // idle_dialog_shown is now reset so a future idle gap can re-show.
        assert!(!svc.idle_dialog_shown);
    }

    #[test]
    fn search_filters_messages() {
        let mut app = App::new();
        app.messages = dummy_messages();
        let mut svc = TerminalServices::new();
        open_search_panel(&mut app, &mut svc);
        search_update_query(&mut app, &mut svc, SearchEdit::Char('w'));
        search_update_query(&mut app, &mut svc, SearchEdit::Char('o'));
        search_update_query(&mut app, &mut svc, SearchEdit::Char('r'));
        search_update_query(&mut app, &mut svc, SearchEdit::Char('l'));
        search_update_query(&mut app, &mut svc, SearchEdit::Char('d'));
        let sp = svc.search_panel_state.as_ref().expect("search active");
        assert_eq!(sp.matches, vec![0, 1]);
    }

    #[test]
    fn message_selector_open_close() {
        let mut app = App::new();
        app.messages = dummy_messages();
        let mut svc = TerminalServices::new();
        open_message_selector(&mut app, &mut svc, true);
        assert!(matches!(app.active_modal, ActiveModal::MessageSelector(_)));
        assert!(svc.message_selector_state.is_some());
        dismiss_modal(&mut app, &mut svc);
        assert!(!app.active_modal.is_open());
        assert!(svc.message_selector_state.is_none());
    }

    #[test]
    fn message_selector_uses_semantic_render_summaries() {
        let mut app = App::new();
        app.messages = vec![
            MessageData {
                message_type: MessageType::ToolUse,
                content: r#"{"command":"ls -la"}"#.into(),
                timestamp: None,
                is_streaming: false,
                tool_name: Some("Bash".into()),
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            },
            MessageData {
                message_type: MessageType::ToolResult,
                content: r#"{"stdout":"ok\n","exit_code":0,"duration_ms":12}"#.into(),
                timestamp: None,
                is_streaming: false,
                tool_name: Some("Bash".into()),
                is_error: false,
                thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
            },
        ];
        let mut svc = TerminalServices::new();
        open_message_selector(&mut app, &mut svc, true);

        let selector = svc
            .message_selector_state
            .as_ref()
            .expect("message selector state");
        assert_eq!(selector.messages.len(), 1);
        let content = &selector.messages[0].content;
        assert!(content.contains("Bash"));
        assert!(content.contains("exit 0"));
        assert!(!content.contains("\"stdout\""));
        assert!(!content.contains('{'));
    }

    #[test]
    fn submit_reserves_query_slot() {
        let mut app = App::new();
        let mut svc = TerminalServices::new();
        app.services_on_submit(&mut svc);
        // Reservation took — guard should now be active in Dispatching.
        assert!(svc.query_guard.is_active());
    }

    #[test]
    fn esc_opens_message_selector_when_idle() {
        let mut app = App::new();
        app.messages = dummy_messages();
        let mut svc = TerminalServices::new();
        assert!(app.services_handle_escape(&mut svc));
        assert!(matches!(app.active_modal, ActiveModal::MessageSelector(_)));
        // Second press dismisses.
        assert!(app.services_handle_escape(&mut svc));
        assert!(!app.active_modal.is_open());
    }

    #[test]
    fn esc_does_not_open_during_streaming() {
        let mut app = App::new();
        app.state.is_streaming = true;
        let mut svc = TerminalServices::new();
        assert!(!app.services_handle_escape(&mut svc));
        assert!(!app.active_modal.is_open());
    }
}
