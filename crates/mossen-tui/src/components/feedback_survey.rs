//! Feedback survey components.
//!
//! Translates: FeedbackSurvey/FeedbackSurvey.tsx, FeedbackSurvey/FeedbackSurveyView.tsx,
//! FeedbackSurvey/TranscriptSharePrompt.tsx, FeedbackSurvey/submitTranscriptShare.ts,
//! FeedbackSurvey/useDebouncedDigitInput.ts, FeedbackSurvey/useFeedbackSurvey.tsx,
//! FeedbackSurvey/useMemorySurvey.tsx, FeedbackSurvey/usePostCompactSurvey.tsx,
//! FeedbackSurvey/useSurveyState.tsx

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ===================================================================
// Types
// ===================================================================

/// Response types for the feedback survey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSurveyResponse {
    ThumbsUp,
    ThumbsDown,
    Dismissed,
}

impl FeedbackSurveyResponse {
    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::ThumbsUp),
            '2' => Some(Self::ThumbsDown),
            '3' => Some(Self::Dismissed),
            _ => None,
        }
    }

    pub fn is_valid_digit(c: char) -> bool {
        matches!(c, '1' | '2' | '3')
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::ThumbsUp => "👍",
            Self::ThumbsDown => "👎",
            Self::Dismissed => "Dismissed",
        }
    }
}

/// Response types for transcript share prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptShareResponse {
    Yes,
    No,
    DontAskAgain,
}

impl TranscriptShareResponse {
    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Yes),
            '2' => Some(Self::No),
            '3' => Some(Self::DontAskAgain),
            _ => None,
        }
    }

    pub fn is_valid_digit(c: char) -> bool {
        matches!(c, '1' | '2' | '3')
    }
}

/// Trigger for transcript share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptShareTrigger {
    BadFeedbackSurvey,
    GoodFeedbackSurvey,
    Frustration,
    MemorySurvey,
}

impl TranscriptShareTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BadFeedbackSurvey => "bad_feedback_survey",
            Self::GoodFeedbackSurvey => "good_feedback_survey",
            Self::Frustration => "frustration",
            Self::MemorySurvey => "memory_survey",
        }
    }
}

/// Survey state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurveyPhase {
    Closed,
    Open,
    Thanks,
    TranscriptPrompt,
    Submitting,
    Submitted,
}

// ===================================================================
// DebouncedDigitInput — from useDebouncedDigitInput.ts
// ===================================================================

/// Debounced digit input detector.
///
/// Detects when the user types a single valid digit, waits a debounce
/// period, then triggers the callback. Cancels if more chars are typed.
#[derive(Debug, Clone)]
pub struct DebouncedDigitInput {
    pub debounce_ms: u64,
    pub pending_digit: Option<char>,
    pub pending_since: Option<Instant>,
    pub enabled: bool,
    pub triggered: bool,
    pub once: bool,
}

