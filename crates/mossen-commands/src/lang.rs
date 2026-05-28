//! `/lang` — Switch display language preference.
//!
//! Translates `commands/lang/lang.tsx` (81 lines).
//! Manages the interactive language (en/zh) with toggle, set, and auto modes.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Supported language tags.
const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("zh", "中文")];

/// Resolve the language preference from user input.
fn resolve_language_command(args: &str) -> Option<Option<&str>> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return None; // Show usage
    }

    let lowered = trimmed.to_lowercase();
    if matches!(lowered.as_str(), "help" | "-h" | "--help") {
        return None; // Show usage
    }
    if matches!(lowered.as_str(), "toggle" | "switch") {
        // Toggle will be handled separately
        return Some(Some("toggle"));
    }
    if matches!(
        lowered.as_str(),
        "default" | "auto" | "system" | "follow" | "clear"
    ) {
        return Some(None); // Clear preference
    }

    // Try to match a language
    match lowered.as_str() {
        "en" | "english" => Some(Some("en")),
        "zh" | "中文" | "chinese" => Some(Some("zh")),
        _ => None, // Unknown, show usage
    }
}

fn normalize_language_tag(value: Option<&str>) -> Option<&'static str> {
    match mossen_utils::ui_language::normalize_language_preference(value)? {
        mossen_utils::ui_language::InteractiveLanguageTag::En => Some("en"),
        mossen_utils::ui_language::InteractiveLanguageTag::Zh => Some("zh"),
    }
}

fn display_name(lang: &str) -> &'static str {
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(tag, _)| *tag == lang)
        .map(|(_, name)| *name)
        .unwrap_or("English")
}

fn runtime_language_from_env() -> Option<&'static str> {
    ["MOSSEN_LANG", "MOSSEN_LANGUAGE"]
        .iter()
        .find_map(|key| normalize_language_tag(std::env::var(key).ok().as_deref()))
}

fn get_configured_language_preference() -> Option<String> {
    let config = mossen_utils::config::get_global_config();
    normalize_language_tag(config.interactive_language_preference.as_deref()).map(str::to_string)
}

fn get_current_language() -> String {
    if let Some(lang) = runtime_language_from_env() {
        return lang.to_string();
    }

    let config = mossen_utils::config::get_global_config();
    normalize_language_tag(config.interactive_language_preference.as_deref())
        .or_else(|| normalize_language_tag(config.last_interactive_language_tag.as_deref()))
        .unwrap_or("en")
        .to_string()
}

fn set_runtime_language(lang: Option<&str>) {
    if let Some(lang) = lang {
        std::env::set_var("MOSSEN_LANG", lang);
        std::env::set_var("MOSSEN_LANGUAGE", lang);
    } else {
        std::env::remove_var("MOSSEN_LANG");
        std::env::remove_var("MOSSEN_LANGUAGE");
    }
}

fn save_language_preference(lang: Option<&str>) {
    let lang = lang.map(str::to_string);
    mossen_utils::config::save_global_config(move |config| {
        let mut next = config.clone();
        next.interactive_language_preference = lang.clone();
        next.last_interactive_language_tag = lang.clone();
        next
    });
}

fn apply_language_preference(lang: Option<&str>) {
    save_language_preference(lang);
    set_runtime_language(lang);
}

/// Build the usage/status message.
fn build_usage_message(current_lang: &str, preference: Option<&str>) -> String {
    let preference = preference.map(display_name).unwrap_or("auto");
    format!(
        "Current language: {}\n\
         Preference: {}\n\n\
         Usage: /lang <language>\n\
         Shortcut: /lang toggle (switch between English and 中文)\n\
         Note: /lang auto or /lang clear to reset to system default.",
        display_name(current_lang),
        preference,
    )
}

/// `/lang` command.
pub struct LangDirective;

#[async_trait]
impl Directive for LangDirective {
    fn name(&self) -> &str {
        "lang"
    }

