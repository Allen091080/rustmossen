//! Extended custom select components — multi-select, input options, pagination.
//!
//! Translates: CustomSelect/index.ts, CustomSelect/option-map.ts,
//! CustomSelect/select-input-option.tsx, CustomSelect/select-option.tsx,
//! CustomSelect/select.tsx, CustomSelect/SelectMulti.tsx,
//! CustomSelect/use-multi-select-state.ts, CustomSelect/use-select-input.ts,
//! CustomSelect/use-select-navigation.ts, CustomSelect/use-select-state.ts

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};

use crate::theme::Theme;

// ===================================================================
// OptionMap — from option-map.ts
// ===================================================================

/// A linked-list-style option map for O(1) prev/next navigation.
#[derive(Debug, Clone)]
pub struct OptionMapItem<T: Clone + PartialEq> {
    pub label: String,
    pub value: T,
    pub description: Option<String>,
    pub index: usize,
    pub previous_index: Option<usize>,
    pub next_index: Option<usize>,
}

/// Ordered map of options with linked prev/next for navigation.
#[derive(Debug, Clone)]
pub struct OptionMap<T: Clone + PartialEq> {
    pub items: Vec<OptionMapItem<T>>,
}

impl<T: Clone + PartialEq> OptionMap<T> {
    pub fn new(options: &[SelectOptionItem<T>]) -> Self {
        let items: Vec<OptionMapItem<T>> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| OptionMapItem {
                label: opt.label.clone(),
                value: opt.value.clone(),
                description: opt.description.clone(),
                index: i,
                previous_index: if i > 0 { Some(i - 1) } else { None },
                next_index: if i + 1 < options.len() {
                    Some(i + 1)
                } else {
                    None
                },
            })
            .collect();
        Self { items }
    }

    pub fn first(&self) -> Option<&OptionMapItem<T>> {
        self.items.first()
    }

    pub fn last(&self) -> Option<&OptionMapItem<T>> {
        self.items.last()
    }

    pub fn get_by_value(&self, value: &T) -> Option<&OptionMapItem<T>> {
        self.items.iter().find(|item| &item.value == value)
    }

    pub fn get_by_index(&self, index: usize) -> Option<&OptionMapItem<T>> {
        self.items.get(index)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ===================================================================
// SelectOption types — from select.tsx types
// ===================================================================

/// Type of option in the select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectOptionType {
    /// Normal text option.
    Text,
    /// Input option (user can type).
    Input,
}

/// An option with description and optional type info.
#[derive(Debug, Clone)]
pub struct SelectOptionItem<T: Clone + PartialEq> {
    pub value: T,
    pub label: String,
    pub description: Option<String>,
    pub description_color: Option<Color>,
    pub dim_description: bool,
    pub option_type: SelectOptionType,
    pub disabled: bool,
    pub placeholder: Option<String>,
    pub initial_value: Option<String>,
    pub allow_empty_submit_to_cancel: bool,
    pub reset_cursor_on_update: bool,
}

