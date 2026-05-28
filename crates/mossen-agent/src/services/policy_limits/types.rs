//! Policy limits types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Single restriction entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRestriction {
    pub allowed: bool,
}

/// Policy limits API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyLimitsResponse {
    pub restrictions: HashMap<String, PolicyRestriction>,
}

/// Result of fetching policy limits.
#[derive(Debug, Clone)]
pub struct PolicyLimitsFetchResult {
    pub success: bool,
    /// None means 304 Not Modified (cache is valid).
    pub restrictions: Option<HashMap<String, PolicyRestriction>>,
    pub etag: Option<String>,
    pub error: Option<String>,
    /// If true, don't retry on failure (e.g., auth errors).
    pub skip_retry: bool,
}

impl PolicyLimitsFetchResult {
    pub fn success(
        restrictions: Option<HashMap<String, PolicyRestriction>>,
        etag: Option<String>,
    ) -> Self {
        Self {
            success: true,
            restrictions,
            etag,
            error: None,
            skip_retry: false,
        }
    }

    pub fn failure(error: String, skip_retry: bool) -> Self {
        Self {
            success: false,
            restrictions: None,
            etag: None,
            error: Some(error),
            skip_retry,
        }
    }
}

/// Alias for the policy limits response validator (mirrors TS `PolicyLimitsResponseSchema`).
pub type PolicyLimitsResponseSchema = PolicyLimitsResponse;
