//! 数据迁移 — 对应 TS 的 migrations/ 目录（10 个迁移函数）。
//!
//! 每个迁移函数处理设置/配置的一次性变更，保持幂等性。

use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Config / Settings 辅助类型
// ---------------------------------------------------------------------------

/// 全局配置（简化表示）。
#[derive(Debug, Clone, Default)]
pub struct GlobalConfig {
    pub bypass_permissions_mode_accepted: Option<bool>,
    pub auto_updates: Option<bool>,
    pub auto_updates_protected_for_native: Option<bool>,
    pub max_pro_migration_complete: Option<bool>,
    pub max_pro_migration_timestamp: Option<u64>,
    pub balanced1m45_migration_complete: Option<bool>,
    pub balanced45_to46_migration_timestamp: Option<u64>,
    pub legacy_max_migration_timestamp: Option<u64>,
    pub has_reset_auto_mode_opt_in_for_default_offer: Option<bool>,
    pub num_startups: u64,
    pub extra: serde_json::Map<String, JsonValue>,
}

/// 用户设置（简化表示）。
#[derive(Debug, Clone, Default)]
pub struct UserSettings {
    pub model: Option<String>,
    pub fast_mode: Option<bool>,
    pub skip_dangerous_mode_permission_prompt: Option<bool>,
    pub skip_auto_permission_prompt: Option<bool>,
    pub env: std::collections::HashMap<String, String>,
    pub permissions: Option<PermissionsSettings>,
    pub extra: serde_json::Map<String, JsonValue>,
}

/// 权限设置。
#[derive(Debug, Clone, Default)]
pub struct PermissionsSettings {
    pub default_mode: Option<String>,
}

/// 项目配置。
#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub enable_all_project_mcp_servers: Option<bool>,
    pub enabled_mcpjson_servers: Vec<String>,
    pub disabled_mcpjson_servers: Vec<String>,
    pub extra: serde_json::Map<String, JsonValue>,
}

/// 本地设置。
#[derive(Debug, Clone, Default)]
pub struct LocalSettings {
    pub enable_all_project_mcp_servers: Option<bool>,
    pub enabled_mcpjson_servers: Vec<String>,
    pub disabled_mcpjson_servers: Vec<String>,
    pub extra: serde_json::Map<String, JsonValue>,
}

/// 设置来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
}

// ---------------------------------------------------------------------------
// Trait: 迁移上下文
// ---------------------------------------------------------------------------

/// 迁移上下文 trait — 提供对配置/设置的读写访问。
pub trait MigrationContext {
    fn get_global_config(&self) -> GlobalConfig;
    fn save_global_config(&self, updater: Box<dyn FnOnce(GlobalConfig) -> GlobalConfig>);
    fn get_settings_for_source(&self, source: SettingSource) -> Option<UserSettings>;
    fn update_settings_for_source(&self, source: SettingSource, updates: UserSettings);
    fn get_current_project_config(&self) -> ProjectConfig;
    fn save_current_project_config(&self, updater: Box<dyn FnOnce(ProjectConfig) -> ProjectConfig>);
    fn get_local_settings(&self) -> Option<LocalSettings>;
    fn update_local_settings(&self, updates: LocalSettings);
    fn get_api_provider(&self) -> String;
    fn is_pro_subscriber(&self) -> bool;
    fn is_max_subscriber(&self) -> bool;
    fn is_team_premium_subscriber(&self) -> bool;
    fn get_user_type(&self) -> String;
    fn is_legacy_model_remap_enabled(&self) -> bool;
    fn is_max1m_merge_enabled(&self) -> bool;
    fn get_auto_mode_enabled_state(&self) -> String;
    fn get_main_loop_model_override(&self) -> Option<String>;
    fn set_main_loop_model_override(&self, model: &str);
    fn has_skip_dangerous_mode_permission_prompt(&self) -> bool;
    fn parse_user_specified_model(&self, model: &str) -> String;
    fn get_default_main_loop_model_setting(&self) -> String;
    fn log_event(&self, event: &str, metadata: &serde_json::Map<String, JsonValue>);
    fn is_feature_enabled(&self, feature: &str) -> bool;
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Migration 1: migrateBypassPermissionsAcceptedToSettings
// ---------------------------------------------------------------------------

/// 将 bypassPermissionsModeAccepted 从全局配置迁移到 settings.json。
pub fn migrate_bypass_permissions_accepted_to_settings(ctx: &dyn MigrationContext) {
    let global_config = ctx.get_global_config();

    if global_config.bypass_permissions_mode_accepted != Some(true) {
        return;
    }

    if !ctx.has_skip_dangerous_mode_permission_prompt() {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                skip_dangerous_mode_permission_prompt: Some(true),
                ..Default::default()
            },
        );
    }

    ctx.log_event(
        "mossen_migrate_bypass_permissions_accepted",
        &serde_json::Map::new(),
    );

    ctx.save_global_config(Box::new(|mut config| {
        config.bypass_permissions_mode_accepted = None;
        config
    }));

    info!("migrated bypass permissions accepted to settings");
}

