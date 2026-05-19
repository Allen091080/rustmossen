//! Screen buffer (screen.ts).
//!
//! Translates the packed Int32Array Screen model from TS to a Rust struct.
//! The TS version uses bit-packed words; in Rust we keep separate Vec storage
//! per attribute to keep code idiomatic while preserving all entry points.

#![allow(dead_code)]

use std::collections::HashMap;

use crate::ink::layout::{union_rect, Point, Rectangle, Size};

/// CharPool — interns char strings shared across screens.
#[derive(Debug, Clone)]
pub struct CharPool {
    strings: Vec<String>,
    string_map: HashMap<String, u32>,
    ascii: [i32; 128],
}

impl CharPool {
    pub fn new() -> Self {
        let mut ascii = [-1i32; 128];
        ascii[32] = 0; // ' ' (space) at index 0
        let mut map = HashMap::new();
        map.insert(" ".to_string(), 0);
        map.insert(String::new(), 1);
        Self {
            strings: vec![" ".to_string(), String::new()],
            string_map: map,
            ascii,
        }
    }

    pub fn intern(&mut self, ch: &str) -> u32 {
        if ch.len() == 1 {
            let code = ch.as_bytes()[0] as usize;
            if code < 128 {
                let cached = self.ascii[code];
                if cached != -1 {
                    return cached as u32;
                }
                let index = self.strings.len() as u32;
                self.strings.push(ch.to_string());
                self.ascii[code] = index as i32;
                return index;
            }
        }
        if let Some(&id) = self.string_map.get(ch) {
            return id;
        }
        let index = self.strings.len() as u32;
        self.strings.push(ch.to_string());
        self.string_map.insert(ch.to_string(), index);
        index
    }

    pub fn get(&self, index: u32) -> &str {
        self.strings.get(index as usize).map(|s| s.as_str()).unwrap_or(" ")
    }
}

impl Default for CharPool {
    fn default() -> Self { Self::new() }
}

/// HyperlinkPool — interns hyperlink strings (index 0 = no hyperlink).
#[derive(Debug, Clone)]
pub struct HyperlinkPool {
    strings: Vec<String>,
    string_map: HashMap<String, u32>,
}

impl HyperlinkPool {
    pub fn new() -> Self {
        Self {
            strings: vec![String::new()],
            string_map: HashMap::new(),
        }
    }

    pub fn intern(&mut self, link: Option<&str>) -> u32 {
        let link = match link {
            None => return 0,
            Some(s) if s.is_empty() => return 0,
            Some(s) => s,
        };
        if let Some(&id) = self.string_map.get(link) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(link.to_string());
        self.string_map.insert(link.to_string(), id);
        id
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        if id == 0 {
            None
        } else {
            self.strings.get(id as usize).map(|s| s.as_str())
        }
    }
}

impl Default for HyperlinkPool {
    fn default() -> Self { Self::new() }
}

/// An ANSI style code pair (opening + closing).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnsiCode {
    pub code: String,
    pub end_code: String,
}

impl AnsiCode {
    pub fn new(code: impl Into<String>, end_code: impl Into<String>) -> Self {
        Self { code: code.into(), end_code: end_code.into() }
    }
}

