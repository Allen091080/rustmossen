//! Model validation against the live API.
//!
//! Direct translation of `utils/model/validateModel.ts`. The TypeScript source
//! lives next to `sideQuery` (in `mossen-agent`); since this crate sits one
//! layer below, the actual API probe is passed in as a closure. We provide a
//! local error-shape trait that callers can implement against their SDK
//! errors so the structured error handling stays here.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::custom_backend::is_custom_backend_enabled;

use super::aliases::MODEL_ALIASES;
use super::model::get_canonical_name;
use super::model_allowlist::is_model_allowed;
use super::model_strings::get_model_strings;
use super::providers::{get_api_provider, APIProvider};

/// Result of a model validation. Mirrors the TS `{ valid, error? }` shape.
#[derive(Debug, Clone)]
pub struct ModelValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

impl ModelValidationResult {
    pub fn ok() -> Self {
        Self {
            valid: true,
            error: None,
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self {
            valid: false,
            error: Some(msg.into()),
        }
    }
}

/// Error shape used by [`validate_model`] when interpreting a failure from the
/// caller-supplied probe. We can't depend on `mossen-agent`'s `MossenAPIError`
/// directly, so we ask the caller to lower its error into this enum.
#[derive(Debug, Clone)]
pub enum ProbeError {
    /// HTTP-level error from the Mossen API. `status` is the response code,
    /// `message` is the error display string, and `not_found_body` flags the
    /// response payload `{"type":"not_found_error","message": "...model..."}`.
    Api {
        status: u16,
        message: String,
        not_found_model_body: bool,
    },
    /// Network/transport-level error.
    Connection { message: String },
    /// Anything else.
    Other { message: String },
}

fn valid_model_cache() -> &'static Mutex<HashSet<String>> {
    static CACHE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Validate a model by attempting a real API probe. The probe is supplied by
/// the caller (typically `sideQuery` in `mossen-agent`).
pub async fn validate_model<F, Fut>(model: &str, probe: F) -> ModelValidationResult
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), ProbeError>>,
{
    let normalized_model = model.trim().to_string();

    if normalized_model.is_empty() {
        return ModelValidationResult::invalid("Model name cannot be empty");
    }

    if !is_model_allowed(&normalized_model) {
        return ModelValidationResult::invalid(format!(
            "Model '{}' is not in the list of available models",
            normalized_model
        ));
    }

    let lower_model = normalized_model.to_lowercase();
    if MODEL_ALIASES.contains(&lower_model.as_str()) {
        return ModelValidationResult::ok();
    }

    if let Ok(custom_option) = std::env::var("MOSSEN_CODE_CUSTOM_MODEL_OPTION") {
        if !custom_option.is_empty() && normalized_model == custom_option {
            return ModelValidationResult::ok();
        }
    }

    if valid_model_cache()
        .lock()
        .unwrap()
        .contains(&normalized_model)
    {
        return ModelValidationResult::ok();
    }

    match probe(normalized_model.clone()).await {
        Ok(()) => {
            valid_model_cache()
                .lock()
                .unwrap()
                .insert(normalized_model.clone());
            ModelValidationResult::ok()
        }
        Err(error) => handle_validation_error(error, &normalized_model),
    }
}

fn handle_validation_error(error: ProbeError, model_name: &str) -> ModelValidationResult {
    match error {
        ProbeError::Api {
            status: 404,
            not_found_model_body,
            ..
        } => {
            let fallback = get_3p_fallback_suggestion(model_name);
            let suggestion = match (not_found_model_body, fallback.as_deref()) {
                (_, Some(f)) => format!(". Try '{}' instead", f),
                _ => String::new(),
            };
            ModelValidationResult::invalid(format!(
                "Model '{}' not found{}",
                model_name, suggestion
            ))
        }
        ProbeError::Api { status: 401, .. } => ModelValidationResult::invalid(
            "Authentication failed. Please check your API credentials.",
        ),
        ProbeError::Api {
            not_found_model_body: true,
            ..
        } => ModelValidationResult::invalid(format!("Model '{}' not found", model_name)),
        ProbeError::Api { message, .. } => {
            ModelValidationResult::invalid(format!("API error: {}", message))
        }
        ProbeError::Connection { .. } => ModelValidationResult::invalid(
            "Network error. Please check your internet connection.",
        ),
        ProbeError::Other { message } => {
            ModelValidationResult::invalid(format!("Unable to validate model: {}", message))
        }
    }
}

fn get_3p_fallback_suggestion(model: &str) -> Option<String> {
    if !is_custom_backend_enabled() && get_api_provider() == APIProvider::FirstParty {
        return None;
    }
    let canonical = get_canonical_name(&model.to_lowercase().replace('_', "-"));
    let ms = get_model_strings();
    if canonical.contains("mossen-max-4-6") {
        return Some(ms.max41);
    }
    if canonical.contains("mossen-balanced-4-6") {
        return Some(ms.balanced45);
    }
    if canonical.contains("mossen-balanced-4-5") {
        return Some(ms.balanced40);
    }
    None
}

/// Helper for tests / callers that want a synchronous "probe never runs"
/// shortcut — useful for unit-testing fallback messaging.
pub fn validate_model_offline(model: &str) -> ModelValidationResult {
    let normalized_model = model.trim().to_string();
    if normalized_model.is_empty() {
        return ModelValidationResult::invalid("Model name cannot be empty");
    }
    if !is_model_allowed(&normalized_model) {
        return ModelValidationResult::invalid(format!(
            "Model '{}' is not in the list of available models",
            normalized_model
        ));
    }
    let lower_model = normalized_model.to_lowercase();
    if MODEL_ALIASES.contains(&lower_model.as_str()) {
        return ModelValidationResult::ok();
    }
    if let Ok(custom_option) = std::env::var("MOSSEN_CODE_CUSTOM_MODEL_OPTION") {
        if !custom_option.is_empty() && normalized_model == custom_option {
            return ModelValidationResult::ok();
        }
    }
    if valid_model_cache()
        .lock()
        .unwrap()
        .contains(&normalized_model)
    {
        return ModelValidationResult::ok();
    }
    ModelValidationResult::ok()
}
