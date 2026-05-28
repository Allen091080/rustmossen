// vim.rs — Translation of vim/ directory:
// vim/types.ts, vim/motions.ts, vim/textObjects.ts, vim/operators.ts, vim/transitions.ts

use std::collections::{HashMap, HashSet};

// ============================================================================
// types.ts — Core Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindType {
    F,
    BigF,
    T,
    BigT,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjScope {
    Inner,
    Around,
}

#[derive(Debug, Clone)]
pub enum VimState {
    Insert { inserted_text: String },
    Normal { command: CommandState },
}

#[derive(Debug, Clone)]
pub enum CommandState {
    Idle,
    Count {
        digits: String,
    },
    OperatorPending {
        op: Operator,
        count: usize,
    },
    OperatorCount {
        op: Operator,
        count: usize,
        digits: String,
    },
    OperatorFind {
        op: Operator,
        count: usize,
        find: FindType,
    },
    OperatorTextObj {
        op: Operator,
        count: usize,
        scope: TextObjScope,
    },
    Find {
        find: FindType,
        count: usize,
    },
    G {
        count: usize,
    },
    OperatorG {
        op: Operator,
        count: usize,
    },
    Replace {
        count: usize,
    },
    Indent {
        dir: char,
        count: usize,
    },
}

#[derive(Debug, Clone)]
pub struct PersistentState {
    pub last_change: Option<RecordedChange>,
    pub last_find: Option<LastFind>,
    pub register: String,
    pub register_is_linewise: bool,
}

#[derive(Debug, Clone)]
pub struct LastFind {
    pub find_type: FindType,
    pub ch: String,
}

#[derive(Debug, Clone)]
pub enum RecordedChange {
    Insert {
        text: String,
    },
    OperatorMotion {
        op: Operator,
        motion: String,
        count: usize,
    },
    OperatorTextObj {
        op: Operator,
        obj_type: String,
        scope: TextObjScope,
        count: usize,
    },
    OperatorFind {
        op: Operator,
        find: FindType,
        ch: String,
        count: usize,
    },
    ReplaceChar {
        ch: String,
        count: usize,
    },
    X {
        count: usize,
    },
    ToggleCase {
        count: usize,
    },
    IndentChange {
        dir: char,
        count: usize,
    },
    OpenLine {
        direction: OpenDirection,
    },
    Join {
        count: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDirection {
    Above,
    Below,
}

pub fn operator_from_key(key: &str) -> Option<Operator> {
    match key {
        "d" => Some(Operator::Delete),
        "c" => Some(Operator::Change),
        "y" => Some(Operator::Yank),
        _ => None,
    }
}

pub fn is_operator_key(key: &str) -> bool {
    matches!(key, "d" | "c" | "y")
}

pub fn simple_motions() -> HashSet<&'static str> {
    [
        "h", "l", "j", "k", "w", "b", "e", "W", "B", "E", "0", "^", "$",
    ]
    .into_iter()
    .collect()
}

pub fn find_keys() -> HashSet<&'static str> {
    ["f", "F", "t", "T"].into_iter().collect()
}

pub fn text_obj_scope_from_key(key: &str) -> Option<TextObjScope> {
    match key {
        "i" => Some(TextObjScope::Inner),
        "a" => Some(TextObjScope::Around),
        _ => None,
    }
}

pub fn is_text_obj_scope_key(key: &str) -> bool {
    matches!(key, "i" | "a")
}

pub fn text_obj_types() -> HashSet<&'static str> {
    [
        "w", "W", "\"", "'", "`", "(", ")", "b", "[", "]", "{", "}", "B", "<", ">",
    ]
    .into_iter()
    .collect()
}

pub const MAX_VIM_COUNT: usize = 10000;

pub fn create_initial_vim_state() -> VimState {
    VimState::Insert {
        inserted_text: String::new(),
    }
}

pub fn create_initial_persistent_state() -> PersistentState {
    PersistentState {
        last_change: None,
        last_find: None,
        register: String::new(),
        register_is_linewise: false,
    }
}

// ============================================================================
// motions.ts — Motion Functions
// ============================================================================

/// Position in text (simplified from Cursor)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPos {
    pub offset: usize,
}

/// Resolve a motion to a target cursor position. Pure calculation.
pub fn resolve_motion(key: &str, text: &str, offset: usize, count: usize) -> usize {
    let mut result = offset;
    for _ in 0..count {
        let next = apply_single_motion(key, text, result);
        if next == result {
            break;
        }
        result = next;
    }
    result
}

fn apply_single_motion(key: &str, text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    match key {
        "h" => move_left(&chars, offset),
        "l" => move_right(&chars, offset, len),
        "j" => move_down_logical(&chars, text, offset),
        "k" => move_up_logical(&chars, text, offset),
        "gj" => move_down_logical(&chars, text, offset),
        "gk" => move_up_logical(&chars, text, offset),
        "w" => next_vim_word(text, offset),
        "b" => prev_vim_word(text, offset),
        "e" => end_of_vim_word(text, offset),
        "W" => next_word_big(text, offset),
        "B" => prev_word_big(text, offset),
        "E" => end_of_word_big(text, offset),
        "0" => start_of_logical_line(text, offset),
        "^" => first_non_blank_in_logical_line(text, offset),
        "$" => end_of_logical_line(text, offset),
        "G" => start_of_last_line(text),
        _ => offset,
    }
}

fn move_left(_chars: &[char], offset: usize) -> usize {
    if offset == 0 {
        0
    } else {
        offset - 1
    }
}

fn move_right(_chars: &[char], offset: usize, len: usize) -> usize {
    if offset >= len {
        offset
    } else {
        offset + 1
    }
}

