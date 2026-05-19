//! Permission management UI components.
//!
//! Translates: components/permissions/ (15 files + 15 subdirectories)
//! + ManagedSettingsSecurityDialog/ into permission prompt widgets.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::components::design_system::DialogWidget;
use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Core permission types
// ---------------------------------------------------------------------------

/// Access policy — replaces TS PermissionMode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessPolicy {
    Supervised,
    ReadOnly,
    TrustEdits,
    Unrestricted,
    AutoDeny,
    SwiftMode,
    Delegated,
}

impl AccessPolicy {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Supervised => "Supervised",
            Self::ReadOnly => "Read Only",
            Self::TrustEdits => "Trust Edits",
            Self::Unrestricted => "Unrestricted",
            Self::AutoDeny => "Auto Deny",
            Self::SwiftMode => "Swift Mode",
            Self::Delegated => "Delegated",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Supervised => "Confirm each action before execution",
            Self::ReadOnly => "Only read operations allowed",
            Self::TrustEdits => "Auto-approve file edits, ask for others",
            Self::Unrestricted => "All operations approved automatically",
            Self::AutoDeny => "Deny all operations without asking",
            Self::SwiftMode => "AI decides based on safety analysis",
            Self::Delegated => "Delegated to parent agent",
        }
    }

    pub fn all_user_facing() -> &'static [AccessPolicy] {
        &[
            Self::Supervised,
            Self::ReadOnly,
            Self::TrustEdits,
            Self::SwiftMode,
            Self::Unrestricted,
        ]
    }
}

/// Verdict for a permission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessVerdict {
    Permit,
    Block,
    Prompt,
}

/// The type of permission being requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionKind {
    Shell { command: String },
    FileEdit { path: String },
    FileWrite { path: String },
    FileRead { path: String },
    WebFetch { url: String },
    Skill { name: String },
    UserQuestion { question: String },
    PlanMode { enter: bool },
    ComputerUse,
    Notebook { path: String },
    PowerShell { command: String },
    Filesystem { paths: Vec<String> },
    /// Generic catch-all for engine-emitted tool-use requests when the
    /// permission gate doesn't pre-classify into one of the typed variants
    /// above. Carries the tool name so the modal title/description can still
    /// be specific.
    ToolUse { name: String },
}

impl PermissionKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Shell { .. } => "Shell Command",
            Self::FileEdit { .. } => "File Edit",
            Self::FileWrite { .. } => "File Write",
            Self::FileRead { .. } => "File Read",
            Self::WebFetch { .. } => "Web Fetch",
            Self::Skill { .. } => "Skill",
            Self::UserQuestion { .. } => "User Question",
            Self::PlanMode { enter: true } => "Enter Plan Mode",
            Self::PlanMode { enter: false } => "Exit Plan Mode",
            Self::ComputerUse => "Computer Use",
            Self::Notebook { .. } => "Notebook Edit",
            Self::PowerShell { .. } => "PowerShell",
            Self::Filesystem { .. } => "Filesystem Access",
            Self::ToolUse { .. } => "Tool Use",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Shell { command } => command.clone(),
            Self::FileEdit { path } | Self::FileWrite { path } | Self::FileRead { path } => {
                path.clone()
            }
            Self::WebFetch { url } => url.clone(),
            Self::Skill { name } => name.clone(),
            Self::UserQuestion { question } => question.clone(),
            Self::PlanMode { .. } => String::new(),
            Self::ComputerUse => "Computer interaction".into(),
            Self::Notebook { path } => path.clone(),
            Self::PowerShell { command } => command.clone(),
            Self::Filesystem { paths } => paths.join(", "),
            Self::ToolUse { name } => name.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionPromptState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PermissionPromptState {
    pub kind: PermissionKind,
    pub tool_name: String,
    pub explanation: Option<String>,
    pub selected_action: PermissionAction,
    pub result: Option<AccessVerdict>,
    pub show_details: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    AllowAlways,
    Deny,
}

impl PermissionPromptState {
    pub fn new(kind: PermissionKind, tool_name: impl Into<String>) -> Self {
        Self {
            kind,
            tool_name: tool_name.into(),
            explanation: None,
            selected_action: PermissionAction::Allow,
            result: None,
            show_details: false,
        }
    }

