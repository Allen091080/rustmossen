//! Selection (selection.ts).
//!
//! Text selection state for fullscreen TUI mode. Tracks a linear selection in
//! screen-buffer coordinates with anchor/focus + scrolled-off-row accumulators.

#![allow(dead_code)]

use crate::ink::screen::{
    cell_at, cell_at_index, set_cell_style_id, CellWidth, Screen, StylePool,
};

/// Coordinate in screen-buffer cells (col/row, 0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub col: i32,
    pub row: i32,
}

/// Keyboard focus moves consumed by `moveFocus` callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMove {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
}

/// Kind of multi-click anchor span (word vs. line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSpanKind {
    Word,
    Line,
}

/// Anchor span captured by a double/triple click.
#[derive(Debug, Clone, Copy)]
pub struct AnchorSpan {
    pub lo: Point,
    pub hi: Point,
    pub kind: AnchorSpanKind,
}

/// Selection state machine.
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub anchor: Option<Point>,
    pub focus: Option<Point>,
    pub is_dragging: bool,
    pub anchor_span: Option<AnchorSpan>,
    pub scrolled_off_above: Vec<String>,
    pub scrolled_off_below: Vec<String>,
    pub scrolled_off_above_sw: Vec<bool>,
    pub scrolled_off_below_sw: Vec<bool>,
    pub virtual_anchor_row: Option<i32>,
    pub virtual_focus_row: Option<i32>,
    pub last_press_had_alt: bool,
}

pub fn create_selection_state() -> SelectionState {
    SelectionState::default()
}

pub fn start_selection(s: &mut SelectionState, col: i32, row: i32) {
    s.anchor = Some(Point { col, row });
    s.focus = None;
    s.is_dragging = true;
    s.anchor_span = None;
    s.scrolled_off_above.clear();
    s.scrolled_off_below.clear();
    s.scrolled_off_above_sw.clear();
    s.scrolled_off_below_sw.clear();
    s.virtual_anchor_row = None;
    s.virtual_focus_row = None;
    s.last_press_had_alt = false;
}

pub fn update_selection(s: &mut SelectionState, col: i32, row: i32) {
    if !s.is_dragging {
        return;
    }
    if s.focus.is_none()
        && s.anchor.map_or(false, |a| a.col == col && a.row == row)
    {
        return;
    }
    s.focus = Some(Point { col, row });
}

pub fn finish_selection(s: &mut SelectionState) {
    s.is_dragging = false;
}

pub fn clear_selection(s: &mut SelectionState) {
    *s = SelectionState::default();
}

pub fn has_selection(s: &SelectionState) -> bool {
    s.anchor.is_some() && s.focus.is_some()
}

#[derive(Debug, Clone, Copy)]
pub struct SelectionRange {
    pub start: Point,
    pub end: Point,
}

fn compare_points(a: Point, b: Point) -> i32 {
    if a.row != b.row {
        if a.row < b.row { -1 } else { 1 }
    } else if a.col != b.col {
        if a.col < b.col { -1 } else { 1 }
    } else {
        0
    }
}

pub fn selection_bounds(s: &SelectionState) -> Option<SelectionRange> {
    let a = s.anchor?;
    let f = s.focus?;
    if compare_points(a, f) <= 0 {
        Some(SelectionRange { start: a, end: f })
    } else {
        Some(SelectionRange { start: f, end: a })
    }
}

pub fn is_cell_selected(s: &SelectionState, col: i32, row: i32) -> bool {
    let Some(b) = selection_bounds(s) else { return false; };
    if row < b.start.row || row > b.end.row {
        return false;
    }
    if row == b.start.row && col < b.start.col {
        return false;
    }
    if row == b.end.row && col > b.end.col {
        return false;
    }
    true
}

fn char_class(c: &str) -> u8 {
    if c == " " || c.is_empty() {
        return 0;
    }
    let first = c.chars().next().unwrap();
    if first.is_alphanumeric() || matches!(first, '_' | '/' | '.' | '-' | '+' | '~' | '\\') {
        1
    } else {
        2
    }
}

