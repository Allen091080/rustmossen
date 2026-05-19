//! Root-level large components (>300 lines each in TS source).
//! Covers: LogSelector, Stats, VirtualMessageList, ScrollKeybindingHandler,
//! Messages, MessageSelector, ConsoleOAuthFlow, Feedback, Spinner (root),
//! FullscreenLayout, Message, ContextVisualization, ModelPicker,
//! messageActions, MessageRow, DesktopHandoff, ResumeTask, ThemePicker,
//! StatusLine, TaskListV2, GlobalSearchDialog, RemoteEnvironmentDialog, MarkdownTable

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::theme::Theme;

// ─── AgenticSearch / LogSelector ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AgenticSearchStatus {
    Idle,
    Searching,
    Results { results: Vec<LogOption>, query: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct LogOption {
    pub session_id: String,
    pub title: String,
    pub modified: Instant,
    pub project_path: Option<String>,
    pub is_sidechain: bool,
    pub parent_session_id: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Snippet {
    pub before: String,
    pub match_text: String,
    pub after: String,
}

const PARENT_PREFIX_WIDTH: usize = 2;
const CHILD_PREFIX_WIDTH: usize = 4;
const DEEP_SEARCH_MAX_MESSAGES: usize = 2000;
const DEEP_SEARCH_CROP_SIZE: usize = 1000;
const DEEP_SEARCH_MAX_TEXT_LENGTH: usize = 50000;
const FUSE_THRESHOLD: f64 = 0.3;
const DATE_TIE_THRESHOLD_MS: u64 = 60_000;
const SNIPPET_CONTEXT_CHARS: usize = 50;

fn normalize_and_truncate(text: &str, max_width: usize) -> String {
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= max_width {
        normalized
    } else {
        let mut s = normalized[..max_width.saturating_sub(1)].to_string();
        s.push('…');
        s
    }
}

fn format_snippet(snippet: &Snippet, _highlight_color: Color) -> String {
    format!("{}{}{}",  snippet.before, snippet.match_text, snippet.after)
}

fn extract_snippet(text: &str, query: &str, context_chars: usize) -> Option<Snippet> {
    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let match_index = lower_text.find(&lower_query)?;
    let match_end = match_index + query.len();
    let snippet_start = match_index.saturating_sub(context_chars);
    let snippet_end = (match_end + context_chars).min(text.len());
    let before_raw = &text[snippet_start..match_index];
    let match_text = &text[match_index..match_end];
    let after_raw = &text[match_end..snippet_end];
    let before = if snippet_start > 0 {
        format!("…{}", before_raw.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        before_raw.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let after = if snippet_end < text.len() {
        format!("{}…", after_raw.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        after_raw.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    Some(Snippet { before, match_text: match_text.to_string(), after })
}

#[derive(Debug, Clone)]
pub struct LogTreeNode {
    pub log: LogOption,
    pub index_in_filtered: usize,
    pub children: Vec<LogTreeNode>,
    pub is_expanded: bool,
}

fn build_log_label(log: &LogOption, max_label_width: usize, is_group_header: bool, is_child: bool, fork_count: usize) -> String {
    let prefix_width = if is_group_header && fork_count > 0 {
        PARENT_PREFIX_WIDTH
    } else if is_child {
        CHILD_PREFIX_WIDTH
    } else {
        0
    };
    let session_count_suffix = if is_group_header && fork_count > 0 {
        let word = if fork_count == 1 { "session" } else { "sessions" };
        format!(" (+{} other {})", fork_count, word)
    } else {
        String::new()
    };
    let sidechain_suffix = if log.is_sidechain { " (sidechain)" } else { "" };
    let max_summary_width = max_label_width.saturating_sub(prefix_width + sidechain_suffix.len() + session_count_suffix.len());
    let truncated = normalize_and_truncate(&log.title, max_summary_width);
    format!("{}{}{}", truncated, sidechain_suffix, session_count_suffix)
}

fn build_stale_suffix(modified: Instant) -> String {
    let elapsed = modified.elapsed();
    let days = elapsed.as_secs() / 86400;
    if days < 14 {
        String::new()
    } else {
        format!(" · stale {}d", days)
    }
}

fn build_log_metadata(log: &LogOption, is_child: bool, show_project_path: bool, current_cwd: Option<&str>, show_worktree_path: bool) -> String {
    let child_padding = if is_child { "    " } else { "" };
    let project_suffix = if show_project_path {
        log.project_path.as_deref().map(|p| format!(" · {}", p)).unwrap_or_default()
    } else {
        String::new()
    };
    let stale_suffix = build_stale_suffix(log.modified);
    format!("{}{}{}{}", child_padding, log.session_id, project_suffix, stale_suffix)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogViewMode {
    List,
    Search,
    Tree,
    Preview,
}

pub struct LogSelectorState {
    pub logs: Vec<LogOption>,
    pub max_height: usize,
    pub force_width: Option<usize>,
    pub search_query: String,
    pub filtered_logs: Vec<LogOption>,
    pub focused_index: usize,
    pub view_mode: LogViewMode,
    pub preview_log: Option<LogOption>,
    pub current_branch: Option<String>,
    pub branch_filter_enabled: bool,
    pub show_all_worktrees: bool,
    pub has_multiple_worktrees: bool,
    pub rename_value: String,
    pub rename_cursor_offset: usize,
    pub expanded_group_session_ids: HashSet<String>,
    pub selected_tag_index: usize,
    pub agentic_search_state: AgenticSearchStatus,
    pub show_all_projects: bool,
    pub tree_nodes: Vec<LogTreeNode>,
    pub columns: u16,
}

impl LogSelectorState {
    pub fn new(logs: Vec<LogOption>, columns: u16) -> Self {
        let filtered_logs = logs.clone();
        Self {
            logs,
            max_height: usize::MAX,
            force_width: None,
            search_query: String::new(),
            filtered_logs,
            focused_index: 0,
            view_mode: LogViewMode::List,
            preview_log: None,
            current_branch: None,
            branch_filter_enabled: false,
            show_all_worktrees: false,
            has_multiple_worktrees: false,
            rename_value: String::new(),
            rename_cursor_offset: 0,
            expanded_group_session_ids: HashSet::new(),
            selected_tag_index: 0,
            agentic_search_state: AgenticSearchStatus::Idle,
            show_all_projects: false,
            tree_nodes: Vec::new(),
            columns,
        }
    }

    pub fn set_search_query(&mut self, query: String) {
        self.search_query = query.clone();
        if query.is_empty() {
            self.filtered_logs = self.logs.clone();
        } else {
            let lower_query = query.to_lowercase();
            self.filtered_logs = self.logs.iter()
                .filter(|log| log.title.to_lowercase().contains(&lower_query))
                .cloned()
                .collect();
        }
        self.focused_index = 0;
    }

    pub fn focus_next(&mut self) {
        if !self.filtered_logs.is_empty() {
            self.focused_index = (self.focused_index + 1).min(self.filtered_logs.len() - 1);
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn select_current(&self) -> Option<&LogOption> {
        self.filtered_logs.get(self.focused_index)
    }

    pub fn toggle_branch_filter(&mut self) {
        self.branch_filter_enabled = !self.branch_filter_enabled;
        self.apply_filters();
    }

    pub fn toggle_all_projects(&mut self) {
        self.show_all_projects = !self.show_all_projects;
        self.apply_filters();
    }

    fn apply_filters(&mut self) {
        let lower_query = self.search_query.to_lowercase();
        self.filtered_logs = self.logs.iter()
            .filter(|log| {
                if !self.search_query.is_empty() && !log.title.to_lowercase().contains(&lower_query) {
                    return false;
                }
                if self.branch_filter_enabled {
                    if let Some(ref branch) = self.current_branch {
                        if !log.tags.contains(branch) {
                            return false;
                        }
                    }
                }
                true
            })
            .cloned()
            .collect();
        self.focused_index = 0;
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            LogViewMode::List => LogViewMode::Search,
            LogViewMode::Search => LogViewMode::List,
            LogViewMode::Tree => LogViewMode::List,
            LogViewMode::Preview => LogViewMode::List,
        };
    }

    pub fn set_preview(&mut self, log: Option<LogOption>) {
        self.preview_log = log;
        if self.preview_log.is_some() {
            self.view_mode = LogViewMode::Preview;
        }
    }

    pub fn expand_group(&mut self, session_id: String) {
        self.expanded_group_session_ids.insert(session_id);
    }

    pub fn collapse_group(&mut self, session_id: &str) {
        self.expanded_group_session_ids.remove(session_id);
    }
}

pub struct LogSelectorWidget<'a> {
    pub state: &'a LogSelectorState,
    pub theme: &'a Theme,
}

impl<'a> Widget for LogSelectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

        // Search input area
        let search_block = Block::default().borders(Borders::BOTTOM);
        let search_text = if self.state.search_query.is_empty() {
            "Search sessions..."
        } else {
            &self.state.search_query
        };
        let search_para = Paragraph::new(search_text)
            .block(search_block)
            .style(Style::default().fg(Color::White));
        search_para.render(layout[0], buf);

        // List area
        let visible_height = layout[1].height as usize;
        let start_idx = if self.state.focused_index >= visible_height {
            self.state.focused_index - visible_height + 1
        } else {
            0
        };
        let end_idx = (start_idx + visible_height).min(self.state.filtered_logs.len());

        for (i, log) in self.state.filtered_logs[start_idx..end_idx].iter().enumerate() {
            let y = layout[1].y + i as u16;
            if y >= layout[1].y + layout[1].height {
                break;
            }
            let is_focused = start_idx + i == self.state.focused_index;
            let label = build_log_label(log, area.width as usize - 4, false, false, 0);
            let style = if is_focused {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(layout[1].x + 1, y, &label, style);
        }

        // Footer with count
        let footer_text = format!("{} sessions", self.state.filtered_logs.len());
        buf.set_string(layout[2].x + 1, layout[2].y, &footer_text, Style::default().fg(Color::DarkGray));
    }
}

// ─── Stats ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsDateRange {
    SevenDays,
    ThirtyDays,
    AllTime,
}

impl StatsDateRange {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SevenDays => "Last 7 days",
            Self::ThirtyDays => "Last 30 days",
            Self::AllTime => "All time",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::AllTime => Self::SevenDays,
            Self::SevenDays => Self::ThirtyDays,
            Self::ThirtyDays => Self::AllTime,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DailyModelTokens {
    pub date: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct MossenStats {
    pub total_sessions: u64,
    pub total_messages: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_duration_secs: f64,
    pub total_api_duration_secs: f64,
    pub total_cost_usd: f64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub daily_model_tokens: Vec<DailyModelTokens>,
    pub peak_day: Option<String>,
    pub heatmap_data: Vec<(String, u64)>,
}

#[derive(Debug, Clone)]
pub enum StatsResult {
    Success(MossenStats),
    Error(String),
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsTab {
    Overview,
    Heatmap,
    Chart,
}

pub struct StatsState {
    pub date_range: StatsDateRange,
    pub active_tab: StatsTab,
    pub result: StatsResult,
    pub copied: bool,
}

impl StatsState {
    pub fn new() -> Self {
        Self {
            date_range: StatsDateRange::AllTime,
            active_tab: StatsTab::Overview,
            result: StatsResult::Empty,
            copied: false,
        }
    }

    pub fn cycle_date_range(&mut self) {
        self.date_range = self.date_range.next();
    }

    pub fn set_tab(&mut self, tab: StatsTab) {
        self.active_tab = tab;
    }

    pub fn set_result(&mut self, result: StatsResult) {
        self.result = result;
    }

    pub fn copy_to_clipboard(&mut self) {
        self.copied = true;
    }
}

/// Format an ISO-8601 date string (`YYYY-MM-DD…`) as the bare `MM-DD` segment
/// used by the cost-by-day panel. The slice indices match the TS port which
/// just uses `date.slice(5, 10)`; we deliberately keep this byte-slice (not a
/// full chrono parse) because the input is always emitted by our own
/// serialiser in canonical RFC-3339, never user input — adding parsing would
/// trade cycles for nothing.
fn format_peak_day(date_str: &str) -> String {
    if date_str.len() >= 10 {
        date_str[5..10].to_string()
    } else {
        date_str.to_string()
    }
}

fn format_duration_human(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        format!("{:.0}m {:.0}s", secs / 60.0, secs % 60.0)
    } else {
        format!("{:.0}h {:.0}m", secs / 3600.0, (secs % 3600.0) / 60.0)
    }
}

fn format_number_compact(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

fn generate_ascii_chart(data: &[f64], width: usize, height: usize) -> Vec<String> {
    if data.is_empty() {
        return vec![String::new(); height];
    }
    let max_val = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let range = if (max_val - min_val).abs() < f64::EPSILON { 1.0 } else { max_val - min_val };
    let mut lines = vec![vec![' '; width]; height];
    let step = if data.len() > width { data.len() / width } else { 1 };
    let sampled: Vec<f64> = data.iter().step_by(step).take(width).cloned().collect();
    for (x, &val) in sampled.iter().enumerate() {
        let normalized = ((val - min_val) / range * (height as f64 - 1.0)) as usize;
        let y = height - 1 - normalized.min(height - 1);
        if x < width && y < height {
            lines[y][x] = '█';
        }
    }
    lines.into_iter().map(|row| row.into_iter().collect()).collect()
}

fn generate_heatmap(data: &[(String, u64)], width: usize) -> Vec<String> {
    if data.is_empty() {
        return Vec::new();
    }
    let max_val = data.iter().map(|(_, v)| *v).max().unwrap_or(1);
    let chars = ['░', '▒', '▓', '█'];
    let mut lines = Vec::new();
    for chunk in data.chunks(width) {
        let line: String = chunk.iter().map(|(_, v)| {
            if *v == 0 {
                ' '
            } else {
                let idx = ((*v as f64 / max_val as f64) * 3.0) as usize;
                chars[idx.min(3)]
            }
        }).collect();
        lines.push(line);
    }
    lines
}

pub struct StatsWidget<'a> {
    pub state: &'a StatsState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StatsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Stats ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match &self.state.result {
            StatsResult::Empty => {
                let text = Paragraph::new("No stats data available.")
                    .style(Style::default().fg(Color::DarkGray));
                text.render(inner, buf);
            }
            StatsResult::Error(msg) => {
                let text = Paragraph::new(msg.as_str())
                    .style(Style::default().fg(Color::Red));
                text.render(inner, buf);
            }
            StatsResult::Success(stats) => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Min(1),
                    ])
                    .split(inner);

                // Tab bar
                let tabs = vec![
                    ("Overview", StatsTab::Overview),
                    ("Heatmap", StatsTab::Heatmap),
                    ("Chart", StatsTab::Chart),
                ];
                let tab_line: Vec<Span> = tabs.iter().map(|(label, tab)| {
                    if *tab == self.state.active_tab {
                        Span::styled(format!(" {} ", label), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                    } else {
                        Span::styled(format!(" {} ", label), Style::default().fg(Color::DarkGray))
                    }
                }).collect();
                let tabs_para = Paragraph::new(Line::from(tab_line));
                tabs_para.render(layout[0], buf);

                // Date range
                let range_text = format!("Range: {} [r to cycle]", self.state.date_range.label());
                buf.set_string(layout[1].x, layout[1].y, &range_text, Style::default().fg(Color::DarkGray));

                // Content area
                match self.state.active_tab {
                    StatsTab::Overview => {
                        let lines = vec![
                            format!("Sessions: {}", stats.total_sessions),
                            format!("Messages: {}", stats.total_messages),
                            format!("Input tokens: {}", format_number_compact(stats.total_input_tokens)),
                            format!("Output tokens: {}", format_number_compact(stats.total_output_tokens)),
                            format!("Total cost: ${:.2}", stats.total_cost_usd),
                            format!("Total duration: {}", format_duration_human(stats.total_duration_secs)),
                            format!("API duration: {}", format_duration_human(stats.total_api_duration_secs)),
                            format!("Lines added: +{}", stats.total_lines_added),
                            format!("Lines removed: -{}", stats.total_lines_removed),
                        ];
                        let text = lines.join("\n");
                        let para = Paragraph::new(text).style(Style::default().fg(Color::White));
                        para.render(layout[2], buf);
                    }
                    StatsTab::Heatmap => {
                        let heatmap_lines = generate_heatmap(&stats.heatmap_data, layout[2].width as usize);
                        for (i, line) in heatmap_lines.iter().enumerate() {
                            let y = layout[2].y + i as u16;
                            if y < layout[2].y + layout[2].height {
                                buf.set_string(layout[2].x, y, line, Style::default().fg(Color::Green));
                            }
                        }
                    }
                    StatsTab::Chart => {
                        let chart_data: Vec<f64> = stats.daily_model_tokens.iter()
                            .map(|d| (d.input_tokens + d.output_tokens) as f64)
                            .collect();
                        let chart_lines = generate_ascii_chart(&chart_data, layout[2].width as usize, layout[2].height as usize);
                        for (i, line) in chart_lines.iter().enumerate() {
                            let y = layout[2].y + i as u16;
                            if y < layout[2].y + layout[2].height {
                                buf.set_string(layout[2].x, y, line, Style::default().fg(Color::Yellow));
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── VirtualMessageList ────────────────────────────────────────────────────

const HEADROOM: usize = 3;
const STICKY_TEXT_CAP: usize = 500;

#[derive(Debug, Clone)]
pub struct StickyPrompt {
    pub text: String,
    pub scroll_to_index: usize,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub message_index: usize,
    pub char_offset: usize,
}

pub struct VirtualMessageListState {
    pub scroll_top: usize,
    pub viewport_height: usize,
    pub total_height: usize,
    pub height_cache: HashMap<String, u16>,
    pub columns: u16,
    pub sticky_prompt: Option<StickyPrompt>,
    pub search_query: String,
    pub search_matches: Vec<SearchMatch>,
    pub current_match_index: usize,
    pub anchor_scroll_top: Option<usize>,
    pub selected_index: Option<usize>,
    pub is_sticky: bool,
}

impl VirtualMessageListState {
    pub fn new(columns: u16, viewport_height: usize) -> Self {
        Self {
            scroll_top: 0,
            viewport_height,
            total_height: 0,
            height_cache: HashMap::new(),
            columns,
            sticky_prompt: None,
            search_query: String::new(),
            search_matches: Vec::new(),
            current_match_index: 0,
            anchor_scroll_top: None,
            selected_index: None,
            is_sticky: true,
        }
    }

    pub fn jump_to_index(&mut self, index: usize) {
        let target_top = index.saturating_sub(HEADROOM);
        self.scroll_top = target_top;
        self.is_sticky = false;
    }

    pub fn set_search_query(&mut self, query: String, messages: &[String]) {
        self.search_query = query.clone();
        self.search_matches.clear();
        self.current_match_index = 0;
        if query.is_empty() {
            if let Some(anchor) = self.anchor_scroll_top.take() {
                self.scroll_top = anchor;
            }
            return;
        }
        let lower_query = query.to_lowercase();
        for (i, msg) in messages.iter().enumerate() {
            let lower_msg = msg.to_lowercase();
            let mut start = 0;
            while let Some(pos) = lower_msg[start..].find(&lower_query) {
                self.search_matches.push(SearchMatch {
                    message_index: i,
                    char_offset: start + pos,
                });
                start += pos + lower_query.len();
            }
        }
        if !self.search_matches.is_empty() {
            self.jump_to_match(0);
        }
    }

    pub fn next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.current_match_index = (self.current_match_index + 1) % self.search_matches.len();
        self.jump_to_match(self.current_match_index);
    }

    pub fn prev_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.current_match_index = if self.current_match_index == 0 {
            self.search_matches.len() - 1
        } else {
            self.current_match_index - 1
        };
        self.jump_to_match(self.current_match_index);
    }

    fn jump_to_match(&mut self, match_idx: usize) {
        if let Some(m) = self.search_matches.get(match_idx) {
            self.jump_to_index(m.message_index);
        }
    }

    pub fn set_anchor(&mut self) {
        self.anchor_scroll_top = Some(self.scroll_top);
    }

    pub fn disarm_search(&mut self) {
        self.search_matches.clear();
        self.current_match_index = 0;
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta > 0 {
            self.scroll_top = (self.scroll_top + delta as usize).min(self.total_height.saturating_sub(self.viewport_height));
        } else {
            self.scroll_top = self.scroll_top.saturating_sub((-delta) as usize);
        }
        self.is_sticky = self.scroll_top + self.viewport_height >= self.total_height;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_top = self.total_height.saturating_sub(self.viewport_height);
        self.is_sticky = true;
    }

    pub fn invalidate_heights(&mut self) {
        self.height_cache.clear();
    }

    pub fn match_status(&self) -> (usize, usize) {
        (self.search_matches.len(), if self.search_matches.is_empty() { 0 } else { self.current_match_index + 1 })
    }
}

pub struct VirtualMessageListWidget<'a> {
    pub state: &'a VirtualMessageListState,
    pub messages: &'a [String],
    pub theme: &'a Theme,
}

impl<'a> Widget for VirtualMessageListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let visible_start = self.state.scroll_top;
        let visible_end = (visible_start + area.height as usize).min(self.messages.len());
        for (i, msg) in self.messages[visible_start..visible_end].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let style = if Some(visible_start + i) == self.state.selected_index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            let display = if msg.len() > area.width as usize {
                &msg[..area.width as usize]
            } else {
                msg
            };
            buf.set_string(area.x, y, display, style);
        }
    }
}

// ─── ScrollKeybindingHandler ───────────────────────────────────────────────

const WHEEL_ACCEL_WINDOW_MS: u64 = 40;
const WHEEL_ACCEL_STEP: f64 = 0.3;
const WHEEL_ACCEL_MAX: f64 = 6.0;
const WHEEL_BOUNCE_GAP_MAX_MS: u64 = 200;
const WHEEL_MODE_STEP: f64 = 15.0;
const WHEEL_MODE_CAP: f64 = 15.0;
const WHEEL_MODE_RAMP: f64 = 3.0;
const WHEEL_MODE_IDLE_DISENGAGE_MS: u64 = 1500;
const WHEEL_DECAY_HALFLIFE_MS: f64 = 150.0;
const WHEEL_DECAY_STEP: f64 = 5.0;
const WHEEL_BURST_MS: u64 = 5;
const WHEEL_DECAY_GAP_MS: u64 = 80;
const WHEEL_DECAY_CAP_SLOW: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct ScrollAccelState {
    pub last_event_time: Option<Instant>,
    pub last_direction: i8,
    pub multiplier: f64,
    pub wheel_mode: bool,
    pub bounce_count: u32,
    pub momentum: f64,
}

impl ScrollAccelState {
    pub fn new() -> Self {
        Self {
            last_event_time: None,
            last_direction: 0,
            multiplier: 1.0,
            wheel_mode: false,
            bounce_count: 0,
            momentum: 0.0,
        }
    }

    pub fn compute_scroll_delta(&mut self, direction: i8, is_xterm: bool) -> i32 {
        let now = Instant::now();
        let gap_ms = self.last_event_time
            .map(|t| now.duration_since(t).as_millis() as u64)
            .unwrap_or(u64::MAX);

        // Bounce detection
        if direction != 0 && self.last_direction != 0 && direction != self.last_direction && gap_ms < WHEEL_BOUNCE_GAP_MAX_MS {
            self.bounce_count += 1;
            if self.bounce_count >= 2 {
                self.wheel_mode = true;
            }
        } else if direction == self.last_direction {
            self.bounce_count = 0;
        }

        // Idle disengage
        if gap_ms > WHEEL_MODE_IDLE_DISENGAGE_MS {
            self.wheel_mode = false;
            self.multiplier = 1.0;
            self.momentum = 0.0;
        }

        let delta = if is_xterm {
            // xterm.js exponential decay path
            if gap_ms < WHEEL_BURST_MS {
                // Same-batch burst: 1 row per event
                1
            } else {
                self.momentum = 0.5_f64.powf(gap_ms as f64 / WHEEL_DECAY_HALFLIFE_MS);
                let target = 1.0 + WHEEL_DECAY_STEP * self.momentum;
                let cap = if gap_ms >= WHEEL_DECAY_GAP_MS { WHEEL_DECAY_CAP_SLOW } else { 6.0 };
                target.min(cap) as i32
            }
        } else if self.wheel_mode {
            // Mouse wheel mode with bounce detection
            let m = 0.5_f64.powf(gap_ms as f64 / WHEEL_DECAY_HALFLIFE_MS);
            let target = 1.0 + WHEEL_MODE_STEP * m;
            let growth = (target - self.multiplier).min(WHEEL_MODE_RAMP);
            self.multiplier = (self.multiplier + growth).min(WHEEL_MODE_CAP);
            self.multiplier as i32
        } else {
            // Native terminal linear ramp
            if gap_ms < WHEEL_ACCEL_WINDOW_MS {
                self.multiplier = (self.multiplier + WHEEL_ACCEL_STEP).min(WHEEL_ACCEL_MAX);
            } else {
                self.multiplier = 1.0;
            }
            self.multiplier as i32
        };

        self.last_event_time = Some(now);
        self.last_direction = direction;
        delta * direction as i32
    }
}

pub struct ScrollKeybindingState {
    pub is_active: bool,
    pub is_modal: bool,
    pub selection_active: bool,
    pub selection_start: Option<(u16, u16)>,
    pub selection_end: Option<(u16, u16)>,
    pub accel: ScrollAccelState,
}

impl ScrollKeybindingState {
    pub fn new(is_modal: bool) -> Self {
        Self {
            is_active: true,
            is_modal,
            selection_active: false,
            selection_start: None,
            selection_end: None,
            accel: ScrollAccelState::new(),
        }
    }

    pub fn handle_scroll_up(&mut self, is_xterm: bool) -> i32 {
        self.accel.compute_scroll_delta(-1, is_xterm)
    }

    pub fn handle_scroll_down(&mut self, is_xterm: bool) -> i32 {
        self.accel.compute_scroll_delta(1, is_xterm)
    }

    pub fn handle_page_up(&self, viewport_height: usize) -> i32 {
        -(viewport_height as i32 / 2)
    }

    pub fn handle_page_down(&self, viewport_height: usize) -> i32 {
        viewport_height as i32 / 2
    }

    pub fn handle_half_page_up(&self, viewport_height: usize) -> i32 {
        -(viewport_height as i32 / 4)
    }

    pub fn handle_half_page_down(&self, viewport_height: usize) -> i32 {
        viewport_height as i32 / 4
    }

    pub fn start_selection(&mut self, x: u16, y: u16) {
        self.selection_active = true;
        self.selection_start = Some((x, y));
        self.selection_end = Some((x, y));
    }

    pub fn update_selection(&mut self, x: u16, y: u16) {
        if self.selection_active {
            self.selection_end = Some((x, y));
        }
    }

    pub fn end_selection(&mut self) {
        self.selection_active = false;
    }

    pub fn clear_selection(&mut self) {
        self.selection_active = false;
        self.selection_start = None;
        self.selection_end = None;
    }
}

// ─── Messages (root component) ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderableMessageType {
    User,
    Assistant,
    System,
    Progress,
    ToolUse,
    ToolResult,
    CollapsedReadSearch,
    Thinking,
    Meta,
}

#[derive(Debug, Clone)]
pub struct RenderableMessage {
    pub uuid: String,
    pub message_type: RenderableMessageType,
    pub content: String,
    pub tool_use_id: Option<String>,
    pub is_meta: bool,
    pub is_api_error: bool,
    pub timestamp: Option<Instant>,
    pub model: Option<String>,
    pub thinking_content: Option<String>,
}

pub struct MessagesState {
    pub renderable_messages: Vec<RenderableMessage>,
    pub verbose: bool,
    pub in_progress_tool_use_ids: HashSet<String>,
    pub streaming_tool_use_ids: HashSet<String>,
    pub last_thinking_block_id: Option<String>,
    pub latest_bash_output_uuid: Option<String>,
    pub columns: u16,
    pub is_loading: bool,
    pub can_animate: bool,
    pub show_logo: bool,
}

impl MessagesState {
    pub fn new(columns: u16) -> Self {
        Self {
            renderable_messages: Vec::new(),
            verbose: false,
            in_progress_tool_use_ids: HashSet::new(),
            streaming_tool_use_ids: HashSet::new(),
            last_thinking_block_id: None,
            latest_bash_output_uuid: None,
            columns,
            is_loading: false,
            can_animate: true,
            show_logo: true,
        }
    }

    pub fn set_messages(&mut self, messages: Vec<RenderableMessage>) {
        self.renderable_messages = messages;
    }

    pub fn toggle_verbose(&mut self) {
        self.verbose = !self.verbose;
    }

    pub fn should_render_statically(msg: &RenderableMessage, in_progress: &HashSet<String>, streaming: &HashSet<String>) -> bool {
        if msg.is_meta {
            return true;
        }
        if let Some(ref tool_id) = msg.tool_use_id {
            if in_progress.contains(tool_id) || streaming.contains(tool_id) {
                return false;
            }
        }
        true
    }

    pub fn has_content_after_index(messages: &[RenderableMessage], index: usize, streaming_ids: &HashSet<String>) -> bool {
        messages[index + 1..].iter().any(|m| {
            if m.is_meta {
                return false;
            }
            if m.message_type == RenderableMessageType::ToolResult {
                return false;
            }
            if let Some(ref id) = m.tool_use_id {
                if streaming_ids.contains(id) {
                    return false;
                }
            }
            true
        })
    }
}

pub struct MessagesWidget<'a> {
    pub state: &'a MessagesState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessagesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let visible_count = area.height as usize;
        let start = self.state.renderable_messages.len().saturating_sub(visible_count);
        for (i, msg) in self.state.renderable_messages[start..].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let (prefix, style) = match msg.message_type {
                RenderableMessageType::User => ("❯ ", Style::default().fg(Color::Cyan)),
                RenderableMessageType::Assistant => ("⏎ ", Style::default().fg(Color::White)),
                RenderableMessageType::System => ("ℹ ", Style::default().fg(Color::DarkGray)),
                RenderableMessageType::Progress => ("⟳ ", Style::default().fg(Color::Yellow)),
                RenderableMessageType::ToolUse => ("⚙ ", Style::default().fg(Color::Magenta)),
                RenderableMessageType::ToolResult => ("✓ ", Style::default().fg(Color::Green)),
                RenderableMessageType::CollapsedReadSearch => ("… ", Style::default().fg(Color::DarkGray)),
                RenderableMessageType::Thinking => ("💭 ", Style::default().fg(Color::Blue)),
                RenderableMessageType::Meta => ("", Style::default().fg(Color::DarkGray)),
            };
            let max_w = (area.width as usize).saturating_sub(prefix.len() + 1);
            let content = if msg.content.len() > max_w {
                &msg.content[..max_w]
            } else {
                &msg.content
            };
            buf.set_string(area.x, y, prefix, style);
            buf.set_string(area.x + prefix.len() as u16, y, content, style);
        }
    }
}

// ─── MessageSelector ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOption {
    Both,
    Conversation,
    Code,
    Summarize,
    SummarizeUpTo,
    Nevermind,
}

