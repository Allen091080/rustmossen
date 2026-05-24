use mossen_tools::todo::TodoItem;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::{RenderGlyphMode, RenderGlyphs};
use crate::theme::Theme;

fn truncate_display_width(text: &str, max_width: usize, glyphs: RenderGlyphs) -> String {
    if max_width == 0 {
        return String::new();
    }
    let suffix = glyphs.ellipsis();
    let suffix_width = UnicodeWidthStr::width(suffix);
    let content_width = max_width.saturating_sub(suffix_width);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = UnicodeWidthStr::width(ch.to_string().as_str());
        if used.saturating_add(width) > content_width {
            out.push_str(suffix);
            return out;
        }
        used = used.saturating_add(width);
        out.push(ch);
    }
    out
}

pub struct TaskListV2Widget<'a> {
    tasks: &'a [TodoItem],
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> TaskListV2Widget<'a> {
    pub fn new(tasks: &'a [TodoItem], theme: &'a Theme) -> Self {
        Self {
            tasks,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for TaskListV2Widget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.tasks.is_empty() {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                format!(" Tasks ({} total) ", self.tasks.len()),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let items = self
            .tasks
            .iter()
            .map(|todo| {
                let (icon, color) = match todo.status.as_str() {
                    "in_progress" => (
                        task_icon("in_progress", self.glyphs),
                        self.theme.spinner_primary,
                    ),
                    "completed" => (task_icon("completed", self.glyphs), self.theme.success),
                    _ => (task_icon("pending", self.glyphs), self.theme.text_dim),
                };
                let content_budget = (inner.width as usize)
                    .saturating_sub(UnicodeWidthStr::width(icon))
                    .saturating_sub(1);
                let content = truncate_display_width(&todo.content, content_budget, self.glyphs);
                ListItem::new(Line::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(content, Style::default().fg(self.theme.text)),
                ]))
            })
            .collect::<Vec<_>>();
        List::new(items).render(inner, buf);
    }
}

fn task_icon(status: &str, glyphs: RenderGlyphs) -> &'static str {
    match (status, glyphs.mode) {
        ("in_progress", RenderGlyphMode::Unicode) => "◐",
        ("in_progress", RenderGlyphMode::Ascii) => "*",
        ("completed", RenderGlyphMode::Unicode) => "✓",
        ("completed", RenderGlyphMode::Ascii) => "x",
        (_, RenderGlyphMode::Unicode) => "○",
        (_, RenderGlyphMode::Ascii) => "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    #[test]
    fn task_list_clips_multibyte_content() {
        let theme = Theme::default();
        let tasks = vec![TodoItem {
            id: "render".into(),
            content: "完整渲染红线：逐行阅读，不能把旧组件当成果".into(),
            status: "in_progress".into(),
        }];
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 4));

        TaskListV2Widget::new(&tasks, &theme).render(buf.area, &mut buf);

        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(rendered.contains("Tasks"));
        assert!(rendered.contains('逐') || rendered.contains('…'));
    }

    #[test]
    fn task_list_can_render_ascii_status_and_truncation() {
        let theme = Theme::default();
        let tasks = vec![TodoItem {
            id: "render".into(),
            content: "render profile truncation should stay plain".into(),
            status: "completed".into(),
        }];
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 4));

        TaskListV2Widget::new(&tasks, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(rendered.contains('x'), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        assert!(!rendered.contains('…'), "{rendered}");
    }
}
