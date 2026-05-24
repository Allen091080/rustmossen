//! Diff display widgets.
//!
//! Active ratatui diff widgets for file lists, unified hunks, and file paths.

use std::collections::BTreeSet;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    Header,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub line_no_old: Option<usize>,
    pub line_no_new: Option<usize>,
}

/// A diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// A file-level diff.
#[derive(Debug, Clone, PartialEq, Eq)]
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
// DiffFileListWidget — file list sidebar
// ---------------------------------------------------------------------------

pub struct DiffFileListWidget<'a> {
    pub files: &'a [FileDiff],
    pub selected: usize,
    pub collapsed_files: Option<&'a BTreeSet<usize>>,
    pub theme: &'a Theme,
}

impl<'a> DiffFileListWidget<'a> {
    pub fn new(files: &'a [FileDiff], selected: usize, theme: &'a Theme) -> Self {
        Self {
            files,
            selected,
            collapsed_files: None,
            theme,
        }
    }

    pub fn collapsed_files(mut self, collapsed_files: &'a BTreeSet<usize>) -> Self {
        self.collapsed_files = Some(collapsed_files);
        self
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
                let fold_marker = if self
                    .collapsed_files
                    .is_some_and(|collapsed| collapsed.contains(&i))
                {
                    ">"
                } else {
                    "v"
                };

                let style = if i == self.selected {
                    Style::default()
                        .fg(self.theme.text)
                        .bg(self.theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text)
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", fold_marker),
                        Style::default().fg(self.theme.text_dim),
                    ),
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
// DiffDetailWidget — unified diff view
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

            let mut index = 0;
            while index < hunk.lines.len() {
                let dl = &hunk.lines[index];
                if dl.kind == DiffLineKind::Removed
                    && hunk
                        .lines
                        .get(index + 1)
                        .is_some_and(|next| next.kind == DiffLineKind::Added)
                {
                    let added = &hunk.lines[index + 1];
                    lines.push(diff_detail_line(dl, Some(&added.content), self.theme));
                    lines.push(diff_detail_line(added, Some(&dl.content), self.theme));
                    index += 2;
                    continue;
                }

                lines.push(diff_detail_line(dl, None, self.theme));
                index += 1;
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

fn diff_line_style(theme: &Theme, fg: Color, bg: Color) -> Style {
    let style = Style::default().fg(fg);
    if theme.uses_color() {
        style.bg(theme.terminal_color(bg))
    } else {
        style
    }
}

fn diff_detail_line(line: &DiffLine, paired_content: Option<&str>, theme: &Theme) -> Line<'static> {
    let (prefix, style) = match line.kind {
        DiffLineKind::Added => (
            "+",
            diff_line_style(theme, theme.success, Color::Rgb(20, 40, 20)),
        ),
        DiffLineKind::Removed => (
            "-",
            diff_line_style(theme, theme.error, Color::Rgb(50, 20, 20)),
        ),
        DiffLineKind::Context => (" ", Style::default().fg(theme.text)),
        DiffLineKind::Header => ("@", Style::default().fg(theme.info)),
    };

    let gutter = match (&line.line_no_old, &line.line_no_new) {
        (Some(o), Some(n)) => format!("{:>4} {:>4} ", o, n),
        (Some(o), None) => format!("{:>4}      ", o),
        (None, Some(n)) => format!("     {:>4} ", n),
        (None, None) => "          ".to_string(),
    };

    let mut spans = vec![Span::styled(gutter, Style::default().fg(theme.text_dim))];
    if matches!(line.kind, DiffLineKind::Added | DiffLineKind::Removed) {
        let changed_style = diff_changed_span_style(theme, line.kind.clone());
        spans.extend(diff_content_spans(
            prefix,
            &line.content,
            paired_content,
            style,
            changed_style,
        ));
    } else {
        spans.push(Span::styled(format!("{}{}", prefix, line.content), style));
    }
    Line::from(spans)
}

fn diff_changed_span_style(theme: &Theme, kind: DiffLineKind) -> Style {
    let (fg, bg) = match kind {
        DiffLineKind::Added => (theme.success, Color::Rgb(35, 70, 35)),
        DiffLineKind::Removed => (theme.error, Color::Rgb(80, 35, 35)),
        _ => (theme.text, Color::Rgb(40, 40, 40)),
    };
    diff_line_style(theme, fg, bg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

fn diff_content_spans(
    prefix: &str,
    content: &str,
    paired_content: Option<&str>,
    base_style: Style,
    changed_style: Style,
) -> Vec<Span<'static>> {
    let Some(paired_content) = paired_content else {
        return vec![Span::styled(format!("{prefix}{content}"), base_style)];
    };
    let segments = changed_content_segments(content, paired_content);
    let mut spans = vec![Span::styled(prefix.to_string(), base_style)];
    for (segment, changed) in segments {
        if segment.is_empty() {
            continue;
        }
        let style = if changed { changed_style } else { base_style };
        spans.push(Span::styled(segment, style));
    }
    spans
}

fn changed_content_segments(content: &str, paired_content: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = content.chars().collect();
    let paired: Vec<char> = paired_content.chars().collect();

    let mut prefix_len = 0;
    while prefix_len < chars.len()
        && prefix_len < paired.len()
        && chars[prefix_len] == paired[prefix_len]
    {
        prefix_len += 1;
    }

    let mut suffix_len = 0;
    while suffix_len < chars.len().saturating_sub(prefix_len)
        && suffix_len < paired.len().saturating_sub(prefix_len)
        && chars[chars.len() - 1 - suffix_len] == paired[paired.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let changed_end = chars.len().saturating_sub(suffix_len);
    let mut segments = Vec::new();
    if prefix_len > 0 {
        segments.push((chars[..prefix_len].iter().collect(), false));
    }
    if prefix_len < changed_end {
        segments.push((chars[prefix_len..changed_end].iter().collect(), true));
    }
    if suffix_len > 0 {
        segments.push((chars[changed_end..].iter().collect(), false));
    }
    if segments.is_empty() {
        segments.push((content.to_string(), false));
    }
    segments
}

// ---------------------------------------------------------------------------
// DiffDialogWidget — full diff dialog
// ---------------------------------------------------------------------------

pub struct DiffDialogWidget<'a> {
    pub files: &'a [FileDiff],
    pub selected_file: usize,
    pub scroll_offset: usize,
    pub collapsed_files: Option<&'a BTreeSet<usize>>,
    pub theme: &'a Theme,
}

impl<'a> DiffDialogWidget<'a> {
    pub fn new(files: &'a [FileDiff], theme: &'a Theme) -> Self {
        Self {
            files,
            selected_file: 0,
            scroll_offset: 0,
            collapsed_files: None,
            theme,
        }
    }

    pub fn selected_file(mut self, selected_file: usize) -> Self {
        self.selected_file = selected_file;
        self
    }

    pub fn scroll(mut self, scroll_offset: usize) -> Self {
        self.scroll_offset = scroll_offset;
        self
    }

    pub fn collapsed_files(mut self, collapsed_files: &'a BTreeSet<usize>) -> Self {
        self.collapsed_files = Some(collapsed_files);
        self
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

        let selected = self.selected_file.min(self.files.len().saturating_sub(1));

        // File list
        let mut file_list = DiffFileListWidget::new(self.files, selected, self.theme);
        if let Some(collapsed_files) = self.collapsed_files {
            file_list = file_list.collapsed_files(collapsed_files);
        }
        file_list.render(chunks[0], buf);

        // Detail view
        if let Some(diff) = self.files.get(selected) {
            if self
                .collapsed_files
                .is_some_and(|collapsed| collapsed.contains(&selected))
            {
                let lines = vec![
                    Line::from(Span::styled(
                        "File collapsed",
                        Style::default()
                            .fg(self.theme.primary)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(format!(
                        "{}  +{} -{}  {} hunks",
                        diff.path,
                        diff.additions,
                        diff.deletions,
                        diff.hunks.len()
                    )),
                    Line::from("Press Space to expand this file."),
                ];
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .render(chunks[1], buf);
            } else {
                let detail = DiffDetailWidget::new(diff, self.theme).scroll(self.scroll_offset);
                detail.render(chunks[1], buf);
            }
        }
    }
}

pub fn parse_unified_diff(input: &str) -> Vec<FileDiff> {
    let mut files = Vec::<FileDiff>::new();
    let mut current: Option<FileDiff> = None;
    let mut old_no = 0usize;
    let mut new_no = 0usize;

    for line in input.lines() {
        if line.starts_with("diff --git ") {
            finish_current_file(&mut files, &mut current);
            let (old_path, new_path) = parse_diff_git_paths(line);
            current = Some(FileDiff {
                path: new_path.unwrap_or_else(|| "diff".to_string()),
                old_path,
                hunks: Vec::new(),
                is_new: false,
                is_deleted: false,
                is_binary: false,
                additions: 0,
                deletions: 0,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("--- ") {
            let file = ensure_current_file(&mut current);
            let old_path = normalize_diff_path(path.trim());
            file.is_new = old_path == "/dev/null";
            if !file.is_new {
                file.old_path = Some(old_path);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("+++ ") {
            let file = ensure_current_file(&mut current);
            let new_path = normalize_diff_path(path.trim());
            file.is_deleted = new_path == "/dev/null";
            if !file.is_deleted {
                file.path = new_path;
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("rename from ") {
            ensure_current_file(&mut current).old_path = Some(path.trim().to_string());
            continue;
        }

        if let Some(path) = line.strip_prefix("rename to ") {
            ensure_current_file(&mut current).path = path.trim().to_string();
            continue;
        }

        if line.starts_with("new file mode") {
            ensure_current_file(&mut current).is_new = true;
            continue;
        }

        if line.starts_with("deleted file mode") {
            ensure_current_file(&mut current).is_deleted = true;
            continue;
        }

        if line.starts_with("Binary files ") {
            ensure_current_file(&mut current).is_binary = true;
            continue;
        }

        if line.starts_with("@@") {
            let (old_start, new_start) = parse_hunk_header(line);
            old_no = old_start;
            new_no = new_start;
            ensure_current_file(&mut current).hunks.push(DiffHunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        let Some(file) = current.as_mut() else {
            continue;
        };
        let Some(hunk) = file.hunks.last_mut() else {
            continue;
        };

        if let Some(content) = line.strip_prefix('+') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Added,
                content: content.to_string(),
                line_no_old: None,
                line_no_new: Some(new_no),
            });
            file.additions = file.additions.saturating_add(1);
            new_no = new_no.saturating_add(1);
        } else if let Some(content) = line.strip_prefix('-') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Removed,
                content: content.to_string(),
                line_no_old: Some(old_no),
                line_no_new: None,
            });
            file.deletions = file.deletions.saturating_add(1);
            old_no = old_no.saturating_add(1);
        } else if let Some(content) = line.strip_prefix(' ') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Context,
                content: content.to_string(),
                line_no_old: Some(old_no),
                line_no_new: Some(new_no),
            });
            old_no = old_no.saturating_add(1);
            new_no = new_no.saturating_add(1);
        } else if line.starts_with('\\') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Header,
                content: line.to_string(),
                line_no_old: None,
                line_no_new: None,
            });
        }
    }

    finish_current_file(&mut files, &mut current);
    files
}

fn finish_current_file(files: &mut Vec<FileDiff>, current: &mut Option<FileDiff>) {
    if let Some(file) = current.take() {
        if file.is_binary || !file.hunks.is_empty() || file.additions > 0 || file.deletions > 0 {
            files.push(file);
        }
    }
}

fn ensure_current_file(current: &mut Option<FileDiff>) -> &mut FileDiff {
    current.get_or_insert_with(|| FileDiff {
        path: "diff".to_string(),
        old_path: None,
        hunks: Vec::new(),
        is_new: false,
        is_deleted: false,
        is_binary: false,
        additions: 0,
        deletions: 0,
    })
}

fn parse_diff_git_paths(line: &str) -> (Option<String>, Option<String>) {
    let mut parts = line.split_whitespace().skip(2);
    let old_path = parts.next().map(normalize_diff_path);
    let new_path = parts.next().map(normalize_diff_path);
    (old_path, new_path)
}

fn normalize_diff_path(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_string()
}

fn parse_hunk_header(line: &str) -> (usize, usize) {
    let mut parts = line.split_whitespace();
    let _ = parts.next();
    let old_start = parts
        .next()
        .and_then(|part| parse_hunk_start(part.trim_start_matches('-')))
        .unwrap_or(1);
    let new_start = parts
        .next()
        .and_then(|part| parse_hunk_start(part.trim_start_matches('+')))
        .unwrap_or(1);
    (old_start, new_start)
}

fn parse_hunk_start(part: &str) -> Option<usize> {
    part.split(',').next()?.parse().ok()
}

// ---------------------------------------------------------------------------
// FileEditDiffWidget — inline diff for file edits
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
                similar::ChangeTag::Insert => ("+", Style::default().fg(self.theme.success)),
                similar::ChangeTag::Delete => ("-", Style::default().fg(self.theme.error)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    fn sample_diff() -> &'static str {
        concat!(
            "diff --git a/src/old.rs b/src/new.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/old.rs\n",
            "+++ b/src/new.rs\n",
            "@@ -10,3 +10,4 @@\n",
            " fn main() {\n",
            "-    println!(\"old\");\n",
            "+    println!(\"new\");\n",
            "+    println!(\"extra\");\n",
            " }\n",
            "diff --git a/src/added.rs b/src/added.rs\n",
            "new file mode 100644\n",
            "--- /dev/null\n",
            "+++ b/src/added.rs\n",
            "@@ -0,0 +1,2 @@\n",
            "+added\n",
            "+file\n",
        )
    }

    fn render_widget(widget: impl Widget, width: u16, height: u16) -> String {
        let buffer = render_buffer(widget, width, height);
        buffer_text(&buffer, width, height)
    }

    fn render_buffer(widget: impl Widget, width: u16, height: u16) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        widget.render(Rect::new(0, 0, width, height), &mut buffer);
        buffer
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buffer.content[buffer.index_of(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn find_text(buffer: &Buffer, width: u16, height: u16, needle: &str) -> Option<(u16, u16)> {
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buffer.content[buffer.index_of(x, y)].symbol());
            }
            if let Some(x) = line.find(needle) {
                return Some((x as u16, y));
            }
        }
        None
    }

