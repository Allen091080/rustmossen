//! Ansi Render (ansi-render.ts).

/// Component-equivalent marker for the `<Ansi>` element — passes through
/// pre-rendered ANSI text to the renderer untouched.
pub const ANSI: &str = "ansi-passthrough";

#[allow(non_upper_case_globals)]
pub const Ansi: &str = "ansi-passthrough";

#[derive(Debug, Clone, Default)]
pub struct AnsiRenderState {
    pub initialized: bool,
}

impl AnsiRenderState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