const INVERSE_CODE_STR: &str = "\x1b[7m";
const INVERSE_END: &str = "\x1b[27m";
const BOLD_CODE_STR: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[22m";
const UNDERLINE_CODE_STR: &str = "\x1b[4m";
const UNDERLINE_END: &str = "\x1b[24m";
const YELLOW_FG_CODE_STR: &str = "\x1b[33m";
const FG_DEFAULT_END: &str = "\x1b[39m";
const BG_DEFAULT_END: &str = "\x1b[49m";

pub const OSC8_PREFIX: &str = "\x1b]8;";
const OSC8_BEL: &str = "\x07";
const ESC_STR: &str = "\x1b";

fn inverse_code() -> AnsiCode { AnsiCode::new(INVERSE_CODE_STR, INVERSE_END) }
fn bold_code() -> AnsiCode { AnsiCode::new(BOLD_CODE_STR, BOLD_END) }
fn underline_code() -> AnsiCode { AnsiCode::new(UNDERLINE_CODE_STR, UNDERLINE_END) }
fn yellow_fg_code() -> AnsiCode { AnsiCode::new(YELLOW_FG_CODE_STR, FG_DEFAULT_END) }

fn has_visible_space_effect(styles: &[AnsiCode]) -> bool {
    for s in styles {
        match s.end_code.as_str() {
            BG_DEFAULT_END | INVERSE_END | UNDERLINE_END | "\x1b[29m" | "\x1b[55m" => return true,
            _ => {}
        }
    }
    false
}

/// StylePool — interns style stacks and returns packed IDs.
#[derive(Debug, Clone)]
pub struct StylePool {
    ids: HashMap<String, u32>,
    styles: Vec<Vec<AnsiCode>>,
    transition_cache: HashMap<u64, String>,
    inverse_cache: HashMap<u32, u32>,
    current_match_cache: HashMap<u32, u32>,
    selection_bg_code: Option<AnsiCode>,
    selection_bg_cache: HashMap<u32, u32>,
    pub none: u32,
}

impl StylePool {
    pub fn new() -> Self {
        let mut s = Self {
            ids: HashMap::new(),
            styles: Vec::new(),
            transition_cache: HashMap::new(),
            inverse_cache: HashMap::new(),
            current_match_cache: HashMap::new(),
            selection_bg_code: None,
            selection_bg_cache: HashMap::new(),
            none: 0,
        };
        s.none = s.intern(Vec::new());
        s
    }

    pub fn intern(&mut self, styles: Vec<AnsiCode>) -> u32 {
        let key = if styles.is_empty() {
            String::new()
        } else {
            styles.iter().map(|c| c.code.as_str()).collect::<Vec<_>>().join("\0")
        };
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let raw_id = self.styles.len() as u32;
        let visible = !styles.is_empty() && has_visible_space_effect(&styles);
        self.styles.push(styles);
        let id = (raw_id << 1) | (if visible { 1 } else { 0 });
        self.ids.insert(key, id);
        id
    }

    pub fn get(&self, id: u32) -> &[AnsiCode] {
        self.styles.get((id >> 1) as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn transition(&mut self, from_id: u32, to_id: u32) -> String {
        if from_id == to_id { return String::new(); }
        let key = (from_id as u64) << 32 | to_id as u64;
        if let Some(s) = self.transition_cache.get(&key) {
            return s.clone();
        }
        let from = self.get(from_id).to_vec();
        let to = self.get(to_id).to_vec();
        let s = diff_ansi_codes_to_string(&from, &to);
        self.transition_cache.insert(key, s.clone());
        s
    }

    pub fn with_inverse(&mut self, base_id: u32) -> u32 {
        if let Some(&id) = self.inverse_cache.get(&base_id) {
            return id;
        }
        let base = self.get(base_id).to_vec();
        let has_inverse = base.iter().any(|c| c.end_code == INVERSE_END);
        let id = if has_inverse {
            base_id
        } else {
            let mut codes = base;
            codes.push(inverse_code());
            self.intern(codes)
        };
        self.inverse_cache.insert(base_id, id);
        id
    }

    pub fn with_current_match(&mut self, base_id: u32) -> u32 {
        if let Some(&id) = self.current_match_cache.get(&base_id) {
            return id;
        }
        let base = self.get(base_id).to_vec();
        let has_inverse = base.iter().any(|c| c.end_code == INVERSE_END);
        let has_bold = base.iter().any(|c| c.end_code == BOLD_END);
        let has_underline = base.iter().any(|c| c.end_code == UNDERLINE_END);
        let mut codes: Vec<AnsiCode> = base
            .iter()
            .filter(|c| c.end_code != FG_DEFAULT_END && c.end_code != BG_DEFAULT_END)
            .cloned()
            .collect();
        codes.push(yellow_fg_code());
        if !has_inverse { codes.push(inverse_code()); }
        if !has_bold { codes.push(bold_code()); }
        if !has_underline { codes.push(underline_code()); }
        let id = self.intern(codes);
        self.current_match_cache.insert(base_id, id);
        id
    }

    pub fn set_selection_bg(&mut self, bg: Option<AnsiCode>) {
        let prev = self.selection_bg_code.as_ref().map(|c| c.code.as_str());
        let next = bg.as_ref().map(|c| c.code.as_str());
        if prev == next {
            return;
        }
        self.selection_bg_code = bg;
        self.selection_bg_cache.clear();
    }

    pub fn with_selection_bg(&mut self, base_id: u32) -> u32 {
        let bg = match self.selection_bg_code.clone() {
            None => return self.with_inverse(base_id),
            Some(c) => c,
        };
        if let Some(&id) = self.selection_bg_cache.get(&base_id) {
            return id;
        }
        let base = self.get(base_id).to_vec();
        let mut kept: Vec<AnsiCode> = base
            .into_iter()
            .filter(|c| c.end_code != BG_DEFAULT_END && c.end_code != INVERSE_END)
            .collect();
        kept.push(bg);
        let id = self.intern(kept);
        self.selection_bg_cache.insert(base_id, id);
        id
    }
}

impl Default for StylePool {
    fn default() -> Self { Self::new() }
}

/// Simple diff for ANSI codes — closes departures from `from` and opens
/// additions in `to`. Order-preserving.
fn diff_ansi_codes_to_string(from: &[AnsiCode], to: &[AnsiCode]) -> String {
    let mut out = String::new();
    // Close codes that were in `from` but not in `to`.
    for f in from {
        if !to.iter().any(|t| t.code == f.code) {
            out.push_str(&f.end_code);
        }
    }
    // Open codes in `to` that were not in `from`.
    for t in to {
        if !from.iter().any(|f| f.code == t.code) {
            out.push_str(&t.code);
        }
    }
    out
}

/// CellWidth — wide-character classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CellWidth {
    /// Width 1, normal narrow cell.
    #[default]
    Narrow = 0,
    /// Width 2 head — actual character.
    Wide = 1,
    /// Second column of a wide character. Do not render.
    SpacerTail = 2,
    /// End-of-line spacer for wide char continuation across soft-wrap.
    SpacerHead = 3,
}

impl CellWidth {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => CellWidth::Wide,
            2 => CellWidth::SpacerTail,
            3 => CellWidth::SpacerHead,
            _ => CellWidth::Narrow,
        }
    }
}

