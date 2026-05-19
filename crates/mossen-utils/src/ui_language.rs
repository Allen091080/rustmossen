//! UI language detection and switching.
//!
//! Manages interactive language preferences (en/zh), detects language from text,
//! and provides persistence and localization helpers.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use std::path::Path;

/// Supported interactive language tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLanguageTag {
    En,
    Zh,
}

impl InteractiveLanguageTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            InteractiveLanguageTag::En => "en",
            InteractiveLanguageTag::Zh => "zh",
        }
    }
}

/// Global observed interactive language tag.
static OBSERVED_LANGUAGE_TAG: Mutex<Option<InteractiveLanguageTag>> = Mutex::new(None);

/// Trait for config file operations (abstraction for testing).
pub trait ConfigFileOps {
    fn read_file_sync(&self, path: &str) -> Result<String, std::io::Error>;
    fn save_global_config(&self, updater: Box<dyn FnOnce(serde_json::Value) -> serde_json::Value>);
}

/// Read persisted settings language preference from global config file.
pub fn read_persisted_settings_language_preference(config_path: &str) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let content = strip_bom(&content);
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    parsed
        .get("language")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read persisted interactive language preference setting.
pub fn read_persisted_interactive_language_preference_setting(
    config_path: &str,
) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let content = strip_bom(&content);
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    parsed
        .get("interactiveLanguagePreference")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read persisted interactive language tag (lastInteractiveLanguageTag).
pub fn read_persisted_interactive_language_preference(config_path: &str) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let content = strip_bom(&content);
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    parsed
        .get("lastInteractiveLanguageTag")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Get persisted interactive language tag.
pub fn get_persisted_interactive_language_tag(
    config_path: &str,
) -> Option<InteractiveLanguageTag> {
    normalize_language_preference(
        read_persisted_interactive_language_preference(config_path).as_deref(),
    )
}

/// Get configured interactive language tag from settings.
pub fn get_configured_interactive_language_tag_internal(
    config_path: &str,
) -> Option<InteractiveLanguageTag> {
    normalize_language_preference(
        read_persisted_interactive_language_preference_setting(config_path).as_deref(),
    )
}

/// Normalize a language preference string to a tag.
pub fn normalize_language_preference(value: Option<&str>) -> Option<InteractiveLanguageTag> {
    let value = value?;
    let lower = value.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }

    if lower == "zn"
        || lower == "cn"
        || lower.starts_with("zh")
        || lower.contains("中文")
        || lower.contains("汉语")
        || lower.contains("漢語")
        || lower.contains("简体")
        || lower.contains("繁体")
        || lower.contains("繁體")
        || lower.contains("chinese")
        || lower.contains("mandarin")
    {
        return Some(InteractiveLanguageTag::Zh);
    }

    if lower == "en"
        || lower.starts_with("en-")
        || lower.starts_with("en_")
        || lower.contains("english")
        || lower.contains("英文")
        || lower.contains("英语")
        || lower.contains("英語")
    {
        return Some(InteractiveLanguageTag::En);
    }

    if lower == "default"
        || lower == "auto"
        || lower == "system"
        || lower == "follow"
        || lower == "automatic"
    {
        return None;
    }

    None
}

/// Convert language preference to persisted form.
pub fn to_persisted_language_preference(value: Option<&str>) -> Option<String> {
    let tag = normalize_language_preference(value)?;
    Some(match tag {
        InteractiveLanguageTag::Zh => "中文".to_string(),
        InteractiveLanguageTag::En => "English".to_string(),
    })
}

/// Regex for CJK character detection.
static CJK_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\p{Han}\x{3000}-\x{303f}\x{ff00}-\x{ffef}]").unwrap()
});

/// Regex for Latin words.
static LATIN_WORDS_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[A-Za-z]+").unwrap());

/// Infer interactive language tag from text content.
pub fn infer_interactive_language_tag_from_text(
    value: Option<&str>,
) -> Option<InteractiveLanguageTag> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if CJK_REGEX.is_match(trimmed) {
        return Some(InteractiveLanguageTag::Zh);
    }

    let latin_words: Vec<&str> = LATIN_WORDS_REGEX
        .find_iter(trimmed)
        .map(|m| m.as_str())
        .collect();
    if latin_words.len() < 2 {
        return None;
    }

    let total_latin_len: usize = latin_words.iter().map(|w| w.len()).sum();
    if total_latin_len >= 8 {
        Some(InteractiveLanguageTag::En)
    } else {
        None
    }
}

/// Observe interactive language from text and persist if detected.
pub fn observe_interactive_language(
    value: Option<&str>,
    config_path: Option<&str>,
) -> Option<InteractiveLanguageTag> {
    let inferred = infer_interactive_language_tag_from_text(value);
    if let Some(tag) = inferred {
        *OBSERVED_LANGUAGE_TAG.lock() = Some(tag);
        if let Some(path) = config_path {
            persist_observed_interactive_language_tag(tag, path);
        }
    }
    inferred
}

/// Persist the observed interactive language tag to config.
fn persist_observed_interactive_language_tag(tag: InteractiveLanguageTag, config_path: &str) {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(config_path)?;
        let content = strip_bom(&content);
        let mut parsed: serde_json::Value = serde_json::from_str(content)?;
        if let Some(obj) = parsed.as_object_mut() {
            let current = obj
                .get("lastInteractiveLanguageTag")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if current == tag.as_str() {
                return Ok(());
            }
            obj.insert(
                "lastInteractiveLanguageTag".to_string(),
                serde_json::Value::String(tag.as_str().to_string()),
            );
        }
        std::fs::write(config_path, serde_json::to_string_pretty(&parsed)?)?;
        Ok(())
    })();
    // Best-effort persistence, ignore errors
    let _ = result;
}

