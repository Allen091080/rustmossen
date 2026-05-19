// voice.rs — Translation of voice/voiceModeEnabled.ts

/// Kill-switch check for voice mode. Returns true unless the
/// tengu_amber_quartz_disabled GrowthBook flag is flipped on.
pub fn is_voice_growthbook_enabled() -> bool {
    // Feature-gated: VOICE_MODE bundle flag
    // In Rust build, voice mode availability is controlled by compile features
    #[cfg(feature = "voice_mode")]
    {
        // In production, check GrowthBook flag
        // Default: enabled unless kill-switch is active
        true
    }
    #[cfg(not(feature = "voice_mode"))]
    {
        false
    }
}

/// Auth-only check for voice mode. Returns true when the user has a
/// configured custom voice backend or an explicit hosted voice adapter token.
pub fn has_voice_auth() -> bool {
    // Check custom voice backend
    if is_custom_voice_enabled() {
        return true;
    }

    // Hosted voice is an explicit external adapter path.
    if !is_mossen_hosted_auth_enabled() {
        return false;
    }

    // Check for OAuth tokens
    has_hosted_oauth_token()
}

/// Full runtime check: auth + GrowthBook kill-switch.
pub fn is_voice_mode_enabled() -> bool {
    has_voice_auth() && is_voice_growthbook_enabled()
}

fn is_custom_voice_enabled() -> bool {
    std::env::var("MOSSEN_CODE_CUSTOM_VOICE_ENDPOINT")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn is_mossen_hosted_auth_enabled() -> bool {
    // Check if using Mossen hosted auth
    std::env::var("MOSSEN_CODE_AUTH_PROVIDER")
        .map(|v| v == "mossen")
        .unwrap_or(false)
}

fn has_hosted_oauth_token() -> bool {
    std::env::var("MOSSEN_CODE_OAUTH_TOKEN")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}