fn start_of_logical_line(text: &str, offset: usize) -> usize {
    let before = &text[..offset.min(text.len())];
    match before.rfind('\n') {
        Some(pos) => pos + 1,
        None => 0,
    }
}

fn end_of_logical_line(text: &str, offset: usize) -> usize {
    let after = &text[offset.min(text.len())..];
    match after.find('\n') {
        Some(pos) => offset + pos,
        None => text.len(),
    }
}

fn first_non_blank_in_logical_line(text: &str, offset: usize) -> usize {
    let line_start = start_of_logical_line(text, offset);
    let line_end = end_of_logical_line(text, offset);
    let line = &text[line_start..line_end];
    let trimmed = line.trim_start();
    line_start + (line.len() - trimmed.len())
}

fn start_of_last_line(text: &str) -> usize {
    match text.rfind('\n') {
        Some(pos) => pos + 1,
        None => 0,
    }
}

fn move_down_logical(_chars: &[char], text: &str, offset: usize) -> usize {
    let current_start = start_of_logical_line(text, offset);
    let current_end = end_of_logical_line(text, offset);
    if current_end >= text.len() {
        return text.len();
    }
    let col = offset - current_start;
    let next_start = current_end + 1;
    let next_end = end_of_logical_line(text, next_start);
    let next_line_len = next_end - next_start;
    next_start + col.min(next_line_len)
}

fn move_up_logical(_chars: &[char], text: &str, offset: usize) -> usize {
    let current_start = start_of_logical_line(text, offset);
    if current_start == 0 {
        return 0;
    }
    let col = offset - current_start;
    let prev_end = current_start - 1;
    let prev_start = start_of_logical_line(text, prev_end);
    let prev_line_len = prev_end - prev_start;
    prev_start + col.min(prev_line_len)
}

fn is_vim_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_vim_whitespace(c: char) -> bool {
    c.is_whitespace()
}

fn is_vim_punctuation(c: char) -> bool {
    !is_vim_word_char(c) && !is_vim_whitespace(c)
}

fn next_vim_word(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if offset >= len {
        return offset;
    }
    let mut pos = offset;
    let c = chars[pos];
    if is_vim_word_char(c) {
        while pos < len && is_vim_word_char(chars[pos]) {
            pos += 1;
        }
    } else if is_vim_punctuation(c) {
        while pos < len && is_vim_punctuation(chars[pos]) {
            pos += 1;
        }
    }
    while pos < len && is_vim_whitespace(chars[pos]) {
        pos += 1;
    }
    pos
}

fn prev_vim_word(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    if offset == 0 {
        return 0;
    }
    let mut pos = offset - 1;
    while pos > 0 && is_vim_whitespace(chars[pos]) {
        pos -= 1;
    }
    if pos == 0 && is_vim_whitespace(chars[0]) {
        return 0;
    }
    let c = chars[pos];
    if is_vim_word_char(c) {
        while pos > 0 && is_vim_word_char(chars[pos - 1]) {
            pos -= 1;
        }
    } else if is_vim_punctuation(c) {
        while pos > 0 && is_vim_punctuation(chars[pos - 1]) {
            pos -= 1;
        }
    }
    pos
}

fn end_of_vim_word(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if offset >= len {
        return offset;
    }
    let mut pos = offset + 1;
    while pos < len && is_vim_whitespace(chars[pos]) {
        pos += 1;
    }
    if pos >= len {
        return len.saturating_sub(1);
    }
    let c = chars[pos];
    if is_vim_word_char(c) {
        while pos + 1 < len && is_vim_word_char(chars[pos + 1]) {
            pos += 1;
        }
    } else if is_vim_punctuation(c) {
        while pos + 1 < len && is_vim_punctuation(chars[pos + 1]) {
            pos += 1;
        }
    }
    pos
}

fn next_word_big(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut pos = offset;
    while pos < len && !is_vim_whitespace(chars[pos]) {
        pos += 1;
    }
    while pos < len && is_vim_whitespace(chars[pos]) {
        pos += 1;
    }
    pos
}

fn prev_word_big(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    if offset == 0 {
        return 0;
    }
    let mut pos = offset;
    if pos > 0 && is_vim_whitespace(chars[pos.saturating_sub(1)]) {
        pos -= 1;
    }
    while pos > 0 && is_vim_whitespace(chars[pos]) {
        pos -= 1;
    }
    while pos > 0 && !is_vim_whitespace(chars[pos - 1]) {
        pos -= 1;
    }
    pos
}

fn end_of_word_big(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if offset >= len {
        return offset;
    }
    let mut pos = offset + 1;
    while pos < len && is_vim_whitespace(chars[pos]) {
        pos += 1;
    }
    while pos < len && !is_vim_whitespace(chars[pos]) {
        if pos + 1 >= len || is_vim_whitespace(chars[pos + 1]) {
            break;
        }
        pos += 1;
    }
    pos.min(len.saturating_sub(1))
}

pub fn is_inclusive_motion(key: &str) -> bool {
    matches!(key, "e" | "E" | "$")
}

pub fn is_linewise_motion(key: &str) -> bool {
    matches!(key, "j" | "k" | "G") || key == "gg"
}

// ============================================================================
// textObjects.ts — Text Object Finding
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct TextObjectRange {
    pub start: usize,
    pub end: usize,
}

fn delimiter_pairs() -> HashMap<&'static str, (&'static str, &'static str)> {
    let mut m = HashMap::new();
    m.insert("(", ("(", ")"));
    m.insert(")", ("(", ")"));
    m.insert("b", ("(", ")"));
    m.insert("[", ("[", "]"));
    m.insert("]", ("[", "]"));
    m.insert("{", ("{", "}"));
    m.insert("}", ("{", "}"));
    m.insert("B", ("{", "}"));
    m.insert("<", ("<", ">"));
    m.insert(">", ("<", ">"));
    m.insert("\"", ("\"", "\""));
    m.insert("'", ("'", "'"));
    m.insert("`", ("`", "`"));
    m
}

