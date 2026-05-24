//! 彩色差异渲染 — 对应 TS 的 native-ts/color-diff/index.ts。
//!
//! 语法高亮 + word diff 的终端彩色 diff 渲染。
//! 使用 ANSI 转义序列输出。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Diff hunk。
#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
}

/// 语法主题信息。
#[derive(Debug, Clone)]
pub struct SyntaxTheme {
    pub theme: String,
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Color / ANSI helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Debug, Clone, Copy)]
struct Style {
    foreground: Color,
    background: Color,
}

type Block = (Style, String);

#[derive(Debug, Clone, Copy, PartialEq)]
enum ColorMode {
    TrueColor,
    Color256,
    Ansi,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Marker {
    Add,
    Delete,
    Context,
}

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const UNDIM: &str = "\x1b[22m";

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

fn ansi_idx(index: u8) -> Color {
    Color {
        r: index,
        g: 0,
        b: 0,
        a: 0,
    }
}

const DEFAULT_BG: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 1,
};

fn detect_color_mode(theme: &str) -> ColorMode {
    if theme.contains("ansi") {
        return ColorMode::Ansi;
    }
    let ct = std::env::var("COLORTERM").unwrap_or_default();
    if ct == "truecolor" || ct == "24bit" {
        ColorMode::TrueColor
    } else {
        ColorMode::Color256
    }
}

const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn ansi256_from_rgb(r: u8, g: u8, b: u8) -> u8 {
    let q = |c: u8| -> u8 {
        if c < 48 {
            0
        } else if c < 115 {
            1
        } else if c < 155 {
            2
        } else if c < 195 {
            3
        } else if c < 235 {
            4
        } else {
            5
        }
    };
    let qr = q(r);
    let qg = q(g);
    let qb = q(b);
    let cube_idx = 16 + 36 * qr + 6 * qg + qb;
    let grey = ((r as u16 + g as u16 + b as u16) / 3) as u8;
    if grey < 5 {
        return 16;
    }
    if grey > 244 && qr == qg && qg == qb {
        return cube_idx;
    }
    let grey_level = ((grey.saturating_sub(8)) as f32 / 10.0)
        .round()
        .max(0.0)
        .min(23.0) as u8;
    let grey_idx = 232 + grey_level;
    let grey_rgb = 8 + grey_level * 10;
    let cr = CUBE_LEVELS[qr as usize];
    let cg = CUBE_LEVELS[qg as usize];
    let cb = CUBE_LEVELS[qb as usize];
    let d_cube = (r as i32 - cr as i32).pow(2)
        + (g as i32 - cg as i32).pow(2)
        + (b as i32 - cb as i32).pow(2);
    let d_grey = (r as i32 - grey_rgb as i32).pow(2)
        + (g as i32 - grey_rgb as i32).pow(2)
        + (b as i32 - grey_rgb as i32).pow(2);
    if d_grey < d_cube {
        grey_idx
    } else {
        cube_idx
    }
}

fn color_to_escape(c: Color, fg: bool, mode: ColorMode) -> String {
    if c.a == 0 {
        let idx = c.r;
        if idx < 8 {
            return format!("\x1b[{}m", if fg { 30 + idx } else { 40 + idx });
        }
        if idx < 16 {
            return format!("\x1b[{}m", if fg { 90 + idx - 8 } else { 100 + idx - 8 });
        }
        return format!("\x1b[{};5;{}m", if fg { 38 } else { 48 }, idx);
    }
    if c.a == 1 {
        return if fg {
            "\x1b[39m".to_string()
        } else {
            "\x1b[49m".to_string()
        };
    }
    let code_type = if fg { 38 } else { 48 };
    if mode == ColorMode::TrueColor {
        format!("\x1b[{};2;{};{};{}m", code_type, c.r, c.g, c.b)
    } else {
        format!("\x1b[{};5;{}m", code_type, ansi256_from_rgb(c.r, c.g, c.b))
    }
}

