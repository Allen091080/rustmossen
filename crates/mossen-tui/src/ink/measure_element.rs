//! Measure Element (measure-element.ts).

#[derive(Debug, Clone, Default)]
pub struct MeasureElementState {
    pub initialized: bool,
}

impl MeasureElementState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
