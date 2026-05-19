//! Output (output.ts) — operations queued for the output buffer.

/// One operation in the output queue.
#[derive(Debug, Clone)]
pub enum Operation {
    Write { x: u16, y: u16, text: String },
    ClearLine { y: u16 },
    ClearRegion { x: u16, y: u16, w: u16, h: u16 },
    SetCursor { x: u16, y: u16 },
}

/// Clipping rectangle applied to draw operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clip {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Default)]
pub struct OutputState {
    pub initialized: bool,
}

impl OutputState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
