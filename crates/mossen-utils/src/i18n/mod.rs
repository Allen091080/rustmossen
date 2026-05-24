//! Internationalization (i18n) module — translated from utils/i18n/

mod strings_en;
mod strings_zh;

pub use strings_en::STRINGS_EN;
pub use strings_zh::STRINGS_ZH;

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Supported interactive language tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLanguageTag {
    En,
    Zh,
}

/// All valid i18n key strings (derived from STRINGS_EN keys)
pub type I18nKey = &'static str;

static PLACEHOLDER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{(\w+)\}").unwrap());

/// Format a template string by replacing `{name}` placeholders with values
fn format_template(template: &str, params: &HashMap<String, String>) -> String {
    PLACEHOLDER_RE
        .replace_all(template, |caps: &regex::Captures| {
            let name = &caps[1];
            params
                .get(name)
                .cloned()
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
}

/// Get the current interactive language tag (stub - should be connected to config)
fn get_interactive_language_tag() -> InteractiveLanguageTag {
    // Default to English; in production this reads from user config
    if std::env::var("MOSSEN_LANG").as_deref() == Ok("zh") {
        InteractiveLanguageTag::Zh
    } else {
        InteractiveLanguageTag::En
    }
}

/// Localized lookup; falls back to en, then to key string itself.
/// Missing keys appear as the literal key in UI.
pub fn t(
    key: I18nKey,
    params: Option<&HashMap<String, String>>,
    lang_override: Option<InteractiveLanguageTag>,
) -> String {
    let tag = lang_override.unwrap_or_else(get_interactive_language_tag);
    let tmpl = match tag {
        InteractiveLanguageTag::Zh => STRINGS_ZH
            .get(key)
            .copied()
            .or_else(|| STRINGS_EN.get(key).copied()),
        InteractiveLanguageTag::En => STRINGS_EN.get(key).copied(),
    };

    match tmpl {
        Some(template) => {
            if let Some(p) = params {
                format_template(template, p)
            } else {
                template.to_string()
            }
        }
        None => {
            if std::env::var("MOSSEN_DEBUG_I18N").is_ok() {
                eprintln!("[i18n] missing key: {}", key);
            }
            key.to_string()
        }
    }
}

/// Runtime predicate: checks if a key exists in the EN table
pub fn has_i18n_key(key: &str) -> bool {
    STRINGS_EN.contains_key(key)
}

/// Diagnostic: returns keys missing from zh or en tables
pub fn get_missing_i18n_keys() -> MissingKeys {
    let en_keys: Vec<&str> = STRINGS_EN.keys().copied().collect();
    let zh_keys: Vec<&str> = STRINGS_ZH.keys().copied().collect();

    let missing_in_zh: Vec<String> = en_keys
        .iter()
        .filter(|k| !STRINGS_ZH.contains_key(**k))
        .map(|k| k.to_string())
        .collect();

    let missing_in_en: Vec<String> = zh_keys
        .iter()
        .filter(|k| !STRINGS_EN.contains_key(**k))
        .map(|k| k.to_string())
        .collect();

    MissingKeys {
        missing_in_zh,
        missing_in_en,
    }
}

/// Result of missing key diagnostic
#[derive(Debug)]
pub struct MissingKeys {
    pub missing_in_zh: Vec<String>,
    pub missing_in_en: Vec<String>,
}
