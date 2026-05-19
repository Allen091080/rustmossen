//! Optimizer (optimizer.ts).

/// Optimise an ANSI output buffer by deduping redundant SGR resets.
pub fn optimize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_reset = false;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"\x1b[0m") {
            if last_was_reset {
                i += 4;
                continue;
            }
            last_was_reset = true;
            out.push_str("\x1b[0m");
            i += 4;
        } else {
            last_was_reset = false;
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct OptimizerState {
    pub initialized: bool,
}

impl OptimizerState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
