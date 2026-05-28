use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::RenderGlyphs;
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
        let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
        if used.saturating_add(ch_width) > content_width {
            out.push_str(suffix);
            return out;
        }
        used = used.saturating_add(ch_width);
        out.push(ch);
    }
    out
}

fn selected_window_start(len: usize, visible_count: usize, selected: usize) -> usize {
    if len <= visible_count {
        return 0;
    }
    selected
        .min(len.saturating_sub(1))
        .saturating_add(1)
        .saturating_sub(visible_count)
        .min(len.saturating_sub(visible_count))
}

fn panel_block<'a>(title: &'a str, theme: &Theme, glyphs: RenderGlyphs) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(glyphs.border)
        .border_style(theme.style_border())
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub supports_thinking: bool,
    pub supports_streaming: bool,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct ModelPickerState {
    pub models: Vec<ModelInfo>,
    pub selected: usize,
    pub filter: String,
}

impl ModelPickerState {
    pub fn new(models: Vec<ModelInfo>) -> Self {
        Self {
            models,
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn filtered(&self) -> Vec<(usize, &ModelInfo)> {
        if self.filter.is_empty() {
            self.models.iter().enumerate().collect()
        } else {
            let q = self.filter.to_lowercase();
            self.models
                .iter()
                .enumerate()
                .filter(|(_, model)| {
                    model.name.to_lowercase().contains(&q) || model.id.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max = self.filtered().len();
        if self.selected + 1 < max {
            self.selected += 1;
        }
    }
}

pub struct ModelPickerWidget<'a> {
    state: &'a ModelPickerState,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> ModelPickerWidget<'a> {
    pub fn new(state: &'a ModelPickerState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for ModelPickerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 24 || area.height < 5 {
            return;
        }

        let block = panel_block("Select Model", self.theme, self.glyphs);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        let filter_line = if self.state.filter.is_empty() {
            Line::from(Span::styled(
                "Type to filter models...",
                Style::default().fg(self.theme.text_dim),
            ))
        } else {
            let prefix = "Search: ";
            let filter = truncate_display_width(
                &self.state.filter,
                (chunks[0].width as usize).saturating_sub(UnicodeWidthStr::width(prefix)),
                self.glyphs,
            );
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(self.theme.text_dim)),
                Span::styled(filter, Style::default().fg(self.theme.text)),
            ])
        };
        buf.set_line(chunks[0].x, chunks[0].y, &filter_line, chunks[0].width);

        let filtered = self.state.filtered();
        let visible_count = chunks[1].height as usize;
        let start = selected_window_start(filtered.len(), visible_count, self.state.selected);

        for (row, (visible_index, (_, model))) in filtered
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_count)
            .enumerate()
        {
            let y = chunks[1].y + row as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }

            let selected = visible_index == self.state.selected;
            let bg = if selected {
                self.theme.selection
            } else {
                Color::Reset
            };
            for x in chunks[1].x..chunks[1].x + chunks[1].width {
                buf.set_string(x, y, " ", Style::default().bg(bg));
            }

            let prefix = if selected {
                format!("{} ", self.glyphs.selected_indicator())
            } else {
                "  ".to_string()
            };
            let current = if model.is_current { "* " } else { "" };
            let provider = format!("  ({})", model.provider);
            let thinking = if model.supports_thinking {
                " think"
            } else {
                ""
            };
            let fixed_width = UnicodeWidthStr::width(prefix.as_str())
                + UnicodeWidthStr::width(current)
                + UnicodeWidthStr::width(provider.as_str())
                + UnicodeWidthStr::width(thinking);
            let name = truncate_display_width(
                &model.name,
                (chunks[1].width as usize).saturating_sub(fixed_width),
                self.glyphs,
            );

            let mut spans = vec![Span::styled(
                prefix,
                Style::default().fg(self.theme.primary).bg(bg),
            )];
            if model.is_current {
                spans.push(Span::styled(
                    current,
                    Style::default().fg(self.theme.success).bg(bg),
                ));
            }
            spans.push(Span::styled(
                name,
                Style::default()
                    .fg(self.theme.text)
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ));
            spans.push(Span::styled(
                provider,
                Style::default().fg(self.theme.text_dim).bg(bg),
            ));
            if model.supports_thinking {
                spans.push(Span::styled(
                    thinking,
                    Style::default().fg(self.theme.secondary).bg(bg),
                ));
            }
            buf.set_line(chunks[1].x, y, &Line::from(spans), chunks[1].width);
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub title: String,
    pub category: String,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct MemoryPanelState {
    pub entries: Vec<MemoryEntry>,
    pub selected: usize,
}

impl MemoryPanelState {
    pub fn new(entries: Vec<MemoryEntry>) -> Self {
        Self {
            entries,
            selected: 0,
        }
    }
}

pub struct MemoryPanelWidget<'a> {
    state: &'a MemoryPanelState,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> MemoryPanelWidget<'a> {
    pub fn new(state: &'a MemoryPanelState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for MemoryPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 16 || area.height < 3 {
            return;
        }
        let block = panel_block("Recall", self.theme, self.glyphs);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let visible_count = inner.height as usize;
        let start =
            selected_window_start(self.state.entries.len(), visible_count, self.state.selected);
        let items = self
            .state
            .entries
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_count)
            .map(|(i, entry)| {
                let selected = i == self.state.selected;
                let bg = if selected {
                    self.theme.selection
                } else {
                    Color::Reset
                };
                let prefix = if selected {
                    format!("{} ", self.glyphs.selected_indicator())
                } else {
                    "  ".to_string()
                };
                let category = format!("  [{}]", entry.category);
                let fixed_width = UnicodeWidthStr::width(prefix.as_str())
                    + UnicodeWidthStr::width(category.as_str());
                let title = truncate_display_width(
                    &entry.title,
                    (inner.width as usize).saturating_sub(fixed_width),
                    self.glyphs,
                );
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(self.theme.primary).bg(bg)),
                    Span::styled(
                        title,
                        Style::default()
                            .fg(self.theme.text)
                            .bg(bg)
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(category, Style::default().fg(self.theme.text_dim).bg(bg)),
                ]))
            })
            .collect::<Vec<_>>();
        List::new(items).render(inner, buf);
    }
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SkillsPanelState {
    pub skills: Vec<SkillInfo>,
    pub selected: usize,
}

