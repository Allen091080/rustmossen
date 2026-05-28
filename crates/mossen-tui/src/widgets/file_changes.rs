//! `/files` semantic file-change summary widget.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
use crate::render_model::{FileChangeListRenderModel, FileChangeRowRenderModel};
use crate::theme::Theme;

pub struct FileChangesWidget<'a> {
    model: &'a FileChangeListRenderModel,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
    selected: usize,
    scroll: usize,
}

impl<'a> FileChangesWidget<'a> {
    pub fn new(model: &'a FileChangeListRenderModel, theme: &'a Theme) -> Self {
        Self {
            model,
            theme,
            glyphs: RenderGlyphs::default(),
            selected: 0,
            scroll: 0,
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }
}

impl<'a> Widget for FileChangesWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        let block = Block::default()
            .title(Span::styled(
                " File Changes ",
                Style::default()
                    .fg(self.theme.info)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(self.theme.style_border());
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let summary = file_change_summary_line(self.model, self.glyphs);
        buf.set_stringn(
            inner.x,
            inner.y,
            clip_to_width(&summary, inner.width as usize),
            inner.width as usize,
            Style::default().fg(self.theme.text_dim),
        );

        if inner.height < 3 {
            return;
        }

        let footer_y = inner.y.saturating_add(inner.height.saturating_sub(1));
        let footer = format!(
            "Esc closes{}Up/Down selects{}Home/End jump",
            self.glyphs.separator(),
            self.glyphs.separator()
        );
        buf.set_stringn(
            inner.x,
            footer_y,
            clip_to_width(&footer, inner.width as usize),
            inner.width as usize,
            Style::default()
                .fg(self.theme.text_subtle)
                .add_modifier(Modifier::ITALIC),
        );

        let mut y = inner.y.saturating_add(2);
        let rows_bottom = footer_y;
        if self.model.rows.is_empty() {
            buf.set_stringn(
                inner.x,
                y,
                "No file changes recorded.",
                inner.width as usize,
                Style::default().fg(self.theme.text_dim),
            );
            return;
        }

        let selected = self.selected.min(self.model.rows.len().saturating_sub(1));
        for (index, row) in self.model.rows.iter().enumerate().skip(self.scroll) {
            if y >= rows_bottom {
                break;
            }
            let selected_row = index == selected;
            self.render_row_header(row, selected_row, inner.x, y, inner.width, buf);
            y = y.saturating_add(1);

            if selected_row && y < rows_bottom {
                let detail = selected_file_change_detail(row, self.glyphs);
                buf.set_stringn(
                    inner.x,
                    y,
                    clip_to_width(&format!("  {detail}"), inner.width as usize),
                    inner.width as usize,
                    Style::default().fg(self.theme.text_dim),
                );
                y = y.saturating_add(1);
            }
        }
    }
}

impl FileChangesWidget<'_> {
    fn render_row_header(
        &self,
        row: &FileChangeRowRenderModel,
        selected: bool,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
    ) {
        let marker = if selected { ">" } else { " " };
        let marker_style = if selected {
            Style::default()
                .fg(self.theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_dim)
        };
        let title_style = if selected {
            Style::default()
                .fg(self.theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text)
        };
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), marker_style),
            Span::styled(format!("[{}]", row.status_label()), self.status_style(row)),
            Span::raw(" "),
            Span::styled(row.path.clone(), title_style),
            Span::styled(
                format!("  +{} -{}", row.additions, row.deletions),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        buf.set_line(x, y, &line, width);
    }

    fn status_style(&self, row: &FileChangeRowRenderModel) -> Style {
        let color = match row.status_label() {
            "A" => self.theme.success,
            "D" => self.theme.error,
            "M" => self.theme.warning,
            _ => self.theme.info,
        };
        Style::default().fg(color)
    }
}

fn file_change_summary_line(model: &FileChangeListRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "files: {}{sep}modified: {}{sep}added: {}{sep}deleted: {}{sep}other: {}{sep}+{} -{}",
        model.summary.total_count,
        model.summary.modified_count,
        model.summary.added_count,
        model.summary.deleted_count,
        model.summary.other_count,
        model.summary.total_additions,
        model.summary.total_deletions
    )
}

fn selected_file_change_detail(row: &FileChangeRowRenderModel, glyphs: RenderGlyphs) -> String {
    let sep = glyphs.separator();
    format!(
        "{}{sep}path: {}{sep}additions: {}{sep}deletions: {}",
        row.status_name(),
        row.path,
        row.additions,
        row.deletions
    )
}

fn clip_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let suffix = "...";
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width <= suffix.len() {
        return ".".repeat(width);
    }

    let suffix_width = UnicodeWidthStr::width(suffix);
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used
            .saturating_add(grapheme_width)
            .saturating_add(suffix_width)
            > width
        {
            break;
        }
        out.push_str(grapheme);
        used = used.saturating_add(grapheme_width);
    }
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_lifecycle::FileChangeSummaryModel;
    use crate::render_model::FileChangeListRenderModel;

    fn render_files(model: &FileChangeListRenderModel, width: u16, height: u16) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        FileChangesWidget::new(model, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(Rect::new(0, 0, width, height), &mut buf);
        let mut out = String::new();
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buf[(x, y)].symbol());
            }
            out.push_str(line.trim_end());
            if y + 1 < height {
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn file_changes_renders_counts_and_selected_detail() {
        let model = FileChangeListRenderModel::from_files(vec![
            FileChangeSummaryModel {
                path: "src/lib.rs".to_string(),
                status: "M".to_string(),
                additions: 12,
                deletions: 3,
            },
            FileChangeSummaryModel {
                path: "src/new.rs".to_string(),
                status: "A".to_string(),
                additions: 8,
                deletions: 0,
            },
        ]);

        let rendered = render_files(&model, 86, 12);

        assert!(rendered.contains("File Changes"), "{rendered}");
        assert!(rendered.contains("files: 2"), "{rendered}");
        assert!(rendered.contains("modified: 1"), "{rendered}");
        assert!(rendered.contains("[M]"), "{rendered}");
        assert!(rendered.contains("src/lib.rs"), "{rendered}");
        assert!(rendered.contains("Modified"), "{rendered}");
        assert!(rendered.contains("Up/Down selects"), "{rendered}");
    }

    #[test]
    fn file_changes_clips_multibyte_paths_with_ascii_separator() {
        let model = FileChangeListRenderModel::from_files(vec![FileChangeSummaryModel {
            path: format!("crates/{}", "终端渲染".repeat(20)),
            status: "M".to_string(),
            additions: 120,
            deletions: 7,
        }]);

        let rendered = render_files(&model, 46, 8);

        assert!(rendered.contains("..."), "{rendered}");
        assert!(rendered.contains(" - "), "{rendered}");
        assert!(rendered.contains("[M]"), "{rendered}");
    }
}
