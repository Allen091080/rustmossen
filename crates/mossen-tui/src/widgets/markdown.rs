//! Markdown rendering widget.
//!
//! Renders Markdown content as styled terminal text using pulldown-cmark for
//! parsing and ratatui for drawing.

use std::collections::VecDeque;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::render_glyphs::{RenderGlyphMode, RenderGlyphs};
use crate::theme::Theme;

/// Markdown rendering widget.
pub struct MarkdownWidget<'a> {
    pub content: &'a str,
    pub base_style: Style,
    pub code_style: Style,
    pub heading_style: Style,
    pub link_style: Style,
    pub max_width: Option<u16>,
    pub glyphs: RenderGlyphs,
    pub theme: Theme,
}

impl<'a> MarkdownWidget<'a> {
    pub fn new(content: &'a str) -> Self {
        let theme = Theme::default();
        Self {
            content,
            base_style: Style::default(),
            code_style: Style::default().fg(theme.warning),
            heading_style: Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.primary),
            link_style: Style::default()
                .fg(theme.info)
                .add_modifier(Modifier::UNDERLINED),
            max_width: None,
            glyphs: RenderGlyphs::default(),
            theme,
        }
    }

    pub fn base_style(mut self, style: Style) -> Self {
        self.base_style = style;
        self
    }

    pub fn max_width(mut self, width: u16) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    pub fn theme(mut self, theme: &Theme) -> Self {
        self.theme = theme.clone();
        self.code_style = Style::default().fg(theme.warning);
        self.heading_style = Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme.primary);
        self.link_style = Style::default()
            .fg(theme.info)
            .add_modifier(Modifier::UNDERLINED);
        self
    }

    /// Parse markdown content into styled terminal lines.
    pub fn parse_to_lines(&self) -> Vec<Line<'static>> {
        use pulldown_cmark::CodeBlockKind;
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut style_stack: Vec<Style> = vec![self.base_style];
        let mut link_stack: Vec<String> = Vec::new();
        let mut list_stack: Vec<ListState> = Vec::new();
        let mut pending_item_prefix: Option<String> = None;
        let mut quote_depth = 0usize;
        let mut in_code_block = false;
        // Buffer text inside a fenced code block so syntect can highlight
        // the whole thing at once. `code_lang` carries the language
        // hint from the fence (e.g. ```rust → "rust"); empty string
        // = no hint, syntect falls back to plain text.
        let mut code_buf = String::new();
        let mut code_lang = String::new();
        let mut table: Option<TableState> = None;

        let options =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext(self.content, options);

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        flush_line(&mut lines, &mut current_spans);
                        ensure_blank_line(&mut lines);
                        let prefix = "#".repeat(level as usize);
                        current_spans
                            .push(Span::styled(format!("{} ", prefix), self.heading_style));
                        style_stack.push(self.heading_style);
                    }
                    Tag::Strong => {
                        let s = current_style(&style_stack).add_modifier(Modifier::BOLD);
                        style_stack.push(s);
                    }
                    Tag::Emphasis => {
                        let s = current_style(&style_stack).add_modifier(Modifier::ITALIC);
                        style_stack.push(s);
                    }
                    Tag::Strikethrough => {
                        let s = current_style(&style_stack).add_modifier(Modifier::CROSSED_OUT);
                        style_stack.push(s);
                    }
                    Tag::BlockQuote(_) => {
                        flush_line(&mut lines, &mut current_spans);
                        ensure_blank_line(&mut lines);
                        quote_depth += 1;
                        let s = current_style(&style_stack)
                            .fg(self.theme.text_dim)
                            .add_modifier(Modifier::ITALIC);
                        style_stack.push(s);
                    }
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_buf.clear();
                        code_lang = match kind {
                            CodeBlockKind::Fenced(s) => s.to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        flush_line(&mut lines, &mut current_spans);
                        ensure_blank_line(&mut lines);
                    }
                    Tag::List(first) => {
                        flush_line(&mut lines, &mut current_spans);
                        ensure_blank_line(&mut lines);
                        list_stack.push(ListState {
                            ordered: first.is_some(),
                            next: first.unwrap_or(1),
                        });
                    }
                    Tag::Item => {
                        pending_item_prefix = Some(next_list_prefix(&mut list_stack, self.glyphs));
                    }
                    Tag::Link { dest_url, .. } => {
                        style_stack.push(self.link_style);
                        link_stack.push(dest_url.to_string());
                    }
                    Tag::Image { dest_url, .. } => {
                        style_stack.push(self.link_style);
                        link_stack.push(dest_url.to_string());
                    }
                    Tag::Table(_) => {
                        flush_line(&mut lines, &mut current_spans);
                        ensure_blank_line(&mut lines);
                        table = Some(TableState::default());
                    }
                    Tag::TableRow => {
                        if let Some(table) = table.as_mut() {
                            table.current_row.clear();
                        }
                    }
                    Tag::TableCell => {
                        if let Some(table) = table.as_mut() {
                            table.current_cell.clear();
                        }
                    }
                    Tag::Paragraph => {}
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Heading(_) | TagEnd::Paragraph => {
                        flush_line(&mut lines, &mut current_spans);
                        if matches!(tag_end, TagEnd::Heading(_)) {
                            style_stack.pop();
                        }
                        if matches!(tag_end, TagEnd::Heading(_))
                            || (matches!(tag_end, TagEnd::Paragraph)
                                && list_stack.is_empty()
                                && quote_depth == 0)
                        {
                            ensure_blank_line(&mut lines);
                        }
                    }
                    TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                        style_stack.pop();
                    }
                    TagEnd::Link | TagEnd::Image => {
                        style_stack.pop();
                        if let Some(dest) = link_stack.pop() {
                            append_link_target(&mut current_spans, &dest, &self.theme);
                        }
                    }
                    TagEnd::BlockQuote(_) => {
                        flush_line(&mut lines, &mut current_spans);
                        quote_depth = quote_depth.saturating_sub(1);
                        style_stack.pop();
                        ensure_blank_line(&mut lines);
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        push_code_block(
                            &mut lines,
                            &code_buf,
                            &code_lang,
                            self.glyphs,
                            &self.theme,
                        );
                        code_buf.clear();
                        code_lang.clear();
                        ensure_blank_line(&mut lines);
                    }
                    TagEnd::Item => {
                        flush_line(&mut lines, &mut current_spans);
                        pending_item_prefix = None;
                    }
                    TagEnd::List(_) => {
                        list_stack.pop();
                        if list_stack.is_empty() {
                            ensure_blank_line(&mut lines);
                        }
                    }
                    TagEnd::TableCell => {
                        if let Some(table) = table.as_mut() {
                            table
                                .current_row
                                .push(table.current_cell.trim().to_string());
                            table.current_cell.clear();
                        }
                    }
                    TagEnd::TableRow | TagEnd::TableHead => {
                        if let Some(table) = table.as_mut() {
                            if !table.current_row.is_empty() {
                                table.rows.push(std::mem::take(&mut table.current_row));
                            }
                        }
                    }
                    TagEnd::Table => {
                        if let Some(table) = table.take() {
                            lines.extend(render_table(
                                table,
                                self.max_width,
                                self.glyphs,
                                &self.theme,
                            ));
                            ensure_blank_line(&mut lines);
                        }
                    }
                    _ => {}
                },
                Event::Text(text) => {
                    if let Some(table) = table.as_mut() {
                        table.current_cell.push_str(&text);
                        continue;
                    }
                    if in_code_block {
                        // Buffer raw text so syntect can re-parse the
                        // whole block at once when the fence closes.
                        code_buf.push_str(&text);
                        continue;
                    }
                    ensure_line_prefix(
                        &mut current_spans,
                        &mut pending_item_prefix,
                        quote_depth,
                        self.glyphs,
                        &self.theme,
                    );
                    let style = current_style(&style_stack);
                    current_spans.push(Span::styled(text.to_string(), style));
                }
                Event::Code(code) => {
                    if let Some(table) = table.as_mut() {
                        table.current_cell.push('`');
                        table.current_cell.push_str(&code);
                        table.current_cell.push('`');
                        continue;
                    }
                    ensure_line_prefix(
                        &mut current_spans,
                        &mut pending_item_prefix,
                        quote_depth,
                        self.glyphs,
                        &self.theme,
                    );
                    current_spans.push(Span::styled(format!("`{}`", code), self.code_style));
                }
                Event::TaskListMarker(checked) => {
                    ensure_line_prefix(
                        &mut current_spans,
                        &mut pending_item_prefix,
                        quote_depth,
                        self.glyphs,
                        &self.theme,
                    );
                    let marker = if checked { "[x] " } else { "[ ] " };
                    current_spans.push(Span::styled(
                        marker.to_string(),
                        Style::default().fg(self.theme.text_dim),
                    ));
                }
                Event::SoftBreak => {
                    if let Some(table) = table.as_mut() {
                        table.current_cell.push(' ');
                        continue;
                    }
                    current_spans.push(Span::raw(" "));
                }
                Event::HardBreak => {
                    if let Some(table) = table.as_mut() {
                        table.current_cell.push(' ');
                        continue;
                    }
                    flush_line(&mut lines, &mut current_spans);
                }
                Event::Rule => {
                    flush_line(&mut lines, &mut current_spans);
                    ensure_blank_line(&mut lines);
                    lines.push(Line::from(Span::styled(
                        markdown_rule(self.glyphs),
                        Style::default().fg(self.theme.text_subtle),
                    )));
                    ensure_blank_line(&mut lines);
                }
                Event::Html(html) => {
                    ensure_line_prefix(
                        &mut current_spans,
                        &mut pending_item_prefix,
                        quote_depth,
                        self.glyphs,
                        &self.theme,
                    );
                    current_spans.push(Span::styled(
                        html.to_string(),
                        Style::default().fg(self.theme.text_subtle),
                    ));
                }
                _ => {}
            }
        }

        if in_code_block {
            push_code_block(&mut lines, &code_buf, &code_lang, self.glyphs, &self.theme);
        }

        // Flush remaining spans
        flush_line(&mut lines, &mut current_spans);
        trim_trailing_blank_lines(&mut lines);

        lines
    }

    /// Count the visual lines after Markdown has been parsed and styled.
    /// This lets the scrollback reserve space for the rendered form rather
    /// than the raw markdown source.
    pub fn rendered_height(&self, width: u16) -> usize {
        wrapped_line_count_for_lines(&self.parse_to_lines(), width)
    }
}

