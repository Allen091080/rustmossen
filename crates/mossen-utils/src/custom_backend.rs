// Custom backend configuration, URL management, and authentication headers.

use std::collections::HashMap;
use std::env;

/// Custom backend protocol types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomBackendProtocol {
    MossenCompatible,
    OpenaiCompatible,
    Private,
}

impl CustomBackendProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MossenCompatible => "mossen-compatible",
            Self::OpenaiCompatible => "openai-compatible",
            Self::Private => "private",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "mossen-compatible" => Some(Self::MossenCompatible),
            "openai-compatible" => Some(Self::OpenaiCompatible),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomBackendConfig {
    pub api_key: Option<String>,
    pub auth_token: Option<String>,
    pub base_url: String,
    pub headers: HashMap<String, String>,
    pub max_input_tokens: Option<u64>,
    pub model: Option<String>,
    pub name: String,
    pub protocol: CustomBackendProtocol,
}

#[derive(Debug, Clone)]
pub struct ChromeIntegrationUrls {
    pub docs_url: String,
    pub extension_url: String,
    pub focus_tab_url_base: String,
    pub permissions_url: String,
    pub reconnect_url: String,
}

#[derive(Debug, Clone)]
pub struct HostedPlatformUrls {
    pub bedrock_docs_url: String,
    pub connectors_url: String,
    pub desktop_docs_url: String,
    pub desktop_mac_download_url: String,
    pub desktop_windows_download_url: String,
    pub foundry_docs_url: String,
    pub github_app_url: String,
    pub github_actions_docs_url: String,
    pub privacy_url: String,
    pub remote_base_url: String,
    pub remote_environment_url: String,
    pub remote_setup_url: String,
    pub remote_web_url: String,
    pub security_docs_url: String,
    pub upgrade_url: String,
    pub usage_url: String,
    pub vertex_docs_url: String,
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

fn strip_1m_suffix(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(stripped) = trimmed.strip_suffix("[1m]") {
        stripped.trim().to_string()
    } else if let Some(stripped) = trimmed.strip_suffix("[1M]") {
        stripped.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_env_truthy(val: Option<&str>) -> bool {
    match val {
        Some(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"),
        None => false,
    }
}

fn is_env_defined_falsy(val: Option<&str>) -> bool {
    match val {
        Some(v) => matches!(v.to_lowercase().as_str(), "0" | "false" | "no"),
        None => false,
    }
}

pub fn is_placeholder_hosted_platform_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => {
            if let Some(host) = parsed.host_str() {
                let lower = host.to_lowercase();
                lower == "platform.example" || lower.ends_with(".example")
            } else {
                true
            }
        }
        Err(_) => true,
    }
}

pub fn is_custom_backend_enabled() -> bool {
    if is_env_defined_falsy(env::var("MOSSEN_CODE_USE_CUSTOM_BACKEND").ok().as_deref()) {
        return false;
    }
    if is_env_truthy(env::var("MOSSEN_CODE_USE_CUSTOM_BACKEND").ok().as_deref()) {
        return true;
    }
    if env::var("MOSSEN_CODE_CUSTOM_BASE_URL").ok().is_some() {
        return true;
    }
    false
}

pub fn get_custom_backend_base_url() -> Option<String> {
    if let Ok(raw) = env::var("MOSSEN_CODE_CUSTOM_BASE_URL") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trim_trailing_slash(trimmed));
        }
    }
    None
}

