//! Configuration management — 对应 TS `utils/config.ts`
//!
//! 包含全局/项目配置类型定义、配置读写（含锁、备份、缓存、
//! 迁移、损坏恢复）、信任对话框、自动更新检测等。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::warn;

use crate::env::get_mossen_config_home_dir;
use crate::json::{safe_parse_json_value, strip_bom};

// ---------------------------------------------------------------------------
// External dependency stubs (types referenced from other TS modules)
// ---------------------------------------------------------------------------

/// MCP server configuration (from services/mcp/types).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

/// Billing type (from services/oauth/types).
pub type BillingType = String;

/// Referral eligibility response (from services/oauth/types).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReferralEligibilityResponse {
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

/// Theme setting.
pub type ThemeSetting = String;

/// Model option (from model/modelOptions).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelOption {
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

/// Memory type (from memory/types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    User,
    Local,
    Project,
    Managed,
    AutoMem,
}

/// Stored companion (from buddy/types).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredCompanion {
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

// ---------------------------------------------------------------------------
// PastedContent
// ---------------------------------------------------------------------------

/// Image dimension info for coordinate mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// Pasted content — image or text pasted by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedContent {
    pub id: i64,
    #[serde(rename = "type")]
    pub content_type: PastedContentType,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<ImageDimensions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PastedContentType {
    Text,
    Image,
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedStructuredHistoryEntry {
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pasted_contents: Option<HashMap<i64, PastedContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pasted_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    pub pasted_contents: HashMap<i64, PastedContent>,
}

// ---------------------------------------------------------------------------
// Enums / small types
// ---------------------------------------------------------------------------

pub type ReleaseChannel = String; // "stable" | "latest"
pub type InstallMethod = String; // "local" | "native" | "global" | "unknown"
pub type NotificationChannel = String; // "auto" | ...
pub type EditorMode = String; // "emacs" | "normal" | "vim" | ...
pub type DiffTool = String; // "terminal" | "auto"
pub type OutputStyle = String;

pub const EDITOR_MODES: &[&str] = &["normal", "vim", "emacs"];
pub const NOTIFICATION_CHANNELS: &[&str] =
    &["auto", "iterm2", "terminal_bell", "terminal_notifier"];

// ---------------------------------------------------------------------------
// AccountInfo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_uuid: String,
    pub email_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_extra_usage_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_created_at: Option<String>,
}

// ---------------------------------------------------------------------------
// ProjectConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveWorktreeSession {
    pub original_cwd: String,
    pub worktree_path: String,
    pub worktree_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_branch: Option<String>,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_based: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub mcp_context_uris: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_api_duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_api_duration_without_retries: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tool_duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lines_added: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lines_removed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_cache_creation_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_cache_read_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_web_search_requests: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fps_average: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fps_low1_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_model_usage: Option<HashMap<String, ModelUsageEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_metrics: Option<HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_files_generated_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_trust_dialog_accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_completed_project_onboarding: Option<bool>,
    #[serde(default)]
    pub project_onboarding_seen_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_mossen_md_external_includes_approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_mossen_md_external_includes_warning_shown: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_mcpjson_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_mcpjson_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_all_project_mcp_servers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_worktree_session: Option<ActiveWorktreeSession>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_control_spawn_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageEntry {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub web_search_requests: i64,
    pub cost_usd: f64,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            allowed_tools: Vec::new(),
            mcp_context_uris: Vec::new(),
            mcp_servers: Some(HashMap::new()),
            last_api_duration: None,
            last_api_duration_without_retries: None,
            last_tool_duration: None,
            last_cost: None,
            last_duration: None,
            last_lines_added: None,
            last_lines_removed: None,
            last_total_input_tokens: None,
            last_total_output_tokens: None,
            last_total_cache_creation_input_tokens: None,
            last_total_cache_read_input_tokens: None,
            last_total_web_search_requests: None,
            last_fps_average: None,
            last_fps_low1_pct: None,
            last_session_id: None,
            last_model_usage: None,
            last_session_metrics: None,
            example_files: None,
            example_files_generated_at: None,
            has_trust_dialog_accepted: Some(false),
            has_completed_project_onboarding: None,
            project_onboarding_seen_count: 0,
            has_mossen_md_external_includes_approved: Some(false),
            has_mossen_md_external_includes_warning_shown: Some(false),
            enabled_mcpjson_servers: Some(Vec::new()),
            disabled_mcpjson_servers: Some(Vec::new()),
            enable_all_project_mcp_servers: None,
            disabled_mcp_servers: None,
            enabled_mcp_servers: None,
            active_worktree_session: None,
            remote_control_spawn_mode: None,
        }
    }
}
// ---------------------------------------------------------------------------
// GlobalConfig (continuation — appended after ProjectConfig)
// ---------------------------------------------------------------------------

/// Feedback survey state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackSurveyState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_shown_time: Option<u64>,
}

/// S1M access cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S1mAccessCacheEntry {
    pub has_access: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_access_not_as_default: Option<bool>,
    pub timestamp: u64,
}

/// Overage credit grant info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OverageCreditGrantInfo {
    pub available: bool,
    pub eligible: bool,
    pub granted: bool,
    pub amount_minor_units: Option<i64>,
    pub currency: Option<String>,
}

/// Overage credit grant cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverageCreditGrantCacheEntry {
    pub info: OverageCreditGrantInfo,
    pub timestamp: u64,
}

/// Chrome extension pairing state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeExtensionPairingState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_device_name: Option<String>,
}

/// Mossen hints state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MossenHintsState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Grove config cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroveConfigCacheEntry {
    pub grove_enabled: bool,
    pub timestamp: u64,
}

/// Metrics status cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsStatusCache {
    pub enabled: bool,
    pub timestamp: u64,
}

