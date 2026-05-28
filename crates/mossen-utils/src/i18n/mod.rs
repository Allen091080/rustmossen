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

fn normalize_language_tag(value: Option<&str>) -> Option<InteractiveLanguageTag> {
    match crate::ui_language::normalize_language_preference(value)? {
        crate::ui_language::InteractiveLanguageTag::En => Some(InteractiveLanguageTag::En),
        crate::ui_language::InteractiveLanguageTag::Zh => Some(InteractiveLanguageTag::Zh),
    }
}

fn get_runtime_language_tag_from_env() -> Option<InteractiveLanguageTag> {
    ["MOSSEN_LANG", "MOSSEN_LANGUAGE"]
        .iter()
        .find_map(|key| normalize_language_tag(std::env::var(key).ok().as_deref()))
}

/// Get the current interactive language tag.
fn get_interactive_language_tag() -> InteractiveLanguageTag {
    if let Some(tag) = get_runtime_language_tag_from_env() {
        return tag;
    }

    let config = crate::config::get_global_config();
    normalize_language_tag(config.interactive_language_preference.as_deref())
        .or_else(|| normalize_language_tag(config.last_interactive_language_tag.as_deref()))
        .unwrap_or(InteractiveLanguageTag::En)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    struct EnvRestore {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvRestore {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
            crate::config::_reset_global_config_cache_for_testing();
        }
    }

    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("i18n env test lock poisoned")
    }

    #[test]
    fn i18n_uses_persisted_interactive_language_preference() {
        let _lock = env_test_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _config_dir =
            EnvRestore::set("MOSSEN_CONFIG_DIR", temp.path().to_string_lossy().as_ref());
        let _lang = EnvRestore::remove("MOSSEN_LANG");
        let _language = EnvRestore::remove("MOSSEN_LANGUAGE");
        std::fs::write(
            temp.path().join(".mossen.json"),
            r#"{"interactiveLanguagePreference":"zh"}"#,
        )
        .expect("write config");
        crate::config::_reset_global_config_cache_for_testing();

        assert_eq!(t("lang.preference.auto", None, None), "自动");
    }

    #[test]
    fn i18n_env_language_overrides_persisted_preference() {
        let _lock = env_test_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _config_dir =
            EnvRestore::set("MOSSEN_CONFIG_DIR", temp.path().to_string_lossy().as_ref());
        let _lang = EnvRestore::set("MOSSEN_LANG", "en");
        let _language = EnvRestore::remove("MOSSEN_LANGUAGE");
        std::fs::write(
            temp.path().join(".mossen.json"),
            r#"{"interactiveLanguagePreference":"zh"}"#,
        )
        .expect("write config");
        crate::config::_reset_global_config_cache_for_testing();

        assert_eq!(t("lang.preference.auto", None, None), "Auto");
    }
}
