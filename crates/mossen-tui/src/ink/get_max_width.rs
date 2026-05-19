//! Get Max Width (get-max-width.ts).

#[derive(Debug, Clone, Default)]
pub struct GetMaxWidthState {
    pub initialized: bool,
}

impl GetMaxWidthState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
