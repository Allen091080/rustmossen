//! SGR (Select Graphic Rendition) parsing (sgr.ts).

use super::types::{Color, NamedColor, TextStyle, UnderlineStyle};

/// Apply SGR parameters to a text style.
pub fn apply_sgr(style: &mut TextStyle, params: &[u32]) {
    let mut i = 0;
    while i < params.len() {
        let p = params[i];
        match p {
            0 => *style = TextStyle::default_style(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => {
                // Check for extended underline (4:x)
                if i + 1 < params.len() && params[i + 1] <= 5 {
                    style.underline = match params[i + 1] {
                        0 => UnderlineStyle::None,
                        1 => UnderlineStyle::Single,
                        2 => UnderlineStyle::Double,
                        3 => UnderlineStyle::Curly,
                        4 => UnderlineStyle::Dotted,
                        5 => UnderlineStyle::Dashed,
                        _ => UnderlineStyle::Single,
                    };
                    i += 1;
                } else {
                    style.underline = UnderlineStyle::Single;
                }
            }
            5 => style.blink = true,
            7 => style.inverse = true,
            8 => style.hidden = true,
            9 => style.strikethrough = true,
            21 => style.underline = UnderlineStyle::Double,
            22 => { style.bold = false; style.dim = false; }
            23 => style.italic = false,
            24 => style.underline = UnderlineStyle::None,
            25 => style.blink = false,
            27 => style.inverse = false,
            28 => style.hidden = false,
            29 => style.strikethrough = false,
            30..=37 => style.fg = Color::Named(named_from_sgr(p - 30)),
            38 => { i += parse_extended_color(params, i, &mut style.fg); }
            39 => style.fg = Color::Default,
            40..=47 => style.bg = Color::Named(named_from_sgr(p - 40)),
            48 => { i += parse_extended_color(params, i, &mut style.bg); }
            49 => style.bg = Color::Default,
            53 => style.overline = true,
            55 => style.overline = false,
            58 => { i += parse_extended_color(params, i, &mut style.underline_color); }
            59 => style.underline_color = Color::Default,
            90..=97 => style.fg = Color::Named(bright_named_from_sgr(p - 90)),
            100..=107 => style.bg = Color::Named(bright_named_from_sgr(p - 100)),
            _ => {}
        }
        i += 1;
    }
}

fn named_from_sgr(idx: u32) -> NamedColor {
    match idx {
        0 => NamedColor::Black, 1 => NamedColor::Red, 2 => NamedColor::Green,
        3 => NamedColor::Yellow, 4 => NamedColor::Blue, 5 => NamedColor::Magenta,
        6 => NamedColor::Cyan, _ => NamedColor::White,
    }
}

fn bright_named_from_sgr(idx: u32) -> NamedColor {
    match idx {
        0 => NamedColor::BrightBlack, 1 => NamedColor::BrightRed, 2 => NamedColor::BrightGreen,
        3 => NamedColor::BrightYellow, 4 => NamedColor::BrightBlue, 5 => NamedColor::BrightMagenta,
        6 => NamedColor::BrightCyan, _ => NamedColor::BrightWhite,
    }
}

fn parse_extended_color(params: &[u32], base: usize, color: &mut Color) -> usize {
    if base + 1 >= params.len() { return 0; }
    match params[base + 1] {
        5 => {
            // 256-color: 38;5;n
            if base + 2 < params.len() {
                *color = Color::Indexed(params[base + 2] as u8);
                return 2;
            }
            1
        }
        2 => {
            // RGB: 38;2;r;g;b
            if base + 4 < params.len() {
                *color = Color::Rgb(params[base + 2] as u8, params[base + 3] as u8, params[base + 4] as u8);
                return 4;
            }
            1
        }
        _ => 1,
    }
}