impl<T: Clone + PartialEq> SelectOptionItem<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            description: None,
            description_color: None,
            dim_description: true,
            option_type: SelectOptionType::Text,
            disabled: false,
            placeholder: None,
            initial_value: None,
            allow_empty_submit_to_cancel: false,
            reset_cursor_on_update: false,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_input(mut self) -> Self {
        self.option_type = SelectOptionType::Input;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

// ===================================================================
// SelectNavigation — from use-select-navigation.ts
// ===================================================================

/// Navigation state for a select list with windowed visibility.
#[derive(Debug, Clone)]
pub struct SelectNavigationState<T: Clone + PartialEq> {
    pub options: Vec<SelectOptionItem<T>>,
    pub focused_value: Option<T>,
    pub visible_from_index: usize,
    pub visible_to_index: usize,
    pub visible_option_count: usize,
}

impl<T: Clone + PartialEq> SelectNavigationState<T> {
    pub fn new(options: Vec<SelectOptionItem<T>>, visible_count: usize) -> Self {
        let focused = options.first().map(|o| o.value.clone());
        let to_idx = visible_count.min(options.len());
        Self {
            options,
            focused_value: focused,
            visible_from_index: 0,
            visible_to_index: to_idx,
            visible_option_count: visible_count,
        }
    }

    pub fn with_initial_focus(mut self, value: T) -> Self {
        if let Some(idx) = self.options.iter().position(|o| o.value == value) {
            self.focused_value = Some(value);
            // Adjust viewport to show focused item
            if idx >= self.visible_to_index {
                self.visible_to_index = (idx + 1).min(self.options.len());
                self.visible_from_index = self.visible_to_index.saturating_sub(self.visible_option_count);
            } else if idx < self.visible_from_index {
                self.visible_from_index = idx;
                self.visible_to_index = (idx + self.visible_option_count).min(self.options.len());
            }
        }
        self
    }

    /// 1-based index of the focused option.
    pub fn focused_index(&self) -> usize {
        match &self.focused_value {
            Some(val) => self
                .options
                .iter()
                .position(|o| &o.value == val)
                .map_or(0, |i| i + 1),
            None => 0,
        }
    }

    /// Get visible options slice with original indices.
    pub fn visible_options(&self) -> Vec<(usize, &SelectOptionItem<T>)> {
        self.options
            .iter()
            .enumerate()
            .skip(self.visible_from_index)
            .take(self.visible_to_index - self.visible_from_index)
            .collect()
    }

    /// Whether the focused option is an input type.
    pub fn is_in_input(&self) -> bool {
        if let Some(ref val) = self.focused_value {
            self.options
                .iter()
                .find(|o| &o.value == val)
                .map_or(false, |o| o.option_type == SelectOptionType::Input)
        } else {
            false
        }
    }

    pub fn focus_next_option(&mut self) {
        let current_idx = self.focused_value.as_ref().and_then(|v| {
            self.options.iter().position(|o| &o.value == v)
        });
        let next_idx = match current_idx {
            Some(i) if i + 1 < self.options.len() => i + 1,
            None if !self.options.is_empty() => 0,
            _ => return,
        };
        self.focused_value = Some(self.options[next_idx].value.clone());
        // Scroll down if needed
        if next_idx >= self.visible_to_index {
            self.visible_to_index = next_idx + 1;
            self.visible_from_index = self.visible_to_index.saturating_sub(self.visible_option_count);
        }
    }

    pub fn focus_previous_option(&mut self) {
        let current_idx = self.focused_value.as_ref().and_then(|v| {
            self.options.iter().position(|o| &o.value == v)
        });
        let prev_idx = match current_idx {
            Some(i) if i > 0 => i - 1,
            _ => return,
        };
        self.focused_value = Some(self.options[prev_idx].value.clone());
        // Scroll up if needed
        if prev_idx < self.visible_from_index {
            self.visible_from_index = prev_idx;
            self.visible_to_index = (prev_idx + self.visible_option_count).min(self.options.len());
        }
    }

    pub fn focus_next_page(&mut self) {
        let current_idx = self.focused_value.as_ref().and_then(|v| {
            self.options.iter().position(|o| &o.value == v)
        }).unwrap_or(0);
        let page_size = self.visible_option_count;
        let new_idx = (current_idx + page_size).min(self.options.len().saturating_sub(1));
        self.focused_value = Some(self.options[new_idx].value.clone());
        // Adjust viewport
        self.visible_to_index = (new_idx + 1).min(self.options.len());
        self.visible_from_index = self.visible_to_index.saturating_sub(self.visible_option_count);
    }

    pub fn focus_previous_page(&mut self) {
        let current_idx = self.focused_value.as_ref().and_then(|v| {
            self.options.iter().position(|o| &o.value == v)
        }).unwrap_or(0);
        let page_size = self.visible_option_count;
        let new_idx = current_idx.saturating_sub(page_size);
        self.focused_value = Some(self.options[new_idx].value.clone());
        // Adjust viewport
        self.visible_from_index = new_idx;
        self.visible_to_index = (new_idx + self.visible_option_count).min(self.options.len());
    }

    pub fn focus_option(&mut self, value: T) {
        if self.options.iter().any(|o| o.value == value) {
            self.focused_value = Some(value.clone());
            if let Some(idx) = self.options.iter().position(|o| o.value == value) {
                if idx >= self.visible_to_index {
                    self.visible_to_index = idx + 1;
                    self.visible_from_index = self.visible_to_index.saturating_sub(self.visible_option_count);
                } else if idx < self.visible_from_index {
                    self.visible_from_index = idx;
                    self.visible_to_index = (idx + self.visible_option_count).min(self.options.len());
                }
            }
        }
    }

    pub fn reset_with_options(&mut self, new_options: Vec<SelectOptionItem<T>>, preserve_focus: Option<T>) {
        let focus = preserve_focus
            .and_then(|v| new_options.iter().find(|o| o.value == v).map(|o| o.value.clone()))
            .or_else(|| new_options.first().map(|o| o.value.clone()));
        self.options = new_options;
        self.focused_value = focus.clone();
        // Recompute viewport
        let focus_idx = focus.and_then(|v| self.options.iter().position(|o| o.value == v)).unwrap_or(0);
        self.visible_from_index = focus_idx.saturating_sub(self.visible_option_count / 2);
        self.visible_to_index = (self.visible_from_index + self.visible_option_count).min(self.options.len());
    }
}

// ===================================================================
// SelectState — from use-select-state.ts
// ===================================================================

/// Full select state combining navigation and selection.
#[derive(Debug, Clone)]
pub struct SelectState<T: Clone + PartialEq> {
    pub navigation: SelectNavigationState<T>,
    pub selected_value: Option<T>,
}

impl<T: Clone + PartialEq> SelectState<T> {
    pub fn new(options: Vec<SelectOptionItem<T>>, visible_count: usize) -> Self {
        Self {
            navigation: SelectNavigationState::new(options, visible_count),
            selected_value: None,
        }
    }

    pub fn with_default(mut self, value: T) -> Self {
        self.selected_value = Some(value);
        self
    }

    pub fn select_focused(&mut self) {
        self.selected_value = self.navigation.focused_value.clone();
    }

    pub fn focused_value(&self) -> Option<&T> {
        self.navigation.focused_value.as_ref()
    }

    pub fn focus_next(&mut self) {
        self.navigation.focus_next_option();
    }

    pub fn focus_previous(&mut self) {
        self.navigation.focus_previous_option();
    }

    pub fn focus_next_page(&mut self) {
        self.navigation.focus_next_page();
    }

    pub fn focus_previous_page(&mut self) {
        self.navigation.focus_previous_page();
    }
}

// ===================================================================
// MultiSelectState — from use-multi-select-state.ts
// ===================================================================

/// State for multi-select with checkboxes.
#[derive(Debug, Clone)]
pub struct MultiSelectState<T: Clone + PartialEq> {
    pub navigation: SelectNavigationState<T>,
    pub selected_values: Vec<T>,
    pub is_submit_focused: bool,
    pub input_values: std::collections::HashMap<String, String>,
}

impl<T: Clone + PartialEq + std::fmt::Debug + std::hash::Hash + Eq> MultiSelectState<T> {
    pub fn new(
        options: Vec<SelectOptionItem<T>>,
        visible_count: usize,
        default_selected: Vec<T>,
    ) -> Self {
        let mut input_values = std::collections::HashMap::new();
        for opt in &options {
            if opt.option_type == SelectOptionType::Input {
                if let Some(ref initial) = opt.initial_value {
                    input_values.insert(format!("{:?}", opt.value), initial.clone());
                }
            }
        }
        Self {
            navigation: SelectNavigationState::new(options, visible_count),
            selected_values: default_selected,
            is_submit_focused: false,
            input_values,
        }
    }

    pub fn toggle_focused(&mut self) {
        if let Some(ref val) = self.navigation.focused_value {
            if let Some(pos) = self.selected_values.iter().position(|v| v == val) {
                self.selected_values.remove(pos);
            } else {
                self.selected_values.push(val.clone());
            }
        }
    }

    pub fn is_selected(&self, value: &T) -> bool {
        self.selected_values.contains(value)
    }

    pub fn select_all(&mut self) {
        self.selected_values = self
            .navigation
            .options
            .iter()
            .filter(|o| !o.disabled)
            .map(|o| o.value.clone())
            .collect();
    }

    pub fn deselect_all(&mut self) {
        self.selected_values.clear();
    }

    pub fn focus_submit(&mut self) {
        self.is_submit_focused = true;
    }

    pub fn unfocus_submit(&mut self) {
        self.is_submit_focused = false;
    }

    pub fn move_up(&mut self) {
        if self.is_submit_focused {
            self.is_submit_focused = false;
            // Focus last option
            if let Some(last) = self.navigation.options.last() {
                let val = last.value.clone();
                self.navigation.focus_option(val);
            }
        } else {
            self.navigation.focus_previous_option();
        }
    }

    pub fn move_down(&mut self) {
        if self.is_submit_focused {
            return;
        }
        // Check if at last option
        let at_last = self.navigation.focused_value.as_ref().and_then(|v| {
            self.navigation.options.iter().position(|o| &o.value == v)
        }).map_or(false, |i| i + 1 >= self.navigation.options.len());

        if at_last {
            self.is_submit_focused = true;
        } else {
            self.navigation.focus_next_option();
        }
    }

    pub fn set_input_value(&mut self, key: &str, value: String) {
        self.input_values.insert(key.to_string(), value);
    }

    pub fn get_input_value(&self, key: &str) -> &str {
        self.input_values.get(key).map(|s| s.as_str()).unwrap_or("")
    }
}

// ===================================================================
// UseSelectInput — from use-select-input.ts
// ===================================================================

/// Input handling mode for the select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectInputMode {
    /// Normal navigation mode.
    Navigate,
    /// Typing in an input option.
    Input,
}

