//! Cursor/text editing utilities.
//!
//! Translates `utils/Cursor.ts` — provides cursor navigation, kill ring,
//! text measurement with Unicode grapheme clusters, and word boundary detection.

use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

// ---------------------------------------------------------------------------
// Kill Ring
// ---------------------------------------------------------------------------

const KILL_RING_MAX_SIZE: usize = 10;

struct KillRingState {
    ring: Vec<String>,
    index: usize,
    last_action_was_kill: bool,
    last_yank_start: usize,
    last_yank_length: usize,
    last_action_was_yank: bool,
}

static KILL_RING: Lazy<Mutex<KillRingState>> = Lazy::new(|| {
    Mutex::new(KillRingState {
        ring: Vec::new(),
        index: 0,
        last_action_was_kill: false,
        last_yank_start: 0,
        last_yank_length: 0,
        last_action_was_yank: false,
    })
});

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KillDirection {
    Prepend,
    Append,
}

pub fn push_to_kill_ring(text: &str, direction: KillDirection) {
    if text.is_empty() {
        return;
    }
    let mut state = KILL_RING.lock().unwrap();
    if state.last_action_was_kill && !state.ring.is_empty() {
        match direction {
            KillDirection::Prepend => {
                state.ring[0] = format!("{}{}", text, state.ring[0]);
            }
            KillDirection::Append => {
                state.ring[0] = format!("{}{}", state.ring[0], text);
            }
        }
    } else {
        state.ring.insert(0, text.to_string());
        if state.ring.len() > KILL_RING_MAX_SIZE {
            state.ring.pop();
        }
    }
    state.last_action_was_kill = true;
    state.last_action_was_yank = false;
}

pub fn get_last_kill() -> String {
    let state = KILL_RING.lock().unwrap();
    state.ring.first().cloned().unwrap_or_default()
}

pub fn get_kill_ring_item(index: i32) -> String {
    let state = KILL_RING.lock().unwrap();
    if state.ring.is_empty() {
        return String::new();
    }
    let len = state.ring.len() as i32;
    let normalized = ((index % len) + len) % len;
    state
        .ring
        .get(normalized as usize)
        .cloned()
        .unwrap_or_default()
}

pub fn get_kill_ring_size() -> usize {
    KILL_RING.lock().unwrap().ring.len()
}

pub fn clear_kill_ring() {
    let mut state = KILL_RING.lock().unwrap();
    state.ring.clear();
    state.index = 0;
    state.last_action_was_kill = false;
    state.last_action_was_yank = false;
    state.last_yank_start = 0;
    state.last_yank_length = 0;
}

pub fn reset_kill_accumulation() {
    KILL_RING.lock().unwrap().last_action_was_kill = false;
}

pub fn record_yank(start: usize, length: usize) {
    let mut state = KILL_RING.lock().unwrap();
    state.last_yank_start = start;
    state.last_yank_length = length;
    state.last_action_was_yank = true;
    state.index = 0;
}

pub fn can_yank_pop() -> bool {
    let state = KILL_RING.lock().unwrap();
    state.last_action_was_yank && state.ring.len() > 1
}

#[derive(Debug, Clone)]
pub struct YankPopResult {
    pub text: String,
    pub start: usize,
    pub length: usize,
}

pub fn yank_pop() -> Option<YankPopResult> {
    let mut state = KILL_RING.lock().unwrap();
    if !state.last_action_was_yank || state.ring.len() <= 1 {
        return None;
    }
    state.index = (state.index + 1) % state.ring.len();
    let text = state.ring.get(state.index).cloned().unwrap_or_default();
    Some(YankPopResult {
        text,
        start: state.last_yank_start,
        length: state.last_yank_length,
    })
}

pub fn update_yank_length(length: usize) {
    KILL_RING.lock().unwrap().last_yank_length = length;
}

pub fn reset_yank_state() {
    KILL_RING.lock().unwrap().last_action_was_yank = false;
}

// ---------------------------------------------------------------------------
// Vim character classification
// ---------------------------------------------------------------------------

static VIM_WORD_CHAR_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\p{L}\p{N}\p{M}_]$").unwrap());

static WHITESPACE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s").unwrap());

pub fn is_vim_word_char(ch: &str) -> bool {
    VIM_WORD_CHAR_REGEX.is_match(ch)
}

pub fn is_vim_whitespace(ch: &str) -> bool {
    WHITESPACE_REGEX.is_match(ch)
}

pub fn is_vim_punctuation(ch: &str) -> bool {
    !ch.is_empty() && !is_vim_whitespace(ch) && !is_vim_word_char(ch)
}

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

// ---------------------------------------------------------------------------
// WrappedLine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct WrappedLine {
    text: String,
    start_offset: usize,
    is_preceded_by_newline: bool,
    ends_with_newline: bool,
}

