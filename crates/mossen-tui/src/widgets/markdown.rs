//! Markdown rendering widget.
//!
//! Translates Markdown.tsx — renders Markdown content as styled terminal text
//! using pulldown-cmark for parsing and ratatui for rendering.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

/// Markdown rendering widget.
pub struct MarkdownWidget<'a> {
    pub content: &'a str,
    pub base_style: Style,
    pub code_style: Style,
    pub heading_style: Style,
    pub link_style: Style,
    pub max_width: Option<u16>,
}

impl<'a> MarkdownWidget<'a> {
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            base_style: Style::default(),
            code_style: Style::default().fg(Color::Rgb(200, 180, 130)),
            heading_style: Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Rgb(130, 170, 255)),
            link_style: Style::default()
                .fg(Color::Rgb(100, 180, 255))
                .add_modifier(Modifier::UNDERLINED),
            max_width: None,
        }
    }

    pub fn base_style(mut self, style: Style) -> Self {
        self.base_style = style;
        self
    }

    /// Parse markdown content into styled Lines.
    pub fn parse_to_lines(&self) -> Vec<Line<'a>> {
        use pulldown_cmark::CodeBlockKind;
        let mut lines: Vec<Line> = Vec::new();
        let mut current_spans: Vec<Span> = Vec::new();
        let mut style_stack: Vec<Style> = vec![self.base_style];
        let mut in_code_block = false;
        // Buffer text inside a fenced code block so syntect can highlight
        // the whole thing at once. `code_lang` carries the language
        // hint from the fence (e.g. ```rust → "rust"); empty string
        // = no hint, syntect falls back to plain text.
        let mut code_buf = String::new();
        let mut code_lang = String::new();

        let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(self.content, options);

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
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
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_buf.clear();
                        code_lang = match kind {
                            CodeBlockKind::Fenced(s) => s.to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        // Flush current line
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    Tag::Link { dest_url, .. } => {
                        style_stack.push(self.link_style);
                        let _ = dest_url; // URL tracked for hyperlink
                    }
                    Tag::List(_) | Tag::Item => {}
                    Tag::Paragraph => {}
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Heading(_) | TagEnd::Paragraph => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                        style_stack.pop();
                    }
                    TagEnd::Strong | TagEnd::Emphasis | TagEnd::Link => {
                        style_stack.pop();
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        // Run the accumulated code through syntect for
                        // proper per-language highlighting. Theme is
                        // pulled from the parent so the colours match
                        // the surrounding chrome.
                        let theme = crate::theme::Theme::for_name(crate::theme::ThemeName::Dark);
                        let mut hl = crate::widgets::highlighted_code::HighlightedCodeWidget::new(
                            &code_buf, &theme,
                        );
                        if !code_lang.is_empty() {
                            hl = hl.language(&code_lang);
                        }
                        let highlighted: Vec<Line<'static>> = hl.build_lines();
                        for l in highlighted {
                            lines.push(l);
                        }
                        code_buf.clear();
                        code_lang.clear();
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    TagEnd::Item => {
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    _ => {
                        style_stack.pop();
                    }
                },
                Event::Text(text) => {
                    if in_code_block {
                        // Buffer raw text so syntect can re-parse the
                        // whole block at once when the fence closes.
                        code_buf.push_str(&text);
                        continue;
                    }
                    let style = current_style(&style_stack);
                    current_spans.push(Span::styled(text.to_string(), style));
                }
                Event::Code(code) => {
                    current_spans.push(Span::styled(format!("`{}`", code), self.code_style));
                }
                Event::SoftBreak => {
                    current_spans.push(Span::raw(" "));
                }
                Event::HardBreak => {
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                }
                Event::Rule => {
                    if !current_spans.is_empty() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                    }
                    lines.push(Line::from(Span::styled(
                        "─".repeat(40),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                _ => {}
            }
        }

        // Flush remaining spans
        if !current_spans.is_empty() {
            lines.push(Line::from(current_spans));
        }

        lines
    }
}

fn current_style(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or_default()
}

impl<'a> Widget for MarkdownWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let lines = self.parse_to_lines();
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }
}
