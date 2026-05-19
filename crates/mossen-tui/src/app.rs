//! App framework — main TUI application struct and event loop.
//!
//! Translates: App.tsx (top-level wrapper) + replLauncher.tsx (REPL lifecycle)
//! + ink.tsx (render loop) into a Rust App struct with a ratatui render loop.
//!
//! This is the wired-up version: keyboard Enter dispatches into
//! `mossen_agent::engine::submit_prompt`, the `SdkMessage` stream is consumed
//! from `engine_rx`, slash commands route to the `mossen_commands` directive
//! registry, and modal overlays (permission prompts, tool-use confirms) are
//! drawn on top of the main UI when active.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};
use ratatui::widgets::Clear;
use ratatui::{layout::Rect, widgets::Widget, Frame};
use tokio::sync::mpsc;

use crate::components::permissions::{
    AccessGateWidget, PermissionAction, PermissionKind, PermissionPromptState, ToolUseConfirm,
};
use crate::event::{spawn_crossterm_reader, spawn_tick_timer, AppEvent, EventBus, InputAction};
use crate::layout::AppLayout;
use crate::layout::VirtualScroll;
use crate::state::{AppState, AppStore};
use crate::theme::Theme;
use crate::widgets::message::{MessageData, MessageType};
use crate::widgets::messages::MessagesWidget;
use crate::widgets::prompt_input::{PromptInputState, PromptInputWidget, Suggestion, SuggestionKind};
use crate::widgets::spinner::{SpinnerRowWidget, SpinnerState};

use mossen_agent::engine::submit_prompt;
use mossen_agent::types::ContentDelta;
use mossen_agent::types::{OriginTag, PromptParams, SdkMessage, StreamEventData};
use mossen_commands::{find_directive, BoxedDirective, CommandContext, CommandResult};
use mossen_types::{ContentBlock, ToolUseContext};

// ---------------------------------------------------------------------------
// Engine integration config
// ---------------------------------------------------------------------------

/// Static engine configuration the App needs to build `PromptParams` for each
/// turn. Built once by the launcher and threaded through `App::with_engine`.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Default model id (e.g. "MiniMax-M2" in tests).
    pub model: String,
    /// Pre-assembled system prompt blocks. Built once by the launcher
    /// (`mossen_cli::system_prompt::assemble`) and reused across every turn
    /// in the session — `EngineConfig` is meant to be static.
    pub system_prompt: Vec<mossen_agent::types::SystemBlock>,
    /// Working directory for `ToolUseContext`.
    pub cwd: String,
    /// API base URL override.
    pub api_base_url: Option<String>,
    /// API key (read from env in launcher).
    pub api_key: Option<String>,
    /// Origin tag used for telemetry / dialog routing.
    pub origin_tag: OriginTag,
    /// Max conversation turns per dispatch (None = engine default).
    pub max_turns: Option<u32>,
    /// Extra request body fields passed verbatim to the backend.
    pub extra_body: HashMap<String, serde_json::Value>,
    /// Output style selected via `/output-style` picker. `None` = the
    /// composer's default; otherwise the launcher's system-prompt
    /// assembler appends a guidance section that pushes the model
    /// toward the requested style (e.g. "Concise", "Explanatory").
    /// Stored on EngineConfig so the picker can update it live and the
    /// next `handle_submit` reassembles the prompt automatically.
    pub output_style: Option<String>,
}