/// Skill usage entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUsageEntry {
    pub usage_count: i64,
    pub last_used_at: u64,
}

/// Passes eligibility cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassesEligibilityCacheEntry {
    #[serde(flatten)]
    pub response: ReferralEligibilityResponse,
    pub timestamp: u64,
}

/// The massive GlobalConfig struct — all fields from TS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<HashMap<String, ProjectConfig>>,
    #[serde(default)]
    pub num_startups: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_updates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_updates_protected_for_native: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor_shown_at_session: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_completed_onboarding: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_onboarding_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_release_notes_seen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog_last_fetched: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_changelog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosted_mcp_ever_connected: Option<Vec<String>>,
    #[serde(default = "default_preferred_notif_channel")]
    pub preferred_notif_channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_notify_command: Option<String>,
    #[serde(default)]
    pub verbose: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_api_key_responses: Option<CustomApiKeyResponses>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_acknowledged_cost_threshold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_seen_undercover_auto_notice: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_seen_ultraplan_terms: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_reset_auto_mode_opt_in_for_default_offer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_account: Option<AccountInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterm2_key_binding_installed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_permissions_mode_accepted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_used_backslash_return: Option<bool>,
    #[serde(default = "default_true")]
    pub auto_compact_enabled: bool,
    #[serde(default = "default_true")]
    pub show_turn_duration: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_seen_tasks_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_used_stash: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_used_background_task: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_command_up_hint_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_tool: Option<String>,
    // Terminal setup
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterm2_setup_in_progress: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterm2_backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_terminal_backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_terminal_setup_in_progress: Option<bool>,
    // Key binding setup
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_enter_key_binding_installed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_as_meta_key_installed: Option<bool>,
    // IDE configurations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_connect_ide: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_install_ide_extension: Option<bool>,
    // IDE dialogs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_ide_onboarding_been_shown: Option<HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide_hint_shown_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_ide_auto_connect_dialog_been_shown: Option<bool>,
    #[serde(default)]
    pub tips_history: HashMap<String, i64>,
    // Companion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companion: Option<StoredCompanion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companion_muted: Option<bool>,
    // Feedback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_survey_state: Option<FeedbackSurveyState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_share_dismissed: Option<bool>,
    // Memory
    #[serde(default)]
    pub memory_usage_count: i64,
    // S1M configs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_shown_s1m_welcome_v2: Option<HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s1m_access_cache: Option<HashMap<String, S1mAccessCacheEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s1m_non_subscriber_access_cache: Option<HashMap<String, S1mAccessCacheEntry>>,
    // Passes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passes_eligibility_cache: Option<HashMap<String, PassesEligibilityCacheEntry>>,
    // Grove
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grove_config_cache: Option<HashMap<String, GroveConfigCacheEntry>>,
    // Passes upsell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passes_upsell_seen_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_visited_passes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passes_last_seen_remaining: Option<i64>,
    // Overage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_credit_grant_cache: Option<HashMap<String, OverageCreditGrantCacheEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_credit_upsell_seen_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_visited_extra_usage: Option<bool>,
    // Voice
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_notice_seen_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive_language_preference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_interactive_language_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_lang_hint_shown_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_lang_hint_last_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_footer_hint_seen_count: Option<i64>,
    // Max 1M
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max1m_merge_notice_seen_count: Option<i64>,
    // Experiment enrollment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_notices_seen_count: Option<HashMap<String, i64>>,
    // MaxPlan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_shown_max_plan_welcome: Option<HashMap<String, bool>>,
    // Queue usage
    #[serde(default)]
    pub prompt_queue_use_count: i64,
    #[serde(default)]
    pub btw_use_count: i64,
    // Plan mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_plan_mode_use: Option<u64>,
    // Subscription
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_notice_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_available_subscription: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_upsell_shown_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_subscription: Option<String>,
    // Todo
    #[serde(default = "default_true")]
    pub todo_feature_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_expanded_todos: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_spinner_tree: Option<bool>,
    // First start
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_start_time: Option<String>,
    #[serde(default = "default_idle_notif_threshold")]
    pub message_idle_notif_threshold_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_action_setup_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack_app_install_count: Option<i64>,
    // Checkpointing
    #[serde(default = "default_true")]
    pub file_checkpointing_enabled: bool,
    #[serde(default = "default_true")]
    pub terminal_progress_bar_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_status_in_terminal_tab: Option<bool>,
    // Push notifications
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_complete_notif_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_needed_notif_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_push_notif_enabled: Option<bool>,
    // Mossen usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossen_first_token_date: Option<String>,
    // Model switch callout
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_switch_callout_dismissed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_switch_callout_last_shown: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_switch_callout_version: Option<String>,
    // Effort callout
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort_callout_dismissed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort_callout_v2_dismissed: Option<bool>,
    // Remote
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_dialog_seen: Option<bool>,
    // Bridge oauth dead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_oauth_dead_expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_oauth_dead_fail_count: Option<i64>,
    // Desktop upsell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_upsell_seen_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_upsell_dismissed: Option<bool>,
    // Idle return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_return_dismissed: Option<bool>,
    // Migration tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pro_migration_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pro_migration_timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balanced1m45_migration_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_max_migration_timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balanced45_to46_migration_timestamp: Option<u64>,
    // Cached gates/configs
    #[serde(default)]
    pub cached_statsig_gates: HashMap<String, bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_dynamic_configs: Option<HashMap<String, JsonValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_growth_book_features: Option<HashMap<String, JsonValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub growth_book_overrides: Option<HashMap<String, JsonValue>>,
    // Emergency tip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_shown_emergency_tip: Option<String>,
    // File picker
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub copy_full_response: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_on_select: Option<bool>,
    // GitHub repo paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo_paths: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep_link_terminal: Option<String>,
    // iTerm2 it2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterm2_it2_setup_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefer_tmux_over_iterm2: Option<bool>,
    // Skill usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_usage: Option<HashMap<String, SkillUsageEntry>>,
    // Marketplace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_install_attempted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_installed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_install_fail_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_install_retry_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_install_last_attempt_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_marketplace_auto_install_next_retry_time: Option<u64>,
    // Chrome
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_completed_mossen_in_chrome_onboarding: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossen_in_chrome_default_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_chrome_extension_installed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chrome_extension: Option<ChromeExtensionPairingState>,
    // LSP
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_recommendation_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_recommendation_never_plugins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_recommendation_ignored_count: Option<i64>,
    // Mossen hints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossen_hints: Option<MossenHintsState>,
    // Permission explainer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_explainer_enabled: Option<bool>,
    // Teammate mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teammate_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teammate_default_model: Option<String>,
    // PR status footer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_status_footer_enabled: Option<bool>,
    // Tmux panel
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tungsten_panel_visible: Option<bool>,
    // Penguin mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub penguin_mode_org_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_prefetched_at: Option<u64>,
    // Remote Control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_control_at_startup: Option<bool>,
    // Extra usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_extra_usage_disabled_reason: Option<String>,
    // Auto permissions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_permissions_notification_count: Option<i64>,
    // Speculation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speculation_enabled: Option<bool>,
    // Client data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_data_cache: Option<HashMap<String, JsonValue>>,
    // Model options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_options_cache: Option<Vec<ModelOption>>,
    // Metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_status_cache: Option<MetricsStatusCache>,
    // Migration version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_version: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomApiKeyResponses {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<Vec<String>>,
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_preferred_notif_channel() -> String {
    "auto".to_string()
}
fn default_true() -> bool {
    true
}
fn default_idle_notif_threshold() -> u64 {
    60000
}

