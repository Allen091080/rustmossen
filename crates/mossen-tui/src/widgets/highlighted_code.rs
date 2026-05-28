//! Syntax-highlighted code display widget.
//!
//! Syntax highlighting rendered into ratatui styled spans with `syntect`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use std::sync::LazyLock;
use syntect::{
    highlighting::{self, ThemeSet},
    parsing::SyntaxSet,
};

use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Global syntax set (loaded once)
// ---------------------------------------------------------------------------

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

// ---------------------------------------------------------------------------
// HighlightedCodeWidget
// ---------------------------------------------------------------------------

/// Renders code with syntax highlighting.
pub struct HighlightedCodeWidget<'a> {
    pub code: &'a str,
    pub language: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub line_numbers: bool,
    pub line_number_separator: &'a str,
    pub start_line: usize,
    pub highlight_lines: Vec<usize>,
    pub theme: &'a Theme,
    pub max_lines: Option<usize>,
}

impl<'a> HighlightedCodeWidget<'a> {
    pub fn new(code: &'a str, theme: &'a Theme) -> Self {
        Self {
            code,
            language: None,
            file_path: None,
            line_numbers: true,
            line_number_separator: " ",
            start_line: 1,
            highlight_lines: Vec::new(),
            theme,
            max_lines: None,
        }
    }

    pub fn language(mut self, lang: &'a str) -> Self {
        self.language = Some(lang);
        self
    }

    pub fn file_path(mut self, path: &'a str) -> Self {
        self.file_path = Some(path);
        self
    }

    pub fn line_numbers(mut self, show: bool) -> Self {
        self.line_numbers = show;
        self
    }

    pub fn line_number_separator(mut self, separator: &'a str) -> Self {
        self.line_number_separator = separator;
        self
    }

    pub fn start_line(mut self, n: usize) -> Self {
        self.start_line = n;
        self
    }

    pub fn highlight_lines(mut self, lines: Vec<usize>) -> Self {
        self.highlight_lines = lines;
        self
    }

    pub fn max_lines(mut self, n: usize) -> Self {
        self.max_lines = Some(n);
        self
    }

    /// Determine the syntect syntax to use.
    fn resolve_syntax(&self) -> &syntect::parsing::SyntaxReference {
        let ss = &*SYNTAX_SET;
        if let Some(lang) = self.language {
            if let Some(syn) = ss.find_syntax_by_token(lang) {
                return syn;
            }
        }
        if let Some(path) = self.file_path {
            if let Some(ext) = path.rsplit('.').next() {
                if let Some(syn) = ss.find_syntax_by_extension(ext) {
                    return syn;
                }
            }
        }
        ss.find_syntax_plain_text()
    }

    /// Convert syntect color to ratatui Color.
    fn syntect_to_ratatui(&self, c: highlighting::Color) -> Color {
        self.theme.terminal_color(Color::Rgb(c.r, c.g, c.b))
    }

    /// Build styled lines from code using syntect. Public so callers
    /// (e.g. `MarkdownWidget`) can fold the same syntect output into
    /// their own `Vec<Line>` pipeline.
    pub fn build_lines(&self) -> Vec<Line<'static>> {
        let ss = &*SYNTAX_SET;
        let ts = &*THEME_SET;
        let syntax = self.resolve_syntax();

        // Use a dark theme from syntect as base
        let highlight_theme = &ts.themes["base16-ocean.dark"];
        let highlighter = highlighting::Highlighter::new(highlight_theme);
        let mut highlight_state =
            highlighting::HighlightState::new(&highlighter, syntect::parsing::ScopeStack::new());

        let code_lines: Vec<&str> = self.code.lines().collect();
        let max = self.max_lines.unwrap_or(code_lines.len());
        let gutter_width = if self.line_numbers {
            let last = self
                .start_line
                .saturating_add(max.min(code_lines.len()).saturating_sub(1));
            format!("{}", last).len() + usize::from(self.line_number_separator == " ")
        } else {
            0
        };

        let mut result = Vec::new();
        let mut parse_state = syntect::parsing::ParseState::new(syntax);