fn as_terminal_escaped(blocks: &[Block], mode: ColorMode, skip_bg: bool, dim: bool) -> String {
    let mut out = if dim {
        format!("{}{}", RESET, DIM)
    } else {
        RESET.to_string()
    };
    for (style, text) in blocks {
        out.push_str(&color_to_escape(style.foreground, true, mode));
        if !skip_bg {
            out.push_str(&color_to_escape(style.background, false, mode));
        }
        out.push_str(text);
    }
    out.push_str(RESET);
    out
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

struct Theme {
    add_line: Color,
    add_word: Color,
    add_decoration: Color,
    delete_line: Color,
    delete_word: Color,
    delete_decoration: Color,
    foreground: Color,
    background: Color,
    scopes: HashMap<String, Color>,
}

fn default_syntax_theme_name(theme_name: &str) -> String {
    if theme_name.contains("ansi") {
        "ansi".to_string()
    } else if theme_name.contains("dark") {
        "Monokai Extended".to_string()
    } else {
        "GitHub".to_string()
    }
}

fn build_monokai_scopes() -> HashMap<String, Color> {
    let mut m = HashMap::new();
    m.insert("keyword".into(), rgb(249, 38, 114));
    m.insert("_storage".into(), rgb(102, 217, 239));
    m.insert("built_in".into(), rgb(166, 226, 46));
    m.insert("type".into(), rgb(166, 226, 46));
    m.insert("literal".into(), rgb(190, 132, 255));
    m.insert("number".into(), rgb(190, 132, 255));
    m.insert("string".into(), rgb(230, 219, 116));
    m.insert("title".into(), rgb(166, 226, 46));
    m.insert("title.function".into(), rgb(166, 226, 46));
    m.insert("title.class".into(), rgb(166, 226, 46));
    m.insert("title.class.inherited".into(), rgb(166, 226, 46));
    m.insert("params".into(), rgb(253, 151, 31));
    m.insert("comment".into(), rgb(117, 113, 94));
    m.insert("meta".into(), rgb(117, 113, 94));
    m.insert("attr".into(), rgb(166, 226, 46));
    m.insert("attribute".into(), rgb(166, 226, 46));
    m.insert("variable".into(), rgb(255, 255, 255));
    m.insert("variable.language".into(), rgb(255, 255, 255));
    m.insert("property".into(), rgb(255, 255, 255));
    m.insert("operator".into(), rgb(249, 38, 114));
    m.insert("punctuation".into(), rgb(248, 248, 242));
    m.insert("symbol".into(), rgb(190, 132, 255));
    m.insert("regexp".into(), rgb(230, 219, 116));
    m.insert("subst".into(), rgb(248, 248, 242));
    m
}

fn build_github_scopes() -> HashMap<String, Color> {
    let mut m = HashMap::new();
    m.insert("keyword".into(), rgb(167, 29, 93));
    m.insert("_storage".into(), rgb(167, 29, 93));
    m.insert("built_in".into(), rgb(0, 134, 179));
    m.insert("type".into(), rgb(0, 134, 179));
    m.insert("literal".into(), rgb(0, 134, 179));
    m.insert("number".into(), rgb(0, 134, 179));
    m.insert("string".into(), rgb(24, 54, 145));
    m.insert("title".into(), rgb(121, 93, 163));
    m.insert("title.function".into(), rgb(121, 93, 163));
    m.insert("title.class".into(), rgb(0, 0, 0));
    m.insert("title.class.inherited".into(), rgb(0, 0, 0));
    m.insert("params".into(), rgb(0, 134, 179));
    m.insert("comment".into(), rgb(150, 152, 150));
    m.insert("meta".into(), rgb(150, 152, 150));
    m.insert("attr".into(), rgb(0, 134, 179));
    m.insert("attribute".into(), rgb(0, 134, 179));
    m.insert("variable".into(), rgb(0, 134, 179));
    m.insert("variable.language".into(), rgb(0, 134, 179));
    m.insert("property".into(), rgb(0, 134, 179));
    m.insert("operator".into(), rgb(167, 29, 93));
    m.insert("punctuation".into(), rgb(51, 51, 51));
    m.insert("symbol".into(), rgb(0, 134, 179));
    m.insert("regexp".into(), rgb(24, 54, 145));
    m.insert("subst".into(), rgb(51, 51, 51));
    m
}

fn build_ansi_scopes() -> HashMap<String, Color> {
    let mut m = HashMap::new();
    m.insert("keyword".into(), ansi_idx(13));
    m.insert("_storage".into(), ansi_idx(14));
    m.insert("built_in".into(), ansi_idx(14));
    m.insert("type".into(), ansi_idx(14));
    m.insert("literal".into(), ansi_idx(12));
    m.insert("number".into(), ansi_idx(12));
    m.insert("string".into(), ansi_idx(10));
    m.insert("title".into(), ansi_idx(11));
    m.insert("title.function".into(), ansi_idx(11));
    m.insert("title.class".into(), ansi_idx(11));
    m.insert("comment".into(), ansi_idx(8));
    m.insert("meta".into(), ansi_idx(8));
    m
}

fn build_theme(theme_name: &str, mode: ColorMode) -> Theme {
    let is_dark = theme_name.contains("dark");
    let is_ansi = theme_name.contains("ansi");
    let is_daltonized = theme_name.contains("daltonized");
    let tc = mode == ColorMode::TrueColor;

    if is_ansi {
        return Theme {
            add_line: DEFAULT_BG,
            add_word: DEFAULT_BG,
            add_decoration: ansi_idx(10),
            delete_line: DEFAULT_BG,
            delete_word: DEFAULT_BG,
            delete_decoration: ansi_idx(9),
            foreground: ansi_idx(7),
            background: DEFAULT_BG,
            scopes: build_ansi_scopes(),
        };
    }
    if is_dark {
        let fg = rgb(248, 248, 242);
        let dl = rgb(61, 1, 0);
        let dw = rgb(92, 2, 0);
        let dd = rgb(220, 90, 90);
        if is_daltonized {
            return Theme {
                add_line: if tc { rgb(0, 27, 41) } else { ansi_idx(17) },
                add_word: if tc { rgb(0, 48, 71) } else { ansi_idx(24) },
                add_decoration: rgb(81, 160, 200),
                delete_line: dl,
                delete_word: dw,
                delete_decoration: dd,
                foreground: fg,
                background: DEFAULT_BG,
                scopes: build_monokai_scopes(),
            };
        }
        return Theme {
            add_line: if tc { rgb(2, 40, 0) } else { ansi_idx(22) },
            add_word: if tc { rgb(4, 71, 0) } else { ansi_idx(28) },
            add_decoration: rgb(80, 200, 80),
            delete_line: dl,
            delete_word: dw,
            delete_decoration: dd,
            foreground: fg,
            background: DEFAULT_BG,
            scopes: build_monokai_scopes(),
        };
    }
    // light
    let fg = rgb(51, 51, 51);
    let dl = rgb(255, 220, 220);
    let dw = rgb(255, 199, 199);
    let dd = rgb(207, 34, 46);
    if is_daltonized {
        return Theme {
            add_line: rgb(219, 237, 255),
            add_word: rgb(179, 217, 255),
            add_decoration: rgb(36, 87, 138),
            delete_line: dl,
            delete_word: dw,
            delete_decoration: dd,
            foreground: fg,
            background: DEFAULT_BG,
            scopes: build_github_scopes(),
        };
    }
    Theme {
        add_line: rgb(220, 255, 220),
        add_word: rgb(178, 255, 178),
        add_decoration: rgb(36, 138, 61),
        delete_line: dl,
        delete_word: dw,
        delete_decoration: dd,
        foreground: fg,
        background: DEFAULT_BG,
        scopes: build_github_scopes(),
    }
}

fn default_style(theme: &Theme) -> Style {
    Style {
        foreground: theme.foreground,
        background: theme.background,
    }
}

fn line_background(marker: Marker, theme: &Theme) -> Color {
    match marker {
        Marker::Add => theme.add_line,
        Marker::Delete => theme.delete_line,
        Marker::Context => theme.background,
    }
}

fn word_background(marker: Marker, theme: &Theme) -> Color {
    match marker {
        Marker::Add => theme.add_word,
        Marker::Delete => theme.delete_word,
        Marker::Context => theme.background,
    }
}

fn decoration_color(marker: Marker, theme: &Theme) -> Color {
    match marker {
        Marker::Add => theme.add_decoration,
        Marker::Delete => theme.delete_decoration,
        Marker::Context => theme.foreground,
    }
}

// ---------------------------------------------------------------------------
// Word diff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Range {
    start: usize,
    end: usize,
}

