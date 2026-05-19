//! Structured diff display widget.
//!
//! Translates: components/StructuredDiff/ (StructuredDiffView.tsx, DiffSections.tsx)
//! into a ratatui widget showing side-by-side or unified diffs with file grouping.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display mode for structured diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffViewMode {
    #[default]
    Unified,
    SideBySide,
}

/// A section within the structured diff.
#[derive(Debug, Clone)]
pub struct DiffSection {
    pub file_path: String,
    pub old_path: Option<String>,
    pub status: DiffFileStatus,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<StructuredHunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone)]
pub struct StructuredHunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<StructuredDiffLine>,
}

#[derive(Debug, Clone)]
pub struct StructuredDiffLine {
    pub kind: StructuredLineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredLineKind {
    Context,
    Added,
    Removed,
}

// ---------------------------------------------------------------------------
// StructuredDiffWidget — main diff viewer
// ---------------------------------------------------------------------------

pub struct StructuredDiffWidget<'a> {
    pub sections: &'a [DiffSection],
    pub mode: DiffViewMode,
    pub selected_section: usize,
    pub scroll_offset: usize,
    pub theme: &'a Theme,
}

impl<'a> StructuredDiffWidget<'a> {
    pub fn new(sections: &'a [DiffSection], theme: &'a Theme) -> Self {
        Self {
            sections,
            mode: DiffViewMode::Unified,
            selected_section: 0,
            scroll_offset: 0,
            theme,
        }
    }

    pub fn mode(mut self, mode: DiffViewMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn scroll(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    fn render_summary_bar(&self, area: Rect, buf: &mut Buffer) {
        let total_add: usize = self.sections.iter().map(|s| s.additions).sum();
        let total_del: usize = self.sections.iter().map(|s| s.deletions).sum();
        let file_count = self.sections.len();

        let line = Line::from(vec![
            Span::styled(
                format!(
                    "{} file{}",
                    file_count,
                    if file_count != 1 { "s" } else { "" }
                ),
                Style::default().fg(self.theme.text),
            ),
            Span::raw("  "),
            Span::styled(
                format!("+{}", total_add),
                Style::default().fg(self.theme.success),
            ),
            Span::raw("  "),
            Span::styled(
                format!("-{}", total_del),
                Style::default().fg(self.theme.error),
            ),
        ]);
        buf.set_line(area.x, area.y, &line, area.width);
    }

    fn render_unified_section(
        section: &DiffSection,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
        scroll: usize,
    ) {
        let mut lines: Vec<Line> = Vec::new();

        // File header
        let status_icon = match section.status {
            DiffFileStatus::Added => ("A", theme.success),
            DiffFileStatus::Deleted => ("D", theme.error),
            DiffFileStatus::Modified => ("M", theme.warning),
            DiffFileStatus::Renamed => ("R", theme.info),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", status_icon.0),
                Style::default().fg(status_icon.1),
            ),
            Span::styled(
                &section.file_path,
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        for hunk in &section.hunks {
            lines.push(Line::from(Span::styled(
                format!("@@ -{},{} +{},{} @@", hunk.old_start, 0, hunk.new_start, 0),
                Style::default().fg(theme.info),
            )));

            for dl in &hunk.lines {
                let (prefix, style) = match dl.kind {
                    StructuredLineKind::Added => ("+", Style::default().fg(Color::Green)),
                    StructuredLineKind::Removed => ("-", Style::default().fg(Color::Red)),
                    StructuredLineKind::Context => (" ", Style::default().fg(theme.text_dim)),
                };

                let gutter = match (dl.old_no, dl.new_no) {
                    (Some(o), Some(n)) => format!("{:>4} {:>4} ", o, n),
                    (Some(o), None) => format!("{:>4}      ", o),
                    (None, Some(n)) => format!("     {:>4} ", n),
                    _ => "          ".to_string(),
                };

                lines.push(Line::from(vec![
                    Span::styled(gutter, Style::default().fg(theme.text_subtle)),
                    Span::styled(format!("{}{}", prefix, dl.content), style),
                ]));
            }
        }

        let visible: Vec<Line> = lines
            .into_iter()
            .skip(scroll)
            .take(area.height as usize)
            .collect();

        Paragraph::new(visible).render(area, buf);
    }
}

impl<'a> Widget for StructuredDiffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height < 3 || self.sections.is_empty() {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);

        // Summary bar
        self.render_summary_bar(chunks[0], buf);

        // Content
        if let Some(section) = self.sections.get(self.selected_section) {
            Self::render_unified_section(section, self.theme, chunks[1], buf, self.scroll_offset);
        }
    }
}

// ---------------------------------------------------------------------------
// StructuredDiffListWidget — list of changed files with stats
// ---------------------------------------------------------------------------

pub struct StructuredDiffListWidget<'a> {
    pub sections: &'a [DiffSection],
    pub theme: &'a Theme,
}

impl<'a> StructuredDiffListWidget<'a> {
    pub fn new(sections: &'a [DiffSection], theme: &'a Theme) -> Self {
        Self { sections, theme }
    }
}

impl<'a> Widget for StructuredDiffListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = self
            .sections
            .iter()
            .map(|s| {
                let status_char = match s.status {
                    DiffFileStatus::Added => "A",
                    DiffFileStatus::Deleted => "D",
                    DiffFileStatus::Modified => "M",
                    DiffFileStatus::Renamed => "R",
                };
                let color = match s.status {
                    DiffFileStatus::Added => self.theme.success,
                    DiffFileStatus::Deleted => self.theme.error,
                    DiffFileStatus::Modified => self.theme.warning,
                    DiffFileStatus::Renamed => self.theme.info,
                };

                let short = s.file_path.rsplit('/').next().unwrap_or(&s.file_path);
                let line = Line::from(vec![
                    Span::styled(format!("{} ", status_char), Style::default().fg(color)),
                    Span::styled(short.to_string(), Style::default().fg(self.theme.text)),
                    Span::styled(
                        format!(" +{}/-{}", s.additions, s.deletions),
                        Style::default().fg(self.theme.text_dim),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        List::new(items).render(area, buf);
    }
}