pub fn find_text_object(
    text: &str,
    offset: usize,
    object_type: &str,
    is_inner: bool,
) -> Option<TextObjectRange> {
    let chars: Vec<char> = text.chars().collect();
    if object_type == "w" {
        return find_word_object(&chars, offset, is_inner, is_vim_word_char);
    }
    if object_type == "W" {
        return find_word_object(&chars, offset, is_inner, |c| !is_vim_whitespace(c));
    }
    let pairs = delimiter_pairs();
    if let Some(&(open, close)) = pairs.get(object_type) {
        if open == close {
            return find_quote_object(text, offset, open.chars().next().unwrap(), is_inner);
        } else {
            return find_bracket_object(
                text,
                offset,
                open.chars().next().unwrap(),
                close.chars().next().unwrap(),
                is_inner,
            );
        }
    }
    None
}

fn find_word_object(
    chars: &[char],
    offset: usize,
    is_inner: bool,
    is_word: fn(char) -> bool,
) -> Option<TextObjectRange> {
    if offset >= chars.len() {
        return None;
    }
    let mut start = offset;
    let mut end = offset;

    if is_word(chars[offset]) {
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
    } else if is_vim_whitespace(chars[offset]) {
        while start > 0 && is_vim_whitespace(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_vim_whitespace(chars[end]) {
            end += 1;
        }
        return Some(TextObjectRange { start, end });
    } else if is_vim_punctuation(chars[offset]) {
        while start > 0 && is_vim_punctuation(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_vim_punctuation(chars[end]) {
            end += 1;
        }
    } else {
        return None;
    }

    if !is_inner {
        if end < chars.len() && is_vim_whitespace(chars[end]) {
            while end < chars.len() && is_vim_whitespace(chars[end]) {
                end += 1;
            }
        } else if start > 0 && is_vim_whitespace(chars[start - 1]) {
            while start > 0 && is_vim_whitespace(chars[start - 1]) {
                start -= 1;
            }
        }
    }

    Some(TextObjectRange { start, end })
}

fn find_quote_object(
    text: &str,
    offset: usize,
    quote: char,
    is_inner: bool,
) -> Option<TextObjectRange> {
    let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[offset..]
        .find('\n')
        .map(|p| offset + p)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let pos_in_line = offset - line_start;

    let positions: Vec<usize> = line
        .char_indices()
        .filter(|&(_, c)| c == quote)
        .map(|(i, _)| i)
        .collect();

    let mut i = 0;
    while i + 1 < positions.len() {
        let qs = positions[i];
        let qe = positions[i + 1];
        if qs <= pos_in_line && pos_in_line <= qe {
            return if is_inner {
                Some(TextObjectRange {
                    start: line_start + qs + 1,
                    end: line_start + qe,
                })
            } else {
                Some(TextObjectRange {
                    start: line_start + qs,
                    end: line_start + qe + 1,
                })
            };
        }
        i += 2;
    }
    None
}

fn find_bracket_object(
    text: &str,
    offset: usize,
    open: char,
    close: char,
    is_inner: bool,
) -> Option<TextObjectRange> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth: i32 = 0;
    let mut start: Option<usize> = None;

    // Search backward for opening bracket
    let mut i = offset.min(chars.len().saturating_sub(1));
    loop {
        if chars[i] == close && i != offset {
            depth += 1;
        } else if chars[i] == open {
            if depth == 0 {
                start = Some(i);
                break;
            }
            depth -= 1;
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }

    let s = start?;
    depth = 0;
    let mut end: Option<usize> = None;
    for j in (s + 1)..chars.len() {
        if chars[j] == open {
            depth += 1;
        } else if chars[j] == close {
            if depth == 0 {
                end = Some(j);
                break;
            }
            depth -= 1;
        }
    }

    let e = end?;
    if is_inner {
        Some(TextObjectRange {
            start: s + 1,
            end: e,
        })
    } else {
        Some(TextObjectRange {
            start: s,
            end: e + 1,
        })
    }
}

// ============================================================================
// operators.ts — Operator Functions
// ============================================================================

pub struct OperatorContext {
    pub text: String,
    pub offset: usize,
    pub register: String,
    pub register_is_linewise: bool,
    pub last_find: Option<LastFind>,
    pub recorded_change: Option<RecordedChange>,
    pub enter_insert_offset: Option<usize>,
}

impl OperatorContext {
    pub fn new(text: String, offset: usize) -> Self {
        Self {
            text,
            offset,
            register: String::new(),
            register_is_linewise: false,
            last_find: None,
            recorded_change: None,
            enter_insert_offset: None,
        }
    }
}

pub fn execute_operator_motion(
    op: &Operator,
    motion: &str,
    count: usize,
    ctx: &mut OperatorContext,
) {
    let target = resolve_motion(motion, &ctx.text, ctx.offset, count);
    if target == ctx.offset {
        return;
    }
    let range = get_operator_range(ctx.offset, target, motion, op, count, &ctx.text);
    apply_operator(op, range.0, range.1, ctx, range.2);
    ctx.recorded_change = Some(RecordedChange::OperatorMotion {
        op: op.clone(),
        motion: motion.to_string(),
        count,
    });
}

pub fn execute_operator_find(
    op: &Operator,
    find_type: FindType,
    ch: &str,
    count: usize,
    ctx: &mut OperatorContext,
) {
    let target_offset = find_character_in_text(&ctx.text, ctx.offset, ch, find_type, count);
    let target_offset = match target_offset {
        Some(o) => o,
        None => return,
    };
    let from = ctx.offset.min(target_offset);
    let to = (ctx.offset.max(target_offset) + 1).min(ctx.text.len());
    apply_operator(op, from, to, ctx, false);
    ctx.last_find = Some(LastFind {
        find_type,
        ch: ch.to_string(),
    });
    ctx.recorded_change = Some(RecordedChange::OperatorFind {
        op: op.clone(),
        find: find_type,
        ch: ch.to_string(),
        count,
    });
}

pub fn execute_operator_text_obj(
    op: &Operator,
    scope: TextObjScope,
    obj_type: &str,
    count: usize,
    ctx: &mut OperatorContext,
) {
    let range = find_text_object(
        &ctx.text,
        ctx.offset,
        obj_type,
        scope == TextObjScope::Inner,
    );
    let range = match range {
        Some(r) => r,
        None => return,
    };
    apply_operator(op, range.start, range.end, ctx, false);
    ctx.recorded_change = Some(RecordedChange::OperatorTextObj {
        op: op.clone(),
        obj_type: obj_type.to_string(),
        scope,
        count,
    });
}

pub fn execute_line_op(op: &Operator, count: usize, ctx: &mut OperatorContext) {
    let lines: Vec<&str> = ctx.text.split('\n').collect();
    let current_line = ctx.text[..ctx.offset.min(ctx.text.len())]
        .matches('\n')
        .count();
    let lines_to_affect = count.min(lines.len() - current_line);
    let line_start = start_of_logical_line(&ctx.text, ctx.offset);
    let mut line_end = line_start;
    for _ in 0..lines_to_affect {
        match ctx.text[line_end..].find('\n') {
            Some(pos) => line_end = line_end + pos + 1,
            None => {
                line_end = ctx.text.len();
                break;
            }
        }
    }
    let mut content = ctx.text[line_start..line_end].to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    ctx.register = content;
    ctx.register_is_linewise = true;

    match op {
        Operator::Yank => {
            ctx.offset = line_start;
        }
        Operator::Delete => {
            let mut del_start = line_start;
            let del_end = line_end;
            if del_end == ctx.text.len()
                && del_start > 0
                && ctx.text.as_bytes().get(del_start - 1) == Some(&b'\n')
            {
                del_start -= 1;
            }
            let new_text = format!("{}{}", &ctx.text[..del_start], &ctx.text[del_end..]);
            let max_off = new_text.len().saturating_sub(1);
            ctx.offset = del_start.min(max_off);
            ctx.text = new_text;
        }
        Operator::Change => {
            if lines.len() == 1 {
                ctx.text = String::new();
                ctx.enter_insert_offset = Some(0);
            } else {
                let before: Vec<&str> = lines[..current_line].to_vec();
                let after: Vec<&str> = lines[(current_line + lines_to_affect)..].to_vec();
                let mut new_lines = before;
                new_lines.push("");
                new_lines.extend(after);
                ctx.text = new_lines.join("\n");
                ctx.enter_insert_offset = Some(line_start);
            }
        }
    }
    let op_char = match op {
        Operator::Delete => "d",
        Operator::Change => "c",
        Operator::Yank => "y",
    };
    ctx.recorded_change = Some(RecordedChange::OperatorMotion {
        op: op.clone(),
        motion: op_char.to_string(),
        count,
    });
}

pub fn execute_x(count: usize, ctx: &mut OperatorContext) {
    let from = ctx.offset;
    if from >= ctx.text.len() {
        return;
    }
    let to = (from + count).min(ctx.text.len());
    let deleted = ctx.text[from..to].to_string();
    let new_text = format!("{}{}", &ctx.text[..from], &ctx.text[to..]);
    ctx.register = deleted;
    ctx.register_is_linewise = false;
    let max_off = new_text.len().saturating_sub(1);
    ctx.offset = from.min(max_off);
    ctx.text = new_text;
    ctx.recorded_change = Some(RecordedChange::X { count });
}

pub fn execute_replace(ch: &str, count: usize, ctx: &mut OperatorContext) {
    let mut offset = ctx.offset;
    let mut new_text = ctx.text.clone();
    for _ in 0..count {
        if offset >= new_text.len() {
            break;
        }
        new_text = format!("{}{}{}", &new_text[..offset], ch, &new_text[offset + 1..]);
        offset += ch.len();
    }
    ctx.text = new_text;
    ctx.offset = offset.saturating_sub(ch.len());
    ctx.recorded_change = Some(RecordedChange::ReplaceChar {
        ch: ch.to_string(),
        count,
    });
}

pub fn execute_toggle_case(count: usize, ctx: &mut OperatorContext) {
    let start = ctx.offset;
    if start >= ctx.text.len() {
        return;
    }
    let chars: Vec<char> = ctx.text.chars().collect();
    let mut new_chars = chars.clone();
    let end = (start + count).min(chars.len());
    for i in start..end {
        let c = chars[i];
        if c.is_uppercase() {
            for lc in c.to_lowercase() {
                new_chars[i] = lc;
                break;
            }
        } else if c.is_lowercase() {
            for uc in c.to_uppercase() {
                new_chars[i] = uc;
                break;
            }
        }
    }
    ctx.text = new_chars.into_iter().collect();
    ctx.offset = end;
    ctx.recorded_change = Some(RecordedChange::ToggleCase { count });
}

pub fn execute_join(count: usize, ctx: &mut OperatorContext) {
    let lines: Vec<&str> = ctx.text.split('\n').collect();
    let current_line = ctx.text[..ctx.offset.min(ctx.text.len())]
        .matches('\n')
        .count();
    if current_line >= lines.len() - 1 {
        return;
    }
    let lines_to_join = count.min(lines.len() - current_line - 1);
    let mut joined = lines[current_line].to_string();
    let cursor_pos = joined.len();
    for i in 1..=lines_to_join {
        let next = lines[current_line + i].trim_start();
        if !next.is_empty() {
            if !joined.ends_with(' ') && !joined.is_empty() {
                joined.push(' ');
            }
            joined.push_str(next);
        }
    }
    let mut new_lines: Vec<&str> = lines[..current_line].to_vec();
    new_lines.push(&joined);
    new_lines.extend_from_slice(&lines[current_line + lines_to_join + 1..]);
    let new_text = new_lines.join("\n");
    let line_start = get_line_start_offset(&lines[..current_line]);
    ctx.offset = line_start + cursor_pos;
    ctx.text = new_text;
    ctx.recorded_change = Some(RecordedChange::Join { count });
}

pub fn execute_paste(after: bool, count: usize, ctx: &mut OperatorContext) {
    let register = ctx.register.clone();
    if register.is_empty() {
        return;
    }
    let is_linewise = register.ends_with('\n');
    let content = if is_linewise {
        &register[..register.len() - 1]
    } else {
        &register
    };

    if is_linewise {
        let lines: Vec<&str> = ctx.text.split('\n').collect();
        let current_line = ctx.text[..ctx.offset.min(ctx.text.len())]
            .matches('\n')
            .count();
        let insert_line = if after {
            current_line + 1
        } else {
            current_line
        };
        let mut new_lines: Vec<String> = lines[..insert_line.min(lines.len())]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for _ in 0..count {
            for l in content.split('\n') {
                new_lines.push(l.to_string());
            }
        }
        for l in &lines[insert_line.min(lines.len())..] {
            new_lines.push(l.to_string());
        }
        let new_text = new_lines.join("\n");
        let new_offset =
            get_line_start_offset_strings(&new_lines[..insert_line.min(new_lines.len())]);
        ctx.text = new_text;
        ctx.offset = new_offset;
    } else {
        let text_to_insert = content.repeat(count);
        let insert_point = if after && ctx.offset < ctx.text.len() {
            ctx.offset + 1
        } else {
            ctx.offset
        };
        let new_text = format!(
            "{}{}{}",
            &ctx.text[..insert_point],
            text_to_insert,
            &ctx.text[insert_point..]
        );
        let new_offset = insert_point + text_to_insert.len().saturating_sub(1);
        ctx.text = new_text;
        ctx.offset = new_offset.max(insert_point);
    }
}

pub fn execute_indent(dir: char, count: usize, ctx: &mut OperatorContext) {
    let mut lines: Vec<String> = ctx.text.split('\n').map(|s| s.to_string()).collect();
    let current_line = ctx.text[..ctx.offset.min(ctx.text.len())]
        .matches('\n')
        .count();
    let lines_to_affect = count.min(lines.len() - current_line);
    let indent = "  ";
    for i in 0..lines_to_affect {
        let idx = current_line + i;
        if idx >= lines.len() {
            break;
        }
        if dir == '>' {
            lines[idx] = format!("{}{}", indent, lines[idx]);
        } else if lines[idx].starts_with(indent) {
            lines[idx] = lines[idx][indent.len()..].to_string();
        } else if lines[idx].starts_with('\t') {
            lines[idx] = lines[idx][1..].to_string();
        } else {
            let trimmed = lines[idx].trim_start();
            let ws_len = lines[idx].len() - trimmed.len();
            let remove = ws_len.min(indent.len());
            lines[idx] = lines[idx][remove..].to_string();
        }
    }
    let cur_line_text = &lines[current_line.min(lines.len().saturating_sub(1))];
    let first_non_blank = cur_line_text.len() - cur_line_text.trim_start().len();
    let line_start = get_line_start_offset_strings(&lines[..current_line]);
    ctx.text = lines.join("\n");
    ctx.offset = line_start + first_non_blank;
    ctx.recorded_change = Some(RecordedChange::IndentChange { dir, count });
}

pub fn execute_open_line(direction: OpenDirection, ctx: &mut OperatorContext) {
    let lines: Vec<&str> = ctx.text.split('\n').collect();
    let current_line = ctx.text[..ctx.offset.min(ctx.text.len())]
        .matches('\n')
        .count();
    let insert_line = match direction {
        OpenDirection::Below => current_line + 1,
        OpenDirection::Above => current_line,
    };
    let mut new_lines: Vec<String> = lines[..insert_line.min(lines.len())]
        .iter()
        .map(|s| s.to_string())
        .collect();
    new_lines.push(String::new());
    for l in &lines[insert_line.min(lines.len())..] {
        new_lines.push(l.to_string());
    }
    let new_text = new_lines.join("\n");
    let insert_offset =
        get_line_start_offset_strings(&new_lines[..insert_line.min(new_lines.len())]);
    ctx.text = new_text;
    ctx.enter_insert_offset = Some(insert_offset);
    ctx.recorded_change = Some(RecordedChange::OpenLine { direction });
}

pub fn execute_operator_g(op: &Operator, count: usize, ctx: &mut OperatorContext) {
    let target = if count == 1 {
        start_of_last_line(&ctx.text)
    } else {
        go_to_line(&ctx.text, count)
    };
    if target == ctx.offset {
        return;
    }
    let range = get_operator_range(ctx.offset, target, "G", op, count, &ctx.text);
    apply_operator(op, range.0, range.1, ctx, range.2);
    ctx.recorded_change = Some(RecordedChange::OperatorMotion {
        op: op.clone(),
        motion: "G".to_string(),
        count,
    });
}

pub fn execute_operator_gg(op: &Operator, count: usize, ctx: &mut OperatorContext) {
    let target = if count == 1 {
        0
    } else {
        go_to_line(&ctx.text, count)
    };
    if target == ctx.offset {
        return;
    }
    let range = get_operator_range(ctx.offset, target, "gg", op, count, &ctx.text);
    apply_operator(op, range.0, range.1, ctx, range.2);
    ctx.recorded_change = Some(RecordedChange::OperatorMotion {
        op: op.clone(),
        motion: "gg".to_string(),
        count,
    });
}

fn go_to_line(text: &str, line_number: usize) -> usize {
    let lines: Vec<&str> = text.split('\n').collect();
    let target = (line_number.saturating_sub(1)).min(lines.len().saturating_sub(1));
    let mut offset = 0;
    for i in 0..target {
        offset += lines.get(i).map(|l| l.len()).unwrap_or(0) + 1;
    }
    offset
}

fn get_line_start_offset(lines: &[&str]) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let content_len: usize = lines.iter().map(|l| l.len()).sum::<usize>();
    content_len + lines.len() // each line followed by \n
}

fn get_line_start_offset_strings(lines: &[String]) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let content_len: usize = lines.iter().map(|l| l.len()).sum::<usize>();
    content_len + lines.len()
}