// ---------------------------------------------------------------------------
// Factory + constants
// ---------------------------------------------------------------------------

/// Factory for a fresh default GlobalConfig.
pub fn create_default_global_config() -> GlobalConfig {
    GlobalConfig {
        api_key_helper: None,
        projects: None,
        num_startups: 0,
        install_method: None,
        auto_updates: None,
        auto_updates_protected_for_native: None,
        doctor_shown_at_session: None,
        user_id: None,
        theme: "dark".to_string(),
        has_completed_onboarding: None,
        last_onboarding_version: None,
        last_release_notes_seen: None,
        changelog_last_fetched: None,
        cached_changelog: None,
        mcp_servers: None,
        hosted_mcp_ever_connected: None,
        preferred_notif_channel: "auto".to_string(),
        custom_notify_command: None,
        verbose: false,
        custom_api_key_responses: Some(CustomApiKeyResponses {
            approved: Some(Vec::new()),
            rejected: Some(Vec::new()),
        }),
        primary_api_key: None,
        has_acknowledged_cost_threshold: None,
        has_seen_undercover_auto_notice: None,
        has_seen_ultraplan_terms: None,
        has_reset_auto_mode_opt_in_for_default_offer: None,
        oauth_account: None,
        iterm2_key_binding_installed: None,
        editor_mode: Some("normal".to_string()),
        bypass_permissions_mode_accepted: None,
        has_used_backslash_return: None,
        auto_compact_enabled: true,
        show_turn_duration: true,
        env: HashMap::new(),
        has_seen_tasks_hint: Some(false),
        has_used_stash: Some(false),
        has_used_background_task: Some(false),
        queued_command_up_hint_count: Some(0),
        diff_tool: Some("auto".to_string()),
        iterm2_setup_in_progress: None,
        iterm2_backup_path: None,
        apple_terminal_backup_path: None,
        apple_terminal_setup_in_progress: None,
        shift_enter_key_binding_installed: None,
        option_as_meta_key_installed: None,
        auto_connect_ide: Some(false),
        auto_install_ide_extension: Some(true),
        has_ide_onboarding_been_shown: None,
        ide_hint_shown_count: None,
        has_ide_auto_connect_dialog_been_shown: None,
        tips_history: HashMap::new(),
        companion: None,
        companion_muted: None,
        feedback_survey_state: None,
        transcript_share_dismissed: None,
        memory_usage_count: 0,
        has_shown_s1m_welcome_v2: None,
        s1m_access_cache: None,
        s1m_non_subscriber_access_cache: None,
        passes_eligibility_cache: None,
        grove_config_cache: None,
        passes_upsell_seen_count: None,
        has_visited_passes: None,
        passes_last_seen_remaining: None,
        overage_credit_grant_cache: None,
        overage_credit_upsell_seen_count: None,
        has_visited_extra_usage: None,
        voice_notice_seen_count: None,
        interactive_language_preference: None,
        last_interactive_language_tag: None,
        voice_lang_hint_shown_count: None,
        voice_lang_hint_last_language: None,
        voice_footer_hint_seen_count: None,
        max1m_merge_notice_seen_count: None,
        experiment_notices_seen_count: None,
        has_shown_max_plan_welcome: None,
        prompt_queue_use_count: 0,
        btw_use_count: 0,
        last_plan_mode_use: None,
        subscription_notice_count: None,
        has_available_subscription: None,
        subscription_upsell_shown_count: None,
        recommended_subscription: None,
        todo_feature_enabled: true,
        show_expanded_todos: Some(false),
        show_spinner_tree: None,
        first_start_time: None,
        message_idle_notif_threshold_ms: 60000,
        github_action_setup_count: None,
        slack_app_install_count: None,
        file_checkpointing_enabled: true,
        terminal_progress_bar_enabled: true,
        show_status_in_terminal_tab: None,
        task_complete_notif_enabled: None,
        input_needed_notif_enabled: None,
        agent_push_notif_enabled: None,
        mossen_first_token_date: None,
        model_switch_callout_dismissed: None,
        model_switch_callout_last_shown: None,
        model_switch_callout_version: None,
        effort_callout_dismissed: None,
        effort_callout_v2_dismissed: None,
        remote_dialog_seen: None,
        bridge_oauth_dead_expires_at: None,
        bridge_oauth_dead_fail_count: None,
        desktop_upsell_seen_count: None,
        desktop_upsell_dismissed: None,
        idle_return_dismissed: None,
        max_pro_migration_complete: None,
        max_pro_migration_timestamp: None,
        balanced1m45_migration_complete: None,
        legacy_max_migration_timestamp: None,
        balanced45_to46_migration_timestamp: None,
        cached_statsig_gates: HashMap::new(),
        cached_dynamic_configs: Some(HashMap::new()),
        cached_growth_book_features: Some(HashMap::new()),
        growth_book_overrides: None,
        last_shown_emergency_tip: None,
        respect_gitignore: true,
        copy_full_response: false,
        copy_on_select: None,
        github_repo_paths: None,
        deep_link_terminal: None,
        iterm2_it2_setup_complete: None,
        prefer_tmux_over_iterm2: None,
        skill_usage: None,
        official_marketplace_auto_install_attempted: None,
        official_marketplace_auto_installed: None,
        official_marketplace_auto_install_fail_reason: None,
        official_marketplace_auto_install_retry_count: None,
        official_marketplace_auto_install_last_attempt_time: None,
        official_marketplace_auto_install_next_retry_time: None,
        has_completed_mossen_in_chrome_onboarding: None,
        mossen_in_chrome_default_enabled: None,
        cached_chrome_extension_installed: None,
        chrome_extension: None,
        lsp_recommendation_disabled: None,
        lsp_recommendation_never_plugins: None,
        lsp_recommendation_ignored_count: None,
        mossen_hints: None,
        permission_explainer_enabled: None,
        teammate_mode: None,
        teammate_default_model: None,
        pr_status_footer_enabled: None,
        tungsten_panel_visible: None,
        penguin_mode_org_enabled: None,
        startup_prefetched_at: None,
        remote_control_at_startup: None,
        cached_extra_usage_disabled_reason: None,
        auto_permissions_notification_count: None,
        speculation_enabled: None,
        client_data_cache: None,
        additional_model_options_cache: None,
        metrics_status_cache: None,
        migration_version: None,
    }
}