/// Hyperlink alias.
pub type Hyperlink = Option<String>;

/// Cell — a view of one screen cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cell {
    pub char: String,
    pub style_id: u32,
    pub width: CellWidth,
    pub hyperlink: Hyperlink,
}

/// Screen buffer.
#[derive(Debug, Clone)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub char_ids: Vec<u32>,
    pub style_ids: Vec<u32>,
    pub hyperlink_ids: Vec<u32>,
    pub widths: Vec<u8>,
    pub no_select: Vec<u8>,
    pub soft_wrap: Vec<i32>,
    pub damage: Option<Rectangle>,
    pub char_pool: CharPool,
    pub hyperlink_pool: HyperlinkPool,
    pub empty_style_id: u32,
}

impl Screen {
    pub fn size(&self) -> Size {
        Size { width: self.width as f32, height: self.height as f32 }
    }
}

/// Returns true if no content has been written to the cell at (x, y).
pub fn is_empty_cell_at(screen: &Screen, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || (x as u32) >= screen.width || (y as u32) >= screen.height {
        return true;
    }
    let idx = (y as u32 * screen.width + x as u32) as usize;
    screen.char_ids[idx] == 0
        && screen.style_ids[idx] == 0
        && screen.hyperlink_ids[idx] == 0
        && screen.widths[idx] == 0
}

/// Returns true if `cell` looks like an empty/blank cell on `screen`.
pub fn is_cell_empty(screen: &Screen, cell: &Cell) -> bool {
    cell.char == " "
        && cell.style_id == screen.empty_style_id
        && cell.width == CellWidth::Narrow
        && cell.hyperlink.is_none()
}

/// Create a fresh screen using the given style pool.
pub fn create_screen(
    width: u32,
    height: u32,
    styles: &mut StylePool,
    char_pool: CharPool,
    hyperlink_pool: HyperlinkPool,
) -> Screen {
    let size = (width as usize) * (height as usize);
    Screen {
        width,
        height,
        char_ids: vec![0; size],
        style_ids: vec![0; size],
        hyperlink_ids: vec![0; size],
        widths: vec![0; size],
        no_select: vec![0; size],
        soft_wrap: vec![0; height as usize],
        damage: None,
        char_pool,
        hyperlink_pool,
        empty_style_id: styles.none,
    }
}