// ---------------------------------------------------------------------------
// Migration 2: migrateLegacyMaxToCurrent
// ---------------------------------------------------------------------------

/// 遗留 Max 模型 ID 列表。
const LEGACY_MAX_FIRSTPARTY_MODEL_IDS: &[&str] =
    &["mossen-max-4-0-20250514", "mossen-max-4-1-20250620"];

/// 将 1P 用户从显式 Max 4.0/4.1 模型字符串迁移到 'max' 别名。
pub fn migrate_legacy_max_to_current(ctx: &dyn MigrationContext) {
    if ctx.get_api_provider() != "firstParty" {
        return;
    }

    if !ctx.is_legacy_model_remap_enabled() {
        return;
    }

    let model = match ctx.get_settings_for_source(SettingSource::UserSettings) {
        Some(s) => s.model,
        None => None,
    };

    let model_str = match &model {
        Some(m) => m.as_str(),
        None => return,
    };

    if !LEGACY_MAX_FIRSTPARTY_MODEL_IDS.contains(&model_str) {
        return;
    }

    ctx.update_settings_for_source(
        SettingSource::UserSettings,
        UserSettings {
            model: Some("max".to_string()),
            ..Default::default()
        },
    );

    ctx.save_global_config(Box::new(|mut config| {
        config.legacy_max_migration_timestamp = Some(now_millis());
        config
    }));

    let mut meta = serde_json::Map::new();
    meta.insert(
        "from_model".to_string(),
        JsonValue::String(model_str.to_string()),
    );
    ctx.log_event("mossen_legacy_max_migration", &meta);

    info!(from_model = model_str, "migrated legacy max to current");
}

// ---------------------------------------------------------------------------
// Migration 3: migrateMaxToMax1m
// ---------------------------------------------------------------------------

/// 将有资格的用户从 'max' 迁移到 'max[1m]'。
pub fn migrate_max_to_max1m(ctx: &dyn MigrationContext) {
    if !ctx.is_max1m_merge_enabled() {
        return;
    }

    let model = match ctx.get_settings_for_source(SettingSource::UserSettings) {
        Some(s) => s.model,
        None => None,
    };

    if model.as_deref() != Some("max") {
        return;
    }

    let migrated = "max[1m]";
    let parsed_migrated = ctx.parse_user_specified_model(migrated);
    let parsed_default = ctx.parse_user_specified_model(&ctx.get_default_main_loop_model_setting());

    let model_to_set = if parsed_migrated == parsed_default {
        None
    } else {
        Some(migrated.to_string())
    };

    ctx.update_settings_for_source(
        SettingSource::UserSettings,
        UserSettings {
            model: model_to_set,
            ..Default::default()
        },
    );

    ctx.log_event("mossen_max_to_max1m_migration", &serde_json::Map::new());

    info!("migrated max to max[1m]");
}

// ---------------------------------------------------------------------------
// Migration 4: migrateBalanced45ToBalanced46
// ---------------------------------------------------------------------------

/// 遗留 Balanced 4.5 模型 ID 列表。
const LEGACY_BALANCED_45_FIRSTPARTY_MODEL_IDS: &[&str] = &[
    "mossen-balanced-4-5-20250929",
    "mossen-balanced-4-5-20250929[1m]",
];

