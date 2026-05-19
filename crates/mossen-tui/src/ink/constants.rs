//! Constants (constants.ts).

/// Approximate frame interval (60 Hz) in milliseconds.
pub const FRAME_INTERVAL_MS: u64 = 16;

#[derive(Debug, Clone, Default)]
pub struct ConstantsState {
    pub initialized: bool,
}

impl ConstantsState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
