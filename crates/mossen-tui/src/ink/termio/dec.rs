//! DEC private mode constants (dec.ts).

/// DEC private mode numbers.
pub struct DEC;
impl DEC {
    pub const CURSOR_KEYS: u16 = 1;
    pub const ORIGIN: u16 = 6;
    pub const AUTO_WRAP: u16 = 7;
    pub const CURSOR_VISIBLE: u16 = 25;
    pub const ALT_SCREEN: u16 = 47;
    pub const MOUSE_NORMAL: u16 = 1000;
    pub const MOUSE_BUTTON: u16 = 1002;
    pub const MOUSE_ANY: u16 = 1003;
    pub const FOCUS_EVENT: u16 = 1004;
    pub const FOCUS_EVENTS: u16 = 1004;
    pub const MOUSE_SGR: u16 = 1006;
    pub const ALTERNATE_SCREEN: u16 = 1049;
    pub const ALT_SCREEN_CLEAR: u16 = 1049;
    pub const BRACKETED_PASTE: u16 = 2004;
    pub const SYNCHRONIZED_UPDATE: u16 = 2026;
}

/// Generate `CSI ? N h` (set DEC private mode).
pub fn decset(mode: u16) -> String {
    super::csi::csi_raw(&format!("?{}h", mode))
}

/// Generate `CSI ? N l` (reset DEC private mode).
pub fn decreset(mode: u16) -> String {
    super::csi::csi_raw(&format!("?{}l", mode))
}

/// Begin synchronized update (BSU).
pub fn bsu() -> String { decset(DEC::SYNCHRONIZED_UPDATE) }

/// End synchronized update (ESU).
pub fn esu() -> String { decreset(DEC::SYNCHRONIZED_UPDATE) }

/// Enable bracketed paste (EBP).
pub fn ebp() -> String { decset(DEC::BRACKETED_PASTE) }

/// Disable bracketed paste (DBP).
pub fn dbp() -> String { decreset(DEC::BRACKETED_PASTE) }

/// Enable focus events (EFE).
pub fn efe() -> String { decset(DEC::FOCUS_EVENTS) }

/// Disable focus events (DFE).
pub fn dfe() -> String { decreset(DEC::FOCUS_EVENTS) }

/// Show cursor.
pub fn show_cursor() -> String { decset(DEC::CURSOR_VISIBLE) }

/// Hide cursor.
pub fn hide_cursor() -> String { decreset(DEC::CURSOR_VISIBLE) }

/// Enter alternate screen buffer (clearing).
pub fn enter_alt_screen() -> String { decset(DEC::ALT_SCREEN_CLEAR) }

/// Exit alternate screen buffer.
pub fn exit_alt_screen() -> String { decreset(DEC::ALT_SCREEN_CLEAR) }

/// Enable full mouse tracking with SGR encoding (1000+1002+1003+1006).
pub fn enable_mouse_tracking() -> String {
    let mut s = String::new();
    s.push_str(&decset(DEC::MOUSE_NORMAL));
    s.push_str(&decset(DEC::MOUSE_BUTTON));
    s.push_str(&decset(DEC::MOUSE_ANY));
    s.push_str(&decset(DEC::MOUSE_SGR));
    s
}

/// Disable mouse tracking (in reverse order).
pub fn disable_mouse_tracking() -> String {
    let mut s = String::new();
    s.push_str(&decreset(DEC::MOUSE_SGR));
    s.push_str(&decreset(DEC::MOUSE_ANY));
    s.push_str(&decreset(DEC::MOUSE_BUTTON));
    s.push_str(&decreset(DEC::MOUSE_NORMAL));
    s
}

/// Map DEC mode number to semantic action.
pub fn dec_mode_to_action(mode: u16, enabled: bool) -> Option<super::types::ModeAction> {
    use super::types::{ModeAction, MouseTrackingMode};
    match mode {
        DEC::ALTERNATE_SCREEN => Some(ModeAction::AlternateScreen(enabled)),
        DEC::BRACKETED_PASTE => Some(ModeAction::BracketedPaste(enabled)),
        DEC::FOCUS_EVENT => Some(ModeAction::FocusEvents(enabled)),
        DEC::MOUSE_NORMAL => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Normal } else { MouseTrackingMode::Off })),
        DEC::MOUSE_BUTTON => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Button } else { MouseTrackingMode::Off })),
        DEC::MOUSE_ANY => Some(ModeAction::MouseTracking(if enabled { MouseTrackingMode::Any } else { MouseTrackingMode::Off })),
        _ => None,
    }
}
