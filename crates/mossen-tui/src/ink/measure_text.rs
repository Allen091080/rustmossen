//! Measure Text (measure-text.ts).

#[derive(Debug, Clone, Default)]
pub struct MeasureTextState {
    pub initialized: bool,
}

impl MeasureTextState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