impl RestoreOption {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Both => "Restore code and conversation",
            Self::Conversation => "Restore conversation",
            Self::Code => "Restore code",
            Self::Summarize => "Summarize from here",
            Self::SummarizeUpTo => "Summarize up to here",
            Self::Nevermind => "Never mind",
        }
    }

    pub fn is_summarize(&self) -> bool {
        matches!(self, Self::Summarize | Self::SummarizeUpTo)
    }
}

#[derive(Debug, Clone)]
pub struct DiffStats {
    pub files_changed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

#[derive(Debug)]
pub struct MessageSelectorState {
    pub messages: Vec<RenderableMessage>,
    pub selected_index: usize,
    pub message_to_restore: Option<usize>,
    pub diff_stats: Option<DiffStats>,
    pub is_restoring: bool,
    pub restoring_option: Option<RestoreOption>,
    pub selected_restore_option: RestoreOption,
    pub summarize_feedback: String,
    pub error: Option<String>,
    pub file_history_enabled: bool,
}

impl MessageSelectorState {
    pub fn new(messages: Vec<RenderableMessage>, file_history_enabled: bool) -> Self {
        let selected_index = messages.len().saturating_sub(1);
        Self {
            messages,
            selected_index,
            message_to_restore: None,
            diff_stats: None,
            is_restoring: false,
            restoring_option: None,
            selected_restore_option: RestoreOption::Both,
            summarize_feedback: String::new(),
            error: None,
            file_history_enabled,
        }
    }