const CHANGE_THRESHOLD: f64 = 0.4;

/// 分词：词组、空白组、单个标点。
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_alphanumeric() || ch == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            tokens.push(chars[i..j].iter().collect());
            i = j;
        } else if ch.is_whitespace() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            tokens.push(chars[i..j].iter().collect());
            i = j;
        } else {
            tokens.push(ch.to_string());
            i += 1;
        }
    }
    tokens
}

fn find_adjacent_pairs(markers: &[Marker]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < markers.len() {
        if markers[i] == Marker::Delete {
            let del_start = i;
            let mut del_end = i;
            while del_end < markers.len() && markers[del_end] == Marker::Delete {
                del_end += 1;
            }
            let mut add_end = del_end;
            while add_end < markers.len() && markers[add_end] == Marker::Add {
                add_end += 1;
            }
            let del_count = del_end - del_start;
            let add_count = add_end - del_end;
            if del_count > 0 && add_count > 0 {
                let n = del_count.min(add_count);
                for k in 0..n {
                    pairs.push((del_start + k, del_end + k));
                }
                i = add_end;
            } else {
                i = del_end;
            }
        } else {
            i += 1;
        }
    }
    pairs
}

/// 简化的 word diff（使用 LCS 方式对比 token 序列）。
fn word_diff_strings(old_str: &str, new_str: &str) -> (Vec<Range>, Vec<Range>) {
    let old_tokens = tokenize(old_str);
    let new_tokens = tokenize(new_str);

    let total_len = old_str.len() + new_str.len();
    if total_len == 0 {
        return (Vec::new(), Vec::new());
    }

    // 简化的 diff：比较 token 序列
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    let mut old_off = 0usize;
    let mut new_off = 0usize;
    let mut changed_len = 0usize;

    let mut oi = 0;
    let mut ni = 0;

    while oi < old_tokens.len() && ni < new_tokens.len() {
        if old_tokens[oi] == new_tokens[ni] {
            old_off += old_tokens[oi].len();
            new_off += new_tokens[ni].len();
            oi += 1;
            ni += 1;
        } else {
            // 找到不同的 token 区间
            let old_start = old_off;
            let new_start = new_off;
            // 尝试在后续找到共同 token
            let mut found = false;
            for look in 1..10 {
                if oi + look < old_tokens.len()
                    && ni < new_tokens.len()
                    && old_tokens[oi + look] == new_tokens[ni]
                {
                    // 旧文本有 `look` 个额外 token
                    for k in 0..look {
                        old_off += old_tokens[oi + k].len();
                        changed_len += old_tokens[oi + k].len();
                    }
                    old_ranges.push(Range {
                        start: old_start,
                        end: old_off,
                    });
                    oi += look;
                    found = true;
                    break;
                }
                if ni + look < new_tokens.len()
                    && oi < old_tokens.len()
                    && new_tokens[ni + look] == old_tokens[oi]
                {
                    for k in 0..look {
                        new_off += new_tokens[ni + k].len();
                        changed_len += new_tokens[ni + k].len();
                    }
                    new_ranges.push(Range {
                        start: new_start,
                        end: new_off,
                    });
                    ni += look;
                    found = true;
                    break;
                }
            }
            if !found {
                old_off += old_tokens[oi].len();
                new_off += new_tokens[ni].len();
                changed_len += old_tokens[oi].len() + new_tokens[ni].len();
                old_ranges.push(Range {
                    start: old_start,
                    end: old_off,
                });
                new_ranges.push(Range {
                    start: new_start,
                    end: new_off,
                });
                oi += 1;
                ni += 1;
            }
        }
    }

    // 处理剩余
    while oi < old_tokens.len() {
        let start = old_off;
        old_off += old_tokens[oi].len();
        changed_len += old_tokens[oi].len();
        old_ranges.push(Range {
            start,
            end: old_off,
        });
        oi += 1;
    }
    while ni < new_tokens.len() {
        let start = new_off;
        new_off += new_tokens[ni].len();
        changed_len += new_tokens[ni].len();
        new_ranges.push(Range {
            start,
            end: new_off,
        });
        ni += 1;
    }

    if total_len > 0 && (changed_len as f64 / total_len as f64) > CHANGE_THRESHOLD {
        return (Vec::new(), Vec::new());
    }

    (old_ranges, new_ranges)
}

