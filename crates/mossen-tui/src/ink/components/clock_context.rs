//! ClockContext component (clock_context.ts/tsx).
//! Provides a shared animation clock for synchronized animations.

#[derive(Debug, Clone)]
pub struct ClockContextState {
    pub active: bool,
}
impl ClockContextState {
    pub fn new() -> Self { Self { active: true } }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for ClockContextState { fn default() -> Self { Self::new() } }

/// Clock primitive — tick counter and elapsed-time accumulator.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    pub tick: u64,
    pub last_tick_ms: u64,
    pub fps: u32,
}

/// Build a new clock with the given target FPS.
pub fn create_clock(fps: u32) -> Clock {
    Clock {
        tick: 0,
        last_tick_ms: 0,
        fps,
    }
}

/// Provider state for the Clock context.
#[derive(Debug, Clone, Default)]
pub struct ClockProvider {
    pub clock: Clock,
}

/// Context handle — alias used by readers.
#[allow(non_upper_case_globals)]
pub static ClockContext: Option<&'static Clock> = None;

/// Initialise a fresh provider with the given FPS.
pub fn clock_provider(fps: u32) -> ClockProvider {
    ClockProvider {
        clock: create_clock(fps),
    }
}
