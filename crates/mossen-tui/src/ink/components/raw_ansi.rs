//! RawAnsi component (raw_ansi.ts/tsx).
//! Renders pre-formatted ANSI escape sequences directly.

#[derive(Debug, Clone)]
pub struct RawAnsiState {
    pub active: bool,
}
impl RawAnsiState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for RawAnsiState { fn default() -> Self { Self::new() } }

/// Raw passthrough component — emits the ANSI text unchanged.
pub fn raw_ansi(text: &str) -> String {
    text.to_string()
}

/// PascalCase alias matching the TS component name.
#[allow(non_snake_case)]
pub fn RawAnsi(text: &str) -> String {
    raw_ansi(text)
}
