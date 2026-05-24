//! `/voice` — Toggle voice mode.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Voice directive — toggle voice dictation mode on/off.
pub struct VoiceDirective;

/// Check if voice mode is currently enabled.
fn is_voice_enabled(ctx: &CommandContext) -> bool {
    ctx.is_env_truthy("MOSSEN_VOICE_ENABLED")
}

/// Check if voice mode is available (feature flag + backend).
fn is_voice_mode_available(ctx: &CommandContext) -> bool {
    ctx.is_env_truthy("MOSSEN_VOICE_GROWTHBOOK_ENABLED")
}

/// Check if voice stream backend is reachable.
fn is_voice_stream_available(ctx: &CommandContext) -> bool {
    // Check if VOICE_STREAM_BASE_URL or equivalent is set
    ctx.env_vars.contains_key("VOICE_STREAM_BASE_URL")
        || ctx
            .env_vars
            .contains_key("MOSSEN_CODE_CUSTOM_VOICE_BASE_URL")
        || !ctx.is_custom_backend
}

/// Check for recording tool availability.
fn check_voice_dependencies() -> (bool, Option<String>) {
    // Check for sox/rec on the system
    let sox_check = std::process::Command::new("which").arg("sox").output();

    match sox_check {
        Ok(output) if output.status.success() => (true, None),
        _ => {
            let install_cmd = if cfg!(target_os = "macos") {
                Some("brew install sox".to_string())
            } else if cfg!(target_os = "linux") {
                Some("apt install sox".to_string())
            } else {
                None
            };
            (false, install_cmd)
        }
    }
}

/// Normalize language code for speech-to-text.
fn normalize_language_for_stt(ctx: &CommandContext) -> (String, Option<String>) {
    let lang = ctx
        .env_vars
        .get("MOSSEN_LANGUAGE")
        .cloned()
        .unwrap_or_else(|| "en".to_string());

    // Supported STT languages
    let supported = ["en", "zh", "ja", "ko", "fr", "de", "es", "it", "pt", "ru"];

    if supported.contains(&lang.as_str()) {
        (lang, None)
    } else {
        // Fall back to English if language not supported for STT
        ("en".to_string(), Some(lang))
    }
}

/// Get the shortcut display for push-to-talk.
fn get_shortcut_display() -> &'static str {
    "Space"
}

#[async_trait]
impl Directive for VoiceDirective {
    fn name(&self) -> &str {
        "voice"
    }

    fn description(&self) -> &str {
        "Toggle voice mode"
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_DEFERRED_SLASH_VOICE") && is_voice_mode_available(ctx)
    }

    fn is_hidden(&self) -> bool {
        false
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check configured backend access and kill-switch before allowing voice mode
        if !is_voice_mode_available(ctx) {
            if ctx.is_custom_backend {
                return Ok(CommandResult::Text(
                    "Voice mode is not available. Set MOSSEN_CODE_CUSTOM_VOICE_BASE_URL or \
                     VOICE_STREAM_BASE_URL to enable voice mode with your custom backend."
                        .to_string(),
                ));
            }
            return Ok(CommandResult::Text(
                "Voice mode is not available.".to_string(),
            ));
        }

        let currently_enabled = is_voice_enabled(ctx);

        // Toggle OFF — no checks needed
        if currently_enabled {
            // In production: updateSettingsForSource('userSettings', { voiceEnabled: false })
            return Ok(CommandResult::Text("Voice mode disabled.".to_string()));
        }

        // Toggle ON — run pre-flight checks first

        // Check for voice stream backend
        if !is_voice_stream_available(ctx) {
            let msg = if ctx.is_custom_backend {
                "Voice mode backend is not reachable. Set VOICE_STREAM_BASE_URL or \
                 MOSSEN_CODE_CUSTOM_VOICE_BASE_URL to a speech-to-text endpoint that supports \
                 /api/ws/speech_to_text/voice_stream."
            } else {
                "Voice mode requires a configured Mossen voice backend or explicit \
                 hosted voice adapter credentials."
            };
            return Ok(CommandResult::Text(msg.to_string()));
        }

        // Check for recording tools
        let (deps_available, install_command) = check_voice_dependencies();
        if !deps_available {
            let hint = match install_command {
                Some(cmd) => format!("\nInstall audio recording tools? Run: {}", cmd),
                None => "\nInstall SoX manually for audio recording.".to_string(),
            };
            return Ok(CommandResult::Text(format!(
                "No audio recording tool found.{}",
                hint
            )));
        }

        // All checks passed — enable voice
        // In production: updateSettingsForSource('userSettings', { voiceEnabled: true })
        let key = get_shortcut_display();
        let (stt_code, fell_back_from) = normalize_language_for_stt(ctx);

        let lang_note;
        if let Some(original) = fell_back_from {
            lang_note = format!(
                " Note: \"{}\" is not a supported dictation language; using English. Change it via /config.",
                original
            );
        } else {
            lang_note = format!(" Dictation language: {} (/config to change).", stt_code);
        }

        Ok(CommandResult::Text(format!(
            "Voice mode enabled. Hold {} to record.{}",
            key, lang_note
        )))
    }
}
