//! ANSI text to SVG conversion.
//!
//! Parses ANSI escape sequences from terminal text and converts
//! them to SVG with colored `<tspan>` elements.

/// An RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const DEFAULT_FG: AnsiColor = AnsiColor {
    r: 229,
    g: 229,
    b: 229,
};
pub const DEFAULT_BG: AnsiColor = AnsiColor {
    r: 30,
    g: 30,
    b: 30,
};

/// A text span with styling.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub color: AnsiColor,
    pub bold: bool,
}

pub type ParsedLine = Vec<TextSpan>;

fn get_ansi_color(code: u8) -> AnsiColor {
    match code {
        30 => AnsiColor { r: 0, g: 0, b: 0 },
        31 => AnsiColor {
            r: 205,
            g: 49,
            b: 49,
        },
        32 => AnsiColor {
            r: 13,
            g: 188,
            b: 121,
        },
        33 => AnsiColor {
            r: 229,
            g: 229,
            b: 16,
        },
        34 => AnsiColor {
            r: 36,
            g: 114,
            b: 200,
        },
        35 => AnsiColor {
            r: 188,
            g: 63,
            b: 188,
        },
        36 => AnsiColor {
            r: 17,
            g: 168,
            b: 205,
        },
        37 => AnsiColor {
            r: 229,
            g: 229,
            b: 229,
        },
        90 => AnsiColor {
            r: 102,
            g: 102,
            b: 102,
        },
        91 => AnsiColor {
            r: 241,
            g: 76,
            b: 76,
        },
        92 => AnsiColor {
            r: 35,
            g: 209,
            b: 139,
        },
        93 => AnsiColor {
            r: 245,
            g: 245,
            b: 67,
        },
        94 => AnsiColor {
            r: 59,
            g: 142,
            b: 234,
        },
        95 => AnsiColor {
            r: 214,
            g: 112,
            b: 214,
        },
        96 => AnsiColor {
            r: 41,
            g: 184,
            b: 219,
        },
        97 => AnsiColor {
            r: 255,
            g: 255,
            b: 255,
        },
        _ => DEFAULT_FG,
    }
}

fn get_256_color(index: u8) -> AnsiColor {
    if index < 16 {
        let standard: [AnsiColor; 16] = [
            AnsiColor { r: 0, g: 0, b: 0 },
            AnsiColor { r: 128, g: 0, b: 0 },
            AnsiColor { r: 0, g: 128, b: 0 },
            AnsiColor {
                r: 128,
                g: 128,
                b: 0,
            },
            AnsiColor { r: 0, g: 0, b: 128 },
            AnsiColor {
                r: 128,
                g: 0,
                b: 128,
            },
            AnsiColor {
                r: 0,
                g: 128,
                b: 128,
            },
            AnsiColor {
                r: 192,
                g: 192,
                b: 192,
            },
            AnsiColor {
                r: 128,
                g: 128,
                b: 128,
            },
            AnsiColor { r: 255, g: 0, b: 0 },
            AnsiColor { r: 0, g: 255, b: 0 },
            AnsiColor {
                r: 255,
                g: 255,
                b: 0,
            },
            AnsiColor { r: 0, g: 0, b: 255 },
            AnsiColor {
                r: 255,
                g: 0,
                b: 255,
            },
            AnsiColor {
                r: 0,
                g: 255,
                b: 255,
            },
            AnsiColor {
                r: 255,
                g: 255,
                b: 255,
            },
        ];
        return standard[index as usize];
    }
    if index < 232 {
        let i = (index - 16) as u16;
        let r = (i / 36) as u8;
        let g = ((i % 36) / 6) as u8;
        let b = (i % 6) as u8;
        return AnsiColor {
            r: if r == 0 { 0 } else { 55 + r * 40 },
            g: if g == 0 { 0 } else { 55 + g * 40 },
            b: if b == 0 { 0 } else { 55 + b * 40 },
        };
    }
    let gray = (index - 232) * 10 + 8;
    AnsiColor {
        r: gray,
        g: gray,
        b: gray,
    }
}