fn get_operator_range(
    cursor: usize,
    target: usize,
    motion: &str,
    op: &Operator,
    count: usize,
    text: &str,
) -> (usize, usize, bool) {
    let mut from = cursor.min(target);
    let mut to = cursor.max(target);
    let mut linewise = false;

    if *op == Operator::Change && (motion == "w" || motion == "W") {
        let word_end = end_of_vim_word(text, cursor);
        to = (word_end + 1).min(text.len());
    } else if is_linewise_motion(motion) {
        linewise = true;
        match text[to..].find('\n') {
            Some(pos) => to = to + pos + 1,
            None => {
                to = text.len();
                if from > 0 && text.as_bytes().get(from - 1) == Some(&b'\n') {
                    from -= 1;
                }
            }
        }
    } else if is_inclusive_motion(motion) && cursor <= target {
        to = (to + 1).min(text.len());
    }
    (from, to, linewise)
}

fn apply_operator(
    op: &Operator,
    from: usize,
    to: usize,
    ctx: &mut OperatorContext,
    linewise: bool,
) {
    let mut content = ctx.text[from..to.min(ctx.text.len())].to_string();
    if linewise && !content.ends_with('\n') {
        content.push('\n');
    }
    ctx.register = content;
    ctx.register_is_linewise = linewise;

    match op {
        Operator::Yank => {
            ctx.offset = from;
        }
        Operator::Delete => {
            let new_text = format!(
                "{}{}",
                &ctx.text[..from],
                &ctx.text[to.min(ctx.text.len())..]
            );
            let max_off = new_text.len().saturating_sub(1);
            ctx.offset = from.min(max_off);
            ctx.text = new_text;
        }
        Operator::Change => {
            let new_text = format!(
                "{}{}",
                &ctx.text[..from],
                &ctx.text[to.min(ctx.text.len())..]
            );
            ctx.text = new_text;
            ctx.enter_insert_offset = Some(from);
        }
    }
}

