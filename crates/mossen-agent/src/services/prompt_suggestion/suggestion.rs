//! Prompt suggestion generation and filtering

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Prompt variant type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptVariant {
    UserIntent,
    StatedIntent,
}

impl PromptVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserIntent => "user_intent",
            Self::StatedIntent => "stated_intent",
        }
    }
}

/// Returns the current prompt variant
pub fn get_prompt_variant() -> PromptVariant {
    PromptVariant::UserIntent
}

/// Suppress reason for prompt suggestions
#[derive(Debug, Clone)]
pub enum SuppressReason {
    Disabled,
    PendingPermission,
    ElicitationActive,
    PlanMode,
    RateLimit,
    Aborted,
    EarlyConversation,
    LastResponseError,
    CacheCold,
    Empty,
}

impl SuppressReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::PendingPermission => "pending_permission",
            Self::ElicitationActive => "elicitation_active",
            Self::PlanMode => "plan_mode",
            Self::RateLimit => "rate_limit",
            Self::Aborted => "aborted",
            Self::EarlyConversation => "early_conversation",
            Self::LastResponseError => "last_response_error",
            Self::CacheCold => "cache_cold",
            Self::Empty => "empty",
        }
    }
}

/// App state check for suggestion suppression
pub trait SuggestionAppState {
    fn is_prompt_suggestion_enabled(&self) -> bool;
    fn has_pending_worker_request(&self) -> bool;
    fn has_pending_sandbox_request(&self) -> bool;
    fn elicitation_queue_len(&self) -> usize;
    fn is_plan_mode(&self) -> bool;
    fn is_rate_limited(&self) -> bool;
}

/// Check if suggestions should be suppressed
pub fn get_suggestion_suppress_reason(state: &dyn SuggestionAppState) -> Option<SuppressReason> {
    if !state.is_prompt_suggestion_enabled() {
        return Some(SuppressReason::Disabled);
    }
    if state.has_pending_worker_request() || state.has_pending_sandbox_request() {
        return Some(SuppressReason::PendingPermission);
    }
    if state.elicitation_queue_len() > 0 {
        return Some(SuppressReason::ElicitationActive);
    }
    if state.is_plan_mode() {
        return Some(SuppressReason::PlanMode);
    }
    if state.is_rate_limited() {
        return Some(SuppressReason::RateLimit);
    }
    None
}

/// Determine if feature should be enabled
pub fn should_enable_prompt_suggestion(
    env_override: Option<&str>,
    feature_gate: bool,
    is_non_interactive: bool,
    is_swarm_teammate: bool,
    settings_enabled: bool,
) -> bool {
    // Env var overrides everything
    if let Some(val) = env_override {
        let lower = val.to_lowercase();
        if lower == "0" || lower == "false" || lower == "no" {
            return false;
        }
        if lower == "1" || lower == "true" || lower == "yes" {
            return true;
        }
    }

    // Feature gate
    if !feature_gate {
        return false;
    }

    // Disable in non-interactive mode
    if is_non_interactive {
        return false;
    }

    // Disable for swarm teammates
    if is_swarm_teammate {
        return false;
    }

    settings_enabled
}

static CURRENT_ABORT: Lazy<Mutex<Option<CancellationToken>>> = Lazy::new(|| Mutex::new(None));

/// Abort the current prompt suggestion
pub fn abort_prompt_suggestion() {
    let mut guard = CURRENT_ABORT.lock();
    if let Some(token) = guard.take() {
        token.cancel();
    }
}

const MAX_PARENT_UNCACHED_TOKENS: u64 = 10_000;

/// Check if cache is cold (too many uncached tokens)
pub fn get_parent_cache_suppress_reason(
    input_tokens: u64,
    cache_write_tokens: u64,
    output_tokens: u64,
) -> Option<SuppressReason> {
    if input_tokens + cache_write_tokens + output_tokens > MAX_PARENT_UNCACHED_TOKENS {
        Some(SuppressReason::CacheCold)
    } else {
        None
    }
}

/// The suggestion prompt text
pub const SUGGESTION_PROMPT: &str = r#"[SUGGESTION MODE: Suggest what the user might naturally type next into Mossen.]

FIRST: Look at the user's recent messages and original request.

Your job is to predict what THEY would type - not what you think they should do.

THE TEST: Would they think "I was just about to type that"?

EXAMPLES:
User asked "fix the bug and run tests", bug is fixed → "run the tests"
After code written → "try it out"
Mossen offers options → suggest the one the user would likely pick, based on conversation
Mossen asks to continue → "yes" or "go ahead"
Task complete, obvious follow-up → "commit this" or "push it"
After error or misunderstanding → silence (let them assess/correct)

Be specific: "run the tests" beats "continue".

NEVER SUGGEST:
- Evaluative ("looks good", "thanks")
- Questions ("what about...?")
- Mossen-voice ("Let me...", "I'll...", "Here's...")
- New ideas they didn't ask about
- Multiple sentences

