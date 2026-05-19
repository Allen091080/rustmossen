//! Semantic ANSI parser — produces structured actions from input (parser.ts).

use super::ansi::C0;
use super::csi::{self, CSI};
use super::dec::dec_mode_to_action;
use super::esc::parse_esc;
use super::osc::parse_osc;
use super::sgr::apply_sgr;
use super::tokenize::{Token, Tokenizer};
use super::types::*;

/// Streaming ANSI parser that produces semantic actions.
#[derive(Debug, Clone)]
pub struct Parser {
    tokenizer: Tokenizer,
    current_style: TextStyle,
}

impl Parser {
    pub fn new() -> Self {
        Self { tokenizer: Tokenizer::new(false), current_style: TextStyle::default_style() }
    }

    /// Parse input into semantic actions.
    pub fn parse(&mut self, input: &str) -> Vec<Action> {
        let tokens = self.tokenizer.feed(input);
        let mut actions = Vec::new();

        for token in tokens {
            match token {
                Token::Text(text) => {
                    let graphemes = self.segment_graphemes(&text);
                    if !graphemes.is_empty() {
                        actions.push(Action::Text { graphemes, style: self.current_style });
                    }
                }
                Token::Sequence(seq) => {
                    if let Some(action) = self.parse_sequence(&seq) {
                        actions.push(action);
                    }
                }
            }
        }
        actions
    }

    /// Parse a single escape sequence into an action.
    fn parse_sequence(&mut self, seq: &str) -> Option<Action> {
        let bytes = seq.as_bytes();
        if bytes.is_empty() { return None; }

        // Single control character
        if bytes.len() == 1 {
            return match bytes[0] {
                C0::BEL => Some(Action::Bell),
                _ => None,
            };
        }

        // Must start with ESC
        if bytes[0] != C0::ESC { return None; }
        if bytes.len() < 2 { return None; }

        match bytes[1] {
            b'[' => self.parse_csi(seq),
            b']' => self.parse_osc_seq(seq),
            b'O' => self.parse_ss3(seq),
            _ => parse_esc(bytes[1]),
        }
    }

    fn parse_csi(&mut self, seq: &str) -> Option<Action> {
        let inner = &seq[2..];
        if inner.is_empty() { return None; }

        let final_byte = inner.as_bytes()[inner.len() - 1];
        let before_final = &inner[..inner.len() - 1];

        let (private_mode, param_str) = if !before_final.is_empty() && "?>=".contains(before_final.chars().next().unwrap_or(' ')) {
            (Some(before_final.chars().next().unwrap()), &before_final[1..])
        } else {
            (None, before_final)
        };

        let params = csi::parse_csi_params(param_str);

        // Private mode sequences (? prefix)
        if let Some('?') = private_mode {
            let mode = params.first().copied().unwrap_or(0) as u16;
            let enabled = final_byte == CSI::SM;
            if let Some(action) = dec_mode_to_action(mode, enabled) {
                return Some(Action::Mode(action));
            }
            // Cursor style (DECSCUSR)
            if final_byte == b'q' {
                let style_num = params.first().copied().unwrap_or(0);
                let (cursor_style, blinking) = match style_num {
                    0 | 1 => (CursorStyle::Block, true),
                    2 => (CursorStyle::Block, false),
                    3 => (CursorStyle::Underline, true),
                    4 => (CursorStyle::Underline, false),
                    5 => (CursorStyle::Bar, true),
                    6 => (CursorStyle::Bar, false),
                    _ => (CursorStyle::Block, true),
                };
                return Some(Action::Cursor(CursorAction::Style { style: cursor_style, blinking }));
            }
            return Some(Action::Unknown(seq.to_string()));
        }

        // Standard CSI sequences
        let count = params.first().copied().unwrap_or(1).max(1);
        match final_byte {
            CSI::CUU => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Up, count })),
            CSI::CUD => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Down, count })),
            CSI::CUF => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Forward, count })),
            CSI::CUB => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Back, count })),
            CSI::CNL => Some(Action::Cursor(CursorAction::NextLine { count })),
            CSI::CPL => Some(Action::Cursor(CursorAction::PrevLine { count })),
            CSI::CHA => Some(Action::Cursor(CursorAction::Column { col: count })),
            CSI::CUP => {
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                Some(Action::Cursor(CursorAction::Position { row, col }))
            }
            CSI::ED => {
                let region = match params.first().copied().unwrap_or(0) {
                    0 => EraseRegion::ToEnd, 1 => EraseRegion::ToStart,
                    2 => EraseRegion::All, 3 => EraseRegion::Scrollback,
                    _ => EraseRegion::ToEnd,
                };
                Some(Action::Erase(EraseAction::Display(region)))
            }
            CSI::EL => {
                let region = match params.first().copied().unwrap_or(0) {
                    0 => EraseLineRegion::ToEnd, 1 => EraseLineRegion::ToStart,
                    _ => EraseLineRegion::All,
                };
                Some(Action::Erase(EraseAction::Line(region)))
            }
            CSI::SU => Some(Action::Scroll(ScrollAction::Up(count))),
            CSI::SD => Some(Action::Scroll(ScrollAction::Down(count))),
            CSI::SGR => {
                let sgr_params = if params.is_empty() { vec![0] } else { params };
                apply_sgr(&mut self.current_style, &sgr_params);
                None // Style change is tracked internally
            }
            _ => Some(Action::Unknown(seq.to_string())),
        }
    }

    fn parse_osc_seq(&self, seq: &str) -> Option<Action> {
        // Strip ESC ] prefix and BEL/ST suffix
        let payload = if seq.starts_with("]") {
            let end = if seq.ends_with("") { seq.len() - 1 }
                else if seq.ends_with("\\") { seq.len() - 2 }
                else { seq.len() };
            &seq[2..end]
        } else {
            return None;
        };
        parse_osc(payload)
    }

    fn parse_ss3(&self, seq: &str) -> Option<Action> {
        if seq.len() < 3 { return None; }
        let final_byte = seq.as_bytes()[2];
        match final_byte {
            b'A' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Up, count: 1 })),
            b'B' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Down, count: 1 })),
            b'C' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Forward, count: 1 })),
            b'D' => Some(Action::Cursor(CursorAction::Move { direction: CursorDirection::Back, count: 1 })),
            _ => None,
        }
    }

    fn segment_graphemes(&self, text: &str) -> Vec<Grapheme> {
        use unicode_segmentation::UnicodeSegmentation;
        text.graphemes(true).map(|g| {
            let width = if g.len() > 4 || g.chars().any(|c| {
                let cp = c as u32;
                (0x1100..=0x115F).contains(&cp) || (0x2E80..=0x9FFF).contains(&cp) ||
                (0xAC00..=0xD7A3).contains(&cp) || (0xF900..=0xFAFF).contains(&cp) ||
                (0x1F300..=0x1FAFF).contains(&cp)
            }) { 2 } else { 1 };
            Grapheme { value: g.to_string(), width }
        }).collect()
    }

    /// Reset parser state.
    pub fn reset(&mut self) {
        self.tokenizer.reset();
        self.current_style = TextStyle::default_style();
    }

    /// Get current style.
    pub fn current_style(&self) -> TextStyle { self.current_style }
}

impl Default for Parser { fn default() -> Self { Self::new() } }
