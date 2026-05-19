//! Auto mode state functions.
//!
//! Translates `utils/permissions/autoModeState.ts`.
//! Lives in its own module so callers can conditionally use it
//! on feature('TRANSCRIPT_CLASSIFIER').

use std::sync::atomic::{AtomicBool, Ordering};

static AUTO_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
static AUTO_MODE_FLAG_CLI: AtomicBool = AtomicBool::new(false);
static AUTO_MODE_CIRCUIT_BROKEN: AtomicBool = AtomicBool::new(false);

pub fn set_auto_mode_active(active: bool) {
    AUTO_MODE_ACTIVE.store(active, Ordering::SeqCst);
}

pub fn is_auto_mode_active() -> bool {
    AUTO_MODE_ACTIVE.load(Ordering::SeqCst)
}

pub fn set_auto_mode_flag_cli(passed: bool) {
    AUTO_MODE_FLAG_CLI.store(passed, Ordering::SeqCst);
}

pub fn get_auto_mode_flag_cli() -> bool {
    AUTO_MODE_FLAG_CLI.load(Ordering::SeqCst)
}

pub fn set_auto_mode_circuit_broken(broken: bool) {
    AUTO_MODE_CIRCUIT_BROKEN.store(broken, Ordering::SeqCst);
}

pub fn is_auto_mode_circuit_broken() -> bool {
    AUTO_MODE_CIRCUIT_BROKEN.load(Ordering::SeqCst)
}

pub fn reset_for_testing() {
    AUTO_MODE_ACTIVE.store(false, Ordering::SeqCst);
    AUTO_MODE_FLAG_CLI.store(false, Ordering::SeqCst);
    AUTO_MODE_CIRCUIT_BROKEN.store(false, Ordering::SeqCst);
}
