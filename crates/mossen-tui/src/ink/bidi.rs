//! Bidi (bidi.ts) — basic bidirectional text reordering.

/// Reorder a string containing both LTR and RTL runs for display.
///
/// This is a simplified port that only handles strong-direction runs;
/// it does not implement the full Unicode Bidirectional Algorithm.
pub fn reorder_bidi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut run = String::new();
    let mut in_rtl = false;
    for c in input.chars() {
        let is_rtl = matches!(c as u32,
            0x0590..=0x05FF | 0x0600..=0x06FF |
            0x0700..=0x074F | 0xFB1D..=0xFDFF |
            0xFE70..=0xFEFF);
        if is_rtl != in_rtl {
            if in_rtl {
                out.push_str(&run.chars().rev().collect::<String>());
            } else {
                out.push_str(&run);
            }
            run.clear();
            in_rtl = is_rtl;
        }
        run.push(c);
    }
    if in_rtl {
        out.push_str(&run.chars().rev().collect::<String>());
    } else {
        out.push_str(&run);
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct BidiState {
    pub initialized: bool,
}

impl BidiState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