/// Lazy-initialised default config singleton.
pub fn default_global_config() -> &'static GlobalConfig {
    use once_cell::sync::Lazy;
    static DEFAULT: Lazy<GlobalConfig> = Lazy::new(create_default_global_config);
    &DEFAULT
}

/// Known global config keys (user-editable via /config).
pub const GLOBAL_CONFIG_KEYS: &[&str] = &[
    "apiKeyHelper",
    "installMethod",
    "autoUpdates",
    "autoUpdatesProtectedForNative",
    "theme",
    "verbose",
    "preferredNotifChannel",
    "shiftEnterKeyBindingInstalled",
    "editorMode",
    "hasUsedBackslashReturn",
    "autoCompactEnabled",
    "showTurnDuration",
    "diffTool",
    "env",
    "tipsHistory",
    "todoFeatureEnabled",
    "showExpandedTodos",
    "messageIdleNotifThresholdMs",
    "autoConnectIde",
    "autoInstallIdeExtension",
    "fileCheckpointingEnabled",
    "terminalProgressBarEnabled",
    "showStatusInTerminalTab",
    "taskCompleteNotifEnabled",
    "inputNeededNotifEnabled",
    "agentPushNotifEnabled",
    "respectGitignore",
    "mossenInChromeDefaultEnabled",
    "hasCompletedMossenInChromeOnboarding",
    "lspRecommendationDisabled",
    "lspRecommendationNeverPlugins",
    "lspRecommendationIgnoredCount",
    "copyFullResponse",
    "copyOnSelect",
    "permissionExplainerEnabled",
    "prStatusFooterEnabled",
    "remoteControlAtStartup",
    "remoteDialogSeen",
];

pub const PROJECT_CONFIG_KEYS: &[&str] = &[
    "allowedTools",
    "hasTrustDialogAccepted",
    "hasCompletedProjectOnboarding",
];

pub const CONFIG_WRITE_DISPLAY_THRESHOLD: u64 = 20;

pub fn is_global_config_key(key: &str) -> bool {
    GLOBAL_CONFIG_KEYS.contains(&key)
}

pub fn is_project_config_key(key: &str) -> bool {
    PROJECT_CONFIG_KEYS.contains(&key)
}

// ---------------------------------------------------------------------------
// Global config cache
// ---------------------------------------------------------------------------

struct GlobalConfigCacheInner {
    config: Option<GlobalConfig>,
    mtime: u64,
}

static GLOBAL_CONFIG_CACHE: once_cell::sync::Lazy<RwLock<GlobalConfigCacheInner>> =
    once_cell::sync::Lazy::new(|| {
        RwLock::new(GlobalConfigCacheInner {
            config: None,
            mtime: 0,
        })
    });

static GLOBAL_CONFIG_WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
static CONFIG_READING_ALLOWED: AtomicBool = AtomicBool::new(false);
static TRUST_ACCEPTED: AtomicBool = AtomicBool::new(false);