fn find_character_in_text(
    text: &str,
    offset: usize,
    ch: &str,
    find_type: FindType,
    count: usize,
) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    let target_char = ch.chars().next()?;
    let forward = matches!(find_type, FindType::F | FindType::T);
    let till = matches!(find_type, FindType::T | FindType::BigT);
    let mut found = 0;

    if forward {
        for i in (offset + 1)..chars.len() {
            if chars[i] == target_char {
                found += 1;
                if found == count {
                    return if till {
                        Some(i.saturating_sub(1).max(offset))
                    } else {
                        Some(i)
                    };
                }
            }
        }
    } else {
        if offset == 0 {
            return None;
        }
        let mut i = offset - 1;
        loop {
            if chars[i] == target_char {
                found += 1;
                if found == count {
                    return if till {
                        Some((i + 1).min(offset))
                    } else {
                        Some(i)
                    };
                }
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
    }
    None
}

// ============================================================================
// transitions.ts — State Transition Table
// ============================================================================

pub struct TransitionResult {
    pub next: Option<CommandState>,
    pub execute: Option<Box<dyn FnOnce(&mut OperatorContext)>>,
}

impl TransitionResult {
    fn empty() -> Self {
        Self {
            next: None,
            execute: None,
        }
    }
    fn with_next(state: CommandState) -> Self {
        Self {
            next: Some(state),
            execute: None,
        }
    }
    fn with_exec(f: impl FnOnce(&mut OperatorContext) + 'static) -> Self {
        Self {
            next: None,
            execute: Some(Box::new(f)),
        }
    }
}

pub fn transition(
    state: &CommandState,
    input: &str,
    persistent: &PersistentState,
) -> TransitionResult {
    match state {
        CommandState::Idle => from_idle(input, persistent),
        CommandState::Count { digits } => from_count(digits, input, persistent),
        CommandState::OperatorPending { op, count } => from_operator(op, *count, input, persistent),
        CommandState::OperatorCount { op, count, digits } => {
            from_operator_count(op, *count, digits, input, persistent)
        }
        CommandState::OperatorFind { op, count, find } => {
            from_operator_find(op, *count, *find, input)
        }
        CommandState::OperatorTextObj { op, count, scope } => {
            from_operator_text_obj(op, *count, *scope, input)
        }
        CommandState::Find { find, count } => from_find(*find, *count, input),
        CommandState::G { count } => from_g(*count, input),
        CommandState::OperatorG { op, count } => from_operator_g_state(op, *count, input),
        CommandState::Replace { count } => from_replace(*count, input),
        CommandState::Indent { dir, count } => from_indent(*dir, *count, input),
    }
}

fn handle_normal_input(
    input: &str,
    count: usize,
    persistent: &PersistentState,
) -> Option<TransitionResult> {
    if let Some(op) = operator_from_key(input) {
        return Some(TransitionResult::with_next(CommandState::OperatorPending {
            op,
            count,
        }));
    }
    if simple_motions().contains(input) {
        let motion = input.to_string();
        return Some(TransitionResult::with_exec(move |ctx| {
            let target = resolve_motion(&motion, &ctx.text, ctx.offset, count);
            ctx.offset = target;
        }));
    }
    if find_keys().contains(input) {
        let ft = match input {
            "f" => FindType::F,
            "F" => FindType::BigF,
            "t" => FindType::T,
            "T" => FindType::BigT,
            _ => return None,
        };
        return Some(TransitionResult::with_next(CommandState::Find {
            find: ft,
            count,
        }));
    }
    match input {
        "g" => Some(TransitionResult::with_next(CommandState::G { count })),
        "r" => Some(TransitionResult::with_next(CommandState::Replace { count })),
        ">" | "<" => Some(TransitionResult::with_next(CommandState::Indent {
            dir: input.chars().next().unwrap(),
            count,
        })),
        "~" => Some(TransitionResult::with_exec(move |ctx| {
            execute_toggle_case(count, ctx)
        })),
        "x" => Some(TransitionResult::with_exec(move |ctx| {
            execute_x(count, ctx)
        })),
        "J" => Some(TransitionResult::with_exec(move |ctx| {
            execute_join(count, ctx)
        })),
        "p" => Some(TransitionResult::with_exec(move |ctx| {
            execute_paste(true, count, ctx)
        })),
        "P" => Some(TransitionResult::with_exec(move |ctx| {
            execute_paste(false, count, ctx)
        })),
        "D" => Some(TransitionResult::with_exec(move |ctx| {
            execute_operator_motion(&Operator::Delete, "$", 1, ctx);
        })),
        "C" => Some(TransitionResult::with_exec(move |ctx| {
            execute_operator_motion(&Operator::Change, "$", 1, ctx);
        })),
        "Y" => Some(TransitionResult::with_exec(move |ctx| {
            execute_line_op(&Operator::Yank, count, ctx);
        })),
        "G" => Some(TransitionResult::with_exec(move |ctx| {
            if count == 1 {
                ctx.offset = start_of_last_line(&ctx.text);
            } else {
                ctx.offset = go_to_line(&ctx.text, count);
            }
        })),
        "i" => Some(TransitionResult::with_exec(move |ctx| {
            ctx.enter_insert_offset = Some(ctx.offset);
        })),
        "I" => Some(TransitionResult::with_exec(move |ctx| {
            ctx.enter_insert_offset = Some(first_non_blank_in_logical_line(&ctx.text, ctx.offset));
        })),
        "a" => Some(TransitionResult::with_exec(move |ctx| {
            let new_off = if ctx.offset >= ctx.text.len() {
                ctx.offset
            } else {
                ctx.offset + 1
            };
            ctx.enter_insert_offset = Some(new_off);
        })),
        "A" => Some(TransitionResult::with_exec(move |ctx| {
            ctx.enter_insert_offset = Some(end_of_logical_line(&ctx.text, ctx.offset));
        })),
        "o" => Some(TransitionResult::with_exec(move |ctx| {
            execute_open_line(OpenDirection::Below, ctx);
        })),
        "O" => Some(TransitionResult::with_exec(move |ctx| {
            execute_open_line(OpenDirection::Above, ctx);
        })),
        ";" => {
            let lf = persistent.last_find.clone();
            Some(TransitionResult::with_exec(move |ctx| {
                if let Some(ref lf) = lf {
                    if let Some(pos) =
                        find_character_in_text(&ctx.text, ctx.offset, &lf.ch, lf.find_type, count)
                    {
                        ctx.offset = pos;
                    }
                }
            }))
        }
        "," => {
            let lf = persistent.last_find.clone();
            Some(TransitionResult::with_exec(move |ctx| {
                if let Some(ref lf) = lf {
                    let flipped = match lf.find_type {
                        FindType::F => FindType::BigF,
                        FindType::BigF => FindType::F,
                        FindType::T => FindType::BigT,
                        FindType::BigT => FindType::T,
                    };
                    if let Some(pos) =
                        find_character_in_text(&ctx.text, ctx.offset, &lf.ch, flipped, count)
                    {
                        ctx.offset = pos;
                    }
                }
            }))
        }
        _ => None,
    }
}

fn handle_operator_input(op: &Operator, count: usize, input: &str) -> Option<TransitionResult> {
    if is_text_obj_scope_key(input) {
        let scope = text_obj_scope_from_key(input).unwrap();
        return Some(TransitionResult::with_next(CommandState::OperatorTextObj {
            op: op.clone(),
            count,
            scope,
        }));
    }
    if find_keys().contains(input) {
        let ft = match input {
            "f" => FindType::F,
            "F" => FindType::BigF,
            "t" => FindType::T,
            "T" => FindType::BigT,
            _ => return None,
        };
        return Some(TransitionResult::with_next(CommandState::OperatorFind {
            op: op.clone(),
            count,
            find: ft,
        }));
    }
    if simple_motions().contains(input) {
        let op2 = op.clone();
        let motion = input.to_string();
        return Some(TransitionResult::with_exec(move |ctx| {
            execute_operator_motion(&op2, &motion, count, ctx);
        }));
    }
    if input == "G" {
        let op2 = op.clone();
        return Some(TransitionResult::with_exec(move |ctx| {
            execute_operator_g(&op2, count, ctx);
        }));
    }
    if input == "g" {
        return Some(TransitionResult::with_next(CommandState::OperatorG {
            op: op.clone(),
            count,
        }));
    }
    None
}

fn from_idle(input: &str, persistent: &PersistentState) -> TransitionResult {
    if input.len() == 1 {
        let c = input.chars().next().unwrap();
        if c.is_ascii_digit() && c != '0' {
            return TransitionResult::with_next(CommandState::Count {
                digits: input.to_string(),
            });
        }
    }
    if input == "0" {
        return TransitionResult::with_exec(move |ctx| {
            ctx.offset = start_of_logical_line(&ctx.text, ctx.offset);
        });
    }
    if let Some(r) = handle_normal_input(input, 1, persistent) {
        return r;
    }
    TransitionResult::empty()
}

fn from_count(digits: &str, input: &str, persistent: &PersistentState) -> TransitionResult {
    if input.len() == 1 && input.chars().next().unwrap().is_ascii_digit() {
        let new_digits = format!("{}{}", digits, input);
        let count = new_digits.parse::<usize>().unwrap_or(0).min(MAX_VIM_COUNT);
        return TransitionResult::with_next(CommandState::Count {
            digits: count.to_string(),
        });
    }
    let count = digits.parse::<usize>().unwrap_or(1);
    if let Some(r) = handle_normal_input(input, count, persistent) {
        return r;
    }
    TransitionResult::with_next(CommandState::Idle)
}

fn from_operator(
    op: &Operator,
    count: usize,
    input: &str,
    _persistent: &PersistentState,
) -> TransitionResult {
    let op_char = match op {
        Operator::Delete => "d",
        Operator::Change => "c",
        Operator::Yank => "y",
    };
    if input == op_char {
        let op2 = op.clone();
        return TransitionResult::with_exec(move |ctx| execute_line_op(&op2, count, ctx));
    }
    if input.len() == 1 && input.chars().next().unwrap().is_ascii_digit() {
        return TransitionResult::with_next(CommandState::OperatorCount {
            op: op.clone(),
            count,
            digits: input.to_string(),
        });
    }
    if let Some(r) = handle_operator_input(op, count, input) {
        return r;
    }
    TransitionResult::with_next(CommandState::Idle)
}

fn from_operator_count(
    op: &Operator,
    count: usize,
    digits: &str,
    input: &str,
    _persistent: &PersistentState,
) -> TransitionResult {
    if input.len() == 1 && input.chars().next().unwrap().is_ascii_digit() {
        let new_digits = format!("{}{}", digits, input);
        let parsed = new_digits.parse::<usize>().unwrap_or(0).min(MAX_VIM_COUNT);
        return TransitionResult::with_next(CommandState::OperatorCount {
            op: op.clone(),
            count,
            digits: parsed.to_string(),
        });
    }
    let motion_count = digits.parse::<usize>().unwrap_or(1);
    let effective = count * motion_count;
    if let Some(r) = handle_operator_input(op, effective, input) {
        return r;
    }
    TransitionResult::with_next(CommandState::Idle)
}

fn from_operator_find(
    op: &Operator,
    count: usize,
    find: FindType,
    input: &str,
) -> TransitionResult {
    let op2 = op.clone();
    let ch = input.to_string();
    TransitionResult::with_exec(move |ctx| {
        execute_operator_find(&op2, find, &ch, count, ctx);
    })
}

fn from_operator_text_obj(
    op: &Operator,
    count: usize,
    scope: TextObjScope,
    input: &str,
) -> TransitionResult {
    if text_obj_types().contains(input) {
        let op2 = op.clone();
        let ot = input.to_string();
        return TransitionResult::with_exec(move |ctx| {
            execute_operator_text_obj(&op2, scope, &ot, count, ctx);
        });
    }
    TransitionResult::with_next(CommandState::Idle)
}

fn from_find(find: FindType, count: usize, input: &str) -> TransitionResult {
    let ch = input.to_string();
    TransitionResult::with_exec(move |ctx| {
        if let Some(pos) = find_character_in_text(&ctx.text, ctx.offset, &ch, find, count) {
            ctx.offset = pos;
            ctx.last_find = Some(LastFind {
                find_type: find,
                ch,
            });
        }
    })
}

fn from_g(count: usize, input: &str) -> TransitionResult {
    match input {
        "j" | "k" => {
            let motion = format!("g{}", input);
            TransitionResult::with_exec(move |ctx| {
                let target = resolve_motion(&motion, &ctx.text, ctx.offset, count);
                ctx.offset = target;
            })
        }
        "g" => {
            if count > 1 {
                TransitionResult::with_exec(move |ctx| {
                    ctx.offset = go_to_line(&ctx.text, count);
                })
            } else {
                TransitionResult::with_exec(move |ctx| {
                    ctx.offset = 0;
                })
            }
        }
        _ => TransitionResult::with_next(CommandState::Idle),
    }
}

fn from_operator_g_state(op: &Operator, count: usize, input: &str) -> TransitionResult {
    match input {
        "j" | "k" => {
            let op2 = op.clone();
            let motion = format!("g{}", input);
            TransitionResult::with_exec(move |ctx| {
                execute_operator_motion(&op2, &motion, count, ctx);
            })
        }
        "g" => {
            let op2 = op.clone();
            TransitionResult::with_exec(move |ctx| {
                execute_operator_gg(&op2, count, ctx);
            })
        }
        _ => TransitionResult::with_next(CommandState::Idle),
    }
}

fn from_replace(count: usize, input: &str) -> TransitionResult {
    if input.is_empty() {
        return TransitionResult::with_next(CommandState::Idle);
    }
    let ch = input.to_string();
    TransitionResult::with_exec(move |ctx| execute_replace(&ch, count, ctx))
}

fn from_indent(dir: char, count: usize, input: &str) -> TransitionResult {
    if input.len() == 1 && input.chars().next().unwrap() == dir {
        return TransitionResult::with_exec(move |ctx| execute_indent(dir, count, ctx));
    }
    TransitionResult::with_next(CommandState::Idle)
}
