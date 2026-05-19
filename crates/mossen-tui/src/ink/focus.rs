//! Focus (focus.ts) — focus management for the Ink virtual DOM.

use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
pub struct FocusState {
    pub initialized: bool,
}

impl FocusState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
}

/// Focusable DOM node id.
pub type NodeId = u64;

/// Root node accessor — returns the root id of the focus tree.
pub fn get_root_node() -> NodeId {
    0
}

/// Focus manager — owns the focus stack and the current target.
#[derive(Debug, Clone, Default)]
pub struct FocusManager {
    pub focused: Option<NodeId>,
    pub stack: Vec<NodeId>,
    pub disabled: bool,
}

impl FocusManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn focus(&mut self, id: NodeId) {
        self.focused = Some(id);
        if !self.stack.last().is_some_and(|&top| top == id) {
            self.stack.push(id);
        }
    }

    pub fn blur(&mut self) {
        self.focused = None;
    }

    pub fn pop(&mut self) {
        self.stack.pop();
        self.focused = self.stack.last().copied();
    }

    pub fn disable(&mut self) {
        self.disabled = true;
    }

    pub fn enable(&mut self) {
        self.disabled = false;
    }
}

/// Process-wide singleton accessor for the focus manager.
pub fn get_focus_manager() -> &'static Mutex<FocusManager> {
    static CELL: OnceLock<Mutex<FocusManager>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(FocusManager::new()))
}