/// 将 Pro/Max/Team Premium 1P 用户从 Balanced 4.5 迁移到 'balanced' 别名。
pub fn migrate_balanced45_to_balanced46(ctx: &dyn MigrationContext) {
    if ctx.get_api_provider() != "firstParty" {
        return;
    }

    if !ctx.is_pro_subscriber() && !ctx.is_max_subscriber() && !ctx.is_team_premium_subscriber() {
        return;
    }

    let model = match ctx.get_settings_for_source(SettingSource::UserSettings) {
        Some(s) => s.model,
        None => None,
    };

    let model_str = match &model {
        Some(m) => m.as_str(),
        None => return,
    };

    if !LEGACY_BALANCED_45_FIRSTPARTY_MODEL_IDS.contains(&model_str) {
        return;
    }

    let has_1m = model_str.ends_with("[1m]");
    let new_model = if has_1m { "balanced[1m]" } else { "balanced" };

    ctx.update_settings_for_source(
        SettingSource::UserSettings,
        UserSettings {
            model: Some(new_model.to_string()),
            ..Default::default()
        },
    );

    let config = ctx.get_global_config();
    if config.num_startups > 1 {
        ctx.save_global_config(Box::new(|mut c| {
            c.balanced45_to46_migration_timestamp = Some(now_millis());
            c
        }));
    }

    let mut meta = serde_json::Map::new();
    meta.insert(
        "from_model".to_string(),
        JsonValue::String(model_str.to_string()),
    );
    meta.insert("has_1m".to_string(), JsonValue::Bool(has_1m));
    ctx.log_event("mossen_balanced45_to_46_migration", &meta);

    info!(from_model = model_str, "migrated balanced 4.5 to 4.6");
}

// ---------------------------------------------------------------------------
// Migration 5: migrateAutoUpdatesToSettings
// ---------------------------------------------------------------------------

/// 将用户的 autoUpdates 偏好迁移到 settings.json 的环境变量。
pub fn migrate_auto_updates_to_settings(ctx: &dyn MigrationContext) {
    let global_config = ctx.get_global_config();

    if global_config.auto_updates != Some(false)
        || global_config.auto_updates_protected_for_native == Some(true)
    {
        return;
    }

    let user_settings = ctx
        .get_settings_for_source(SettingSource::UserSettings)
        .unwrap_or_default();
    let already_had_env_var = user_settings.env.contains_key("DISABLE_AUTOUPDATER");

    let mut updated = user_settings.clone();
    updated
        .env
        .insert("DISABLE_AUTOUPDATER".to_string(), "1".to_string());
    ctx.update_settings_for_source(SettingSource::UserSettings, updated);

    // 设置环境变量立即生效
    std::env::set_var("DISABLE_AUTOUPDATER", "1");

    let mut meta = serde_json::Map::new();
    meta.insert("was_user_preference".to_string(), JsonValue::Bool(true));
    meta.insert(
        "already_had_env_var".to_string(),
        JsonValue::Bool(already_had_env_var),
    );
    ctx.log_event("mossen_migrate_autoupdates_to_settings", &meta);

    ctx.save_global_config(Box::new(|mut config| {
        config.auto_updates = None;
        config.auto_updates_protected_for_native = None;
        config
    }));

    info!("migrated auto-updates to settings");
}

// ---------------------------------------------------------------------------
// Migration 6: resetAutoModeOptInForDefaultOffer
// ---------------------------------------------------------------------------

/// 重置 skipAutoPermissionPrompt 以显示新的 "make it my default mode" 选项。
pub fn reset_auto_mode_opt_in_for_default_offer(ctx: &dyn MigrationContext) {
    if !ctx.is_feature_enabled("TRANSCRIPT_CLASSIFIER") {
        return;
    }

    let config = ctx.get_global_config();
    if config.has_reset_auto_mode_opt_in_for_default_offer == Some(true) {
        return;
    }

    if ctx.get_auto_mode_enabled_state() != "enabled" {
        return;
    }

    let user = ctx
        .get_settings_for_source(SettingSource::UserSettings)
        .unwrap_or_default();

    if user.skip_auto_permission_prompt == Some(true)
        && user
            .permissions
            .as_ref()
            .and_then(|p| p.default_mode.as_deref())
            != Some("auto")
    {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                skip_auto_permission_prompt: None,
                ..Default::default()
            },
        );
        ctx.log_event(
            "mossen_migrate_reset_auto_opt_in_for_default_offer",
            &serde_json::Map::new(),
        );
    }

    ctx.save_global_config(Box::new(|mut c| {
        c.has_reset_auto_mode_opt_in_for_default_offer = Some(true);
        c
    }));

    info!("reset auto mode opt-in for default offer");
}