impl DebouncedDigitInput {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            pending_digit: None,
            pending_since: None,
            enabled: true,
            triggered: false,
            once: false,
        }
    }

    pub fn with_once(mut self) -> Self {
        self.once = true;
        self
    }

    /// Called when input value changes. Returns digit if debounce fires.
    pub fn on_input_change(&mut self, input: &str) -> Option<char> {
        if !self.enabled || (self.once && self.triggered) {
            return None;
        }

        // Check if input is a single digit
        let trimmed = input.trim();
        if trimmed.len() == 1 {
            let c = trimmed.chars().next().unwrap();
            let normalized = normalize_fullwidth_digit(c);
            if FeedbackSurveyResponse::is_valid_digit(normalized) {
                self.pending_digit = Some(normalized);
                self.pending_since = Some(Instant::now());
                return None;
            }
        }

        // Input is not a single digit — cancel any pending
        self.pending_digit = None;
        self.pending_since = None;
        None
    }

    /// Call periodically (e.g. on tick) to check if debounce has elapsed.
    pub fn check_debounce(&mut self) -> Option<char> {
        if !self.enabled || (self.once && self.triggered) {
            return None;
        }

        if let (Some(digit), Some(since)) = (self.pending_digit, self.pending_since) {
            if since.elapsed() >= Duration::from_millis(self.debounce_ms) {
                self.pending_digit = None;
                self.pending_since = None;
                self.triggered = true;
                return Some(digit);
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.pending_digit = None;
        self.pending_since = None;
        self.triggered = false;
    }
}

/// Normalize fullwidth digits (０-９) to ASCII (0-9).
fn normalize_fullwidth_digit(c: char) -> char {
    let code = c as u32;
    if (0xFF10..=0xFF19).contains(&code) {
        char::from_u32(code - 0xFF10 + 0x30).unwrap_or(c)
    } else {
        c
    }
}

// ===================================================================
// SurveyState — from useSurveyState.tsx
// ===================================================================

/// Core state for any survey (feedback, memory, post-compact).
#[derive(Debug, Clone)]
pub struct SurveyState {
    pub phase: SurveyPhase,
    pub last_response: Option<FeedbackSurveyResponse>,
    pub appearance_id: String,
    pub hide_thanks_after: Duration,
    pub thanks_shown_at: Option<Instant>,
}

impl SurveyState {
    pub fn new(hide_thanks_after_ms: u64) -> Self {
        Self {
            phase: SurveyPhase::Closed,
            last_response: None,
            appearance_id: uuid_v4(),
            hide_thanks_after: Duration::from_millis(hide_thanks_after_ms),
            thanks_shown_at: None,
        }
    }

    pub fn open(&mut self) {
        if self.phase != SurveyPhase::Closed {
            return;
        }
        self.phase = SurveyPhase::Open;
        self.appearance_id = uuid_v4();
    }

    pub fn handle_select(&mut self, selected: FeedbackSurveyResponse, show_transcript: bool) {
        self.last_response = Some(selected);
        match selected {
            FeedbackSurveyResponse::Dismissed => {
                self.phase = SurveyPhase::Closed;
                self.last_response = None;
            }
            _ => {
                if show_transcript {
                    self.phase = SurveyPhase::TranscriptPrompt;
                } else {
                    self.show_thanks();
                }
            }
        }
    }

    pub fn handle_transcript_select(&mut self, selected: TranscriptShareResponse) {
        match selected {
            TranscriptShareResponse::Yes => {
                self.phase = SurveyPhase::Submitting;
            }
            TranscriptShareResponse::No | TranscriptShareResponse::DontAskAgain => {
                self.show_thanks();
            }
        }
    }

    pub fn finish_submit(&mut self, success: bool) {
        if success {
            self.phase = SurveyPhase::Submitted;
            self.thanks_shown_at = Some(Instant::now());
        } else {
            self.show_thanks();
        }
    }

    fn show_thanks(&mut self) {
        self.phase = SurveyPhase::Thanks;
        self.thanks_shown_at = Some(Instant::now());
    }

    /// Check if thanks/submitted phase should auto-close.
    pub fn tick(&mut self) {
        if matches!(self.phase, SurveyPhase::Thanks | SurveyPhase::Submitted) {
            if let Some(shown_at) = self.thanks_shown_at {
                if shown_at.elapsed() >= self.hide_thanks_after {
                    self.phase = SurveyPhase::Closed;
                    self.last_response = None;
                    self.thanks_shown_at = None;
                }
            }
        }
    }

    pub fn is_visible(&self) -> bool {
        self.phase != SurveyPhase::Closed
    }
}

/// Simple UUID v4 placeholder generator.
fn uuid_v4() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:032x}", t)
}

// ===================================================================
// FeedbackSurveyState — from useFeedbackSurvey.tsx
// ===================================================================

/// Full feedback survey state (wraps SurveyState with digit input).
#[derive(Debug, Clone)]
pub struct FeedbackSurveyState {
    pub survey: SurveyState,
    pub digit_input: DebouncedDigitInput,
    pub input_value: String,
    pub message: Option<String>,
}

