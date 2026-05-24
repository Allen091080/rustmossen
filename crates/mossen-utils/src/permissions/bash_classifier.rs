//! Bash classifier stub for external builds.
//!
//! Translates `utils/permissions/bashClassifier.ts`.
//! Classifier permissions feature is INTERNAL-ONLY; this stub provides
//! the interface for external builds.

pub const PROMPT_PREFIX: &str = "prompt:";

/// Result from a classifier invocation.
#[derive(Debug, Clone)]
pub struct ClassifierResult {
    pub matches: bool,
    pub matched_description: Option<String>,
    pub confidence: ClassifierConfidence,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifierConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifierBehavior {
    Deny,
    Ask,
    Allow,
}

pub fn extract_prompt_description(_rule_content: Option<&str>) -> Option<String> {
    None
}

pub fn create_prompt_rule_content(description: &str) -> String {
    format!("{} {}", PROMPT_PREFIX, description.trim())
}

pub fn is_classifier_permissions_enabled() -> bool {
    false
}

pub fn get_bash_prompt_deny_descriptions(_context: &()) -> Vec<String> {
    Vec::new()
}

pub fn get_bash_prompt_ask_descriptions(_context: &()) -> Vec<String> {
    Vec::new()
}

pub fn get_bash_prompt_allow_descriptions(_context: &()) -> Vec<String> {
    Vec::new()
}

pub async fn classify_bash_command(
    _command: &str,
    _cwd: &str,
    _descriptions: &[String],
    _behavior: ClassifierBehavior,
    _signal: &tokio::sync::watch::Receiver<bool>,
    _is_non_interactive_session: bool,
) -> ClassifierResult {
    ClassifierResult {
        matches: false,
        matched_description: None,
        confidence: ClassifierConfidence::High,
        reason: "This feature is disabled".to_string(),
    }
}

pub async fn generate_generic_description(
    _command: &str,
    specific_description: Option<&str>,
    _signal: &tokio::sync::watch::Receiver<bool>,
) -> Option<String> {
    specific_description.map(|s| s.to_string())
}