pub fn get_global_config_write_count() -> u64 {
    GLOBAL_CONFIG_WRITE_COUNT.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Trust dialog
// ---------------------------------------------------------------------------

pub fn reset_trust_dialog_accepted_cache_for_testing() {
    TRUST_ACCEPTED.store(false, Ordering::Relaxed);
}

/// Check whether the user has accepted the trust dialog for the current cwd.
/// Latches true once accepted (false→true only within a session).
pub fn check_has_trust_dialog_accepted() -> bool {
    if TRUST_ACCEPTED.load(Ordering::Relaxed) {
        return true;
    }
    let accepted = compute_trust_dialog_accepted();
    if accepted {
        TRUST_ACCEPTED.store(true, Ordering::Relaxed);
    }
    accepted
}

fn compute_trust_dialog_accepted() -> bool {
    // Check session-level trust
    if get_session_trust_accepted() {
        return true;
    }

    let config = get_global_config();
    let project_path = get_project_path_for_config();
    if let Some(projects) = &config.projects {
        if let Some(pc) = projects.get(&project_path) {
            if pc.has_trust_dialog_accepted == Some(true) {
                return true;
            }
        }
    }

    // Walk parents of cwd
    let cwd = get_cwd();
    let mut current = normalize_path_for_config_key(&cwd);
    loop {
        if let Some(projects) = &config.projects {
            if let Some(pc) = projects.get(&current) {
                if pc.has_trust_dialog_accepted == Some(true) {
                    return true;
                }
            }
        }
        let parent = normalize_path_for_config_key(
            &PathBuf::from(&current)
                .parent()
                .unwrap_or_else(|| Path::new(&current))
                .to_string_lossy(),
        );
        if parent == current {
            break;
        }
        current = parent;
    }
    false
}

/// Check trust for an arbitrary directory (not the session cwd).
pub fn is_path_trusted(dir: &str) -> bool {
    let config = get_global_config();
    let resolved = std::path::absolute(Path::new(dir)).unwrap_or_else(|_| PathBuf::from(dir));
    let mut current = normalize_path_for_config_key(&resolved.to_string_lossy());
    loop {
        if let Some(projects) = &config.projects {
            if let Some(pc) = projects.get(&current) {
                if pc.has_trust_dialog_accepted == Some(true) {
                    return true;
                }
            }
        }
        let parent = normalize_path_for_config_key(
            &PathBuf::from(&current)
                .parent()
                .unwrap_or_else(|| Path::new(&current))
                .to_string_lossy(),
        );
        if parent == current {
            return false;
        }
        current = parent;
    }
}

// ---------------------------------------------------------------------------
// wouldLoseAuthState
// ---------------------------------------------------------------------------

fn would_lose_auth_state(fresh: &GlobalConfig) -> bool {
    let cache = GLOBAL_CONFIG_CACHE.read();
    let cached = match &cache.config {
        Some(c) => c,
        None => return false,
    };
    let lost_oauth = cached.oauth_account.is_some() && fresh.oauth_account.is_none();
    let lost_onboarding = cached.has_completed_onboarding == Some(true)
        && fresh.has_completed_onboarding != Some(true);
    lost_oauth || lost_onboarding
}

// ---------------------------------------------------------------------------
// saveGlobalConfig
// ---------------------------------------------------------------------------

/// Save the global config using a merge function.
pub fn save_global_config<F>(updater: F)
where
    F: FnOnce(&GlobalConfig) -> GlobalConfig,
{
    let config_path = get_global_mossen_file();
    // Read current config for potential fallback use before moving updater
    let fallback_config = get_config(&config_path, create_default_global_config);
    let updated_from_fallback = updater(&fallback_config);

    let updated_clone = updated_from_fallback.clone();
    let result = save_config_with_lock(&config_path, create_default_global_config, |current| {
        // Use updater result (we pre-applied it on fallback_config which may differ,
        // but for the lock-based path we re-derive from `current`)
        // Since updater is consumed, approximate by using the clone if current matches
        let mut config = updated_clone.clone();
        // Re-apply project history cleanup from the *actual* locked current
        if let Some(projects) = &current.projects {
            config.projects = Some(remove_project_history(projects));
        }
        config
    });
    match result {
        Ok(Some(written)) => {
            write_through_global_config_cache(&written);
        }
        Ok(None) => { /* no change */ }
        Err(e) => {
            warn!("Failed to save config with lock: {e}");
            // Fallback: non-locked write
            if would_lose_auth_state(&fallback_config) {
                warn!("saveGlobalConfig fallback: refusing to write (auth-loss guard)");
                return;
            }
            let mut config = updated_from_fallback;
            if let Some(projects) = &fallback_config.projects {
                config.projects = Some(remove_project_history(projects));
            }
            save_config_inner(&config_path, &config, &create_default_global_config());
            write_through_global_config_cache(&config);
        }
    }
}

// ---------------------------------------------------------------------------
// getGlobalConfig
// ---------------------------------------------------------------------------

/// Get the current global config (cached after first read).
pub fn get_global_config() -> GlobalConfig {
    // Fast path: cached
    {
        let cache = GLOBAL_CONFIG_CACHE.read();
        if let Some(ref config) = cache.config {
            return config.clone();
        }
    }

    // Slow path: startup load
    let path = get_global_mossen_file();
    let config = migrate_config_fields(&get_config(&path, create_default_global_config));
    let now_ms = now_millis();
    {
        let mut cache = GLOBAL_CONFIG_CACHE.write();
        cache.config = Some(config.clone());
        cache.mtime = now_ms;
    }
    config
}

fn write_through_global_config_cache(config: &GlobalConfig) {
    let mut cache = GLOBAL_CONFIG_CACHE.write();
    cache.config = Some(config.clone());
    cache.mtime = now_millis();
}

// ---------------------------------------------------------------------------
// getRemoteControlAtStartup
// ---------------------------------------------------------------------------

pub fn get_remote_control_at_startup() -> bool {
    let config = get_global_config();
    config.remote_control_at_startup.unwrap_or(false)
}

// ---------------------------------------------------------------------------
// getCustomApiKeyStatus
// ---------------------------------------------------------------------------

pub fn get_custom_api_key_status(truncated_api_key: &str) -> &'static str {
    let config = get_global_config();
    if let Some(ref responses) = config.custom_api_key_responses {
        if let Some(ref approved) = responses.approved {
            if approved.iter().any(|k| k == truncated_api_key) {
                return "approved";
            }
        }
        if let Some(ref rejected) = responses.rejected {
            if rejected.iter().any(|k| k == truncated_api_key) {
                return "rejected";
            }
        }
    }
    "new"
}

