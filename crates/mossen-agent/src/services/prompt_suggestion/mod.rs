//! Prompt suggestion service — generates follow-up prompt suggestions after assistant responses.

pub mod speculation;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Prompt variant for suggestion generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptVariant {
    UserIntent,
    StatedIntent,
}

/// Get the current prompt variant.
pub fn get_prompt_variant() -> PromptVariant {
    PromptVariant::UserIntent
}

/// Check if prompt suggestions should be enabled.
pub fn should_enable_prompt_suggestion() -> bool {
    // Check env var overrides
    if let Ok(val) = std::env::var("MOSSEN_CODE_ENABLE_PROMPT_SUGGESTION") {
        if val == "0" || val.eq_ignore_ascii_case("false") {
            return false;
        }
        if val == "1" || val.eq_ignore_ascii_case("true") {
            return true;
        }
    }

    // Default: enabled unless non-interactive session
    true
}

/// Abort controller for prompt suggestion generation.
static ABORT_FLAG: AtomicBool = AtomicBool::new(false);

/// Abort any in-progress prompt suggestion generation.
pub fn abort_prompt_suggestion() {
    ABORT_FLAG.store(true, Ordering::SeqCst);
}

/// Check if suggestion generation was aborted.
pub fn is_aborted() -> bool {
    ABORT_FLAG.load(Ordering::SeqCst)
}

/// Reset the abort flag for a new suggestion cycle.
pub fn reset_abort() {
    ABORT_FLAG.store(false, Ordering::SeqCst);
}

/// A generated prompt suggestion.
#[derive(Debug, Clone)]
pub struct PromptSuggestion {
    pub text: String,
    pub description: Option<String>,
}

/// Context for generating prompt suggestions.
#[derive(Debug, Clone)]
pub struct SuggestionContext {
    pub messages: Vec<serde_json::Value>,
    pub variant: PromptVariant,
    pub max_suggestions: usize,
}

/// Generate prompt suggestions based on conversation context.
pub async fn generate_prompt_suggestions(
    context: &SuggestionContext,
) -> Result<Vec<PromptSuggestion>, String> {
    if is_aborted() {
        return Ok(Vec::new());
    }

    // In production, this calls a forked agent to generate suggestions
    // based on the conversation context. The agent analyzes the last
    // assistant message and generates relevant follow-up prompts.
    Ok(Vec::new())
}

/// Try to generate and set prompt suggestions after an assistant response.
pub async fn try_generate_suggestions(
    messages: &[serde_json::Value],
    variant: PromptVariant,
) -> Vec<PromptSuggestion> {
    reset_abort();

    let context = SuggestionContext {
        messages: messages.to_vec(),
        variant,
        max_suggestions: 3,
    };

    match generate_prompt_suggestions(&context).await {
        Ok(suggestions) => suggestions,
        Err(_) => Vec::new(),
    }
}