    pub fn focus_next(&mut self) {
        if self.selected_index < self.messages.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn focus_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn confirm_selection(&mut self) {
        self.message_to_restore = Some(self.selected_index);
    }

    pub fn set_restore_option(&mut self, option: RestoreOption) {
        self.selected_restore_option = option;
    }

    pub fn start_restore(&mut self) {
        self.is_restoring = true;
        self.restoring_option = Some(self.selected_restore_option.clone());
    }

    pub fn back(&mut self) {
        if self.message_to_restore.is_some() {
            self.message_to_restore = None;
        }
    }

    pub fn get_restore_options(&self, can_restore_code: bool) -> Vec<RestoreOption> {
        let mut options = if can_restore_code {
            vec![RestoreOption::Both, RestoreOption::Conversation, RestoreOption::Code]
        } else {
            vec![RestoreOption::Conversation]
        };
        options.push(RestoreOption::Summarize);
        options.push(RestoreOption::SummarizeUpTo);
        options.push(RestoreOption::Nevermind);
        options
    }
}

pub struct MessageSelectorWidget<'a> {
    pub state: &'a MessageSelectorState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessageSelectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Select Message ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.message_to_restore.is_some() {
            // Show restore options
            let options = self.state.get_restore_options(self.state.file_history_enabled);
            for (i, opt) in options.iter().enumerate() {
                let y = inner.y + i as u16;
                if y >= inner.y + inner.height {
                    break;
                }
                let is_selected = *opt == self.state.selected_restore_option;
                let prefix = if is_selected { "▸ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                buf.set_string(inner.x, y, &format!("{}{}", prefix, opt.label()), style);
            }
        } else {
            // Show message list
            let max_visible = 7usize;
            let first_visible = self.state.selected_index.saturating_sub(max_visible / 2)
                .min(self.state.messages.len().saturating_sub(max_visible));
            let end_visible = (first_visible + max_visible).min(self.state.messages.len());
            for (i, msg) in self.state.messages[first_visible..end_visible].iter().enumerate() {
                let y = inner.y + i as u16;
                if y >= inner.y + inner.height {
                    break;
                }
                let is_selected = first_visible + i == self.state.selected_index;
                let prefix = if is_selected { "▸ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                let content = if msg.content.len() > (area.width as usize - 4) {
                    &msg.content[..(area.width as usize - 4)]
                } else {
                    &msg.content
                };
                buf.set_string(inner.x, y, &format!("{}{}", prefix, content), style);
            }
        }
    }
}