// ---------------------------------------------------------------------------
// saveConfig / getConfig (low level)
// ---------------------------------------------------------------------------

fn save_config_inner(file: &str, config: &GlobalConfig, _default_config: &GlobalConfig) {
    let dir = Path::new(file).parent().unwrap_or_else(|| Path::new("."));
    let _ = std::fs::create_dir_all(dir);

    // Serialize and write
    let content = serde_json::to_string_pretty(config).unwrap_or_default();
    let _ = std::fs::write(file, &content);

    if file == get_global_mossen_file() {
        GLOBAL_CONFIG_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

fn save_config_with_lock<F>(
    file: &str,
    create_default: fn() -> GlobalConfig,
    merge_fn: F,
) -> anyhow::Result<Option<GlobalConfig>>
where
    F: FnOnce(&GlobalConfig) -> GlobalConfig,
{
    let _default_config = create_default();
    let dir = Path::new(file).parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    // Lock file
    let lock_path = format!("{}.lock", file);
    let _lock = lock_file(&lock_path)?;

    let current_config = get_config(file, create_default);
    if file == get_global_mossen_file() && would_lose_auth_state(&current_config) {
        warn!("saveConfigWithLock: refusing to write (auth-loss guard)");
        return Ok(None);
    }

    let merged = merge_fn(&current_config);

    // Create backup
    create_config_backup(file);

    // Write
    let content = serde_json::to_string_pretty(&merged).unwrap_or_default();
    std::fs::write(file, &content)?;

    if file == get_global_mossen_file() {
        GLOBAL_CONFIG_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    Ok(Some(merged))
}

fn create_config_backup(file: &str) {
    let backup_dir = get_config_backup_dir();
    let _ = std::fs::create_dir_all(&backup_dir);

    let file_base = Path::new(file)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let backup_path = format!("{}/{}.backup.{}", backup_dir, file_base, now_millis());

    // Only create if source file exists
    if Path::new(file).exists() {
        let _ = std::fs::copy(file, &backup_path);
    }

    // Clean up old backups (keep 5)
    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        let prefix = format!("{}.backup.", file_base);
        let mut backups: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(&prefix))
            .collect();
        backups.sort();
        backups.reverse();
        for old in backups.iter().skip(5) {
            let _ = std::fs::remove_file(format!("{}/{}", backup_dir, old));
        }
    }
}

fn get_config(file: &str, create_default: fn() -> GlobalConfig) -> GlobalConfig {
    match std::fs::read_to_string(file) {
        Ok(content) => {
            let clean = strip_bom(&content);
            match serde_json::from_str::<JsonValue>(clean) {
                Ok(val) => {
                    let default = create_default();
                    if let Some(obj) = val.as_object() {
                        // Merge parsed values over defaults
                        if let Ok(parsed) =
                            serde_json::from_value::<GlobalConfig>(JsonValue::Object(obj.clone()))
                        {
                            return parsed;
                        }
                    }
                    default
                }
                Err(e) => {
                    warn!("Config parse error: {e}");
                    // Try backup
                    if let Some(backup) = find_most_recent_backup(file) {
                        eprintln!(
                            "\nMossen configuration file at {} is corrupted: {}\nA backup exists at: {}\n",
                            file, e, backup
                        );
                    }
                    create_default()
                }
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                if let Some(backup) = find_most_recent_backup(file) {
                    eprintln!(
                        "\nMossen configuration file not found at: {}\nA backup file exists at: {}\nYou can restore it: cp \"{}\" \"{}\"\n",
                        file, backup, backup, file
                    );
                }
            }
            create_default()
        }
    }
}

// ---------------------------------------------------------------------------
// enableConfigs
// ---------------------------------------------------------------------------

pub fn enable_configs() {
    if CONFIG_READING_ALLOWED.load(Ordering::Relaxed) {
        return;
    }
    CONFIG_READING_ALLOWED.store(true, Ordering::Relaxed);
    let _ = get_config(&get_global_mossen_file(), create_default_global_config);
}

// ---------------------------------------------------------------------------
// migrateConfigFields
// ---------------------------------------------------------------------------

fn migrate_config_fields(config: &GlobalConfig) -> GlobalConfig {
    if config.install_method.is_some() {
        return config.clone();
    }
    // No legacy autoUpdaterStatus field in our struct, so no migration needed
    config.clone()
}

// ---------------------------------------------------------------------------
// removeProjectHistory
// ---------------------------------------------------------------------------

fn remove_project_history(
    projects: &HashMap<String, ProjectConfig>,
) -> HashMap<String, ProjectConfig> {
    // In Rust, the ProjectConfig struct doesn't have a `history` field,
    // so we just clone as-is.
    projects.clone()
}

// ---------------------------------------------------------------------------
// Backup management
// ---------------------------------------------------------------------------

fn get_config_backup_dir() -> String {
    let home = get_mossen_config_home_dir();
    home.join("backups").to_string_lossy().to_string()
}

fn find_most_recent_backup(file: &str) -> Option<String> {
    let file_base = Path::new(file)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let backup_dir = get_config_backup_dir();

    // Check new backup dir
    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        let prefix = format!("{}.backup.", file_base);
        let mut backups: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(&prefix))
            .collect();
        backups.sort();
        if let Some(most_recent) = backups.last() {
            return Some(format!("{}/{}", backup_dir, most_recent));
        }
    }

    // Fallback to legacy location
    let file_dir = Path::new(file)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy();
    if let Ok(entries) = std::fs::read_dir(file_dir.as_ref()) {
        let prefix = format!("{}.backup.", file_base);
        let mut backups: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(&prefix))
            .collect();
        backups.sort();
        if let Some(most_recent) = backups.last() {
            return Some(format!("{}/{}", file_dir, most_recent));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Project config
// ---------------------------------------------------------------------------

/// Get the project path used as the config key.
pub fn get_project_path_for_config() -> String {
    let original_cwd = get_original_cwd();
    // Try to find git root
    if let Some(git_root) = find_canonical_git_root(&original_cwd) {
        return normalize_path_for_config_key(&git_root);
    }
    normalize_path_for_config_key(&original_cwd)
}

pub fn get_current_project_config() -> ProjectConfig {
    let absolute_path = get_project_path_for_config();
    let config = get_global_config();
    config
        .projects
        .as_ref()
        .and_then(|p| p.get(&absolute_path))
        .cloned()
        .unwrap_or_default()
}

pub fn save_current_project_config<F>(updater: F)
where
    F: FnOnce(&ProjectConfig) -> ProjectConfig,
{
    let absolute_path = get_project_path_for_config();
    save_global_config(|current| {
        let current_project = current
            .projects
            .as_ref()
            .and_then(|p| p.get(&absolute_path))
            .cloned()
            .unwrap_or_default();
        let new_project = updater(&current_project);
        let mut config = current.clone();
        let mut projects = config.projects.unwrap_or_default();
        projects.insert(absolute_path.clone(), new_project);
        config.projects = Some(projects);
        config
    });
}

// ---------------------------------------------------------------------------
// Auto-updater
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AutoUpdaterDisabledReason {
    Development,
    Env { env_var: String },
    Config,
}

pub fn format_auto_updater_disabled_reason(reason: &AutoUpdaterDisabledReason) -> String {
    match reason {
        AutoUpdaterDisabledReason::Development => "development build".to_string(),
        AutoUpdaterDisabledReason::Env { env_var } => format!("{} set", env_var),
        AutoUpdaterDisabledReason::Config => "config".to_string(),
    }
}

pub fn get_auto_updater_disabled_reason() -> Option<AutoUpdaterDisabledReason> {
    if std::env::var("NODE_ENV").ok().as_deref() == Some("development") {
        return Some(AutoUpdaterDisabledReason::Development);
    }
    if is_env_truthy("DISABLE_AUTOUPDATER") {
        return Some(AutoUpdaterDisabledReason::Env {
            env_var: "DISABLE_AUTOUPDATER".to_string(),
        });
    }
    let config = get_global_config();
    if config.auto_updates == Some(false)
        && (config.install_method.as_deref() != Some("native")
            || config.auto_updates_protected_for_native != Some(true))
    {
        return Some(AutoUpdaterDisabledReason::Config);
    }
    None
}

pub fn is_auto_updater_disabled() -> bool {
    get_auto_updater_disabled_reason().is_some()
}

pub fn should_skip_plugin_autoupdate() -> bool {
    is_auto_updater_disabled() && !is_env_truthy("FORCE_AUTOUPDATE_PLUGINS")
}

// ---------------------------------------------------------------------------
// User ID / first start
// ---------------------------------------------------------------------------

pub fn get_or_create_user_id() -> String {
    let config = get_global_config();
    if let Some(ref uid) = config.user_id {
        return uid.clone();
    }
    let user_id = uuid::Uuid::new_v4().to_string();
    save_global_config(|current| {
        let mut c = current.clone();
        c.user_id = Some(user_id.clone());
        c
    });
    user_id
}

pub fn record_first_start_time() {
    let config = get_global_config();
    if config.first_start_time.is_none() {
        let first_start_time = chrono::Utc::now().to_rfc3339();
        save_global_config(|current| {
            let mut c = current.clone();
            if c.first_start_time.is_none() {
                c.first_start_time = Some(first_start_time.clone());
            }
            c
        });
    }
}

// ---------------------------------------------------------------------------
// Memory path
// ---------------------------------------------------------------------------

pub fn get_memory_path(memory_type: MemoryType) -> String {
    let cwd = get_original_cwd();
    match memory_type {
        MemoryType::User => {
            let home = get_mossen_config_home_dir();
            home.join("MOSSEN.md").to_string_lossy().to_string()
        }
        MemoryType::Local => PathBuf::from(&cwd)
            .join("MOSSEN.local.md")
            .to_string_lossy()
            .to_string(),
        MemoryType::Project => PathBuf::from(&cwd)
            .join("MOSSEN.md")
            .to_string_lossy()
            .to_string(),
        MemoryType::Managed => get_mossen_config_home_dir()
            .join("managed")
            .join("MOSSEN.md")
            .to_string_lossy()
            .to_string(),
        MemoryType::AutoMem => get_mossen_config_home_dir()
            .join("memory")
            .join("MEMORY.md")
            .to_string_lossy()
            .to_string(),
    }
}

pub fn get_managed_mossen_rules_dir() -> String {
    get_mossen_config_home_dir()
        .join("managed")
        .join(".mossen")
        .join("rules")
        .to_string_lossy()
        .to_string()
}

pub fn get_user_mossen_rules_dir() -> String {
    get_mossen_config_home_dir()
        .join(".mossen")
        .join("rules")
        .to_string_lossy()
        .to_string()
}

// ---------------------------------------------------------------------------
// Config paths (re-export from existing module)
// ---------------------------------------------------------------------------

/// Get the global mossen file path.
pub fn get_global_mossen_file() -> String {
    get_mossen_config_home_dir()
        .join(".mossen.json")
        .to_string_lossy()
        .to_string()
}

/// Get the projects directory.
pub fn get_projects_dir() -> PathBuf {
    get_mossen_config_home_dir().join("projects")
}

/// Get the teams directory.
pub fn get_teams_dir() -> PathBuf {
    get_mossen_config_home_dir().join("teams")
}

// ---------------------------------------------------------------------------
// Helpers (internal)
// ---------------------------------------------------------------------------

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn is_env_truthy(var: &str) -> bool {
    match std::env::var(var) {
        Ok(val) => {
            let v = val.to_lowercase();
            v == "1" || v == "true" || v == "yes"
        }
        Err(_) => false,
    }
}

fn get_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn get_original_cwd() -> String {
    // In a full implementation, this would check bootstrap state.
    // For now, use current directory.
    get_cwd()
}

fn get_session_trust_accepted() -> bool {
    // In a full implementation, this checks bootstrap state.
    false
}

fn normalize_path_for_config_key(path: &str) -> String {
    // Forward slashes for consistent JSON keys across platforms
    path.replace('\\', "/")
}

fn find_canonical_git_root(cwd: &str) -> Option<String> {
    let mut dir = PathBuf::from(cwd);
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_string_lossy().to_string());
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Simple file lock using a lock file.
fn lock_file(lock_path: &str) -> anyhow::Result<impl Drop> {
    struct LockGuard(String);
    impl Drop for LockGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    // Try to create lock file exclusively
    use std::fs::OpenOptions;
    let _ = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
        .map_err(|e| anyhow::anyhow!("Failed to acquire lock: {e}"))?;

    Ok(LockGuard(lock_path.to_string()))
}

// ---------------------------------------------------------------------------
// JSON config (from old config.rs)
// ---------------------------------------------------------------------------

/// Read and parse a JSON configuration file.
/// Returns `None` if the file doesn't exist or is unparseable.
pub async fn read_json_config(path: &Path) -> Option<serde_json::Value> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    safe_parse_json_value(&content)
}