// ---------------------------------------------------------------------------
// Highlight transform pipeline
// ---------------------------------------------------------------------------

struct Highlight {
    marker: Option<Marker>,
    line_number: usize,
    lines: Vec<Vec<Block>>,
}

fn remove_newlines(h: &mut Highlight) {
    h.lines = h
        .lines
        .iter()
        .map(|line| {
            let mut result = Vec::new();
            for (style, text) in line {
                for part in text.split('\n') {
                    if !part.is_empty() {
                        result.push((*style, part.to_string()));
                    }
                }
            }
            result
        })
        .collect();
}

fn wrap_text(h: &mut Highlight, width: usize, theme: &Theme) {
    let mut new_lines: Vec<Vec<Block>> = Vec::new();
    for line in &h.lines {
        let mut cur: Vec<Block> = Vec::new();
        let mut cur_w = 0usize;
        let mut queue: Vec<Block> = line.clone();
        queue.reverse();

        while let Some((style, text)) = queue.pop() {
            let tw = UnicodeWidthStr::width(text.as_str());
            if cur_w + tw <= width {
                cur.push((style, text));
                cur_w += tw;
            } else {
                let remaining = width.saturating_sub(cur_w);
                let mut byte_pos = 0;
                let mut acc_w = 0;
                for ch in text.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                    if acc_w + cw > remaining {
                        break;
                    }
                    acc_w += cw;
                    byte_pos += ch.len_utf8();
                }
                if byte_pos == 0 {
                    if cur_w == 0 {
                        byte_pos = text.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    } else {
                        new_lines.push(cur);
                        queue.push((style, text));
                        cur = Vec::new();
                        cur_w = 0;
                        continue;
                    }
                }
                cur.push((style, text[..byte_pos].to_string()));
                new_lines.push(cur);
                queue.push((style, text[byte_pos..].to_string()));
                cur = Vec::new();
                cur_w = 0;
            }
        }
        new_lines.push(cur);
    }
    h.lines = new_lines;

    // 填充背景到行末
    if let Some(marker) = h.marker {
        if marker != Marker::Context {
            let bg = line_background(marker, theme);
            let pad_style = Style {
                foreground: theme.foreground,
                background: bg,
            };
            for line in &mut h.lines {
                let cur_w: usize = line
                    .iter()
                    .map(|(_, t)| UnicodeWidthStr::width(t.as_str()))
                    .sum();
                if cur_w < width {
                    line.push((pad_style, " ".repeat(width - cur_w)));
                }
            }
        }
    }
}