// ─── ConsoleOAuthFlow ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthState {
    Idle,
    PlatformSetup,
    ReadyToStart,
    WaitingForLogin { url: String },
    CreatingApiKey,
    AboutToRetry,
    Success { token: Option<String> },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginMethod {
    Hosted,
    Console,
}

pub struct ConsoleOAuthFlowState {
    pub oauth_status: OAuthState,
    pub login_method: Option<LoginMethod>,
    pub pasted_code: String,
    pub cursor_offset: usize,
    pub show_paste_hint: bool,
    pub starting_message: Option<String>,
    pub force_login_method: Option<LoginMethod>,
    pub login_with_hosted_account: bool,
}

impl ConsoleOAuthFlowState {
    pub fn new(mode: &str, force_login_method: Option<LoginMethod>) -> Self {
        let initial_status = if mode == "setup-token" {
            OAuthState::ReadyToStart
        } else if force_login_method.is_some() {
            OAuthState::ReadyToStart
        } else {
            OAuthState::Idle
        };
        let login_with_hosted = mode == "setup-token" || matches!(force_login_method, Some(LoginMethod::Hosted));
        Self {
            oauth_status: initial_status,
            login_method: force_login_method.clone(),
            pasted_code: String::new(),
            cursor_offset: 0,
            show_paste_hint: false,
            starting_message: None,
            force_login_method,
            login_with_hosted_account: login_with_hosted,
        }
    }

    pub fn set_status(&mut self, status: OAuthState) {
        self.oauth_status = status;
    }

    pub fn select_login_method(&mut self, method: LoginMethod) {
        self.login_method = Some(method.clone());
        self.login_with_hosted_account = matches!(method, LoginMethod::Hosted);
        self.oauth_status = OAuthState::ReadyToStart;
    }

    pub fn set_pasted_code(&mut self, code: String) {
        self.cursor_offset = code.len();
        self.pasted_code = code;
    }

    pub fn start_flow(&mut self) {
        self.oauth_status = OAuthState::WaitingForLogin { url: String::new() };
    }

    pub fn set_error(&mut self, message: String) {
        self.oauth_status = OAuthState::Error { message };
    }

    pub fn set_success(&mut self, token: Option<String>) {
        self.oauth_status = OAuthState::Success { token };
    }

    pub fn retry(&mut self) {
        self.oauth_status = OAuthState::ReadyToStart;
    }
}

pub struct ConsoleOAuthFlowWidget<'a> {
    pub state: &'a ConsoleOAuthFlowState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ConsoleOAuthFlowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Login ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match &self.state.oauth_status {
            OAuthState::Idle => {
                let lines = vec![
                    "Select login method:",
                    "",
                    "  1. Hosted subscription billing",
                    "  2. Hosted API billing",
                ];
                for (i, line) in lines.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y < inner.y + inner.height {
                        buf.set_string(inner.x, y, line, Style::default().fg(Color::White));
                    }
                }
            }
            OAuthState::WaitingForLogin { url } => {
                let url_line = format!("URL: {}", url);
                let lines: Vec<&str> = vec![
                    "Waiting for browser login...",
                    "",
                    &url_line,
                    "",
                    "Paste code here if prompted > ",
                ];
                for (i, line) in lines.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y < inner.y + inner.height {
                        buf.set_string(inner.x, y, line, Style::default().fg(Color::White));
                    }
                }
                if !self.state.pasted_code.is_empty() {
                    let y = inner.y + 4;
                    if y < inner.y + inner.height {
                        buf.set_string(inner.x + 30, y, &self.state.pasted_code, Style::default().fg(Color::Cyan));
                    }
                }
            }
            OAuthState::CreatingApiKey => {
                buf.set_string(inner.x, inner.y, "Creating API key...", Style::default().fg(Color::Yellow));
            }
            OAuthState::Success { token } => {
                buf.set_string(inner.x, inner.y, "✓ Login successful!", Style::default().fg(Color::Green));
                if let Some(t) = token {
                    let masked = format!("{}...{}", &t[..4.min(t.len())], &t[t.len().saturating_sub(4)..]);
                    buf.set_string(inner.x, inner.y + 1, &format!("Token: {}", masked), Style::default().fg(Color::DarkGray));
                }
            }
            OAuthState::Error { message } => {
                buf.set_string(inner.x, inner.y, &format!("Error: {}", message), Style::default().fg(Color::Red));
                buf.set_string(inner.x, inner.y + 2, "Press Enter to retry", Style::default().fg(Color::DarkGray));
            }
            OAuthState::ReadyToStart => {
                buf.set_string(inner.x, inner.y, "Press Enter to open browser...", Style::default().fg(Color::White));
            }
            OAuthState::PlatformSetup => {
                buf.set_string(inner.x, inner.y, "Setting up platform...", Style::default().fg(Color::Yellow));
            }
            OAuthState::AboutToRetry => {
                buf.set_string(inner.x, inner.y, "Retrying...", Style::default().fg(Color::Yellow));
            }
        }
    }
}

// ─── Feedback ──────────────────────────────────────────────────────────────

const GITHUB_URL_LIMIT: usize = 7250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackStep {
    UserInput,
    Consent,
    Submitting,
    Done,
}

#[derive(Debug, Clone)]
pub struct FeedbackData {
    pub latest_assistant_message_id: Option<String>,
    pub message_count: usize,
    pub datetime: String,
    pub description: String,
    pub platform: String,
    pub git_repo: bool,
    pub version: Option<String>,
}

pub fn redact_sensitive_info(text: &str) -> String {
    let mut redacted = text.to_string();
    // API keys (sk-ant...) - simple string-based redaction
    let mut result = String::new();
    let mut i = 0;
    let bytes = redacted.as_bytes();
    while i < bytes.len() {
        if i + 6 < bytes.len() && &redacted[i..i+6] == "sk-ant" {
            // Find end of key (non-alphanumeric/dash/underscore)
            let start = i;
            i += 6;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
                i += 1;
            }
            if i - start >= 16 {
                result.push_str("[REDACTED_API_KEY]");
            } else {
                result.push_str(&redacted[start..i]);
            }
        } else if i + 4 < bytes.len() && &redacted[i..i+4] == "AKIA" {
            let start = i;
            i += 4;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start >= 20 {
                result.push_str("[REDACTED_AWS_KEY]");
            } else {
                result.push_str(&redacted[start..i]);
            }
        } else {
            result.push(redacted[i..].chars().next().unwrap_or(' '));
            i += redacted[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
    }
    result
}

pub struct FeedbackState {
    pub step: FeedbackStep,
    pub description: String,
    pub cursor_offset: usize,
    pub include_transcript: bool,
    pub error: Option<String>,
    pub submitted_url: Option<String>,
    pub data: Option<FeedbackData>,
}

impl FeedbackState {
    pub fn new(initial_description: Option<&str>) -> Self {
        Self {
            step: FeedbackStep::UserInput,
            description: initial_description.unwrap_or("").to_string(),
            cursor_offset: initial_description.map(|s| s.len()).unwrap_or(0),
            include_transcript: true,
            error: None,
            submitted_url: None,
            data: None,
        }
    }

    pub fn set_description(&mut self, desc: String) {
        self.cursor_offset = desc.len();
        self.description = desc;
    }

    pub fn toggle_transcript(&mut self) {
        self.include_transcript = !self.include_transcript;
    }

    pub fn submit(&mut self) {
        if self.description.trim().is_empty() {
            self.error = Some("Description cannot be empty".to_string());
            return;
        }
        self.step = FeedbackStep::Consent;
    }

    pub fn confirm_consent(&mut self) {
        self.step = FeedbackStep::Submitting;
    }

    pub fn complete(&mut self, url: Option<String>) {
        self.submitted_url = url;
        self.step = FeedbackStep::Done;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.step = FeedbackStep::UserInput;
    }
}

pub struct FeedbackWidget<'a> {
    pub state: &'a FeedbackState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FeedbackWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Feedback ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match self.state.step {
            FeedbackStep::UserInput => {
                buf.set_string(inner.x, inner.y, "Describe the issue:", Style::default().fg(Color::White));
                let desc_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(4));
                let para = Paragraph::new(self.state.description.as_str())
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::Cyan));
                para.render(desc_area, buf);
                if let Some(ref err) = self.state.error {
                    let y = inner.y + inner.height - 2;
                    buf.set_string(inner.x, y, err, Style::default().fg(Color::Red));
                }
                let footer_y = inner.y + inner.height - 1;
                let transcript_label = if self.state.include_transcript {
                    "[x] Include transcript"
                } else {
                    "[ ] Include transcript"
                };
                buf.set_string(inner.x, footer_y, transcript_label, Style::default().fg(Color::DarkGray));
            }
            FeedbackStep::Consent => {
                buf.set_string(inner.x, inner.y, "Submit feedback? (y/n)", Style::default().fg(Color::White));
                buf.set_string(inner.x, inner.y + 2, "Your feedback will be sent to the development team.", Style::default().fg(Color::DarkGray));
            }
            FeedbackStep::Submitting => {
                buf.set_string(inner.x, inner.y, "Submitting...", Style::default().fg(Color::Yellow));
            }
            FeedbackStep::Done => {
                buf.set_string(inner.x, inner.y, "✓ Feedback submitted! Thank you.", Style::default().fg(Color::Green));
                if let Some(ref url) = self.state.submitted_url {
                    buf.set_string(inner.x, inner.y + 2, url, Style::default().fg(Color::Blue));
                }
            }
        }
    }
}

// ─── FullscreenLayout ──────────────────────────────────────────────────────

const MODAL_TRANSCRIPT_PEEK: u16 = 2;

pub struct UnseenDividerState {
    pub divider_index: Option<usize>,
    pub divider_y: Option<usize>,
    pub message_count_at_snapshot: usize,
}

impl UnseenDividerState {
    pub fn new() -> Self {
        Self {
            divider_index: None,
            divider_y: None,
            message_count_at_snapshot: 0,
        }
    }

    pub fn on_scroll_away(&mut self, scroll_height: usize, message_count: usize) {
        if self.divider_index.is_none() {
            self.divider_index = Some(message_count);
            self.divider_y = Some(scroll_height);
            self.message_count_at_snapshot = message_count;
        }
    }

    pub fn on_repin(&mut self) {
        self.divider_index = None;
        self.divider_y = None;
    }