    pub fn cycle_action(&mut self) {
        self.selected_action = match self.selected_action {
            PermissionAction::Allow => PermissionAction::AllowAlways,
            PermissionAction::AllowAlways => PermissionAction::Deny,
            PermissionAction::Deny => PermissionAction::Allow,
        };
    }

    pub fn confirm(&mut self) {
        self.result = Some(match self.selected_action {
            PermissionAction::Allow | PermissionAction::AllowAlways => AccessVerdict::Permit,
            PermissionAction::Deny => AccessVerdict::Block,
        });
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }
}

// ---------------------------------------------------------------------------
// PermissionPromptWidget — main permission prompt (PermissionPrompt.tsx)
// ---------------------------------------------------------------------------

pub struct PermissionPromptWidget<'a> {
    pub state: &'a PermissionPromptState,
    pub theme: &'a Theme,
}

impl<'a> PermissionPromptWidget<'a> {
    pub fn new(state: &'a PermissionPromptState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for PermissionPromptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.permission))
            .title(Span::styled(
                format!(" {} ", self.state.kind.label()),
                Style::default()
                    .fg(self.theme.permission)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tool name
                Constraint::Length(1), // Detail
                Constraint::Min(1),    // Explanation / detail expansion
                Constraint::Length(1), // Actions
            ])
            .split(inner);

        // Tool name
        let tool_line = Line::from(vec![
            Span::styled("Tool: ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                &self.state.tool_name,
                Style::default()
                    .fg(self.theme.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        buf.set_line(chunks[0].x, chunks[0].y, &tool_line, chunks[0].width);

        // Detail
        let detail = self.state.kind.detail();
        if !detail.is_empty() {
            let detail_line = Line::from(Span::styled(
                &detail,
                Style::default().fg(self.theme.primary),
            ));
            buf.set_line(chunks[1].x, chunks[1].y, &detail_line, chunks[1].width);
        }

        // Explanation
        if let Some(ref expl) = self.state.explanation {
            if self.state.show_details {
                let p = Paragraph::new(expl.as_str())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(self.theme.text_dim));
                p.render(chunks[2], buf);
            }
        }

        // Action buttons
        let actions = [
            (PermissionAction::Allow, "Allow", self.theme.success),
            (PermissionAction::AllowAlways, "Always", self.theme.info),
            (PermissionAction::Deny, "Deny", self.theme.error),
        ];

        let mut spans: Vec<Span> = Vec::new();
        for (action, label, color) in &actions {
            let is_selected = self.state.selected_action == *action;
            let style = if is_selected {
                Style::default()
                    .fg(self.theme.text)
                    .bg(*color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(*color)
            };
            spans.push(Span::styled(format!(" {} ", label), style));
            spans.push(Span::raw("  "));
        }
        let action_line = Line::from(spans);
        buf.set_line(chunks[3].x, chunks[3].y, &action_line, chunks[3].width);
    }
}

// ---------------------------------------------------------------------------
// AccessGateWidget — permission gate overlay (PermissionDialog.tsx)
// ---------------------------------------------------------------------------

pub struct AccessGateWidget<'a> {
    pub state: &'a PermissionPromptState,
    pub theme: &'a Theme,
}

impl<'a> AccessGateWidget<'a> {
    pub fn new(state: &'a PermissionPromptState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for AccessGateWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear background for modal overlay
        Clear.render(area, buf);

        // Center the permission prompt
        let prompt_height = if self.state.show_details { 10u16 } else { 7u16 };
        let prompt_width = 60u16.min(area.width.saturating_sub(4));
        let prompt_area = crate::layout::center(area, prompt_width, prompt_height);

        let prompt_widget = PermissionPromptWidget::new(self.state, self.theme);
        prompt_widget.render(prompt_area, buf);
    }
}

// ---------------------------------------------------------------------------
// ManagedSettingsSecurityDialogWidget
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ManagedSettingsSecurityDialogState {
    pub managed_settings: Vec<ManagedSetting>,
    pub acknowledged: bool,
}

#[derive(Debug, Clone)]
pub struct ManagedSetting {
    pub key: String,
    pub value: String,
    pub source: String,
}

impl ManagedSettingsSecurityDialogState {
    pub fn new(settings: Vec<ManagedSetting>) -> Self {
        Self {
            managed_settings: settings,
            acknowledged: false,
        }
    }
}

pub struct ManagedSettingsSecurityDialogWidget<'a> {
    pub state: &'a ManagedSettingsSecurityDialogState,
    pub theme: &'a Theme,
}

impl<'a> ManagedSettingsSecurityDialogWidget<'a> {
    pub fn new(state: &'a ManagedSettingsSecurityDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for ManagedSettingsSecurityDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = DialogWidget::new("Managed Settings", self.theme).size(65, 16);
        let inner = dialog.inner_area(area);
        dialog.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut lines = vec![
            Line::from(Span::styled(
                "The following settings are managed externally:",
                Style::default().fg(self.theme.warning),
            )),
            Line::from(""),
        ];

        for setting in &self.state.managed_settings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} = ", setting.key),
                    Style::default().fg(self.theme.text),
                ),
                Span::styled(&setting.value, Style::default().fg(self.theme.primary)),
                Span::styled(
                    format!("  ({})", setting.source),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to acknowledge",
            Style::default().fg(self.theme.text_dim),
        )));

        Paragraph::new(lines).render(inner, buf);
    }
}