/// Input handler state for select components.
#[derive(Debug, Clone)]
pub struct SelectInputHandler {
    pub mode: SelectInputMode,
    pub typeahead_buffer: String,
    pub typeahead_active: bool,
}

impl SelectInputHandler {
    pub fn new() -> Self {
        Self {
            mode: SelectInputMode::Navigate,
            typeahead_buffer: String::new(),
            typeahead_active: false,
        }
    }

    pub fn start_typeahead(&mut self) {
        self.typeahead_active = true;
        self.typeahead_buffer.clear();
    }

    pub fn append_typeahead(&mut self, c: char) {
        self.typeahead_buffer.push(c);
    }

    pub fn clear_typeahead(&mut self) {
        self.typeahead_buffer.clear();
        self.typeahead_active = false;
    }

    pub fn enter_input_mode(&mut self) {
        self.mode = SelectInputMode::Input;
    }

    pub fn exit_input_mode(&mut self) {
        self.mode = SelectInputMode::Navigate;
    }

    /// Match typeahead to first option starting with the buffer text.
    pub fn typeahead_match<T: Clone + PartialEq>(
        &self,
        options: &[SelectOptionItem<T>],
    ) -> Option<T> {
        if self.typeahead_buffer.is_empty() {
            return None;
        }
        let lower = self.typeahead_buffer.to_lowercase();
        options
            .iter()
            .find(|o| o.label.to_lowercase().starts_with(&lower))
            .map(|o| o.value.clone())
    }
}

