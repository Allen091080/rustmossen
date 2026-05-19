//! TabStatus hook (use-tab-status.ts).
//! Manages per-tab status chrome metadata.

#[derive(Debug, Clone)]
pub struct TabStatusHookState {
    pub active: bool,
    pub indicator: Option<String>,
    pub status: Option<String>,
}
impl TabStatusHookState {
    pub fn new() -> Self { Self { active: true, indicator: None, status: None } }
    pub fn set_indicator(&mut self, color: Option<String>) { self.indicator = color; }
    pub fn set_status(&mut self, text: Option<String>) { self.status = text; }
    pub fn clear(&mut self) { self.indicator = None; self.status = None; }
}
impl Default for TabStatusHookState { fn default() -> Self { Self::new() } }

/// Kind of tab status indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatusKind {
    Idle,
    Busy,
    Attention,
    Error,
    Success,
}

/// Hook-equivalent useTabStatus.
pub fn use_tab_status(state: &mut TabStatusHookState) -> &mut TabStatusHookState {
    state
}
