use regex::Regex;
use tracing::debug;

use super::schemas::MarketplaceSource;

/// Format plugin failure details for user display.
pub fn format_failure_details(failures: &[FailureInfo], include_reasons: bool) -> String {
    let max_show = 2;
    let details: Vec<String> = failures
        .iter()
        .take(max_show)
        .map(|f| {
            let reason = f
                .reason
                .as_deref()
                .or(f.error.as_deref())
                .unwrap_or("unknown error");
            if include_reasons {
                format!("{} ({})", f.name, reason)
            } else {
                f.name.clone()
            }
        })
        .collect();

    let separator = if include_reasons { "; " } else { ", " };
    let mut result = details.join(separator);

    let remaining = failures.len().saturating_sub(max_show);
    if remaining > 0 {
        result.push_str(&format!(" and {} more", remaining));
    }
    result
}

#[derive(Debug, Clone)]
pub struct FailureInfo {
    pub name: String,
    pub reason: Option<String>,
    pub error: Option<String>,
}

/// Extract source display string from marketplace configuration.
pub fn get_marketplace_source_display(source: &MarketplaceSource) -> String {
    match source {
        MarketplaceSource::GitHub { repo, .. } => repo.clone(),
        MarketplaceSource::Url { url, .. } => url.clone(),
        MarketplaceSource::Git { url, .. } => url.clone(),
        MarketplaceSource::Directory { path } => path.clone(),
        MarketplaceSource::File { path } => path.clone(),
        MarketplaceSource::Settings { name, .. } => format!("settings:{}", name),
        _ => "Unknown source".to_string(),
    }
}

/// Create a plugin ID from plugin name and marketplace name.
pub fn create_plugin_id(plugin_name: &str, marketplace_name: &str) -> String {
    format!("{}@{}", plugin_name, marketplace_name)
}

/// Load marketplaces with graceful degradation for individual failures.
pub struct MarketplaceLoadResult {
    pub marketplaces: Vec<LoadedMarketplace>,
    pub failures: Vec<MarketplaceFailure>,
}