Stay silent if the next step isn't obvious from what the user said.

Format: 2-12 words, match the user's style. Or nothing.

Reply with ONLY the suggestion, no quotes or explanation."#;

/// Filter rules for suggestions
pub fn should_filter_suggestion(suggestion: &str, _prompt_id: PromptVariant) -> Option<&'static str> {
    if suggestion.is_empty() {
        return Some("empty");
    }

    let lower = suggestion.to_lowercase();
    let word_count = suggestion.split_whitespace().count();

    // "done" filter
    if lower == "done" {
        return Some("done");
    }

    // Meta text
    if lower == "nothing found"
        || lower == "nothing found."
        || lower.starts_with("nothing to suggest")
        || lower.starts_with("no suggestion")
    {
        return Some("meta_text");
    }

    // Silence patterns
    static RE_SILENCE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\bsilence is\b|\bstay(s|ing)? silent\b").unwrap());
    static RE_BARE_SILENCE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^\W*silence\W*$").unwrap());
    if RE_SILENCE.is_match(&lower) || RE_BARE_SILENCE.is_match(&lower) {
        return Some("meta_text");
    }

    // Meta wrapped in parens/brackets
    static RE_META_WRAPPED: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^\(.*\)$|^\[.*\]$").unwrap());
    if RE_META_WRAPPED.is_match(suggestion) {
        return Some("meta_wrapped");
    }

    // Error messages
    if lower.starts_with("api error:")
        || lower.starts_with("prompt is too long")
        || lower.starts_with("request timed out")
        || lower.starts_with("invalid api key")
        || lower.starts_with("image was too large")
    {
        return Some("error_message");
    }

    // Prefixed label
    static RE_PREFIXED: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\w+:\s").unwrap());
    if RE_PREFIXED.is_match(suggestion) {
        return Some("prefixed_label");
    }

    // Too few words
    if word_count < 2 {
        if !suggestion.starts_with('/') {
            static ALLOWED_SINGLE_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
                [
                    "yes", "yeah", "yep", "yea", "yup", "sure", "ok", "okay",
                    "push", "commit", "deploy", "stop", "continue", "check", "exit", "quit",
                    "no",
                ]
                .into_iter()
                .collect()
            });
            if !ALLOWED_SINGLE_WORDS.contains(lower.as_str()) {
                return Some("too_few_words");
            }
        }
    }

    // Too many words
    if word_count > 12 {
        return Some("too_many_words");
    }

    // Too long
    if suggestion.len() >= 100 {
        return Some("too_long");
    }

    // Multiple sentences
    static RE_MULTI_SENTENCE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"[.!?]\s+[A-Z]").unwrap());
    if RE_MULTI_SENTENCE.is_match(suggestion) {
        return Some("multiple_sentences");
    }

    // Has formatting
    if suggestion.contains('\n') || suggestion.contains('*') {
        return Some("has_formatting");
    }

    // Evaluative
    static RE_EVALUATIVE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"thanks|thank you|looks good|sounds good|that works|that worked|that's all|nice|great|perfect|makes sense|awesome|excellent").unwrap()
    });
    if RE_EVALUATIVE.is_match(&lower) {
        return Some("evaluative");
    }

    // Mossen voice
    static RE_MOSSEN_VOICE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^(let me|i'll|i've|i'm|i can|i would|i think|i notice|here's|here is|here are|that's|this is|this will|you can|you should|you could|sure,|of course|certainly)").unwrap()
    });
    if RE_MOSSEN_VOICE.is_match(suggestion) {
        return Some("mossen_voice");
    }

    None
}

/// Log suggestion outcome (acceptance or ignoring)
pub fn log_suggestion_outcome(
    suggestion: &str,
    user_input: &str,
    emitted_at: u64,
    _prompt_id: PromptVariant,
    _generation_request_id: Option<&str>,
) {
    let was_accepted = user_input == suggestion;
    let _time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        - emitted_at;
    let _similarity =
        (user_input.len() as f64 / suggestion.len().max(1) as f64 * 100.0).round() / 100.0;

    debug!(
        "prompt_suggestion outcome: {} (accepted={})",
        if was_accepted { "accepted" } else { "ignored" },
        was_accepted
    );
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/PromptSuggestion/promptSuggestion.ts` exports.
// ---------------------------------------------------------------------------

/// `promptSuggestion.ts` `tryGenerateSuggestion`.
pub async fn try_generate_suggestion(_context: serde_json::Value) -> Option<String> {
    None
}

/// `promptSuggestion.ts` `generateSuggestion`.
pub async fn generate_suggestion(_context: serde_json::Value) -> Option<String> {
    None
}

/// `promptSuggestion.ts` `logSuggestionSuppressed`.
pub fn log_suggestion_suppressed(reason: &str) {
    debug!(reason = reason, "log_suggestion_suppressed");
}