fn current_style(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct ListState {
    ordered: bool,
    next: u64,
}

#[derive(Debug, Default)]
struct TableState {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
}

fn flush_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(std::mem::take(spans)));
    }
}

fn ensure_blank_line(lines: &mut Vec<Line<'static>>) {
    if lines.is_empty() || is_blank_line(lines.last().expect("checked non-empty")) {
        return;
    }
    lines.push(Line::from(""));
}

fn trim_trailing_blank_lines(lines: &mut Vec<Line<'static>>) {
    while lines.last().is_some_and(is_blank_line) {
        lines.pop();
    }
}

fn is_blank_line(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|span| span.content.as_ref().trim().is_empty())
}

fn ensure_line_prefix(
    spans: &mut Vec<Span<'static>>,
    pending_item_prefix: &mut Option<String>,
    quote_depth: usize,
    glyphs: RenderGlyphs,
    theme: &Theme,
) {
    if !spans.is_empty() {
        return;
    }

    if quote_depth > 0 {
        spans.push(Span::styled(
            markdown_quote_prefix(glyphs).repeat(quote_depth),
            Style::default()
                .fg(theme.text_subtle)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(prefix) = pending_item_prefix.take() {
        spans.push(Span::styled(prefix, Style::default().fg(theme.text_dim)));
    }
}

fn next_list_prefix(stack: &mut [ListState], glyphs: RenderGlyphs) -> String {
    let depth = stack.len().saturating_sub(1);
    let indent = "  ".repeat(depth);
    if let Some(state) = stack.last_mut() {
        if state.ordered {
            let n = state.next;
            state.next = state.next.saturating_add(1);
            return format!("{indent}{n}. ");
        }
    }
    format!("{indent}{} ", markdown_bullet(glyphs))
}

fn append_link_target(spans: &mut Vec<Span<'static>>, dest: &str, theme: &Theme) {
    let dest = dest.trim();
    if dest.is_empty() || dest.starts_with('#') {
        return;
    }
    let visible = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    if visible.ends_with(dest) {
        return;
    }
    spans.push(Span::styled(
        format!(" ({dest})"),
        Style::default()
            .fg(theme.text_subtle)
            .add_modifier(Modifier::ITALIC),
    ));
}

fn push_code_block(
    lines: &mut Vec<Line<'static>>,
    code: &str,
    lang: &str,
    glyphs: RenderGlyphs,
    theme: &Theme,
) {
    let language = lang.split_whitespace().next().unwrap_or("").trim();
    let label = if language.is_empty() {
        "code"
    } else {
        language
    };
    let chrome_style = Style::default().fg(theme.text_subtle);
    lines.push(Line::from(Span::styled(
        format!(
            "{}{} {label}",
            glyphs.border.top_left, glyphs.border.horizontal_top
        ),
        chrome_style,
    )));

    let mut hl = crate::widgets::highlighted_code::HighlightedCodeWidget::new(code, theme);
    if !language.is_empty() {
        hl = hl.language(language);
    }
    let highlighted = hl.build_lines();
    if highlighted.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", glyphs.border.vertical_left), chrome_style),
            Span::raw(""),
        ]));
    } else {
        for line in highlighted {
            let mut spans = vec![Span::styled(
                format!("{} ", glyphs.border.vertical_left),
                chrome_style,
            )];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}",
            glyphs.border.bottom_left, glyphs.border.horizontal_bottom
        ),
        chrome_style,
    )));
}