impl WrappedLine {
    fn new(
        text: String,
        start_offset: usize,
        is_preceded_by_newline: bool,
        ends_with_newline: bool,
    ) -> Self {
        Self {
            text,
            start_offset,
            is_preceded_by_newline,
            ends_with_newline,
        }
    }

    fn length(&self) -> usize {
        self.text.len() + if self.ends_with_newline { 1 } else { 0 }
    }
}

// ---------------------------------------------------------------------------
// MeasuredText
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MeasuredText {
    pub text: String,
    pub columns: usize,
    wrapped_lines: Option<Vec<WrappedLine>>,
    grapheme_boundaries: Option<Vec<usize>>,
    word_boundaries_cache: Option<Vec<WordBoundary>>,
}

#[derive(Debug, Clone)]
pub struct WordBoundary {
    pub start: usize,
    pub end: usize,
    pub is_word_like: bool,
}

impl MeasuredText {
    pub fn new(text: &str, columns: usize) -> Self {
        // Normalize to NFC
        let normalized = unicode_normalization_nfc(text);
        Self {
            text: normalized,
            columns,
            wrapped_lines: None,
            grapheme_boundaries: None,
            word_boundaries_cache: None,
        }
    }

    fn get_wrapped_lines_internal(&mut self) -> &[WrappedLine] {
        if self.wrapped_lines.is_none() {
            self.wrapped_lines = Some(self.measure_wrapped_text());
        }
        self.wrapped_lines.as_ref().unwrap()
    }

    fn get_grapheme_boundaries_internal(&mut self) -> &[usize] {
        if self.grapheme_boundaries.is_none() {
            let mut boundaries = Vec::new();
            for (idx, _) in self.text.grapheme_indices(true) {
                boundaries.push(idx);
            }
            boundaries.push(self.text.len());
            self.grapheme_boundaries = Some(boundaries);
        }
        self.grapheme_boundaries.as_ref().unwrap()
    }

    pub fn get_word_boundaries(&mut self) -> &[WordBoundary] {
        if self.word_boundaries_cache.is_none() {
            let mut boundaries = Vec::new();
            let mut idx = 0;
            for word in self.text.split_word_bounds() {
                let start = idx;
                let end = idx + word.len();
                let is_word_like = word.chars().any(|c| c.is_alphanumeric());
                boundaries.push(WordBoundary {
                    start,
                    end,
                    is_word_like,
                });
                idx = end;
            }
            self.word_boundaries_cache = Some(boundaries);
        }
        self.word_boundaries_cache.as_ref().unwrap()
    }

    pub fn get_wrapped_text(&mut self) -> Vec<String> {
        let lines = self.get_wrapped_lines_internal();
        lines
            .iter()
            .map(|line| {
                if line.is_preceded_by_newline {
                    line.text.clone()
                } else {
                    line.text.trim_start().to_string()
                }
            })
            .collect()
    }

    pub fn get_wrapped_lines(&mut self) -> Vec<(String, usize)> {
        let lines = self.get_wrapped_lines_internal();
        lines
            .iter()
            .map(|l| (l.text.clone(), l.start_offset))
            .collect()
    }

    pub fn line_count(&mut self) -> usize {
        self.get_wrapped_lines_internal().len()
    }

    pub fn next_offset(&mut self, offset: usize) -> usize {
        let boundaries = self.get_grapheme_boundaries_internal().to_vec();
        self.binary_search_boundary(&boundaries, offset, true)
    }

    pub fn prev_offset(&mut self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        let boundaries = self.get_grapheme_boundaries_internal().to_vec();
        self.binary_search_boundary(&boundaries, offset, false)
    }

