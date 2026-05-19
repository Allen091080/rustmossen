//! ESC sequence parsing (esc.ts).

use super::types::Action;

/// Parse a simple ESC sequence (ESC + one byte).
pub fn parse_esc(final_byte: u8) -> Option<Action> {
    match final_byte {
        b'c' => Some(Action::Reset),
        b'7' => Some(Action::Cursor(super::types::CursorAction::Save)),
        b'8' => Some(Action::Cursor(super::types::CursorAction::Restore)),
        b'D' => Some(Action::Scroll(super::types::ScrollAction::Up(1))),
        b'M' => Some(Action::Scroll(super::types::ScrollAction::Down(1))),
        b'E' => Some(Action::Cursor(super::types::CursorAction::NextLine { count: 1 })),
        _ => None,
    }
}
