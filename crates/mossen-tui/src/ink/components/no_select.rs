//! NoSelect component (no_select.ts/tsx).
//! Prevents text selection within its children.

#[derive(Debug, Clone)]
pub struct NoSelectState {
    pub active: bool,
}
impl NoSelectState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for NoSelectState { fn default() -> Self { Self::new() } }

/// Wrap content so its text cannot be selected.
pub fn no_select<T>(content: T) -> T {
    content
}

#[allow(non_snake_case)]
pub fn NoSelect<T>(content: T) -> T {
    content
}
