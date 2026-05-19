//! Styles (styles.ts).

/// 24-bit hex colour (e.g. "#ff00aa").
pub type HexColor = String;

/// 256-colour palette index.
pub type Ansi256Color = u8;

/// Named ANSI 16-colour identifier.
pub type AnsiColor = &'static str;

#[derive(Debug, Clone, Default)]
pub struct StylesState {
    pub initialized: bool,
}

impl StylesState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