fn word_bounds_at(screen: &Screen, col: i32, row: i32) -> Option<(i32, i32)> {
    if row < 0 || (row as u32) >= screen.height {
        return None;
    }
    let width = screen.width as i32;
    let mut c = col;
    if c > 0 {
        if let Some(cell) = cell_at(screen, c, row) {
            if cell.width == CellWidth::SpacerTail {
                c -= 1;
            }
        }
    }
    if c < 0 || c >= width {
        return None;
    }
    let row_off = (row as u32 * screen.width) as usize;
    if screen.no_select[row_off + c as usize] == 1 {
        return None;
    }
    let start_cell = cell_at(screen, c, row)?;
    let cls = char_class(&start_cell.char);

    let mut lo = c;
    while lo > 0 {
        let prev = lo - 1;
        if screen.no_select[row_off + prev as usize] == 1 {
            break;
        }
        let Some(pc) = cell_at(screen, prev, row) else { break; };
        if pc.width == CellWidth::SpacerTail {
            if prev == 0 || screen.no_select[row_off + (prev - 1) as usize] == 1 {
                break;
            }
            let Some(head) = cell_at(screen, prev - 1, row) else { break; };
            if char_class(&head.char) != cls {
                break;
            }
            lo = prev - 1;
            continue;
        }
        if char_class(&pc.char) != cls {
            break;
        }
        lo = prev;
    }

    let mut hi = c;
    while hi < width - 1 {
        let next = hi + 1;
        if screen.no_select[row_off + next as usize] == 1 {
            break;
        }
        let Some(nc) = cell_at(screen, next, row) else { break; };
        if nc.width == CellWidth::SpacerTail {
            hi = next;
            continue;
        }
        if char_class(&nc.char) != cls {
            break;
        }
        hi = next;
    }

    Some((lo, hi))
}

pub fn select_word_at(s: &mut SelectionState, screen: &Screen, col: i32, row: i32) {
    let Some((lo, hi)) = word_bounds_at(screen, col, row) else { return; };
    let lo_p = Point { col: lo, row };
    let hi_p = Point { col: hi, row };
    s.anchor = Some(lo_p);
    s.focus = Some(hi_p);
    s.is_dragging = true;
    s.anchor_span = Some(AnchorSpan { lo: lo_p, hi: hi_p, kind: AnchorSpanKind::Word });
}

pub fn select_line_at(s: &mut SelectionState, screen: &Screen, row: i32) {
    if row < 0 || (row as u32) >= screen.height {
        return;
    }
    let lo = Point { col: 0, row };
    let hi = Point { col: (screen.width as i32) - 1, row };
    s.anchor = Some(lo);
    s.focus = Some(hi);
    s.is_dragging = true;
    s.anchor_span = Some(AnchorSpan { lo, hi, kind: AnchorSpanKind::Line });
}

fn is_url_char(c: &str) -> bool {
    if c.len() != 1 {
        return false;
    }
    let b = c.as_bytes()[0];
    if !(0x21..=0x7e).contains(&b) {
        return false;
    }
    !matches!(b, b'<' | b'>' | b'"' | b'\'' | b'`' | b' ')
}

pub fn find_plain_text_url_at(screen: &Screen, col: i32, row: i32) -> Option<String> {
    if row < 0 || (row as u32) >= screen.height {
        return None;
    }
    let width = screen.width as i32;
    let row_off = (row as u32 * screen.width) as usize;
    let mut c = col;
    if c > 0 {
        if let Some(cell) = cell_at(screen, c, row) {
            if cell.width == CellWidth::SpacerTail {
                c -= 1;
            }
        }
    }
    if c < 0 || c >= width || screen.no_select[row_off + c as usize] == 1 {
        return None;
    }
    let start_cell = cell_at(screen, c, row)?;
    if !is_url_char(&start_cell.char) {
        return None;
    }

    let mut lo = c;
    while lo > 0 {
        let prev = lo - 1;
        if screen.no_select[row_off + prev as usize] == 1 {
            break;
        }
        let Some(pc) = cell_at(screen, prev, row) else { break; };
        if pc.width != CellWidth::Narrow || !is_url_char(&pc.char) {
            break;
        }
        lo = prev;
    }
    let mut hi = c;
    while hi < width - 1 {
        let next = hi + 1;
        if screen.no_select[row_off + next as usize] == 1 {
            break;
        }
        let Some(nc) = cell_at(screen, next, row) else { break; };
        if nc.width != CellWidth::Narrow || !is_url_char(&nc.char) {
            break;
        }
        hi = next;
    }

    let mut token = String::new();
    for i in lo..=hi {
        token.push_str(&cell_at(screen, i, row)?.char);
    }

    let click_idx = (c - lo) as usize;
    let bytes = token.as_bytes();
    let mut url_start: i32 = -1;
    let mut url_end: usize = token.len();
    let mut i = 0;
    let schemes = [&b"https://"[..], &b"http://"[..], &b"file://"[..]];
    while i < bytes.len() {
        let mut matched = false;
        for s in &schemes {
            if i + s.len() <= bytes.len() && &bytes[i..i + s.len()] == *s {
                if i > click_idx {
                    url_end = i;
                    i = bytes.len();
                    matched = true;
                    break;
                }
                url_start = i as i32;
                matched = true;
                i += s.len();
                break;
            }
        }
        if !matched {
            i += 1;
        }
    }
    if url_start < 0 {
        return None;
    }
    let mut url = token[url_start as usize..url_end].to_string();
    let openers = [(')', '('), (']', '['), ('}', '{')];
    while !url.is_empty() {
        let last = url.chars().last().unwrap();
        if ".,;:!?".contains(last) {
            url.pop();
            continue;
        }
        let opener = openers.iter().find(|(c, _)| *c == last).map(|(_, o)| *o);
        let Some(opener) = opener else { break; };
        let opens = url.chars().filter(|c| *c == opener).count();
        let closes = url.chars().filter(|c| *c == last).count();
        if closes > opens {
            url.pop();
        } else {
            break;
        }
    }
    if click_idx >= url_start as usize + url.len() {
        return None;
    }
    Some(url)
}