    #[test]
    fn parse_unified_diff_groups_files_and_counts_lines() {
        let files = parse_unified_diff(sample_diff());

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/new.rs");
        assert_eq!(files[0].old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].deletions, 1);
        let removed = files[0].hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == DiffLineKind::Removed)
            .expect("removed line");
        assert_eq!(removed.line_no_old, Some(11));

        let added = files[0].hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == DiffLineKind::Added)
            .expect("added line");
        assert_eq!(added.line_no_new, Some(11));
        assert!(files[1].is_new);
        assert_eq!(files[1].path, "src/added.rs");
    }

    #[test]
    fn diff_dialog_renders_file_list_and_selected_detail() {
        let files = parse_unified_diff(sample_diff());
        let theme = Theme::default();
        let rendered = render_widget(
            DiffDialogWidget::new(&files, &theme).selected_file(0),
            96,
            14,
        );

        assert!(rendered.contains("new.rs"));
        assert!(rendered.contains("+2/-1"));
        assert!(rendered.contains("@@ -10,3 +10,4 @@"));
        assert!(rendered.contains("println!(\"new\")"));
        assert!(rendered.contains("println!(\"old\")"));
    }

    #[test]
    fn diff_dialog_can_collapse_selected_file() {
        let files = parse_unified_diff(sample_diff());
        let theme = Theme::default();
        let mut collapsed = BTreeSet::new();
        collapsed.insert(0);
        let rendered = render_widget(
            DiffDialogWidget::new(&files, &theme)
                .selected_file(0)
                .collapsed_files(&collapsed),
            80,
            10,
        );

        assert!(rendered.contains("File collapsed"));
        assert!(rendered.contains("Press Space to expand"));
        assert!(!rendered.contains("println!(\"new\")"));
    }

    #[test]
    fn diff_detail_highlights_changed_fragments_inside_paired_lines() {
        let files = parse_unified_diff(concat!(
            "diff --git a/src/demo.rs b/src/demo.rs\n",
            "--- a/src/demo.rs\n",
            "+++ b/src/demo.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn main() {\n",
            "-    let label = \"old value\";\n",
            "+    let label = \"new value\";\n",
            " }\n",
        ));
        let theme = Theme::default();
        let buffer = render_buffer(DiffDetailWidget::new(&files[0], &theme), 88, 10);
        let rendered = buffer_text(&buffer, 88, 10);

        let (old_x, old_y) = find_text(&buffer, 88, 10, "old value").expect(&rendered);
        let old_cell = &buffer.content[buffer.index_of(old_x, old_y)];
        assert_eq!(old_cell.fg, theme.error);
        assert!(
            old_cell.modifier.contains(Modifier::BOLD),
            "changed removed text should be emphasized\n{rendered}"
        );
        assert!(
            old_cell.modifier.contains(Modifier::UNDERLINED),
            "changed removed text should be underlined\n{rendered}"
        );

        let (new_x, new_y) = find_text(&buffer, 88, 10, "new value").expect(&rendered);
        let new_cell = &buffer.content[buffer.index_of(new_x, new_y)];
        assert_eq!(new_cell.fg, theme.success);
        assert!(
            new_cell.modifier.contains(Modifier::BOLD),
            "changed added text should be emphasized\n{rendered}"
        );

        let (unchanged_x, unchanged_y) =
            find_text(&buffer, 88, 10, "let label").expect("unchanged prefix should render");
        let unchanged_cell = &buffer.content[buffer.index_of(unchanged_x, unchanged_y)];
        assert!(
            !unchanged_cell.modifier.contains(Modifier::BOLD),
            "unchanged prefix should keep the base diff style\n{rendered}"
        );
    }
}