fn render_table(
    table: TableState,
    max_width: Option<u16>,
    glyphs: RenderGlyphs,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if table.rows.is_empty() {
        return Vec::new();
    }

    let column_count = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }

    let mut widths = vec![1usize; column_count];
    for row in &table.rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }
    if let Some(max_width) = max_width {
        let max_width = max_width as usize;
        let chrome_width = 4 + column_count.saturating_sub(1) * 3; // "│ " + " │" + separators
        let available = max_width.saturating_sub(chrome_width).max(column_count);
        let max_col_width = (available / column_count).max(3);
        for width in &mut widths {
            *width = (*width).min(max_col_width);
        }
    }

    let mut out = Vec::new();
    let border_style = Style::default().fg(theme.text_subtle);
    for (row_idx, row) in table.rows.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("{} ", glyphs.border.vertical_left),
            border_style,
        )];
        for idx in 0..column_count {
            let cell = row.get(idx).map(String::as_str).unwrap_or("");
            let cell = fit_table_cell(cell, widths[idx], glyphs);
            let style = if row_idx == 0 {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(cell.clone(), style));
            let pad = widths[idx].saturating_sub(UnicodeWidthStr::width(cell.as_str()));
            spans.push(Span::raw(" ".repeat(pad)));
            if idx + 1 < column_count {
                spans.push(Span::styled(
                    format!(" {} ", glyphs.border.vertical_left),
                    border_style,
                ));
            }
        }
        spans.push(Span::styled(
            format!(" {}", glyphs.border.vertical_right),
            border_style,
        ));
        out.push(Line::from(spans));

        if row_idx == 0 && table.rows.len() > 1 {
            let sep = table_separator(&widths, glyphs);
            out.push(Line::from(Span::styled(sep, border_style)));
        }
    }
    out
}

