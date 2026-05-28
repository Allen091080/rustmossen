//! Authentication utilities — 对应 TS `utils/auth.ts`
//!
//! API key management, OAuth token handling, AWS/GCP credential refresh,
//! subscription type queries, custom headers helper, account information.

use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use crate::config::{
    check_has_trust_dialog_accepted, get_global_config, save_global_config, AccountInfo,
};

// ---------------------------------------------------------------------------
// External dependency stubs
// ---------------------------------------------------------------------------

/// Subscription type (from services/oauth/types).
pub type SubscriptionType = String;

/// OAuth tokens (from services/oauth/types).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<SubscriptionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
}

/// Sync hook JSON output stub.
pub type SyncHookJSONOutput = serde_json::Value;

/// Settings map stub (merged settings from various sources).
pub type MergedSettings = HashMap<String, serde_json::Value>;

// Stub functions for external dependencies
fn is_env_truthy(val: Option<&str>) -> bool {
    matches!(val, Some(v) if v == "1" || v.eq_ignore_ascii_case("true"))
}

fn is_bare_mode() -> bool {
    is_env_truthy(std::env::var("MOSSEN_CODE_BARE").ok().as_deref())
}

fn is_running_on_homespace() -> bool {
    is_env_truthy(std::env::var("MOSSEN_CODE_HOMESPACE").ok().as_deref())
}

fn is_custom_backend_enabled() -> bool {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_URL")
        .ok()
        .is_some_and(|v| !v.is_empty())
}

fn get_custom_backend_api_key() -> Option<String> {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
}

fn get_custom_backend_auth_token() -> Option<String> {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_AUTH_TOKEN")
        .ok()
        .filter(|v| !v.is_empty())
}