impl EngineConfig {
    /// Build a sensible default suitable for the TUI: model from env or
    /// hardcoded test value, MiniMax base URL if present, etc.
    pub fn from_env(default_model: &str) -> Self {
        Self {
            model: std::env::var("MOSSEN_MODEL")
                .ok()
                .unwrap_or_else(|| default_model.to_string()),
            system_prompt: Vec::new(),
            cwd: std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| ".".to_string()),
            api_base_url: std::env::var("MOSSEN_API_BASE_URL").ok(),
            api_key: std::env::var("MOSSEN_API_KEY")
                .ok()
                .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
            origin_tag: OriginTag::Repl,
            max_turns: None,
            extra_body: HashMap::new(),
            output_style: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Active modal
// ---------------------------------------------------------------------------

/// Tracks which (if any) modal overlay is currently displayed on top of the
/// REPL surface. Matches the TS REPL.tsx modal-stack semantics: exactly one
/// modal is interactive at a time, and key events are routed to its handler.
pub enum ActiveModal {
    None,
    PermissionRequest(PermissionPromptState),
    ToolUseConfirm {
        confirm: ToolUseConfirm,
        prompt: PermissionPromptState,
    },
    /// Reserved variants — wired into the enum so future work can target
    /// them without changing the App shape. They're currently not produced
    /// by `handle_engine_message` but are routable through the same dispatcher.
    CostThreshold(String),
    IdleReturn(String),
    MessageSelector(usize),
    Search(String),
    /// Generic single-select picker — used by `/theme`, `/output-style`,
    /// etc. The renderer simply walks `items` with `▸` next to `selected`,
    /// and the App reacts to Enter based on `kind`.
    Picker {
        kind: PickerKind,
        title: String,
        items: Vec<String>,
        selected: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum PickerKind {
    Theme,
    OutputStyle,
}

impl ActiveModal {
    pub fn is_open(&self) -> bool {
        !matches!(self, ActiveModal::None)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// The main TUI application.
///
/// Owns all state and orchestrates rendering + event handling.
/// Translates: App.tsx + REPL.tsx + FullscreenLayout.tsx.
pub struct App {
    // --- Core state ---
    pub state: AppState,
    pub store: AppStore,
    pub theme: Theme,

    // --- UI state ---
    pub prompt: PromptInputState,
    pub spinner: SpinnerState,
    pub messages: Vec<MessageData>,
    pub scroll: VirtualScroll,

    // --- Lifecycle ---
    pub started_at: Instant,
    pub should_quit: bool,
    pub fullscreen: bool,

    // --- Engine integration ---
    pub engine_config: EngineConfig,
    pub engine_rx: Option<mpsc::Receiver<SdkMessage>>,
    pub engine_session_id: Option<String>,
    /// Index in `self.messages` of the assistant message currently being
    /// filled by streaming deltas, or `None` when there is no pending turn.
    pub pending_assistant_idx: Option<usize>,
    /// Set by `handle_submit` when a user prompt should be dispatched to
    /// the engine on the next iteration of the main loop. The main loop
    /// consumes this to perform the async `submit_prompt` call.
    pub pending_submit: Option<PromptParams>,
    /// Accumulated text for the in-flight assistant message — used as a
    /// safety net when the engine produces only `StreamEvent` deltas.
    pub assistant_buf: String,

    // --- Modal overlay ---
    pub active_modal: ActiveModal,

    // --- Commands ---
    /// Shared directive list. Optional because some tests construct an
    /// `App` without command support. Stored as `Arc<Vec<BoxedDirective>>`
    /// so the App can be cloned cheaply and so that callers (e.g. the CLI
    /// repl launcher) can build the registry once and share it.
    pub directives: Option<Arc<Vec<BoxedDirective>>>,
    /// Cached `CommandContext` used when invoking directives.
    pub command_context: CommandContext,

    // --- Terminal services (chrome / dialogs / search / message-selector) ---
    /// Auxiliary services: terminal title + tab status + cost/idle/search/
    /// message-selector dialog state. See `app_services.rs`.
    pub services: crate::app_services::TerminalServices,

    /// Shared skill registry — loaded once by the launcher and passed in via
    /// [`with_engine`]. Stored so slash commands, agent-side hooks, and the
    /// engine's tool execution path can look up skills by id/name. The legacy
    /// [`App::new`] constructor leaves this as `None`; production callers
    /// always wire a registry.
    pub skill_registry: Option<mossen_skills::SharedCraftRegistry>,

    /// Running session cost in USD — accumulated from every
    /// `SdkMessage::Result.cost_usd` the engine emits at the end of a turn.
    /// Surfaced to the status bar and the cost-threshold dialog via
    /// `services_tick`.
    pub total_cost_usd: f64,

    /// Receiver end of the channel the engine's `InteractiveGate` posts
    /// `PermissionRequest`s on. The main tick loop pulls from this and, when
    /// no other modal is active, opens a `ToolUseConfirm` overlay populated
    /// from the request; the modal's Allow / Deny / Allow-Always buttons
    /// then drive `active_permission_responder` below.
    pub permission_rx: Option<tokio::sync::mpsc::Receiver<mossen_agent::types::PermissionRequest>>,

    /// One-shot reply channel for the currently-pending permission request.
    /// `Some` only while a `ToolUseConfirm` modal is awaiting user input;
    /// cleared once `Allow` / `Deny` is sent back to the engine.
    pub active_permission_responder:
        Option<tokio::sync::oneshot::Sender<mossen_agent::types::PermissionDecision>>,

    /// Executable tool registry, shared with the agent. The CLI builds this
    /// from `mossen_tools::all_tools()` and injects via
    /// [`App::with_tool_registry`]. When `Some`, `handle_submit` extracts
    /// `ToolDefinition`s for the request body and clones the `Arc` into
    /// `PromptParams::tool_registry` so the dialogue loop can actually
    /// execute the `tool_use` blocks the model emits.
    pub tool_registry: Option<std::sync::Arc<mossen_agent::tool_registry::ToolRegistry>>,

    /// Ctrl+E toggle — when true, every assistant message's thinking
    /// block stays rendered regardless of the 30s auto-fade timer.
    pub show_all_thinking: bool,

    /// Pluggable task-list snapshot provider — set by the launcher so
    /// Ctrl+T can dump the live TaskStore content without forcing
    /// mossen-tui to depend on mossen-tools directly. Each entry is
    /// `(status, id, subject)`.
    pub task_snapshot_provider: Option<std::sync::Arc<dyn Fn() -> Vec<(String, String, String)> + Send + Sync>>,

    /// Indices of `ToolUse` messages whose following `ToolResult` is
    /// currently collapsed (hidden from view). Press Space/Enter while a
    /// `ToolUse` row is focused to toggle. Tool-use blocks are auto-added
    /// here when their stream finishes so the default UX is a tidy log.
    pub collapsed_tool_groups: std::collections::HashSet<usize>,

    /// Index of the message currently receiving keyboard focus, or
    /// `None` when the prompt has focus instead. Up/Down arrows move it
    /// while the prompt is empty and no stream is active.
    pub focused_message_idx: Option<usize>,

    /// Images the user has pasted (Ctrl+V) but not yet submitted. Each
    /// entry is `(mime, base64)`. The prompt input shows a `[Image #N]`
    /// marker for each, and `handle_submit` folds them into the User
    /// message's content as `ContentBlock::Image` blocks so the API
    /// gets the actual bytes (not just the textual marker).
    pub pending_images: Vec<(String, String)>,
}

impl App {
    /// Create a new App instance without engine wiring (legacy path — keeps
    /// existing tests/screens working; will not actually call the model).
    pub fn new() -> Self {
        let state = AppState::default();
        let theme = Theme::for_name(state.theme);
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| ".".to_string());
        let command_context = CommandContext {
            cwd: std::path::PathBuf::from(&cwd),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: std::env::vars().collect(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_time: None,
        };

        Self {
            store: AppStore::new(state.clone()),
            state,
            theme,
            prompt: PromptInputState::new(),
            spinner: SpinnerState::new(),
            messages: Vec::new(),
            scroll: VirtualScroll::new(24),
            started_at: Instant::now(),
            should_quit: false,
            fullscreen: true,
            engine_config: EngineConfig::from_env("MiniMax-M2"),
            engine_rx: None,
            engine_session_id: None,
            pending_assistant_idx: None,
            pending_submit: None,
            assistant_buf: String::new(),
            active_modal: ActiveModal::None,
            directives: None,
            command_context,
            services: crate::app_services::TerminalServices::new(),
            skill_registry: None,
            total_cost_usd: 0.0,
            permission_rx: None,
            active_permission_responder: None,
            tool_registry: None,
            show_all_thinking: false,
            task_snapshot_provider: None,
            collapsed_tool_groups: std::collections::HashSet::new(),
            focused_message_idx: None,
            pending_images: Vec::new(),
        }
    }

    /// Wire a TaskStore snapshot provider so Ctrl+T can dump live tasks.
    pub fn with_task_snapshot_provider(
        mut self,
        provider: std::sync::Arc<dyn Fn() -> Vec<(String, String, String)> + Send + Sync>,
    ) -> Self {
        self.task_snapshot_provider = Some(provider);
        self
    }

    /// Ctrl+T helper — render the current TaskStore snapshot as a
    /// multi-line string. Uses the launcher-injected provider so TUI
    /// stays decoupled from `mossen-tools`.
    fn snapshot_task_list(&self) -> String {
        let Some(provider) = &self.task_snapshot_provider else {
            return "(task provider not wired)".to_string();
        };
        let tasks = provider();
        if tasks.is_empty() {
            return "(no tasks in store)".to_string();
        }
        let mut out = String::from("Task store snapshot:\n");
        for (status, id, subject) in &tasks {
            out.push_str(&format!(" • [{}] {} — {}\n", status, id, subject));
        }
        out
    }

    /// Ctrl+S helper — append the stash payload to a per-user file so it
    /// survives across sessions. Best-effort: any IO failure is silently
    /// swallowed so a missing cache dir doesn't crash the input flow.
    fn save_stash(&self, text: &str) {
        let Some(cache) = dirs::cache_dir() else {
            return;
        };
        let dir = cache.join("mossen");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("stash.txt");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(
                f,
                "---\n# {}\n{}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                text
            );
        }
    }

    /// Move keyboard focus across the message list. Steps are signed
    /// (+1 down / -1 up). Wraps to the first/last *visible* message —
    /// collapsed `ToolResult` rows are skipped because they're not
    /// rendered. Empty message list → no-op.
    fn move_focus(&mut self, delta: i32) {
        let n = self.messages.len();
        if n == 0 {
            return;
        }
        let mut idx = self.focused_message_idx.unwrap_or(n - 1) as i32;
        let mut tries = n as i32 * 2;
        loop {
            idx = (idx + delta).rem_euclid(n as i32);
            tries -= 1;
            if tries <= 0 {
                break;
            }
            if self.row_visible(idx as usize) {
                break;
            }
        }
        self.focused_message_idx = Some(idx as usize);
    }

    /// True when row `i` would be rendered — i.e. it isn't a
    /// `ToolResult` whose preceding `ToolUse` is in `collapsed_tool_groups`.
    fn row_visible(&self, i: usize) -> bool {
        let Some(msg) = self.messages.get(i) else {
            return false;
        };
        if matches!(msg.message_type, MessageType::ToolResult) {
            if i > 0
                && matches!(self.messages[i - 1].message_type, MessageType::ToolUse)
                && self.collapsed_tool_groups.contains(&(i - 1))
            {
                return false;
            }
        }
        true
    }

    /// Toggle the collapsed state of the currently-focused ToolUse row.
    /// Returns `true` when the press was consumed (caller short-circuits
    /// further key dispatch). When the focused row isn't a ToolUse the
    /// press falls through to the prompt.
    fn toggle_focused_group(&mut self) -> bool {
        let Some(idx) = self.focused_message_idx else {
            return false;
        };
        let Some(msg) = self.messages.get(idx) else {
            return false;
        };
        if !matches!(msg.message_type, MessageType::ToolUse) {
            return false;
        }
        if !self.collapsed_tool_groups.remove(&idx) {
            self.collapsed_tool_groups.insert(idx);
        }
        true
    }

    /// Toggle the `expanded` flag on the focused ToolResult row.
    /// `to_expanded` carries the desired state — Right key expands, Left
    /// collapses. Returns `true` when consumed.
    fn toggle_focused_expand(&mut self, to_expanded: bool) -> bool {
        let Some(idx) = self.focused_message_idx else {
            return false;
        };
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
        if !matches!(msg.message_type, MessageType::ToolResult) {
            return false;
        }
        if msg.full_content.is_none() {
            return false;
        }
        msg.expanded = to_expanded;
        true
    }

    /// Read an image off the OS clipboard, base64-encode it, store as a
    /// pending paste, and insert a `[Image #N]` marker in the prompt.
    /// Returns `true` when an image was found and queued. The actual
    /// bytes ride along in `pending_images`; submission folds them into
    /// `ContentBlock::Image` so the model gets the multimodal payload,
    /// not just a textual marker.
    fn try_paste_image(&mut self) -> bool {
        let bytes = read_clipboard_image_bytes();
        let Some(bytes) = bytes else {
            return false;
        };
        // base64 encode for the data URI the API serializer expects.
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        self.pending_images.push(("image/png".to_string(), encoded));
        let n = self.pending_images.len();
        self.prompt.input.insert_str(&format!("[Image #{}]", n));
        true
    }

    /// Ctrl+G helper — spawn `$EDITOR` (falls back to `vi`) on a temp file
    /// seeded with current prompt input. When the editor exits cleanly,
    /// replace the prompt with the edited content. Raw mode is suspended
    /// for the duration so vim/nano work normally.
    fn spawn_external_editor(&mut self) {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let tmp = std::env::temp_dir().join(format!("mossen-prompt-{}.txt", std::process::id()));
        if std::fs::write(&tmp, self.prompt.input.value.as_bytes()).is_err() {
            return;
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen
        );
        let status = std::process::Command::new(&editor)
            .arg(&tmp)
            .status();
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen
        );
        if status.map(|s| s.success()).unwrap_or(false) {
            if let Ok(text) = std::fs::read_to_string(&tmp) {
                self.prompt.input.clear();
                self.prompt.input.insert_str(text.trim_end());
            }
        }
        let _ = std::fs::remove_file(&tmp);
    }

    /// Attach an executable tool registry built by the launcher. Without
    /// this the engine has no tools to call and the model falls back to
    /// describing actions as plain text.
    pub fn with_tool_registry(
        mut self,
        registry: std::sync::Arc<mossen_agent::tool_registry::ToolRegistry>,
    ) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// Create a new App with engine + directive registry wired up. This is
    /// the production path: launcher builds an `EngineConfig` + the global
    /// `DirectiveRegistry` and passes them in here.
    pub fn with_engine(
        engine_config: EngineConfig,
        directives: Arc<Vec<BoxedDirective>>,
    ) -> Self {
        let mut app = Self::new();
        app.engine_config = engine_config;
        app.directives = Some(directives);
        app
    }

    /// Inject a pre-built skill registry. Separated from [`with_engine`] so the
    /// caller can construct the registry lazily (e.g. discover skills on disk
    /// off the hot path) and attach it once ready.
    pub fn with_skill_registry(
        mut self,
        registry: mossen_skills::SharedCraftRegistry,
    ) -> Self {
        self.skill_registry = Some(registry);
        self
    }

    /// Run the main event loop.
    ///
    /// This is the Rust equivalent of Ink's render loop:
    /// ```text
    /// loop {
    ///   terminal.draw(|f| app.render(f));
    ///   match event { ... }
    /// }
    /// ```
    pub async fn run(
        &mut self,
        mut terminal: ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> anyhow::Result<()> {
        let mut event_bus = EventBus::new();
        let tx = event_bus.sender();

        // Spawn event readers
        spawn_crossterm_reader(tx.clone());
        spawn_tick_timer(tx, 33); // ~30fps

        // Main loop
        while !self.should_quit {
            // If there's a queued submit, fire it off (async) before drawing.
            if let Some(params) = self.pending_submit.take() {
                let rx = submit_prompt(params).await;
                self.engine_rx = Some(rx);
            }

            // Draw
            terminal.draw(|frame| {
                self.render_frame(frame);
            })?;

            // Wait for either an input event or an engine message. We pull
            // the receiver out of `self` for the select to satisfy the
            // borrow checker, then put it back if it's still live.
            let engine_rx = self.engine_rx.take();
            match engine_rx {
                Some(mut rx) => {
                    tokio::select! {
                        biased;
                        ev = event_bus.recv() => {
                            // Put receiver back; engine is still streaming.
                            self.engine_rx = Some(rx);
                            if let Some(event) = ev {
                                self.handle_event(event);
                            }
                        }
                        msg = rx.recv() => {
                            match msg {
                                Some(m) => {
                                    self.engine_rx = Some(rx);
                                    self.handle_engine_message(m);
                                }
                                None => {
                                    // Channel closed — finalize stream.
                                    self.finalize_assistant_turn(None);
                                }
                            }
                        }
                    }
                }
                None => {
                    if let Some(event) = event_bus.recv().await {
                        self.handle_event(event);
                    }
                }
            }
        }

        Ok(())
    }

    /// Render a single frame.
    fn render_frame(&self, frame: &mut Frame) {
        let area = frame.area();

        if self.fullscreen {
            self.render_fullscreen(frame, area);
        } else {
            self.render_inline(frame, area);
        }

        // Modal overlay drawn last so it stacks above the REPL surface.
        self.render_modal(frame, area);
    }

    /// Fullscreen layout rendering.
    fn render_fullscreen(&self, frame: &mut Frame, area: Rect) {
        let header_height = self.sticky_prompt_header_height();
        let prompt_height = PromptInputWidget::new(&self.prompt, &self.theme).required_height();
        // Bottom area: prompt + 1-line spinner (when streaming) + 1-line status bar + 1-line hint bar.
        // Hint bar carries `? for shortcuts · ↵ send · ↑↓ history` — TS REPL has a similar footer.
        let bottom_height =
            prompt_height + if self.state.is_streaming { 1 } else { 0 } + 2; // status + hint

        let layout = AppLayout::fullscreen(area, header_height, bottom_height);

        // Messages area — or Welcome placeholder when empty.
        if self.messages.is_empty() {
            self.render_welcome(frame, layout.content);
        } else {
            let messages_widget = MessagesWidget::new(&self.messages, &self.theme, &self.scroll)
                .show_all_thinking(self.show_all_thinking)
                .collapsed_tool_groups(&self.collapsed_tool_groups)
                .focused_idx(self.focused_message_idx);
            frame.render_widget(messages_widget, layout.content);
        }

        // Bottom area: spinner + prompt + status bar
        let mut y = layout.bottom.y;
        let mut remaining = layout.bottom.height;
        if self.state.is_streaming && remaining > 0 {
            let spinner_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            // Build a live "Thinking… Xs" status string so users see the
            // wait time tick up — beats a static glyph for slow backends.
            let elapsed = self.spinner.elapsed().as_secs();
            let status_text = format!("Thinking… {}s", elapsed);
            let spinner_widget = SpinnerRowWidget::new(&self.spinner, &status_text);
            frame.render_widget(spinner_widget, spinner_area);
            y = y.saturating_add(1);
            remaining = remaining.saturating_sub(1);
        }
        // Status + hint rows sit *above* the prompt so the input row always
        // anchors to the bottom — matches the TS REPL's footer placement.
        if remaining > 2 {
            let prompt_h = remaining.saturating_sub(2);
            let prompt_area = Rect::new(layout.bottom.x, y, layout.bottom.width, prompt_h);
            let prompt_widget = PromptInputWidget::new(&self.prompt, &self.theme);
            frame.render_widget(prompt_widget, prompt_area);
            let status_area =
                Rect::new(layout.bottom.x, y + prompt_h, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area);
            let hint_area =
                Rect::new(layout.bottom.x, y + prompt_h + 1, layout.bottom.width, 1);
            self.render_hint_bar(frame, hint_area);
        } else if remaining == 2 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area);
            let hint_area = Rect::new(layout.bottom.x, y + 1, layout.bottom.width, 1);
            self.render_hint_bar(frame, hint_area);
        } else if remaining == 1 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area);
        }
    }

    /// Inline (non-fullscreen) layout rendering.
    fn render_inline(&self, frame: &mut Frame, area: Rect) {
        let prompt_height = 2u16;
        // Inline layout reserves prompt_height for the input row + 1 row for
        // the status bar footer. Spinner is rendered above the prompt when
        // a turn is in flight.
        let bottom_height = prompt_height + if self.state.is_streaming { 1 } else { 0 } + 1;
        let layout = AppLayout::inline(area, bottom_height);

        let messages_widget = MessagesWidget::new(&self.messages, &self.theme, &self.scroll)
            .show_all_thinking(self.show_all_thinking)
            .collapsed_tool_groups(&self.collapsed_tool_groups)
            .focused_idx(self.focused_message_idx);
        frame.render_widget(messages_widget, layout.content);

        let mut y = layout.bottom.y;
        let mut remaining = layout.bottom.height;
        if self.state.is_streaming && remaining > 0 {
            let spinner_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            // Build a live "Thinking… Xs" status string so users see the
            // wait time tick up — beats a static glyph for slow backends.
            let elapsed = self.spinner.elapsed().as_secs();
            let status_text = format!("Thinking… {}s", elapsed);
            let spinner_widget = SpinnerRowWidget::new(&self.spinner, &status_text);
            frame.render_widget(spinner_widget, spinner_area);
            y = y.saturating_add(1);
            remaining = remaining.saturating_sub(1);
        }
        if remaining > 1 {
            let prompt_h = remaining.saturating_sub(1);
            let prompt_area = Rect::new(layout.bottom.x, y, layout.bottom.width, prompt_h);
            let prompt_widget = PromptInputWidget::new(&self.prompt, &self.theme);
            frame.render_widget(prompt_widget, prompt_area);
            let status_area =
                Rect::new(layout.bottom.x, y + prompt_h, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area);
        } else if remaining == 1 {
            let status_area = Rect::new(layout.bottom.x, y, layout.bottom.width, 1);
            self.render_status_bar(frame, status_area);
        }
    }

    /// Compose a `StatusBarState` from live app state and hand it to the
    /// already-existing `StatusBarWidget`. Keeps state assembly out of the
    /// render hot path while making sure every relevant value (model, cost,
    /// permission mode, message count, fast mode) actually reaches the
    /// footer. Matches the TS `StatusNotices` footer layout.
    /// Welcome screen — drawn when `messages` is empty. Mirrors the TS
    /// REPL splash: an ASCII tear-drop glyph, the cwd, the active model,
    /// and a short hint that nudges the user to type. Stays out of the
    /// way as soon as the first turn lands.
    fn render_welcome(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;
        if area.height < 4 || area.width < 20 {
            return;
        }
        let cwd = &self.engine_config.cwd;
        // Shorten home path so the line doesn't dominate the screen.
        let display_cwd = if let Some(home) = dirs::home_dir() {
            let h = home.to_string_lossy().to_string();
            if cwd.starts_with(&h) {
                format!("~{}", &cwd[h.len()..])
            } else {
                cwd.clone()
            }
        } else {
            cwd.clone()
        };
        let model = &self.engine_config.model;
        let accent = Style::default()
            .fg(self.theme.success)
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.theme.text_dim);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ✻  ", accent),
                Span::styled("Welcome to Mossen", accent),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("     cwd  ", dim),
                Span::raw(display_cwd),
            ]),
            Line::from(vec![
                Span::styled("    model ", dim),
                Span::raw(model.clone()),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "     Type your question to begin.  /help for commands.",
                dim,
            )]),
        ];
        let p = Paragraph::new(lines);
        let centered_y = area.y + area.height / 3;
        let inner = Rect::new(area.x, centered_y, area.width, area.height - area.height / 3);
        frame.render_widget(p, inner);
    }

    /// Bottom hint bar — a single dim line listing the canonical
    /// keystrokes the prompt understands. Renders below the status bar
    /// so the user sees both cwd/model/cost (status) and shortcuts (hint)
    /// without taking either's slot.
    fn render_hint_bar(&self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;
        if area.height == 0 || area.width < 10 {
            return;
        }
        let dim = Style::default()
            .fg(self.theme.text_subtle)
            .add_modifier(Modifier::DIM);
        let sep = Span::styled("  ·  ", dim);
        let line = Line::from(vec![
            Span::styled("  ↵ send", dim),
            sep.clone(),
            Span::styled("↑↓ history", dim),
            sep.clone(),
            Span::styled("Esc selector", dim),
            sep.clone(),
            Span::styled("Ctrl+T tasks", dim),
            sep.clone(),
            Span::styled("Ctrl+E ⇄ think", dim),
            sep.clone(),
            Span::styled("Ctrl+G editor", dim),
            sep,
            Span::styled("/help", dim),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        use crate::components::misc::{StatusBarState, StatusBarWidget};
        let mode = self
            .command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .cloned()
            .unwrap_or_else(|| "Supervised".to_string());
        let state = StatusBarState {
            model: Some(self.engine_config.model.clone()),
            access_policy: mode,
            fast_mode: self.engine_config.extra_body.contains_key("fast_mode"),
            thinking: self.state.is_streaming,
            message_count: self.messages.len(),
            cost: if self.total_cost_usd > 0.0 {
                Some(self.total_cost_usd)
            } else {
                None
            },
            left_text: Some(self.engine_config.cwd.clone()),
        };
        let widget = StatusBarWidget::new(&state, &self.theme);
        frame.render_widget(widget, area);
    }

    /// Render the active modal overlay (if any).
    fn render_modal(&self, frame: &mut Frame, area: Rect) {
        match &self.active_modal {
            ActiveModal::None => {}
            ActiveModal::PermissionRequest(state) => {
                let width = 60u16.min(area.width.saturating_sub(4));
                let height = if state.show_details { 12u16 } else { 9u16 };
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let widget = AccessGateWidget::new(state, &self.theme);
                frame.render_widget(widget, modal_area);
            }
            ActiveModal::ToolUseConfirm { prompt, .. } => {
                let width = 70u16.min(area.width.saturating_sub(4));
                let height = if prompt.show_details { 14u16 } else { 10u16 };
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let widget = AccessGateWidget::new(prompt, &self.theme);
                frame.render_widget(widget, modal_area);
            }
            ActiveModal::CostThreshold(_) => {
                if let Some(state) = &self.services.cost_threshold_state {
                    let width = 56u16.min(area.width.saturating_sub(4));
                    let height = 10u16;
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::components::dialogs::CostThresholdDialogWidget::new(
                        state, &self.theme,
                    );
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::IdleReturn(_) => {
                if let Some(state) = &self.services.idle_return_state {
                    let width = 60u16.min(area.width.saturating_sub(4));
                    let height = 6u16;
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::components::root_medium::IdleReturnDialogWidget {
                        state,
                        theme: &self.theme,
                    };
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::MessageSelector(_) => {
                if let Some(state) = &self.services.message_selector_state {
                    let width = (area.width.saturating_sub(4)).min(80);
                    let height = (area.height.saturating_sub(4)).min(20);
                    let modal_area = crate::layout::center(area, width, height);
                    frame.render_widget(Clear, modal_area);
                    let widget = crate::components::root_large::MessageSelectorWidget {
                        state,
                        theme: &self.theme,
                    };
                    frame.render_widget(widget, modal_area);
                }
            }
            ActiveModal::Search(query) => {
                use ratatui::style::{Color, Style};
                use ratatui::widgets::{Block, Borders, Paragraph};
                let width = (area.width.saturating_sub(4)).min(70);
                let height = 14u16;
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let block = Block::default().title(" Search ").borders(Borders::ALL);
                let inner = block.inner(modal_area);
                frame.render_widget(block, modal_area);
                // Query line
                let q_line = format!("> {}", query);
                frame.render_widget(
                    Paragraph::new(q_line).style(Style::default().fg(Color::Cyan)),
                    Rect::new(inner.x, inner.y, inner.width, 1),
                );
                // Matches preview from search_panel_state
                if let Some(panel) = &self.services.search_panel_state {
                    let mut y = inner.y + 2;
                    let max_y = inner.y + inner.height;
                    for (i, &msg_idx) in panel.matches.iter().take(8).enumerate() {
                        if y >= max_y {
                            break;
                        }
                        let prefix = if i == panel.selected { "▸ " } else { "  " };
                        let preview = self
                            .messages
                            .get(msg_idx)
                            .map(|m| m.content.lines().next().unwrap_or("").to_string())
                            .unwrap_or_default();
                        let line = format!("{}{}", prefix, preview);
                        frame.render_widget(
                            Paragraph::new(line).style(Style::default().fg(Color::White)),
                            Rect::new(inner.x, y, inner.width, 1),
                        );
                        y += 1;
                    }
                }
            }
            ActiveModal::Picker {
                title,
                items,
                selected,
                ..
            } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear, Paragraph};
                let width = 50u16.min(area.width.saturating_sub(4));
                let height =
                    (items.len() as u16 + 2).min(area.height.saturating_sub(4)).max(4);
                let modal_area = crate::layout::center(area, width, height);
                frame.render_widget(Clear, modal_area);
                let block = Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.theme.border_focused));
                let inner = block.inner(modal_area);
                frame.render_widget(block, modal_area);
                for (i, label) in items.iter().enumerate() {
                    if i as u16 >= inner.height {
                        break;
                    }
                    let style = if i == *selected {
                        Style::default()
                            .fg(self.theme.background)
                            .bg(self.theme.border_focused)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.text)
                    };
                    let prefix = if i == *selected { "▸ " } else { "  " };
                    frame.render_widget(
                        Paragraph::new(format!("{}{}", prefix, label)).style(style),
                        Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                    );
                }
            }
        }
    }

    /// Handle an application event.
    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Mouse(mouse) => self.handle_mouse(mouse),
            AppEvent::Resize { width, height } => {
                self.state.terminal_width = width;
                self.state.terminal_height = height;
                self.scroll.set_viewport_height(height.saturating_sub(4));
            }
            AppEvent::Tick => {
                // Tick animations
                self.spinner.frame_index(4); // keep animation running
                // Drive terminal services (chrome on streaming edges, idle dialog,
                // cost threshold). The services_* methods take a separate
                // `&mut TerminalServices` arg, so we temporarily move it out via
                // mem::take to satisfy the borrow checker.
                let mut svc = std::mem::take(&mut self.services);
                // Pulls the live total cost the engine reports via
                // `SdkMessage::Result.cost_usd` — see `handle_engine_message`.
                let current_cost = self.total_cost_usd;
                let _escape = self.services_tick(&mut svc, current_cost);
                self.services = svc;

                // Drain a pending `PermissionRequest` off the gate channel
                // when no other modal is up. The gate stays blocked on the
                // oneshot until the user clicks Allow / Deny in the modal,
                // so we only ever surface one request at a time.
                self.poll_permission_request();
            }
            AppEvent::FocusChange(focused) => {
                let mut svc = std::mem::take(&mut self.services);
                self.services_on_focus_change(&mut svc, focused);
                self.services = svc;
            }
            AppEvent::Quit => {
                self.should_quit = true;
            }
        }
    }

    /// Handle keyboard input.
    /// Public for integration tests — drives a single key event end-to-end
    /// through the same dispatcher the event loop uses.
    pub fn dispatch_key_for_test(&mut self, key: KeyEvent) {
        self.handle_key(key);
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // If a modal is active, route the key to its handler first.
        if self.active_modal.is_open() {
            self.handle_modal_key(key);
            return;
        }

        // Ctrl+R → open search panel (out-of-band, not part of InputAction).
        use crossterm::event::{KeyCode, KeyModifiers};
        if let KeyCode::Char('r') = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let mut svc = std::mem::take(&mut self.services);
                let _ = self.services_handle_ctrl_r(&mut svc);
                self.services = svc;
                return;
            }
        }

        // Ctrl+V → read an image off the system clipboard, store in
        // `pending_images`, and insert a `[Image #N]` marker into the
        // prompt. macOS-first: uses `osascript`/`pbpaste -Prefer 'png'`
        // semantics; Linux gets `xclip -selection clipboard -t image/png
        // -o`. If no image is on the clipboard we fall through to plain
        // text paste (which the prompt's normal char handler covers).
        if let KeyCode::Char('v') = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if self.try_paste_image() {
                    return;
                }
                // Fall-through: no image on clipboard → let prompt
                // handle Ctrl+V (which does nothing today, but at least
                // doesn't trigger the picker / Ctrl+E branches).
                return;
            }
        }

        // Esc → open MessageSelector (TS REPL.tsx Esc handler). The
        // selector lets the user pick a prior message / session to resume,
        // and is the canonical "back out" gesture when no modal is active.
        if matches!(key.code, KeyCode::Esc) {
            // If focus is on a message, clear focus first (one-step
            // escape — pressing Esc twice opens the selector).
            if self.focused_message_idx.take().is_some() {
                return;
            }
            let mut svc = std::mem::take(&mut self.services);
            let _consumed = self.services_handle_escape(&mut svc);
            self.services = svc;
            return;
        }

        // ── Message focus / expand-collapse interactions ─────────────
        // Only active when the prompt is empty and no stream is in
        // progress — otherwise the keys belong to the prompt input.
        let prompt_empty = self.prompt.input.value.is_empty();
        let idle = !self.state.is_streaming;
        if prompt_empty && idle {
            match key.code {
                KeyCode::Up => {
                    self.move_focus(-1);
                    return;
                }
                KeyCode::Down => {
                    self.move_focus(1);
                    return;
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if self.toggle_focused_group() {
                        return;
                    }
                }
                KeyCode::Right => {
                    if self.toggle_focused_expand(true) {
                        return;
                    }
                }
                KeyCode::Left => {
                    if self.toggle_focused_expand(false) {
                        return;
                    }
                }
                _ => {}
            }
        }

        // ── Ctrl+E/G/L/T/S — the five shortcuts the TS keybindings.ts
        //    surfaces by default. Implemented here at the source so we
        //    don't reroute through the full keybinding-context machinery
        //    (which is overkill for the fixed default set).
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    // Ctrl+L → force redraw. Ratatui repaints on every
                    // tick already, but pressing Ctrl+L should also reset
                    // the virtual scroll to the tail so a user pinned
                    // mid-history can re-anchor on the latest message.
                    'l' => {
                        self.scroll.scroll_to_bottom();
                        return;
                    }
                    // Ctrl+T → dump current TaskStore snapshot into the
                    // message stream so the user can see live todo state
                    // without typing /tasks. Lists subject + status.
                    't' => {
                        let lines = self.snapshot_task_list();
                        self.messages.push(MessageData {
                            message_type: MessageType::System,
                            content: lines,
                            timestamp: None,
                            is_streaming: false,
                            tool_name: None,
                            is_error: false,
                            thinking: None,
                        thinking_completed_at: None,
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                        return;
                    }
                    // Ctrl+S → stash the current prompt input to
                    // `~/.cache/mossen/stash.txt` and clear the input.
                    // Survives across sessions; `Ctrl+G` (editor) can pull
                    // it back.
                    's' => {
                        let text = self.prompt.input.value.clone();
                        if !text.is_empty() {
                            self.save_stash(&text);
                            self.prompt.input.clear();
                            self.messages.push(MessageData {
                                message_type: MessageType::System,
                                content: format!("(stashed {} chars)", text.len()),
                                timestamp: None,
                                is_streaming: false,
                                tool_name: None,
                                is_error: false,
                                thinking: None,
                            thinking_completed_at: None,
                            full_content: None,
                            expanded: false,
                            });
                            self.scroll.set_total_items(self.messages.len());
                        }
                        return;
                    }
                    // Ctrl+E → toggle "show all thinking" — when on,
                    // every assistant message's thinking block stays
                    // visible regardless of the 30s fade timer.
                    'e' => {
                        self.show_all_thinking = !self.show_all_thinking;
                        return;
                    }
                    // Ctrl+G → launch $EDITOR on a temp file seeded with
                    // current prompt input; replace the prompt with the
                    // edited content when the editor exits. Crossterm's
                    // raw mode is suspended for the duration so the
                    // editor (vim/nvim/nano) has a normal terminal.
                    'g' => {
                        self.spawn_external_editor();
                        return;
                    }
                    _ => {}
                }
            }
        }

        if let Some(action) = InputAction::from_key_event(&key) {
            match action {
                InputAction::Interrupt => {
                    if self.state.is_streaming {
                        // Cancel current request: drop the receiver so the
                        // background dialogue task gets EOF on its sender
                        // path, then reset streaming state.
                        self.engine_rx = None;
                        self.pending_assistant_idx = None;
                        self.assistant_buf.clear();
                        self.state.is_streaming = false;
                        self.state.is_waiting_for_response = false;
                        self.messages.push(MessageData {
                            message_type: MessageType::System,
                            content: "(interrupted)".to_string(),
                            timestamp: None,
                            is_streaming: false,
                            tool_name: None,
                            is_error: false,
                            thinking: None,
                        thinking_completed_at: None,
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                    } else {
                        self.should_quit = true;
                    }
                }
                InputAction::Eof => {
                    if self.prompt.input.value.is_empty() {
                        self.should_quit = true;
                    }
                }
                InputAction::Submit => {
                    if let Some(input) = self.prompt.submit() {
                        let mut svc = std::mem::take(&mut self.services);
                        self.services_on_submit(&mut svc);
                        self.services = svc;
                        self.handle_submit(input);
                    }
                }
                InputAction::Char(c) => {
                    self.prompt.input.insert_char(c);
                    self.update_suggestions();
                }
                InputAction::Backspace => {
                    self.prompt.input.delete_backward();
                    self.update_suggestions();
                }
                InputAction::Delete => {
                    self.prompt.input.delete_forward();
                }
                InputAction::Left => self.prompt.input.move_left(),
                InputAction::Right => self.prompt.input.move_right(),
                InputAction::Home => self.prompt.input.move_home(),
                InputAction::End => self.prompt.input.move_end(),
                InputAction::Up => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_up();
                    } else {
                        self.prompt.input.history_up();
                    }
                }
                InputAction::Down => {
                    if self.prompt.show_suggestions {
                        self.prompt.suggestion_down();
                    } else {
                        self.prompt.input.history_down();
                    }
                }
                InputAction::Tab => {
                    if self.prompt.show_suggestions {
                        self.prompt.accept_suggestion();
                    }
                }
                InputAction::Escape => {
                    if self.prompt.show_suggestions {
                        self.prompt.show_suggestions = false;
                    } else if self.prompt.show_help {
                        self.prompt.show_help = false;
                    } else {
                        // Esc → message-selector (when not streaming) or dismiss
                        // an open modal. Routed through TerminalServices.
                        let mut svc = std::mem::take(&mut self.services);
                        let _consumed = self.services_handle_escape(&mut svc);
                        self.services = svc;
                    }
                }
                InputAction::PageUp => {
                    self.scroll.scroll_up(10);
                }
                InputAction::PageDown => {
                    self.scroll.scroll_down(10);
                }
                InputAction::Paste(text) => {
                    self.prompt.input.insert_str(&text);
                }
            }
        }
    }

    /// Route keys to the active modal.
    fn handle_modal_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode;

        // Map the key into a coarse modal verb first.
        let action = InputAction::from_key_event(&key);

        match &mut self.active_modal {
            ActiveModal::PermissionRequest(state) => {
                match action {
                    Some(InputAction::Tab) | Some(InputAction::Right) | Some(InputAction::Down) => {
                        state.cycle_action();
                    }
                    Some(InputAction::Submit) => {
                        state.confirm();
                        let decision = state.selected_action;
                        // Acknowledge in the message stream.
                        let msg_text = match decision {
                            PermissionAction::Allow => {
                                format!("Permission allowed for: {}", state.tool_name)
                            }
                            PermissionAction::AllowAlways => {
                                format!("Permission allowed (always) for: {}", state.tool_name)
                            }
                            PermissionAction::Deny => {
                                format!("Permission denied for: {}", state.tool_name)
                            }
                        };
                        let is_error = matches!(decision, PermissionAction::Deny);
                        self.messages.push(MessageData {
                            message_type: MessageType::System,
                            content: msg_text,
                            timestamp: None,
                            is_streaming: false,
                            tool_name: None,
                            is_error,
                            thinking: None,
                        thinking_completed_at: None,
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                        self.active_modal = ActiveModal::None;
                    }
                    Some(InputAction::Escape) | Some(InputAction::Interrupt) => {
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            ActiveModal::ToolUseConfirm { confirm, prompt } => {
                match action {
                    Some(InputAction::Tab) | Some(InputAction::Right) | Some(InputAction::Down) => {
                        prompt.cycle_action();
                    }
                    Some(InputAction::Submit) => {
                        prompt.confirm();
                        let decision = prompt.selected_action;
                        let tool_name = confirm.tool_name.clone();
                        let approved = !matches!(decision, PermissionAction::Deny);
                        self.messages.push(MessageData {
                            message_type: MessageType::System,
                            content: format!(
                                "Tool use {} for: {}",
                                if approved { "approved" } else { "denied" },
                                tool_name
                            ),
                            timestamp: None,
                            is_streaming: false,
                            tool_name: Some(tool_name),
                            is_error: !approved,
                            thinking: None,
                        thinking_completed_at: None,
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                        // Round-trip the decision back into the engine via
                        // the gate's oneshot reply channel. Mapping:
                        //   UI Allow / AllowAlways / Deny
                        //     → engine PermissionDecision::{Allow,
                        //                                   AllowAlways,
                        //                                   Deny}
                        // The engine's `InteractiveGate::check` is awaiting
                        // exactly this send.
                        if let Some(responder) = self.active_permission_responder.take() {
                            let engine_decision = match decision {
                                PermissionAction::Allow => {
                                    mossen_agent::types::PermissionDecision::Allow
                                }
                                PermissionAction::AllowAlways => {
                                    mossen_agent::types::PermissionDecision::AllowAlways
                                }
                                PermissionAction::Deny => {
                                    mossen_agent::types::PermissionDecision::Deny
                                }
                            };
                            // Dropping the receiver is harmless — engine
                            // treats a missing reply as Deny.
                            let _ = responder.send(engine_decision);
                        }
                        self.active_modal = ActiveModal::None;
                    }
                    Some(InputAction::Escape) | Some(InputAction::Interrupt) => {
                        // Treat ESC as deny — also round-trip it so the
                        // engine doesn't hang waiting on the gate.
                        let tool_name = confirm.tool_name.clone();
                        self.messages.push(MessageData {
                            message_type: MessageType::System,
                            content: format!("Tool use cancelled for: {}", tool_name),
                            timestamp: None,
                            is_streaming: false,
                            tool_name: Some(tool_name),
                            is_error: true,
                            thinking: None,
                        thinking_completed_at: None,
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                        if let Some(responder) = self.active_permission_responder.take() {
                            let _ = responder.send(mossen_agent::types::PermissionDecision::Deny);
                        }
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            ActiveModal::MessageSelector(_) => {
                // ↑/↓ moves focus, Enter selects (and triggers the
                // restore-option submenu), Esc closes. Submenu mode
                // (after Enter) rebinds ↑/↓ to option selection +
                // Enter to commit the chosen restore action; Esc
                // backs out.
                match key.code {
                    KeyCode::Up => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                // Submenu: cycle restore option up.
                                let opts = state
                                    .get_restore_options(state.file_history_enabled);
                                let cur_pos = opts
                                    .iter()
                                    .position(|o| *o == state.selected_restore_option)
                                    .unwrap_or(0);
                                let next = cur_pos.saturating_sub(1);
                                state.selected_restore_option = opts[next].clone();
                            } else {
                                state.focus_prev();
                            }
                        }
                    }
                    KeyCode::Down => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                let opts = state
                                    .get_restore_options(state.file_history_enabled);
                                let cur_pos = opts
                                    .iter()
                                    .position(|o| *o == state.selected_restore_option)
                                    .unwrap_or(0);
                                let next = (cur_pos + 1).min(opts.len() - 1);
                                state.selected_restore_option = opts[next].clone();
                            } else {
                                state.focus_next();
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                // Commit the restore action.
                                state.start_restore();
                                // Trim messages back to the selected
                                // point — equivalent of TS REPL's
                                // "resume from message N". We only
                                // implement the Conversation /
                                // Nevermind paths for now; Code restore
                                // and Summarize would need persistence
                                // wiring beyond this iteration.
                                let restore_to = state.selected_index;
                                let nevermind = matches!(
                                    state.selected_restore_option,
                                    crate::components::root_large::RestoreOption::Nevermind
                                );
                                if !nevermind {
                                    self.messages.truncate(restore_to + 1);
                                    self.scroll.set_total_items(self.messages.len());
                                }
                                self.active_modal = ActiveModal::None;
                                self.services.message_selector_state = None;
                            } else {
                                state.confirm_selection();
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(state) = self.services.message_selector_state.as_mut() {
                            if state.message_to_restore.is_some() {
                                state.back();
                                return;
                            }
                        }
                        self.services.message_selector_state = None;
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            ActiveModal::Picker {
                kind,
                items,
                selected,
                ..
            } => {
                let kind = *kind;
                let len = items.len();
                match key.code {
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < len {
                            *selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        let choice = items.get(*selected).cloned().unwrap_or_default();
                        self.active_modal = ActiveModal::None;
                        self.apply_picker_choice(kind, &choice);
                    }
                    KeyCode::Esc => {
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
            _ => {
                // For scaffolded modals, ESC cancels.
                if matches!(key.code, KeyCode::Esc) {
                    self.active_modal = ActiveModal::None;
                }
            }
        }
    }

    /// React to a Picker selection. Wires both `/theme` (rebuilds the
    /// active Theme by name) and `/output-style` (stores the chosen
    /// style id so future Assistant renders use it).
    fn apply_picker_choice(&mut self, kind: PickerKind, choice: &str) {
        match kind {
            PickerKind::Theme => {
                let name = match choice {
                    "Dark" => crate::theme::ThemeName::Dark,
                    "Light" => crate::theme::ThemeName::Light,
                    "Dark (high contrast)" => crate::theme::ThemeName::DarkHighContrast,
                    "Light (high contrast)" => crate::theme::ThemeName::LightHighContrast,
                    _ => return,
                };
                self.state.theme = name;
                self.theme = crate::theme::Theme::for_name(name);
                self.messages.push(MessageData {
                    message_type: MessageType::System,
                    content: format!("Theme set to: {}", choice),
                    timestamp: None,
                    is_streaming: false,
                    tool_name: None,
                    is_error: false,
                    thinking: None,
                    thinking_completed_at: None,
                    full_content: None,
                    expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());
            }
            PickerKind::OutputStyle => {
                // Persist the choice on EngineConfig and append the
                // matching guidance block to the system prompt so the
                // next turn's API request actually reflects it.
                self.engine_config.output_style = Some(choice.to_string());
                let guidance = match choice {
                    "Concise" => Some(
                        "# Output style: Concise\n\nKeep responses tight: one sentence per idea, no preamble, no \"Sure!\" or \"Of course!\" lead-ins. If the answer is one line, the response is one line.",
                    ),
                    "Explanatory" => Some(
                        "# Output style: Explanatory\n\nWalk the user through your reasoning. State assumptions, explain why one approach beats another, and call out edge cases. Bias toward depth over brevity, but stay focused.",
                    ),
                    "Code-first" => Some(
                        "# Output style: Code-first\n\nLead with the code that solves the problem. Place explanatory prose *after* the code block. If the answer is purely conceptual (no code), say so up front and skip the empty code fence.",
                    ),
                    "Default" => None,
                    _ => None,
                };
                // Drop any previous output-style block (recognised by the
                // `# Output style:` header), then append the new one.
                self.engine_config.system_prompt.retain(|b| {
                    !b.text.starts_with("# Output style:")
                });
                if let Some(text) = guidance {
                    self.engine_config.system_prompt.push(
                        mossen_agent::types::SystemBlock {
                            text: text.to_string(),
                            cache_control: None,
                        },
                    );
                }
                self.messages.push(MessageData {
                    message_type: MessageType::System,
                    content: format!("Output style set to: {}", choice),
                    timestamp: None,
                    is_streaming: false,
                    tool_name: None,
                    is_error: false,
                    thinking: None,
                    thinking_completed_at: None,
                    full_content: None,
                    expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());
            }
        }
    }

    /// Handle submitted input.
    fn handle_submit(&mut self, input: String) {
        // Add user message
        self.messages.push(MessageData {
            message_type: MessageType::User,
            content: input.clone(),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
        });
        self.scroll.set_total_items(self.messages.len());

        // Check for slash commands
        if input.starts_with('/') {
            self.handle_command(&input[1..]);
            return;
        }

        // Build PromptParams for the engine.
        let cfg = &self.engine_config;
        let system_prompt = cfg.system_prompt.clone();

        // Build the interactive permission gate. We pair a fresh mpsc
        // channel per dispatch (capacity 16 is plenty — at most one
        // outstanding request at a time today). The TX side is wrapped in
        // an `InteractiveGate` and shipped down through `PromptParams`; the
        // RX side is stashed on `self` so the tick loop can drain
        // `PermissionRequest`s and open the modal.
        let (perm_tx, perm_rx) = tokio::sync::mpsc::channel::<
            mossen_agent::types::PermissionRequest,
        >(16);
        let gate: std::sync::Arc<dyn mossen_agent::types::PermissionGate> =
            std::sync::Arc::new(mossen_agent::types::InteractiveGate::new(perm_tx));
        self.permission_rx = Some(perm_rx);

        // Pull executable tool definitions from the registry the launcher
        // attached via `with_tool_registry`. Falling back to `Vec::new()`
        // keeps the empty-registry test path working; production runs
        // always carry the full built-in tool list so the model knows what
        // it can actually call (without this, MiniMax falls back to
        // emitting bash commands inside markdown code blocks).
        let tools = self
            .tool_registry
            .as_ref()
            .map(|r| r.definitions())
            .unwrap_or_default();

        // Drain any pasted images into ContentBlock::Image so the
        // multimodal API gets the actual bytes. The textual `[Image #N]`
        // markers stay in the prompt so the model can reference them by
        // ordinal in its reply.
        let additional_blocks: Vec<mossen_types::ContentBlock> = self
            .pending_images
            .drain(..)
            .map(|(mime, data)| {
                mossen_types::ContentBlock::Image(mossen_types::ImageBlock {
                    source: mossen_types::ImageSource {
                        source_type: "base64".to_string(),
                        media_type: mime,
                        data,
                    },
                })
            })
            .collect();

        let params = PromptParams {
            prompt: input,
            additional_blocks,
            model: cfg.model.clone(),
            system_prompt,
            tools,
            tool_use_context: ToolUseContext {
                cwd: cfg.cwd.clone(),
                additional_working_directories: None,
                extra: Default::default(),
            },
            origin_tag: cfg.origin_tag.clone(),
            max_turns: cfg.max_turns,
            api_base_url: cfg.api_base_url.clone(),
            api_key: cfg.api_key.clone(),
            extra_body: cfg.extra_body.clone(),
            permission_gate: Some(gate),
            tool_registry: self.tool_registry.clone(),
        };

        // Push empty assistant placeholder; deltas fill it in.
        let placeholder_idx = self.messages.len();
        self.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: String::new(),
            timestamp: None,
            is_streaming: true,
            tool_name: None,
            is_error: false,
            thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
        });
        self.scroll.set_total_items(self.messages.len());
        self.pending_assistant_idx = Some(placeholder_idx);
        self.assistant_buf.clear();

        self.pending_submit = Some(params);
        self.state.is_streaming = true;
        self.state.is_waiting_for_response = true;
        self.spinner.reset();
    }

    /// Handle slash commands.
    fn handle_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let args_raw = parts.get(1).copied().unwrap_or("");
        let args: Vec<&str> = if args_raw.is_empty() {
            Vec::new()
        } else {
            args_raw.split_whitespace().collect()
        };

        // Built-in fast-path commands (UI-only, never reach the registry).
        match command {
            "quit" | "exit" => {
                self.should_quit = true;
                return;
            }
            "clear" => {
                self.messages.clear();
                self.scroll.set_total_items(0);
                self.assistant_buf.clear();
                self.pending_assistant_idx = None;
                return;
            }
            "theme" => {
                self.active_modal = ActiveModal::Picker {
                    kind: PickerKind::Theme,
                    title: "Select theme".to_string(),
                    items: vec![
                        "Dark".to_string(),
                        "Light".to_string(),
                        "Dark (high contrast)".to_string(),
                        "Light (high contrast)".to_string(),
                    ],
                    selected: 0,
                };
                return;
            }
            "output-style" | "output_style" => {
                self.active_modal = ActiveModal::Picker {
                    kind: PickerKind::OutputStyle,
                    title: "Select output style".to_string(),
                    items: vec![
                        "Default".to_string(),
                        "Concise".to_string(),
                        "Explanatory".to_string(),
                        "Code-first".to_string(),
                    ],
                    selected: 0,
                };
                return;
            }
            _ => {}
        }

        // Try the directive registry.
        if let Some(reg) = self.directives.clone() {
            if let Some(directive) = find_directive(reg.as_slice(), command) {
                let ctx = self.command_context.clone();
                let name = directive.name().to_string();
                let dtype = directive.directive_type();
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { directive.execute(&args, &ctx).await })
                });

                let (msg, is_error) = match result {
                    Ok(CommandResult::Text(t)) => (t, false),
                    Ok(CommandResult::System(t)) => (t, false),
                    Ok(CommandResult::Empty) => (format!("/{} executed", name), false),
                    Ok(CommandResult::Widget) => (
                        format!("/{} produced a widget (not yet rendered in TUI)", name),
                        false,
                    ),
                    Ok(CommandResult::Exit(text)) => {
                        self.should_quit = true;
                        (text.unwrap_or_else(|| "Exiting...".to_string()), false)
                    }
                    Ok(CommandResult::Error(e)) => (e, true),
                    Err(e) => (format!("/{} failed: {}", name, e), true),
                };

                self.messages.push(MessageData {
                    message_type: MessageType::System,
                    content: msg,
                    timestamp: None,
                    is_streaming: false,
                    tool_name: None,
                    is_error,
                    thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());

                // Prompt-type directives produce a model-bound payload —
                // future work can forward `CommandResult::Text` back into
                // a follow-up `submit_prompt` call.
                let _ = dtype;
                return;
            }
        }

        // Built-in fallback help (when registry missing or command unknown).
        if command == "help" {
            self.messages.push(MessageData {
                message_type: MessageType::System,
                content: "Available commands: /help, /clear, /exit, /model, /compact, /cost, /resume, /config".into(),
                timestamp: None,
                is_streaming: false,
                tool_name: None,
                is_error: false,
                thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
            });
            self.scroll.set_total_items(self.messages.len());
            return;
        }

        self.messages.push(MessageData {
            message_type: MessageType::System,
            content: format!("Unknown command: /{}", command),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: true,
            thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
        });
        self.scroll.set_total_items(self.messages.len());
    }

    /// Update suggestions based on current input.
    fn update_suggestions(&mut self) {
        if self.prompt.is_command_input() && self.prompt.input.value.len() > 1 {
            // Strip leading '/' to get the typed prefix.
            let prefix = self.prompt.input.value[1..].to_lowercase();
            let filtered: Vec<_> = available_commands()
                .iter()
                .filter(|cmd| cmd.label.to_lowercase().starts_with(&prefix))
                .cloned()
                .collect();

            self.prompt.show_suggestions = !filtered.is_empty();
            if !filtered.is_empty() && self.prompt.selected_suggestion.is_none() {
                self.prompt.selected_suggestion = Some(0);
            } else if let Some(sel) = self.prompt.selected_suggestion {
                if sel >= filtered.len() {
                    self.prompt.selected_suggestion = Some(0);
                }
            }
            self.prompt.suggestions = filtered;
        } else {
            self.prompt.show_suggestions = false;
            self.prompt.suggestions.clear();
            self.prompt.selected_suggestion = None;
        }
    }

    /// Sticky prompt header height — 1 row when there is at least one user
    /// message to anchor to, 0 otherwise. Mirrors TS `StickyPromptHeader`
    /// which fixes its height at 1 row (truncate-end) to keep the scroll
    /// region stable across header text changes.
    fn sticky_prompt_header_height(&self) -> u16 {
        let has_user_message = self
            .messages
            .iter()
            .any(|m| matches!(m.message_type, MessageType::User));
        if has_user_message {
            1
        } else {
            0
        }
    }

    /// Forward mouse events from the ratatui pipeline. Currently translates
    /// scroll wheel events into virtual scroll movements; click/drag are
    /// reserved for future selection-mode work.
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll.scroll_up(3);
            }
            MouseEventKind::ScrollDown => {
                self.scroll.scroll_down(3);
            }
            // Click / drag / move events are not yet routed to widgets.
            _ => {}
        }
    }

    /// Add an assistant message (called when streaming completes).
    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content,
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
        });
        self.scroll.set_total_items(self.messages.len());
        self.state.is_streaming = false;
        self.state.is_waiting_for_response = false;
    }

    // -----------------------------------------------------------------
    // Engine message handling
    // -----------------------------------------------------------------

    /// Handle a single `SdkMessage` produced by the engine. Routes streaming
    /// deltas into the pending assistant message, surfaces tool-use blocks
    /// as their own message rows, and finalizes the turn on `Result`.
    /// Pull a single pending `PermissionRequest` off the gate channel and
    /// open a `ToolUseConfirm` modal for it. Called from the tick handler
    /// once per frame. The gate guarantees only one request is in flight at
    /// a time (the engine awaits the oneshot reply before issuing another).
    fn poll_permission_request(&mut self) {
        if self.active_modal.is_open() {
            return;
        }
        if self.active_permission_responder.is_some() {
            return;
        }
        let Some(rx) = self.permission_rx.as_mut() else {
            return;
        };
        let request = match rx.try_recv() {
            Ok(req) => req,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                self.permission_rx = None;
                return;
            }
        };

        let mossen_agent::types::PermissionRequest {
            tool_id,
            tool_name,
            input,
            responder,
        } = request;

        let input_summary = serde_json::to_string(&input)
            .unwrap_or_else(|_| "<unserialisable input>".to_string());
        let summary_truncated = if input_summary.len() > 240 {
            format!("{}…", &input_summary[..240])
        } else {
            input_summary
        };

        let confirm = crate::components::permissions::ToolUseConfirm {
            tool_use_id: tool_id,
            tool_name: tool_name.clone(),
            raw_input: input,
            input_summary: summary_truncated,
            risk_level: 1, // medium until we wire per-tool risk classification
        };
        let prompt = crate::components::permissions::PermissionPromptState::new(
            crate::components::permissions::PermissionKind::ToolUse {
                name: tool_name.clone(),
            },
            tool_name,
        );
        self.active_modal = ActiveModal::ToolUseConfirm { confirm, prompt };
        self.active_permission_responder = Some(responder);
    }

    pub fn handle_engine_message(&mut self, msg: SdkMessage) {
        match msg {
            SdkMessage::SystemInit { session_id, .. } => {
                self.engine_session_id = Some(session_id);
            }
            SdkMessage::User { .. } => {
                // User echo — already appended locally in handle_submit.
            }
            SdkMessage::Assistant { message, .. } => {
                // Replace any partial buffer with the final, full content.
                // Some backends emit a single Assistant message with all
                // text blocks instead of delta events; others emit deltas
                // followed by an Assistant with the full content. Either
                // way, the *final* Assistant payload is authoritative.
                let mut full_text = String::new();
                let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();
                for block in &message.content {
                    match block {
                        ContentBlock::Text(t) => full_text.push_str(&t.text),
                        ContentBlock::ToolUse(tu) => {
                            tool_uses.push((tu.id.clone(), tu.name.clone(), tu.input.clone()));
                        }
                        _ => {}
                    }
                }

                if !full_text.is_empty() {
                    // Re-derive the (thinking, content) split from the
                    // authoritative final text — same routine the streaming
                    // path uses, so the placeholder we filled in via deltas
                    // ends up identical to what we'd render from scratch.
                    let (final_thinking, final_content) =
                        split_thinking_and_content(&full_text);
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            m.thinking = final_thinking;
                            m.content = final_content;
                        }
                    } else {
                        self.messages.push(MessageData {
                            message_type: MessageType::Assistant,
                            content: final_content,
                            timestamp: None,
                            is_streaming: false,
                            tool_name: None,
                            is_error: false,
                            thinking: final_thinking,
                            // Mark the fade timer right when the final
                            // (non-streaming) Assistant payload lands —
                            // any thinking content visible here is
                            // already complete, so the 30s fade starts now.
                            thinking_completed_at: Some(std::time::Instant::now()),
                        full_content: None,
                        expanded: false,
                        });
                        self.scroll.set_total_items(self.messages.len());
                    }
                    self.assistant_buf = full_text;
                }

                for (_id, name, input) in tool_uses {
                    // Render a compact, human-readable argument list. For
                    // single-arg tools (Bash → command, Read → file_path)
                    // we show the bare value; for multi-arg tools we show
                    // `key=value, key=value`. Falls back to compact JSON
                    // when the input isn't an object.
                    let preview = format_tool_input(&input);
                    self.messages.push(MessageData {
                        message_type: MessageType::ToolUse,
                        content: preview,
                        timestamp: None,
                        is_streaming: false,
                        tool_name: Some(name),
                        is_error: false,
                        thinking: None,
                    thinking_completed_at: None,
                    full_content: None,
                    expanded: false,
                    });
                    self.scroll.set_total_items(self.messages.len());
                }
            }
            SdkMessage::StreamEvent { event } => self.handle_stream_event(event),
            SdkMessage::Result {
                terminal, cost_usd, ..
            } => {
                // Accumulate live cost so the status-bar segment + cost
                // threshold dialog can read real numbers instead of the
                // hard-coded 0.0 placeholder we used before the engine
                // started reporting per-turn cost.
                if let Some(cost) = cost_usd {
                    self.total_cost_usd += cost;
                }
                self.finalize_assistant_turn(Some(terminal));
            }
            SdkMessage::ToolUseSummary {
                tool_name,
                summary,
                full_content,
            } => {
                self.messages.push(MessageData {
                    message_type: MessageType::ToolResult,
                    content: summary,
                    timestamp: None,
                    is_streaming: false,
                    tool_name: Some(tool_name),
                    is_error: false,
                    thinking: None,
                    thinking_completed_at: None,
                    full_content,
                    expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());
            }
            SdkMessage::CompactBoundary {
                before_token_count,
                after_token_count,
            } => {
                self.messages.push(MessageData {
                    message_type: MessageType::System,
                    content: format!(
                        "(compact) tokens {} -> {}",
                        before_token_count, after_token_count
                    ),
                    timestamp: None,
                    is_streaming: false,
                    tool_name: None,
                    is_error: false,
                    thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());
            }
            SdkMessage::ApiRetry {
                error,
                attempt,
                max_retries,
                retry_in_ms,
            } => {
                self.messages.push(MessageData {
                    message_type: MessageType::System,
                    content: format!(
                        "API retry {}/{} in {}ms: {}",
                        attempt, max_retries, retry_in_ms, error
                    ),
                    timestamp: None,
                    is_streaming: false,
                    tool_name: None,
                    is_error: true,
                    thinking: None,
                thinking_completed_at: None,
                full_content: None,
                expanded: false,
                });
                self.scroll.set_total_items(self.messages.len());
            }
        }
    }

    /// Handle a streaming `StreamEventData` from the engine — text deltas
    /// are appended to the pending assistant message.
    fn handle_stream_event(&mut self, event: StreamEventData) {
        match event {
            StreamEventData::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::TextDelta { text } => {
                    // Accumulate the full streamed text, then re-derive the
                    // `(thinking, content)` split on every chunk. Recomputing
                    // from the whole buffer (rather than tracking incremental
                    // open/close state) makes us robust against `<think>`
                    // / `</think>` tags arriving split across chunk
                    // boundaries — which MiniMax routinely does.
                    self.assistant_buf.push_str(&text);
                    let (thinking, content) =
                        split_thinking_and_content(&self.assistant_buf);
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            m.thinking = thinking;
                            m.content = content;
                        }
                    }
                }
                ContentDelta::ThinkingDelta { thinking } => {
                    // Some backends emit reasoning via the dedicated
                    // `thinking_delta` channel instead of inline `<think>`
                    // tags. Append to the pending message's thinking buffer.
                    if let Some(idx) = self.pending_assistant_idx {
                        if let Some(m) = self.messages.get_mut(idx) {
                            match &mut m.thinking {
                                Some(buf) => buf.push_str(&thinking),
                                None => m.thinking = Some(thinking),
                            }
                        }
                    }
                }
                ContentDelta::InputJsonDelta { .. } => {
                    // Tool-input deltas accumulate on the engine side; the
                    // finalized Assistant message provides the parsed input.
                }
            },
            StreamEventData::MessageDelta {
                stop_reason: Some(reason),
                ..
            } => {
                // Surface stop reasons as quiet system breadcrumbs.
                if reason != "end_turn" {
                    self.messages.push(MessageData {
                        message_type: MessageType::Progress,
                        content: format!("(stop: {})", reason),
                        timestamp: None,
                        is_streaming: false,
                        tool_name: None,
                        is_error: false,
                        thinking: None,
                    thinking_completed_at: None,
                    full_content: None,
                    expanded: false,
                    });
                    self.scroll.set_total_items(self.messages.len());
                }
            }
            _ => {}
        }
    }

    /// Mark the in-flight assistant turn as finished.
    fn finalize_assistant_turn(&mut self, terminal: Option<String>) {
        if let Some(idx) = self.pending_assistant_idx.take() {
            if let Some(m) = self.messages.get_mut(idx) {
                m.is_streaming = false;
                // Stamp the thinking-fade clock now that the stream is
                // closed. Render side reads this to decide whether the
                // 30s display window has elapsed.
                m.thinking_completed_at = Some(std::time::Instant::now());
                if m.content.is_empty() {
                    m.content = match &terminal {
                        Some(t) => format!("(no content — terminal={})", t),
                        None => "(no content)".to_string(),
                    };
                }
            }
        }
        // Auto-collapse every ToolUse row this turn produced so the
        // scrollback stays tidy after the turn ends. The user can
        // expand any of them via Space/Enter while focused.
        for (i, m) in self.messages.iter().enumerate() {
            if matches!(m.message_type, MessageType::ToolUse) {
                self.collapsed_tool_groups.insert(i);
            }
        }
        self.engine_rx = None;
        self.assistant_buf.clear();
        self.state.is_streaming = false;
        self.state.is_waiting_for_response = false;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate `s` to at most `max` chars (byte-safe over UTF-8 codepoints).
/// Read an image off the OS clipboard. Platform-specific helpers:
///   * macOS — `osascript` extracts PNG from `«class PNGf»` and writes
///     it to a temp file we then read; `pbpaste` does not handle binary
///     clipboard data so this is the canonical path.
///   * Linux — try `xclip -selection clipboard -t image/png -o`.
///   * Anything else — return None (text-only paste).
/// Returns the raw PNG bytes when an image is present, or None when the
/// clipboard holds text / nothing / on unsupported platforms.
fn read_clipboard_image_bytes() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::temp_dir().join(format!(
            "mossen-clipboard-{}.png",
            std::process::id()
        ));
        let tmp_str = tmp.to_string_lossy().to_string();
        let script = format!(
            "set png_data to the clipboard as «class PNGf»\n\
             set f to open for access POSIX file \"{}\" with write permission\n\
             try\n  set eof of f to 0\n  write png_data to f\nend try\n\
             close access f",
            tmp_str.replace('"', "\\\"")
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .ok()?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        let bytes = std::fs::read(&tmp).ok();
        let _ = std::fs::remove_file(&tmp);
        bytes.filter(|b| !b.is_empty())
    }
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-o"])
            .output()
            .ok()?;
        if !out.status.success() || out.stdout.is_empty() {
            return None;
        }
        Some(out.stdout)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Render a tool-call's JSON input as a short, human-readable single-line
/// summary for the ToolUse row. We deliberately avoid showing the raw JSON
/// blob the way the engine emits it — long Bash commands, file paths, and
/// edit diffs become unreadable through `{"command":"…"}` framing.
fn format_tool_input(input: &serde_json::Value) -> String {
    use serde_json::Value;
    const MAX_PREVIEW: usize = 240;

    let Some(obj) = input.as_object() else {
        // Non-object input — fall back to compact JSON.
        let raw = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
        return truncate(&raw, MAX_PREVIEW);
    };
    if obj.is_empty() {
        return String::new();
    }

    // Single-key tools (Bash.command, Read.file_path, Write.content): just
    // show the value. This is what the user actually wants to read.
    if obj.len() == 1 {
        let (_k, v) = obj.iter().next().unwrap();
        let s = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        return truncate(&s, MAX_PREVIEW);
    }

    // Multi-key: `k=v, k=v`. Strings show unquoted; everything else uses
    // its compact JSON form so `Bool(true)` reads as `true` not `"true"`.
    let mut parts: Vec<String> = Vec::new();
    for (k, v) in obj.iter() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        parts.push(format!("{}={}", k, rendered));
    }
    truncate(&parts.join(", "), MAX_PREVIEW)
}