fn markdown_bullet(glyphs: RenderGlyphs) -> &'static str {
    match glyphs.mode {
        RenderGlyphMode::Unicode => "•",
        RenderGlyphMode::Ascii => "-",
    }
}

fn markdown_quote_prefix(glyphs: RenderGlyphs) -> &'static str {
    match glyphs.mode {
        RenderGlyphMode::Unicode => "▌ ",
        RenderGlyphMode::Ascii => "| ",
    }
}

fn markdown_rule(glyphs: RenderGlyphs) -> String {
    glyphs.border.horizontal_top.repeat(40)
}

fn table_separator(widths: &[usize], glyphs: RenderGlyphs) -> String {
    match glyphs.mode {
        RenderGlyphMode::Unicode => {
            let mut sep = String::from("├─");
            for (idx, width) in widths.iter().enumerate() {
                sep.push_str(&glyphs.border.horizontal_top.repeat(*width));
                if idx + 1 < widths.len() {
                    sep.push_str("─┼─");
                }
            }
            sep.push_str("─┤");
            sep
        }
        RenderGlyphMode::Ascii => {
            let mut sep = String::from("|-");
            for (idx, width) in widths.iter().enumerate() {
                sep.push_str(&glyphs.border.horizontal_top.repeat(*width));
                if idx + 1 < widths.len() {
                    sep.push_str("-+-");
                }
            }
            sep.push_str("-|");
            sep
        }
    }
}

