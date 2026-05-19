//! AnimationFrame hook (use-animation-frame.ts).
//! Calls a callback on each animation frame tick.

#[derive(Debug, Clone)]
pub struct AnimationFrameHookState {
    pub active: bool,
    pub interval_ms: Option<u64>,
    pub time: u64,
    pub frame_count: u64,
}
impl AnimationFrameHookState {
    pub fn new(interval_ms: Option<u64>) -> Self { Self { active: interval_ms.is_some(), interval_ms, time: 0, frame_count: 0 } }
    pub fn tick(&mut self, delta_ms: u64) { if self.active { self.time += delta_ms; self.frame_count += 1; } }
    pub fn set_interval(&mut self, ms: Option<u64>) { self.interval_ms = ms; self.active = ms.is_some(); }
}
impl Default for AnimationFrameHookState { fn default() -> Self { Self::new(None) } }

/// Hook-equivalent useAnimationFrame — advance frame count and return it.
pub fn use_animation_frame(state: &mut AnimationFrameHookState, delta_ms: u64) -> u64 {
    state.tick(delta_ms);
    state.frame_count
}
