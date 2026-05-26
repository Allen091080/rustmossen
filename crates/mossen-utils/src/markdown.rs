//! Markdown rendering utilities for terminal output.
//!
//! Translates parsed Markdown tokens into ANSI-styled terminal text,
//! supporting blockquotes, code blocks, headings, lists, tables, links, etc.

use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};

/// End-of-line constant (always \n, never \r\n).
const EOL: &str = "\n";

/// Blockquote bar character.
const BLOCKQUOTE_BAR: &str = "▐";

/// Whether marked has been configured.
static MARKED_CONFIGURED: AtomicBool = AtomicBool::new(false);

/// Theme name type alias.
pub type ThemeName = String;

/// Trait for CLI code highlighting.
pub trait CliHighlight: Send + Sync {
    fn supports_language(&self, lang: &str) -> bool;
    fn highlight(&self, text: &str, language: &str) -> String;
}

/// Alignment for table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// A parsed markdown token.
#[derive(Debug, Clone)]
pub enum Token {
    Blockquote {
        tokens: Vec<Token>,
    },
    Code {
        text: String,
        lang: Option<String>,
    },
    Codespan {
        text: String,
    },
    Em {
        tokens: Vec<Token>,
    },
    Strong {
        tokens: Vec<Token>,
    },
    Heading {
        depth: u8,
        tokens: Vec<Token>,
    },
    Hr,
    Image {
        href: String,
    },
    Link {
        href: String,
        tokens: Vec<Token>,
    },
    List {
        items: Vec<Token>,
        ordered: bool,
        start: usize,
    },
    ListItem {
        tokens: Vec<Token>,
    },
    Paragraph {
        tokens: Vec<Token>,
    },
    Space,
    Br,
    Text {
        text: String,
        tokens: Option<Vec<Token>>,
    },
    Table {
        header: Vec<TableCell>,
        rows: Vec<Vec<TableCell>>,
        align: Vec<Option<Alignment>>,
    },
    Escape {
        text: String,
    },
    Def,
    Del,
    Html,
}

/// A table cell with its tokens.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub tokens: Vec<Token>,
}

/// Configure the markdown parser (idempotent).
pub fn configure_marked() {
    if MARKED_CONFIGURED.swap(true, Ordering::SeqCst) {}
    // In Rust we use pulldown-cmark; strikethrough is disabled by default
    // unless we enable the extension, so no extra config needed.
}

/// Strip prompt XML tags from content (placeholder - delegates to messages module).
fn strip_prompt_xml_tags(content: &str) -> String {
    // Simple implementation: remove <command-name>...</command-name> etc.
    let re = Regex::new(r"</?(?:command-name|ide_opened_file|context)[^>]*>").unwrap();
    re.replace_all(content, "").to_string()
}