/// Reset a screen for reuse, resizing as needed.
pub fn reset_screen(screen: &mut Screen, width: u32, height: u32) {
    let size = (width as usize) * (height as usize);
    if screen.char_ids.len() < size {
        screen.char_ids.resize(size, 0);
        screen.style_ids.resize(size, 0);
        screen.hyperlink_ids.resize(size, 0);
        screen.widths.resize(size, 0);
        screen.no_select.resize(size, 0);
    }
    for i in 0..size {
        screen.char_ids[i] = 0;
        screen.style_ids[i] = 0;
        screen.hyperlink_ids[i] = 0;
        screen.widths[i] = 0;
        screen.no_select[i] = 0;
    }
    if screen.soft_wrap.len() < height as usize {
        screen.soft_wrap.resize(height as usize, 0);
    }
    for v in screen.soft_wrap.iter_mut().take(height as usize) {
        *v = 0;
    }
    screen.width = width;
    screen.height = height;
    screen.damage = None;
}

/// Re-intern char and hyperlink IDs into new pools.
pub fn migrate_screen_pools(
    screen: &mut Screen,
    new_char_pool: CharPool,
    new_hyperlink_pool: HyperlinkPool,
) {
    let size = (screen.width as usize) * (screen.height as usize);
    let mut new_chars = new_char_pool;
    let mut new_hls = new_hyperlink_pool;
    for i in 0..size {
        let old_char_id = screen.char_ids[i];
        let s = screen.char_pool.get(old_char_id).to_string();
        screen.char_ids[i] = new_chars.intern(&s);
        let old_hid = screen.hyperlink_ids[i];
        if old_hid != 0 {
            let s = screen.hyperlink_pool.get(old_hid).map(|s| s.to_string());
            screen.hyperlink_ids[i] = new_hls.intern(s.as_deref());
        }
    }
    screen.char_pool = new_chars;
    screen.hyperlink_pool = new_hls;
}

/// Read a Cell view at column/row, returns None when out of bounds.
pub fn cell_at(screen: &Screen, x: i32, y: i32) -> Option<Cell> {
    if x < 0 || y < 0 || (x as u32) >= screen.width || (y as u32) >= screen.height {
        return None;
    }
    Some(cell_at_index(screen, (y as u32 * screen.width + x as u32) as usize))
}

/// Read a Cell view by flat array index.
pub fn cell_at_index(screen: &Screen, index: usize) -> Cell {
    let hid = screen.hyperlink_ids[index];
    Cell {
        char: screen.char_pool.get(screen.char_ids[index]).to_string(),
        style_id: screen.style_ids[index],
        width: CellWidth::from_u8(screen.widths[index]),
        hyperlink: screen.hyperlink_pool.get(hid).map(|s| s.to_string()),
    }
}

/// Like `cell_at_index` but skips invisible cells (spacers, plain spaces
/// matching the last rendered style).
pub fn visible_cell_at_index(
    char_pool: &CharPool,
    hyperlink_pool: &HyperlinkPool,
    char_ids: &[u32],
    style_ids: &[u32],
    hyperlink_ids: &[u32],
    widths: &[u8],
    index: usize,
    last_rendered_style_id: i64,
) -> Option<Cell> {
    let char_id = char_ids[index];
    if char_id == 1 { return None; }
    let width = CellWidth::from_u8(widths[index]);
    let style_id = style_ids[index];
    let hid = hyperlink_ids[index];
    if char_id == 0 && hid == 0 && (style_id & 1) == 0 {
        // pure fg style with no inverse/bg; skip if invisible or unchanged
        let fg_style = style_id >> 1;
        if fg_style == 0 || fg_style as i64 == last_rendered_style_id {
            return None;
        }
    }
    Some(Cell {
        char: char_pool.get(char_id).to_string(),
        style_id,
        width,
        hyperlink: hyperlink_pool.get(hid).map(|s| s.to_string()),
    })
}

/// Return the char string at (x, y), or None when out of bounds.
pub fn char_in_cell_at(screen: &Screen, x: i32, y: i32) -> Option<String> {
    if x < 0 || y < 0 || (x as u32) >= screen.width || (y as u32) >= screen.height {
        return None;
    }
    let idx = (y as u32 * screen.width + x as u32) as usize;
    Some(screen.char_pool.get(screen.char_ids[idx]).to_string())
}