// ===================================================================
// Permission request shared types
// ===================================================================

/// ToolUseConfirm describes a pending tool use awaiting permission.
#[derive(Debug, Clone)]
pub struct ToolUseConfirm {
    pub tool_use_id: String,
    pub tool_name: String,
    pub raw_input: serde_json::Value,
    pub input_summary: String,
    pub risk_level: u8, // 0=low, 1=med, 2=high
}

/// One option displayed in the permission prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPromptOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub hotkey: Option<char>,
}

/// Analytics context attached to permission decisions.
#[derive(Debug, Clone, Default)]
pub struct ToolAnalyticsContext {
    pub tool_name: String,
    pub session_id: String,
    pub turn_index: u64,
    pub mcp_server: Option<String>,
}

/// Props for the top-level PermissionPrompt.
#[derive(Debug, Clone)]
pub struct PermissionPromptProps {
    pub title: String,
    pub body: String,
    pub options: Vec<PermissionPromptOption>,
    pub analytics: ToolAnalyticsContext,
}

/// PermissionPrompt state: which option is currently focused.
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    pub props: PermissionPromptProps,
    pub selected_index: usize,
}

impl PermissionPrompt {
    pub fn new(props: PermissionPromptProps) -> Self {
        Self {
            props,
            selected_index: 0,
        }
    }
    pub fn next(&mut self) {
        if !self.props.options.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.props.options.len();
        }
    }
    pub fn prev(&mut self) {
        if !self.props.options.is_empty() {
            self.selected_index =
                (self.selected_index + self.props.options.len() - 1) % self.props.options.len();
        }
    }
    pub fn selected(&self) -> Option<&PermissionPromptOption> {
        self.props.options.get(self.selected_index)
    }
}

// ===================================================================
// PermissionDialog — top-level dialog wrapper around a prompt
// ===================================================================

#[derive(Debug, Clone)]
pub struct PermissionDialog {
    pub prompt: PermissionPrompt,
    pub width: u16,
    pub height: u16,
}

impl PermissionDialog {
    pub fn new(prompt: PermissionPrompt) -> Self {
        Self {
            prompt,
            width: 70,
            height: 14,
        }
    }
}