pub fn extend_selection(s: &mut SelectionState, screen: &Screen, col: i32, row: i32) {
    if !s.is_dragging || s.anchor_span.is_none() {
        return;
    }
    let span = s.anchor_span.unwrap();
    let (m_lo, m_hi) = match span.kind {
        AnchorSpanKind::Word => {
            let bounds = word_bounds_at(screen, col, row);
            let (lo, hi) = bounds.unwrap_or((col, col));
            (Point { col: lo, row }, Point { col: hi, row })
        }
        AnchorSpanKind::Line => {
            let r = row.max(0).min((screen.height as i32) - 1);
            (Point { col: 0, row: r }, Point { col: (screen.width as i32) - 1, row: r })
        }
    };
    if compare_points(m_hi, span.lo) < 0 {
        s.anchor = Some(span.hi);
        s.focus = Some(m_lo);
    } else if compare_points(m_lo, span.hi) > 0 {
        s.anchor = Some(span.lo);
        s.focus = Some(m_hi);
    } else {
        s.anchor = Some(span.lo);
        s.focus = Some(span.hi);
    }
}

pub fn move_focus(s: &mut SelectionState, col: i32, row: i32) {
    if s.focus.is_none() {
        return;
    }
    s.anchor_span = None;
    s.focus = Some(Point { col, row });
    s.virtual_focus_row = None;
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi)
}

pub fn shift_selection(
    s: &mut SelectionState,
    d_row: i32,
    min_row: i32,
    max_row: i32,
    width: i32,
) {
    let Some(anchor) = s.anchor else { return; };
    let Some(focus) = s.focus else { return; };
    let v_anchor = s.virtual_anchor_row.unwrap_or(anchor.row) + d_row;
    let v_focus = s.virtual_focus_row.unwrap_or(focus.row) + d_row;
    if (v_anchor < min_row && v_focus < min_row)
        || (v_anchor > max_row && v_focus > max_row)
    {
        clear_selection(s);
        return;
    }
    let old_min = s
        .virtual_anchor_row
        .unwrap_or(anchor.row)
        .min(s.virtual_focus_row.unwrap_or(focus.row));
    let old_max = s
        .virtual_anchor_row
        .unwrap_or(anchor.row)
        .max(s.virtual_focus_row.unwrap_or(focus.row));
    let old_above_debt = (min_row - old_min).max(0);
    let old_below_debt = (old_max - max_row).max(0);
    let new_above_debt = (min_row - v_anchor.min(v_focus)).max(0);
    let new_below_debt = (v_anchor.max(v_focus) - max_row).max(0);
    if new_above_debt < old_above_debt {
        let drop = (old_above_debt - new_above_debt) as usize;
        let len = s.scrolled_off_above.len().saturating_sub(drop);
        s.scrolled_off_above.truncate(len);
        s.scrolled_off_above_sw.truncate(len);
    }
    if new_below_debt < old_below_debt {
        let drop = (old_below_debt - new_below_debt) as usize;
        let drop = drop.min(s.scrolled_off_below.len());
        s.scrolled_off_below.drain(0..drop);
        s.scrolled_off_below_sw.drain(0..drop);
    }
    if (s.scrolled_off_above.len() as i32) > new_above_debt {
        let keep = new_above_debt.max(0) as usize;
        let drop = s.scrolled_off_above.len() - keep;
        s.scrolled_off_above.drain(0..drop);
        s.scrolled_off_above_sw.drain(0..drop);
    }
    if (s.scrolled_off_below.len() as i32) > new_below_debt {
        let keep = new_below_debt.max(0) as usize;
        s.scrolled_off_below.truncate(keep);
        s.scrolled_off_below_sw.truncate(keep);
    }
    let shift_p = |p: Point, v_row: i32| -> Point {
        if v_row < min_row {
            Point { col: 0, row: min_row }
        } else if v_row > max_row {
            Point { col: width - 1, row: max_row }
        } else {
            Point { col: p.col, row: v_row }
        }
    };
    s.anchor = Some(shift_p(anchor, v_anchor));
    s.focus = Some(shift_p(focus, v_focus));
    s.virtual_anchor_row = if v_anchor < min_row || v_anchor > max_row { Some(v_anchor) } else { None };
    s.virtual_focus_row = if v_focus < min_row || v_focus > max_row { Some(v_focus) } else { None };
    if let Some(span) = s.anchor_span {
        let sp = |p: Point| -> Point {
            let r = p.row + d_row;
            if r < min_row { Point { col: 0, row: min_row } }
            else if r > max_row { Point { col: width - 1, row: max_row } }
            else { Point { col: p.col, row: r } }
        };
        s.anchor_span = Some(AnchorSpan { lo: sp(span.lo), hi: sp(span.hi), kind: span.kind });
    }
}