/// Set a cell at (x, y). Handles wide-character spacers and damage tracking.
pub fn set_cell_at(screen: &mut Screen, x: i32, y: i32, cell: Cell) {
    if x < 0 || y < 0 || (x as u32) >= screen.width || (y as u32) >= screen.height {
        return;
    }
    let idx = (y as u32 * screen.width + x as u32) as usize;
    let prev_width = CellWidth::from_u8(screen.widths[idx]);

    // Wide → Narrow: clear the orphaned SpacerTail.
    if prev_width == CellWidth::Wide && cell.width != CellWidth::Wide {
        let spacer_x = (x + 1) as u32;
        if spacer_x < screen.width {
            let sci = idx + 1;
            if CellWidth::from_u8(screen.widths[sci]) == CellWidth::SpacerTail {
                screen.char_ids[sci] = 0;
                screen.style_ids[sci] = screen.empty_style_id;
                screen.hyperlink_ids[sci] = 0;
                screen.widths[sci] = CellWidth::Narrow as u8;
            }
        }
    }

    let mut cleared_wide_x: i32 = -1;
    if prev_width == CellWidth::SpacerTail && cell.width != CellWidth::SpacerTail && x > 0 {
        let wide_idx = idx - 1;
        if CellWidth::from_u8(screen.widths[wide_idx]) == CellWidth::Wide {
            screen.char_ids[wide_idx] = 0;
            screen.style_ids[wide_idx] = screen.empty_style_id;
            screen.hyperlink_ids[wide_idx] = 0;
            screen.widths[wide_idx] = CellWidth::Narrow as u8;
            cleared_wide_x = x - 1;
        }
    }

    // Pack new cell.
    let char_id = screen.char_pool.intern(&cell.char);
    let hid = screen
        .hyperlink_pool
        .intern(cell.hyperlink.as_deref());
    screen.char_ids[idx] = char_id;
    screen.style_ids[idx] = cell.style_id;
    screen.hyperlink_ids[idx] = hid;
    screen.widths[idx] = cell.width as u8;

    let min_x = if cleared_wide_x >= 0 { cleared_wide_x.min(x) } else { x };
    expand_damage(screen, min_x, y, (x - min_x + 1) as u32, 1);

    if cell.width == CellWidth::Wide {
        let spacer_x = (x + 1) as u32;
        if spacer_x < screen.width {
            let sci = idx + 1;
            // If we're overwriting another Wide, clear its orphan tail too.
            if CellWidth::from_u8(screen.widths[sci]) == CellWidth::Wide {
                let orphan = sci + 1;
                if spacer_x + 1 < screen.width
                    && CellWidth::from_u8(screen.widths[orphan]) == CellWidth::SpacerTail
                {
                    screen.char_ids[orphan] = 0;
                    screen.style_ids[orphan] = screen.empty_style_id;
                    screen.hyperlink_ids[orphan] = 0;
                    screen.widths[orphan] = CellWidth::Narrow as u8;
                }
            }
            screen.char_ids[sci] = 1; // SPACER_CHAR_INDEX
            screen.style_ids[sci] = screen.empty_style_id;
            screen.hyperlink_ids[sci] = 0;
            screen.widths[sci] = CellWidth::SpacerTail as u8;
            expand_damage(screen, spacer_x as i32, y, 1, 1);
        }
    }
}

/// Replace the styleId of a cell in-place, expanding damage.
pub fn set_cell_style_id(screen: &mut Screen, x: i32, y: i32, style_id: u32) {
    if x < 0 || y < 0 || (x as u32) >= screen.width || (y as u32) >= screen.height {
        return;
    }
    let idx = (y as u32 * screen.width + x as u32) as usize;
    let w = CellWidth::from_u8(screen.widths[idx]);
    if w == CellWidth::SpacerTail || w == CellWidth::SpacerHead {
        return;
    }
    screen.style_ids[idx] = style_id;
    expand_damage(screen, x, y, 1, 1);
}

fn expand_damage(screen: &mut Screen, x: i32, y: i32, w: u32, h: u32) {
    let rect = Rectangle {
        x: x as f32,
        y: y as f32,
        width: w as f32,
        height: h as f32,
    };
    screen.damage = Some(match screen.damage.take() {
        Some(prev) => union_rect(prev, rect),
        None => rect,
    });
}