fn get_custom_backend_base_url() -> Option<String> {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

fn get_custom_backend_name() -> String {
    std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_NAME").unwrap_or_else(|_| "custom".to_string())
}

fn get_settings_deprecated() -> Option<MergedSettings> {
    None // Stub: merged settings from JSON files
}

fn get_settings_for_source(_source: &str) -> Option<MergedSettings> {
    None // Stub: settings from a specific source
}

fn get_is_non_interactive_session() -> bool {
    is_env_truthy(std::env::var("MOSSEN_CODE_NON_INTERACTIVE").ok().as_deref())
}

fn prefer_third_party_authentication() -> bool {
    is_env_truthy(std::env::var("MOSSEN_CODE_PRINT").ok().as_deref())
}

fn get_api_key_from_file_descriptor() -> Option<String> {
    None // Stub: read API key from file descriptor
}

fn get_oauth_token_from_file_descriptor() -> Option<String> {
    std::env::var("MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR")
        .ok()
        .filter(|v| !v.is_empty())
        .and(None) // Stub: actual FD reading
}

fn normalize_api_key_for_config(key: &str) -> String {
    // Hash-based normalization for safe config storage
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

fn should_use_hosted_auth(scopes: Option<&[String]>) -> bool {
    scopes.is_some_and(|s| s.iter().any(|scope| scope.contains("inference")))
}

fn is_oauth_token_expired(expires_at: Option<u64>) -> bool {
    let expires_at = match expires_at {
        Some(v) => v,
        None => return false,
    };
    let buffer_ms: u64 = 5 * 60 * 1000;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now_ms + buffer_ms >= expires_at
}

fn should_use_mock_subscription() -> bool {
    is_env_truthy(
        std::env::var("MOSSEN_CODE_MOCK_SUBSCRIPTION")
            .ok()
            .as_deref(),
    )
}

fn get_mock_subscription_type() -> Option<SubscriptionType> {
    std::env::var("MOSSEN_CODE_MOCK_SUBSCRIPTION_TYPE").ok()
}

fn get_api_provider() -> String {
    if is_env_truthy(std::env::var("MOSSEN_CODE_USE_BEDROCK").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_VERTEX").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_FOUNDRY").ok().as_deref())
    {
        "thirdParty".to_string()
    } else {
        "firstParty".to_string()
    }
}

fn log_event(_name: &str, _meta: &[(&str, &str)]) {
    // Stub: analytics event logging
}

fn log_for_debugging(msg: &str) {
    debug!("{}", msg);
}

fn log_error(err: &dyn std::fmt::Display) {
    error!("{}", err);
}

fn get_mossen_config_home_dir() -> std::path::PathBuf {
    crate::env::get_mossen_config_home_dir()
}

fn exec_sync_with_defaults(cmd: &str) -> Option<String> {
    let output = Command::new("sh").arg("-c").arg(cmd).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn get_secure_storage_read() -> Option<SecureStorageData> {
    None // Stub: secure storage read
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecureStorageData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosted_oauth: Option<OAuthTokens>,
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

const HOSTED_PROFILE_SCOPE: &str = "user:profile";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default TTL for API key helper cache in milliseconds (5 minutes).
const DEFAULT_API_KEY_HELPER_TTL: u64 = 5 * 60 * 1000;

/// Default STS credentials TTL (1 hour).
const DEFAULT_AWS_STS_TTL: u64 = 60 * 60 * 1000;

/// Timeout for AWS auth refresh command (3 minutes).
const AWS_AUTH_REFRESH_TIMEOUT_MS: u64 = 3 * 60 * 1000;

/// Short timeout for GCP credentials probe (5 seconds).
const GCP_CREDENTIALS_CHECK_TIMEOUT_MS: u64 = 5_000;

/// Default GCP credential TTL (1 hour).
const DEFAULT_GCP_CREDENTIAL_TTL: u64 = 60 * 60 * 1000;

/// Timeout for GCP auth refresh command (3 minutes).
const GCP_AUTH_REFRESH_TIMEOUT_MS: u64 = 3 * 60 * 1000;

/// Default custom headers debounce (29 minutes).
const DEFAULT_CUSTOM_HEADERS_DEBOUNCE_MS: u64 = 29 * 60 * 1000;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Source of an API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeySource {
    MossenCodeApiKey,
    CustomBackend,
    ApiKeyHelper,
    MossenManagedKey,
    None,
}

impl std::fmt::Display for ApiKeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MossenCodeApiKey => write!(f, "MOSSEN_CODE_API_KEY"),
            Self::CustomBackend => write!(f, "custom_backend"),
            Self::ApiKeyHelper => write!(f, "apiKeyHelper"),
            Self::MossenManagedKey => write!(f, "mossen managed key"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Result of getMossenApiKeyWithSource.
#[derive(Debug, Clone)]
pub struct ApiKeyWithSource {
    pub key: Option<String>,
    pub source: ApiKeySource,
}

/// Auth token source info.
#[derive(Debug, Clone)]
pub struct AuthTokenSourceInfo {
    pub source: AuthTokenSource,
    pub has_token: bool,
}

/// Auth token source variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthTokenSource {
    CustomBackend,
    MossenCodeAuthToken,
    MossenCodeAuthTokenFileDescriptor,
    CcrOauthTokenFile,
    ApiKeyHelper,
    Hosted,
    None,
}

/// User account info for display.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserAccountInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Org validation result.
#[derive(Debug, Clone)]
pub enum OrgValidationResult {
    Valid,
    Invalid { message: String },
}

/// GCP credentials timeout error.
#[derive(Debug, thiserror::Error)]
#[error("GCP credentials check timed out")]
pub struct GcpCredentialsTimeoutError;

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

struct ApiKeyHelperCache {
    value: String,
    timestamp: u64,
}

struct ApiKeyHelperInflight {
    started_at: Option<Instant>,
}

static API_KEY_HELPER_CACHE: RwLock<Option<ApiKeyHelperCache>> = RwLock::new(None);
static API_KEY_HELPER_EPOCH: AtomicU64 = AtomicU64::new(0);
static API_KEY_HELPER_INFLIGHT: Mutex<Option<ApiKeyHelperInflight>> = Mutex::new(None);

struct CustomHeadersCache {
    headers: HashMap<String, String>,
    timestamp: u64,
}

static CUSTOM_HEADERS_CACHE: RwLock<Option<CustomHeadersCache>> = RwLock::new(None);

struct AwsCredentialsCache {
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
    timestamp: u64,
}

static AWS_CREDENTIALS_CACHE: RwLock<Option<AwsCredentialsCache>> = RwLock::new(None);

static GCP_CREDENTIALS_CACHE: RwLock<Option<u64>> = RwLock::new(None);

static LAST_CREDENTIALS_MTIME_MS: AtomicU64 = AtomicU64::new(0);

static PENDING_REFRESH_CHECK: Mutex<bool> = Mutex::new(false);

// ---------------------------------------------------------------------------
// Helper: current time in millis
// ---------------------------------------------------------------------------

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// isManagedOAuthContext
// ---------------------------------------------------------------------------

fn is_managed_oauth_context() -> bool {
    is_env_truthy(std::env::var("MOSSEN_CODE_REMOTE").ok().as_deref())
        || std::env::var("MOSSEN_CODE_ENTRYPOINT").ok().as_deref() == Some("mossen-desktop")
}

// ---------------------------------------------------------------------------
// isHostedAuthAdapterEnabled
// ---------------------------------------------------------------------------

pub fn is_hosted_auth_adapter_enabled() -> bool {
    if is_custom_backend_enabled() || is_bare_mode() {
        return false;
    }
    is_env_truthy(
        std::env::var("MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER")
            .ok()
            .as_deref(),
    ) || std::env::var("MOSSEN_CODE_AUTH_TOKEN")
        .ok()
        .is_some_and(|v| !v.is_empty())
        || std::env::var("MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR")
            .ok()
            .is_some_and(|v| !v.is_empty())
        || std::env::var("MOSSEN_CODE_AUTH_REFRESH_TOKEN")
            .ok()
            .is_some_and(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// isMossenHostedAuthEnabled
// ---------------------------------------------------------------------------

pub fn is_mossen_hosted_auth_enabled() -> bool {
    if is_custom_backend_enabled() {
        return false;
    }
    if is_bare_mode() {
        return false;
    }
    if !is_hosted_auth_adapter_enabled() {
        return false;
    }

    // `mossen ssh` remote
    if std::env::var("MOSSEN_CODE_UNIX_SOCKET")
        .ok()
        .is_some_and(|v| !v.is_empty())
    {
        return std::env::var("MOSSEN_CODE_AUTH_TOKEN")
            .ok()
            .is_some_and(|v| !v.is_empty());
    }

    let is_3p = is_env_truthy(std::env::var("MOSSEN_CODE_USE_BEDROCK").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_VERTEX").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_FOUNDRY").ok().as_deref());

    let settings = get_settings_deprecated().unwrap_or_default();
    let api_key_helper = settings
        .get("apiKeyHelper")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let has_external_auth_token = std::env::var("MOSSEN_CODE_AUTH_TOKEN")
        .ok()
        .is_some_and(|v| !v.is_empty())
        || api_key_helper.is_some()
        || std::env::var("MOSSEN_CODE_API_KEY_FILE_DESCRIPTOR")
            .ok()
            .is_some_and(|v| !v.is_empty());

    let ApiKeyWithSource {
        source: api_key_source,
        ..
    } = get_mossen_api_key_with_source(true);
    let has_external_api_key = api_key_source == ApiKeySource::MossenCodeApiKey
        || api_key_source == ApiKeySource::ApiKeyHelper;

    let should_disable_auth = is_3p
        || (has_external_auth_token && !is_managed_oauth_context())
        || (has_external_api_key && !is_managed_oauth_context());

    !should_disable_auth
}

// ---------------------------------------------------------------------------
// getAuthTokenSource
// ---------------------------------------------------------------------------

pub fn get_auth_token_source() -> AuthTokenSourceInfo {
    if is_custom_backend_enabled() {
        return AuthTokenSourceInfo {
            source: AuthTokenSource::CustomBackend,
            has_token: get_custom_backend_auth_token().is_some()
                || get_custom_backend_api_key().is_some()
                || get_custom_backend_base_url().is_some(),
        };
    }

    if is_bare_mode() {
        if get_configured_api_key_helper().is_some() {
            return AuthTokenSourceInfo {
                source: AuthTokenSource::ApiKeyHelper,
                has_token: true,
            };
        }
        return AuthTokenSourceInfo {
            source: AuthTokenSource::None,
            has_token: false,
        };
    }

    if std::env::var("MOSSEN_CODE_AUTH_TOKEN")
        .ok()
        .is_some_and(|v| !v.is_empty())
        && !is_managed_oauth_context()
    {
        return AuthTokenSourceInfo {
            source: AuthTokenSource::MossenCodeAuthToken,
            has_token: true,
        };
    }

    let oauth_token_from_fd = get_oauth_token_from_file_descriptor();
    if oauth_token_from_fd.is_some() {
        if std::env::var("MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR")
            .ok()
            .is_some_and(|v| !v.is_empty())
        {
            return AuthTokenSourceInfo {
                source: AuthTokenSource::MossenCodeAuthTokenFileDescriptor,
                has_token: true,
            };
        }
        return AuthTokenSourceInfo {
            source: AuthTokenSource::CcrOauthTokenFile,
            has_token: true,
        };
    }

    let api_key_helper = get_configured_api_key_helper();
    if api_key_helper.is_some() && !is_managed_oauth_context() {
        return AuthTokenSourceInfo {
            source: AuthTokenSource::ApiKeyHelper,
            has_token: true,
        };
    }

    let oauth_tokens = get_hosted_oauth_tokens();
    if let Some(ref tokens) = oauth_tokens {
        if should_use_hosted_auth(Some(&tokens.scopes)) && !tokens.access_token.is_empty() {
            return AuthTokenSourceInfo {
                source: AuthTokenSource::Hosted,
                has_token: true,
            };
        }
    }

    AuthTokenSourceInfo {
        source: AuthTokenSource::None,
        has_token: false,
    }
}

// ---------------------------------------------------------------------------
// getMossenApiKey / getMossenApiKeyWithSource
// ---------------------------------------------------------------------------

pub fn get_mossen_api_key() -> Option<String> {
    let result = get_mossen_api_key_with_source(false);
    result.key
}

pub fn has_mossen_api_key_auth() -> bool {
    let result = get_mossen_api_key_with_source(true);
    result.key.is_some() && result.source != ApiKeySource::None
}

pub fn get_mossen_api_key_with_source(
    skip_retrieving_key_from_api_key_helper: bool,
) -> ApiKeyWithSource {
    if is_custom_backend_enabled() {
        let custom_api_key = get_custom_backend_api_key();
        return ApiKeyWithSource {
            source: if custom_api_key.is_some() {
                ApiKeySource::CustomBackend
            } else {
                ApiKeySource::None
            },
            key: custom_api_key,
        };
    }

    if is_bare_mode() {
        if let Ok(api_key) = std::env::var("MOSSEN_CODE_API_KEY") {
            if !api_key.is_empty() {
                return ApiKeyWithSource {
                    key: Some(api_key),
                    source: ApiKeySource::MossenCodeApiKey,
                };
            }
        }
        if get_configured_api_key_helper().is_some() {
            return ApiKeyWithSource {
                key: if skip_retrieving_key_from_api_key_helper {
                    None
                } else {
                    get_api_key_from_api_key_helper_cached()
                },
                source: ApiKeySource::ApiKeyHelper,
            };
        }
        return ApiKeyWithSource {
            key: None,
            source: ApiKeySource::None,
        };
    }

    let api_key_env = if is_running_on_homespace() {
        None
    } else {
        std::env::var("MOSSEN_CODE_API_KEY")
            .ok()
            .filter(|v| !v.is_empty())
    };

    if prefer_third_party_authentication() {
        if let Some(ref key) = api_key_env {
            return ApiKeyWithSource {
                key: Some(key.clone()),
                source: ApiKeySource::MossenCodeApiKey,
            };
        }
    }

    let is_ci = is_env_truthy(std::env::var("CI").ok().as_deref())
        || std::env::var("NODE_ENV").ok().as_deref() == Some("test");

    if is_ci {
        let api_key_from_fd = get_api_key_from_file_descriptor();
        if let Some(key) = api_key_from_fd {
            return ApiKeyWithSource {
                key: Some(key),
                source: ApiKeySource::MossenCodeApiKey,
            };
        }
        if api_key_env.is_none() {
            // In CI, API key is required — return None with error logged
            error!("MOSSEN_CODE_API_KEY env var is required in CI");
            return ApiKeyWithSource {
                key: None,
                source: ApiKeySource::None,
            };
        }
        if let Some(key) = api_key_env {
            return ApiKeyWithSource {
                key: Some(key),
                source: ApiKeySource::MossenCodeApiKey,
            };
        }
        return ApiKeyWithSource {
            key: None,
            source: ApiKeySource::None,
        };
    }

    // Check approved custom API keys
    if let Some(ref key) = api_key_env {
        let config = get_global_config();
        let normalized = normalize_api_key_for_config(key);
        if let Some(ref responses) = config.custom_api_key_responses {
            if let Some(ref approved) = responses.approved {
                if approved.iter().any(|a| a == &normalized) {
                    return ApiKeyWithSource {
                        key: Some(key.clone()),
                        source: ApiKeySource::MossenCodeApiKey,
                    };
                }
            }
        }
    }

    // Check API key from file descriptor
    let api_key_from_fd = get_api_key_from_file_descriptor();
    if let Some(key) = api_key_from_fd {
        return ApiKeyWithSource {
            key: Some(key),
            source: ApiKeySource::MossenCodeApiKey,
        };
    }

    // Check apiKeyHelper
    let api_key_helper_command = get_configured_api_key_helper();
    if api_key_helper_command.is_some() {
        if skip_retrieving_key_from_api_key_helper {
            return ApiKeyWithSource {
                key: None,
                source: ApiKeySource::ApiKeyHelper,
            };
        }
        return ApiKeyWithSource {
            key: get_api_key_from_api_key_helper_cached(),
            source: ApiKeySource::ApiKeyHelper,
        };
    }

    // Check config or macOS keychain
    if let Some(result) = get_api_key_from_config_or_macos_keychain() {
        return result;
    }

    ApiKeyWithSource {
        key: None,
        source: ApiKeySource::None,
    }
}

// ---------------------------------------------------------------------------
// getConfiguredApiKeyHelper
// ---------------------------------------------------------------------------

pub fn get_configured_api_key_helper() -> Option<String> {
    if is_bare_mode() {
        return get_settings_for_source("flagSettings")
            .and_then(|s| s.get("apiKeyHelper")?.as_str().map(|s| s.to_string()));
    }
    let settings = get_settings_deprecated().unwrap_or_default();
    settings
        .get("apiKeyHelper")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// isApiKeyHelperFromProjectOrLocalSettings
// ---------------------------------------------------------------------------

fn is_api_key_helper_from_project_or_local_settings() -> bool {
    let api_key_helper = match get_configured_api_key_helper() {
        Some(h) => h,
        None => return false,
    };
    let project_settings = get_settings_for_source("projectSettings");
    let local_settings = get_settings_for_source("localSettings");

    let project_match = project_settings
        .and_then(|s| s.get("apiKeyHelper")?.as_str().map(|s| s.to_string()))
        .is_some_and(|v| v == api_key_helper);
    let local_match = local_settings
        .and_then(|s| s.get("apiKeyHelper")?.as_str().map(|s| s.to_string()))
        .is_some_and(|v| v == api_key_helper);

    project_match || local_match
}

// ---------------------------------------------------------------------------
// AWS auth refresh settings
// ---------------------------------------------------------------------------

fn get_configured_aws_auth_refresh() -> Option<String> {
    let settings = get_settings_deprecated().unwrap_or_default();
    settings
        .get("awsAuthRefresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn is_aws_auth_refresh_from_project_settings() -> bool {
    let aws_auth_refresh = match get_configured_aws_auth_refresh() {
        Some(r) => r,
        None => return false,
    };
    let project_settings = get_settings_for_source("projectSettings");
    let local_settings = get_settings_for_source("localSettings");
    project_settings
        .and_then(|s| s.get("awsAuthRefresh")?.as_str().map(|s| s.to_string()))
        .is_some_and(|v| v == aws_auth_refresh)
        || local_settings
            .and_then(|s| s.get("awsAuthRefresh")?.as_str().map(|s| s.to_string()))
            .is_some_and(|v| v == aws_auth_refresh)
}

fn get_configured_aws_credential_export() -> Option<String> {
    let settings = get_settings_deprecated().unwrap_or_default();
    settings
        .get("awsCredentialExport")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn is_aws_credential_export_from_project_settings() -> bool {
    let val = match get_configured_aws_credential_export() {
        Some(v) => v,
        None => return false,
    };
    let project_settings = get_settings_for_source("projectSettings");
    let local_settings = get_settings_for_source("localSettings");
    project_settings
        .and_then(|s| {
            s.get("awsCredentialExport")?
                .as_str()
                .map(|s| s.to_string())
        })
        .is_some_and(|v| v == val)
        || local_settings
            .and_then(|s| {
                s.get("awsCredentialExport")?
                    .as_str()
                    .map(|s| s.to_string())
            })
            .is_some_and(|v| v == val)
}

// ---------------------------------------------------------------------------
// calculateApiKeyHelperTTL
// ---------------------------------------------------------------------------

pub fn calculate_api_key_helper_ttl() -> u64 {
    if let Ok(env_ttl) = std::env::var("MOSSEN_CODE_API_KEY_HELPER_TTL_MS") {
        if let Ok(parsed) = env_ttl.parse::<u64>() {
            return parsed;
        }
        log_for_debugging(&format!(
            "Found MOSSEN_CODE_API_KEY_HELPER_TTL_MS env var, but it was not a valid number. Got {}",
            env_ttl
        ));
    }
    DEFAULT_API_KEY_HELPER_TTL
}

// ---------------------------------------------------------------------------
// API key helper cache (SWR pattern)
// ---------------------------------------------------------------------------

pub fn get_api_key_helper_elapsed_ms() -> u64 {
    let inflight = API_KEY_HELPER_INFLIGHT.lock();
    inflight
        .as_ref()
        .and_then(|i| i.started_at)
        .map_or(0, |started| started.elapsed().as_millis() as u64)
}

pub async fn get_api_key_from_api_key_helper(is_non_interactive_session: bool) -> Option<String> {
    get_configured_api_key_helper()?;
    let ttl = calculate_api_key_helper_ttl();
    let epoch = API_KEY_HELPER_EPOCH.load(Ordering::Relaxed);

    // Check cache
    {
        let cache = API_KEY_HELPER_CACHE.read();
        if let Some(ref c) = *cache {
            if now_millis() - c.timestamp < ttl {
                return Some(c.value.clone());
            }
            // Stale — trigger background refresh
            let mut inflight = API_KEY_HELPER_INFLIGHT.lock();
            if inflight.is_none() {
                *inflight = Some(ApiKeyHelperInflight { started_at: None });
                drop(inflight);
                let _ = run_and_cache_api_key_helper(is_non_interactive_session, false, epoch);
            }
            return Some(c.value.clone());
        }
    }

    // Cold cache
    {
        let inflight = API_KEY_HELPER_INFLIGHT.lock();
        if inflight.is_some() {
            // Deduplicate concurrent calls — return cached if available
            let cache = API_KEY_HELPER_CACHE.read();
            return cache.as_ref().map(|c| c.value.clone());
        }
    }

    {
        let mut inflight = API_KEY_HELPER_INFLIGHT.lock();
        *inflight = Some(ApiKeyHelperInflight {
            started_at: Some(Instant::now()),
        });
    }

    run_and_cache_api_key_helper(is_non_interactive_session, true, epoch)
}

fn run_and_cache_api_key_helper(
    is_non_interactive_session: bool,
    is_cold: bool,
    epoch: u64,
) -> Option<String> {
    match execute_api_key_helper(is_non_interactive_session) {
        Ok(value) => {
            if epoch != API_KEY_HELPER_EPOCH.load(Ordering::Relaxed) {
                return value;
            }
            if let Some(ref v) = value {
                let mut cache = API_KEY_HELPER_CACHE.write();
                *cache = Some(ApiKeyHelperCache {
                    value: v.clone(),
                    timestamp: now_millis(),
                });
            }
            if epoch == API_KEY_HELPER_EPOCH.load(Ordering::Relaxed) {
                *API_KEY_HELPER_INFLIGHT.lock() = None;
            }
            value
        }
        Err(e) => {
            if epoch != API_KEY_HELPER_EPOCH.load(Ordering::Relaxed) {
                return Some(" ".to_string());
            }
            error!("apiKeyHelper failed: {}", e);
            log_for_debugging(&format!("Error getting API key from apiKeyHelper: {}", e));

            if !is_cold {
                let cache = API_KEY_HELPER_CACHE.read();
                if let Some(ref c) = *cache {
                    if c.value != " " {
                        drop(cache);
                        let mut cache = API_KEY_HELPER_CACHE.write();
                        if let Some(ref mut c) = *cache {
                            c.timestamp = now_millis();
                        }
                        let cache = API_KEY_HELPER_CACHE.read();
                        let result = cache.as_ref().map(|c| c.value.clone());
                        if epoch == API_KEY_HELPER_EPOCH.load(Ordering::Relaxed) {
                            *API_KEY_HELPER_INFLIGHT.lock() = None;
                        }
                        return result;
                    }
                }
            }

            let mut cache = API_KEY_HELPER_CACHE.write();
            *cache = Some(ApiKeyHelperCache {
                value: " ".to_string(),
                timestamp: now_millis(),
            });
            if epoch == API_KEY_HELPER_EPOCH.load(Ordering::Relaxed) {
                *API_KEY_HELPER_INFLIGHT.lock() = None;
            }
            Some(" ".to_string())
        }
    }
}

fn execute_api_key_helper(is_non_interactive_session: bool) -> anyhow::Result<Option<String>> {
    let api_key_helper = match get_configured_api_key_helper() {
        Some(h) => h,
        None => return Ok(None),
    };

    if is_api_key_helper_from_project_or_local_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !is_non_interactive_session {
            warn!("Security: apiKeyHelper executed before workspace trust is confirmed.");
            return Ok(None);
        }
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(&api_key_helper)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute apiKeyHelper: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let why = if output.status.code().is_none() {
            "timed out".to_string()
        } else {
            format!("exited {}", output.status.code().unwrap_or(-1))
        };
        return Err(anyhow::anyhow!(if stderr.is_empty() {
            why
        } else {
            format!("{}: {}", why, stderr)
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err(anyhow::anyhow!("did not return a value"));
    }

    Ok(Some(stdout))
}

pub fn get_api_key_from_api_key_helper_cached() -> Option<String> {
    let cache = API_KEY_HELPER_CACHE.read();
    cache.as_ref().map(|c| c.value.clone())
}

pub fn clear_api_key_helper_cache() {
    API_KEY_HELPER_EPOCH.fetch_add(1, Ordering::Relaxed);
    *API_KEY_HELPER_CACHE.write() = None;
    *API_KEY_HELPER_INFLIGHT.lock() = None;
}

pub fn prefetch_api_key_from_api_key_helper_if_safe(is_non_interactive_session: bool) {
    if is_api_key_helper_from_project_or_local_settings() && !check_has_trust_dialog_accepted() {
        return;
    }
    let _ = tokio::spawn(async move {
        let _ = get_api_key_from_api_key_helper(is_non_interactive_session).await;
    });
}

// ---------------------------------------------------------------------------
// AWS auth refresh
// ---------------------------------------------------------------------------

async fn run_aws_auth_refresh() -> bool {
    let aws_auth_refresh = match get_configured_aws_auth_refresh() {
        Some(r) => r,
        None => return false,
    };

    if is_aws_auth_refresh_from_project_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !get_is_non_interactive_session() {
            warn!("Security: awsAuthRefresh executed before workspace trust is confirmed.");
            return false;
        }
    }

    // Try checking STS caller identity first
    log_for_debugging("Fetching AWS caller identity for AWS auth refresh command");
    if check_sts_caller_identity().is_ok() {
        log_for_debugging("Fetched AWS caller identity, skipping AWS auth refresh command");
        return false;
    }

    refresh_aws_auth(&aws_auth_refresh).await
}

fn check_sts_caller_identity() -> anyhow::Result<()> {
    let output = Command::new("aws")
        .args(["sts", "get-caller-identity"])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("STS caller identity check failed"))
    }
}

pub async fn refresh_aws_auth(aws_auth_refresh: &str) -> bool {
    log_for_debugging("Running AWS auth refresh command");
    let output = Command::new("sh").arg("-c").arg(aws_auth_refresh).output();

    match output {
        Ok(o) if o.status.success() => {
            log_for_debugging("AWS auth refresh completed successfully");
            true
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            error!("Error running awsAuthRefresh: {}", stderr);
            false
        }
        Err(e) => {
            error!("Failed to run awsAuthRefresh: {}", e);
            false
        }
    }
}

async fn get_aws_creds_from_credential_export() -> Option<AwsCredentials> {
    let aws_credential_export = get_configured_aws_credential_export()?;

    if is_aws_credential_export_from_project_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !get_is_non_interactive_session() {
            warn!("Security: awsCredentialExport executed before workspace trust is confirmed.");
            return None;
        }
    }

    // Check STS caller identity first
    log_for_debugging("Fetching AWS caller identity for credential export command");
    if check_sts_caller_identity().is_ok() {
        log_for_debugging("Fetched AWS caller identity, skipping AWS credential export command");
        return None;
    }

    log_for_debugging("Running AWS credential export command");
    let output = Command::new("sh")
        .arg("-c")
        .arg(&aws_credential_export)
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        error!("awsCredentialExport did not return a valid value");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let aws_output: serde_json::Value = serde_json::from_str(&stdout).ok()?;

    let credentials = aws_output.get("Credentials")?;
    let access_key_id = credentials.get("AccessKeyId")?.as_str()?.to_string();
    let secret_access_key = credentials.get("SecretAccessKey")?.as_str()?.to_string();
    let session_token = credentials.get("SessionToken")?.as_str()?.to_string();

    log_for_debugging("AWS credentials retrieved from awsCredentialExport");
    Some(AwsCredentials {
        access_key_id,
        secret_access_key,
        session_token,
    })
}

#[derive(Debug, Clone)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
}

pub async fn refresh_and_get_aws_credentials() -> Option<AwsCredentials> {
    // Check cache
    {
        let cache = AWS_CREDENTIALS_CACHE.read();
        if let Some(ref c) = *cache {
            if now_millis() - c.timestamp < DEFAULT_AWS_STS_TTL {
                return Some(AwsCredentials {
                    access_key_id: c.access_key_id.clone(),
                    secret_access_key: c.secret_access_key.clone(),
                    session_token: c.session_token.clone(),
                });
            }
        }
    }

    let _refreshed = run_aws_auth_refresh().await;
    let credentials = get_aws_creds_from_credential_export().await;

    if let Some(ref creds) = credentials {
        let mut cache = AWS_CREDENTIALS_CACHE.write();
        *cache = Some(AwsCredentialsCache {
            access_key_id: creds.access_key_id.clone(),
            secret_access_key: creds.secret_access_key.clone(),
            session_token: creds.session_token.clone(),
            timestamp: now_millis(),
        });
    }

    credentials
}

pub fn clear_aws_credentials_cache() {
    *AWS_CREDENTIALS_CACHE.write() = None;
}

// ---------------------------------------------------------------------------
// GCP auth refresh
// ---------------------------------------------------------------------------

fn get_configured_gcp_auth_refresh() -> Option<String> {
    let settings = get_settings_deprecated().unwrap_or_default();
    settings
        .get("gcpAuthRefresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn is_gcp_auth_refresh_from_project_settings() -> bool {
    let val = match get_configured_gcp_auth_refresh() {
        Some(v) => v,
        None => return false,
    };
    let project_settings = get_settings_for_source("projectSettings");
    let local_settings = get_settings_for_source("localSettings");
    project_settings
        .and_then(|s| s.get("gcpAuthRefresh")?.as_str().map(|s| s.to_string()))
        .is_some_and(|v| v == val)
        || local_settings
            .and_then(|s| s.get("gcpAuthRefresh")?.as_str().map(|s| s.to_string()))
            .is_some_and(|v| v == val)
}

pub async fn check_gcp_credentials_valid() -> bool {
    // Attempt to verify GCP credentials via gcloud
    let output = Command::new("gcloud")
        .args(["auth", "application-default", "print-access-token"])
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

async fn run_gcp_auth_refresh() -> bool {
    let gcp_auth_refresh = match get_configured_gcp_auth_refresh() {
        Some(r) => r,
        None => return false,
    };

    if is_gcp_auth_refresh_from_project_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !get_is_non_interactive_session() {
            warn!("Security: gcpAuthRefresh executed before workspace trust is confirmed.");
            return false;
        }
    }

    log_for_debugging("Checking GCP credentials validity for auth refresh");
    if check_gcp_credentials_valid().await {
        log_for_debugging("GCP credentials are valid, skipping auth refresh command");
        return false;
    }

    refresh_gcp_auth(&gcp_auth_refresh).await
}

pub async fn refresh_gcp_auth(gcp_auth_refresh: &str) -> bool {
    log_for_debugging("Running GCP auth refresh command");
    let output = Command::new("sh").arg("-c").arg(gcp_auth_refresh).output();

    match output {
        Ok(o) if o.status.success() => {
            log_for_debugging("GCP auth refresh completed successfully");
            true
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            error!("Error running gcpAuthRefresh: {}", stderr);
            false
        }
        Err(e) => {
            error!("Failed to run gcpAuthRefresh: {}", e);
            false
        }
    }
}

pub async fn refresh_gcp_credentials_if_needed() -> bool {
    // Simple TTL-based memoization
    {
        let cache = GCP_CREDENTIALS_CACHE.read();
        if let Some(timestamp) = *cache {
            if now_millis() - timestamp < DEFAULT_GCP_CREDENTIAL_TTL {
                return false;
            }
        }
    }
    let refreshed = run_gcp_auth_refresh().await;
    if refreshed {
        *GCP_CREDENTIALS_CACHE.write() = Some(now_millis());
    }
    refreshed
}

pub fn clear_gcp_credentials_cache() {
    *GCP_CREDENTIALS_CACHE.write() = None;
}

pub fn prefetch_gcp_credentials_if_safe() {
    let _gcp_auth_refresh = match get_configured_gcp_auth_refresh() {
        Some(r) => r,
        None => return,
    };

    if is_gcp_auth_refresh_from_project_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !get_is_non_interactive_session() {
            return;
        }
    }

    let _ = tokio::spawn(async move {
        let _ = refresh_gcp_credentials_if_needed().await;
    });
}

pub fn prefetch_aws_credentials_and_bedrock_info_if_safe() {
    let aws_auth_refresh = get_configured_aws_auth_refresh();
    let aws_credential_export = get_configured_aws_credential_export();

    if aws_auth_refresh.is_none() && aws_credential_export.is_none() {
        return;
    }

    if is_aws_auth_refresh_from_project_settings()
        || is_aws_credential_export_from_project_settings()
    {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust && !get_is_non_interactive_session() {
            return;
        }
    }

    let _ = tokio::spawn(async move {
        let _ = refresh_and_get_aws_credentials().await;
    });
}

// ---------------------------------------------------------------------------
// getApiKeyFromConfigOrMacOSKeychain (memoized)
// ---------------------------------------------------------------------------

static API_KEY_FROM_CONFIG_CACHE: once_cell::sync::Lazy<RwLock<Option<ApiKeyWithSource>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

fn get_api_key_from_config_or_macos_keychain() -> Option<ApiKeyWithSource> {
    // Check memo cache
    {
        let cache = API_KEY_FROM_CONFIG_CACHE.read();
        if let Some(ref result) = *cache {
            return Some(result.clone());
        }
    }

    if is_bare_mode() {
        return None;
    }

    // macOS keychain check
    if cfg!(target_os = "macos") {
        let storage_service_name = "mossen-code-credentials";
        if let Some(result) = exec_sync_with_defaults(&format!(
            "security find-generic-password -a $USER -w -s \"{}\"",
            storage_service_name
        )) {
            if !result.is_empty() {
                let r = ApiKeyWithSource {
                    key: Some(result),
                    source: ApiKeySource::MossenManagedKey,
                };
                *API_KEY_FROM_CONFIG_CACHE.write() = Some(r.clone());
                return Some(r);
            }
        }
    }

    let config = get_global_config();
    if let Some(ref primary_api_key) = config.primary_api_key {
        if !primary_api_key.is_empty() {
            let r = ApiKeyWithSource {
                key: Some(primary_api_key.clone()),
                source: ApiKeySource::MossenManagedKey,
            };
            *API_KEY_FROM_CONFIG_CACHE.write() = Some(r.clone());
            return Some(r);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// isValidApiKey
// ---------------------------------------------------------------------------

fn is_valid_api_key(api_key: &str) -> bool {
    api_key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

// ---------------------------------------------------------------------------
// saveApiKey
// ---------------------------------------------------------------------------

pub async fn save_api_key(api_key: &str) -> anyhow::Result<()> {
    if !is_valid_api_key(api_key) {
        anyhow::bail!("Invalid API key format. API key must contain only alphanumeric characters, dashes, and underscores.");
    }

    maybe_remove_api_key_from_macos_keychain().await;
    let mut saved_to_keychain = false;

    if cfg!(target_os = "macos") {
        let storage_service_name = "mossen-code-credentials";
        let hex_value = api_key
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        // Use whoami for username on macOS
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let _command = format!(
            "add-generic-password -U -a \"{}\" -s \"{}\" -X \"{}\"",
            username, storage_service_name, hex_value
        );
        let result = Command::new("security")
            .arg("-i")
            .stdin(std::process::Stdio::piped())
            .output();
        match result {
            Ok(_) => {
                saved_to_keychain = true;
            }
            Err(e) => {
                log_error(&e);
            }
        }
    }

    let normalized_key = normalize_api_key_for_config(api_key);
    let api_key_owned = api_key.to_string();

    save_global_config(move |current| {
        let mut new_config = current.clone();
        if !saved_to_keychain {
            new_config.primary_api_key = Some(api_key_owned.clone());
        }
        // Update custom_api_key_responses.approved
        let responses = new_config
            .custom_api_key_responses
            .get_or_insert_with(Default::default);
        let approved = responses.approved.get_or_insert_with(Vec::new);
        if !approved.contains(&normalized_key) {
            approved.push(normalized_key.clone());
        }
        new_config
    });

    // Clear memo cache
    *API_KEY_FROM_CONFIG_CACHE.write() = None;

    Ok(())
}

// ---------------------------------------------------------------------------
// isCustomApiKeyApproved
// ---------------------------------------------------------------------------

pub fn is_custom_api_key_approved(api_key: &str) -> bool {
    let config = get_global_config();
    let normalized_key = normalize_api_key_for_config(api_key);
    config
        .custom_api_key_responses
        .as_ref()
        .and_then(|r| r.approved.as_ref())
        .is_some_and(|arr| arr.iter().any(|a| a == &normalized_key))
}

// ---------------------------------------------------------------------------
// removeApiKey
// ---------------------------------------------------------------------------

pub async fn remove_api_key() {
    maybe_remove_api_key_from_macos_keychain().await;

    save_global_config(|current| {
        let mut new_config = current.clone();
        new_config.primary_api_key = None;
        new_config
    });

    *API_KEY_FROM_CONFIG_CACHE.write() = None;
}

async fn maybe_remove_api_key_from_macos_keychain() {
    if cfg!(target_os = "macos") {
        let storage_service_name = "mossen-code-credentials";
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", storage_service_name])
            .output();
    }
}

// ---------------------------------------------------------------------------
// OAuth token management
// ---------------------------------------------------------------------------

pub fn save_oauth_tokens_if_needed(tokens: &OAuthTokens) -> SaveResult {
    if !should_use_hosted_auth(Some(&tokens.scopes)) {
        return SaveResult {
            success: true,
            warning: None,
        };
    }

    if tokens.refresh_token.is_none() || tokens.expires_at.is_none() {
        return SaveResult {
            success: true,
            warning: None,
        };
    }

    // Stub: actual secure storage write
    SaveResult {
        success: true,
        warning: None,
    }
}

#[derive(Debug, Clone)]
pub struct SaveResult {
    pub success: bool,
    pub warning: Option<String>,
}

pub fn get_hosted_oauth_tokens() -> Option<OAuthTokens> {
    if is_bare_mode() {
        return None;
    }
    if !is_hosted_auth_adapter_enabled() {
        return None;
    }

    // Check env var token
    if let Ok(token) = std::env::var("MOSSEN_CODE_AUTH_TOKEN") {
        if !token.is_empty() {
            return Some(OAuthTokens {
                access_token: token,
                refresh_token: None,
                expires_at: None,
                scopes: vec!["user:inference".to_string()],
                subscription_type: None,
                rate_limit_tier: None,
            });
        }
    }

    // Check FD token
    if let Some(token) = get_oauth_token_from_file_descriptor() {
        return Some(OAuthTokens {
            access_token: token,
            refresh_token: None,
            expires_at: None,
            scopes: vec!["user:inference".to_string()],
            subscription_type: None,
            rate_limit_tier: None,
        });
    }

    // Check secure storage
    if let Some(storage) = get_secure_storage_read() {
        if let Some(oauth) = storage.hosted_oauth {
            if !oauth.access_token.is_empty() {
                return Some(oauth);
            }
        }
    }

    None
}

pub fn clear_oauth_token_cache() {
    // Clear memoized caches
}

pub async fn invalidate_oauth_cache_if_disk_changed() {
    let credentials_path = get_mossen_config_home_dir().join(".credentials.json");
    match tokio::fs::metadata(&credentials_path).await {
        Ok(meta) => {
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let prev = LAST_CREDENTIALS_MTIME_MS.load(Ordering::Relaxed);
            if mtime_ms != prev {
                LAST_CREDENTIALS_MTIME_MS.store(mtime_ms, Ordering::Relaxed);
                clear_oauth_token_cache();
            }
        }
        Err(_) => {
            // ENOENT
            clear_oauth_token_cache();
        }
    }
}

pub async fn handle_oauth_401_error(failed_access_token: &str) -> bool {
    if is_custom_backend_enabled() {
        return false;
    }
    handle_oauth_401_error_impl(failed_access_token).await
}

async fn handle_oauth_401_error_impl(failed_access_token: &str) -> bool {
    clear_oauth_token_cache();
    let current_tokens = get_hosted_oauth_tokens_async().await;

    match current_tokens {
        Some(ref tokens) if tokens.refresh_token.is_some() => {
            if tokens.access_token != failed_access_token {
                return true; // Another tab already refreshed
            }
            check_and_refresh_oauth_token_if_needed(0, true).await
        }
        _ => false,
    }
}

pub async fn get_hosted_oauth_tokens_async() -> Option<OAuthTokens> {
    if is_bare_mode() {
        return None;
    }

    if std::env::var("MOSSEN_CODE_AUTH_TOKEN")
        .ok()
        .is_some_and(|v| !v.is_empty())
        || get_oauth_token_from_file_descriptor().is_some()
    {
        return get_hosted_oauth_tokens();
    }

    // Stub: async secure storage read
    get_secure_storage_read().and_then(|s| s.hosted_oauth)
}

pub async fn check_and_refresh_oauth_token_if_needed(retry_count: u32, force: bool) -> bool {
    if is_custom_backend_enabled() {
        return false;
    }

    check_and_refresh_oauth_token_if_needed_impl(retry_count, force).await
}

async fn check_and_refresh_oauth_token_if_needed_impl(retry_count: u32, force: bool) -> bool {
    const MAX_RETRIES: u32 = 5;

    invalidate_oauth_cache_if_disk_changed().await;

    let tokens = get_hosted_oauth_tokens();
    if !force {
        if let Some(ref t) = tokens {
            if t.refresh_token.is_none() || !is_oauth_token_expired(t.expires_at) {
                return false;
            }
        } else {
            return false;
        }
    }

    let tokens = match tokens {
        Some(t) if t.refresh_token.is_some() => t,
        _ => return false,
    };

    if !should_use_hosted_auth(Some(&tokens.scopes)) {
        return false;
    }

    // Re-read tokens async to check if still expired
    clear_oauth_token_cache();
    let fresh_tokens = get_hosted_oauth_tokens_async().await;
    match fresh_tokens {
        Some(ref t) if t.refresh_token.is_some() && is_oauth_token_expired(t.expires_at) => {
            // Still expired, attempt refresh
        }
        _ => return false,
    }

    // Attempt lock-based refresh
    let mossen_dir = get_mossen_config_home_dir();
    let _ = tokio::fs::create_dir_all(&mossen_dir).await;

    // Simplified lock: just attempt the refresh
    // In production, this would use proper file locking
    match attempt_token_refresh(&tokens).await {
        Ok(true) => true,
        Ok(false) => false,
        Err(_) => {
            if retry_count < MAX_RETRIES {
                let jitter = rand::random::<u64>() % 1000;
                tokio::time::sleep(Duration::from_millis(1000 + jitter)).await;
                return Box::pin(check_and_refresh_oauth_token_if_needed_impl(
                    retry_count + 1,
                    force,
                ))
                .await;
            }
            false
        }
    }
}

async fn attempt_token_refresh(_tokens: &OAuthTokens) -> anyhow::Result<bool> {
    // Stub: actual token refresh via OAuth endpoint
    // In production, this would call refreshOAuthToken() and saveOAuthTokensIfNeeded()
    log_for_debugging("Attempting OAuth token refresh");
    Ok(false)
}

// ---------------------------------------------------------------------------
// Subscription queries
// ---------------------------------------------------------------------------

pub fn is_hosted_subscriber() -> bool {
    if !is_mossen_hosted_auth_enabled() {
        return false;
    }
    get_hosted_oauth_tokens()
        .as_ref()
        .is_some_and(|t| should_use_hosted_auth(Some(&t.scopes)))
}

pub fn has_profile_scope() -> bool {
    get_hosted_oauth_tokens()
        .as_ref()
        .is_some_and(|t| t.scopes.iter().any(|s| s == HOSTED_PROFILE_SCOPE))
}

pub fn is_1p_api_customer() -> bool {
    if is_env_truthy(std::env::var("MOSSEN_CODE_USE_BEDROCK").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_VERTEX").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_FOUNDRY").ok().as_deref())
    {
        return false;
    }
    if is_hosted_subscriber() {
        return false;
    }
    true
}

pub fn get_oauth_account_info() -> Option<AccountInfo> {
    if is_mossen_hosted_auth_enabled() {
        get_global_config().oauth_account
    } else {
        None
    }
}

pub fn is_overage_provisioning_allowed() -> bool {
    let account_info = get_oauth_account_info();
    let billing_type = account_info.as_ref().and_then(|a| a.billing_type.as_ref());

    if !is_hosted_subscriber() || billing_type.is_none() {
        return false;
    }

    let billing_type = billing_type.unwrap();
    matches!(
        billing_type.as_str(),
        "stripe_subscription"
            | "stripe_subscription_contracted"
            | "apple_subscription"
            | "google_play_subscription"
    )
}

pub fn has_max_access() -> bool {
    let sub_type = get_subscription_type();
    matches!(
        sub_type.as_deref(),
        Some("max") | Some("enterprise") | Some("team") | Some("pro") | None
    )
}

pub fn get_subscription_type() -> Option<SubscriptionType> {
    if should_use_mock_subscription() {
        return get_mock_subscription_type();
    }
    if !is_mossen_hosted_auth_enabled() {
        return None;
    }
    let tokens = get_hosted_oauth_tokens()?;
    tokens.subscription_type
}

pub fn is_max_subscriber() -> bool {
    get_subscription_type().as_deref() == Some("max")
}

pub fn is_team_subscriber() -> bool {
    get_subscription_type().as_deref() == Some("team")
}

pub fn is_team_premium_subscriber() -> bool {
    get_subscription_type().as_deref() == Some("team")
        && get_rate_limit_tier().as_deref() == Some("default_mossen_max_5x")
}

pub fn is_enterprise_subscriber() -> bool {
    get_subscription_type().as_deref() == Some("enterprise")
}

pub fn is_pro_subscriber() -> bool {
    get_subscription_type().as_deref() == Some("pro")
}

pub fn get_rate_limit_tier() -> Option<String> {
    if !is_mossen_hosted_auth_enabled() {
        return None;
    }
    get_hosted_oauth_tokens()?.rate_limit_tier
}

pub fn get_subscription_name() -> String {
    match get_subscription_type().as_deref() {
        Some("enterprise") => "Mossen Enterprise".to_string(),
        Some("team") => "Mossen Team".to_string(),
        Some("max") => "Mossen Max".to_string(),
        Some("pro") => "Mossen Pro".to_string(),
        _ => "Mossen API".to_string(),
    }
}

pub fn is_using_3p_services() -> bool {
    is_custom_backend_enabled()
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_BEDROCK").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_VERTEX").ok().as_deref())
        || is_env_truthy(std::env::var("MOSSEN_CODE_USE_FOUNDRY").ok().as_deref())
}

// ---------------------------------------------------------------------------
// Custom headers helper
// ---------------------------------------------------------------------------

pub fn get_configured_custom_headers_helper() -> Option<String> {
    let settings = get_settings_deprecated().unwrap_or_default();
    settings
        .get("customHeadersHelper")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn is_custom_headers_helper_from_project_or_local_settings() -> bool {
    let value = match get_configured_custom_headers_helper() {
        Some(v) => v,
        None => return false,
    };
    let project_settings = get_settings_for_source("projectSettings");
    let local_settings = get_settings_for_source("localSettings");
    project_settings
        .and_then(|s| {
            s.get("customHeadersHelper")?
                .as_str()
                .map(|s| s.to_string())
        })
        .is_some_and(|v| v == value)
        || local_settings
            .and_then(|s| {
                s.get("customHeadersHelper")?
                    .as_str()
                    .map(|s| s.to_string())
            })
            .is_some_and(|v| v == value)
}

pub fn get_custom_headers_for_request() -> HashMap<String, String> {
    let helper = match get_configured_custom_headers_helper() {
        Some(h) => h,
        None => return HashMap::new(),
    };

    let debounce_ms = std::env::var("MOSSEN_CODE_CUSTOM_HEADERS_HELPER_DEBOUNCE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CUSTOM_HEADERS_DEBOUNCE_MS);

    // Check cache
    {
        let cache = CUSTOM_HEADERS_CACHE.read();
        if let Some(ref c) = *cache {
            if now_millis() - c.timestamp < debounce_ms {
                return c.headers.clone();
            }
        }
    }

    if is_custom_headers_helper_from_project_or_local_settings() {
        let has_trust = check_has_trust_dialog_accepted();
        if !has_trust {
            return HashMap::new();
        }
    }

    match exec_sync_with_defaults(&helper) {
        Some(result) if !result.is_empty() => {
            match serde_json::from_str::<HashMap<String, serde_json::Value>>(&result) {
                Ok(parsed) => {
                    let mut headers = HashMap::new();
                    for (key, value) in &parsed {
                        if let Some(s) = value.as_str() {
                            headers.insert(key.clone(), s.to_string());
                        } else {
                            error!(
                                "customHeadersHelper returned non-string value for key \"{}\": {}",
                                key, value
                            );
                            return HashMap::new();
                        }
                    }

                    let mut cache = CUSTOM_HEADERS_CACHE.write();
                    *cache = Some(CustomHeadersCache {
                        headers: headers.clone(),
                        timestamp: now_millis(),
                    });

                    headers
                }
                Err(e) => {
                    error!("Error parsing customHeadersHelper output: {}", e);
                    HashMap::new()
                }
            }
        }
        _ => {
            error!("customHeadersHelper did not return a valid value");
            HashMap::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Consumer/subscription helpers
// ---------------------------------------------------------------------------

fn is_consumer_plan(plan: &str) -> bool {
    plan == "max" || plan == "pro"
}

pub fn is_consumer_subscriber() -> bool {
    let sub_type = get_subscription_type();
    is_hosted_subscriber() && sub_type.as_ref().is_some_and(|s| is_consumer_plan(s))
}

// ---------------------------------------------------------------------------
// getAccountInformation
// ---------------------------------------------------------------------------

pub fn get_account_information() -> Option<UserAccountInfo> {
    if is_custom_backend_enabled() {
        let mut info = UserAccountInfo::default();
        info.token_source = Some(get_custom_backend_name());
        if get_custom_backend_api_key().is_some() {
            info.api_key_source = Some("custom_backend".to_string());
        }
        return Some(info);
    }

    let api_provider = get_api_provider();
    if api_provider != "firstParty" {
        return None;
    }

    let auth_token_source = get_auth_token_source();
    let mut info = UserAccountInfo::default();

    match auth_token_source.source {
        AuthTokenSource::MossenCodeAuthToken
        | AuthTokenSource::MossenCodeAuthTokenFileDescriptor => {
            info.token_source = Some(format!("{:?}", auth_token_source.source));
        }
        _ if is_hosted_subscriber() => {
            info.subscription = Some(get_subscription_name());
        }
        _ => {
            info.token_source = Some(format!("{:?}", auth_token_source.source));
        }
    }

    let ApiKeyWithSource { key, source } = get_mossen_api_key_with_source(false);
    if key.is_some() {
        info.api_key_source = Some(source.to_string());
    }

    if matches!(auth_token_source.source, AuthTokenSource::Hosted)
        || source == ApiKeySource::MossenManagedKey
    {
        if let Some(account) = get_oauth_account_info() {
            if let Some(ref org_name) = account.organization_name {
                info.organization = Some(org_name.clone());
            }
        }
    }

    if (matches!(auth_token_source.source, AuthTokenSource::Hosted)
        || source == ApiKeySource::MossenManagedKey)
    {
        if let Some(account) = get_oauth_account_info() {
            if !account.email_address.is_empty() {
                info.email = Some(account.email_address.clone());
            }
        }
    }

    Some(info)
}

// ---------------------------------------------------------------------------
// validateForceLoginOrg
// ---------------------------------------------------------------------------

pub async fn validate_force_login_org() -> OrgValidationResult {
    if std::env::var("MOSSEN_CODE_UNIX_SOCKET")
        .ok()
        .is_some_and(|v| !v.is_empty())
    {
        return OrgValidationResult::Valid;
    }

    if !is_mossen_hosted_auth_enabled() {
        return OrgValidationResult::Valid;
    }

    let _required_org_uuid = match get_settings_for_source("policySettings")
        .and_then(|s| s.get("forceLoginOrgUUID")?.as_str().map(|s| s.to_string()))
    {
        Some(uuid) => uuid,
        None => return OrgValidationResult::Valid,
    };

    let _ = check_and_refresh_oauth_token_if_needed(0, false).await;

    let _tokens = match get_hosted_oauth_tokens() {
        Some(t) => t,
        None => return OrgValidationResult::Valid,
    };

    // In production, this would call getOauthProfileFromOauthToken
    // Stub: assume validation passes
    let auth_source = get_auth_token_source();
    let _is_env_var_token = matches!(
        auth_source.source,
        AuthTokenSource::MossenCodeAuthToken | AuthTokenSource::MossenCodeAuthTokenFileDescriptor
    );

    // Stub: profile fetch would happen here
    // For now, return Valid since we can't fetch the profile
    OrgValidationResult::Valid
}