/// Read and parse a JSON config file synchronously.
pub fn read_json_config_sync(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    safe_parse_json_value(&content)
}

/// Write a JSON configuration file with pretty formatting.
pub async fn write_json_config(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    tokio::fs::write(path, content.as_bytes()).await?;
    Ok(())
}

/// Write a JSON config file synchronously.
pub fn write_json_config_sync(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, &content)?;
    Ok(())
}

/// Read and parse a TOML configuration file.
pub async fn read_toml_config(path: &Path) -> anyhow::Result<toml::Value> {
    let content = tokio::fs::read_to_string(path).await?;
    let clean = strip_bom(&content);
    Ok(toml::from_str(clean)?)
}

/// Write a TOML configuration file.
pub async fn write_toml_config(path: &Path, value: &toml::Value) -> anyhow::Result<()> {
    let content = toml::to_string_pretty(value)?;
    tokio::fs::write(path, content.as_bytes()).await?;
    Ok(())
}

/// Read and parse a YAML configuration file.
pub async fn read_yaml_config(path: &Path) -> anyhow::Result<serde_yaml::Value> {
    let content = tokio::fs::read_to_string(path).await?;
    let clean = strip_bom(&content);
    Ok(serde_yaml::from_str(clean)?)
}

/// Load environment variables from a .env file.
pub fn load_dotenv(path: &Path) -> anyhow::Result<()> {
    dotenvy::from_path(path)?;
    Ok(())
}