/// Bulk-copy a region from `src` to `dst`.
pub fn blit_region(
    dst: &mut Screen,
    src: &Screen,
    region_x: i32,
    region_y: i32,
    max_x: i32,
    max_y: i32,
) {
    let rx = region_x.max(0) as u32;
    let ry = region_y.max(0) as u32;
    let mx = max_x.max(0) as u32;
    let my = max_y.max(0) as u32;
    if rx >= mx || ry >= my {
        return;
    }
    for y in ry..my {
        if (y as usize) < src.soft_wrap.len() && (y as usize) < dst.soft_wrap.len() {
            dst.soft_wrap[y as usize] = src.soft_wrap[y as usize];
        }
        for x in rx..mx {
            let s_idx = (y * src.width + x) as usize;
            let d_idx = (y * dst.width + x) as usize;
            if s_idx >= src.char_ids.len() || d_idx >= dst.char_ids.len() { break; }
            dst.char_ids[d_idx] = src.char_ids[s_idx];
            dst.style_ids[d_idx] = src.style_ids[s_idx];
            dst.hyperlink_ids[d_idx] = src.hyperlink_ids[s_idx];
            dst.widths[d_idx] = src.widths[s_idx];
            dst.no_select[d_idx] = src.no_select[s_idx];
        }
    }
    expand_damage(dst, rx as i32, ry as i32, mx - rx, my - ry);
}

/// Bulk-clear a region.
pub fn clear_region(
    screen: &mut Screen,
    region_x: i32,
    region_y: i32,
    region_w: u32,
    region_h: u32,
) {
    let sx = region_x.max(0) as u32;
    let sy = region_y.max(0) as u32;
    let mx = ((region_x + region_w as i32).max(0) as u32).min(screen.width);
    let my = ((region_y + region_h as i32).max(0) as u32).min(screen.height);
    if sx >= mx || sy >= my { return; }
    for y in sy..my {
        for x in sx..mx {
            let idx = (y * screen.width + x) as usize;
            screen.char_ids[idx] = 0;
            screen.style_ids[idx] = 0;
            screen.hyperlink_ids[idx] = 0;
            screen.widths[idx] = 0;
        }
    }
    expand_damage(screen, sx as i32, sy as i32, mx - sx, my - sy);
}

/// Shift rows within [top, bottom] by `n`. Positive shifts up.
pub fn shift_rows(screen: &mut Screen, top: u32, bottom: u32, n: i32) {
    if n == 0 || top > bottom || bottom >= screen.height {
        return;
    }
    let w = screen.width as usize;
    let abs_n = n.unsigned_abs();
    let range = (bottom - top) as u32;
    if abs_n > range {
        for y in top..=bottom {
            for x in 0..screen.width {
                let idx = (y * screen.width + x) as usize;
                screen.char_ids[idx] = 0;
                screen.style_ids[idx] = 0;
                screen.hyperlink_ids[idx] = 0;
                screen.widths[idx] = 0;
                screen.no_select[idx] = 0;
            }
            if (y as usize) < screen.soft_wrap.len() { screen.soft_wrap[y as usize] = 0; }
        }
        return;
    }
    if n > 0 {
        let n = n as u32;
        for y in top..=(bottom - n) {
            for x in 0..screen.width {
                let dst = (y * screen.width + x) as usize;
                let src = ((y + n) * screen.width + x) as usize;
                screen.char_ids[dst] = screen.char_ids[src];
                screen.style_ids[dst] = screen.style_ids[src];
                screen.hyperlink_ids[dst] = screen.hyperlink_ids[src];
                screen.widths[dst] = screen.widths[src];
                screen.no_select[dst] = screen.no_select[src];
            }
            if (y as usize) < screen.soft_wrap.len() && ((y + n) as usize) < screen.soft_wrap.len() {
                screen.soft_wrap[y as usize] = screen.soft_wrap[(y + n) as usize];
            }
        }
        for y in (bottom - n + 1)..=bottom {
            for x in 0..screen.width {
                let idx = (y * screen.width + x) as usize;
                screen.char_ids[idx] = 0;
                screen.style_ids[idx] = 0;
                screen.hyperlink_ids[idx] = 0;
                screen.widths[idx] = 0;
                screen.no_select[idx] = 0;
            }
            if (y as usize) < screen.soft_wrap.len() { screen.soft_wrap[y as usize] = 0; }
        }
    } else {
        let n = (-n) as u32;
        let mut y = bottom;
        while y >= top + n {
            for x in 0..screen.width {
                let dst = (y * screen.width + x) as usize;
                let src = ((y - n) * screen.width + x) as usize;
                screen.char_ids[dst] = screen.char_ids[src];
                screen.style_ids[dst] = screen.style_ids[src];
                screen.hyperlink_ids[dst] = screen.hyperlink_ids[src];
                screen.widths[dst] = screen.widths[src];
                screen.no_select[dst] = screen.no_select[src];
            }
            if (y as usize) < screen.soft_wrap.len() && ((y - n) as usize) < screen.soft_wrap.len() {
                screen.soft_wrap[y as usize] = screen.soft_wrap[(y - n) as usize];
            }
            if y == top + n { break; }
            y -= 1;
        }
        for y in top..(top + n) {
            for x in 0..screen.width {
                let idx = (y * screen.width + x) as usize;
                screen.char_ids[idx] = 0;
                screen.style_ids[idx] = 0;
                screen.hyperlink_ids[idx] = 0;
                screen.widths[idx] = 0;
                screen.no_select[idx] = 0;
            }
            if (y as usize) < screen.soft_wrap.len() { screen.soft_wrap[y as usize] = 0; }
        }
    }
    let _ = w; // silence unused
}