// ===================================================================
// PermissionRuleExplanation / PermissionExplanation / PermissionRequestTitle
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleExplanationProps {
    pub rule_text: String,
    pub source: String,
    pub scope: String, // "user" | "project" | "managed"
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleExplanation {
    pub props: PermissionRuleExplanationProps,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRequestTitle {
    pub tool_name: String,
    pub action: String,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionExplainerContent {
    pub heading: String,
    pub bullets: Vec<String>,
}

/// Hook-equivalent: build the explainer UI for a given prompt.
pub fn use_permission_explainer_ui(prompt: &PermissionPrompt) -> PermissionExplainerContent {
    PermissionExplainerContent {
        heading: prompt.props.title.clone(),
        bullets: prompt
            .props
            .options
            .iter()
            .map(|o| o.label.clone())
            .collect(),
    }
}

// ===================================================================
// Shell permission helpers
// ===================================================================

/// Build a label for shell command suggestions (a comma-joined preview).
pub fn generate_shell_suggestions_label(suggestions: &[&str], max_len: usize) -> String {
    let joined = suggestions.join(", ");
    if joined.len() <= max_len {
        joined
    } else {
        let mut s = joined;
        s.truncate(max_len.saturating_sub(3));
        s.push_str("...");
        s
    }
}

/// Shell permission feedback hook state.
#[derive(Debug, Clone, Default)]
pub struct ShellPermissionFeedback {
    pub last_decision: Option<String>,
    pub last_command: Option<String>,
    pub suggestion_chosen: Option<String>,
}

/// Hook-equivalent: return a mutable handle to the shell feedback state.
pub fn use_shell_permission_feedback(state: &mut ShellPermissionFeedback) -> &mut ShellPermissionFeedback {
    state
}

// ===================================================================
// Worker badges
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct WorkerBadgeProps {
    pub worker_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkerBadge {
    pub props: WorkerBadgeProps,
}

#[derive(Debug, Clone, Default)]
pub struct WorkerPendingPermission {
    pub worker_id: String,
    pub tool_use_id: String,
    pub queued_at_ms: u64,
}

// ===================================================================
// Utility: log unary permission events
// ===================================================================

/// Log helper recording a single binary permission event.
pub fn log_unary_permission_event(name: &str, decision: &str, tool: &str) {
    tracing::debug!(target: "permissions", "{} decision={} tool={}", name, decision, tool);
}

// ===================================================================
// Fallback / Sandbox permission requests
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct FallbackPermissionRequest {
    pub confirm: Option<ToolUseConfirm>,
}

#[derive(Debug, Clone, Default)]
pub struct SandboxPermissionRequestProps {
    pub action: String,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SandboxPermissionRequest {
    pub props: SandboxPermissionRequestProps,
}

// ===================================================================
// PermissionDecisionDebugInfo — dev-only debug overlay
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct PermissionDecisionDebugInfo {
    pub matched_rule: Option<String>,
    pub scope: Option<String>,
    pub allowed: bool,
    pub reason: String,
}

// ===================================================================
// Generic permission request logging hook
// ===================================================================

/// Unary event payload — single bit of decision telemetry.
#[derive(Debug, Clone)]
pub struct UnaryEvent {
    pub name: String,
    pub tool: String,
    pub value: bool,
    pub at_ms: u64,
}

/// Hook-equivalent: record permission-request lifecycle events.
pub fn use_permission_request_logging(events: &mut Vec<UnaryEvent>, ev: UnaryEvent) {
    events.push(ev);
}

// ===================================================================
// Specific permission request widgets (state structs)
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct NotebookEditPermissionRequest {
    pub notebook_path: String,
    pub cell_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct NotebookEditToolDiff {
    pub notebook_path: String,
    pub old_source: String,
    pub new_source: String,
}

#[derive(Debug, Clone, Default)]
pub struct PowerShellPermissionRequest {
    pub command: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Default)]
pub struct PowerShellToolUseOption {
    pub label: String,
    pub value: String,
    pub kind: String, // "approve_once" | "approve_session" | "deny" | "edit_rule"
}

/// Build the list of options shown in the PowerShell permission request.
pub fn powershell_tool_use_options(cmd: &str) -> Vec<PowerShellToolUseOption> {
    vec![
        PowerShellToolUseOption {
            label: "Approve once".into(),
            value: "approve_once".into(),
            kind: "approve_once".into(),
        },
        PowerShellToolUseOption {
            label: format!("Always approve `{}`", first_word(cmd)),
            value: "approve_session".into(),
            kind: "approve_session".into(),
        },
        PowerShellToolUseOption {
            label: "Deny".into(),
            value: "deny".into(),
            kind: "deny".into(),
        },
    ]
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

#[derive(Debug, Clone, Default)]
pub struct SedEditPermissionRequest {
    pub path: String,
    pub script: String,
}

#[derive(Debug, Clone, Default)]
pub struct FilesystemPermissionRequest {
    pub action: String,
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct ComputerUseApproval {
    pub action: String,
    pub target: String,
}

#[derive(Debug, Clone, Default)]
pub struct BashPermissionRequest {
    pub command: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Default)]
pub struct BashToolUseOption {
    pub label: String,
    pub value: String,
    pub kind: String,
}

pub fn bash_tool_use_options(cmd: &str) -> Vec<BashToolUseOption> {
    vec![
        BashToolUseOption {
            label: "Approve once".into(),
            value: "approve_once".into(),
            kind: "approve_once".into(),
        },
        BashToolUseOption {
            label: format!("Always approve `{}`", first_word(cmd)),
            value: "approve_session".into(),
            kind: "approve_session".into(),
        },
        BashToolUseOption {
            label: "Deny".into(),
            value: "deny".into(),
            kind: "deny".into(),
        },
    ]
}

#[derive(Debug, Clone, Default)]
pub struct FileWriteToolDiff {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct FileWritePermissionRequest {
    pub path: String,
    pub diff: FileWriteToolDiff,
}

#[derive(Debug, Clone, Default)]
pub struct ExitPlanModePermissionRequest {
    pub plan_text: String,
    pub auto_named_session: Option<String>,
}

/// Build permission updates from a plan-mode exit confirmation.
pub fn build_permission_updates(approve_edits: bool, approve_bash: bool) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    if approve_edits {
        out.push(("edits".to_string(), true));
    }
    if approve_bash {
        out.push(("bash".to_string(), true));
    }
    out
}

/// Auto-derive a session name from the first line of a plan.
pub fn auto_name_session_from_plan(plan: &str) -> Option<String> {
    plan.lines()
        .next()
        .map(|l| l.trim().trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Build the option set for a plan-approval prompt.
pub fn build_plan_approval_options(can_edit: bool, can_bash: bool) -> Vec<PermissionPromptOption> {
    let mut out = vec![PermissionPromptOption {
        label: "Approve plan".into(),
        value: "approve_plan".into(),
        description: Some("Exit plan mode and execute".into()),
        hotkey: Some('y'),
    }];
    if can_edit {
        out.push(PermissionPromptOption {
            label: "Approve + trust edits".into(),
            value: "approve_plan_trust_edits".into(),
            description: Some("Auto-approve file edits".into()),
            hotkey: Some('e'),
        });
    }
    if can_bash {
        out.push(PermissionPromptOption {
            label: "Approve + trust bash".into(),
            value: "approve_plan_trust_bash".into(),
            description: Some("Auto-approve shell commands".into()),
            hotkey: Some('b'),
        });
    }
    out.push(PermissionPromptOption {
        label: "Reject plan".into(),
        value: "reject_plan".into(),
        description: Some("Stay in plan mode".into()),
        hotkey: Some('n'),
    });
    out
}

#[derive(Debug, Clone, Default)]
pub struct FileEditPermissionRequest {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
}

// ===================================================================
// AskUserQuestion permission request — multiple-choice state
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct QuestionNavigationBar {
    pub current: usize,
    pub total: usize,
}

/// Value of one answer in a multiple-choice question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerValue {
    Single(String),
    Multi(Vec<String>),
    Text(String),
    Skipped,
}

impl Default for AnswerValue {
    fn default() -> Self {
        AnswerValue::Skipped
    }
}

/// State of one question.
#[derive(Debug, Clone, Default)]
pub struct QuestionState {
    pub question_id: String,
    pub answer: AnswerValue,
    pub focused_option: usize,
    pub free_text: String,
}

/// Aggregate state for the multi-choice questionnaire.
#[derive(Debug, Clone, Default)]
pub struct MultipleChoiceState {
    pub questions: Vec<QuestionState>,
    pub current_index: usize,
}

impl MultipleChoiceState {
    pub fn next_question(&mut self) {
        if self.current_index + 1 < self.questions.len() {
            self.current_index += 1;
        }
    }
    pub fn prev_question(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }
    pub fn current(&self) -> Option<&QuestionState> {
        self.questions.get(self.current_index)
    }
    pub fn current_mut(&mut self) -> Option<&mut QuestionState> {
        self.questions.get_mut(self.current_index)
    }
}

/// Hook-equivalent for useMultipleChoiceState.
pub fn use_multiple_choice_state(state: &mut MultipleChoiceState) -> &mut MultipleChoiceState {
    state
}

#[derive(Debug, Clone, Default)]
pub struct PreviewQuestionView {
    pub question_text: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PreviewBox {
    pub preview_lines: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QuestionView {
    pub state: QuestionState,
}

#[derive(Debug, Clone, Default)]
pub struct SubmitQuestionsView {
    pub answers: Vec<AnswerValue>,
    pub submitting: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AskUserQuestionPermissionRequest {
    pub state: MultipleChoiceState,
}

#[derive(Debug, Clone, Default)]
pub struct WebFetchPermissionRequest {
    pub url: String,
    pub method: String,
}

// ===================================================================
// FilePermissionDialog — IDE diff integration
// ===================================================================

/// One file edit operation in the IDE diff config.
#[derive(Debug, Clone, Default)]
pub struct FileEdit {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
}

/// Change input passed into the IDE diff support.
#[derive(Debug, Clone, Default)]
pub struct IDEDiffChangeInput {
    pub edits: Vec<FileEdit>,
    pub apply_immediately: bool,
}

/// Capability descriptor for the IDE diff plugin.
#[derive(Debug, Clone, Default)]
pub struct IDEDiffSupport {
    pub supported: bool,
    pub plugin_id: String,
}

/// Resolved IDE diff configuration.
#[derive(Debug, Clone, Default)]
pub struct IDEDiffConfig {
    pub support: IDEDiffSupport,
    pub change: IDEDiffChangeInput,
}

/// Build an IDE diff config for a single-file edit.
pub fn create_single_edit_diff_config(
    path: &str,
    old_text: &str,
    new_text: &str,
    support: IDEDiffSupport,
) -> IDEDiffConfig {
    IDEDiffConfig {
        support,
        change: IDEDiffChangeInput {
            edits: vec![FileEdit {
                path: path.into(),
                old_text: old_text.into(),
                new_text: new_text.into(),
            }],
            apply_immediately: false,
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilePermissionDialogProps {
    pub tool_use: Option<ToolUseConfirm>,
    pub diff_config: Option<IDEDiffConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct FilePermissionDialog {
    pub props: FilePermissionDialogProps,
    pub selected_index: usize,
}

/// Tool input shape for the file-permission dialog hook.
#[derive(Debug, Clone, Default)]
pub struct ToolInput {
    pub tool_name: String,
    pub path: String,
    pub content: Option<String>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UseFilePermissionDialogProps {
    pub tool_input: ToolInput,
    pub auto_approve: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UseFilePermissionDialogResult {
    pub dialog: FilePermissionDialog,
    pub awaiting_user: bool,
}

/// Hook-equivalent: derive the dialog state from a file permission request.
pub fn use_file_permission_dialog(
    props: &UseFilePermissionDialogProps,
) -> UseFilePermissionDialogResult {
    UseFilePermissionDialogResult {
        dialog: FilePermissionDialog {
            props: FilePermissionDialogProps {
                tool_use: None,
                diff_config: None,
            },
            selected_index: 0,
        },
        awaiting_user: !props.auto_approve,
    }
}

// ===================================================================
// File permission options
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperationType {
    Read,
    Write,
    Edit,
    Delete,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    pub value: String,
    pub scope: String, // "session" | "project" | "user"
    pub operation: FileOperationType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOptionWithLabel {
    pub option: PermissionOption,
    pub label: String,
    pub description: String,
}

/// Whether path is inside the project-local .mossen folder.
pub fn is_in_mossen_folder(path: &str, project_root: &str) -> bool {
    let prefix = format!("{}/.mossen/", project_root.trim_end_matches('/'));
    path.starts_with(&prefix)
}

/// Whether path is inside the user's global ~/.mossen folder.
pub fn is_in_global_mossen_folder(path: &str, home: &str) -> bool {
    let prefix = format!("{}/.mossen/", home.trim_end_matches('/'));
    path.starts_with(&prefix)
}

/// Build the option list shown in a file permission dialog.
pub fn get_file_permission_options(
    path: &str,
    op: FileOperationType,
) -> Vec<PermissionOptionWithLabel> {
    let mut opts = vec![PermissionOptionWithLabel {
        option: PermissionOption {
            value: "approve_once".into(),
            scope: "session".into(),
            operation: op,
        },
        label: "Approve this time".into(),
        description: format!("Allow {:?} on {} once", op, path),
    }];
    opts.push(PermissionOptionWithLabel {
        option: PermissionOption {
            value: "approve_always".into(),
            scope: "project".into(),
            operation: op,
        },
        label: "Always approve for this project".into(),
        description: "Persist to project settings".into(),
    });
    opts.push(PermissionOptionWithLabel {
        option: PermissionOption {
            value: "deny".into(),
            scope: "session".into(),
            operation: op,
        },
        label: "Deny".into(),
        description: "Refuse this operation".into(),
    });
    opts
}

// ===================================================================
// Permission handlers (FilePermissionDialog/usePermissionHandler.ts)
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct PermissionHandlerParams {
    pub tool_input: ToolInput,
    pub decision: String,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionHandlerOptions {
    pub persist_to_project: bool,
    pub persist_to_user: bool,
    pub apply_diff_in_ide: bool,
}

/// Registry of named permission handlers (each handles a decision verb).
pub fn permission_handlers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("approve_once", "Approve a single tool use."),
        ("approve_session", "Approve for the rest of the session."),
        ("approve_project", "Persist to project settings."),
        ("approve_user", "Persist to user settings."),
        ("deny", "Deny this tool use."),
        ("edit_rule", "Open the rule editor."),
    ]
}

/// Static list of permission handler verbs.
pub const PERMISSION_HANDLERS: &[&str] = &[
    "approve_once",
    "approve_session",
    "approve_project",
    "approve_user",
    "deny",
    "edit_rule",
];

// ===================================================================
// Rules tabs (rules/*.tsx)
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct WorkspaceTab {
    pub directories: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleList {
    pub rules: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RecentDenialsTab {
    pub recent: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoveWorkspaceDirectory {
    pub target: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleDescription {
    pub rule: String,
    pub humanized: String,
}

#[derive(Debug, Clone, Default)]
pub struct AddWorkspaceDirectory {
    pub draft: String,
}

#[derive(Debug, Clone, Default)]
pub struct AddPermissionRules {
    pub draft: String,
    pub destination: String,
}

/// Build option labels for "save destination" (project vs user vs managed).
pub fn option_for_permission_save_destination(
    has_project: bool,
    has_user: bool,
) -> Vec<PermissionPromptOption> {
    let mut opts = Vec::new();
    if has_project {
        opts.push(PermissionPromptOption {
            label: "Save to project settings".into(),
            value: "project".into(),
            description: Some(".mossen/settings.json in the repo".into()),
            hotkey: Some('p'),
        });
    }
    if has_user {
        opts.push(PermissionPromptOption {
            label: "Save to user settings".into(),
            value: "user".into(),
            description: Some("~/.mossen/settings.json".into()),
            hotkey: Some('u'),
        });
    }
    opts.push(PermissionPromptOption {
        label: "Just this session".into(),
        value: "session".into(),
        description: Some("Don't persist".into()),
        hotkey: Some('s'),
    });
    opts
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleInputProps {
    pub initial_value: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRuleInput {
    pub props: PermissionRuleInputProps,
    pub value: String,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SkillPermissionRequest {
    pub skill_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Default)]
pub struct EnterPlanModePermissionRequest {
    pub reason: String,
}

/// Permission feedback type — mirrors TS `type FeedbackType = 'accept' | 'reject'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackType {
    Accept,
    Reject,
}