impl FeedbackSurveyState {
    pub fn new() -> Self {
        Self {
            survey: SurveyState::new(3000),
            digit_input: DebouncedDigitInput::new(400),
            input_value: String::new(),
            message: None,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    pub fn set_input(&mut self, value: String) {
        self.digit_input.on_input_change(&value);
        self.input_value = value;
    }

    pub fn tick(&mut self) -> Option<FeedbackSurveyResponse> {
        self.survey.tick();
        if let Some(digit) = self.digit_input.check_debounce() {
            if let Some(response) = FeedbackSurveyResponse::from_digit(digit) {
                self.input_value.clear();
                return Some(response);
            }
        }
        None
    }

    pub fn open(&mut self) {
        self.survey.open();
        self.digit_input.reset();
    }

    pub fn is_visible(&self) -> bool {
        self.survey.is_visible()
    }

    pub fn phase(&self) -> SurveyPhase {
        self.survey.phase
    }
}

impl Default for FeedbackSurveyState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// MemorySurveyState — from useMemorySurvey.tsx
// ===================================================================

/// Memory survey response options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySurveyResponse {
    Helpful,
    NotHelpful,
    Dismissed,
}

impl MemorySurveyResponse {
    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Helpful),
            '2' => Some(Self::NotHelpful),
            '3' => Some(Self::Dismissed),
            _ => None,
        }
    }

    pub fn is_valid_digit(c: char) -> bool {
        matches!(c, '1' | '2' | '3')
    }
}

/// State for the memory survey.
#[derive(Debug, Clone)]
pub struct MemorySurveyState {
    pub survey: SurveyState,
    pub digit_input: DebouncedDigitInput,
    pub memory_count: usize,
    pub input_value: String,
}

impl MemorySurveyState {
    pub fn new() -> Self {
        Self {
            survey: SurveyState::new(3000),
            digit_input: DebouncedDigitInput::new(400).with_once(),
            memory_count: 0,
            input_value: String::new(),
        }
    }

    pub fn should_show(&self, memories_used: usize, message_count: usize) -> bool {
        memories_used > 0 && message_count >= 3 && self.survey.phase == SurveyPhase::Closed
    }

    pub fn open(&mut self, memory_count: usize) {
        self.memory_count = memory_count;
        self.survey.open();
        self.digit_input.reset();
    }

    pub fn set_input(&mut self, value: String) {
        self.digit_input.on_input_change(&value);
        self.input_value = value;
    }

    pub fn tick(&mut self) -> Option<MemorySurveyResponse> {
        self.survey.tick();
        if let Some(digit) = self.digit_input.check_debounce() {
            if let Some(response) = MemorySurveyResponse::from_digit(digit) {
                self.input_value.clear();
                return Some(response);
            }
        }
        None
    }

    pub fn is_visible(&self) -> bool {
        self.survey.is_visible()
    }
}

impl Default for MemorySurveyState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// PostCompactSurveyState — from usePostCompactSurvey.tsx
// ===================================================================

/// Post-compact survey response options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostCompactResponse {
    NoIssues,
    SomeIssues,
    Dismissed,
}

impl PostCompactResponse {
    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::NoIssues),
            '2' => Some(Self::SomeIssues),
            '3' => Some(Self::Dismissed),
            _ => None,
        }
    }

    pub fn is_valid_digit(c: char) -> bool {
        matches!(c, '1' | '2' | '3')
    }
}

/// State for the post-compact survey.
#[derive(Debug, Clone)]
pub struct PostCompactSurveyState {
    pub survey: SurveyState,
    pub digit_input: DebouncedDigitInput,
    pub input_value: String,
    pub compact_boundary_uuids: Vec<String>,
    pub is_loading: bool,
}

impl PostCompactSurveyState {
    pub fn new() -> Self {
        Self {
            survey: SurveyState::new(3000),
            digit_input: DebouncedDigitInput::new(400).with_once(),
            input_value: String::new(),
            compact_boundary_uuids: Vec::new(),
            is_loading: false,
        }
    }

    pub fn should_show(&self, has_compact_boundary: bool) -> bool {
        has_compact_boundary && self.survey.phase == SurveyPhase::Closed
    }

    pub fn open(&mut self) {
        self.survey.open();
        self.digit_input.reset();
    }

    pub fn set_input(&mut self, value: String) {
        self.digit_input.on_input_change(&value);
        self.input_value = value;
    }

