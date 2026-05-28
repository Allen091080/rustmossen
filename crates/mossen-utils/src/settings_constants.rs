//! # settings_constants — 设置常量
//!
//! 对应 TypeScript `utils/settings/constants.ts`。
//! 设置来源常量和管理配置。

/// 所有可能的设置来源
/// 顺序很重要 - 后续来源会覆盖之前的
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingSource {
    /// 用户设置（全局）
    UserSettings,
    /// 项目设置（按目录共享）
    ProjectSettings,
    /// 本地设置（被 gitignore）
    LocalSettings,
    /// 标志设置（来自 --settings 标志）
    FlagSettings,
    /// 策略设置（managed-settings.json 或 API 的远程设置）
    PolicySettings,
}

impl SettingSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SettingSource::UserSettings => "userSettings",
            SettingSource::ProjectSettings => "projectSettings",
            SettingSource::LocalSettings => "localSettings",
            SettingSource::FlagSettings => "flagSettings",
            SettingSource::PolicySettings => "policySettings",
        }
    }
}

/// 获取设置来源的名称
pub fn get_setting_source_name(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "user",
        SettingSource::ProjectSettings => "project",
        SettingSource::LocalSettings => "project, gitignored",
        SettingSource::FlagSettings => "cli flag",
        SettingSource::PolicySettings => "managed",
    }
}

/// 获取设置来源的短显示名称（首字母大写，用于上下文/技能 UI）
pub fn get_source_display_name(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "User",
        SettingSource::ProjectSettings => "Project",
        SettingSource::LocalSettings => "Local",
        SettingSource::FlagSettings => "Flag",
        SettingSource::PolicySettings => "Managed",
    }
}

/// 获取设置来源的显示名称（小写，用于内联使用）
pub fn get_setting_source_display_name_lowercase(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "user settings",
        SettingSource::ProjectSettings => "shared project settings",
        SettingSource::LocalSettings => "project local settings",
        SettingSource::FlagSettings => "command line arguments",
        SettingSource::PolicySettings => "enterprise managed settings",
    }
}

/// 获取设置来源的显示名称（大写，用于 UI 标签）
pub fn get_setting_source_display_name_capitalized(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "User settings",
        SettingSource::ProjectSettings => "Shared project settings",
        SettingSource::LocalSettings => "Project local settings",
        SettingSource::FlagSettings => "Command line arguments",
        SettingSource::PolicySettings => "Enterprise managed settings",
    }
}

/// 可编辑的设置来源（排除 policySettings 和 flagSettings，它们是只读的）
pub type EditableSettingSource = SettingSource;

/// 可以保存权限规则的来源列表，按显示顺序排列。
pub const SOURCES: &[SettingSource] = &[
    SettingSource::LocalSettings,
    SettingSource::ProjectSettings,
    SettingSource::UserSettings,
];

/// Mossen 设置的 JSON Schema URL
pub const MOSSEN_CODE_SETTINGS_SCHEMA_URL: &str =
    "https://schemas.mossen.invalid/mossen-code-settings.json";

/// 所有设置来源
pub const SETTING_SOURCES: &[SettingSource] = &[
    SettingSource::UserSettings,
    SettingSource::ProjectSettings,
    SettingSource::LocalSettings,
    SettingSource::FlagSettings,
    SettingSource::PolicySettings,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setting_source_as_str() {
        assert_eq!(SettingSource::UserSettings.as_str(), "userSettings");
        assert_eq!(SettingSource::PolicySettings.as_str(), "policySettings");
    }

    #[test]
    fn test_get_source_display_name() {
        assert_eq!(get_source_display_name(SettingSource::UserSettings), "User");
        assert_eq!(
            get_source_display_name(SettingSource::PolicySettings),
            "Managed"
        );
    }
}