pub fn get_custom_backend_api_key() -> Option<String> {
    let val = env::var("MOSSEN_CODE_CUSTOM_API_KEY").ok()?;
    let trimmed = val.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn get_custom_backend_auth_token() -> Option<String> {
    if let Ok(val) = env::var("MOSSEN_CODE_CUSTOM_AUTH_TOKEN") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub fn get_custom_backend_model() -> Option<String> {
    if let Ok(val) = env::var("MOSSEN_CODE_CUSTOM_MODEL") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub fn get_custom_backend_max_input_tokens() -> Option<u64> {
    let raw = env::var("MOSSEN_CODE_CUSTOM_MAX_INPUT_TOKENS").ok()?;
    let trimmed = raw.trim();
    let parsed: u64 = trimmed.parse().ok()?;
    if parsed > 0 {
        Some(parsed)
    } else {
        None
    }
}

pub fn custom_backend_capability_applies_to_model(model: &str) -> bool {
    if !is_custom_backend_enabled() {
        return false;
    }
    let configured_model = match get_custom_backend_model() {
        Some(m) => m,
        None => return true,
    };
    strip_1m_suffix(model).to_lowercase() == strip_1m_suffix(&configured_model).to_lowercase()
}

fn parse_headers(raw: Option<&str>) -> HashMap<String, String> {
    let raw = match raw {
        Some(r) if !r.trim().is_empty() => r.trim(),
        _ => return HashMap::new(),
    };

    if raw.starts_with('{') {
        // Try JSON parse
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(raw) {
            return parsed
                .into_iter()
                .filter_map(|(k, v)| {
                    if let serde_json::Value::String(s) = v {
                        Some((k.trim().to_string(), s.trim().to_string()))
                    } else {
                        None
                    }
                })
                .collect();
        }
        return HashMap::new();
    }

    // Line-based parsing
    let mut headers = HashMap::new();
    for line in raw.lines() {
        if let Some(sep_idx) = line.find(':') {
            let name = line[..sep_idx].trim();
            let value = line[sep_idx + 1..].trim();
            if !name.is_empty() {
                headers.insert(name.to_string(), value.to_string());
            }
        }
    }
    headers
}

pub fn get_custom_backend_headers() -> HashMap<String, String> {
    parse_headers(env::var("MOSSEN_CODE_CUSTOM_HEADERS").ok().as_deref())
}

pub fn get_custom_backend_protocol() -> CustomBackendProtocol {
    let value = env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").ok();
    if let Some(ref val) = value {
        let trimmed = val.trim();
        if let Some(p) = CustomBackendProtocol::from_str(trimmed) {
            return p;
        }
    }
    CustomBackendProtocol::MossenCompatible
}

pub fn get_custom_backend_auth_headers() -> HashMap<String, String> {
    let mut headers = get_custom_backend_headers();
    let auth_token = get_custom_backend_auth_token();
    let api_key = get_custom_backend_api_key();
    let protocol = get_custom_backend_protocol();

    if let Some(ref token) = auth_token {
        if !headers.contains_key("Authorization") {
            headers.insert("Authorization".to_string(), format!("Bearer {}", token));
        }
    }
    if let Some(ref key) = api_key {
        if !headers.contains_key("x-api-key")
            && !headers.contains_key("X-Api-Key")
            && !headers.contains_key("Authorization")
        {
            if protocol == CustomBackendProtocol::OpenaiCompatible {
                headers.insert("Authorization".to_string(), format!("Bearer {}", key));
            } else {
                headers.insert("x-api-key".to_string(), key.clone());
            }
        }
    }

    headers
}

pub fn has_custom_backend_auth() -> bool {
    !get_custom_backend_auth_headers().is_empty()
}

pub fn get_custom_backend_name() -> String {
    env::var("MOSSEN_CODE_CUSTOM_NAME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Custom backend".to_string())
}

fn resolve_custom_feature_flag(env_name: &str, default_when_custom_backend: bool) -> bool {
    let val = env::var(env_name).ok();
    if is_env_truthy(val.as_deref()) {
        return true;
    }
    if is_env_defined_falsy(val.as_deref()) {
        return false;
    }
    if is_custom_backend_enabled() {
        default_when_custom_backend
    } else {
        false
    }
}

pub fn is_custom_chrome_enabled() -> bool {
    resolve_custom_feature_flag("MOSSEN_CODE_ENABLE_CHROME", false)
}

pub fn is_custom_voice_enabled() -> bool {
    resolve_custom_feature_flag("MOSSEN_CODE_ENABLE_VOICE", true) && has_custom_backend_auth()
}

pub fn has_chrome_command_access() -> bool {
    is_custom_chrome_enabled() && has_configured_chrome_integration_urls()
}

pub fn has_configured_hosted_platform_urls() -> bool {
    if !is_custom_backend_enabled() {
        return true;
    }
    let urls = get_hosted_platform_urls();
    !is_placeholder_hosted_platform_url(&urls.remote_base_url)
        && !is_placeholder_hosted_platform_url(&urls.bedrock_docs_url)
        && !is_placeholder_hosted_platform_url(&urls.connectors_url)
}

pub fn has_configured_chrome_integration_urls() -> bool {
    if !is_custom_backend_enabled() {
        return true;
    }
    let urls = get_chrome_integration_urls();
    !is_placeholder_hosted_platform_url(&urls.docs_url)
        && !is_placeholder_hosted_platform_url(&urls.extension_url)
        && !is_placeholder_hosted_platform_url(&urls.focus_tab_url_base)
        && !is_placeholder_hosted_platform_url(&urls.permissions_url)
        && !is_placeholder_hosted_platform_url(&urls.reconnect_url)
}

pub fn has_configured_feedback_urls() -> bool {
    if !is_custom_backend_enabled() {
        return true;
    }
    let remote_base_url = get_hosted_platform_urls().remote_base_url;
    let issue_url = env::var("MOSSEN_CODE_PLATFORM_ISSUES_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/support/issues", remote_base_url));
    let feedback_url = env::var("MOSSEN_CODE_PLATFORM_FEEDBACK_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/api/feedback", remote_base_url));

    !is_placeholder_hosted_platform_url(&issue_url)
        && !is_placeholder_hosted_platform_url(&feedback_url)
}

pub fn get_chrome_integration_urls() -> ChromeIntegrationUrls {
    let is_custom = is_custom_backend_enabled();
    let remote_base_url = env::var("MOSSEN_CODE_PLATFORM_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if is_custom {
                "https://platform.example".to_string()
            } else {
                "https://platform.mossen.invalid".to_string()
            }
        });
    ChromeIntegrationUrls {
        extension_url: env::var("MOSSEN_CODE_CHROME_EXTENSION_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/chrome", remote_base_url)),
        focus_tab_url_base: env::var("MOSSEN_CODE_CHROME_FOCUS_TAB_URL_BASE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/chrome/tab/", remote_base_url)),
        permissions_url: env::var("MOSSEN_CODE_CHROME_PERMISSIONS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/chrome/permissions", remote_base_url)),
        reconnect_url: env::var("MOSSEN_CODE_CHROME_RECONNECT_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/chrome/reconnect", remote_base_url)),
        docs_url: env::var("MOSSEN_CODE_CHROME_DOCS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/docs/chrome", remote_base_url)),
    }
}