impl SkillsPanelState {
    pub fn new(skills: Vec<SkillInfo>) -> Self {
        Self {
            skills,
            selected: 0,
        }
    }
}

pub struct SkillsPanelWidget<'a> {
    state: &'a SkillsPanelState,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> SkillsPanelWidget<'a> {
    pub fn new(state: &'a SkillsPanelState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for SkillsPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 16 || area.height < 3 {
            return;
        }
        let block = panel_block("Crafts", self.theme, self.glyphs);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let visible_count = inner.height as usize;
        let start =
            selected_window_start(self.state.skills.len(), visible_count, self.state.selected);
        for (row, (i, skill)) in self
            .state
            .skills
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_count)
            .enumerate()
        {
            let y = inner.y + row as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let selected = i == self.state.selected;
            let bg = if selected {
                self.theme.selection
            } else {
                Color::Reset
            };
            let checkbox = if skill.enabled { "[x]" } else { "[ ]" };
            let prefix = format!(
                "{} {} ",
                if selected {
                    self.glyphs.selected_indicator()
                } else {
                    " "
                },
                checkbox
            );
            let prefix_width = UnicodeWidthStr::width(prefix.as_str());
            let available = (inner.width as usize).saturating_sub(prefix_width);
            let desc_budget = (available / 2).min(36);
            let name_budget = available.saturating_sub(desc_budget);
            let name = truncate_display_width(&skill.name, name_budget, self.glyphs);
            let description = truncate_display_width(
                &format!("  {}", skill.description),
                available.saturating_sub(UnicodeWidthStr::width(name.as_str())),
                self.glyphs,
            );
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(self.theme.text).bg(bg)),
                Span::styled(
                    name,
                    Style::default()
                        .fg(self.theme.text)
                        .bg(bg)
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(description, Style::default().fg(self.theme.text_dim).bg(bg)),
            ]);
            buf.set_line(inner.x, y, &line, inner.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn panels_clip_multibyte_rows_on_active_widgets() {
        let theme = Theme::default();
        let model = ModelPickerState::new(vec![ModelInfo {
            id: "m2".into(),
            name: "example-fast 逐行阅读核心代码模型".into(),
            provider: "custom".into(),
            supports_thinking: true,
            supports_streaming: true,
            is_current: true,
        }]);
        let mut buf = Buffer::empty(Rect::new(0, 0, 36, 6));
        ModelPickerWidget::new(&model, &theme).render(buf.area, &mut buf);
        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Select Model"));
        assert!(rendered.contains("example-fast"));
        assert!(!rendered.contains("legacy-dialog-marker"));
    }

    #[test]
    fn panels_can_render_ascii_borders_and_truncation() {
        let theme = Theme::default();
        let model = ModelPickerState::new(vec![ModelInfo {
            id: "m2".into(),
            name: "example-fast plain rendering profile clipping".into(),
            provider: "custom".into(),
            supports_thinking: false,
            supports_streaming: true,
            is_current: true,
        }]);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 6));

        ModelPickerWidget::new(&model, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains('+'), "{rendered}");
        assert!(rendered.contains("..."), "{rendered}");
        assert!(!rendered.contains('…'), "{rendered}");
    }

    #[test]
    fn model_picker_window_follows_selected_profile() {
        let theme = Theme::default();
        let mut model = ModelPickerState::new(
            (0..14)
                .map(|index| ModelInfo {
                    id: format!("profile-{index:02}"),
                    name: format!("Model {index:02}"),
                    provider: "settings".into(),
                    supports_thinking: false,
                    supports_streaming: true,
                    is_current: index == 11,
                })
                .collect(),
        );
        model.selected = 11;
        let mut buf = Buffer::empty(Rect::new(0, 0, 48, 8));

        ModelPickerWidget::new(&model, &theme).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Model 11"), "{rendered}");
        assert!(rendered.contains("Model 07"), "{rendered}");
        assert!(
            !rendered.contains("Model 00"),
            "model picker should scroll past initial rows:\n{rendered}"
        );
    }

    #[test]
    fn skills_and_memory_panels_follow_selected_item() {
        let theme = Theme::default();
        let skills = SkillsPanelState {
            skills: (0..12)
                .map(|index| SkillInfo {
                    name: format!("skill-{index:02}"),
                    description: "project skill".into(),
                    enabled: true,
                })
                .collect(),
            selected: 10,
        };
        let memory = MemoryPanelState {
            entries: (0..12)
                .map(|index| MemoryEntry {
                    title: format!("memory-{index:02}"),
                    category: "project".into(),
                    preview: String::new(),
                })
                .collect(),
            selected: 10,
        };

        let mut skill_buf = Buffer::empty(Rect::new(0, 0, 48, 6));
        SkillsPanelWidget::new(&skills, &theme).render(skill_buf.area, &mut skill_buf);
        let rendered_skills = buffer_text(&skill_buf);
        assert!(rendered_skills.contains("skill-10"), "{rendered_skills}");
        assert!(!rendered_skills.contains("skill-00"), "{rendered_skills}");

        let mut memory_buf = Buffer::empty(Rect::new(0, 0, 48, 6));
        MemoryPanelWidget::new(&memory, &theme).render(memory_buf.area, &mut memory_buf);
        let rendered_memory = buffer_text(&memory_buf);
        assert!(rendered_memory.contains("memory-10"), "{rendered_memory}");
        assert!(!rendered_memory.contains("memory-00"), "{rendered_memory}");
    }
}
