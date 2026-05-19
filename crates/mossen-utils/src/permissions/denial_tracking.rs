//! Denial tracking infrastructure for permission classifiers.
//!
//! Translates `utils/permissions/denialTracking.ts`.
//! Tracks consecutive denials and total denials to determine
//! when to fall back to prompting.

/// Denial tracking state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenialTrackingState {
    pub consecutive_denials: u32,
    pub total_denials: u32,
}

/// Maximum denial limits before falling back to prompting.
pub struct DenialLimits;

impl DenialLimits {
    pub const MAX_CONSECUTIVE: u32 = 3;
    pub const MAX_TOTAL: u32 = 20;
}

pub fn create_denial_tracking_state() -> DenialTrackingState {
    DenialTrackingState {
        consecutive_denials: 0,
        total_denials: 0,
    }
}

pub fn record_denial(state: &DenialTrackingState) -> DenialTrackingState {
    DenialTrackingState {
        consecutive_denials: state.consecutive_denials + 1,
        total_denials: state.total_denials + 1,
    }
}

pub fn record_success(state: &DenialTrackingState) -> DenialTrackingState {
    if state.consecutive_denials == 0 {
        return *state;
    }
    DenialTrackingState {
        consecutive_denials: 0,
        total_denials: state.total_denials,
    }
}

pub fn should_fallback_to_prompting(state: &DenialTrackingState) -> bool {
    state.consecutive_denials >= DenialLimits::MAX_CONSECUTIVE
        || state.total_denials >= DenialLimits::MAX_TOTAL
}