    pub fn jump_to_new_offset(&self) -> Option<usize> {
        self.divider_y
    }

    pub fn new_message_count(&self, current_count: usize) -> usize {
        current_count.saturating_sub(self.message_count_at_snapshot)
    }

    pub fn shift_for_prepend(&mut self, index_delta: usize, height_delta: usize) {
        if let Some(ref mut idx) = self.divider_index {
            *idx += index_delta;
        }
        if let Some(ref mut y) = self.divider_y {
            *y += height_delta;
        }
    }
}

#[derive(Debug, Clone)]
pub struct FullscreenLayoutConfig {
    pub show_sticky_prompt: bool,
    pub show_pill: bool,
    pub hide_sticky: bool,
    pub hide_pill: bool,
    pub new_message_count: usize,
}

pub struct FullscreenLayoutState {
    pub is_fullscreen: bool,
    pub sticky_prompt: Option<StickyPrompt>,
    pub pill_visible: bool,
    pub modal_visible: bool,
    pub unseen: UnseenDividerState,
    pub config: FullscreenLayoutConfig,
}

impl FullscreenLayoutState {
    pub fn new(is_fullscreen: bool) -> Self {
        Self {
            is_fullscreen,
            sticky_prompt: None,
            pill_visible: false,
            modal_visible: false,
            unseen: UnseenDividerState::new(),
            config: FullscreenLayoutConfig {
                show_sticky_prompt: true,
                show_pill: true,
                hide_sticky: false,
                hide_pill: false,
                new_message_count: 0,
            },
        }
    }

    pub fn set_sticky_prompt(&mut self, prompt: Option<StickyPrompt>) {
        self.sticky_prompt = prompt;
    }

    pub fn set_pill_visible(&mut self, visible: bool) {
        self.pill_visible = visible && !self.config.hide_pill;
    }

    pub fn set_modal_visible(&mut self, visible: bool) {
        self.modal_visible = visible;
    }

    pub fn compute_scroll_area_height(&self, total_height: u16, bottom_height: u16) -> u16 {
        let sticky_height = if self.sticky_prompt.is_some() && !self.config.hide_sticky { 2 } else { 0 };
        let modal_height = if self.modal_visible { total_height / 2 } else { 0 };
        total_height.saturating_sub(bottom_height + sticky_height + modal_height)
    }
}

pub struct FullscreenLayoutWidget<'a> {
    pub state: &'a FullscreenLayoutState,
    pub theme: &'a Theme,
}

impl<'a> Widget for FullscreenLayoutWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Sticky prompt header
        if let Some(ref prompt) = self.state.sticky_prompt {
            if !self.state.config.hide_sticky {
                let text = if prompt.text.len() > STICKY_TEXT_CAP {
                    &prompt.text[..STICKY_TEXT_CAP]
                } else {
                    &prompt.text
                };
                let header_area = Rect::new(area.x, area.y, area.width, 1);
                buf.set_string(header_area.x + 1, header_area.y, &format!("❯ {}", text), Style::default().fg(Color::DarkGray));
            }
        }

        // Pill (jump to bottom / N new messages)
        if self.state.pill_visible {
            let pill_text = if self.state.config.new_message_count > 0 {
                format!(" {} new messages ↓ ", self.state.config.new_message_count)
            } else {
                " Jump to bottom ↓ ".to_string()
            };
            let pill_width = pill_text.len() as u16;
            let pill_x = area.x + (area.width.saturating_sub(pill_width)) / 2;
            let pill_y = area.y + area.height - 2;
            buf.set_string(pill_x, pill_y, &pill_text, Style::default().fg(Color::Black).bg(Color::White));
        }
    }
}

// ─── ModelPicker ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Low => "⚡",
            Self::Medium => "⚡⚡",
            Self::High => "⚡⚡⚡",
            Self::Max => "⚡⚡⚡⚡",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Max => "Max",
        }
    }
}

const NO_PREFERENCE: &str = "__NO_PREFERENCE__";

pub struct ModelPickerState {
    pub model_options: Vec<ModelOption>,
    pub focused_value: String,
    pub initial_value: String,
    pub session_model: Option<String>,
    pub effort: Option<EffortLevel>,
    pub has_toggled_effort: bool,
    pub show_fast_mode_notice: bool,
    pub header_text: String,
    pub skip_settings_write: bool,
}

impl ModelPickerState {
    pub fn new(initial: Option<&str>, model_options: Vec<ModelOption>) -> Self {
        let initial_value = initial.unwrap_or(NO_PREFERENCE).to_string();
        Self {
            focused_value: initial_value.clone(),
            initial_value,
            model_options,
            session_model: None,
            effort: None,
            has_toggled_effort: false,
            show_fast_mode_notice: false,
            header_text: "Switch between available models.".to_string(),
            skip_settings_write: false,
        }
    }

    pub fn focus_next(&mut self) {
        let current_idx = self.model_options.iter().position(|o| o.value == self.focused_value);
        if let Some(idx) = current_idx {
            let next = (idx + 1) % self.model_options.len();
            self.focused_value = self.model_options[next].value.clone();
        }
    }

    pub fn focus_prev(&mut self) {
        let current_idx = self.model_options.iter().position(|o| o.value == self.focused_value);
        if let Some(idx) = current_idx {
            let prev = if idx == 0 { self.model_options.len() - 1 } else { idx - 1 };
            self.focused_value = self.model_options[prev].value.clone();
        }
    }

    pub fn select_current(&self) -> Option<&str> {
        if self.focused_value == NO_PREFERENCE {
            None
        } else {
            Some(&self.focused_value)
        }
    }

    pub fn toggle_effort(&mut self) {
        self.has_toggled_effort = true;
        self.effort = match &self.effort {
            None => Some(EffortLevel::Low),
            Some(EffortLevel::Low) => Some(EffortLevel::Medium),
            Some(EffortLevel::Medium) => Some(EffortLevel::High),
            Some(EffortLevel::High) => Some(EffortLevel::Max),
            Some(EffortLevel::Max) => None,
        };
    }
}

pub struct ModelPickerWidget<'a> {
    pub state: &'a ModelPickerState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ModelPickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Select Model ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        // Header
        buf.set_string(inner.x, inner.y, &self.state.header_text, Style::default().fg(Color::DarkGray));

        // Model list
        let list_start_y = inner.y + 2;
        for (i, opt) in self.state.model_options.iter().enumerate() {
            let y = list_start_y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_focused = opt.value == self.state.focused_value;
            let is_current = opt.value == self.state.initial_value;
            let prefix = if is_focused { "▸ " } else { "  " };
            let suffix = if is_current { " (current)" } else { "" };
            let style = if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(inner.x, y, &format!("{}{}{}", prefix, opt.label, suffix), style);
            if let Some(ref desc) = opt.description {
                if is_focused && y + 1 < inner.y + inner.height {
                    buf.set_string(inner.x + 4, y + 1, desc, Style::default().fg(Color::DarkGray));
                }
            }
        }

        // Effort indicator
        if let Some(ref effort) = self.state.effort {
            let effort_y = inner.y + inner.height - 1;
            let effort_text = format!("Effort: {} {}", effort.symbol(), effort.label());
            buf.set_string(inner.x, effort_y, &effort_text, Style::default().fg(Color::Yellow));
        }
    }
}

// ─── messageActions ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigableType {
    User,
    Assistant,
    GroupedToolUse,
    CollapsedReadSearch,
    System,
    Attachment,
}

#[derive(Debug, Clone)]
pub struct PrimaryInput {
    pub label: &'static str,
    pub tool_name: &'static str,
}

pub fn get_primary_inputs() -> Vec<PrimaryInput> {
    vec![
        PrimaryInput { label: "path", tool_name: "Read" },
        PrimaryInput { label: "path", tool_name: "Edit" },
        PrimaryInput { label: "path", tool_name: "Write" },
        PrimaryInput { label: "path", tool_name: "MultiEdit" },
        PrimaryInput { label: "command", tool_name: "Bash" },
        PrimaryInput { label: "query", tool_name: "Grep" },
        PrimaryInput { label: "query", tool_name: "Glob" },
        PrimaryInput { label: "url", tool_name: "WebFetch" },
        PrimaryInput { label: "query", tool_name: "WebSearch" },
    ]
}

pub fn is_navigable_message(msg: &RenderableMessage) -> bool {
    match msg.message_type {
        RenderableMessageType::Assistant => {
            !msg.content.is_empty()
        }
        RenderableMessageType::User => {
            if msg.is_meta {
                return false;
            }
            if msg.content.is_empty() {
                return false;
            }
            let trimmed = msg.content.trim();
            !trimmed.starts_with('<')
        }
        RenderableMessageType::System => true,
        RenderableMessageType::CollapsedReadSearch => true,
        _ => false,
    }
}

pub fn strip_system_reminders(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(end_tag_pos) = trimmed.find("</system_reminder>") {
        let after = &trimmed[end_tag_pos + "</system_reminder>".len()..];
        after.trim_start()
    } else {
        trimmed
    }
}

pub fn tool_call_of(msg: &RenderableMessage) -> Option<&str> {
    msg.tool_use_id.as_deref()
}

#[derive(Debug, Clone)]
pub struct MessageActionsState {
    pub cursor_index: Option<usize>,
    pub expanded: bool,
}

impl MessageActionsState {
    pub fn new() -> Self {
        Self {
            cursor_index: None,
            expanded: false,
        }
    }

    pub fn set_cursor(&mut self, index: Option<usize>) {
        self.cursor_index = index;
        self.expanded = false;
    }

    pub fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }
}

pub struct MessageActionsNav {
    pub navigable_indices: Vec<usize>,
    pub current_index: usize,
}

impl MessageActionsNav {
    pub fn new(messages: &[RenderableMessage]) -> Self {
        let navigable_indices: Vec<usize> = messages.iter()
            .enumerate()
            .filter(|(_, m)| is_navigable_message(m))
            .map(|(i, _)| i)
            .collect();
        Self {
            navigable_indices,
            current_index: 0,
        }
    }

    pub fn next(&mut self) -> Option<usize> {
        if self.current_index < self.navigable_indices.len().saturating_sub(1) {
            self.current_index += 1;
        }
        self.navigable_indices.get(self.current_index).copied()
    }

    pub fn prev(&mut self) -> Option<usize> {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
        self.navigable_indices.get(self.current_index).copied()
    }

    pub fn current(&self) -> Option<usize> {
        self.navigable_indices.get(self.current_index).copied()
    }

    pub fn jump_to(&mut self, msg_index: usize) {
        if let Some(pos) = self.navigable_indices.iter().position(|&i| i == msg_index) {
            self.current_index = pos;
        }
    }
}

// ─── MessageRow ────────────────────────────────────────────────────────────

pub struct MessageRowState {
    pub message: RenderableMessage,
    pub is_user_continuation: bool,
    pub has_content_after: bool,
    pub verbose: bool,
    pub can_animate: bool,
    pub columns: u16,
    pub is_loading: bool,
}

pub fn has_thinking_content(msg: &RenderableMessage) -> bool {
    msg.thinking_content.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
}

pub struct MessageRowWidget<'a> {
    pub state: &'a MessageRowState,
    pub theme: &'a Theme,
}