    pub fn snap_to_grapheme_boundary(&mut self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        if offset >= self.text.len() {
            return self.text.len();
        }
        let boundaries = self.get_grapheme_boundaries_internal();
        // Binary search for largest boundary <= offset
        let mut lo: usize = 0;
        let mut hi = boundaries.len() - 1;
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if boundaries[mid] <= offset {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        boundaries[lo]
    }

    fn binary_search_boundary(
        &self,
        boundaries: &[usize],
        target: usize,
        find_next: bool,
    ) -> usize {
        let mut left: usize = 0;
        let mut right = if boundaries.is_empty() {
            return if find_next { self.text.len() } else { 0 };
        } else {
            boundaries.len() - 1
        };
        let mut result = if find_next { self.text.len() } else { 0 };

        while left <= right {
            let mid = (left + right) / 2;
            let boundary = boundaries[mid];

            if find_next {
                if boundary > target {
                    result = boundary;
                    if mid == 0 {
                        break;
                    }
                    right = mid - 1;
                } else {
                    left = mid + 1;
                }
            } else {
                if boundary < target {
                    result = boundary;
                    left = mid + 1;
                } else {
                    if mid == 0 {
                        break;
                    }
                    right = mid - 1;
                }
            }
        }

        result
    }

    pub fn string_index_to_display_width(text: &str, index: usize) -> usize {
        if index == 0 {
            return 0;
        }
        if index >= text.len() {
            return UnicodeWidthStr::width(text);
        }
        UnicodeWidthStr::width(&text[..index])
    }

    pub fn display_width_to_string_index(&mut self, text: &str, target_width: usize) -> usize {
        if target_width == 0 || text.is_empty() {
            return 0;
        }

        let mut current_width: usize = 0;
        let mut current_offset: usize = 0;

        for (idx, grapheme) in text.grapheme_indices(true) {
            let seg_width = UnicodeWidthStr::width(grapheme);
            if current_width + seg_width > target_width {
                break;
            }
            current_width += seg_width;
            current_offset = idx + grapheme.len();
        }

        current_offset
    }

    pub fn get_position_from_offset(&mut self, offset: usize) -> Position {
        let lines = self.get_wrapped_lines_internal().to_vec();
        for (line_idx, current_line) in lines.iter().enumerate() {
            let next_start = lines.get(line_idx + 1).map(|l| l.start_offset);
            if offset >= current_line.start_offset
                && (next_start.is_none() || offset < next_start.unwrap())
            {
                let str_pos_in_line = offset - current_line.start_offset;
                let display_column = if current_line.is_preceded_by_newline {
                    Self::string_index_to_display_width(&current_line.text, str_pos_in_line)
                } else {
                    let leading_ws = current_line.text.len() - current_line.text.trim_start().len();
                    if str_pos_in_line < leading_ws {
                        0
                    } else {
                        let trimmed = current_line.text.trim_start();
                        let pos_in_trimmed = str_pos_in_line - leading_ws;
                        Self::string_index_to_display_width(trimmed, pos_in_trimmed)
                    }
                };
                return Position {
                    line: line_idx,
                    column: display_column,
                };
            }
        }

        // Past the last character
        let line = lines.len().saturating_sub(1);
        let last_line = &lines[line];
        Position {
            line,
            column: UnicodeWidthStr::width(last_line.text.as_str()),
        }
    }

    pub fn get_offset_from_position(&mut self, position: Position) -> usize {
        let lines = self.get_wrapped_lines_internal().to_vec();
        let line_idx = position.line.min(lines.len().saturating_sub(1));
        let wrapped_line = &lines[line_idx];

        if wrapped_line.text.is_empty() && wrapped_line.ends_with_newline {
            return wrapped_line.start_offset;
        }

        let leading_ws = if wrapped_line.is_preceded_by_newline {
            0
        } else {
            wrapped_line.text.len() - wrapped_line.text.trim_start().len()
        };

        let display_col_with_leading = position.column + leading_ws;
        let string_index =
            self.display_width_to_string_index(&wrapped_line.text, display_col_with_leading);

        let offset = wrapped_line.start_offset + string_index;
        let line_end = wrapped_line.start_offset + wrapped_line.text.len();

        let mut max_offset = line_end;
        let line_display_width = UnicodeWidthStr::width(wrapped_line.text.as_str());
        if wrapped_line.ends_with_newline && position.column > line_display_width {
            max_offset = line_end + 1;
        }

        offset.min(max_offset)
    }

    pub fn get_line_length(&mut self, line: usize) -> usize {
        let lines = self.get_wrapped_lines_internal().to_vec();
        let idx = line.min(lines.len().saturating_sub(1));
        UnicodeWidthStr::width(lines[idx].text.as_str())
    }

    fn measure_wrapped_text(&self) -> Vec<WrappedLine> {
        let wrapped = wrap_text(&self.text, self.columns);
        let mut wrapped_lines = Vec::new();
        let mut search_offset: usize = 0;
        let mut last_newline_pos: Option<usize> = None;

        let lines: Vec<&str> = wrapped.split('\n').collect();
        for (i, line_text) in lines.iter().enumerate() {
            let is_preceded_by_newline = |start_offset: usize| -> bool {
                i == 0
                    || (start_offset > 0
                        && self.text.as_bytes().get(start_offset - 1) == Some(&b'\n'))
            };

            if line_text.is_empty() {
                // Blank line: find next newline
                let search_start = last_newline_pos.map(|p| p + 1).unwrap_or(0);
                if let Some(pos) = self.text[search_start..].find('\n') {
                    let actual_pos = search_start + pos;
                    last_newline_pos = Some(actual_pos);
                    wrapped_lines.push(WrappedLine::new(
                        String::new(),
                        actual_pos,
                        is_preceded_by_newline(actual_pos),
                        true,
                    ));
                } else {
                    wrapped_lines.push(WrappedLine::new(
                        String::new(),
                        self.text.len(),
                        is_preceded_by_newline(self.text.len()),
                        false,
                    ));
                }
            } else {
                // Find text in self.text
                if let Some(pos) = self.text[search_offset..].find(line_text) {
                    let start_offset = search_offset + pos;
                    search_offset = start_offset + line_text.len();

                    let potential_newline_pos = start_offset + line_text.len();
                    let ends_with_newline = potential_newline_pos < self.text.len()
                        && self.text.as_bytes()[potential_newline_pos] == b'\n';

                    if ends_with_newline {
                        last_newline_pos = Some(potential_newline_pos);
                    }

                    wrapped_lines.push(WrappedLine::new(
                        line_text.to_string(),
                        start_offset,
                        is_preceded_by_newline(start_offset),
                        ends_with_newline,
                    ));
                } else {
                    // Fallback: should not normally happen
                    wrapped_lines.push(WrappedLine::new(
                        line_text.to_string(),
                        search_offset,
                        true,
                        false,
                    ));
                }
            }
        }

        wrapped_lines
    }
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Cursor {
    pub measured_text: MeasuredText,
    pub offset: usize,
    pub selection: usize,
}

impl Cursor {
    pub fn new(measured_text: MeasuredText, offset: usize, selection: usize) -> Self {
        let max_offset = measured_text.text.len();
        let clamped = offset.min(max_offset);
        Self {
            measured_text,
            offset: clamped,
            selection,
        }
    }

    pub fn from_text(text: &str, columns: usize, offset: usize, selection: usize) -> Self {
        let mt = MeasuredText::new(text, columns.saturating_sub(1));
        Self::new(mt, offset, selection)
    }

    pub fn text(&self) -> &str {
        &self.measured_text.text
    }

    fn columns(&self) -> usize {
        self.measured_text.columns + 1
    }

    pub fn get_position(&mut self) -> Position {
        self.measured_text.get_position_from_offset(self.offset)
    }

    fn get_offset(&mut self, position: Position) -> usize {
        self.measured_text.get_offset_from_position(position)
    }

    pub fn get_viewport_start_line(&mut self, max_visible_lines: Option<usize>) -> usize {
        let max = match max_visible_lines {
            Some(m) if m > 0 => m,
            _ => return 0,
        };
        let pos = self.get_position();
        let line_count = self.measured_text.line_count();
        if line_count <= max {
            return 0;
        }
        let half = max / 2;
        let mut start_line = pos.line.saturating_sub(half);
        let end_line = (start_line + max).min(line_count);
        if end_line - start_line < max {
            start_line = end_line.saturating_sub(max);
        }
        start_line
    }

    pub fn left(&mut self) -> Cursor {
        if self.offset == 0 {
            return self.clone();
        }

        // Check for image ref ending at offset
        if let Some(chip) = self.image_ref_ending_at(self.offset) {
            return Cursor::new(self.measured_text.clone(), chip.0, 0);
        }

        let prev = self.measured_text.prev_offset(self.offset);
        Cursor::new(self.measured_text.clone(), prev, 0)
    }

    pub fn right(&mut self) -> Cursor {
        if self.offset >= self.text().len() {
            return self.clone();
        }

        // Check for image ref starting at offset
        if let Some(chip) = self.image_ref_starting_at(self.offset) {
            return Cursor::new(self.measured_text.clone(), chip.1, 0);
        }

        let next = self.measured_text.next_offset(self.offset);
        Cursor::new(self.measured_text.clone(), next.min(self.text().len()), 0)
    }

    /// If an [Image #N] chip ends at offset, return (start, end).
    fn image_ref_ending_at(&self, offset: usize) -> Option<(usize, usize)> {
        let before = &self.text()[..offset];
        let re = Regex::new(r"\[Image #\d+\]$").unwrap();
        re.find(before).map(|m| (m.start(), offset))
    }

    fn image_ref_starting_at(&self, offset: usize) -> Option<(usize, usize)> {
        let after = &self.text()[offset..];
        let re = Regex::new(r"^\[Image #\d+\]").unwrap();
        re.find(after).map(|m| (offset, offset + m.end()))
    }

    /// Snap offset out of an image ref chip.
    pub fn snap_out_of_image_ref(&self, offset: usize, toward_start: bool) -> usize {
        let re = Regex::new(r"\[Image #\d+\]").unwrap();
        for m in re.find_iter(self.text()) {
            let start = m.start();
            let end = m.end();
            if offset > start && offset < end {
                return if toward_start { start } else { end };
            }
        }
        offset
    }

    pub fn up(&mut self) -> Cursor {
        let pos = self.get_position();
        if pos.line == 0 {
            return self.clone();
        }
        let prev_line_text = {
            let lines = self.measured_text.get_wrapped_text();
            lines.get(pos.line - 1).cloned()
        };
        let Some(prev_line) = prev_line_text else {
            return self.clone();
        };
        let prev_line_width = UnicodeWidthStr::width(prev_line.as_str());
        let col = if pos.column > prev_line_width {
            prev_line_width
        } else {
            pos.column
        };
        let new_offset = self.get_offset(Position {
            line: pos.line - 1,
            column: col,
        });
        Cursor::new(self.measured_text.clone(), new_offset, 0)
    }

    pub fn down(&mut self) -> Cursor {
        let pos = self.get_position();
        let line_count = self.measured_text.line_count();
        if pos.line >= line_count - 1 {
            return self.clone();
        }
        let next_line_text = {
            let lines = self.measured_text.get_wrapped_text();
            lines.get(pos.line + 1).cloned()
        };
        let Some(next_line) = next_line_text else {
            return self.clone();
        };
        let next_line_width = UnicodeWidthStr::width(next_line.as_str());
        let col = if pos.column > next_line_width {
            next_line_width
        } else {
            pos.column
        };
        let new_offset = self.get_offset(Position {
            line: pos.line + 1,
            column: col,
        });
        Cursor::new(self.measured_text.clone(), new_offset, 0)
    }

    pub fn start_of_line(&mut self) -> Cursor {
        let pos = self.get_position();
        if pos.column == 0 && pos.line > 0 {
            let off = self.get_offset(Position {
                line: pos.line - 1,
                column: 0,
            });
            return Cursor::new(self.measured_text.clone(), off, 0);
        }
        let off = self.get_offset(Position {
            line: pos.line,
            column: 0,
        });
        Cursor::new(self.measured_text.clone(), off, 0)
    }

    pub fn end_of_line(&mut self) -> Cursor {
        let pos = self.get_position();
        let col = self.measured_text.get_line_length(pos.line);
        let off = self.get_offset(Position {
            line: pos.line,
            column: col,
        });
        Cursor::new(self.measured_text.clone(), off, 0)
    }

    pub fn first_non_blank_in_line(&mut self) -> Cursor {
        let pos = self.get_position();
        let lines = self.measured_text.get_wrapped_text();
        let line_text = lines.get(pos.line).cloned().unwrap_or_default();
        let col = line_text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
        let display_col = MeasuredText::string_index_to_display_width(&line_text, col);
        let off = self.get_offset(Position {
            line: pos.line,
            column: display_col,
        });
        Cursor::new(self.measured_text.clone(), off, 0)
    }

    fn find_logical_line_start(&self, from_offset: usize) -> usize {
        if from_offset == 0 {
            return 0;
        }
        match self.text()[..from_offset].rfind('\n') {
            Some(pos) => pos + 1,
            None => 0,
        }
    }

    fn find_logical_line_end(&self, from_offset: usize) -> usize {
        match self.text()[from_offset..].find('\n') {
            Some(pos) => from_offset + pos,
            None => self.text().len(),
        }
    }

    pub fn end_of_logical_line(&self) -> Cursor {
        let end = self.find_logical_line_end(self.offset);
        Cursor::new(self.measured_text.clone(), end, 0)
    }

    pub fn start_of_logical_line(&self) -> Cursor {
        let start = self.find_logical_line_start(self.offset);
        Cursor::new(self.measured_text.clone(), start, 0)
    }

    pub fn first_non_blank_in_logical_line(&self) -> Cursor {
        let start = self.find_logical_line_start(self.offset);
        let end = self.find_logical_line_end(self.offset);
        let line_text = &self.text()[start..end];
        let first_non_blank = line_text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
        Cursor::new(self.measured_text.clone(), start + first_non_blank, 0)
    }

    pub fn up_logical_line(&self) -> Cursor {
        let current_start = self.find_logical_line_start(self.offset);
        if current_start == 0 {
            return Cursor::new(self.measured_text.clone(), 0, 0);
        }
        let current_column = self.offset - current_start;
        let prev_line_end = current_start - 1;
        let prev_line_start = self.find_logical_line_start(prev_line_end);
        let prev_line_len = prev_line_end - prev_line_start;
        let clamped_col = current_column.min(prev_line_len);
        let raw_offset = prev_line_start + clamped_col;
        let mut mt = self.measured_text.clone();
        let offset = mt.snap_to_grapheme_boundary(raw_offset);
        Cursor::new(self.measured_text.clone(), offset, 0)
    }

    pub fn down_logical_line(&self) -> Cursor {
        let current_start = self.find_logical_line_start(self.offset);
        let current_end = self.find_logical_line_end(self.offset);
        if current_end >= self.text().len() {
            return Cursor::new(self.measured_text.clone(), self.text().len(), 0);
        }
        let current_column = self.offset - current_start;
        let next_line_start = current_end + 1;
        let next_line_end = self.find_logical_line_end(next_line_start);
        let next_line_len = next_line_end - next_line_start;
        let clamped_col = current_column.min(next_line_len);
        let raw_offset = next_line_start + clamped_col;
        let mut mt = self.measured_text.clone();
        let offset = mt.snap_to_grapheme_boundary(raw_offset);
        Cursor::new(self.measured_text.clone(), offset, 0)
    }

    pub fn next_word(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let boundaries = self.measured_text.get_word_boundaries().to_vec();
        for b in &boundaries {
            if b.is_word_like && b.start > self.offset {
                return Cursor::new(self.measured_text.clone(), b.start, 0);
            }
        }
        Cursor::new(self.measured_text.clone(), self.text().len(), 0)
    }

    pub fn prev_word(&mut self) -> Cursor {
        if self.is_at_start() {
            return self.clone();
        }
        let boundaries = self.measured_text.get_word_boundaries().to_vec();
        let mut prev_start: Option<usize> = None;
        for b in &boundaries {
            if !b.is_word_like {
                continue;
            }
            if b.start < self.offset {
                if self.offset > b.start && self.offset <= b.end {
                    return Cursor::new(self.measured_text.clone(), b.start, 0);
                }
                prev_start = Some(b.start);
            }
        }
        Cursor::new(self.measured_text.clone(), prev_start.unwrap_or(0), 0)
    }

    pub fn end_of_word(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let boundaries = self.measured_text.get_word_boundaries().to_vec();
        for b in &boundaries {
            if !b.is_word_like {
                continue;
            }
            if self.offset >= b.start && self.offset < b.end.saturating_sub(1) {
                return Cursor::new(self.measured_text.clone(), b.end - 1, 0);
            }
            if self.offset == b.end.saturating_sub(1) {
                // Find next word's end
                for nb in &boundaries {
                    if nb.is_word_like && nb.start > self.offset {
                        return Cursor::new(self.measured_text.clone(), nb.end - 1, 0);
                    }
                }
                return self.clone();
            }
        }
        for b in &boundaries {
            if b.is_word_like && b.start > self.offset {
                return Cursor::new(self.measured_text.clone(), b.end - 1, 0);
            }
        }
        self.clone()
    }

    // Vim-specific word methods
    pub fn next_vim_word(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let mut pos = self.offset;
        let current = self.grapheme_at(pos);
        if current.is_empty() {
            return self.clone();
        }

        if is_vim_word_char(&current) {
            while pos < self.text().len() && is_vim_word_char(&self.grapheme_at(pos)) {
                pos = self.measured_text.next_offset(pos);
            }
        } else if is_vim_punctuation(&current) {
            while pos < self.text().len() && is_vim_punctuation(&self.grapheme_at(pos)) {
                pos = self.measured_text.next_offset(pos);
            }
        }

        while pos < self.text().len() && is_vim_whitespace(&self.grapheme_at(pos)) {
            pos = self.measured_text.next_offset(pos);
        }

        Cursor::new(self.measured_text.clone(), pos, 0)
    }

    pub fn prev_vim_word(&mut self) -> Cursor {
        if self.is_at_start() {
            return self.clone();
        }
        let mut pos = self.measured_text.prev_offset(self.offset);

        while pos > 0 && is_vim_whitespace(&self.grapheme_at(pos)) {
            pos = self.measured_text.prev_offset(pos);
        }

        if pos == 0 && is_vim_whitespace(&self.grapheme_at(0)) {
            return Cursor::new(self.measured_text.clone(), 0, 0);
        }

        let ch = self.grapheme_at(pos);
        if is_vim_word_char(&ch) {
            while pos > 0 {
                let prev = self.measured_text.prev_offset(pos);
                if !is_vim_word_char(&self.grapheme_at(prev)) {
                    break;
                }
                pos = prev;
            }
        } else if is_vim_punctuation(&ch) {
            while pos > 0 {
                let prev = self.measured_text.prev_offset(pos);
                if !is_vim_punctuation(&self.grapheme_at(prev)) {
                    break;
                }
                pos = prev;
            }
        }

        Cursor::new(self.measured_text.clone(), pos, 0)
    }

    pub fn end_of_vim_word(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let mut pos = self.measured_text.next_offset(self.offset);

        while pos < self.text().len() && is_vim_whitespace(&self.grapheme_at(pos)) {
            pos = self.measured_text.next_offset(pos);
        }

        if pos >= self.text().len() {
            return Cursor::new(self.measured_text.clone(), self.text().len(), 0);
        }

        let ch = self.grapheme_at(pos);
        if is_vim_word_char(&ch) {
            while pos < self.text().len() {
                let next = self.measured_text.next_offset(pos);
                if next >= self.text().len() || !is_vim_word_char(&self.grapheme_at(next)) {
                    break;
                }
                pos = next;
            }
        } else if is_vim_punctuation(&ch) {
            while pos < self.text().len() {
                let next = self.measured_text.next_offset(pos);
                if next >= self.text().len() || !is_vim_punctuation(&self.grapheme_at(next)) {
                    break;
                }
                pos = next;
            }
        }

        Cursor::new(self.measured_text.clone(), pos, 0)
    }

    pub fn next_word_big(&mut self) -> Cursor {
        let mut cursor = self.clone();
        while !cursor.is_over_whitespace() && !cursor.is_at_end() {
            cursor = cursor.right();
        }
        while cursor.is_over_whitespace() && !cursor.is_at_end() {
            cursor = cursor.right();
        }
        cursor
    }

    pub fn prev_word_big(&mut self) -> Cursor {
        let mut cursor = self.clone();
        if cursor.left().is_over_whitespace() {
            cursor = cursor.left();
        }
        while cursor.is_over_whitespace() && !cursor.is_at_start() {
            cursor = cursor.left();
        }
        if !cursor.is_over_whitespace() {
            while !cursor.left().is_over_whitespace() && !cursor.is_at_start() {
                cursor = cursor.left();
            }
        }
        cursor
    }

    pub fn end_of_word_big(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let mut cursor = self.clone();
        let at_end_of_word = !cursor.is_over_whitespace()
            && (cursor.right().is_over_whitespace() || cursor.right().is_at_end());
        if at_end_of_word {
            cursor = cursor.right();
            return cursor.end_of_word_big();
        }
        if cursor.is_over_whitespace() {
            cursor = cursor.next_word_big();
        }
        while !cursor.right().is_over_whitespace() && !cursor.is_at_end() {
            cursor = cursor.right();
        }
        cursor
    }

    // Text modification
    pub fn modify_text(&self, end: &Cursor, insert_string: &str) -> Cursor {
        let start_offset = self.offset;
        let end_offset = end.offset;
        let new_text = format!(
            "{}{}{}",
            &self.text()[..start_offset],
            insert_string,
            &self.text()[end_offset..]
        );
        let normalized_insert = unicode_normalization_nfc(insert_string);
        Cursor::from_text(
            &new_text,
            self.columns(),
            start_offset + normalized_insert.len(),
            0,
        )
    }

    pub fn insert(&self, insert_string: &str) -> Cursor {
        self.modify_text(self, insert_string)
    }

    pub fn del(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let right = self.right();
        self.modify_text(&right, "")
    }

    pub fn backspace(&mut self) -> Cursor {
        if self.is_at_start() {
            return self.clone();
        }
        let left = self.left();
        left.modify_text(self, "")
    }

    pub fn delete_to_line_start(&mut self) -> (Cursor, String) {
        if self.offset > 0 && self.text().as_bytes()[self.offset - 1] == b'\n' {
            let left = self.left();
            let killed = "\n".to_string();
            let new_cursor = left.modify_text(self, "");
            return (new_cursor, killed);
        }
        let start_cursor = self.start_of_line();
        let killed = self.text()[start_cursor.offset..self.offset].to_string();
        let new_cursor = start_cursor.modify_text(self, "");
        (new_cursor, killed)
    }

    pub fn delete_to_line_end(&mut self) -> (Cursor, String) {
        if self.offset < self.text().len() && self.text().as_bytes()[self.offset] == b'\n' {
            let right = self.right();
            let killed = "\n".to_string();
            let new_cursor = self.modify_text(&right, "");
            return (new_cursor, killed);
        }
        let end_cursor = self.end_of_line();
        let killed = self.text()[self.offset..end_cursor.offset].to_string();
        let new_cursor = self.modify_text(&end_cursor, "");
        (new_cursor, killed)
    }

    pub fn delete_word_before(&mut self) -> (Cursor, String) {
        if self.is_at_start() {
            return (self.clone(), String::new());
        }
        let prev = self.prev_word();
        let target = self.snap_out_of_image_ref(prev.offset, true);
        let prev_cursor = Cursor::new(self.measured_text.clone(), target, 0);
        let killed = self.text()[prev_cursor.offset..self.offset].to_string();
        let new_cursor = prev_cursor.modify_text(self, "");
        (new_cursor, killed)
    }

    pub fn delete_word_after(&mut self) -> Cursor {
        if self.is_at_end() {
            return self.clone();
        }
        let next = self.next_word();
        let target = self.snap_out_of_image_ref(next.offset, false);
        let next_cursor = Cursor::new(self.measured_text.clone(), target, 0);
        self.modify_text(&next_cursor, "")
    }

    pub fn delete_to_logical_line_end(&self) -> Cursor {
        if self.offset < self.text().len() && self.text().as_bytes()[self.offset] == b'\n' {
            let right_cursor = Cursor::new(self.measured_text.clone(), self.offset + 1, 0);
            return self.modify_text(&right_cursor, "");
        }
        let end = self.end_of_logical_line();
        self.modify_text(&end, "")
    }

    /// Delete a token before the cursor if one exists.
    pub fn delete_token_before(&mut self) -> Option<Cursor> {
        let chip_after = self.image_ref_starting_at(self.offset);
        if let Some((_, end)) = chip_after {
            let actual_end = if end < self.text().len() && self.text().as_bytes()[end] == b' ' {
                end + 1
            } else {
                end
            };
            let end_cursor = Cursor::new(self.measured_text.clone(), actual_end, 0);
            return Some(self.modify_text(&end_cursor, ""));
        }

        if self.is_at_start() {
            return None;
        }

        // Only trigger at word boundary
        if self.offset < self.text().len() {
            let ch = &self.text()[self.offset..self.offset + 1];
            if !ch.chars().next().map(|c| c.is_whitespace()).unwrap_or(true) {
                return None;
            }
        }

        let text_before = &self.text()[..self.offset];
        // Check for pasted/truncated text refs
        let paste_re = Regex::new(
            r"(?:^|\s)\[(Pasted text #\d+(?: \+\d+ lines)?|Image #\d+|\.\.\.Truncated text #\d+ \+\d+ lines\.\.\.)\]$",
        )
        .unwrap();

        if let Some(m) = paste_re.find(text_before) {
            let match_start = m.start();
            // Adjust for leading whitespace
            let actual_start = if text_before.as_bytes()[match_start].is_ascii_whitespace() {
                match_start + 1
            } else {
                match_start
            };
            let start_cursor = Cursor::new(self.measured_text.clone(), actual_start, 0);
            return Some(start_cursor.modify_text(self, ""));
        }

        None
    }

    fn grapheme_at(&self, pos: usize) -> String {
        if pos >= self.text().len() {
            return String::new();
        }
        let mut mt = self.measured_text.clone();
        let next = mt.next_offset(pos);
        self.text()[pos..next].to_string()
    }

    fn is_over_whitespace(&self) -> bool {
        if self.offset >= self.text().len() {
            return false;
        }
        self.text()[self.offset..]
            .chars()
            .next()
            .map(|c| c.is_whitespace())
            .unwrap_or(false)
    }

    pub fn is_at_start(&self) -> bool {
        self.offset == 0
    }

    pub fn is_at_end(&self) -> bool {
        self.offset >= self.text().len()
    }

    pub fn start_of_first_line(&self) -> Cursor {
        Cursor::new(self.measured_text.clone(), 0, 0)
    }

    pub fn start_of_last_line(&mut self) -> Cursor {
        match self.text().rfind('\n') {
            Some(pos) => Cursor::new(self.measured_text.clone(), pos + 1, 0),
            None => self.start_of_line(),
        }
    }

    pub fn go_to_line(&self, line_number: usize) -> Cursor {
        let lines: Vec<&str> = self.text().split('\n').collect();
        let target = (line_number.saturating_sub(1)).min(lines.len().saturating_sub(1));
        let mut offset = 0;
        for i in 0..target {
            offset += lines.get(i).map(|l| l.len()).unwrap_or(0) + 1;
        }
        Cursor::new(self.measured_text.clone(), offset, 0)
    }

    pub fn end_of_file(&self) -> Cursor {
        Cursor::new(self.measured_text.clone(), self.text().len(), 0)
    }

    pub fn equals(&self, other: &Cursor) -> bool {
        self.offset == other.offset
    }

    /// Find a character using vim f/F/t/T semantics.
    pub fn find_character(
        &mut self,
        char: &str,
        find_type: VimFindType,
        count: usize,
    ) -> Option<usize> {
        let forward = matches!(find_type, VimFindType::F | VimFindType::T);
        let till = matches!(find_type, VimFindType::T | VimFindType::BigT);
        let mut found = 0;

        if forward {
            let mut pos = self.measured_text.next_offset(self.offset);
            while pos < self.text().len() {
                let grapheme = self.grapheme_at(pos);
                if grapheme == char {
                    found += 1;
                    if found == count {
                        return if till {
                            Some(self.offset.max(self.measured_text.prev_offset(pos)))
                        } else {
                            Some(pos)
                        };
                    }
                }
                pos = self.measured_text.next_offset(pos);
            }
        } else {
            if self.offset == 0 {
                return None;
            }
            let mut pos = self.measured_text.prev_offset(self.offset);
            loop {
                let grapheme = self.grapheme_at(pos);
                if grapheme == char {
                    found += 1;
                    if found == count {
                        return if till {
                            Some(self.offset.min(self.measured_text.next_offset(pos)))
                        } else {
                            Some(pos)
                        };
                    }
                }
                if pos == 0 {
                    break;
                }
                pos = self.measured_text.prev_offset(pos);
            }
        }

        None
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VimFindType {
    F,
    BigF,
    T,
    BigT,
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Simple NFC normalization (using unicode-normalization crate if available,
/// otherwise identity).
fn unicode_normalization_nfc(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfc().collect()
}

/// Simple text wrapping that breaks at column width.
fn wrap_text(text: &str, columns: usize) -> String {
    if columns == 0 {
        return text.to_string();
    }
    let mut result = String::new();
    for line in text.split('\n') {
        if !result.is_empty() {
            result.push('\n');
        }
        let width = UnicodeWidthStr::width(line);
        if width <= columns {
            result.push_str(line);
        } else {
            // Hard wrap
            let mut current_width = 0;
            let mut line_start = true;
            for grapheme in line.graphemes(true) {
                let gw = UnicodeWidthStr::width(grapheme);
                if !line_start && current_width + gw > columns {
                    result.push('\n');
                    current_width = 0;
                }
                result.push_str(grapheme);
                current_width += gw;
                line_start = false;
            }
        }
    }
    result
}