pub fn shift_anchor(s: &mut SelectionState, d_row: i32, min_row: i32, max_row: i32) {
    let Some(anchor) = s.anchor else { return; };
    let raw = s.virtual_anchor_row.unwrap_or(anchor.row) + d_row;
    s.anchor = Some(Point { col: anchor.col, row: clamp(raw, min_row, max_row) });
    s.virtual_anchor_row = if raw < min_row || raw > max_row { Some(raw) } else { None };
    if let Some(span) = s.anchor_span {
        let shift = |p: Point| -> Point {
            Point { col: p.col, row: clamp(p.row + d_row, min_row, max_row) }
        };
        s.anchor_span = Some(AnchorSpan { lo: shift(span.lo), hi: shift(span.hi), kind: span.kind });
    }
}

pub fn shift_selection_for_follow(
    s: &mut SelectionState,
    d_row: i32,
    min_row: i32,
    max_row: i32,
) -> bool {
    let Some(anchor) = s.anchor else { return false; };
    let raw_anchor = s.virtual_anchor_row.unwrap_or(anchor.row) + d_row;
    let raw_focus = s.focus.map(|f| s.virtual_focus_row.unwrap_or(f.row) + d_row);
    if let Some(rf) = raw_focus {
        if raw_anchor < min_row && rf < min_row {
            clear_selection(s);
            return true;
        }
    }
    s.anchor = Some(Point { col: anchor.col, row: clamp(raw_anchor, min_row, max_row) });
    if let (Some(f), Some(rf)) = (s.focus, raw_focus) {
        s.focus = Some(Point { col: f.col, row: clamp(rf, min_row, max_row) });
    }
    s.virtual_anchor_row =
        if raw_anchor < min_row || raw_anchor > max_row { Some(raw_anchor) } else { None };
    s.virtual_focus_row = match raw_focus {
        Some(rf) if rf < min_row || rf > max_row => Some(rf),
        _ => None,
    };
    if let Some(span) = s.anchor_span {
        let shift = |p: Point| -> Point {
            Point { col: p.col, row: clamp(p.row + d_row, min_row, max_row) }
        };
        s.anchor_span = Some(AnchorSpan { lo: shift(span.lo), hi: shift(span.hi), kind: span.kind });
    }
    false
}

fn extract_row_text(screen: &Screen, row: i32, col_start: i32, col_end: i32) -> String {
    let row_off = (row as u32 * screen.width) as usize;
    let content_end = if (row + 1) < screen.height as i32 {
        screen.soft_wrap[(row + 1) as usize]
    } else {
        0
    };
    let last_col = if content_end > 0 {
        col_end.min(content_end - 1)
    } else {
        col_end
    };
    let mut line = String::new();
    let mut col = col_start;
    while col <= last_col {
        if screen.no_select[row_off + col as usize] == 1 {
            col += 1;
            continue;
        }
        if let Some(cell) = cell_at(screen, col, row) {
            if cell.width == CellWidth::SpacerTail || cell.width == CellWidth::SpacerHead {
                col += 1;
                continue;
            }
            line.push_str(&cell.char);
        }
        col += 1;
    }
    if content_end > 0 {
        line
    } else {
        line.trim_end().to_string()
    }
}