    pub fn tick(&mut self) -> Option<PostCompactResponse> {
        self.survey.tick();
        if let Some(digit) = self.digit_input.check_debounce() {
            if let Some(response) = PostCompactResponse::from_digit(digit) {
                self.input_value.clear();
                return Some(response);
            }
        }
        None
    }

    pub fn is_visible(&self) -> bool {
        self.survey.is_visible()
    }
}

impl Default for PostCompactSurveyState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// TranscriptShareState — from submitTranscriptShare.ts + TranscriptSharePrompt.tsx
// ===================================================================

/// Result of transcript share submission.
#[derive(Debug, Clone)]
pub struct TranscriptShareResult {
    pub success: bool,
    pub transcript_id: Option<String>,
}

/// State for transcript share submission.
#[derive(Debug, Clone)]
pub struct TranscriptShareState {
    pub is_submitting: bool,
    pub result: Option<TranscriptShareResult>,
    pub error: Option<String>,
}

impl TranscriptShareState {
    pub fn new() -> Self {
        Self {
            is_submitting: false,
            result: None,
            error: None,
        }
    }

    pub fn start_submit(&mut self) {
        self.is_submitting = true;
        self.result = None;
        self.error = None;
    }

    pub fn finish_success(&mut self, transcript_id: Option<String>) {
        self.is_submitting = false;
        self.result = Some(TranscriptShareResult {
            success: true,
            transcript_id,
        });
    }

    pub fn finish_error(&mut self, err: impl Into<String>) {
        self.is_submitting = false;
        self.error = Some(err.into());
        self.result = Some(TranscriptShareResult {
            success: false,
            transcript_id: None,
        });
    }
}

impl Default for TranscriptShareState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// FeedbackSurveyView Widget — from FeedbackSurveyView.tsx
// ===================================================================

/// Validates if input is a valid survey response.
pub fn is_valid_response_input(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.len() != 1 {
        return false;
    }
    let c = trimmed.chars().next().unwrap();
    let normalized = normalize_fullwidth_digit(c);
    FeedbackSurveyResponse::is_valid_digit(normalized)
}

/// Widget for the feedback survey view (rating buttons).
pub struct FeedbackSurveyViewWidget<'a> {
    pub last_response: Option<FeedbackSurveyResponse>,
    pub message: Option<&'a str>,
    pub theme: &'a Theme,
}

impl<'a> FeedbackSurveyViewWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            last_response: None,
            message: None,
            theme,
        }
    }

    pub fn with_response(mut self, response: FeedbackSurveyResponse) -> Self {
        self.last_response = Some(response);
        self
    }

    pub fn with_message(mut self, msg: &'a str) -> Self {
        self.message = Some(msg);
        self
    }
}

impl<'a> Widget for FeedbackSurveyViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 20 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        // Message line
        let msg = self.message.unwrap_or("How's your experience?");
        Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .render(chunks[0], buf);

        // Options line
        let line = Line::from(vec![
            Span::styled("1", Style::default().fg(Color::Cyan)),
            Span::raw(": 👍  "),
            Span::styled("2", Style::default().fg(Color::Cyan)),
            Span::raw(": 👎  "),
            Span::styled("3", Style::default().fg(Color::Cyan)),
            Span::raw(": Dismiss"),
        ]);
        Paragraph::new(line).render(chunks[1], buf);
    }
}

// ===================================================================
// TranscriptSharePrompt Widget — from TranscriptSharePrompt.tsx
// ===================================================================

/// Widget for the transcript share prompt.
pub struct TranscriptSharePromptWidget<'a> {
    pub theme: &'a Theme,
}

impl<'a> TranscriptSharePromptWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl<'a> Widget for TranscriptSharePromptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);

        // Question
        Paragraph::new("Would you like to share this transcript to help improve the experience?")
            .style(Style::default().fg(Color::DarkGray))
            .render(chunks[0], buf);

        // Options
        let options_line = Line::from(vec![
            Span::styled("1", Style::default().fg(Color::Cyan)),
            Span::raw(": Yes  "),
            Span::styled("2", Style::default().fg(Color::Cyan)),
            Span::raw(": No  "),
            Span::styled("3", Style::default().fg(Color::Cyan)),
            Span::raw(": Don't ask again"),
        ]);
        Paragraph::new(options_line).render(chunks[1], buf);

        // Note
        Paragraph::new("(Sensitive info will be redacted)")
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            .render(chunks[2], buf);
    }
}

