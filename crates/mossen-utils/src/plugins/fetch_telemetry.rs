//! Plugin fetch telemetry.
//!
//! Translated from `utils/plugins/fetchTelemetry.ts` (135 lines).

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use tracing::debug;

use super::official_marketplace::OFFICIAL_MARKETPLACE_NAME;

/// Source of plugin/marketplace fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFetchSource {
    InstallCounts,
    MarketplaceClone,
    MarketplacePull,
    MarketplaceUrl,
    PluginClone,
    Mcpb,
}

impl std::fmt::Display for PluginFetchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstallCounts => write!(f, "install_counts"),
            Self::MarketplaceClone => write!(f, "marketplace_clone"),
            Self::MarketplacePull => write!(f, "marketplace_pull"),
            Self::MarketplaceUrl => write!(f, "marketplace_url"),
            Self::PluginClone => write!(f, "plugin_clone"),
            Self::Mcpb => write!(f, "mcpb"),
        }
    }
}

/// Outcome of plugin fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFetchOutcome {
    Success,
    Failure,
    CacheHit,
}

impl std::fmt::Display for PluginFetchOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::CacheHit => write!(f, "cache_hit"),
        }
    }
}

/// Allowlist of public hosts we report by name.
static KNOWN_PUBLIC_HOSTS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("github.com");
    s.insert("raw.githubusercontent.com");
    s.insert("objects.githubusercontent.com");
    s.insert("gist.githubusercontent.com");
    s.insert("gitlab.com");
    s.insert("bitbucket.org");
    s.insert("codeberg.org");
    s.insert("dev.azure.com");
    s.insert("ssh.dev.azure.com");
    s.insert("storage.googleapis.com");
    s
});

/// Extract hostname from a URL or git spec and bucket to the allowlist.
fn extract_host(url_or_spec: &str) -> &'static str {
    static SCP_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^[^@/]+@([^:/]+):").unwrap());

    let host = if let Some(caps) = SCP_RE.captures(url_or_spec) {
        caps.get(1).map(|m| m.as_str().to_lowercase())
    } else {
        url::Url::parse(url_or_spec)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
    };

    match host {
        Some(h) => {
            for &known in KNOWN_PUBLIC_HOSTS.iter() {
                if h == known {
                    return known;
                }
            }
            "other"
        }
        None => "unknown",
    }
}

/// True if the URL/spec points at mossen/mossen-plugins-official.
fn is_official_repo(url_or_spec: &str) -> bool {
    url_or_spec.contains(&format!("mossen/{}", OFFICIAL_MARKETPLACE_NAME))
}

/// Log a plugin fetch telemetry event.
pub fn log_plugin_fetch(
    source: PluginFetchSource,
    url_or_spec: Option<&str>,
    outcome: PluginFetchOutcome,
    duration_ms: f64,
    error_kind: Option<&str>,
) {
    let host = url_or_spec.map(extract_host).unwrap_or("unknown");
    let is_official = url_or_spec.map(is_official_repo).unwrap_or(false);

    debug!(
        event = "tengu_plugin_remote_fetch",
        source = %source,
        host = host,
        is_official = is_official,
        outcome = %outcome,
        duration_ms = duration_ms.round() as i64,
        error_kind = error_kind.unwrap_or(""),
    );
}

/// Classify an error into a stable bucket for the error_kind field.
pub fn classify_fetch_error(error: &str) -> &'static str {
    static DNS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)ENOTFOUND|ECONNREFUSED|EAI_AGAIN|Could not resolve host|Connection refused").unwrap()
    });
    static TIMEOUT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)ETIMEDOUT|timed out|timeout").unwrap());
    static RESET_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)ECONNRESET|socket hang up|Connection reset by peer|remote end hung up").unwrap()
    });
    static AUTH_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)403|401|authentication|permission denied").unwrap());
    static NOT_FOUND_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)404|not found|repository not found").unwrap());
    static TLS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)certificate|SSL|TLS|unable to get local issuer").unwrap());
    static SCHEMA_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)Invalid response format|Invalid marketplace schema").unwrap()
    });

    if DNS_RE.is_match(error) {
        return "dns_or_refused";
    }
    if TIMEOUT_RE.is_match(error) {
        return "timeout";
    }
    if RESET_RE.is_match(error) {
        return "conn_reset";
    }
    if AUTH_RE.is_match(error) {
        return "auth";
    }
    if NOT_FOUND_RE.is_match(error) {
        return "not_found";
    }
    if TLS_RE.is_match(error) {
        return "tls";
    }
    if SCHEMA_RE.is_match(error) {
        return "invalid_schema";
    }
    "other"
}