fn add_line_number(h: &mut Highlight, theme: &Theme, max_digits: usize, full_dim: bool) {
    let style = Style {
        foreground: if let Some(m) = h.marker {
            decoration_color(m, theme)
        } else {
            theme.foreground
        },
        background: if let Some(m) = h.marker {
            line_background(m, theme)
        } else {
            theme.background
        },
    };
    let should_dim = h.marker.is_none() || h.marker == Some(Marker::Context);
    for (i, line) in h.lines.iter_mut().enumerate() {
        let prefix = if i == 0 {
            format!(" {:>width$} ", h.line_number, width = max_digits)
        } else {
            " ".repeat(max_digits + 2)
        };
        let wrapped = if should_dim && !full_dim {
            format!("{}{}{}", DIM, prefix, UNDIM)
        } else {
            prefix
        };
        line.insert(0, (style, wrapped));
    }
}

fn add_marker(h: &mut Highlight, theme: &Theme) {
    let marker = match h.marker {
        Some(m) => m,
        None => return,
    };
    let style = Style {
        foreground: decoration_color(marker, theme),
        background: line_background(marker, theme),
    };
    let marker_char = match marker {
        Marker::Add => "+",
        Marker::Delete => "-",
        Marker::Context => " ",
    };
    for line in &mut h.lines {
        line.insert(0, (style, marker_char.to_string()));
    }
}