impl Default for SelectInputHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ===================================================================
// SelectOptionWidget — from select-option.tsx
// ===================================================================

/// Renders a single option line with focus/selected indicators and arrows.
pub struct SelectOptionWidget<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub is_focused: bool,
    pub is_selected: bool,
    pub show_up_arrow: bool,
    pub show_down_arrow: bool,
    pub index_display: Option<String>,
    pub theme: &'a Theme,
}

impl<'a> SelectOptionWidget<'a> {
    pub fn new(label: &'a str, theme: &'a Theme) -> Self {
        Self {
            label,
            description: None,
            is_focused: false,
            is_selected: false,
            show_up_arrow: false,
            show_down_arrow: false,
            index_display: None,
            theme,
        }
    }

    pub fn focused(mut self) -> Self {
        self.is_focused = true;
        self
    }

    pub fn selected(mut self) -> Self {
        self.is_selected = true;
        self
    }

    pub fn with_arrows(mut self, up: bool, down: bool) -> Self {
        self.show_up_arrow = up;
        self.show_down_arrow = down;
        self
    }

    pub fn with_index(mut self, idx: usize, max_width: usize) -> Self {
        self.index_display = Some(format!("{:>width$}.", idx, width = max_width));
        self
    }
}

impl<'a> Widget for SelectOptionWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 4 {
            return;
        }

        let mut x = area.x;
        let max_x = area.x + area.width;

        // Up/down arrow indicators (top-right corner)
        if self.show_up_arrow {
            let arrow_x = max_x.saturating_sub(2);
            buf.set_string(
                arrow_x,
                area.y,
                "↑",
                Style::default().fg(Color::DarkGray),
            );
        }
        if self.show_down_arrow {
            let arrow_x = max_x.saturating_sub(2);
            buf.set_string(
                arrow_x,
                area.y,
                "↓",
                Style::default().fg(Color::DarkGray),
            );
        }

        // Focus indicator
        let indicator = if self.is_focused {
            "❯ "
        } else {
            "  "
        };
        let ind_style = if self.is_focused {
            Style::default().fg(self.theme.primary)
        } else {
            Style::default()
        };
        buf.set_string(x, area.y, indicator, ind_style);
        x += 2;

        // Index display
        if let Some(ref idx) = self.index_display {
            buf.set_string(x, area.y, idx, Style::default().fg(Color::DarkGray));
            x += idx.len() as u16 + 1;
        }

        // Label
        let label_color = if self.is_selected {
            Color::Green
        } else if self.is_focused {
            self.theme.primary
        } else {
            self.theme.text
        };
        let label_style = Style::default().fg(label_color);
        let avail = max_x.saturating_sub(x) as usize;
        let label_display: String = self.label.chars().take(avail).collect();
        buf.set_string(x, area.y, &label_display, label_style);
        x += label_display.len() as u16;

        // Inline description
        if let Some(desc) = self.description {
            if x + 2 < max_x {
                x += 1;
                let desc_avail = max_x.saturating_sub(x) as usize;
                let desc_display: String = desc.chars().take(desc_avail).collect();
                let desc_color = if self.is_selected {
                    Color::Green
                } else if self.is_focused {
                    self.theme.primary
                } else {
                    Color::DarkGray
                };
                buf.set_string(x, area.y, &desc_display, Style::default().fg(desc_color));
            }
        }
    }
}