/// Built-in slash commands surfaced via prompt suggestions.
///
/// Mirrors the command set referenced by `App::handle_command`. Kept in sync
/// manually because the full command registry lives in `mossen-commands`
/// and TUI consumes only this top-level subset for autocomplete display.
fn available_commands() -> Vec<Suggestion> {
    const COMMANDS: &[(&str, &str)] = &[
        ("help", "Show available commands"),
        ("clear", "Clear conversation"),
        ("exit", "Exit the application"),
        ("quit", "Exit the application"),
        ("model", "Change the active model"),
        ("compact", "Compact conversation history"),
        ("resume", "Resume a previous session"),
        ("cost", "Show session cost"),
        ("config", "Open configuration"),
    ];
    COMMANDS
        .iter()
        .map(|(label, desc)| Suggestion {
            label: (*label).to_string(),
            description: Some((*desc).to_string()),
            kind: SuggestionKind::Command,
        })
        .collect()
}

// Provide a small modal smoke-test entry point that exercises the
// permission-prompt overlay surface (used during integration testing).
#[doc(hidden)]
pub fn __debug_open_permission_modal(app: &mut App) {
    app.active_modal = ActiveModal::PermissionRequest(PermissionPromptState::new(
        PermissionKind::FileRead {
            path: "/tmp/debug".to_string(),
        },
        "DebugTool",
    ));
}

