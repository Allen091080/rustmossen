//! API key verification hook (useApiKeyVerification.ts).
//!
//! Manages the flow for verifying an API key: tracks the key value,
//! loading state, verification result, and error messages.

use std::time::Instant;

/// Verification status for an API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyVerificationStatus {
    Idle,
    Verifying,
    Valid,
    Invalid { reason: String },
    Error { message: String },
}

/// State for API key verification.
#[derive(Debug, Clone)]
pub struct ApiKeyVerificationState {
    pub api_key: String,
    pub status: ApiKeyVerificationStatus,
    pub last_verified_at: Option<Instant>,
    pub attempts: u32,
}

impl ApiKeyVerificationState {
    pub fn new() -> Self {
        Self {
            api_key: String::new(),
            status: ApiKeyVerificationStatus::Idle,
            last_verified_at: None,
            attempts: 0,
        }
    }

    pub fn set_key(&mut self, key: String) {
        self.api_key = key;
        self.status = ApiKeyVerificationStatus::Idle;
    }

    pub fn start_verification(&mut self) {
        self.status = ApiKeyVerificationStatus::Verifying;
        self.attempts += 1;
    }

    pub fn mark_valid(&mut self) {
        self.status = ApiKeyVerificationStatus::Valid;
        self.last_verified_at = Some(Instant::now());
    }

    pub fn mark_invalid(&mut self, reason: String) {
        self.status = ApiKeyVerificationStatus::Invalid { reason };
        self.last_verified_at = Some(Instant::now());
    }

    pub fn mark_error(&mut self, message: String) {
        self.status = ApiKeyVerificationStatus::Error { message };
    }

    pub fn is_verifying(&self) -> bool {
        self.status == ApiKeyVerificationStatus::Verifying
    }

    pub fn is_valid(&self) -> bool {
        self.status == ApiKeyVerificationStatus::Valid
    }

    pub fn needs_verification(&self) -> bool {
        !self.api_key.is_empty() && self.status == ApiKeyVerificationStatus::Idle
    }
}

impl Default for ApiKeyVerificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Status reported by the verification hook to its caller.
///
/// TS source: `VerificationStatus = 'loading' | 'valid' | 'invalid' |
/// 'missing' | 'error'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Loading,
    Valid,
    Invalid,
    Missing,
    Error,
}

/// Result returned by the verification hook.
///
/// TS source: `export type ApiKeyVerificationResult`.
#[derive(Debug, Clone)]
pub struct ApiKeyVerificationResult {
    pub status: VerificationStatus,
    pub error: Option<String>,
}

/// Inputs needed to compute the initial verification status without
/// touching disk-bound async helpers. Translated from the gates the TS
/// version walks in `getInitialApiKeyVerificationStatus()`.
#[derive(Debug, Clone)]
pub struct InitialApiKeyVerificationInputs<'a> {
    pub custom_backend_enabled: bool,
    pub has_custom_backend_auth: bool,
    pub mossen_hosted_auth_enabled: bool,
    pub is_hosted_subscriber: bool,
    pub configured_key: Option<&'a str>,
    pub key_source: Option<&'a str>,
}

/// Compute the initial status. Mirrors `getInitialApiKeyVerificationStatus()`.
///
/// TS source: `getInitialApiKeyVerificationStatus()`.
pub fn get_initial_api_key_verification_status(
    inputs: &InitialApiKeyVerificationInputs<'_>,
) -> VerificationStatus {
    if inputs.custom_backend_enabled {
        return if inputs.has_custom_backend_auth {
            VerificationStatus::Valid
        } else {
            VerificationStatus::Missing
        };
    }
    if !inputs.mossen_hosted_auth_enabled || inputs.is_hosted_subscriber {
        return VerificationStatus::Valid;
    }
    let has_key = inputs
        .configured_key
        .map(|k| !k.is_empty())
        .unwrap_or(false);
    let needs_helper = inputs.key_source == Some("apiKeyHelper");
    if has_key || needs_helper {
        return VerificationStatus::Loading;
    }
    VerificationStatus::Missing
}