/// Set interactive language preference in config.
pub fn set_interactive_language_preference(
    tag: Option<InteractiveLanguageTag>,
    config_path: &str,
) {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(config_path).unwrap_or_else(|_| "{}".to_string());
        let content_str = strip_bom(&content);
        let mut parsed: serde_json::Value =
            serde_json::from_str(content_str).unwrap_or(serde_json::json!({}));
        if let Some(obj) = parsed.as_object_mut() {
            match tag {
                Some(t) => {
                    obj.insert(
                        "interactiveLanguagePreference".to_string(),
                        serde_json::Value::String(t.as_str().to_string()),
                    );
                    obj.insert(
                        "lastInteractiveLanguageTag".to_string(),
                        serde_json::Value::String(t.as_str().to_string()),
                    );
                }
                None => {
                    obj.insert(
                        "interactiveLanguagePreference".to_string(),
                        serde_json::Value::Null,
                    );
                }
            }
        }
        std::fs::write(config_path, serde_json::to_string_pretty(&parsed)?)?;
        Ok(())
    })();
    let _ = result;
}

/// Reset observed interactive language for testing.
pub fn reset_observed_interactive_language_for_testing() {
    *OBSERVED_LANGUAGE_TAG.lock() = None;
}

/// Get the effective interactive language tag.
pub fn get_interactive_language_tag(
    config_path: &str,
    system_locale: Option<&str>,
) -> InteractiveLanguageTag {
    if let Some(configured) = get_configured_interactive_language_tag_internal(config_path) {
        return configured;
    }

    if let Some(observed) = *OBSERVED_LANGUAGE_TAG.lock() {
        return observed;
    }

    if let Some(persisted) = get_persisted_interactive_language_tag(config_path) {
        return persisted;
    }

    if let Some(system) = normalize_language_preference(system_locale) {
        return system;
    }

    InteractiveLanguageTag::En
}

/// Get display name for a language tag.
pub fn get_interactive_language_display_name(tag: InteractiveLanguageTag) -> &'static str {
    match tag {
        InteractiveLanguageTag::Zh => "中文",
        InteractiveLanguageTag::En => "English",
    }
}

/// Get footer label for the current language.
pub fn get_interactive_language_footer_label(
    config_path: &str,
    system_locale: Option<&str>,
) -> String {
    let effective = get_interactive_language_tag(config_path, system_locale);
    let label = get_interactive_language_display_name(effective);

    if get_configured_interactive_language_tag_internal(config_path).is_some() {
        return label.to_string();
    }

    get_localized_text(
        &format!("{} (auto)", label),
        &format!("{}（自动）", label),
        config_path,
        system_locale,
    )
}

/// Get localized text based on current language setting.
pub fn get_localized_text(
    en: &str,
    zh: &str,
    config_path: &str,
    system_locale: Option<&str>,
) -> String {
    let tag = get_interactive_language_tag(config_path, system_locale);
    match tag {
        InteractiveLanguageTag::Zh => zh.to_string(),
        InteractiveLanguageTag::En => en.to_string(),
    }
}

/// Get the interactive language preference (display text for UI).
pub fn get_interactive_language_preference(
    config_path: &str,
    configured_language: Option<&str>,
    system_locale: Option<&str>,
) -> Option<String> {
    if let Some(configured) = configured_language {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            let configured_tag = normalize_language_preference(Some(trimmed));
            let observed = *OBSERVED_LANGUAGE_TAG.lock();
            if let (Some(ct), Some(obs)) = (configured_tag, observed) {
                if obs != ct {
                    return Some(
                        get_interactive_language_display_name(obs).to_string(),
                    );
                }
            }
            return Some(match configured_tag {
                Some(t) => get_interactive_language_display_name(t).to_string(),
                None => trimmed.to_string(),
            });
        }
    }

    let persisted_configured = read_persisted_settings_language_preference(config_path);
    if let Some(ref pc) = persisted_configured {
        let trimmed = pc.trim();
        if !trimmed.is_empty() {
            let configured_tag = normalize_language_preference(Some(trimmed));
            let observed = *OBSERVED_LANGUAGE_TAG.lock();
            if let (Some(ct), Some(obs)) = (configured_tag, observed) {
                if obs != ct {
                    return Some(
                        get_interactive_language_display_name(obs).to_string(),
                    );
                }
            }
            return Some(match configured_tag {
                Some(t) => get_interactive_language_display_name(t).to_string(),
                None => trimmed.to_string(),
            });
        }
    }

    let observed = *OBSERVED_LANGUAGE_TAG.lock();
    if let Some(obs) = observed {
        return Some(get_interactive_language_display_name(obs).to_string());
    }

    let persisted = get_persisted_interactive_language_tag(config_path);
    persisted.map(|p| get_interactive_language_display_name(p).to_string())
}

/// Get configured interactive language tag (public API).
pub fn get_configured_interactive_language_tag(
    config_path: &str,
) -> Option<InteractiveLanguageTag> {
    get_configured_interactive_language_tag_internal(config_path)
}

/// Strip BOM (byte order mark) from the beginning of a string.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}
