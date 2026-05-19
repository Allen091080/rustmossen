//! Text wrapping (wrap-text.ts).

/// Wrap text to fit within a given width.
pub fn wrap_text(text: &str, max_width: usize, wrap_mode: WrapMode) -> Vec<String> {
    if max_width == 0 { return vec![text.to_string()]; }
    match wrap_mode {
        WrapMode::Wrap => wrap_lines(text, max_width),
        WrapMode::Truncate => truncate_lines(text, max_width, TruncatePosition::End),
        WrapMode::TruncateStart => truncate_lines(text, max_width, TruncatePosition::Start),
        WrapMode::TruncateMiddle => truncate_lines(text, max_width, TruncatePosition::Middle),
        WrapMode::TruncateEnd => truncate_lines(text, max_width, TruncatePosition::End),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode { Wrap, Truncate, TruncateStart, TruncateMiddle, TruncateEnd }

#[derive(Debug, Clone, Copy)]
enum TruncatePosition { Start, Middle, End }

fn wrap_lines(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() { lines.push(String::new()); continue; }
        let mut current = String::new();
        let mut current_width = 0;
        for word in line.split(' ') {
            let word_width = unicode_width::UnicodeWidthStr::width(word);
            if current_width + word_width + if current.is_empty() { 0 } else { 1 } > max_width {
                if !current.is_empty() { lines.push(std::mem::take(&mut current)); current_width = 0; }
                if word_width > max_width {
                    // Break long word
                    for ch in word.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        if current_width + cw > max_width { lines.push(std::mem::take(&mut current)); current_width = 0; }
                        current.push(ch); current_width += cw;
                    }
                } else { current = word.to_string(); current_width = word_width; }
            } else {
                if !current.is_empty() { current.push(' '); current_width += 1; }
                current.push_str(word); current_width += word_width;
            }
        }
        if !current.is_empty() { lines.push(current); }
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

fn truncate_lines(text: &str, max_width: usize, pos: TruncatePosition) -> Vec<String> {
    text.split('\n').map(|line| {
        let width = unicode_width::UnicodeWidthStr::width(line);
        if width <= max_width { return line.to_string(); }
        match pos {
            TruncatePosition::End => { let mut s = String::new(); let mut w = 0; for ch in line.chars() { let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1); if w + cw + 1 > max_width { s.push('…'); break; } s.push(ch); w += cw; } s }
            TruncatePosition::Start => { let chars: Vec<char> = line.chars().rev().collect(); let mut s = String::new(); let mut w = 0; for &ch in &chars { let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1); if w + cw + 1 > max_width { break; } s.insert(0, ch); w += cw; } format!("…{}", s) }
            TruncatePosition::Middle => { let half = max_width / 2; let start: String = line.chars().take(half).collect(); let end: String = line.chars().rev().take(half).collect::<String>().chars().rev().collect(); format!("{}…{}", start, end) }
        }
    }).collect()
}