// ===================================================================
// SelectWidget — full-featured select from select.tsx
// ===================================================================

/// Layout mode for the select widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectLayout {
    /// Compact: label and description on same line.
    Compact,
    /// Expanded: description below label.
    Expanded,
    /// Two-column: label left, description right.
    TwoColumn,
}

/// Full-featured select widget.
pub struct SelectWidget<'a, T: Clone + PartialEq> {
    pub state: &'a SelectState<T>,
    pub layout: SelectLayout,
    pub inline_descriptions: bool,
    pub hide_indexes: bool,
    pub highlight_text: Option<&'a str>,
    pub disabled: bool,
    pub theme: &'a Theme,
}

impl<'a, T: Clone + PartialEq> SelectWidget<'a, T> {
    pub fn new(state: &'a SelectState<T>, theme: &'a Theme) -> Self {
        Self {
            state,
            layout: SelectLayout::Compact,
            inline_descriptions: false,
            hide_indexes: false,
            highlight_text: None,
            disabled: false,
            theme,
        }
    }

    pub fn layout(mut self, layout: SelectLayout) -> Self {
        self.layout = layout;
        self
    }

    pub fn inline_descriptions(mut self) -> Self {
        self.inline_descriptions = true;
        self
    }

    pub fn hide_indexes(mut self) -> Self {
        self.hide_indexes = true;
        self
    }

    pub fn highlight(mut self, text: &'a str) -> Self {
        self.highlight_text = Some(text);
        self
    }
}

impl<'a, T: Clone + PartialEq> Widget for SelectWidget<'a, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        let visible = self.state.navigation.visible_options();
        let total = self.state.navigation.options.len();
        let max_index_width = total.to_string().len();

        let are_more_above = self.state.navigation.visible_from_index > 0;
        let are_more_below = self.state.navigation.visible_to_index < total;

        let line_height = match self.layout {
            SelectLayout::Expanded => 2u16,
            _ => 1u16,
        };

        for (vi, (orig_idx, opt)) in visible.iter().enumerate() {
            let y = area.y + (vi as u16) * line_height;
            if y >= area.y + area.height {
                break;
            }

            let is_first = vi == 0;
            let is_last = vi == visible.len() - 1;
            let is_focused = !self.disabled
                && self.state.navigation.focused_value.as_ref() == Some(&opt.value);
            let is_selected = self.state.selected_value.as_ref() == Some(&opt.value);

            let line_area = Rect::new(area.x, y, area.width, 1);

            let mut option_widget = SelectOptionWidget::new(&opt.label, self.theme);
            if is_focused {
                option_widget = option_widget.focused();
            }
            if is_selected {
                option_widget = option_widget.selected();
            }
            option_widget = option_widget.with_arrows(
                are_more_above && is_first,
                are_more_below && is_last,
            );
            if !self.hide_indexes {
                option_widget = option_widget.with_index(orig_idx + 1, max_index_width);
            }
            if self.inline_descriptions {
                option_widget.description = opt.description.as_deref();
            }
            option_widget.render(line_area, buf);

            // Expanded layout: description on next line
            if self.layout == SelectLayout::Expanded && !self.inline_descriptions {
                if let Some(ref desc) = opt.description {
                    let desc_y = y + 1;
                    if desc_y < area.y + area.height {
                        let pad = if self.hide_indexes { 4 } else { max_index_width as u16 + 5 };
                        let desc_x = area.x + pad;
                        let desc_avail = area.width.saturating_sub(pad) as usize;
                        let desc_display: String = desc.chars().take(desc_avail).collect();
                        let desc_color = if is_selected {
                            Color::Green
                        } else if is_focused {
                            self.theme.primary
                        } else {
                            Color::DarkGray
                        };
                        buf.set_string(
                            desc_x,
                            desc_y,
                            &desc_display,
                            Style::default().fg(desc_color),
                        );
                    }
                }
            }
        }
    }
}

