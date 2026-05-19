//! Diff display widgets.
//!
//! Translates: components/diff/ (DiffDetailView, DiffDialog, DiffFileList)
//! + FileEditToolDiff.tsx + FilePathLink.tsx into ratatui widgets.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single diff hunk line.
#[derive(Debug, Clone)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    Header,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub line_no_old: Option<usize>,
    pub line_no_new: Option<usize>,
}

/// A diff hunk.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// A file-level diff.
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub hunks: Vec<DiffHunk>,
    pub is_new: bool,
    pub is_deleted: bool,
    pub is_binary: bool,
    pub additions: usize,
    pub deletions: usize,
}

// ---------------------------------------------------------------------------
// DiffFileListWidget — file list sidebar (DiffFileList.tsx)
// ---------------------------------------------------------------------------

pub struct DiffFileListWidget<'a> {
    pub files: &'a [FileDiff],
    pub selected: usize,
    pub theme: &'a Theme,
}

impl<'a> DiffFileListWidget<'a> {
    pub fn new(files: &'a [FileDiff], selected: usize, theme: &'a Theme) -> Self {
        Self {
            files,
            selected,
            theme,
        }
    }
}

impl<'a> Widget for DiffFileListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let icon = if f.is_new {
                    "+"
                } else if f.is_deleted {
                    "−"
                } else {
                    "~"
                };
                let icon_color = if f.is_new {
                    self.theme.success
                } else if f.is_deleted {
                    self.theme.error
                } else {
                    self.theme.warning
                };

                let short_name = f.path.rsplit('/').next().unwrap_or(&f.path);

                let style = if i == self.selected {
                    Style::default()
                        .fg(self.theme.text)
                        .bg(self.theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text)
                };

                let line = Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                    Span::styled(short_name.to_string(), style),
                    Span::styled(
                        format!(" +{}/-{}", f.additions, f.deletions),
                        Style::default().fg(self.theme.text_dim),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(self.theme.style_border())
            .title("Files");

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

// ---------------------------------------------------------------------------
// DiffDetailWidget — unified diff view (DiffDetailView.tsx)
// ---------------------------------------------------------------------------

pub struct DiffDetailWidget<'a> {
    pub diff: &'a FileDiff,
    pub scroll_offset: usize,
    pub theme: &'a Theme,
}

impl<'a> DiffDetailWidget<'a> {
    pub fn new(diff: &'a FileDiff, theme: &'a Theme) -> Self {
        Self {
            diff,
            scroll_offset: 0,
            theme,
        }
    }

    pub fn scroll(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }
}

impl<'a> Widget for DiffDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        // File header
        let header_text = if let Some(ref old) = self.diff.old_path {
            format!("{} → {}", old, self.diff.path)
        } else {
            self.diff.path.clone()
        };
        lines.push(Line::from(Span::styled(
            header_text,
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        for hunk in &self.diff.hunks {
            // Hunk header
            lines.push(Line::from(Span::styled(
                &hunk.header,
                Style::default().fg(self.theme.info),
            )));

            for dl in &hunk.lines {
                let (prefix, style) = match dl.kind {
                    DiffLineKind::Added => (
                        "+",
                        Style::default().fg(Color::Green).bg(Color::Rgb(20, 40, 20)),
                    ),
                    DiffLineKind::Removed => (
                        "-",
                        Style::default().fg(Color::Red).bg(Color::Rgb(50, 20, 20)),
                    ),
                    DiffLineKind::Context => (" ", Style::default().fg(self.theme.text)),
                    DiffLineKind::Header => ("@", Style::default().fg(self.theme.info)),
                };

                let gutter = match (&dl.line_no_old, &dl.line_no_new) {
                    (Some(o), Some(n)) => format!("{:>4} {:>4} ", o, n),
                    (Some(o), None) => format!("{:>4}      ", o),
                    (None, Some(n)) => format!("     {:>4} ", n),
                    (None, None) => "          ".to_string(),
                };

                let line = Line::from(vec![
                    Span::styled(gutter, Style::default().fg(self.theme.text_dim)),
                    Span::styled(format!("{}{}", prefix, dl.content), style),
                ]);
                lines.push(line);
            }
            lines.push(Line::from(""));
        }

        // Apply scroll
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(self.scroll_offset)
            .take(area.height as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(area, buf);
    }
}

// ---------------------------------------------------------------------------
// DiffDialogWidget — full diff dialog (DiffDialog.tsx)
// ---------------------------------------------------------------------------

pub struct DiffDialogWidget<'a> {
    pub files: &'a [FileDiff],
    pub selected_file: usize,
    pub scroll_offset: usize,
    pub theme: &'a Theme,
}

impl<'a> DiffDialogWidget<'a> {
    pub fn new(files: &'a [FileDiff], theme: &'a Theme) -> Self {
        Self {
            files,
            selected_file: 0,
            scroll_offset: 0,
            theme,
        }
    }
}

impl<'a> Widget for DiffDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 || self.files.is_empty() {
            return;
        }

        // Two-column layout: file list (30%) | detail (70%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);

        // File list
        let file_list = DiffFileListWidget::new(self.files, self.selected_file, self.theme);
        file_list.render(chunks[0], buf);

        // Detail view
        if let Some(diff) = self.files.get(self.selected_file) {
            let detail = DiffDetailWidget::new(diff, self.theme).scroll(self.scroll_offset);
            detail.render(chunks[1], buf);
        }
    }
}

// ---------------------------------------------------------------------------
// FileEditDiffWidget — inline diff for file edits (FileEditToolDiff.tsx)
// ---------------------------------------------------------------------------

pub struct FileEditDiffWidget<'a> {
    pub path: &'a str,
    pub old_content: &'a str,
    pub new_content: &'a str,
    pub theme: &'a Theme,
    pub collapsed: bool,
}

impl<'a> FileEditDiffWidget<'a> {
    pub fn new(
        path: &'a str,
        old_content: &'a str,
        new_content: &'a str,
        theme: &'a Theme,
    ) -> Self {
        Self {
            path,
            old_content,
            new_content,
            theme,
            collapsed: false,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl<'a> Widget for FileEditDiffWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Header line
        let header = Line::from(vec![
            Span::styled("📝 ", Style::default()),
            Span::styled(
                self.path,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ]);
        buf.set_line(area.x, area.y, &header, area.width);

        if self.collapsed || area.height < 3 {
            return;
        }

        // Compute diff using `similar`
        let diff = similar::TextDiff::from_lines(self.old_content, self.new_content);
        let mut y = area.y + 1;

        for change in diff.iter_all_changes() {
            if y >= area.y + area.height {
                break;
            }

            let (sign, style) = match change.tag() {
                similar::ChangeTag::Insert => ("+", Style::default().fg(Color::Green)),
                similar::ChangeTag::Delete => ("-", Style::default().fg(Color::Red)),
                similar::ChangeTag::Equal => (" ", Style::default().fg(self.theme.text_dim)),
            };

            let text = change.to_string_lossy();
            let line_text = text.trim_end_matches('\n');
            let content = format!("{}{}", sign, line_text);
            let avail = area.width as usize;
            let truncated: String = content.chars().take(avail).collect();
            buf.set_string(area.x, y, &truncated, style);
            y += 1;
        }
    }
}
