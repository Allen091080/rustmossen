//! Renderer (renderer.ts).

/// Top-level renderer handle — owns the framebuffer and previous frame.
#[derive(Debug, Clone, Default)]
pub struct Renderer {
    pub cols: u16,
    pub rows: u16,
    pub prev_frame: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RendererState {
    pub initialized: bool,
}

impl RendererState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