/// Apply markdown formatting to content for terminal display.
pub fn apply_markdown(
    content: &str,
    theme: &ThemeName,
    highlight: Option<&dyn CliHighlight>,
) -> String {
    configure_marked();
    let stripped = strip_prompt_xml_tags(content);
    let tokens = parse_markdown(&stripped);
    tokens
        .iter()
        .map(|t| format_token(t, theme, 0, None, None, highlight))
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

/// Parse markdown text into tokens using pulldown-cmark.
pub fn parse_markdown(input: &str) -> Vec<Token> {
    use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(input, opts);

    let mut tokens: Vec<Token> = Vec::new();
    let mut stack: Vec<(Tag<'_>, Vec<Token>)> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => {
                stack.push((tag.clone(), Vec::new()));
            }
            Event::End(tag_end) => {
                if let Some((_tag, children)) = stack.pop() {
                    let token = match tag_end {
                        TagEnd::Paragraph => Token::Paragraph { tokens: children },
                        TagEnd::Heading(level) => {
                            let depth = match level {
                                HeadingLevel::H1 => 1,
                                HeadingLevel::H2 => 2,
                                HeadingLevel::H3 => 3,
                                HeadingLevel::H4 => 4,
                                HeadingLevel::H5 => 5,
                                HeadingLevel::H6 => 6,
                            };
                            Token::Heading {
                                depth,
                                tokens: children,
                            }
                        }
                        TagEnd::BlockQuote(_) => Token::Blockquote { tokens: children },
                        TagEnd::CodeBlock => {
                            let lang = if let Tag::CodeBlock(CodeBlockKind::Fenced(lang)) = &_tag {
                                if lang.is_empty() {
                                    None
                                } else {
                                    Some(lang.to_string())
                                }
                            } else {
                                None
                            };
                            let text = children
                                .iter()
                                .map(|t| {
                                    if let Token::Text { text, .. } = t {
                                        text.clone()
                                    } else {
                                        String::new()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("");
                            Token::Code { text, lang }
                        }
                        TagEnd::List(_ordered) => {
                            let (ordered, start) = if let Tag::List(first_item) = &_tag {
                                (first_item.is_some(), first_item.unwrap_or(0) as usize)
                            } else {
                                (false, 0)
                            };
                            Token::List {
                                items: children,
                                ordered,
                                start,
                            }
                        }
                        TagEnd::Item => Token::ListItem { tokens: children },
                        TagEnd::Emphasis => Token::Em { tokens: children },
                        TagEnd::Strong => Token::Strong { tokens: children },
                        TagEnd::Link => {
                            let href = if let Tag::Link { dest_url, .. } = &_tag {
                                dest_url.to_string()
                            } else {
                                String::new()
                            };
                            Token::Link {
                                href,
                                tokens: children,
                            }
                        }
                        TagEnd::Image => {
                            let href = if let Tag::Image { dest_url, .. } = &_tag {
                                dest_url.to_string()
                            } else {
                                String::new()
                            };
                            Token::Image { href }
                        }
                        TagEnd::Table => {
                            // Table handling: first child is header row, rest are body rows
                            let mut header = Vec::new();
                            let mut rows = Vec::new();
                            for child in children {
                                if let Token::ListItem { tokens: row_cells } = child {
                                    if header.is_empty() {
                                        header = row_cells
                                            .into_iter()
                                            .map(|c| {
                                                if let Token::ListItem { tokens } = c {
                                                    TableCell { tokens }
                                                } else {
                                                    TableCell { tokens: vec![c] }
                                                }
                                            })
                                            .collect();
                                    } else {
                                        rows.push(
                                            row_cells
                                                .into_iter()
                                                .map(|c| {
                                                    if let Token::ListItem { tokens } = c {
                                                        TableCell { tokens }
                                                    } else {
                                                        TableCell { tokens: vec![c] }
                                                    }
                                                })
                                                .collect(),
                                        );
                                    }
                                }
                            }
                            Token::Table {
                                header,
                                rows,
                                align: Vec::new(),
                            }
                        }
                        TagEnd::TableHead => Token::ListItem { tokens: children },
                        TagEnd::TableRow => Token::ListItem { tokens: children },
                        TagEnd::TableCell => Token::ListItem { tokens: children },
                        _ => Token::Text {
                            text: String::new(),
                            tokens: Some(children),
                        },
                    };
                    if let Some(parent) = stack.last_mut() {
                        parent.1.push(token);
                    } else {
                        tokens.push(token);
                    }
                }
            }
            Event::Text(text) => {
                let token = Token::Text {
                    text: text.to_string(),
                    tokens: None,
                };
                if let Some(parent) = stack.last_mut() {
                    parent.1.push(token);
                } else {
                    tokens.push(token);
                }
            }
            Event::Code(code) => {
                let token = Token::Codespan {
                    text: code.to_string(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.1.push(token);
                } else {
                    tokens.push(token);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                let token = Token::Br;
                if let Some(parent) = stack.last_mut() {
                    parent.1.push(token);
                } else {
                    tokens.push(token);
                }
            }
            Event::Rule => {
                let token = Token::Hr;
                if let Some(parent) = stack.last_mut() {
                    parent.1.push(token);
                } else {
                    tokens.push(token);
                }
            }
            Event::Html(html) => {
                let token = Token::Html;
                let _ = html;
                if let Some(parent) = stack.last_mut() {
                    parent.1.push(token);
                } else {
                    tokens.push(token);
                }
            }
            _ => {}
        }
    }
    tokens
}

/// Format a single token to a styled string.
pub fn format_token(
    token: &Token,
    theme: &ThemeName,
    list_depth: usize,
    ordered_list_number: Option<usize>,
    parent: Option<&Token>,
    highlight: Option<&dyn CliHighlight>,
) -> String {
    match token {
        Token::Blockquote { tokens } => {
            let inner: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, None, highlight))
                .collect::<Vec<_>>()
                .join("");
            let bar = format!("\x1b[2m{}\x1b[0m", BLOCKQUOTE_BAR);
            inner
                .split(EOL)
                .map(|line| {
                    if line.trim().is_empty() {
                        line.to_string()
                    } else {
                        format!("{} \x1b[3m{}\x1b[0m", bar, line)
                    }
                })
                .collect::<Vec<_>>()
                .join(EOL)
        }
        Token::Code { text, lang } => {
            if let Some(hl) = highlight {
                let language = lang.as_deref().unwrap_or("plaintext");
                let language = if hl.supports_language(language) {
                    language
                } else {
                    "plaintext"
                };
                format!("{}{}", hl.highlight(text, language), EOL)
            } else {
                format!("{}{}", text, EOL)
            }
        }
        Token::Codespan { text } => {
            // Inline code with permission color
            format!("\x1b[36m{}\x1b[0m", text)
        }
        Token::Em { tokens } => {
            let inner: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, parent, highlight))
                .collect::<Vec<_>>()
                .join("");
            format!("\x1b[3m{}\x1b[0m", inner)
        }
        Token::Strong { tokens } => {
            let inner: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, parent, highlight))
                .collect::<Vec<_>>()
                .join("");
            format!("\x1b[1m{}\x1b[0m", inner)
        }
        Token::Heading { depth, tokens } => {
            let inner: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, None, highlight))
                .collect::<Vec<_>>()
                .join("");
            match depth {
                1 => format!("\x1b[1;3;4m{}\x1b[0m{}{}", inner, EOL, EOL),
                _ => format!("\x1b[1m{}\x1b[0m{}{}", inner, EOL, EOL),
            }
        }
        Token::Hr => "---".to_string(),
        Token::Image { href } => href.clone(),
        Token::Link { href, tokens } => {
            if href.starts_with("mailto:") {
                return href.replace("mailto:", "");
            }
            let link_text: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, Some(token), highlight))
                .collect::<Vec<_>>()
                .join("");
            let plain = strip_ansi(&link_text);
            if !plain.is_empty() && plain != *href {
                create_hyperlink(href, &link_text)
            } else {
                create_hyperlink(href, href)
            }
        }
        Token::List {
            items,
            ordered,
            start,
        } => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let num = if *ordered { Some(start + index) } else { None };
                format_token(item, theme, list_depth, num, Some(token), highlight)
            })
            .collect::<Vec<_>>()
            .join(""),
        Token::ListItem { tokens } => tokens
            .iter()
            .map(|t| {
                format!(
                    "{}{}",
                    "  ".repeat(list_depth),
                    format_token(
                        t,
                        theme,
                        list_depth + 1,
                        ordered_list_number,
                        Some(token),
                        highlight
                    )
                )
            })
            .collect::<Vec<_>>()
            .join(""),
        Token::Paragraph { tokens } => {
            let inner: String = tokens
                .iter()
                .map(|t| format_token(t, theme, 0, None, None, highlight))
                .collect::<Vec<_>>()
                .join("");
            format!("{}{}", inner, EOL)
        }
        Token::Space => EOL.to_string(),
        Token::Br => EOL.to_string(),
        Token::Text { text, tokens } => {
            if let Some(Token::Link { .. }) = parent {
                return text.clone();
            }
            if let Some(Token::ListItem { .. }) = parent {
                let bullet = match ordered_list_number {
                    None => "-".to_string(),
                    Some(num) => format!("{}.", get_list_number(list_depth, num)),
                };
                let content = if let Some(inner_tokens) = tokens {
                    inner_tokens
                        .iter()
                        .map(|t| {
                            format_token(
                                t,
                                theme,
                                list_depth,
                                ordered_list_number,
                                Some(token),
                                highlight,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("")
                } else {
                    linkify_issue_references(text)
                };
                return format!("{} {}{}", bullet, content, EOL);
            }
            if let Some(inner_tokens) = tokens {
                inner_tokens
                    .iter()
                    .map(|t| {
                        format_token(t, theme, list_depth, ordered_list_number, parent, highlight)
                    })
                    .collect::<Vec<_>>()
                    .join("")
            } else {
                linkify_issue_references(text)
            }
        }
        Token::Table {
            header,
            rows,
            align,
        } => {
            let get_display_text = |tokens: &[Token]| -> String {
                let formatted: String = tokens
                    .iter()
                    .map(|t| format_token(t, theme, 0, None, None, highlight))
                    .collect::<Vec<_>>()
                    .join("");
                strip_ansi(&formatted)
            };

            // Determine column widths
            let column_widths: Vec<usize> = header
                .iter()
                .enumerate()
                .map(|(index, h)| {
                    let mut max_width = unicode_width_str(&get_display_text(&h.tokens));
                    for row in rows {
                        if let Some(cell) = row.get(index) {
                            let cell_len = unicode_width_str(&get_display_text(&cell.tokens));
                            max_width = max_width.max(cell_len);
                        }
                    }
                    max_width.max(3)
                })
                .collect();

            let mut output = String::from("| ");
            for (index, h) in header.iter().enumerate() {
                let content: String = h
                    .tokens
                    .iter()
                    .map(|t| format_token(t, theme, 0, None, None, highlight))
                    .collect::<Vec<_>>()
                    .join("");
                let display_text = get_display_text(&h.tokens);
                let width = column_widths[index];
                let cell_align = align.get(index).copied().flatten();
                output.push_str(&pad_aligned(
                    &content,
                    unicode_width_str(&display_text),
                    width,
                    cell_align,
                ));
                output.push_str(" | ");
            }
            output = output.trim_end().to_string();
            output.push_str(EOL);

            // Separator
            output.push('|');
            for &width in &column_widths {
                output.push_str(&"-".repeat(width + 2));
                output.push('|');
            }
            output.push_str(EOL);

            // Data rows
            for row in rows {
                output.push_str("| ");
                for (index, cell) in row.iter().enumerate() {
                    let content: String = cell
                        .tokens
                        .iter()
                        .map(|t| format_token(t, theme, 0, None, None, highlight))
                        .collect::<Vec<_>>()
                        .join("");
                    let display_text = get_display_text(&cell.tokens);
                    let width = column_widths[index];
                    let cell_align = align.get(index).copied().flatten();
                    output.push_str(&pad_aligned(
                        &content,
                        unicode_width_str(&display_text),
                        width,
                        cell_align,
                    ));
                    output.push_str(" | ");
                }
                output = output.trim_end().to_string();
                output.push_str(EOL);
            }
            output.push_str(EOL);
            output
        }
        Token::Escape { text } => text.clone(),
        Token::Def | Token::Del | Token::Html => String::new(),
    }
}

/// Issue reference pattern: owner/repo#NNN
static ISSUE_REF_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|[^\w./-])([A-Za-z0-9][\w-]*/[A-Za-z0-9][\w.-]*)#(\d+)\b").unwrap()
});