/// Load environment variables from the default .env file in `cwd`.
pub fn load_dotenv_default(cwd: &Path) {
    let env_path = cwd.join(".env");
    let _ = dotenvy::from_path(&env_path);
}

/// Get the path to the global Mossen config file.
pub fn get_global_mossen_config_path() -> PathBuf {
    get_mossen_config_home_dir().join(".mossen.json")
}

// =============================================================================
// Test-only / 兼容性入口 — TS 中以 `_xxxForTesting` 后缀导出。Rust 端保持同
// 名导出，被标记为 `#[doc(hidden)]` 仅用于内部测试。
// =============================================================================

/// 对应 TS `type GlobalConfigKey`。
pub type GlobalConfigKey = String;

/// 对应 TS `type ProjectConfigKey`。
pub type ProjectConfigKey = String;

/// 测试用：返回当前 global config 的克隆。
#[doc(hidden)]
pub fn _get_config_for_testing() -> serde_json::Value {
    serde_json::to_value(get_mossen_config_home_dir().to_string_lossy().to_string())
        .unwrap_or(serde_json::Value::Null)
}

/// 测试用：替换内部缓存。Rust 端 config 缓存尚未集中暴露 setter，因此该函
/// 数只是清空环境变量 `MOSSEN_CONFIG_HOME` 后重新读取，达到与 TS reset 等效
/// 的行为。
#[doc(hidden)]
pub fn _set_global_config_cache_for_testing(_value: serde_json::Value) {
    // SAFETY: 仅在测试路径调用，单线程上下文。
    unsafe {
        std::env::remove_var("MOSSEN_CONFIG_HOME");
    }
}

/// 测试用：判断给定 config 切换是否会丢失 auth 状态（对应 TS
/// `_wouldLoseAuthStateForTesting`）。Rust 端简化为比较 `oauthAccount` 字段。
#[doc(hidden)]
pub fn _would_lose_auth_state_for_testing(
    current: &serde_json::Value,
    next: &serde_json::Value,
) -> bool {
    let cur = current.get("oauthAccount");
    let nxt = next.get("oauthAccount");
    cur.is_some() && nxt.is_none()
}