fn apply_background(h: &mut Highlight, theme: &Theme, ranges: &[Range]) {
    let marker = match h.marker {
        Some(m) => m,
        None => return,
    };
    let line_bg = line_background(marker, theme);
    let word_bg = word_background(marker, theme);

    let mut range_idx = 0;
    let mut byte_off = 0;

    for li in 0..h.lines.len() {
        let old_line = std::mem::take(&mut h.lines[li]);
        let mut new_line = Vec::new();
        for (style, text) in &old_line {
            let text_start = byte_off;
            let text_end = byte_off + text.len();
            while range_idx < ranges.len() && ranges[range_idx].end <= text_start {
                range_idx += 1;
            }
            if range_idx >= ranges.len() {
                new_line.push((
                    Style {
                        background: line_bg,
                        ..*style
                    },
                    text.clone(),
                ));
                byte_off = text_end;
                continue;
            }
            let mut remaining = text.as_str();
            let mut pos = text_start;
            while !remaining.is_empty() && range_idx < ranges.len() {
                let r = &ranges[range_idx];
                let in_range = pos >= r.start && pos < r.end;
                let next = if in_range {
                    r.end.min(text_end)
                } else if r.start > pos && r.start < text_end {
                    r.start
                } else {
                    text_end
                };
                let seg_len = next - pos;
                let seg = &remaining[..seg_len];
                new_line.push((
                    Style {
                        background: if in_range { word_bg } else { line_bg },
                        ..*style
                    },
                    seg.to_string(),
                ));
                remaining = &remaining[seg_len..];
                pos = next;
                if pos >= r.end {
                    range_idx += 1;
                }
            }
            if !remaining.is_empty() {
                new_line.push((
                    Style {
                        background: line_bg,
                        ..*style
                    },
                    remaining.to_string(),
                ));
            }
            byte_off = text_end;
        }
        h.lines[li] = new_line;
    }
}

fn into_lines(h: &Highlight, dim: bool, skip_bg: bool, mode: ColorMode) -> Vec<String> {
    h.lines
        .iter()
        .map(|line| as_terminal_escaped(line, mode, skip_bg, dim))
        .collect()
}

// ---------------------------------------------------------------------------
// Language detection
// ---------------------------------------------------------------------------