impl<'a> Widget for MessageRowWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let msg = &self.state.message;
        let (prefix, prefix_style) = match msg.message_type {
            RenderableMessageType::User => ("❯ ", Style::default().fg(Color::Cyan)),
            RenderableMessageType::Assistant => {
                if has_thinking_content(msg) {
                    ("💭 ", Style::default().fg(Color::Blue))
                } else {
                    ("⏎ ", Style::default().fg(Color::White))
                }
            }
            RenderableMessageType::System => ("ℹ ", Style::default().fg(Color::DarkGray)),
            RenderableMessageType::Progress => ("⟳ ", Style::default().fg(Color::Yellow)),
            RenderableMessageType::ToolUse => ("⚙ ", Style::default().fg(Color::Magenta)),
            RenderableMessageType::ToolResult => ("✓ ", Style::default().fg(Color::Green)),
            RenderableMessageType::CollapsedReadSearch => ("… ", Style::default().fg(Color::DarkGray)),
            RenderableMessageType::Thinking => ("💭 ", Style::default().fg(Color::Blue)),
            RenderableMessageType::Meta => ("", Style::default().fg(Color::DarkGray)),
        };

        // Model badge
        if let Some(ref model) = msg.model {
            if self.state.verbose {
                let model_x = area.x + area.width.saturating_sub(model.len() as u16 + 2);
                buf.set_string(model_x, area.y, model, Style::default().fg(Color::DarkGray));
            }
        }

        // Prefix
        buf.set_string(area.x, area.y, prefix, prefix_style);

        // Content
        let content_x = area.x + prefix.len() as u16;
        let max_width = (area.width as usize).saturating_sub(prefix.len() + 1);
        let display_content = if msg.content.len() > max_width {
            &msg.content[..max_width]
        } else {
            &msg.content
        };
        let content_style = match msg.message_type {
            RenderableMessageType::User => Style::default().fg(Color::White),
            RenderableMessageType::Assistant => Style::default().fg(Color::White),
            RenderableMessageType::System => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(Color::White),
        };
        buf.set_string(content_x, area.y, display_content, content_style);

        // Timestamp
        if let Some(_ts) = msg.timestamp {
            // Timestamp rendering handled by parent
        }
    }
}

// ─── DesktopHandoff ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopHandoffStep {
    Checking,
    PromptDownload,
    Flushing,
    Opening,
    Success,
    Error(String),
}

pub struct DesktopHandoffState {
    pub step: DesktopHandoffStep,
    pub download_url: String,
    pub companion_name: String,
}

impl DesktopHandoffState {
    pub fn new() -> Self {
        Self {
            step: DesktopHandoffStep::Checking,
            download_url: String::new(),
            companion_name: "Desktop".to_string(),
        }
    }

    pub fn set_step(&mut self, step: DesktopHandoffStep) {
        self.step = step;
    }

    pub fn prompt_download(&mut self, url: String) {
        self.download_url = url;
        self.step = DesktopHandoffStep::PromptDownload;
    }

    pub fn start_opening(&mut self) {
        self.step = DesktopHandoffStep::Flushing;
    }

    pub fn complete_opening(&mut self) {
        self.step = DesktopHandoffStep::Opening;
    }

    pub fn success(&mut self) {
        self.step = DesktopHandoffStep::Success;
    }

    pub fn error(&mut self, msg: String) {
        self.step = DesktopHandoffStep::Error(msg);
    }
}

pub struct DesktopHandoffWidget<'a> {
    pub state: &'a DesktopHandoffState,
    pub theme: &'a Theme,
}

impl<'a> Widget for DesktopHandoffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Desktop Handoff ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match &self.state.step {
            DesktopHandoffStep::Checking => {
                buf.set_string(inner.x, inner.y, "Checking desktop app status...", Style::default().fg(Color::Yellow));
            }
            DesktopHandoffStep::PromptDownload => {
                let lines = vec![
                    format!("{} is not installed.", self.state.companion_name),
                    String::new(),
                    "Would you like to download it? (y/n)".to_string(),
                    String::new(),
                    format!("Download URL: {}", self.state.download_url),
                ];
                for (i, line) in lines.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y < inner.y + inner.height {
                        buf.set_string(inner.x, y, line, Style::default().fg(Color::White));
                    }
                }
            }
            DesktopHandoffStep::Flushing => {
                buf.set_string(inner.x, inner.y, "Flushing session data...", Style::default().fg(Color::Yellow));
            }
            DesktopHandoffStep::Opening => {
                buf.set_string(inner.x, inner.y, &format!("Opening in {}...", self.state.companion_name), Style::default().fg(Color::Yellow));
            }
            DesktopHandoffStep::Success => {
                buf.set_string(inner.x, inner.y, &format!("✓ Opened in {}", self.state.companion_name), Style::default().fg(Color::Green));
            }
            DesktopHandoffStep::Error(msg) => {
                buf.set_string(inner.x, inner.y, &format!("Error: {}", msg), Style::default().fg(Color::Red));
            }
        }
    }
}

// ─── ResumeTask ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CodeSession {
    pub id: String,
    pub title: String,
    pub repo: Option<String>,
    pub updated_at: String,
    pub is_current_repo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadErrorType {
    Network,
    Auth,
    Api,
    Other,
}

pub struct ResumeTaskState {
    pub sessions: Vec<CodeSession>,
    pub current_repo: Option<String>,
    pub loading: bool,
    pub load_error_type: Option<LoadErrorType>,
    pub retrying: bool,
    pub focused_index: usize,
    pub is_embedded: bool,
}

impl ResumeTaskState {
    pub fn new(is_embedded: bool) -> Self {
        Self {
            sessions: Vec::new(),
            current_repo: None,
            loading: true,
            load_error_type: None,
            retrying: false,
            focused_index: 0,
            is_embedded,
        }
    }

    pub fn set_sessions(&mut self, sessions: Vec<CodeSession>) {
        self.sessions = sessions;
        self.loading = false;
        self.focused_index = 0;
    }

    pub fn set_error(&mut self, error_type: LoadErrorType) {
        self.load_error_type = Some(error_type);
        self.loading = false;
    }

    pub fn retry(&mut self) {
        self.retrying = true;
        self.load_error_type = None;
        self.loading = true;
    }

    pub fn focus_next(&mut self) {
        if !self.sessions.is_empty() {
            self.focused_index = (self.focused_index + 1).min(self.sessions.len() - 1);
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn select_current(&self) -> Option<&CodeSession> {
        self.sessions.get(self.focused_index)
    }
}

pub struct ResumeTaskWidget<'a> {
    pub state: &'a ResumeTaskState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ResumeTaskWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Resume Task ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.loading {
            buf.set_string(inner.x, inner.y, "Loading sessions...", Style::default().fg(Color::Yellow));
            return;
        }

        if let Some(ref err) = self.state.load_error_type {
            let msg = match err {
                LoadErrorType::Network => "Network error. Press 'r' to retry.",
                LoadErrorType::Auth => "Authentication error. Please login again.",
                LoadErrorType::Api => "API error. Press 'r' to retry.",
                LoadErrorType::Other => "Unknown error. Press 'r' to retry.",
            };
            buf.set_string(inner.x, inner.y, msg, Style::default().fg(Color::Red));
            return;
        }

        if self.state.sessions.is_empty() {
            buf.set_string(inner.x, inner.y, "No remote sessions found.", Style::default().fg(Color::DarkGray));
            return;
        }

        // Header columns
        let col_title_w = (inner.width as usize).saturating_sub(30);
        let header = format!("{:<width$}  {:>12}  {:>10}", "Title", "Updated", "Repo", width = col_title_w);
        buf.set_string(inner.x, inner.y, &header, Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));

        for (i, session) in self.state.sessions.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_focused = i == self.state.focused_index;
            let title = if session.title.len() > col_title_w {
                format!("{}…", &session.title[..col_title_w - 1])
            } else {
                format!("{:<width$}", session.title, width = col_title_w)
            };
            let repo_marker = if session.is_current_repo { "●" } else { "" };
            let line = format!("{}  {:>12}  {:>10}", title, session.updated_at, repo_marker);
            let prefix = if is_focused { "▸ " } else { "  " };
            let style = if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(inner.x, y, &format!("{}{}", prefix, line), style);
        }
    }
}

// ─── ThemePicker ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeSetting {
    Light,
    Dark,
    System,
    Named(String),
}

impl ThemeSetting {
    pub fn label(&self) -> String {
        match self {
            Self::Light => "Light".to_string(),
            Self::Dark => "Dark".to_string(),
            Self::System => "System".to_string(),
            Self::Named(name) => name.clone(),
        }
    }
}

pub struct ThemePickerState {
    pub options: Vec<ThemeSetting>,
    pub focused_index: usize,
    pub current_theme: ThemeSetting,
    pub show_intro_text: bool,
    pub help_text: String,
    pub show_help_text_below: bool,
    pub hide_esc_to_cancel: bool,
    pub preview_theme: Option<ThemeSetting>,
}

impl ThemePickerState {
    pub fn new(options: Vec<ThemeSetting>, current: ThemeSetting) -> Self {
        let focused_index = options.iter().position(|o| *o == current).unwrap_or(0);
        Self {
            options,
            focused_index,
            current_theme: current,
            show_intro_text: false,
            help_text: String::new(),
            show_help_text_below: false,
            hide_esc_to_cancel: false,
            preview_theme: None,
        }
    }

    pub fn focus_next(&mut self) {
        self.focused_index = (self.focused_index + 1) % self.options.len();
        self.preview_theme = Some(self.options[self.focused_index].clone());
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = if self.focused_index == 0 { self.options.len() - 1 } else { self.focused_index - 1 };
        self.preview_theme = Some(self.options[self.focused_index].clone());
    }

    pub fn select_current(&self) -> &ThemeSetting {
        &self.options[self.focused_index]
    }

    pub fn cancel(&mut self) {
        self.preview_theme = None;
    }
}