pub fn get_hosted_platform_urls() -> HostedPlatformUrls {
    let is_custom = is_custom_backend_enabled();
    let remote_base_url = env::var("MOSSEN_CODE_PLATFORM_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if is_custom {
                "https://platform.example".to_string()
            } else {
                "https://platform.mossen.invalid".to_string()
            }
        });
    let remote_web_url = env::var("MOSSEN_CODE_PLATFORM_WEB_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/code", remote_base_url));

    HostedPlatformUrls {
        bedrock_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_BEDROCK_DOCS_URL",
            &format!("{}/docs/providers/amazon-bedrock", remote_base_url),
        ),
        connectors_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_CONNECTORS_URL",
            &format!("{}/settings/connectors", remote_base_url),
        ),
        desktop_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_DESKTOP_DOCS_URL",
            &format!("{}/desktop", remote_base_url),
        ),
        desktop_mac_download_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_DESKTOP_MAC_URL",
            &format!("{}/downloads/desktop/macos", remote_base_url),
        ),
        desktop_windows_download_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_DESKTOP_WINDOWS_URL",
            &format!("{}/downloads/desktop/windows", remote_base_url),
        ),
        foundry_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_FOUNDRY_DOCS_URL",
            &format!("{}/docs/providers/microsoft-foundry", remote_base_url),
        ),
        github_app_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_GITHUB_APP_URL",
            &format!("{}/integrations/github/install", remote_base_url),
        ),
        github_actions_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_GITHUB_ACTIONS_DOCS_URL",
            &format!("{}/docs/github-actions", remote_base_url),
        ),
        privacy_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_PRIVACY_URL",
            &format!("{}/settings/privacy", remote_base_url),
        ),
        remote_base_url: remote_base_url.clone(),
        remote_environment_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_REMOTE_ENV_URL",
            &format!("{}/environments", remote_web_url),
        ),
        remote_setup_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_REMOTE_SETUP_URL",
            &format!("{}/setup", remote_web_url),
        ),
        remote_web_url,
        security_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_SECURITY_DOCS_URL",
            &format!("{}/docs/security", remote_base_url),
        ),
        upgrade_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_UPGRADE_URL",
            &format!("{}/billing/upgrade", remote_base_url),
        ),
        usage_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_USAGE_URL",
            &format!("{}/billing/usage", remote_base_url),
        ),
        vertex_docs_url: env_or_default(
            "MOSSEN_CODE_PLATFORM_VERTEX_DOCS_URL",
            &format!("{}/docs/providers/google-vertex-ai", remote_base_url),
        ),
    }
}

fn env_or_default(env_name: &str, default: &str) -> String {
    env::var(env_name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn get_desktop_companion_name() -> &'static str {
    "Mossen Desktop"
}

pub fn get_custom_voice_stream_base_url() -> Option<String> {
    let explicit = env::var("MOSSEN_CODE_CUSTOM_VOICE_BASE_URL").ok();
    if let Some(ref val) = explicit {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trim_trailing_slash(trimmed));
        }
    }
    get_custom_backend_base_url()
}

pub fn get_custom_backend_config() -> Option<CustomBackendConfig> {
    if !is_custom_backend_enabled() {
        return None;
    }
    let base_url = get_custom_backend_base_url()?;
    Some(CustomBackendConfig {
        api_key: get_custom_backend_api_key(),
        auth_token: get_custom_backend_auth_token(),
        base_url,
        headers: get_custom_backend_auth_headers(),
        max_input_tokens: get_custom_backend_max_input_tokens(),
        model: get_custom_backend_model(),
        name: get_custom_backend_name(),
        protocol: get_custom_backend_protocol(),
    })
}

/// 对应 TS `CUSTOM_BACKEND_PROTOCOLS`：支持的 custom backend 协议字面量集合。
pub const CUSTOM_BACKEND_PROTOCOLS: &[&str] =
    &["mossen-compatible", "openai-compatible", "private"];
