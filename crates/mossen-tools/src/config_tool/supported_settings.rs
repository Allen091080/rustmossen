/// Setting source — where the setting is stored.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingSource {
    Global,
    Settings,
}

/// Configuration for a single setting.
#[derive(Debug, Clone)]
pub struct SettingConfig {
    pub source: SettingSource,
    pub setting_type: &'static str,
    pub description: &'static str,
    pub options: Option<&'static [&'static str]>,
}

/// All supported settings registry.
pub static SUPPORTED_SETTINGS: &[(&str, SettingConfig)] = &[
    ("theme", SettingConfig {
        source: SettingSource::Global,
        setting_type: "string",
        description: "Color theme for the UI",
        options: Some(&["light", "dark", "dark-high-contrast", "light-high-contrast", "auto"]),
    }),
    ("editorMode", SettingConfig {
        source: SettingSource::Global,
        setting_type: "string",
        description: "Key binding mode",
        options: Some(&["default", "vim", "emacs"]),
    }),
    ("verbose", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Show detailed debug output",
        options: None,
    }),
    ("preferredNotifChannel", SettingConfig {
        source: SettingSource::Global,
        setting_type: "string",
        description: "Preferred notification channel",
        options: Some(&["terminal", "iterm2", "terminal_bell"]),
    }),
    ("autoCompactEnabled", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Auto-compact when context is full",
        options: None,
    }),
    ("autoMemoryEnabled", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "boolean",
        description: "Enable auto-memory",
        options: None,
    }),
    ("autoDreamEnabled", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "boolean",
        description: "Enable background memory consolidation",
        options: None,
    }),
    ("fileCheckpointingEnabled", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Enable file checkpointing for code rewind",
        options: None,
    }),
    ("showTurnDuration", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Show turn duration message after responses",
        options: None,
    }),
    ("terminalProgressBarEnabled", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Show OSC 9;4 progress indicator in supported terminals",
        options: None,
    }),
    ("todoFeatureEnabled", SettingConfig {
        source: SettingSource::Global,
        setting_type: "boolean",
        description: "Enable todo/task tracking",
        options: None,
    }),
    ("model", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "string",
        description: "Override the default model",
        options: None,
    }),
    ("alwaysThinkingEnabled", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "boolean",
        description: "Enable extended thinking (false to disable)",
        options: None,
    }),
    ("permissions.defaultMode", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "string",
        description: "Default permission mode for tool usage",
        options: Some(&["default", "plan", "acceptEdits", "dontAsk"]),
    }),
    ("language", SettingConfig {
        source: SettingSource::Settings,
        setting_type: "string",
        description: "Preferred language for Mossen responses and voice dictation",
        options: None,
    }),
    ("teammateMode", SettingConfig {
        source: SettingSource::Global,
        setting_type: "string",
        description: "How to spawn teammates: tmux, in-process, or auto",
        options: Some(&["tmux", "in-process", "auto"]),
    }),
];

/// Check if a setting key is supported.
pub fn is_supported(key: &str) -> bool {
    SUPPORTED_SETTINGS.iter().any(|(k, _)| *k == key)
}

/// Get configuration for a setting.
pub fn get_config(key: &str) -> Option<&'static SettingConfig> {
    SUPPORTED_SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, c)| c)
}

/// Get all setting keys.
pub fn get_all_keys() -> Vec<&'static str> {
    SUPPORTED_SETTINGS.iter().map(|(k, _)| *k).collect()
}

/// Get options for a setting.
pub fn get_options_for_setting(key: &str) -> Option<Vec<&'static str>> {
    get_config(key)?.options.map(|opts| opts.to_vec())
}

/// Get the path components for a setting key.
pub fn get_path(key: &str) -> Vec<String> {
    key.split('.').map(|s| s.to_string()).collect()
}