// ===================================================================
// SelectMultiWidget — from SelectMulti.tsx
// ===================================================================

/// Multi-select widget with checkboxes and optional submit button.
pub struct SelectMultiWidget<'a, T: Clone + PartialEq + std::fmt::Debug + std::hash::Hash + Eq> {
    pub state: &'a MultiSelectState<T>,
    pub title: Option<&'a str>,
    pub submit_label: &'a str,
    pub hide_indexes: bool,
    pub theme: &'a Theme,
}

impl<'a, T: Clone + PartialEq + std::fmt::Debug + std::hash::Hash + Eq> SelectMultiWidget<'a, T> {
    pub fn new(state: &'a MultiSelectState<T>, theme: &'a Theme) -> Self {
        Self {
            state,
            title: None,
            submit_label: "Submit",
            hide_indexes: false,
            theme,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn submit_label(mut self, label: &'a str) -> Self {
        self.submit_label = label;
        self
    }
}

impl<'a, T: Clone + PartialEq + std::fmt::Debug + std::hash::Hash + Eq> Widget
    for SelectMultiWidget<'a, T>
{
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        // Options
        let visible = self.state.navigation.visible_options();
        let total = self.state.navigation.options.len();
        let max_index_width = total.to_string().len();

        let are_more_above = self.state.navigation.visible_from_index > 0;
        let are_more_below = self.state.navigation.visible_to_index < total;

        for (vi, (orig_idx, opt)) in visible.iter().enumerate() {
            let y = chunks[0].y + vi as u16;
            if y >= chunks[0].y + chunks[0].height {
                break;
            }

            let is_first = vi == 0;
            let is_last = vi == visible.len() - 1;
            let is_focused = !self.state.is_submit_focused
                && self.state.navigation.focused_value.as_ref() == Some(&opt.value);
            let is_checked = self.state.is_selected(&opt.value);

            let mut x = chunks[0].x;
            let max_x = chunks[0].x + chunks[0].width;

            // Arrows
            if are_more_above && is_first {
                buf.set_string(
                    max_x.saturating_sub(2),
                    y,
                    "↑",
                    Style::default().fg(Color::DarkGray),
                );
            }
            if are_more_below && is_last {
                buf.set_string(
                    max_x.saturating_sub(2),
                    y,
                    "↓",
                    Style::default().fg(Color::DarkGray),
                );
            }

            // Focus indicator
            let indicator = if is_focused { "❯ " } else { "  " };
            let ind_style = if is_focused {
                Style::default().fg(self.theme.primary)
            } else {
                Style::default()
            };
            buf.set_string(x, y, indicator, ind_style);
            x += 2;

            // Checkbox
            let checkbox = if is_checked { "[x] " } else { "[ ] " };
            let check_color = if is_checked {
                self.theme.primary
            } else {
                Color::DarkGray
            };
            buf.set_string(x, y, checkbox, Style::default().fg(check_color));
            x += 4;

            // Index
            if !self.hide_indexes {
                let idx_str = format!("{:>width$}.", orig_idx + 1, width = max_index_width);
                buf.set_string(x, y, &idx_str, Style::default().fg(Color::DarkGray));
                x += idx_str.len() as u16 + 1;
            }

            // Label
            let label_color = if opt.disabled {
                Color::DarkGray
            } else if is_focused {
                self.theme.primary
            } else {
                self.theme.text
            };
            let avail = max_x.saturating_sub(x) as usize;
            let label: String = opt.label.chars().take(avail).collect();
            buf.set_string(x, y, &label, Style::default().fg(label_color));
            x += label.len() as u16;

            // Description
            if let Some(ref desc) = opt.description {
                if x + 2 < max_x {
                    x += 1;
                    let desc_avail = max_x.saturating_sub(x) as usize;
                    let desc_text: String = desc.chars().take(desc_avail).collect();
                    buf.set_string(x, y, &desc_text, Style::default().fg(Color::DarkGray));
                }
            }
        }

        // Submit button
        let submit_y = chunks[1].y;
        let submit_style = if self.state.is_submit_focused {
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let submit_prefix = if self.state.is_submit_focused { "❯ " } else { "  " };
        let count_label = format!(
            "{}{} ({})",
            submit_prefix,
            self.submit_label,
            self.state.selected_values.len()
        );
        buf.set_string(chunks[1].x, submit_y, &count_label, submit_style);
    }
}

// ===================================================================
// SelectInputOptionWidget — from select-input-option.tsx
// ===================================================================

/// Widget for an input-type option in a select list.
pub struct SelectInputOptionWidget<'a> {
    pub label: &'a str,
    pub input_value: &'a str,
    pub placeholder: &'a str,
    pub is_focused: bool,
    pub is_selected: bool,
    pub show_label: bool,
    pub index_display: Option<String>,
    pub theme: &'a Theme,
}

impl<'a> SelectInputOptionWidget<'a> {
    pub fn new(label: &'a str, input_value: &'a str, theme: &'a Theme) -> Self {
        Self {
            label,
            input_value,
            placeholder: "",
            is_focused: false,
            is_selected: false,
            show_label: true,
            index_display: None,
            theme,
        }
    }