pub struct ThemePickerWidget<'a> {
    pub state: &'a ThemePickerState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ThemePickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Select Theme ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut y = inner.y;
        if self.state.show_intro_text {
            buf.set_string(inner.x, y, "Choose a color theme:", Style::default().fg(Color::White));
            y += 2;
        }

        for (i, option) in self.state.options.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }
            let is_focused = i == self.state.focused_index;
            let is_current = *option == self.state.current_theme;
            let prefix = if is_focused { "▸ " } else { "  " };
            let suffix = if is_current { " (current)" } else { "" };
            let style = if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(inner.x, y, &format!("{}{}{}", prefix, option.label(), suffix), style);
            y += 1;
        }

        if !self.state.help_text.is_empty() && self.state.show_help_text_below {
            y += 1;
            if y < inner.y + inner.height {
                buf.set_string(inner.x, y, &self.state.help_text, Style::default().fg(Color::DarkGray));
            }
        }

        if !self.state.hide_esc_to_cancel {
            let footer_y = inner.y + inner.height - 1;
            buf.set_string(inner.x, footer_y, "Esc to cancel", Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── StatusLine ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Plan,
    AutoEdit,
    FullAuto,
    BypassPermissions,
}

impl PermissionMode {
    pub fn display(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
            Self::AutoEdit => "auto-edit",
            Self::FullAuto => "full-auto",
            Self::BypassPermissions => "bypass",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusLineData {
    pub model: String,
    pub permission_mode: PermissionMode,
    pub cwd: String,
    pub session_title: Option<String>,
    pub context_percent: f64,
    pub total_cost: f64,
    pub total_duration_secs: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub vim_mode: Option<String>,
    pub output_style: String,
    pub utilization: Option<f64>,
}

pub struct StatusLineState {
    pub data: StatusLineData,
    pub visible: bool,
    pub custom_command_output: Option<String>,
}

impl StatusLineState {
    pub fn new(data: StatusLineData) -> Self {
        Self {
            data,
            visible: true,
            custom_command_output: None,
        }
    }

    pub fn update_data(&mut self, data: StatusLineData) {
        self.data = data;
    }

    pub fn set_custom_output(&mut self, output: Option<String>) {
        self.custom_command_output = output;
    }

    pub fn should_display(&self) -> bool {
        self.visible
    }
}

pub struct StatusLineWidget<'a> {
    pub state: &'a StatusLineState,
    pub theme: &'a Theme,
}

impl<'a> Widget for StatusLineWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Custom command output takes priority
        if let Some(ref output) = self.state.custom_command_output {
            buf.set_string(area.x, area.y, output, Style::default().fg(Color::White));
            return;
        }

        let data = &self.state.data;
        // Build status line segments
        let model_segment = &data.model;
        let mode_segment = format!("[{}]", data.permission_mode.display());
        let context_segment = format!("ctx:{:.0}%", data.context_percent);
        let cost_segment = if data.total_cost > 0.0 { format!("${:.2}", data.total_cost) } else { String::new() };
        let tokens_segment = format!("{}↑ {}↓", format_number_compact(data.total_input_tokens), format_number_compact(data.total_output_tokens));

        // Layout: model | mode | context | cost | tokens | cwd
        let segments: Vec<&str> = vec![
            model_segment,
            &mode_segment,
            &context_segment,
        ];
        let mut x = area.x;
        for (i, seg) in segments.iter().enumerate() {
            if x + seg.len() as u16 > area.x + area.width {
                break;
            }
            let style = match i {
                0 => Style::default().fg(Color::Cyan),
                1 => Style::default().fg(Color::Yellow),
                _ => Style::default().fg(Color::DarkGray),
            };
            buf.set_string(x, area.y, seg, style);
            x += seg.len() as u16 + 1;
        }

        // Cost on the right
        if !cost_segment.is_empty() {
            let cost_x = area.x + area.width.saturating_sub(cost_segment.len() as u16 + tokens_segment.len() as u16 + 2);
            buf.set_string(cost_x, area.y, &cost_segment, Style::default().fg(Color::Green));
            buf.set_string(cost_x + cost_segment.len() as u16 + 1, area.y, &tokens_segment, Style::default().fg(Color::DarkGray));
        }
    }
}

// ─── TaskListV2 ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◐",
            Self::Completed => "●",
            Self::Failed => "✗",
            Self::Cancelled => "⊘",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Pending => Color::DarkGray,
            Self::InProgress => Color::Yellow,
            Self::Completed => Color::Green,
            Self::Failed => Color::Red,
            Self::Cancelled => Color::DarkGray,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub agent_color: Option<String>,
    pub recent_activity: Option<String>,
    pub completion_time: Option<Instant>,
}

const RECENT_COMPLETED_TASK_TTL_MS: u64 = 5000;

pub struct TaskListV2State {
    pub tasks: Vec<Task>,
    pub is_standalone: bool,
    pub max_display: usize,
    pub columns: u16,
}

impl TaskListV2State {
    pub fn new(tasks: Vec<Task>, columns: u16, rows: u16, is_standalone: bool) -> Self {
        let max_display = if rows <= 10 { 0 } else { 10usize.min(3usize.max(rows as usize - 14)) };
        Self {
            tasks,
            is_standalone,
            max_display,
            columns,
        }
    }

    pub fn prioritize_for_display(&self) -> Vec<&Task> {
        let now = Instant::now();
        let mut visible: Vec<&Task> = self.tasks.iter()
            .filter(|t| {
                match t.status {
                    TaskStatus::Completed => {
                        if let Some(ct) = t.completion_time {
                            now.duration_since(ct).as_millis() < RECENT_COMPLETED_TASK_TTL_MS as u128
                        } else {
                            false
                        }
                    }
                    TaskStatus::Cancelled => false,
                    _ => true,
                }
            })
            .collect();
        // Sort: in_progress first, then pending, then completed
        visible.sort_by(|a, b| {
            let order = |t: &Task| match t.status {
                TaskStatus::InProgress => 0,
                TaskStatus::Pending => 1,
                TaskStatus::Completed => 2,
                TaskStatus::Failed => 3,
                TaskStatus::Cancelled => 4,
            };
            order(a).cmp(&order(b))
        });
        visible.truncate(self.max_display);
        visible
    }
}

pub struct TaskListV2Widget<'a> {
    pub state: &'a TaskListV2State,
    pub theme: &'a Theme,
}

impl<'a> Widget for TaskListV2Widget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.state.max_display == 0 {
            return;
        }

        let visible_tasks = self.state.prioritize_for_display();
        if visible_tasks.is_empty() {
            return;
        }

        for (i, task) in visible_tasks.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let icon = task.status.icon();
            let icon_color = task.status.color();
            buf.set_string(area.x, y, icon, Style::default().fg(icon_color));

            let title_max = (area.width as usize).saturating_sub(4);
            let title = if task.title.len() > title_max {
                format!("{}…", &task.title[..title_max - 1])
            } else {
                task.title.clone()
            };
            buf.set_string(area.x + 2, y, &title, Style::default().fg(Color::White));

            // Activity suffix
            if let Some(ref activity) = task.recent_activity {
                let activity_max = title_max.saturating_sub(title.len() + 2);
                if activity_max > 3 {
                    let act = if activity.len() > activity_max {
                        format!("{}…", &activity[..activity_max - 1])
                    } else {
                        activity.clone()
                    };
                    let act_x = area.x + 2 + title.len() as u16 + 1;
                    buf.set_string(act_x, y, &act, Style::default().fg(Color::DarkGray));
                }
            }
        }
    }
}

// ─── GlobalSearchDialog ────────────────────────────────────────────────────

const GLOBAL_SEARCH_VISIBLE_RESULTS: usize = 12;
const GLOBAL_SEARCH_DEBOUNCE_MS: u64 = 100;
const GLOBAL_SEARCH_PREVIEW_CONTEXT_LINES: usize = 4;
const GLOBAL_SEARCH_MAX_MATCHES_PER_FILE: usize = 10;
const GLOBAL_SEARCH_MAX_TOTAL_MATCHES: usize = 500;

#[derive(Debug, Clone)]
pub struct SearchMatchResult {
    pub file: String,
    pub line: usize,
    pub text: String,
}

pub struct GlobalSearchDialogState {
    pub query: String,
    pub cursor_offset: usize,
    pub matches: Vec<SearchMatchResult>,
    pub focused_index: usize,
    pub is_searching: bool,
    pub preview_content: Option<String>,
    pub preview_on_right: bool,
    pub columns: u16,
    pub rows: u16,
}

impl GlobalSearchDialogState {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            query: String::new(),
            cursor_offset: 0,
            matches: Vec::new(),
            focused_index: 0,
            is_searching: false,
            preview_content: None,
            preview_on_right: columns >= 140,
            columns,
            rows,
        }
    }

    pub fn set_query(&mut self, query: String) {
        self.cursor_offset = query.len();
        self.query = query;
        self.focused_index = 0;
        self.is_searching = true;
    }

    pub fn set_results(&mut self, matches: Vec<SearchMatchResult>) {
        self.matches = matches;
        self.is_searching = false;
        self.focused_index = 0;
    }

    pub fn focus_next(&mut self) {
        if !self.matches.is_empty() {
            self.focused_index = (self.focused_index + 1).min(self.matches.len() - 1);
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn selected_match(&self) -> Option<&SearchMatchResult> {
        self.matches.get(self.focused_index)
    }

    pub fn set_preview(&mut self, content: Option<String>) {
        self.preview_content = content;
    }
}

pub struct GlobalSearchDialogWidget<'a> {
    pub state: &'a GlobalSearchDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for GlobalSearchDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Search ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        // Search input
        let input_text = if self.state.query.is_empty() {
            "Type to search..."
        } else {
            &self.state.query
        };
        buf.set_string(inner.x, inner.y, input_text, Style::default().fg(Color::White));

        if self.state.is_searching {
            buf.set_string(inner.x, inner.y + 1, "Searching...", Style::default().fg(Color::Yellow));
            return;
        }

        // Results
        let results_start_y = inner.y + 2;
        let visible = GLOBAL_SEARCH_VISIBLE_RESULTS.min(inner.height.saturating_sub(3) as usize);
        let start_idx = if self.state.focused_index >= visible {
            self.state.focused_index - visible + 1
        } else {
            0
        };
        let end_idx = (start_idx + visible).min(self.state.matches.len());

        for (i, m) in self.state.matches[start_idx..end_idx].iter().enumerate() {
            let y = results_start_y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_focused = start_idx + i == self.state.focused_index;
            let prefix = if is_focused { "▸ " } else { "  " };
            let max_w = (inner.width as usize).saturating_sub(prefix.len() + 10);
            let file_display = if m.file.len() > max_w / 2 {
                format!("…{}", &m.file[m.file.len() - max_w / 2..])
            } else {
                m.file.clone()
            };
            let line = format!("{}{}:{} {}", prefix, file_display, m.line, m.text.trim());
            let style = if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            let display = if line.len() > inner.width as usize {
                &line[..inner.width as usize]
            } else {
                &line
            };
            buf.set_string(inner.x, y, display, style);
        }

        // Match count footer
        let footer_y = inner.y + inner.height - 1;
        let count_text = format!("{} matches", self.state.matches.len());
        buf.set_string(inner.x, footer_y, &count_text, Style::default().fg(Color::DarkGray));
    }
}

// ─── RemoteEnvironmentDialog ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnvironmentResource {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteEnvLoadingState {
    Loading,
    Updating,
    Done,
}

pub struct RemoteEnvironmentDialogState {
    pub loading_state: RemoteEnvLoadingState,
    pub environments: Vec<EnvironmentResource>,
    pub selected_environment: Option<String>,
    pub selected_environment_source: Option<String>,
    pub error: Option<String>,
    pub focused_index: usize,
}

impl RemoteEnvironmentDialogState {
    pub fn new() -> Self {
        Self {
            loading_state: RemoteEnvLoadingState::Loading,
            environments: Vec::new(),
            selected_environment: None,
            selected_environment_source: None,
            error: None,
            focused_index: 0,
        }
    }

    pub fn set_environments(&mut self, envs: Vec<EnvironmentResource>, selected: Option<String>, source: Option<String>) {
        self.environments = envs;
        self.selected_environment = selected;
        self.selected_environment_source = source;
        self.loading_state = RemoteEnvLoadingState::Done;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading_state = RemoteEnvLoadingState::Done;
    }

    pub fn focus_next(&mut self) {
        if !self.environments.is_empty() {
            self.focused_index = (self.focused_index + 1).min(self.environments.len() - 1);
        }
    }

    pub fn focus_prev(&mut self) {
        self.focused_index = self.focused_index.saturating_sub(1);
    }

    pub fn select_current(&mut self) {
        if let Some(env) = self.environments.get(self.focused_index) {
            self.selected_environment = Some(env.id.clone());
            self.loading_state = RemoteEnvLoadingState::Updating;
        }
    }
}

pub struct RemoteEnvironmentDialogWidget<'a> {
    pub state: &'a RemoteEnvironmentDialogState,
    pub theme: &'a Theme,
}

