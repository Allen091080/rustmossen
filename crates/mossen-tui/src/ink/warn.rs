//! Warn (warn.ts).

/// Warn (via tracing) when `n` is not an integer; returns the integer cast.
pub fn if_not_integer(n: f64, context: &str) -> i64 {
    if n.fract() != 0.0 {
        tracing::warn!("non-integer {} in {}: {}", n, context, n.fract());
    }
    n.trunc() as i64
}

#[derive(Debug, Clone, Default)]
pub struct WarnState {
    pub initialized: bool,
}

impl WarnState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
