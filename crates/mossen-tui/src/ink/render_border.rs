//! Border rendering (render-border.ts).

/// Border characters for different styles.
pub struct BorderChars {
    pub top_left: char, pub top_right: char, pub bottom_left: char, pub bottom_right: char,
    pub horizontal: char, pub vertical: char,
}

pub fn border_chars(style: &str) -> BorderChars {
    match style {
        "single" => BorderChars { top_left: '┌', top_right: '┐', bottom_left: '└', bottom_right: '┘', horizontal: '─', vertical: '│' },
        "double" => BorderChars { top_left: '╔', top_right: '╗', bottom_left: '╚', bottom_right: '╝', horizontal: '═', vertical: '║' },
        "round" => BorderChars { top_left: '╭', top_right: '╮', bottom_left: '╰', bottom_right: '╯', horizontal: '─', vertical: '│' },
        "bold" => BorderChars { top_left: '┏', top_right: '┓', bottom_left: '┗', bottom_right: '┛', horizontal: '━', vertical: '┃' },
        _ => BorderChars { top_left: '┌', top_right: '┐', bottom_left: '└', bottom_right: '┘', horizontal: '─', vertical: '│' },
    }
}

/// Render a border around content.
pub fn render_border_box(content: &[String], width: usize, style: &str) -> Vec<String> {
    let chars = border_chars(style);
    let mut result = Vec::new();
    let inner_width = width.saturating_sub(2);
    result.push(format!("{}{}{}", chars.top_left, std::iter::repeat(chars.horizontal).take(inner_width).collect::<String>(), chars.top_right));
    for line in content {
        let line_width = unicode_width::UnicodeWidthStr::width(line.as_str());
        let padding = inner_width.saturating_sub(line_width);
        result.push(format!("{}{}{}{}", chars.vertical, line, " ".repeat(padding), chars.vertical));
    }
    result.push(format!("{}{}{}", chars.bottom_left, std::iter::repeat(chars.horizontal).take(inner_width).collect::<String>(), chars.bottom_right));
    result
}

/// Options for inline border text (label embedded in the top edge).
#[derive(Debug, Clone, Default)]
pub struct BorderTextOptions {
    pub text: String,
    pub align: String, // "left" | "center" | "right"
    pub padding: u8,
}

/// Custom border style descriptors (overrides built-in styles by name).
pub static CUSTOM_BORDER_STYLES: &[(&str, [char; 6])] = &[
    ("dashed", ['┌', '┐', '└', '┘', '╌', '╎']),
    ("ascii", ['+', '+', '+', '+', '-', '|']),
];
