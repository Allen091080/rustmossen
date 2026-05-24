//! # hosted_feature_gates — 托管功能门控
//!
//! 对应 TypeScript `utils/hostedFeatureGates.ts`。
//! 托管订阅者的功能门控。

use crate::auth::is_hosted_subscriber;

/// 是否启用了自定义后端（TS `isCustomBackendEnabled`）。
fn is_custom_backend_enabled() -> bool {
    let truthy = matches!(
        std::env::var("MOSSEN_CODE_USE_CUSTOM_BACKEND")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    let has_base_url = std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some();
    truthy || has_base_url
}

/// 示例主机名（`platform.example` 或以 `.example` 结尾）视为未配置。
fn is_placeholder_hosted_platform_url(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or("").to_lowercase();
            host == "platform.example" || host.ends_with(".example")
        }
        Err(_) => true,
    }
}

/// 收集 hosted platform URL —— 与 TS `getHostedPlatformUrls()` 行为等价：
/// 环境变量缺省时按 custom backend 与否回落到示例 host（`platform.example`）。
fn hosted_platform_urls() -> Vec<String> {
    let custom = is_custom_backend_enabled();
    let base = std::env::var("MOSSEN_CODE_PLATFORM_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if custom {
                "https://platform.example".to_string()
            } else {
                "https://platform.mossen.invalid".to_string()
            }
        });
    let envs = [
        "MOSSEN_CODE_GITHUB_APP_URL",
        "MOSSEN_CODE_REMOTE_BASE_URL",
        "MOSSEN_CODE_REMOTE_SETUP_URL",
        "MOSSEN_CODE_UPGRADE_URL",
        "MOSSEN_CODE_USAGE_URL",
        "MOSSEN_CODE_PRIVACY_URL",
    ];
    let mut urls: Vec<String> = envs
        .iter()
        .filter_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .collect();
    urls.push(base);
    urls
}

fn has_configured_hosted_platform_urls() -> bool {
    if !is_custom_backend_enabled() {
        return true;
    }
    hosted_platform_urls()
        .iter()
        .all(|u| !is_placeholder_hosted_platform_url(u))
}

fn has_chrome_command_access() -> bool {
    // TS `isCustomChromeEnabled()` 仅检查 MOSSEN_CODE_ENABLE_CHROME。
    let chrome_on = matches!(
        std::env::var("MOSSEN_CODE_ENABLE_CHROME").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    chrome_on && has_configured_hosted_platform_urls()
}

/// 检查当前后端是否配置了平台。
pub fn has_configured_platform_for_current_backend() -> bool {
    !is_custom_backend_enabled() || has_configured_hosted_platform_urls()
}

/// 检查是否可以使用 Chrome 集成。
pub fn can_use_chrome_integration() -> bool {
    has_chrome_command_access() || (!is_custom_backend_enabled() && is_hosted_subscriber())
}

/// 检查是否可以使用托管工作区功能。
pub fn can_use_hosted_workspace_features() -> bool {
    if is_custom_backend_enabled() {
        has_configured_hosted_platform_urls()
    } else {
        is_hosted_subscriber()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_detection() {
        assert!(is_placeholder_hosted_platform_url(
            "https://platform.example"
        ));
        assert!(is_placeholder_hosted_platform_url("https://sub.example"));
        assert!(!is_placeholder_hosted_platform_url(
            "https://platform.mossen.invalid"
        ));
    }
}
