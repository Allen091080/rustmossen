//! SearchHighlight hook (use-search-highlight.ts).
//! Manages search term highlighting in output.

#[derive(Debug, Clone)]
pub struct SearchHighlightHookState {
    pub active: bool,
}
impl SearchHighlightHookState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for SearchHighlightHookState { fn default() -> Self { Self::new() } }

/// Hook-equivalent useSearchHighlight.
pub fn use_search_highlight(state: &mut SearchHighlightHookState) -> &mut SearchHighlightHookState {
    state
}
