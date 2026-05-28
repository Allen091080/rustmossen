//! API provider detection helpers.
//!
//! Direct translation of `utils/model/providers.ts`.

use crate::custom_backend::is_custom_backend_enabled;
use crate::env::is_env_truthy;

/// Tag for the active API surface. Matches the TS string-literal union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum APIProvider {
    FirstParty,
    Bedrock,
    Vertex,
    Foundry,
}

impl APIProvider {
    /// Internal short string identifier (matches TS union member name).
    pub fn as_str(self) -> &'static str {
        match self {
            APIProvider::FirstParty => "firstParty",
            APIProvider::Bedrock => "bedrock",
            APIProvider::Vertex => "vertex",
            APIProvider::Foundry => "foundry",
        }
    }
}

fn env_truthy(name: &str) -> bool {
    is_env_truthy(std::env::var(name).ok().as_deref())
}

pub fn get_api_provider() -> APIProvider {
    if env_truthy("MOSSEN_CODE_USE_BEDROCK") {
        APIProvider::Bedrock
    } else if env_truthy("MOSSEN_CODE_USE_VERTEX") {
        APIProvider::Vertex
    } else if env_truthy("MOSSEN_CODE_USE_FOUNDRY") {
        APIProvider::Foundry
    } else {
        APIProvider::FirstParty
    }
}

/// TS `getAPIProviderForStatsig` — returns the same enum, cast to the
/// analytics metadata type. In Rust we just expose the same enum since callers
/// can adapt to whatever statsig metadata representation they use.
pub fn get_api_provider_for_statsig() -> APIProvider {
    get_api_provider()
}

/// Check if the Mossen API base URL points at a native hosted API URL.
pub fn is_first_party_mossen_base_url() -> bool {
    if is_custom_backend_enabled() {
        return false;
    }
    let base_url = match std::env::var("MOSSEN_CODE_API_BASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return false,
    };
    let parsed = match url::Url::parse(&base_url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };
    let mut allowed: Vec<&str> = vec!["api.mossen.invalid"];
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        allowed.push("api-staging.mossen.invalid");
    }
    allowed.contains(&host)
}
