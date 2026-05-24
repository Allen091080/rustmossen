//! Voice Integration hook (useVoiceIntegration.ts).
//! Integrates voice recognition with the input system.

#[derive(Debug, Clone)]
pub struct VoiceIntegrationState {
    pub is_recording: bool,
    pub auto_submit: bool,
    pub buffer: String,
    pub session_count: u32,
}

impl VoiceIntegrationState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            auto_submit: true,
            buffer: String::new(),
            session_count: 0,
        }
    }
    pub fn start_session(&mut self) {
        self.is_recording = true;
        self.buffer.clear();
        self.session_count += 1;
    }
    pub fn append_text(&mut self, text: &str) {
        self.buffer.push_str(text);
    }
    pub fn end_session(&mut self) -> Option<String> {
        self.is_recording = false;
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }
    pub fn cancel(&mut self) {
        self.is_recording = false;
        self.buffer.clear();
    }
    pub fn set_auto_submit(&mut self, auto: bool) {
        self.auto_submit = auto;
    }
}
impl Default for VoiceIntegrationState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Voice keybinding handler — translated from useVoiceKeybindingHandler in
// Voice integration hook.
// ============================================================================

/// Default fallback (ms) when a key-down arrives with no auto-repeat yet.
/// Mirrors `REPEAT_FALLBACK_MS` from TS (600).
pub const VOICE_REPEAT_FALLBACK_MS: u64 = 600;

/// First-press fallback (ms) — long enough to span macOS' initial repeat
/// delay. Mirrors the TS constant of the same name in `useVoice.ts`.
pub const VOICE_FIRST_PRESS_FALLBACK_MS: u64 = 2000;

/// Warm-up press count: how many rapid presses we let flow through to the
/// underlying text input before activating voice. Matches the TS hook's
/// `WARMUP_THRESHOLD`.
pub const VOICE_WARMUP_THRESHOLD: u32 = 3;

/// Per-press state for the voice keybinding handler. Translated from the
/// `useVoiceKeybindingHandler` body, which uses several React refs:
///   - `rapidCountRef`
///   - `charsInInputRef`
///   - `recordingFloorRef`
///   - `isHoldActiveRef`
#[derive(Debug, Clone, Default)]
pub struct VoiceKeybindingHandlerState {
    /// Count of recent rapid-fire key events.
    pub rapid_count: u32,
    /// Chars we intentionally let through to the input before activation.
    pub chars_in_input: u32,
    /// Trailing-char count to preserve during the activation strip.
    pub recording_floor: u32,
    /// True while a key-hold is driving an active recording.
    pub is_hold_active: bool,
}

/// Outcome of a voice keybinding decision. Tells the caller what to do
/// with the keypress: swallow it (don't insert), flow through (let it
/// reach the text input), or trigger activation/release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceKeybindingOutcome {
    /// Nothing to do — caller should leave the input alone.
    Ignore,
    /// Let the char reach the text input (warm-up flow-through).
    FlowThrough,
    /// Activate voice recording. The caller should also strip the
    /// flow-through chars from the input.
    Activate { strip_chars: u32 },
    /// Stop hold-mode recording.
    Release,
}

impl VoiceKeybindingHandlerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all counters — called when recording transitions out of the
    /// `recording` state. Mirrors the `useEffect` in TS that resets refs
    /// when `voiceState !== 'recording'`.
    pub fn reset(&mut self) {
        self.rapid_count = 0;
        self.chars_in_input = 0;
        self.recording_floor = 0;
        self.is_hold_active = false;
    }

    /// Handle a candidate bare-char key event. Returns the action the
    /// caller should take.
    ///
    /// TS source: the body of `useVoiceKeybindingHandler.handleKeyDown`
    /// for bare-char (unmodified single printable) bindings. Modifier
    /// chords (meta+k, ctrl+x) are handled by the caller directly.
    pub fn on_bare_char_press(&mut self, voice_active: bool) -> VoiceKeybindingOutcome {
        if voice_active {
            // Auto-repeat during active recording: just swallow it.
            self.rapid_count = self.rapid_count.saturating_add(1);
            return VoiceKeybindingOutcome::Ignore;
        }
        self.rapid_count = self.rapid_count.saturating_add(1);
        if self.rapid_count <= VOICE_WARMUP_THRESHOLD {
            // Warm-up: flow the char into the text input so single taps
            // type normally.
            self.chars_in_input = self.chars_in_input.saturating_add(1);
            return VoiceKeybindingOutcome::FlowThrough;
        }
        // Past the warm-up threshold → activate voice. Strip the
        // chars we let through (plus the activation event's potential
        // leak — one extra char).
        let strip = self.chars_in_input + 1;
        self.recording_floor = self.chars_in_input;
        self.chars_in_input = 0;
        self.is_hold_active = true;
        VoiceKeybindingOutcome::Activate { strip_chars: strip }
    }

    /// Handle a candidate modifier-combo press (e.g. meta+k). Activation
    /// is immediate — no flow-through. The fallback ms tells the caller
    /// how long to wait before assuming the key was released.
    pub fn on_modifier_combo_press(&mut self) -> (VoiceKeybindingOutcome, u64) {
        self.is_hold_active = true;
        self.chars_in_input = 0;
        self.recording_floor = 0;
        self.rapid_count = 0;
        (
            VoiceKeybindingOutcome::Activate { strip_chars: 0 },
            VOICE_FIRST_PRESS_FALLBACK_MS,
        )
    }

    /// Fallback ms appropriate for a bare-char press given the current
    /// rapid count.
    pub fn fallback_ms(&self) -> u64 {
        if self.rapid_count <= 1 {
            VOICE_FIRST_PRESS_FALLBACK_MS
        } else {
            VOICE_REPEAT_FALLBACK_MS
        }
    }

    /// Called when the release timer fires or the key-up event arrives.
    pub fn on_release(&mut self) -> VoiceKeybindingOutcome {
        if !self.is_hold_active {
            return VoiceKeybindingOutcome::Ignore;
        }
        self.is_hold_active = false;
        self.rapid_count = 0;
        self.chars_in_input = 0;
        self.recording_floor = 0;
        VoiceKeybindingOutcome::Release
    }

    /// Construct the equivalent of `useVoiceKeybindingHandler` — returns
    /// a state struct preconfigured for first use. This is the rough
    /// Rust analog of calling the hook in TS.
    pub fn use_voice_keybinding_handler() -> Self {
        Self::default()
    }
}

/// Shim that mirrors the temporary JSX `VoiceKeybindingHandler` wrapper
/// from TS — it just creates a fresh handler state. The Rust port has no
/// JSX; this function exists so callers porting `<VoiceKeybindingHandler
/// .../>` markup have a one-to-one symbol.
///
/// TS source: `export function VoiceKeybindingHandler(props)` which
/// internally calls `useVoiceKeybindingHandler(props)` and returns null.
pub fn voice_keybinding_handler() -> VoiceKeybindingHandlerState {
    VoiceKeybindingHandlerState::new()
}

#[cfg(test)]
mod voice_keybinding_tests {
    use super::*;

    #[test]
    fn warmup_then_activate() {
        let mut s = VoiceKeybindingHandlerState::new();
        assert_eq!(
            s.on_bare_char_press(false),
            VoiceKeybindingOutcome::FlowThrough
        );
        assert_eq!(
            s.on_bare_char_press(false),
            VoiceKeybindingOutcome::FlowThrough
        );
        assert_eq!(
            s.on_bare_char_press(false),
            VoiceKeybindingOutcome::FlowThrough
        );
        match s.on_bare_char_press(false) {
            VoiceKeybindingOutcome::Activate { strip_chars } => {
                assert_eq!(strip_chars, 4);
            }
            other => panic!("expected Activate, got {:?}", other),
        }
        assert!(s.is_hold_active);
    }

    #[test]
    fn modifier_activates_immediately() {
        let mut s = VoiceKeybindingHandlerState::new();
        let (out, ms) = s.on_modifier_combo_press();
        assert!(matches!(
            out,
            VoiceKeybindingOutcome::Activate { strip_chars: 0 }
        ));
        assert_eq!(ms, VOICE_FIRST_PRESS_FALLBACK_MS);
    }

    #[test]
    fn release_clears_state() {
        let mut s = VoiceKeybindingHandlerState::new();
        s.on_modifier_combo_press();
        assert_eq!(s.on_release(), VoiceKeybindingOutcome::Release);
        assert!(!s.is_hold_active);
    }
}