#[derive(Debug, Clone)]
pub struct LoadedMarketplace {
    pub name: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct MarketplaceFailure {
    pub name: String,
    pub error: String,
}

/// Format marketplace loading failures.
pub fn format_marketplace_loading_errors(
    failures: &[MarketplaceFailure],
    success_count: usize,
) -> Option<(String, String)> {
    if failures.is_empty() {
        return None;
    }

    if success_count > 0 {
        let message = if failures.len() == 1 {
            format!(
                "Warning: Failed to load marketplace '{}': {}",
                failures[0].name, failures[0].error
            )
        } else {
            format!(
                "Warning: Failed to load {} marketplaces: {}",
                failures.len(),
                failures
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Some(("warning".to_string(), message))
    } else {
        let errors = failures
            .iter()
            .map(|f| format!("{}: {}", f.name, f.error))
            .collect::<Vec<_>>()
            .join("; ");
        Some((
            "error".to_string(),
            format!("Failed to load all marketplaces. Errors: {}", errors),
        ))
    }
}

/// Trait for policy settings access.
pub trait PolicySettingsProvider: Send + Sync {
    fn get_strict_known_marketplaces(&self) -> Option<Vec<MarketplaceSource>>;
    fn get_blocked_marketplaces(&self) -> Option<Vec<MarketplaceSource>>;
    fn get_plugin_trust_message(&self) -> Option<String>;
}

/// Get the strict marketplace source allowlist from policy settings.
pub fn get_strict_known_marketplaces(
    provider: &dyn PolicySettingsProvider,
) -> Option<Vec<MarketplaceSource>> {
    provider.get_strict_known_marketplaces()
}

/// Get the marketplace source blocklist from policy settings.
pub fn get_blocked_marketplaces(
    provider: &dyn PolicySettingsProvider,
) -> Option<Vec<MarketplaceSource>> {
    provider.get_blocked_marketplaces()
}

/// Get the custom plugin trust message from policy settings.
pub fn get_plugin_trust_message(provider: &dyn PolicySettingsProvider) -> Option<String> {
    provider.get_plugin_trust_message()
}

/// Compare two MarketplaceSource objects for equality.
fn are_sources_equal(a: &MarketplaceSource, b: &MarketplaceSource) -> bool {
    match (a, b) {
        (MarketplaceSource::Url { url: a_url, .. }, MarketplaceSource::Url { url: b_url, .. }) => {
            a_url == b_url
        }
        (
            MarketplaceSource::GitHub {
                repo: a_repo,
                git_ref: a_ref,
                path: a_path,
                ..
            },
            MarketplaceSource::GitHub {
                repo: b_repo,
                git_ref: b_ref,
                path: b_path,
                ..
            },
        ) => a_repo == b_repo && a_ref == b_ref && a_path == b_path,
        (
            MarketplaceSource::Git {
                url: a_url,
                git_ref: a_ref,
                path: a_path,
                ..
            },
            MarketplaceSource::Git {
                url: b_url,
                git_ref: b_ref,
                path: b_path,
                ..
            },
        ) => a_url == b_url && a_ref == b_ref && a_path == b_path,
        (MarketplaceSource::File { path: a_p }, MarketplaceSource::File { path: b_p }) => {
            a_p == b_p
        }
        (
            MarketplaceSource::Directory { path: a_p },
            MarketplaceSource::Directory { path: b_p },
        ) => a_p == b_p,
        (
            MarketplaceSource::Settings {
                name: a_name,
                plugins: a_plugins,
                ..
            },
            MarketplaceSource::Settings {
                name: b_name,
                plugins: b_plugins,
                ..
            },
        ) => a_name == b_name && a_plugins == b_plugins,
        _ => false,
    }
}

/// Extract the host/domain from a marketplace source.
pub fn extract_host_from_source(source: &MarketplaceSource) -> Option<String> {
    match source {
        MarketplaceSource::GitHub { .. } => Some("github.com".to_string()),
        MarketplaceSource::Git { url, .. } => {
            // SSH format: user@HOST:path
            let ssh_re = Regex::new(r"^[^@]+@([^:]+):").unwrap();
            if let Some(caps) = ssh_re.captures(url) {
                return Some(caps[1].to_string());
            }
            // HTTPS format
            url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
        }
        MarketplaceSource::Url { url, .. } => url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string())),
        _ => None,
    }
}

/// Check if a source matches a hostPattern entry.
fn does_source_match_host_pattern(source: &MarketplaceSource, pattern: &str) -> bool {
    let host = match extract_host_from_source(source) {
        Some(h) => h,
        None => return false,
    };
    match Regex::new(pattern) {
        Ok(re) => re.is_match(&host),
        Err(_) => {
            debug!("Invalid hostPattern regex: {}", pattern);
            false
        }
    }
}

/// Check if a source matches a pathPattern entry.
fn does_source_match_path_pattern(source: &MarketplaceSource, pattern: &str) -> bool {
    let path = match source {
        MarketplaceSource::File { path } | MarketplaceSource::Directory { path } => path.as_str(),
        _ => return false,
    };
    match Regex::new(pattern) {
        Ok(re) => re.is_match(path),
        Err(_) => {
            debug!("Invalid pathPattern regex: {}", pattern);
            false
        }
    }
}

/// Get hosts from hostPattern entries in the allowlist.
pub fn get_host_patterns_from_allowlist(provider: &dyn PolicySettingsProvider) -> Vec<String> {
    let allowlist = match provider.get_strict_known_marketplaces() {
        Some(a) => a,
        None => return Vec::new(),
    };
    allowlist
        .iter()
        .filter_map(|entry| {
            if let MarketplaceSource::HostPattern { host_pattern } = entry {
                Some(host_pattern.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Check if a marketplace source is explicitly in the blocklist.
pub fn is_source_in_blocklist(
    source: &MarketplaceSource,
    provider: &dyn PolicySettingsProvider,
) -> bool {
    let blocklist = match provider.get_blocked_marketplaces() {
        Some(b) => b,
        None => return false,
    };
    blocklist
        .iter()
        .any(|blocked| are_sources_equivalent_for_blocklist(source, blocked))
}

/// Check if two sources are equivalent for blocklist purposes.
fn are_sources_equivalent_for_blocklist(
    source: &MarketplaceSource,
    blocked: &MarketplaceSource,
) -> bool {
    // Same type comparison
    if std::mem::discriminant(source) == std::mem::discriminant(blocked) {
        return are_sources_equal(source, blocked);
    }

    // Cross-type: git source matches github blocklist
    if let (MarketplaceSource::Git { url, .. }, MarketplaceSource::GitHub { repo, .. }) =
        (source, blocked)
    {
        if let Some(extracted) = extract_github_repo_from_git_url(url) {
            return &extracted == repo;
        }
    }

    // Cross-type: github source matches git blocklist
    if let (MarketplaceSource::GitHub { repo, .. }, MarketplaceSource::Git { url, .. }) =
        (source, blocked)
    {
        if let Some(extracted) = extract_github_repo_from_git_url(url) {
            return repo == &extracted;
        }
    }

    false
}

fn extract_github_repo_from_git_url(url: &str) -> Option<String> {
    // SSH format: git@github.com:owner/repo.git
    let ssh_re = Regex::new(r"^git@github\.com:([^/]+/[^/]+?)(?:\.git)?$").unwrap();
    if let Some(caps) = ssh_re.captures(url) {
        return Some(caps[1].to_string());
    }
    // HTTPS format
    let https_re = Regex::new(r"^https?://github\.com/([^/]+/[^/]+?)(?:\.git)?$").unwrap();
    if let Some(caps) = https_re.captures(url) {
        return Some(caps[1].to_string());
    }
    None
}

/// Check if a marketplace source is allowed by enterprise policy.
pub fn is_source_allowed_by_policy(
    source: &MarketplaceSource,
    provider: &dyn PolicySettingsProvider,
) -> bool {
    // Check blocklist first
    if is_source_in_blocklist(source, provider) {
        return false;
    }

    // Then check allowlist
    let allowlist = match provider.get_strict_known_marketplaces() {
        Some(a) => a,
        None => return true, // No restrictions
    };

    allowlist.iter().any(|allowed| match allowed {
        MarketplaceSource::HostPattern { host_pattern } => {
            does_source_match_host_pattern(source, host_pattern)
        }
        MarketplaceSource::PathPattern { path_pattern } => {
            does_source_match_path_pattern(source, path_pattern)
        }
        _ => are_sources_equal(source, allowed),
    })
}

/// Format a MarketplaceSource for display in error messages.
pub fn format_source_for_display(source: &MarketplaceSource) -> String {
    match source {
        MarketplaceSource::GitHub { repo, git_ref, .. } => {
            let ref_str = git_ref
                .as_ref()
                .map(|r| format!("@{}", r))
                .unwrap_or_default();
            format!("github:{}{}", repo, ref_str)
        }
        MarketplaceSource::Url { url, .. } => url.clone(),
        MarketplaceSource::Git { url, git_ref, .. } => {
            let ref_str = git_ref
                .as_ref()
                .map(|r| format!("@{}", r))
                .unwrap_or_default();
            format!("git:{}{}", url, ref_str)
        }
        MarketplaceSource::File { path } => format!("file:{}", path),
        MarketplaceSource::Directory { path } => format!("dir:{}", path),
        MarketplaceSource::HostPattern { host_pattern } => format!("hostPattern:{}", host_pattern),
        MarketplaceSource::PathPattern { path_pattern } => format!("pathPattern:{}", path_pattern),
        MarketplaceSource::Settings { name, plugins, .. } => {
            let count = plugins.len();
            let plural = if count == 1 { "plugin" } else { "plugins" };
            format!("settings:{} ({} {})", name, count, plural)
        }
        _ => "unknown source".to_string(),
    }
}

/// Reasons why no marketplaces are available.
#[derive(Debug, Clone, PartialEq)]
pub enum EmptyMarketplaceReason {
    GitNotInstalled,
    AllBlockedByPolicy,
    PolicyRestrictsSources,
    AllMarketplacesFailed,
    NoMarketplacesConfigured,
    AllPluginsInstalled,
}

/// Detect why no marketplaces are available.
pub async fn detect_empty_marketplace_reason(
    configured_count: usize,
    failed_count: usize,
    git_available: bool,
    provider: &dyn PolicySettingsProvider,
) -> EmptyMarketplaceReason {
    if !git_available {
        return EmptyMarketplaceReason::GitNotInstalled;
    }

    let allowlist = provider.get_strict_known_marketplaces();
    if let Some(ref list) = allowlist {
        if list.is_empty() {
            return EmptyMarketplaceReason::AllBlockedByPolicy;
        }
        if configured_count == 0 {
            return EmptyMarketplaceReason::PolicyRestrictsSources;
        }
    }

    if configured_count == 0 {
        return EmptyMarketplaceReason::NoMarketplacesConfigured;
    }

    if failed_count > 0 && failed_count == configured_count {
        return EmptyMarketplaceReason::AllMarketplacesFailed;
    }

    EmptyMarketplaceReason::AllPluginsInstalled
}

/// 对应 TS `loadMarketplacesWithGracefulDegradation`：加载 marketplace 列表，
/// 失败的项以错误形式返回，成功的子集继续可用。
pub async fn load_marketplaces_with_graceful_degradation(
    marketplaces: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for entry in marketplaces {
        if entry.get("name").is_some() {
            ok.push(entry);
        } else {
            errors.push("marketplace entry missing `name`".to_string());
        }
    }
    (ok, errors)
}
