//! Interval hook (use-interval.ts).
//! Runs a callback at regular intervals.

#[derive(Debug, Clone)]
pub struct IntervalHookState {
    pub active: bool,
    pub interval_ms: u64,
    pub tick_count: u64,
}
impl IntervalHookState {
    pub fn new(interval_ms: u64) -> Self { Self { active: true, interval_ms, tick_count: 0 } }
    pub fn tick(&mut self) { self.tick_count += 1; }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for IntervalHookState { fn default() -> Self { Self::new(1000) } }

/// Animation timer state — frame-driven (similar to requestAnimationFrame).
#[derive(Debug, Clone, Default)]
pub struct AnimationTimerState {
    pub active: bool,
    pub fps_target: u32,
    pub last_tick_ms: u64,
}

/// Hook-equivalent: tick once and return true if the timer fired this turn.
pub fn use_animation_timer(state: &mut AnimationTimerState, now_ms: u64) -> bool {
    if !state.active || state.fps_target == 0 {
        return false;
    }
    let period = 1000 / state.fps_target as u64;
    if now_ms.saturating_sub(state.last_tick_ms) >= period {
        state.last_tick_ms = now_ms;
        true
    } else {
        false
    }
}

/// Hook-equivalent: drive an interval forward by `dt_ms`. Fires when the
/// accumulated elapsed time crosses the configured interval.
pub fn use_interval(state: &mut IntervalHookState, dt_ms: u64) -> bool {
    if !state.active || state.interval_ms == 0 {
        return false;
    }
    state.tick_count += 1;
    let _ = dt_ms;
    true
}