        for (i, code_line) in code_lines.iter().take(max).enumerate() {
            let line_no = self.start_line + i;
            let is_highlighted = self.highlight_lines.contains(&line_no);

            let ops = parse_state.parse_line(code_line, ss).unwrap_or_default();
            let regions: Vec<(highlighting::Style, &str)> = highlighting::HighlightIterator::new(
                &mut highlight_state,
                &ops,
                code_line,
                &highlighter,
            )
            .collect();

            let mut spans: Vec<Span<'static>> = Vec::new();

            // Line number gutter
            if self.line_numbers {
                let gutter = format!(
                    "{:>width$}{}",
                    line_no,
                    self.line_number_separator,
                    width = gutter_width
                );
                spans.push(Span::styled(
                    gutter,
                    Style::default().fg(self.theme.text_subtle),
                ));
            }

            // Highlighted spans
            for (style, text) in regions {
                let fg = self.syntect_to_ratatui(style.foreground);
                let mut ratatui_style = Style::default().fg(fg);
                if style.font_style.contains(highlighting::FontStyle::BOLD) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(highlighting::FontStyle::ITALIC) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                if is_highlighted && self.theme.uses_color() {
                    ratatui_style = ratatui_style.bg(Color::Rgb(60, 60, 30));
                }
                spans.push(Span::styled(text.to_string(), ratatui_style));
            }

            result.push(Line::from(spans));
        }

        // Truncation indicator
        if code_lines.len() > max {
            result.push(Line::from(Span::styled(
                format!("... ({} more lines)", code_lines.len() - max),
                Style::default().fg(self.theme.text_dim),
            )));
        }

        result
    }
}

impl<'a> Widget for HighlightedCodeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let lines = self.build_lines();
        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }
}

// ---------------------------------------------------------------------------
// FilePathLinkWidget — clickable file path
// ---------------------------------------------------------------------------

/// Renders a file path with optional line number, styled as a link.
pub struct FilePathLinkWidget<'a> {
    pub path: &'a str,
    pub line: Option<usize>,
    pub theme: &'a Theme,
}

impl<'a> FilePathLinkWidget<'a> {
    pub fn new(path: &'a str, theme: &'a Theme) -> Self {
        Self {
            path,
            line: None,
            theme,
        }
    }

    pub fn line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl<'a> Widget for FilePathLinkWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let text = if let Some(line) = self.line {
            format!("{}:{}", self.path, line)
        } else {
            self.path.to_string()
        };

        let style = Style::default()
            .fg(self.theme.primary)
            .add_modifier(Modifier::UNDERLINED);

        let avail = area.width as usize;
        let truncated: String = text.chars().take(avail).collect();
        buf.set_string(area.x, area.y, &truncated, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_profile::RenderColorMode;
    use crate::theme::ThemeName;

    fn assert_plain_style(style: Style) {
        assert!(
            matches!(style.fg, None | Some(Color::Reset)),
            "plain code style leaked foreground color: {:?}",
            style.fg
        );
        assert!(
            matches!(style.bg, None | Some(Color::Reset)),
            "plain code style leaked background color: {:?}",
            style.bg
        );
    }

    #[test]
    fn plain_color_mode_suppresses_syntax_colors() {
        let theme = Theme::for_name_with_color_mode(ThemeName::Dark, RenderColorMode::Plain);
        let lines = HighlightedCodeWidget::new("fn main() {\n    println!(\"hi\");\n}", &theme)
            .language("rust")
            .highlight_lines(vec![1])
            .build_lines();

        assert!(!lines.is_empty());
        for line in lines {
            assert_plain_style(line.style);
            for span in line.spans {
                assert_plain_style(span.style);
            }
        }
    }

    #[test]
    fn line_number_gutter_uses_separator() {
        let theme = Theme::for_name_with_color_mode(ThemeName::Dark, RenderColorMode::Plain);
        let lines = HighlightedCodeWidget::new("fn main() {}", &theme)
            .language("rust")
            .start_line(10)
            .line_number_separator("│")
            .build_lines();
        let first = lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(first.starts_with("10│"), "{first:?}");
    }
}