    pub fn focused(mut self) -> Self {
        self.is_focused = true;
        self
    }

    pub fn placeholder(mut self, p: &'a str) -> Self {
        self.placeholder = p;
        self
    }
}

impl<'a> Widget for SelectInputOptionWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 5 {
            return;
        }

        let mut x = area.x;
        let max_x = area.x + area.width;

        // Focus indicator
        let indicator = if self.is_focused { "❯ " } else { "  " };
        let ind_style = if self.is_focused {
            Style::default().fg(self.theme.primary)
        } else {
            Style::default()
        };
        buf.set_string(x, area.y, indicator, ind_style);
        x += 2;

        // Index
        if let Some(ref idx) = self.index_display {
            buf.set_string(x, area.y, idx, Style::default().fg(Color::DarkGray));
            x += idx.len() as u16 + 1;
        }

        // Label (if showing)
        if self.show_label {
            let label_color = if self.is_focused {
                self.theme.primary
            } else {
                self.theme.text
            };
            buf.set_string(x, area.y, self.label, Style::default().fg(label_color));
            x += self.label.len() as u16;
            if self.is_focused {
                buf.set_string(x, area.y, ", ", Style::default().fg(self.theme.primary));
                x += 2;
            }
        }

        // Input value or placeholder
        let avail = max_x.saturating_sub(x) as usize;
        if self.is_focused {
            if self.input_value.is_empty() {
                let placeholder_display: String = self.placeholder.chars().take(avail).collect();
                buf.set_string(
                    x,
                    area.y,
                    &placeholder_display,
                    Style::default().fg(Color::DarkGray),
                );
            } else {
                let val_display: String = self.input_value.chars().take(avail).collect();
                buf.set_string(x, area.y, &val_display, Style::default().fg(self.theme.text));
            }
        } else {
            let display = if self.input_value.is_empty() {
                self.placeholder
            } else {
                self.input_value
            };
            let color = if self.input_value.is_empty() {
                Color::DarkGray
            } else {
                self.theme.text
            };
            let display_trunc: String = display.chars().take(avail).collect();
            buf.set_string(x, area.y, &display_trunc, Style::default().fg(color));
        }
    }
}

