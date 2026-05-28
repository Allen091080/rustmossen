//! Markdown Rendering for Terminal
//!
//! Renders markdown content with ANSI formatting for terminal display,
//! including headings, lists, code blocks, links, tables, and blockquotes.

use once_cell::sync::Lazy;
use regex::Regex;

/// Pad content to target width according to alignment.
pub fn pad_aligned(
    content: &str,
    display_width: usize,
    target_width: usize,
    align: Option<&str>,
) -> String {
    let padding = target_width.saturating_sub(display_width);
    match align {
        Some("center") => {
            let left_pad = padding / 2;
            format!(
                "{}{}{}",
                " ".repeat(left_pad),
                content,
                " ".repeat(padding - left_pad)
            )
        }
        Some("right") => format!("{}{}", " ".repeat(padding), content),
        _ => format!("{}{}", content, " ".repeat(padding)),
    }
}

/// Convert a number to a lowercase letter sequence (1=a, 2=b, ..., 27=aa).
fn number_to_letter(mut n: usize) -> String {
    let mut result = String::new();
    while n > 0 {
        n -= 1;
        result.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    result
}

/// Roman numeral values for conversion.
const ROMAN_VALUES: &[(u32, &str)] = &[
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

/// Convert a number to lowercase Roman numerals.
fn number_to_roman(mut n: u32) -> String {
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
pub fn get_list_number(list_depth: usize, ordered_list_number: usize) -> String {
    match list_depth {
        0 | 1 => ordered_list_number.to_string(),
        2 => number_to_letter(ordered_list_number),
        3 => number_to_roman(ordered_list_number as u32),
        _ => ordered_list_number.to_string(),
    }
}

/// Pattern matching owner/repo#NNN style GitHub issue/PR references.
static ISSUE_REF_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|[^\w./-])([A-Za-z0-9][\w-]*/[A-Za-z0-9][\w.-]*)#(\d+)\b").unwrap()
});

/// Replace owner/repo#123 references with GitHub URLs.
pub fn linkify_issue_references(text: &str) -> String {
    ISSUE_REF_PATTERN
        .replace_all(text, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let repo = &caps[2];
            let num = &caps[3];
            format!("{}https://github.com/{}/issues/{}", prefix, repo, num)
        })
        .to_string()
}

/// Markdown token types for rendering.
#[derive(Debug, Clone)]
pub enum MarkdownToken {
    Heading {
        depth: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    Code {
        text: String,
        lang: Option<String>,
    },
    CodeSpan {
        text: String,
    },
    List {
        items: Vec<ListItem>,
        ordered: bool,
        start: usize,
    },
    BlockQuote {
        text: String,
    },
    HorizontalRule,
    Link {
        href: String,
        text: String,
    },
    Image {
        href: String,
    },
    Text {
        text: String,
    },
    Space,
    Br,
    Bold {
        text: String,
    },
    Italic {
        text: String,
    },
}

/// A list item in a markdown list.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub text: String,
    pub sub_items: Vec<ListItem>,
}

/// Simple markdown to plain text renderer (strips formatting).
pub fn strip_markdown_formatting(content: &str) -> String {
    // Remove code blocks
    let re_code_block = Regex::new(r"```[\s\S]*?```").unwrap();
    let result = re_code_block.replace_all(content, "");

    // Remove inline code
    let re_inline_code = Regex::new(r"`([^`]+)`").unwrap();
    let result = re_inline_code.replace_all(&result, "$1");

    // Remove bold
    let re_bold = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    let result = re_bold.replace_all(&result, "$1");

    // Remove italic
    let re_italic = Regex::new(r"\*([^*]+)\*").unwrap();
    let result = re_italic.replace_all(&result, "$1");

    // Remove links, keep text
    let re_link = Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    let result = re_link.replace_all(&result, "$1");

    // Remove headings markers
    let re_heading = Regex::new(r"^#{1,6}\s+").unwrap();
    let result = result
        .lines()
        .map(|line| re_heading.replace(line, "").to_string())
        .collect::<Vec<_>>()
        .join("\n");

    result
}

/// Apply markdown formatting and return rendered text.
/// This is a simplified renderer that produces ANSI-colored output.
pub fn apply_markdown(content: &str) -> String {
    // Strip any XML prompt tags
    let stripped = strip_prompt_xml_tags(content);

    let mut output = String::new();
    let lines: Vec<&str> = stripped.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Heading
        if let Some(heading) = line.strip_prefix("# ") {
            output.push_str(&format!("\x1b[1;3;4m{}\x1b[0m\n\n", heading));
            i += 1;
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            output.push_str(&format!("\x1b[1m{}\x1b[0m\n\n", heading));
            i += 1;
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            output.push_str(&format!("\x1b[1m{}\x1b[0m\n\n", heading));
            i += 1;
            continue;
        }

        // Code block
        if line.starts_with("```") {
            i += 1;
            let mut code_lines = Vec::new();
            while i < lines.len() && !lines[i].starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            output.push_str(&code_lines.join("\n"));
            output.push('\n');
            continue;
        }

        // Horizontal rule
        if line == "---" || line == "***" || line == "___" {
            output.push_str("---");
            i += 1;
            continue;
        }

        // Blockquote
        if let Some(quoted) = line.strip_prefix("> ") {
            output.push_str(&format!("\x1b[2m│\x1b[0m \x1b[3m{}\x1b[0m\n", quoted));
            i += 1;
            continue;
        }

        // List item
        if line.starts_with("- ") || line.starts_with("* ") {
            output.push_str(&format!("- {}\n", &line[2..]));
            i += 1;
            continue;
        }

        // Numbered list
        if let Some(_) = Regex::new(r"^\d+\.\s").unwrap().find(line) {
            output.push_str(line);
            output.push('\n');
            i += 1;
            continue;
        }

        // Regular paragraph
        if !line.is_empty() {
            output.push_str(line);
            output.push('\n');
        } else {
            output.push('\n');
        }
        i += 1;
    }

    output.trim().to_string()
}

/// Strip prompt XML tags from content.
fn strip_prompt_xml_tags(content: &str) -> String {
    let re = Regex::new(r"</?(?:system_prompt|assistant_response|user_query)[^>]*>").unwrap();
    re.replace_all(content, "").to_string()
}

/// Configure the markdown parser (no-op in Rust, included for API parity).
pub fn configure_marked() {
    // No configuration needed in Rust implementation
}