/// Peel `<think>...</think>` reasoning blocks out of a streamed assistant
/// text buffer, returning `(thinking, content)`. Multiple `<think>` blocks
/// are concatenated (newline-joined) into the same thinking buffer. An
/// unclosed `<think>` (still streaming) routes the trailing tail to
/// thinking so it can render live; once `</think>` arrives the split
/// resolves cleanly on the next chunk.
///
/// Returns `(None, buf)` when the buffer contains no `<think>` markers — so
/// non-reasoning backends pay no cost beyond a single `str::find`.
pub fn split_thinking_and_content(buf: &str) -> (Option<String>, String) {
    if !buf.contains("<think>") {
        return (None, buf.to_string());
    }

    let mut thinking_parts: Vec<&str> = Vec::new();
    let mut content_parts: Vec<&str> = Vec::new();
    let mut rest = buf;

    loop {
        match rest.find("<think>") {
            Some(open) => {
                if open > 0 {
                    content_parts.push(&rest[..open]);
                }
                let after_open = &rest[open + "<think>".len()..];
                match after_open.find("</think>") {
                    Some(close) => {
                        thinking_parts.push(&after_open[..close]);
                        rest = &after_open[close + "</think>".len()..];
                    }
                    None => {
                        // Open tag without close — still streaming. The
                        // remainder of the buffer is all thinking for now.
                        thinking_parts.push(after_open);
                        rest = "";
                        break;
                    }
                }
            }
            None => {
                content_parts.push(rest);
                break;
            }
        }
    }

    let thinking_joined = thinking_parts.join("\n").trim().to_string();
    let content_joined = content_parts.join("").trim_start().to_string();
    let thinking = if thinking_joined.is_empty() {
        None
    } else {
        Some(thinking_joined)
    };
    (thinking, content_joined)
}