// ===================================================================
// CustomSelect: option, navigation, multi-select, props
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct SelectInputOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UseSelectNavigationProps {
    pub item_count: usize,
    pub initial_index: usize,
    pub looping: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SelectNavigation {
    pub props: UseSelectNavigationProps,
    pub index: usize,
}

impl SelectNavigation {
    pub fn next(&mut self) {
        if self.props.item_count == 0 {
            return;
        }
        if self.index + 1 < self.props.item_count {
            self.index += 1;
        } else if self.props.looping {
            self.index = 0;
        }
    }
    pub fn prev(&mut self) {
        if self.props.item_count == 0 {
            return;
        }
        if self.index > 0 {
            self.index -= 1;
        } else if self.props.looping {
            self.index = self.props.item_count - 1;
        }
    }
}

/// Hook-equivalent useSelectNavigation.
pub fn use_select_navigation(props: UseSelectNavigationProps) -> SelectNavigation {
    SelectNavigation {
        index: props.initial_index.min(props.item_count.saturating_sub(1)),
        props,
    }
}

#[derive(Debug, Clone, Default)]
pub struct UseMultiSelectStateProps {
    pub item_count: usize,
    pub initial_selected: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct MultiSelectIndexState {
    pub props: UseMultiSelectStateProps,
    pub selected: std::collections::HashSet<usize>,
}

/// Hook-equivalent useMultiSelectState.
pub fn use_multi_select_state(props: UseMultiSelectStateProps) -> MultiSelectIndexState {
    let selected: std::collections::HashSet<usize> = props.initial_selected.iter().copied().collect();
    MultiSelectIndexState { props, selected }
}

impl MultiSelectIndexState {
    pub fn toggle(&mut self, idx: usize) {
        if !self.selected.remove(&idx) {
            self.selected.insert(idx);
        }
    }
    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected.contains(&idx)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectMultiProps {
    pub items: Vec<SelectInputOption>,
    pub initial_selected: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectMulti {
    pub props: SelectMultiProps,
    pub state: MultiSelectIndexState,
    pub focus: SelectNavigation,
}

#[derive(Debug, Clone, Default)]
pub struct UseSelectStateProps {
    pub items: Vec<SelectInputOption>,
    pub initial_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SelectIndexState {
    pub props: UseSelectStateProps,
    pub index: usize,
}

/// Hook-equivalent useSelectState.
pub fn use_select_state(props: UseSelectStateProps) -> SelectIndexState {
    let index = props.initial_index.min(props.items.len().saturating_sub(1));
    SelectIndexState { props, index }
}

#[derive(Debug, Clone, Default)]
pub struct OptionWithDescription {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectProps {
    pub items: Vec<OptionWithDescription>,
    pub initial_index: usize,
    pub looping: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Select {
    pub props: SelectProps,
    pub focus: SelectNavigation,
}

impl Select {
    pub fn new(props: SelectProps) -> Self {
        let count = props.items.len();
        let initial = props.initial_index;
        let looping = props.looping;
        Self {
            props,
            focus: use_select_navigation(UseSelectNavigationProps {
                item_count: count,
                initial_index: initial,
                looping,
            }),
        }
    }
    pub fn selected(&self) -> Option<&OptionWithDescription> {
        self.props.items.get(self.focus.index)
    }
}

#[derive(Debug, Clone, Default)]
pub struct UseSelectProps {
    pub items: Vec<OptionWithDescription>,
    pub on_select: Option<String>,
}

/// `useSelectInput` registers keypress handling for a select widget.
pub fn use_select_input<'a>(state: &'a mut SelectIndexState, key: &str) -> Option<&'a SelectInputOption> {
    match key {
        "up" => {
            if state.index > 0 {
                state.index -= 1;
            }
        }
        "down" => {
            if state.index + 1 < state.props.items.len() {
                state.index += 1;
            }
        }
        "return" => return state.props.items.get(state.index),
        _ => {}
    }
    None
}

/// Props for a single CustomSelect option — mirrors TS `SelectOptionProps`.
#[derive(Debug, Clone, Default)]
pub struct SelectOptionProps {
    /// Determines if option is focused.
    pub is_focused: bool,
    /// Determines if option is selected.
    pub is_selected: bool,
    /// Option label.
    pub children: String,
    /// Optional description to display below the label.
    pub description: Option<String>,
    /// Determines if the down arrow should be shown.
    pub should_show_down_arrow: Option<bool>,
    /// Determines if the up arrow should be shown.
    pub should_show_up_arrow: Option<bool>,
    /// Whether ListItem should declare the terminal cursor position.
    /// Set `false` when a child declares its own cursor (e.g. BaseTextInput).
    pub declare_cursor: Option<bool>,
}
