//! Terminal Focus State (terminal-focus-state.ts).
//!
//! Tracks whether the terminal window currently has focus (DEC 1004 reports
//! ESC [I when focused, ESC [O when blurred). Subscribers can be notified
//! when the focus state changes.

use std::sync::{Mutex, OnceLock};

/// Focus state of the terminal window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalFocusState {
    pub focused: bool,
    pub initialized: bool,
}

impl Default for TerminalFocusState {
    fn default() -> Self {
        Self {
            focused: true,
            initialized: false,
        }
    }
}

struct State {
    cur: TerminalFocusState,
    subscribers: Vec<Box<dyn Fn(TerminalFocusState) + Send + Sync + 'static>>,
}

fn store() -> &'static Mutex<State> {
    static CELL: OnceLock<Mutex<State>> = OnceLock::new();
    CELL.get_or_init(|| {
        Mutex::new(State {
            cur: TerminalFocusState::default(),
            subscribers: Vec::new(),
        })
    })
}

/// Return whether the terminal currently has focus.
pub fn get_terminal_focused() -> bool {
    store().lock().unwrap().cur.focused
}

/// Return a copy of the full focus state.
pub fn get_terminal_focus_state() -> TerminalFocusState {
    store().lock().unwrap().cur
}

/// Subscribe to focus-state changes. Returns the registration index.
pub fn subscribe_terminal_focus(
    cb: impl Fn(TerminalFocusState) + Send + Sync + 'static,
) -> usize {
    let mut s = store().lock().unwrap();
    s.subscribers.push(Box::new(cb));
    s.subscribers.len() - 1
}

/// Update the focus state and notify subscribers.
pub fn set_terminal_focused(focused: bool) {
    let snapshot = {
        let mut s = store().lock().unwrap();
        s.cur.focused = focused;
        s.cur.initialized = true;
        s.cur
    };
    let s = store().lock().unwrap();
    for cb in &s.subscribers {
        cb(snapshot);
    }
}

/// Reset the focus state to defaults (used in tests).
pub fn reset_terminal_focus_state() {
    let mut s = store().lock().unwrap();
    s.cur = TerminalFocusState::default();
    s.subscribers.clear();
}

#[derive(Debug, Clone, Default)]
pub struct TerminalFocusStateState {
    pub initialized: bool,
}

impl TerminalFocusStateState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
}
