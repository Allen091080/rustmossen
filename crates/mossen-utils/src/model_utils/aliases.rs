//! Model alias definitions and helpers.
//!
//! Direct translation of `utils/model/aliases.ts`.

/// Canonical list of model aliases the user can pass to `--model` or via
/// `/model`. Strings here are the raw TS string literals; capability semantics
/// (e.g. the `[1m]` suffix) are handled by other modules.
pub const MODEL_ALIASES: &[&str] = &[
    "sonnet",
    "opus",
    "haiku",
    "best",
    "sonnet[1m]",
    "opus[1m]",
    "opusplan",
];

/// Strongly-typed representation of a model alias. The TS source uses a string
/// literal union; here we reuse it as a Rust enum and provide round-trip
/// helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelAlias {
    Sonnet,
    Opus,
    Haiku,
    Best,
    Sonnet1M,
    Opus1M,
    OpusPlan,
}

impl ModelAlias {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelAlias::Sonnet => "sonnet",
            ModelAlias::Opus => "opus",
            ModelAlias::Haiku => "haiku",
            ModelAlias::Best => "best",
            ModelAlias::Sonnet1M => "sonnet[1m]",
            ModelAlias::Opus1M => "opus[1m]",
            ModelAlias::OpusPlan => "opusplan",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sonnet" => Some(ModelAlias::Sonnet),
            "opus" => Some(ModelAlias::Opus),
            "haiku" => Some(ModelAlias::Haiku),
            "best" => Some(ModelAlias::Best),
            "sonnet[1m]" => Some(ModelAlias::Sonnet1M),
            "opus[1m]" => Some(ModelAlias::Opus1M),
            "opusplan" => Some(ModelAlias::OpusPlan),
            _ => None,
        }
    }
}

/// `isModelAlias` — case-sensitive membership test against [`MODEL_ALIASES`].
pub fn is_model_alias(model_input: &str) -> bool {
    MODEL_ALIASES.contains(&model_input)
}

/// Bare model family aliases that act as wildcards in the availableModels
/// allowlist. When "opus" is in the allowlist, any opus model is allowed;
/// a specific model ID in the allowlist allows only that exact version.
pub const MODEL_FAMILY_ALIASES: &[&str] = &["sonnet", "opus", "haiku"];

/// `isModelFamilyAlias` — case-sensitive membership test.
pub fn is_model_family_alias(model: &str) -> bool {
    MODEL_FAMILY_ALIASES.contains(&model)
}
