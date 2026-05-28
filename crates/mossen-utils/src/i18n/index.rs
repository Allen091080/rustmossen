//! Mossen i18n facade — translated from utils/i18n/index.ts.
//!
//! Public API:
//!   - [`t`] — localized lookup; falls back to en, then to the key string itself
//!     (W1-D6 = A loud failure: missing keys appear as the literal key in UI)
//!   - [`has_i18n_key`] — runtime predicate (uses en table as authoritative)
//!   - [`get_missing_i18n_keys`] — diagnostic for guard scripts; pairs with
//!     scripts/i18n_self_check.py on the TS side.
//!
//! Migration policy:
//!   - New user-visible text MUST use [`t`]. Existing inline
//!     `getLocalizedText({en, zh})` calls remain as a compat layer. Slices
//!     that touch a file SHOULD migrate text in that file to [`t`] and append
//!     keys to strings_{en,zh}.rs.
//!
//! Dependency direction (do not break):
//!   strings_en.rs -> keys.rs -> strings_zh.rs -> index.rs -> ui_language.rs
//!
//! No file outside `crate::i18n` should import from `strings_en` / `strings_zh`
//! / `keys` directly. All consumers go through this module.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::i18n::keys::I18nKey;
use crate::i18n::strings_en::STRINGS_EN;
use crate::i18n::strings_zh::STRINGS_ZH;

/// Supported interactive language tags. Mirrors `InteractiveLanguageTag` from
/// `utils/uiLanguage.ts`; full enum lives in `crate::ui_language` but this
/// module re-declares its own copy to avoid a circular dependency between
/// `i18n` and `ui_language` during the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLanguageTag {
    En,
    Zh,
}

/// `{name}` placeholder regex, compiled once.
static PLACEHOLDER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(\w+)\}").expect("static placeholder regex must compile"));

/// Replace `{name}` placeholders in `template` using `params`. Missing
/// placeholders are preserved literally, matching the TS `format()` behavior
/// where `undefined` falls through to the original match.
fn format(template: &str, params: &HashMap<String, String>) -> String {
    PLACEHOLDER_RE
        .replace_all(template, |caps: &regex::Captures| {
            let name = &caps[1];
            match params.get(name) {
                Some(value) => value.clone(),
                None => caps[0].to_string(),
            }
        })
        .to_string()
}

/// Return the active interactive language tag.
///
/// On the TS side this delegates to `getInteractiveLanguageTag()` in
/// `utils/uiLanguage.ts`. During the port the canonical source lives in
/// `crate::ui_language`; we read the same `MOSSEN_LANG` env var here so that
/// `t()` is usable from the utils layer without a layering violation.
fn get_interactive_language_tag() -> InteractiveLanguageTag {
    match std::env::var("MOSSEN_LANG").as_deref() {
        Ok("zh") => InteractiveLanguageTag::Zh,
        _ => InteractiveLanguageTag::En,
    }
}

/// Localized lookup. Falls back to EN, then to the key string itself.
///
/// Mirrors the TS signature:
/// ```ts
/// t(key: I18nKey, params?: Record<string, string | number>,
///   langOverride?: InteractiveLanguageTag): string
/// ```
pub fn t(
    key: I18nKey,
    params: Option<&HashMap<String, String>>,
    lang_override: Option<InteractiveLanguageTag>,
) -> String {
    let tag = lang_override.unwrap_or_else(get_interactive_language_tag);
    let tmpl: Option<&'static str> = match tag {
        InteractiveLanguageTag::Zh => STRINGS_ZH
            .get(key)
            .copied()
            .or_else(|| STRINGS_EN.get(key).copied()),
        InteractiveLanguageTag::En => STRINGS_EN.get(key).copied(),
    };

    match tmpl {
        Some(template) => match params {
            Some(p) => format(template, p),
            None => template.to_string(),
        },
        None => {
            if std::env::var("MOSSEN_DEBUG_I18N").is_ok() {
                eprintln!("[i18n] missing key: {}", key);
            }
            key.to_string()
        }
    }
}

/// Runtime predicate. Returns `true` iff `key` is present in the EN table.
///
/// On the TS side this is a `key is I18nKey` type guard; in Rust we just
/// return `bool` since `I18nKey` is `&'static str`.
pub fn has_i18n_key(key: &str) -> bool {
    STRINGS_EN.contains_key(key)
}

/// Diagnostic result for guard scripts. Mirrors the TS return shape:
/// `{ missingInZh: I18nKey[]; missingInEn: string[] }`.
#[derive(Debug, Clone)]
pub struct MissingKeys {
    pub missing_in_zh: Vec<I18nKey>,
    pub missing_in_en: Vec<String>,
}

/// Returns the keys present in one table but missing from the other. Used by
/// `scripts/i18n_self_check.py` to enforce EN/ZH parity at build time.
pub fn get_missing_i18n_keys() -> MissingKeys {
    let missing_in_zh: Vec<I18nKey> = STRINGS_EN
        .keys()
        .copied()
        .filter(|k| !STRINGS_ZH.contains_key(k))
        .collect();

    let missing_in_en: Vec<String> = STRINGS_ZH
        .keys()
        .copied()
        .filter(|k| !STRINGS_EN.contains_key(k))
        .map(|k| k.to_string())
        .collect();

    MissingKeys {
        missing_in_zh,
        missing_in_en,
    }
}