// ---------------------------------------------------------------------------
// Migration 7: migrateBalanced1mToBalanced45
// ---------------------------------------------------------------------------

/// 将 "balanced[1m]" 迁移到显式 "balanced-4-5-20250929[1m]"。
pub fn migrate_balanced1m_to_balanced45(ctx: &dyn MigrationContext) {
    let config = ctx.get_global_config();
    if config.balanced1m45_migration_complete == Some(true) {
        return;
    }

    let model = ctx
        .get_settings_for_source(SettingSource::UserSettings)
        .and_then(|s| s.model);

    if model.as_deref() == Some("balanced[1m]") {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                model: Some("balanced-4-5-20250929[1m]".to_string()),
                ..Default::default()
            },
        );
    }

    // 同时迁移内存中的 override
    if ctx.get_main_loop_model_override().as_deref() == Some("balanced[1m]") {
        ctx.set_main_loop_model_override("balanced-4-5-20250929[1m]");
    }

    ctx.save_global_config(Box::new(|mut c| {
        c.balanced1m45_migration_complete = Some(true);
        c
    }));

    info!("migrated balanced[1m] to balanced-4-5-20250929[1m]");
}

// ---------------------------------------------------------------------------
// Migration 8: resetProToMaxDefault
// ---------------------------------------------------------------------------

/// 将 Pro 用户重置为 Max 4.5 默认模型。
pub fn reset_pro_to_max_default(ctx: &dyn MigrationContext) {
    let config = ctx.get_global_config();
    if config.max_pro_migration_complete == Some(true) {
        return;
    }

    let api_provider = ctx.get_api_provider();

    if api_provider != "firstParty" || !ctx.is_pro_subscriber() {
        ctx.save_global_config(Box::new(|mut c| {
            c.max_pro_migration_complete = Some(true);
            c
        }));
        let mut meta = serde_json::Map::new();
        meta.insert("skipped".to_string(), JsonValue::Bool(true));
        ctx.log_event("mossen_reset_pro_to_max_default", &meta);
        return;
    }

    let settings = ctx
        .get_settings_for_source(SettingSource::UserSettings)
        .unwrap_or_default();

    if settings.model.is_none() {
        let ts = now_millis();
        ctx.save_global_config(Box::new(move |mut c| {
            c.max_pro_migration_complete = Some(true);
            c.max_pro_migration_timestamp = Some(ts);
            c
        }));
        let mut meta = serde_json::Map::new();
        meta.insert("skipped".to_string(), JsonValue::Bool(false));
        meta.insert("had_custom_model".to_string(), JsonValue::Bool(false));
        ctx.log_event("mossen_reset_pro_to_max_default", &meta);
    } else {
        ctx.save_global_config(Box::new(|mut c| {
            c.max_pro_migration_complete = Some(true);
            c
        }));
        let mut meta = serde_json::Map::new();
        meta.insert("skipped".to_string(), JsonValue::Bool(false));
        meta.insert("had_custom_model".to_string(), JsonValue::Bool(true));
        ctx.log_event("mossen_reset_pro_to_max_default", &meta);
    }

    info!("reset pro to max default");
}

// ---------------------------------------------------------------------------
// Migration 9: migrateLegacyFastToMax
// ---------------------------------------------------------------------------

/// 将已移除的 legacy_fast 模型别名迁移到新的 Max 4.6 别名。
pub fn migrate_legacy_fast_to_max(ctx: &dyn MigrationContext) {
    if ctx.get_user_type() != "internal" {
        return;
    }

    let settings = ctx
        .get_settings_for_source(SettingSource::UserSettings)
        .unwrap_or_default();

    let model = match &settings.model {
        Some(m) => m.as_str(),
        None => return,
    };

    if model.starts_with("legacy-latest[1m]") {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                model: Some("max[1m]".to_string()),
                ..Default::default()
            },
        );
    } else if model.starts_with("legacy-latest") {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                model: Some("max".to_string()),
                ..Default::default()
            },
        );
    } else if model.starts_with("legacy-fast-latest") || model.starts_with("max-4-5-fast") {
        ctx.update_settings_for_source(
            SettingSource::UserSettings,
            UserSettings {
                model: Some("max[1m]".to_string()),
                fast_mode: Some(true),
                ..Default::default()
            },
        );
    }

    info!("migrated legacy_fast to max");
}

