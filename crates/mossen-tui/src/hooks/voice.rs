//! Voice hook (useVoice.ts).
//! Core voice input state management.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState { Idle, Listening, Processing, Error }

#[derive(Debug, Clone)]
pub struct VoiceInputState {
    pub state: VoiceState,
    pub transcript: String,
    pub confidence: f32,
    pub is_final: bool,
    pub error: Option<String>,
}

impl VoiceInputState {
    pub fn new() -> Self { Self { state: VoiceState::Idle, transcript: String::new(), confidence: 0.0, is_final: false, error: None } }
    pub fn start_listening(&mut self) { self.state = VoiceState::Listening; self.transcript.clear(); self.is_final = false; self.error = None; }
    pub fn update_transcript(&mut self, text: &str, confidence: f32, is_final: bool) {
        self.transcript = text.to_string(); self.confidence = confidence; self.is_final = is_final;
        if is_final { self.state = VoiceState::Idle; }
    }
    pub fn stop(&mut self) { self.state = VoiceState::Idle; }
    pub fn error(&mut self, msg: String) { self.state = VoiceState::Error; self.error = Some(msg); }
    pub fn is_active(&self) -> bool { matches!(self.state, VoiceState::Listening | VoiceState::Processing) }
}
impl Default for VoiceInputState { fn default() -> Self { Self::new() } }

// ============================================================================
// Constants and helpers translated from useVoice.ts
// ============================================================================

/// Fallback (ms) for modifier-combo first-press activation. macOS default
/// key-repeat delay is up to ~2s on "Long"; we arm the release timer for
/// that long.
///
/// TS source: `export const FIRST_PRESS_FALLBACK_MS = 2000`.
pub const FIRST_PRESS_FALLBACK_MS: u64 = 2000;

/// Compute RMS amplitude from a 16-bit signed PCM buffer and return a
/// normalized 0–1 value. A sqrt curve spreads quieter levels across more
/// of the visual range.
///
/// TS source: `computeLevel(chunk)`. The buffer is little-endian 16-bit
/// signed PCM (2 bytes per sample). Returns 0 for empty input.
pub fn compute_level(chunk: &[u8]) -> f32 {
    let samples = chunk.len() >> 1;
    if samples == 0 {
        return 0.0;
    }
    let mut sum_sq: f64 = 0.0;
    let mut i = 0;
    while i + 1 < chunk.len() {
        // Read 16-bit signed little-endian.
        let lo = chunk[i] as i32;
        let hi = chunk[i + 1] as i32;
        let raw = (lo | (hi << 8)) & 0xFFFF;
        // Sign-extend 16 → 32.
        let sample = ((raw << 16) >> 16) as i32;
        sum_sq += (sample * sample) as f64;
        i += 2;
    }
    let mean = sum_sq / samples as f64;
    let rms = mean.sqrt();
    let normalized = (rms / 2000.0).min(1.0);
    (normalized.sqrt()) as f32
}

/// Localized error message shown when the microphone produces only silence
/// for the full warm-up window.
///
/// TS source: `getSilentMicrophoneErrorMessage()`. The TS version uses
/// `getLocalizedText` + `getProductDisplayName`; we emit the English copy
/// (the canonical fallback) with a `{product}` placeholder caller can
/// substitute.
pub fn get_silent_microphone_error_message(product_display_name: &str) -> String {
    format!(
        "No audio detected from microphone. Check that the correct input \
         device is selected and that microphone access is enabled for {}.",
        product_display_name,
    )
}

/// Localized error message shown when voice mode cannot reach a backend.
///
/// TS source: `getVoiceConnectionUnavailableMessage()`. Two flavors based
/// on whether a custom backend is enabled.
pub fn get_voice_connection_unavailable_message(custom_backend_enabled: bool) -> String {
    if custom_backend_enabled {
        "Voice mode could not connect to the current backend voice service. \
         Check backend credentials and voice endpoint settings, then try again."
            .to_string()
    } else {
        "Voice mode requires a configured voice backend. Configure custom \
         voice backend credentials or enable an explicit hosted voice adapter."
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_level_empty() {
        assert_eq!(compute_level(&[]), 0.0);
    }

    #[test]
    fn compute_level_zero_samples() {
        let buf = vec![0u8; 32];
        assert_eq!(compute_level(&buf), 0.0);
    }

    #[test]
    fn compute_level_full_scale() {
        // 0x7FFF = 32767 (max positive 16-bit signed). Little-endian.
        let mut buf = Vec::new();
        for _ in 0..16 {
            buf.push(0xFFu8);
            buf.push(0x7Fu8);
        }
        let level = compute_level(&buf);
        // (32767 / 2000) clamps to 1.0, sqrt(1.0) = 1.0.
        assert!((level - 1.0).abs() < 1e-6);
    }

    #[test]
    fn first_press_fallback_constant() {
        assert_eq!(FIRST_PRESS_FALLBACK_MS, 2000);
    }

    #[test]
    fn silent_microphone_message_has_product() {
        let m = get_silent_microphone_error_message("Mossen");
        assert!(m.contains("Mossen"));
    }
}
