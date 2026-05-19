//! Fullscreen mode and tmux control mode detection.
//!
//! Handles alt-screen, mouse tracking, and tmux `-CC` (iTerm2 integration mode)
//! detection for the terminal UI.

use std::process::Command;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Cached tmux control mode probe result.
static TMUX_CONTROL_MODE_PROBED: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static LOGGED_TMUX_CC_DISABLE: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
static CHECKED_TMUX_MOUSE_HINT: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Env-var heuristic for iTerm2's tmux integration mode (`tmux -CC`).
fn is_tmux_control_mode_env_heuristic() -> bool {
    if std::env::var("TMUX").is_err() {
        return false;
    }
    if std::env::var("TERM_PROGRAM").ok().as_deref() != Some("iTerm.app") {
        return false;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    !term.starts_with("screen") && !term.starts_with("tmux")
}

/// Sync one-shot probe: asks tmux directly whether this client is in control
/// mode via `#{client_control_mode}`. Result is cached.
fn probe_tmux_control_mode_sync() {
    let mut guard = TMUX_CONTROL_MODE_PROBED.lock().unwrap();
    *guard = Some(is_tmux_control_mode_env_heuristic());
    if *guard == Some(true) {
        return;
    }
    if std::env::var("TMUX").is_err() {
        return;
    }
    // Only probe when iTerm might be involved
    if std::env::var("TERM_PROGRAM").is_ok() {
        return;
    }

    let result = match Command::new("tmux")
        .args(["display-message", "-p", "#{client_control_mode}"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return,
    };

    if !result.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&result.stdout);
    *guard = Some(stdout.trim() == "1");
}

/// True when running under `tmux -CC` (iTerm2 integration mode).
///
/// The alt-screen / mouse-tracking path is unrecoverable in -CC mode,
/// so callers auto-disable fullscreen.
pub fn is_tmux_control_mode() -> bool {
    {
        let guard = TMUX_CONTROL_MODE_PROBED.lock().unwrap();
        if let Some(val) = *guard {
            return val;
        }
    }
    probe_tmux_control_mode_sync();
    TMUX_CONTROL_MODE_PROBED
        .lock()
        .unwrap()
        .unwrap_or(false)
}

/// Reset tmux control mode probe for testing.
pub fn reset_tmux_control_mode_probe_for_testing() {
    *TMUX_CONTROL_MODE_PROBED.lock().unwrap() = None;
    *LOGGED_TMUX_CC_DISABLE.lock().unwrap() = false;
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}

fn is_env_defined_falsy(val: &str) -> bool {
    matches!(val, "0" | "false" | "no" | "off")
}

/// Runtime env-var check for fullscreen mode.
/// Ants default to on; external users default to off.
pub fn is_fullscreen_env_enabled(user_type: Option<&str>) -> bool {
    let env_val = std::env::var("MOSSEN_CODE_NO_FLICKER").unwrap_or_default();

    // Explicit user opt-out always wins
    if !env_val.is_empty() && is_env_defined_falsy(&env_val) {
        return false;
    }
    // Explicit opt-in overrides auto-detection
    if is_env_truthy(&env_val) {
        return true;
    }
    // Auto-disable under tmux -CC
    if is_tmux_control_mode() {
        let mut logged = LOGGED_TMUX_CC_DISABLE.lock().unwrap();
        if !*logged {
            *logged = true;
            tracing::debug!(
                "fullscreen disabled: tmux -CC (iTerm2 integration mode) detected · set MOSSEN_CODE_NO_FLICKER=1 to override"
            );
        }
        return false;
    }
    user_type == Some("ant")
}

/// Whether fullscreen mode should enable SGR mouse tracking.
pub fn is_mouse_tracking_enabled() -> bool {
    let val = std::env::var("MOSSEN_CODE_DISABLE_MOUSE").unwrap_or_default();
    !is_env_truthy(&val)
}

/// Whether mouse click handling is disabled.
pub fn is_mouse_clicks_disabled() -> bool {
    let val = std::env::var("MOSSEN_CODE_DISABLE_MOUSE_CLICKS").unwrap_or_default();
    is_env_truthy(&val)
}

/// True when the fullscreen alt-screen layout is actually rendering.
pub fn is_fullscreen_active(is_interactive: bool, user_type: Option<&str>) -> bool {
    is_interactive && is_fullscreen_env_enabled(user_type)
}

/// One-time hint for tmux users in fullscreen with `mouse off`.
///
/// Returns the hint text once per session if TMUX is set, fullscreen is active,
/// and tmux's current `mouse` option is off; None otherwise.
pub async fn maybe_get_tmux_mouse_hint(
    is_interactive: bool,
    user_type: Option<&str>,
) -> Option<String> {
    if std::env::var("TMUX").is_err() {
        return None;
    }
    if !is_fullscreen_active(is_interactive, user_type) || is_tmux_control_mode() {
        return None;
    }
    {
        let mut checked = CHECKED_TMUX_MOUSE_HINT.lock().unwrap();
        if *checked {
            return None;
        }
        *checked = true;
    }

    let output = tokio::process::Command::new("tmux")
        .args(["show", "-Av", "mouse"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim() == "on" {
        return None;
    }

    Some(
        "tmux detected · scroll with PgUp/PgDn · or add 'set -g mouse on' to ~/.tmux.conf for wheel scroll"
            .to_string(),
    )
}

/// Reset module-level once-per-session flags for testing.
pub fn reset_for_testing() {
    *LOGGED_TMUX_CC_DISABLE.lock().unwrap() = false;
    *CHECKED_TMUX_MOUSE_HINT.lock().unwrap() = false;
}