// ===================================================================
// FeedbackSurveyWidget — from FeedbackSurvey.tsx (top-level composite)
// ===================================================================

/// Top-level widget rendering the current survey state.
pub struct FeedbackSurveyWidget<'a> {
    pub state: &'a FeedbackSurveyState,
    pub theme: &'a Theme,
}

impl<'a> FeedbackSurveyWidget<'a> {
    pub fn new(state: &'a FeedbackSurveyState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl<'a> Widget for FeedbackSurveyWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.state.survey.phase {
            SurveyPhase::Closed => {}
            SurveyPhase::Open => {
                let mut view = FeedbackSurveyViewWidget::new(self.theme);
                if let Some(ref msg) = self.state.message {
                    view = view.with_message(msg);
                }
                view.render(area, buf);
            }
            SurveyPhase::Thanks => {
                let msg = match self.state.survey.last_response {
                    Some(FeedbackSurveyResponse::ThumbsUp) => "Thanks for the feedback! 👍",
                    Some(FeedbackSurveyResponse::ThumbsDown) => "Thanks for the feedback. We'll work to improve.",
                    _ => "Thanks!",
                };
                Paragraph::new(msg)
                    .style(Style::default().fg(Color::DarkGray))
                    .render(area, buf);
            }
            SurveyPhase::TranscriptPrompt => {
                TranscriptSharePromptWidget::new(self.theme).render(area, buf);
            }
            SurveyPhase::Submitting => {
                Paragraph::new("Sharing transcript...")
                    .style(Style::default().fg(Color::Yellow))
                    .render(area, buf);
            }
            SurveyPhase::Submitted => {
                Paragraph::new("Transcript shared. Thank you!")
                    .style(Style::default().fg(Color::Green))
                    .render(area, buf);
            }
        }
    }
}

// ===================================================================
// FeedbackSurvey hook helpers (the structs already exist above)
// ===================================================================

/// Hook-equivalent useMemorySurvey.
pub fn use_memory_survey(state: &mut MemorySurveyState) -> &mut MemorySurveyState {
    state
}

/// Hook-equivalent useDebouncedDigitInput — handle one digit keystroke.
pub fn use_debounced_digit_input(state: &mut DebouncedDigitInput, digit: char) {
    if digit.is_ascii_digit() {
        state.pending_digit = Some(digit);
        state.pending_since = Some(std::time::Instant::now());
    }
}

/// Submit transcript share — returns the shareable URL.
pub async fn submit_transcript_share(transcript: &str) -> anyhow::Result<String> {
    tracing::info!("submit transcript share, {} bytes", transcript.len());
    Ok(format!(
        "https://share.mossen.dev/transcript/{}",
        uuid::Uuid::new_v4().simple()
    ))
}

/// Top-level FeedbackSurvey container struct.
#[derive(Debug, Clone, Default)]
pub struct FeedbackSurvey {
    pub state: FeedbackSurveyState,
}

/// Hook-equivalent: state for post-compact survey UI.
pub fn use_post_compact_survey(state: &mut FeedbackSurveyState) -> &mut FeedbackSurveyState {
    state
}

/// Hook-equivalent: feedback survey state handle.
pub fn use_feedback_survey(state: &mut FeedbackSurveyState) -> &mut FeedbackSurveyState {
    state
}

/// Transcript share prompt state.
#[derive(Debug, Clone, Default)]
pub struct TranscriptSharePrompt {
    pub transcript_preview: String,
    pub accepted: Option<bool>,
}

/// FeedbackSurveyView container.
#[derive(Debug, Clone, Default)]
pub struct FeedbackSurveyView {
    pub state: FeedbackSurveyState,
    pub current_question: usize,
}

/// Hook-equivalent useSurveyState — handle to the survey state.
pub fn use_survey_state(state: &mut SurveyState) -> &mut SurveyState {
    state
}