/// Replace owner/repo#123 references with clickable hyperlinks.
fn linkify_issue_references(text: &str) -> String {
    if !supports_hyperlinks() {
        return text.to_string();
    }
    ISSUE_REF_PATTERN
        .replace_all(text, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let repo = &caps[2];
            let num = &caps[3];
            let url = format!("https://github.com/{}/issues/{}", repo, num);
            let display = format!("{}#{}", repo, num);
            format!("{}{}", prefix, create_hyperlink(&url, &display))
        })
        .to_string()
}

/// Convert number to letter (a, b, ..., z, aa, ab, ...).
fn number_to_letter(mut n: usize) -> String {
    let mut result = String::new();
    while n > 0 {
        n -= 1;
        result.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    result
}

/// Roman numeral values.
const ROMAN_VALUES: &[(usize, &str)] = &[
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

/// Convert number to roman numeral.
fn number_to_roman(mut n: usize) -> String {
    let mut result = String::new();
    for &(value, numeral) in ROMAN_VALUES {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

/// Get the list number representation based on depth.
fn get_list_number(list_depth: usize, ordered_list_number: usize) -> String {
    match list_depth {
        0 | 1 => ordered_list_number.to_string(),
        2 => number_to_letter(ordered_list_number),
        3 => number_to_roman(ordered_list_number),
        _ => ordered_list_number.to_string(),
    }
}

/// Pad content to target width according to alignment.
pub fn pad_aligned(
    content: &str,
    display_width: usize,
    target_width: usize,
    align: Option<Alignment>,
) -> String {
    let padding = target_width.saturating_sub(display_width);
    match align {
        Some(Alignment::Center) => {
            let left_pad = padding / 2;
            format!(
                "{}{}{}",
                " ".repeat(left_pad),
                content,
                " ".repeat(padding - left_pad)
            )
        }
        Some(Alignment::Right) => {
            format!("{}{}", " ".repeat(padding), content)
        }
        _ => {
            format!("{}{}", content, " ".repeat(padding))
        }
    }
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    let bytes = strip_ansi_escapes::strip(s);
    String::from_utf8_lossy(&bytes).to_string()
}

/// Calculate Unicode display width of a string.
fn unicode_width_str(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Check if the terminal supports hyperlinks (OSC 8).
fn supports_hyperlinks() -> bool {
    // Heuristic: check if TERM_PROGRAM is known to support hyperlinks
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        matches!(
            term.as_str(),
            "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "ghostty"
        )
    } else {
        false
    }
}

/// Create an OSC 8 hyperlink.
fn create_hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, text)
}