// ---------------------------------------------------------------------------
// Migration 10: migrateEnableAllProjectMcpServersToSettings
// ---------------------------------------------------------------------------

/// 将 MCP 服务器审批字段从项目配置迁移到本地设置。
pub fn migrate_enable_all_project_mcp_servers_to_settings(ctx: &dyn MigrationContext) {
    let project_config = ctx.get_current_project_config();

    let has_enable_all = project_config.enable_all_project_mcp_servers.is_some();
    let has_enabled_servers = !project_config.enabled_mcpjson_servers.is_empty();
    let has_disabled_servers = !project_config.disabled_mcpjson_servers.is_empty();

    if !has_enable_all && !has_enabled_servers && !has_disabled_servers {
        return;
    }

    let existing = ctx.get_local_settings().unwrap_or_default();
    let mut updates = LocalSettings::default();
    let mut fields_to_remove = Vec::new();
    let mut migrated_count = 0u64;

    // 迁移 enableAllProjectMcpServers
    if has_enable_all && existing.enable_all_project_mcp_servers.is_none() {
        updates.enable_all_project_mcp_servers = project_config.enable_all_project_mcp_servers;
        fields_to_remove.push("enableAllProjectMcpServers");
        migrated_count += 1;
    } else if has_enable_all {
        fields_to_remove.push("enableAllProjectMcpServers");
        migrated_count += 1;
    }

    // 迁移 enabledMcpjsonServers（合并，去重）
    if has_enabled_servers {
        let mut merged: HashSet<String> = existing.enabled_mcpjson_servers.into_iter().collect();
        for s in &project_config.enabled_mcpjson_servers {
            merged.insert(s.clone());
        }
        updates.enabled_mcpjson_servers = merged.into_iter().collect();
        fields_to_remove.push("enabledMcpjsonServers");
        migrated_count += 1;
    }

    // 迁移 disabledMcpjsonServers（合并，去重）
    if has_disabled_servers {
        let mut merged: HashSet<String> = existing.disabled_mcpjson_servers.into_iter().collect();
        for s in &project_config.disabled_mcpjson_servers {
            merged.insert(s.clone());
        }
        updates.disabled_mcpjson_servers = merged.into_iter().collect();
        fields_to_remove.push("disabledMcpjsonServers");
        migrated_count += 1;
    }

    // 更新设置
    if updates.enable_all_project_mcp_servers.is_some()
        || !updates.enabled_mcpjson_servers.is_empty()
        || !updates.disabled_mcpjson_servers.is_empty()
    {
        ctx.update_local_settings(updates);
    }

    // 从项目配置中移除已迁移字段
    if !fields_to_remove.is_empty() {
        ctx.save_current_project_config(Box::new(|mut config| {
            config.enable_all_project_mcp_servers = None;
            config.enabled_mcpjson_servers.clear();
            config.disabled_mcpjson_servers.clear();
            config
        }));
    }

    let mut meta = serde_json::Map::new();
    meta.insert(
        "migratedCount".to_string(),
        JsonValue::Number(migrated_count.into()),
    );
    ctx.log_event("mossen_migrate_mcp_approval_fields_success", &meta);

    info!(
        migrated_count = migrated_count,
        "migrated MCP approval fields to settings"
    );
}

// ---------------------------------------------------------------------------
// 运行所有迁移
// ---------------------------------------------------------------------------

/// 依次运行所有迁移。每个迁移内部保证幂等性。
pub fn run_all_migrations(ctx: &dyn MigrationContext) {
    migrate_bypass_permissions_accepted_to_settings(ctx);
    migrate_legacy_max_to_current(ctx);
    migrate_max_to_max1m(ctx);
    migrate_balanced45_to_balanced46(ctx);
    migrate_auto_updates_to_settings(ctx);
    reset_auto_mode_opt_in_for_default_offer(ctx);
    migrate_balanced1m_to_balanced45(ctx);
    reset_pro_to_max_default(ctx);
    migrate_legacy_fast_to_max(ctx);
    migrate_enable_all_project_mcp_servers_to_settings(ctx);
    info!("all migrations complete");
}