fn detect_language(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let ext = path.extension()?.to_str()?;
    Some(ext.to_lowercase())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

fn max_line_number(hunk: &Hunk) -> usize {
    let old_end = hunk
        .old_start
        .saturating_add(hunk.old_lines)
        .saturating_sub(1);
    let new_end = hunk
        .new_start
        .saturating_add(hunk.new_lines)
        .saturating_sub(1);
    old_end.max(new_end)
}

fn parse_marker(s: &str) -> Marker {
    match s {
        "+" => Marker::Add,
        "-" => Marker::Delete,
        _ => Marker::Context,
    }
}

/// 彩色 diff 渲染器。
pub struct ColorDiff {
    hunk: Hunk,
    file_path: String,
    _first_line: Option<String>,
    _prefix_content: Option<String>,
}

impl ColorDiff {
    pub fn new(
        hunk: Hunk,
        first_line: Option<String>,
        file_path: String,
        prefix_content: Option<String>,
    ) -> Self {
        Self {
            hunk,
            file_path,
            _first_line: first_line,
            _prefix_content: prefix_content,
        }
    }

    pub fn render(&self, theme_name: &str, width: usize, dim: bool) -> Option<Vec<String>> {
        let mode = detect_color_mode(theme_name);
        let theme = build_theme(theme_name, mode);
        let _lang = detect_language(&self.file_path);

        let max_digits = max_line_number(&self.hunk).to_string().len();
        let mut old_line = self.hunk.old_start;
        let mut new_line = self.hunk.new_start;
        let effective_width = width.saturating_sub(max_digits + 3).max(1);

        struct Entry {
            line_number: usize,
            marker: Marker,
            code: String,
        }
        let entries: Vec<Entry> = self
            .hunk
            .lines
            .iter()
            .map(|raw_line| {
                let (marker_str, code) = if raw_line.is_empty() {
                    (" ", String::new())
                } else {
                    (&raw_line[..1], raw_line[1..].to_string())
                };
                let marker = parse_marker(marker_str);
                let line_number = match marker {
                    Marker::Add => {
                        let n = new_line;
                        new_line += 1;
                        n
                    }
                    Marker::Delete => {
                        let n = old_line;
                        old_line += 1;
                        n
                    }
                    Marker::Context => {
                        let n = new_line;
                        old_line += 1;
                        new_line += 1;
                        n
                    }
                };
                Entry {
                    line_number,
                    marker,
                    code,
                }
            })
            .collect();

        // Word-diff ranges
        let mut ranges: Vec<Vec<Range>> = entries.iter().map(|_| Vec::new()).collect();
        if !dim {
            let markers: Vec<Marker> = entries.iter().map(|e| e.marker).collect();
            for (del_idx, add_idx) in find_adjacent_pairs(&markers) {
                let (del_r, add_r) =
                    word_diff_strings(&entries[del_idx].code, &entries[add_idx].code);
                ranges[del_idx] = del_r;
                ranges[add_idx] = add_r;
            }
        }

        let mut out = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let tokens: Vec<Block> = vec![(default_style(&theme), entry.code.clone())];
            let mut h = Highlight {
                marker: Some(entry.marker),
                line_number: entry.line_number,
                lines: vec![tokens],
            };
            remove_newlines(&mut h);
            apply_background(&mut h, &theme, &ranges[i]);
            wrap_text(&mut h, effective_width, &theme);
            add_marker(&mut h, &theme);
            add_line_number(&mut h, &theme, max_digits, dim);
            out.extend(into_lines(&h, dim, false, mode));
        }
        Some(out)
    }
}

/// 彩色文件渲染器。
pub struct ColorFile {
    code: String,
    file_path: String,
}

impl ColorFile {
    pub fn new(code: String, file_path: String) -> Self {
        Self { code, file_path }
    }

    pub fn render(&self, theme_name: &str, width: usize, dim: bool) -> Option<Vec<String>> {
        let mode = detect_color_mode(theme_name);
        let theme = build_theme(theme_name, mode);
        let mut lines: Vec<&str> = self.code.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let _lang = detect_language(&self.file_path);

        let max_digits = lines.len().to_string().len();
        let effective_width = width.saturating_sub(max_digits + 2).max(1);

        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let tokens: Vec<Block> = vec![(default_style(&theme), line.to_string())];
            let mut h = Highlight {
                marker: None,
                line_number: i + 1,
                lines: vec![tokens],
            };
            remove_newlines(&mut h);
            wrap_text(&mut h, effective_width, &theme);
            add_line_number(&mut h, &theme, max_digits, dim);
            out.extend(into_lines(&h, dim, true, mode));
        }
        Some(out)
    }
}

/// 获取语法主题信息。
pub fn get_syntax_theme(theme_name: &str) -> SyntaxTheme {
    SyntaxTheme {
        theme: default_syntax_theme_name(theme_name),
        source: None,
    }
}