    fn description(&self) -> &str {
        "Switch display language"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[en|zh|toggle|auto]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let args_str = args.join(" ");
        let command = resolve_language_command(&args_str);
        let current = get_current_language();
        let preference = get_configured_language_preference();

        match command {
            None => Ok(CommandResult::System(build_usage_message(
                &current,
                preference.as_deref(),
            ))),

            Some(None) => {
                apply_language_preference(None);
                Ok(CommandResult::System(
                    "Language preference cleared. Using automatic detection.".to_string(),
                ))
            }

            Some(Some("toggle")) => {
                let next = if current == "zh" { "English" } else { "中文" };
                let target_lang = if current == "zh" { "en" } else { "zh" };
                apply_language_preference(Some(target_lang));
                let msg = if target_lang == "zh" {
                    format!("已切换语言为{}", next)
                } else {
                    format!("Switched language to {}", next)
                };
                Ok(CommandResult::System(msg))
            }

            Some(Some(lang)) => {
                apply_language_preference(Some(lang));
                let display = display_name(lang);
                let msg = if lang == "zh" {
                    format!("已切换语言为{}", display)
                } else {
                    format!("Switched language to {}", display)
                };
                Ok(CommandResult::System(msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

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
            mossen_utils::config::_reset_global_config_cache_for_testing();
        }
    }

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: true,
            is_remote_mode: false,
            is_custom_backend: true,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    #[test]
    fn lang_set_updates_config_and_runtime_language() {
        let _lock = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _config_dir =
            EnvRestore::set("MOSSEN_CONFIG_DIR", temp.path().to_string_lossy().as_ref());
        let _lang = EnvRestore::remove("MOSSEN_LANG");
        let _language = EnvRestore::remove("MOSSEN_LANGUAGE");
        mossen_utils::config::_reset_global_config_cache_for_testing();

        let result = tokio_test::block_on(LangDirective.execute(&["zh"], &test_context()))
            .expect("lang command");
        assert!(matches!(result, CommandResult::System(_)));
        assert_eq!(std::env::var("MOSSEN_LANG").as_deref(), Ok("zh"));
        assert_eq!(std::env::var("MOSSEN_LANGUAGE").as_deref(), Ok("zh"));

        let config = mossen_utils::config::get_global_config();
        assert_eq!(
            config.interactive_language_preference.as_deref(),
            Some("zh")
        );
        assert_eq!(config.last_interactive_language_tag.as_deref(), Some("zh"));
        assert_eq!(
            mossen_utils::i18n::t("lang.preference.auto", None, None),
            "自动"
        );
    }

    #[test]
    fn lang_usage_shows_explicit_preference() {
        let _lock = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _config_dir =
            EnvRestore::set("MOSSEN_CONFIG_DIR", temp.path().to_string_lossy().as_ref());
        let _lang = EnvRestore::remove("MOSSEN_LANG");
        let _language = EnvRestore::remove("MOSSEN_LANGUAGE");
        mossen_utils::config::_reset_global_config_cache_for_testing();
        save_language_preference(Some("zh"));

        let result =
            tokio_test::block_on(LangDirective.execute(&[], &test_context())).expect("lang usage");
        let CommandResult::System(text) = result else {
            panic!("unexpected result");
        };
        assert!(text.contains("Current language: 中文"), "{text}");
        assert!(text.contains("Preference: 中文"), "{text}");
    }

    #[test]
    fn lang_clear_removes_config_and_runtime_language() {
        let _lock = crate::test_support::env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _config_dir =
            EnvRestore::set("MOSSEN_CONFIG_DIR", temp.path().to_string_lossy().as_ref());
        let _lang = EnvRestore::set("MOSSEN_LANG", "zh");
        let _language = EnvRestore::set("MOSSEN_LANGUAGE", "zh");
        mossen_utils::config::_reset_global_config_cache_for_testing();
        save_language_preference(Some("zh"));

        let result = tokio_test::block_on(LangDirective.execute(&["auto"], &test_context()))
            .expect("lang clear");
        assert!(matches!(result, CommandResult::System(_)));
        assert!(std::env::var("MOSSEN_LANG").is_err());
        assert!(std::env::var("MOSSEN_LANGUAGE").is_err());

        let config = mossen_utils::config::get_global_config();
        assert!(config.interactive_language_preference.is_none());
        assert!(config.last_interactive_language_tag.is_none());
    }
}
