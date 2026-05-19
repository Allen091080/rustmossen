//! Custom select / fuzzy picker widget.
//!
//! Translates: components/CustomSelect/ (10 files) into a searchable,
//! scrollable selection list with fuzzy matching support.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// An option in the custom select list.
#[derive(Debug, Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub disabled: bool,
    pub group: Option<String>,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            icon: None,
            disabled: false,
            group: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// State for the custom select widget.
#[derive(Debug, Clone)]
pub struct CustomSelectState {
    pub options: Vec<SelectOption>,
    pub filtered: Vec<usize>,
    pub selected_index: usize,
    pub search_query: String,
    pub is_open: bool,
    pub scroll_offset: usize,
}

impl CustomSelectState {
    pub fn new(options: Vec<SelectOption>) -> Self {
        let count = options.len();
        let filtered: Vec<usize> = (0..count).collect();
        Self {
            options,
            filtered,
            selected_index: 0,
            search_query: String::new(),
            is_open: true,
            scroll_offset: 0,
        }
    }

    /// Filter options by search query (fuzzy match on label).
    pub fn filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.options.len()).collect();
        } else {
            self.filtered = self
                .options
                .iter()
                .enumerate()
                .filter(|(_, opt)| {
                    let label_lower = opt.label.to_lowercase();
                    fuzzy_match(&label_lower, &query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.filtered.len() {
            self.selected_index += 1;
        }
    }

    pub fn selected_option(&self) -> Option<&SelectOption> {
        self.filtered
            .get(self.selected_index)
            .and_then(|&i| self.options.get(i))
    }

    pub fn insert_char(&mut self, c: char) {
        self.search_query.push(c);
        self.filter();
    }

    pub fn delete_char(&mut self) {
        self.search_query.pop();
        self.filter();
    }
}

/// Simple fuzzy match: all chars of pattern appear in order in haystack.
fn fuzzy_match(haystack: &str, pattern: &str) -> bool {
    let mut haystack_chars = haystack.chars();
    for pc in pattern.chars() {
        loop {
            match haystack_chars.next() {
                Some(hc) if hc == pc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// CustomSelectWidget
// ---------------------------------------------------------------------------

pub struct CustomSelectWidget<'a> {
    pub state: &'a CustomSelectState,
    pub title: &'a str,
    pub placeholder: &'a str,
    pub theme: &'a Theme,
    pub max_visible: usize,
}

impl<'a> CustomSelectWidget<'a> {
    pub fn new(state: &'a CustomSelectState, theme: &'a Theme) -> Self {
        Self {
            state,
            title: "Select",
            placeholder: "Type to filter...",
            theme,
            max_visible: 10,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    pub fn placeholder(mut self, p: &'a str) -> Self {
        self.placeholder = p;
        self
    }

    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }
}

impl<'a> Widget for CustomSelectWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.style_border_focused())
            .title(Span::styled(
                self.title,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Search input line
        let input_display = if self.state.search_query.is_empty() {
            Line::from(Span::styled(
                self.placeholder,
                Style::default().fg(self.theme.text_dim),
            ))
        } else {
            Line::from(vec![
                Span::styled("🔍 ", Style::default()),
                Span::styled(
                    &self.state.search_query,
                    Style::default().fg(self.theme.text),
                ),
            ])
        };
        buf.set_line(chunks[0].x, chunks[0].y, &input_display, chunks[0].width);

        // Options list
        let visible_count = self.max_visible.min(chunks[1].height as usize);
        let scroll = self.state.scroll_offset;

        for (vi, &opt_idx) in self
            .state
            .filtered
            .iter()
            .skip(scroll)
            .take(visible_count)
            .enumerate()
        {
            let y = chunks[1].y + vi as u16;
            if y >= chunks[1].y + chunks[1].height {
                break;
            }

            let opt = &self.state.options[opt_idx];
            let is_selected = vi + scroll == self.state.selected_index;

            let (fg, bg) = if is_selected {
                (self.theme.text, self.theme.selection)
            } else if opt.disabled {
                (self.theme.text_subtle, Color::Reset)
            } else {
                (self.theme.text, Color::Reset)
            };

            // Background
            let bg_style = Style::default().bg(bg);
            for x in chunks[1].x..chunks[1].x + chunks[1].width {
                buf.set_string(x, y, " ", bg_style);
            }

            let mut x = chunks[1].x;

            // Indicator
            if is_selected {
                buf.set_string(x, y, "▸", Style::default().fg(self.theme.primary).bg(bg));
            }
            x += 2;

            // Icon
            if let Some(ref icon) = opt.icon {
                buf.set_string(x, y, icon, Style::default().fg(fg).bg(bg));
                x += 2;
            }

            // Label
            let label_style = if is_selected {
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg).bg(bg)
            };
            let avail = (chunks[1].x + chunks[1].width).saturating_sub(x) as usize;
            let label: String = opt.label.chars().take(avail).collect();
            buf.set_string(x, y, &label, label_style);
            x += label.len() as u16;

            // Description
            if let Some(ref desc) = opt.description {
                if x + 2 < chunks[1].x + chunks[1].width {
                    x += 1;
                    let desc_avail = (chunks[1].x + chunks[1].width).saturating_sub(x) as usize;
                    let desc_trunc: String = desc.chars().take(desc_avail).collect();
                    buf.set_string(
                        x,
                        y,
                        &desc_trunc,
                        Style::default().fg(self.theme.text_dim).bg(bg),
                    );
                }
            }
        }

        // Result count
        let count_text = format!("{}/{}", self.state.filtered.len(), self.state.options.len());
        let count_x = inner.x + inner.width.saturating_sub(count_text.len() as u16 + 1);
        buf.set_string(
            count_x,
            chunks[0].y,
            &count_text,
            Style::default().fg(self.theme.text_subtle),
        );
    }
}
