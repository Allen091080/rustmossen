//! Hit Test (hit-test.ts) — pointer to widget hit-testing.

/// One hit-test target.
#[derive(Debug, Clone)]
pub struct HitTarget {
    pub id: u64,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Test which target a (col,row) intersects. Returns the topmost match.
pub fn hit_test(targets: &[HitTarget], col: u16, row: u16) -> Option<&HitTarget> {
    targets
        .iter()
        .rev()
        .find(|t| col >= t.x && col < t.x + t.w && row >= t.y && row < t.y + t.h)
}

/// Dispatch a click to a hit target. Returns the target id, if any.
pub fn dispatch_click(targets: &[HitTarget], col: u16, row: u16) -> Option<u64> {
    hit_test(targets, col, row).map(|t| t.id)
}

/// Dispatch hover-over to a target. Returns the target id, if any.
pub fn dispatch_hover(targets: &[HitTarget], col: u16, row: u16) -> Option<u64> {
    hit_test(targets, col, row).map(|t| t.id)
}

#[derive(Debug, Clone, Default)]
pub struct HitTestState {
    pub initialized: bool,
}

impl HitTestState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