fn join_rows(lines: &mut Vec<String>, text: String, sw: bool) {
    if sw && !lines.is_empty() {
        let last = lines.last_mut().unwrap();
        last.push_str(&text);
    } else {
        lines.push(text);
    }
}

pub fn get_selected_text(s: &SelectionState, screen: &Screen) -> String {
    let Some(b) = selection_bounds(s) else { return String::new(); };
    let mut lines: Vec<String> = Vec::new();
    for (i, row_text) in s.scrolled_off_above.iter().enumerate() {
        let sw = s.scrolled_off_above_sw.get(i).copied().unwrap_or(false);
        join_rows(&mut lines, row_text.clone(), sw);
    }
    let mut row = b.start.row;
    while row <= b.end.row {
        let row_start = if row == b.start.row { b.start.col } else { 0 };
        let row_end = if row == b.end.row { b.end.col } else { (screen.width as i32) - 1 };
        let sw_bit = screen.soft_wrap.get(row as usize).copied().unwrap_or(0) > 0;
        join_rows(&mut lines, extract_row_text(screen, row, row_start, row_end), sw_bit);
        row += 1;
    }
    for (i, row_text) in s.scrolled_off_below.iter().enumerate() {
        let sw = s.scrolled_off_below_sw.get(i).copied().unwrap_or(false);
        join_rows(&mut lines, row_text.clone(), sw);
    }
    lines.join("\n")
}

/// Side argument for capture_scrolled_rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOffSide {
    Above,
    Below,
}

pub fn capture_scrolled_rows(
    s: &mut SelectionState,
    screen: &Screen,
    first_row: i32,
    last_row: i32,
    side: ScrollOffSide,
) {
    let Some(b) = selection_bounds(s) else { return; };
    if first_row > last_row {
        return;
    }
    let lo = first_row.max(b.start.row);
    let hi = last_row.min(b.end.row);
    if lo > hi {
        return;
    }
    let width = screen.width as i32;
    let mut captured: Vec<String> = Vec::new();
    let mut captured_sw: Vec<bool> = Vec::new();
    for row in lo..=hi {
        let col_start = if row == b.start.row { b.start.col } else { 0 };
        let col_end = if row == b.end.row { b.end.col } else { width - 1 };
        captured.push(extract_row_text(screen, row, col_start, col_end));
        captured_sw.push(screen.soft_wrap.get(row as usize).copied().unwrap_or(0) > 0);
    }
    match side {
        ScrollOffSide::Above => {
            s.scrolled_off_above.extend(captured);
            s.scrolled_off_above_sw.extend(captured_sw);
            if s.anchor.map_or(false, |a| a.row == b.start.row) && lo == b.start.row {
                if let Some(a) = s.anchor.as_mut() {
                    a.col = 0;
                }
                if let Some(span) = s.anchor_span.as_mut() {
                    span.lo.col = 0;
                    span.hi.col = width - 1;
                }
            }
        }
        ScrollOffSide::Below => {
            for (i, row_text) in captured.into_iter().enumerate() {
                s.scrolled_off_below.insert(i, row_text);
            }
            for (i, sw) in captured_sw.into_iter().enumerate() {
                s.scrolled_off_below_sw.insert(i, sw);
            }
            if s.anchor.map_or(false, |a| a.row == b.end.row) && hi == b.end.row {
                if let Some(a) = s.anchor.as_mut() {
                    a.col = width - 1;
                }
                if let Some(span) = s.anchor_span.as_mut() {
                    span.lo.col = 0;
                    span.hi.col = width - 1;
                }
            }
        }
    }
}

pub fn apply_selection_overlay(
    screen: &mut Screen,
    selection: &SelectionState,
    style_pool: &mut StylePool,
) {
    let Some(b) = selection_bounds(selection) else { return; };
    let width = screen.width as i32;
    let height = screen.height as i32;
    let mut row = b.start.row;
    while row <= b.end.row && row < height {
        let col_start = if row == b.start.row { b.start.col } else { 0 };
        let col_end_raw = if row == b.end.row { b.end.col.min(width - 1) } else { width - 1 };
        let row_off = (row as u32 * screen.width) as usize;
        let mut col = col_start;
        while col <= col_end_raw {
            let idx = row_off + col as usize;
            if screen.no_select[idx] == 1 {
                col += 1;
                continue;
            }
            let cell = cell_at_index(screen, idx);
            let new_style = style_pool.with_selection_bg(cell.style_id);
            set_cell_style_id(screen, col, row, new_style);
            col += 1;
        }
        row += 1;
    }
}