/// Parse ANSI escape sequences from text.
pub fn parse_ansi(text: &str) -> Vec<ParsedLine> {
    let mut lines: Vec<ParsedLine> = Vec::new();
    for line in text.split('\n') {
        let mut spans: Vec<TextSpan> = Vec::new();
        let mut current_color = DEFAULT_FG;
        let mut bold = false;
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < bytes.len() && !bytes[j].is_ascii_alphabetic() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'm' {
                    let code_str = &line[i + 2..j];
                    let codes: Vec<u8> =
                        code_str.split(';').filter_map(|s| s.parse().ok()).collect();
                    let mut k = 0;
                    while k < codes.len() {
                        let code = codes[k];
                        match code {
                            0 => {
                                current_color = DEFAULT_FG;
                                bold = false;
                            }
                            1 => {
                                bold = true;
                            }
                            30..=37 | 90..=97 => {
                                current_color = get_ansi_color(code);
                            }
                            39 => {
                                current_color = DEFAULT_FG;
                            }
                            38 => {
                                if k + 2 < codes.len() && codes[k + 1] == 5 {
                                    current_color = get_256_color(codes[k + 2]);
                                    k += 2;
                                } else if k + 4 < codes.len() && codes[k + 1] == 2 {
                                    current_color = AnsiColor {
                                        r: codes[k + 2],
                                        g: codes[k + 3],
                                        b: codes[k + 4],
                                    };
                                    k += 4;
                                }
                            }
                            _ => {}
                        }
                        k += 1;
                    }
                }
                i = j + 1;
                continue;
            }
            let text_start = i;
            while i < bytes.len() && bytes[i] != 0x1b {
                i += 1;
            }
            let span_text = &line[text_start..i];
            if !span_text.is_empty() {
                spans.push(TextSpan {
                    text: span_text.to_string(),
                    color: current_color,
                    bold,
                });
            }
        }
        if spans.is_empty() {
            spans.push(TextSpan {
                text: String::new(),
                color: DEFAULT_FG,
                bold: false,
            });
        }
        lines.push(spans);
    }
    lines
}

/// Options for ANSI to SVG conversion.
#[derive(Debug, Clone)]
pub struct AnsiToSvgOptions {
    pub font_family: String,
    pub font_size: u32,
    pub line_height: u32,
    pub padding_x: u32,
    pub padding_y: u32,
    pub background_color: String,
    pub border_radius: u32,
}

impl Default for AnsiToSvgOptions {
    fn default() -> Self {
        Self {
            font_family: "Menlo, Monaco, monospace".to_string(),
            font_size: 14,
            line_height: 22,
            padding_x: 24,
            padding_y: 24,
            background_color: format!("rgb({}, {}, {})", DEFAULT_BG.r, DEFAULT_BG.g, DEFAULT_BG.b),
            border_radius: 8,
        }
    }
}

/// Escape XML special characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Convert ANSI text to SVG.
pub fn ansi_to_svg(ansi_text: &str, options: Option<AnsiToSvgOptions>) -> String {
    let opts = options.unwrap_or_default();
    let mut lines = parse_ansi(ansi_text);

    // Trim trailing empty lines
    while lines
        .last()
        .is_some_and(|l| l.iter().all(|s| s.text.trim().is_empty()))
    {
        lines.pop();
    }

    let char_width_estimate = opts.font_size as f64 * 0.6;
    let max_line_length = lines
        .iter()
        .map(|spans| spans.iter().map(|s| s.text.len()).sum::<usize>())
        .max()
        .unwrap_or(0);
    let width =
        (max_line_length as f64 * char_width_estimate + opts.padding_x as f64 * 2.0).ceil() as u32;
    let height = lines.len() as u32 * opts.line_height + opts.padding_y * 2;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        width, height, width, height
    );
    svg.push_str(&format!(
        "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" rx=\"{}\" ry=\"{}\"/>\n",
        opts.background_color, opts.border_radius, opts.border_radius
    ));
    svg.push_str("  <style>\n");
    svg.push_str(&format!(
        "    text {{ font-family: {}; font-size: {}px; white-space: pre; }}\n",
        opts.font_family, opts.font_size
    ));
    svg.push_str("    .b { font-weight: bold; }\n");
    svg.push_str("  </style>\n");

    for (line_index, spans) in lines.iter().enumerate() {
        let y = opts.padding_y + (line_index as u32 + 1) * opts.line_height
            - (opts.line_height - opts.font_size) / 2;
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" xml:space=\"preserve\">",
            opts.padding_x, y
        ));
        for span in spans {
            if span.text.is_empty() {
                continue;
            }
            let color_str = format!("rgb({}, {}, {})", span.color.r, span.color.g, span.color.b);
            let bold_class = if span.bold { " class=\"b\"" } else { "" };
            svg.push_str(&format!(
                "<tspan fill=\"{}\"{}>{}</tspan>",
                color_str,
                bold_class,
                escape_xml(&span.text)
            ));
        }
        svg.push_str("</text>\n");
    }
    svg.push_str("</svg>");
    svg
}