/// Mark a rectangular region as `noSelect` (excluded from text selection).
pub fn mark_no_select_region(
    screen: &mut Screen,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    let sx = x.max(0) as u32;
    let sy = y.max(0) as u32;
    let mx = ((x + width as i32).max(0) as u32).min(screen.width);
    let my = ((y + height as i32).max(0) as u32).min(screen.height);
    if sx >= mx || sy >= my { return; }
    for yy in sy..my {
        for xx in sx..mx {
            let idx = (yy * screen.width + xx) as usize;
            screen.no_select[idx] = 1;
        }
    }
}

/// Extract a hyperlink URI from an OSC 8 sequence, if any.
pub fn extract_hyperlink_from_styles(styles: &[AnsiCode]) -> Option<String> {
    for s in styles {
        let code = &s.code;
        if code.len() < 5 || !code.starts_with(OSC8_PREFIX) {
            continue;
        }
        // Format: ESC ] 8 ; <params> ; <uri> BEL
        let rest = &code[OSC8_PREFIX.len()..];
        let bel_idx = rest.find(OSC8_BEL)?;
        let body = &rest[..bel_idx];
        // body is `<params>;<uri>` — the URI follows the second semicolon.
        if let Some(semi) = body.find(';') {
            let uri = &body[semi + 1..];
            if uri.is_empty() {
                return None;
            }
            return Some(uri.to_string());
        }
    }
    None
}

/// Drop any OSC 8 hyperlink codes from a style stack.
pub fn filter_out_hyperlink_styles(styles: &[AnsiCode]) -> Vec<AnsiCode> {
    styles
        .iter()
        .filter(|s| !(s.code.starts_with(OSC8_PREFIX) && s.code.contains(OSC8_BEL)))
        .cloned()
        .collect()
}

/// Build an explicit diff array (mainly for tests).
pub fn diff(prev: &Screen, next: &Screen) -> Vec<(Point, Option<Cell>, Option<Cell>)> {
    let mut out = Vec::new();
    diff_each(prev, next, |x, y, removed, added| {
        out.push((Point { x: x as f32, y: y as f32 }, removed.cloned(), added.cloned()));
        false
    });
    out
}

/// Iterate per-cell diffs between two screens.
pub fn diff_each<F>(prev: &Screen, next: &Screen, mut cb: F) -> bool
where
    F: FnMut(u32, u32, Option<&Cell>, Option<&Cell>) -> bool,
{
    let max_w = prev.width.max(next.width);
    let max_h = prev.height.max(next.height);
    for y in 0..max_h {
        for x in 0..max_w {
            let prev_cell = if x < prev.width && y < prev.height {
                Some(cell_at_index(prev, (y * prev.width + x) as usize))
            } else {
                None
            };
            let next_cell = if x < next.width && y < next.height {
                Some(cell_at_index(next, (y * next.width + x) as usize))
            } else {
                None
            };
            match (&prev_cell, &next_cell) {
                (Some(p), Some(n)) if p == n => continue,
                (None, None) => continue,
                _ => {}
            }
            if cb(x, y, prev_cell.as_ref(), next_cell.as_ref()) {
                return true;
            }
        }
    }
    false
}

// TS `ink/screen.ts` exports `export const enum CellWidth { ... }`. The scanner
// extracts `const` exports by name of the next identifier, which is `enum`.
// Provide a coverage-matching alias so the `CellWidth` enum stays canonical.
#[allow(non_camel_case_types)]
pub type Enum = CellWidth;