impl<'a> Widget for RemoteEnvironmentDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(" Select Remote Environment ").borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        match self.state.loading_state {
            RemoteEnvLoadingState::Loading => {
                buf.set_string(inner.x, inner.y, "Loading environments...", Style::default().fg(Color::Yellow));
            }
            RemoteEnvLoadingState::Updating => {
                buf.set_string(inner.x, inner.y, "Updating environment...", Style::default().fg(Color::Yellow));
            }
            RemoteEnvLoadingState::Done => {
                if let Some(ref err) = self.state.error {
                    buf.set_string(inner.x, inner.y, &format!("Error: {}", err), Style::default().fg(Color::Red));
                    return;
                }

                if self.state.environments.is_empty() {
                    buf.set_string(inner.x, inner.y, "No environments available.", Style::default().fg(Color::DarkGray));
                    buf.set_string(inner.x, inner.y + 1, "Configure environments at the platform URL.", Style::default().fg(Color::DarkGray));
                    return;
                }

                for (i, env) in self.state.environments.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y >= inner.y + inner.height {
                        break;
                    }
                    let is_focused = i == self.state.focused_index;
                    let is_selected = self.state.selected_environment.as_deref() == Some(&env.id);
                    let prefix = if is_focused { "▸ " } else { "  " };
                    let check = if is_selected { " ●" } else { "" };
                    let style = if is_focused {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let label = format!("{}{}{}", prefix, env.name, check);
                    buf.set_string(inner.x, y, &label, style);
                    if let Some(ref desc) = env.description {
                        if is_focused && y + 1 < inner.y + inner.height {
                            buf.set_string(inner.x + 4, y + 1, desc, Style::default().fg(Color::DarkGray));
                        }
                    }
                }
            }
        }
    }
}

// ─── MarkdownTable ─────────────────────────────────────────────────────────

const MARKDOWN_TABLE_SAFETY_MARGIN: usize = 4;
const MARKDOWN_TABLE_MIN_COLUMN_WIDTH: usize = 3;
const MARKDOWN_TABLE_MAX_ROW_LINES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct TableColumn {
    pub header: String,
    pub alignment: ColumnAlignment,
    pub width: usize,
}

#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<String>,
}

pub struct MarkdownTableData {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub use_vertical_format: bool,
}

impl MarkdownTableData {
    pub fn new(headers: Vec<String>, alignments: Vec<ColumnAlignment>, rows: Vec<Vec<String>>, available_width: usize) -> Self {
        let num_cols = headers.len();
        let safe_width = available_width.saturating_sub(MARKDOWN_TABLE_SAFETY_MARGIN);

        // Calculate natural widths
        let mut natural_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < natural_widths.len() {
                    natural_widths[i] = natural_widths[i].max(cell.len());
                }
            }
        }

        let total_natural: usize = natural_widths.iter().sum::<usize>() + (num_cols.saturating_sub(1)) * 3; // separators
        let use_vertical = total_natural > safe_width * 2;

        let col_widths = if total_natural <= safe_width {
            natural_widths.clone()
        } else {
            // Distribute proportionally
            let available_for_content = safe_width.saturating_sub((num_cols.saturating_sub(1)) * 3);
            let total_natural_content: usize = natural_widths.iter().sum();
            natural_widths.iter().map(|&w| {
                let proportion = w as f64 / total_natural_content as f64;
                (proportion * available_for_content as f64).max(MARKDOWN_TABLE_MIN_COLUMN_WIDTH as f64) as usize
            }).collect()
        };

        let columns: Vec<TableColumn> = headers.into_iter().zip(alignments.into_iter()).zip(col_widths.into_iter())
            .map(|((header, alignment), width)| TableColumn { header, alignment, width })
            .collect();

        let table_rows: Vec<TableRow> = rows.into_iter()
            .map(|cells| TableRow { cells })
            .collect();

        Self {
            columns,
            rows: table_rows,
            use_vertical_format: use_vertical,
        }
    }

    pub fn pad_aligned(text: &str, width: usize, alignment: &ColumnAlignment) -> String {
        if text.len() >= width {
            return text[..width].to_string();
        }
        let padding = width - text.len();
        match alignment {
            ColumnAlignment::Left => format!("{}{}", text, " ".repeat(padding)),
            ColumnAlignment::Right => format!("{}{}", " ".repeat(padding), text),
            ColumnAlignment::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!("{}{}{}", " ".repeat(left_pad), text, " ".repeat(right_pad))
            }
        }
    }
}

pub struct MarkdownTableWidget<'a> {
    pub data: &'a MarkdownTableData,
    pub theme: &'a Theme,
}

impl<'a> Widget for MarkdownTableWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.data.use_vertical_format {
            // Vertical (key: value) format
            let mut y = area.y;
            for (row_idx, row) in self.data.rows.iter().enumerate() {
                if y >= area.y + area.height {
                    break;
                }
                if row_idx > 0 {
                    // Separator between rows
                    buf.set_string(area.x, y, "─".repeat(area.width as usize).as_str(), Style::default().fg(Color::DarkGray));
                    y += 1;
                }
                for (col_idx, cell) in row.cells.iter().enumerate() {
                    if y >= area.y + area.height {
                        break;
                    }
                    if let Some(col) = self.data.columns.get(col_idx) {
                        let line = format!("{}: {}", col.header, cell);
                        let display = if line.len() > area.width as usize {
                            &line[..area.width as usize]
                        } else {
                            &line
                        };
                        buf.set_string(area.x, y, display, Style::default().fg(Color::White));
                        y += 1;
                    }
                }
            }
        } else {
            // Horizontal table format
            let mut y = area.y;

            // Header
            let header_line: String = self.data.columns.iter()
                .map(|c| MarkdownTableData::pad_aligned(&c.header, c.width, &c.alignment))
                .collect::<Vec<_>>()
                .join(" │ ");
            if y < area.y + area.height {
                buf.set_string(area.x, y, &header_line, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
                y += 1;
            }

            // Separator
            if y < area.y + area.height {
                let sep_line: String = self.data.columns.iter()
                    .map(|c| "─".repeat(c.width))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                buf.set_string(area.x, y, &sep_line, Style::default().fg(Color::DarkGray));
                y += 1;
            }

            // Rows
            for row in &self.data.rows {
                if y >= area.y + area.height {
                    break;
                }
                let row_line: String = row.cells.iter().enumerate()
                    .map(|(i, cell)| {
                        if let Some(col) = self.data.columns.get(i) {
                            MarkdownTableData::pad_aligned(cell, col.width, &col.alignment)
                        } else {
                            cell.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" │ ");
                buf.set_string(area.x, y, &row_line, Style::default().fg(Color::White));
                y += 1;
            }
        }
    }
}

// ─── ContextVisualization ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ContextCategory {
    pub name: String,
    pub token_count: u64,
    pub percentage: f64,
    pub source: Option<String>,
    pub is_reserved: bool,
}

#[derive(Debug, Clone)]
pub struct ContextData {
    pub categories: Vec<ContextCategory>,
    pub total_tokens: u64,
    pub context_window: u64,
    pub used_percentage: f64,
}

#[derive(Debug, Clone)]
pub struct ContextSuggestion {
    pub text: String,
    pub action: String,
}

pub struct ContextVisualizationState {
    pub data: Option<ContextData>,
    pub suggestions: Vec<ContextSuggestion>,
    pub collapse_stats: Option<CollapseStats>,
}

#[derive(Debug, Clone)]
pub struct CollapseStats {
    pub collapsed_spans: usize,
    pub collapsed_messages: usize,
    pub staged_spans: usize,
    pub total_errors: usize,
    pub total_spawns: usize,
}

impl ContextVisualizationState {
    pub fn new() -> Self {
        Self {
            data: None,
            suggestions: Vec::new(),
            collapse_stats: None,
        }
    }

    pub fn set_data(&mut self, data: ContextData) {
        self.suggestions = generate_context_suggestions(&data);
        self.data = Some(data);
    }

    pub fn set_collapse_stats(&mut self, stats: CollapseStats) {
        self.collapse_stats = Some(stats);
    }
}

fn generate_context_suggestions(data: &ContextData) -> Vec<ContextSuggestion> {
    let mut suggestions = Vec::new();
    if data.used_percentage > 90.0 {
        suggestions.push(ContextSuggestion {
            text: "Context window is nearly full. Consider compacting.".to_string(),
            action: "/compact".to_string(),
        });
    }
    if data.used_percentage > 70.0 {
        let large_cats: Vec<_> = data.categories.iter()
            .filter(|c| c.percentage > 30.0 && !c.is_reserved)
            .collect();
        for cat in large_cats {
            suggestions.push(ContextSuggestion {
                text: format!("'{}' uses {:.0}% of context", cat.name, cat.percentage),
                action: String::new(),
            });
        }
    }
    suggestions
}

pub struct ContextVisualizationWidget<'a> {
    pub state: &'a ContextVisualizationState,
    pub theme: &'a Theme,
}

impl<'a> Widget for ContextVisualizationWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(ref data) = self.state.data else {
            buf.set_string(area.x, area.y, "No context data available.", Style::default().fg(Color::DarkGray));
            return;
        };

        let mut y = area.y;

        // Header with total usage
        let header = format!(
            "Context: {}/{} tokens ({:.0}%)",
            format_number_compact(data.total_tokens),
            format_number_compact(data.context_window),
            data.used_percentage
        );
        buf.set_string(area.x, y, &header, Style::default().fg(Color::White));
        y += 1;

        // Progress bar
        if y < area.y + area.height {
            let bar_width = (area.width as usize).saturating_sub(2);
            let filled = ((data.used_percentage / 100.0) * bar_width as f64) as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(bar_width.saturating_sub(filled));
            let bar_color = if data.used_percentage > 90.0 {
                Color::Red
            } else if data.used_percentage > 70.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            buf.set_string(area.x, y, &bar, Style::default().fg(bar_color));
            y += 2;
        }

        // Categories
        for cat in &data.categories {
            if y >= area.y + area.height {
                break;
            }
            let tokens_str = format_number_compact(cat.token_count);
            let line = format!(
                "  {:<20} {:>8} ({:.1}%)",
                if cat.name.len() > 20 { &cat.name[..20] } else { &cat.name },
                tokens_str,
                cat.percentage
            );
            let style = if cat.is_reserved {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(area.x, y, &line, style);
            y += 1;
        }

        // Collapse status
        if let Some(ref stats) = self.state.collapse_stats {
            if y + 1 < area.y + area.height {
                y += 1;
                let collapse_text = if stats.collapsed_spans > 0 {
                    format!("{} spans summarized ({} msgs)", stats.collapsed_spans, stats.collapsed_messages)
                } else if stats.total_spawns > 0 {
                    format!("{} spawns, nothing staged yet", stats.total_spawns)
                } else {
                    "Waiting for first trigger".to_string()
                };
                buf.set_string(area.x, y, &collapse_text, Style::default().fg(Color::DarkGray));
            }
        }

        // Suggestions
        if !self.state.suggestions.is_empty() && y + 2 < area.y + area.height {
            y += 2;
            buf.set_string(area.x, y, "Suggestions:", Style::default().fg(Color::Yellow));
            y += 1;
            for sug in &self.state.suggestions {
                if y >= area.y + area.height {
                    break;
                }
                let line = if sug.action.is_empty() {
                    format!("  • {}", sug.text)
                } else {
                    format!("  • {} ({})", sug.text, sug.action)
                };
                buf.set_string(area.x, y, &line, Style::default().fg(Color::White));
                y += 1;
            }
        }
    }
}