#[cfg(test)]
mod tool_input_tests {
    use super::format_tool_input;
    use serde_json::json;

    #[test]
    fn single_string_arg_shows_bare_value() {
        let v = json!({"command": "echo hi"});
        assert_eq!(format_tool_input(&v), "echo hi");
    }

    #[test]
    fn multi_arg_renders_k_v_list() {
        let v = json!({"file_path": "/tmp/a.txt", "limit": 100});
        let out = format_tool_input(&v);
        assert!(out.contains("file_path=/tmp/a.txt"));
        assert!(out.contains("limit=100"));
    }

    #[test]
    fn empty_object_is_blank() {
        assert_eq!(format_tool_input(&json!({})), "");
    }

    #[test]
    fn long_values_get_truncated() {
        let long = "x".repeat(500);
        let v = json!({"command": long});
        let out = format_tool_input(&v);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 241);
    }
}

#[cfg(test)]
mod think_split_tests {
    use super::split_thinking_and_content;

    #[test]
    fn passthrough_when_no_marker() {
        let (think, content) = split_thinking_and_content("hello world");
        assert!(think.is_none());
        assert_eq!(content, "hello world");
    }

    #[test]
    fn closed_block_splits_cleanly() {
        let (think, content) =
            split_thinking_and_content("<think>weighing options</think>\n\nthe answer is 42");
        assert_eq!(think.as_deref(), Some("weighing options"));
        assert_eq!(content, "the answer is 42");
    }

    #[test]
    fn unclosed_block_streams_to_thinking() {
        let (think, content) = split_thinking_and_content("<think>still reasoning…");
        assert_eq!(think.as_deref(), Some("still reasoning…"));
        assert_eq!(content, "");
    }

    #[test]
    fn multiple_blocks_concatenate() {
        let (think, content) = split_thinking_and_content(
            "<think>step one</think>partial<think>step two</think>final",
        );
        assert_eq!(think.as_deref(), Some("step one\nstep two"));
        assert_eq!(content, "partialfinal");
    }
}