fn fit_table_cell(cell: &str, max_width: usize, glyphs: RenderGlyphs) -> String {
    if UnicodeWidthStr::width(cell) <= max_width {
        return cell.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let suffix = glyphs.ellipsis();
    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_width {
        let mut out = String::new();
        let mut used = 0usize;
        for ch in suffix.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + w > max_width {
                break;
            }
            out.push(ch);
            used += w;
        }
        return out;
    }

    let limit = max_width.saturating_sub(suffix_width);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in cell.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str(suffix);
    out
}

pub fn wrapped_line_count_for_lines(lines: &[Line<'_>], width: u16) -> usize {
    if lines.is_empty() {
        return 1;
    }

    lines
        .iter()
        .map(|line| {
            let text = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            wrapped_line_count_for_text(&text, width)
        })
        .sum::<usize>()
        .max(1)
}

pub fn wrapped_line_count_for_text(text: &str, width: u16) -> usize {
    let width = width.max(1);
    if text.is_empty() {
        return 1;
    }

    text.split('\n')
        .map(|line| wrapped_line_count_for_segment(line, width))
        .sum()
}

const NBSP: &str = "\u{00a0}";
const ZWSP: &str = "\u{200b}";

fn wrapped_line_count_for_segment(text: &str, max_line_width: u16) -> usize {
    let mut count = 0usize;
    let mut line_width = 0u16;
    let mut line_has_content = false;
    let mut word_width = 0u16;
    let mut whitespace_width = 0u16;
    let mut pending_whitespace: VecDeque<u16> = VecDeque::new();
    let mut non_whitespace_previous = false;

    for grapheme in text.graphemes(true) {
        let is_whitespace =
            grapheme == ZWSP || (grapheme.chars().all(char::is_whitespace) && grapheme != NBSP);
        let symbol_width = UnicodeWidthStr::width(grapheme) as u16;

        if symbol_width > max_line_width {
            continue;
        }

        let word_found = non_whitespace_previous && is_whitespace;
        let untrimmed_overflow =
            !line_has_content && word_width + whitespace_width + symbol_width > max_line_width;

        if word_found || untrimmed_overflow {
            line_width = line_width.saturating_add(whitespace_width);
            line_has_content |= whitespace_width > 0;
            line_width = line_width.saturating_add(word_width);
            line_has_content |= word_width > 0;

            pending_whitespace.clear();
            whitespace_width = 0;
            word_width = 0;
        }

        let line_full = line_width >= max_line_width;
        let pending_word_overflow =
            symbol_width > 0 && line_width + whitespace_width + word_width >= max_line_width;
        let breaking_unspaced_word =
            pending_word_overflow && line_width == 0 && whitespace_width == 0;

        if line_full || pending_word_overflow {
            let mut remaining_width = max_line_width.saturating_sub(line_width);
            count += 1;
            line_width = 0;
            line_has_content = false;

            while let Some(width) = pending_whitespace.front().copied() {
                if width > remaining_width {
                    break;
                }

                whitespace_width = whitespace_width.saturating_sub(width);
                remaining_width = remaining_width.saturating_sub(width);
                pending_whitespace.pop_front();
            }

            if breaking_unspaced_word {
                word_width = 0;
            }

            if is_whitespace && pending_whitespace.is_empty() {
                continue;
            }
        }

        if is_whitespace {
            whitespace_width = whitespace_width.saturating_add(symbol_width);
            pending_whitespace.push_back(symbol_width);
        } else {
            word_width = word_width.saturating_add(symbol_width);
        }

        non_whitespace_previous = !is_whitespace;
    }

    if !line_has_content && word_width == 0 && whitespace_width > 0 {
        count += 1;
    }

    line_has_content |= whitespace_width > 0 || word_width > 0;
    if line_has_content {
        count += 1;
    }

    count.max(1)
}

impl<'a> Widget for MarkdownWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let lines = self.parse_to_lines();
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_profile::RenderColorMode;
    use crate::theme::ThemeName;

    fn plain_text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_plain_style(style: Style) {
        assert!(
            matches!(style.fg, None | Some(Color::Reset)),
            "plain markdown style leaked foreground color: {:?}",
            style.fg
        );
        assert!(
            matches!(style.bg, None | Some(Color::Reset)),
            "plain markdown style leaked background color: {:?}",
            style.bg
        );
    }

    fn assert_plain_lines(lines: &[Line<'_>]) {
        for line in lines {
            assert_plain_style(line.style);
            for span in &line.spans {
                assert_plain_style(span.style);
            }
        }
    }

    #[test]
    fn renders_common_markdown_blocks_as_terminal_shapes() {
        let md = concat!(
            "# Result\n\n",
            "- **bold** item with `code`\n",
            "- [x] checked\n\n",
            "> quoted text\n\n",
            "```rust\n",
            "fn main() {}\n",
            "```\n"
        );

        let text = plain_text(&MarkdownWidget::new(md).parse_to_lines());

        assert!(text.contains("# Result"));
        assert!(text.contains("• bold item with `code`"));
        assert!(text.contains("• [x] checked"));
        assert!(text.contains("▌ quoted text"));
        assert!(text.contains("╭─ rust"));
        assert!(text.contains("│  1 fn main() {}"));
        assert!(text.contains("╰─"));
        assert!(!text.contains("```"));
    }

    #[test]
    fn ascii_glyph_profile_uses_plain_markdown_chrome() {
        let md = concat!(
            "- item\n\n",
            "> quote\n\n",
            "---\n\n",
            "```rust\n",
            "fn main() {}\n",
            "```\n\n",
            "| A | B |\n",
            "| --- | --- |\n",
            "| one | two |\n",
        );

        let text = plain_text(
            &MarkdownWidget::new(md)
                .glyphs(RenderGlyphs::ascii())
                .parse_to_lines(),
        );

        assert!(text.contains("- item"), "{text}");
        assert!(text.contains("| quote"), "{text}");
        assert!(text.contains("+- rust"), "{text}");
        assert!(text.contains("|  1 fn main() {}"), "{text}");
        assert!(text.contains("| A   | B   |"), "{text}");
        assert!(text.contains("|-----+-----|"), "{text}");
        for forbidden in ["╭", "╰", "│", "─", "├", "┼", "┤", "•", "▌"] {
            assert!(
                !text.contains(forbidden),
                "ASCII markdown leaked unicode glyph {forbidden:?}\n{text}"
            );
        }
    }

    #[test]
    fn ascii_glyph_profile_uses_plain_table_truncation() {
        let md = "| Name | Status |\n| --- | --- |\n| very-long-name | ready |\n";
        let text = plain_text(
            &MarkdownWidget::new(md)
                .glyphs(RenderGlyphs::ascii())
                .max_width(18)
                .parse_to_lines(),
        );

        assert!(text.contains("..."), "{text}");
        assert!(!text.contains('…'), "{text}");
    }

    #[test]
    fn plain_color_mode_suppresses_markdown_terminal_colors() {
        let md = concat!(
            "# Result\n\n",
            "- **bold** item with `code`\n",
            "- [x] checked\n\n",
            "> quoted [link](https://example.com)\n\n",
            "```rust\n",
            "fn main() {}\n",
            "```\n\n",
            "| Name | Status |\n",
            "| --- | --- |\n",
            "| mossen | ok |\n"
        );
        let theme = Theme::for_name_with_color_mode(ThemeName::Dark, RenderColorMode::Plain);
        let lines = MarkdownWidget::new(md).theme(&theme).parse_to_lines();

        assert_plain_lines(&lines);
        assert!(plain_text(&lines).contains("fn main() {}"));
    }

    #[test]
    fn renders_tables_with_header_separator() {
        let md = "| Name | Status |\n| --- | --- |\n| mossen | ok |\n";
        let text = plain_text(&MarkdownWidget::new(md).parse_to_lines());

        assert!(text.contains("│ Name   │ Status │"));
        assert!(text.contains("├─"));
        assert!(text.contains("│ mossen │ ok     │"));
    }

    #[test]
    fn tables_respect_max_width() {
        let md = concat!(
            "| Name | Description |\n",
            "| --- | --- |\n",
            "| mossen | a very long description that should not break table chrome |\n"
        );
        let lines = MarkdownWidget::new(md).max_width(36).parse_to_lines();
        let text = plain_text(&lines);

        assert!(text.contains('…'));
        for line in text.lines() {
            assert!(
                UnicodeWidthStr::width(line) <= 36,
                "line exceeded width: {line}"
            );
        }
    }

    #[test]
    fn rendered_height_uses_markdown_output_width() {
        let md = "```text\n12345678901234567890\n```";
        let widget = MarkdownWidget::new(md);

        assert!(widget.rendered_height(12) > 3);
    }

    #[test]
    fn wrapped_height_counts_long_words_by_terminal_width() {
        let text = "abcdefg";

        assert_eq!(wrapped_line_count_for_text(text, 3), 3);
    }

    #[test]
    fn wrapped_height_combines_styled_spans_before_counting() {
        let lines = vec![Line::from(vec![
            Span::raw("错误错误"),
            Span::styled("abcdefg", Style::default().fg(Color::Red)),
        ])];
        let expected = wrapped_line_count_for_text("错误错误abcdefg", 6);

        assert_eq!(wrapped_line_count_for_lines(&lines, 6), expected);
    }

    #[test]
    fn keeps_readable_spacing_between_blocks_without_trailing_blank() {
        let md = concat!(
            "# Title\n\n",
            "First paragraph.\n\n",
            "Second paragraph.\n\n",
            "```text\n",
            "hello\n",
            "```\n\n",
            "| A | B |\n",
            "| - | - |\n",
            "| 1 | 2 |\n"
        );
        let text = plain_text(&MarkdownWidget::new(md).parse_to_lines());

        assert!(text.contains("# Title\n\nFirst paragraph."));
        assert!(text.contains("First paragraph.\n\nSecond paragraph."));
        assert!(text.contains("Second paragraph.\n\n╭─ text"));
        assert!(text.contains("╰─\n\n│ A │ B │"));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn clips_partial_area_to_buffer_before_rendering() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 3));

        MarkdownWidget::new("## 标题\n\n```rust\nfn main() {}\n```")
            .render(Rect::new(4, 2, 40, 8), &mut buf);
        MarkdownWidget::new("outside").render(Rect::new(50, 50, 10, 4), &mut buf);

        let rendered = buffer_text(&buf);
        assert!(
            rendered.contains('标') || rendered.contains("fn main"),
            "partial markdown render should leave visible content\n{rendered}"
        );
    }

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
}
