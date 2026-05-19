//! # Ultrareview Quota API
//!
//! 翻译自 `services/api/ultrareviewQuota.ts` (39行)
//! 提供 Ultrareview 配额查询。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Ultrareview quota response from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltrareviewQuotaResponse {
    pub reviews_used: u64,
    pub reviews_limit: u64,
    pub reviews_remaining: u64,
    pub is_overage: bool,
}

/// Peek the ultrareview quota for display and nudge decisions.
/// Consume happens server-side at session creation.
/// Returns None when not a subscriber or the endpoint errors.
pub async fn fetch_ultrareview_quota(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    is_hosted_subscriber: bool,
) -> Option<UltrareviewQuotaResponse> {
    if !is_hosted_subscriber {
        return None;
    }

    match fetch_ultrareview_quota_inner(client, base_api_url, access_token, org_uuid).await {
        Ok(resp) => Some(resp),
        Err(e) => {
            debug!("fetchUltrareviewQuota failed: {}", e);
            None
        }
    }
}

async fn fetch_ultrareview_quota_inner(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Result<UltrareviewQuotaResponse, anyhow::Error> {
    let url = format!("{}/v1/ultrareview/quota", base_api_url);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: UltrareviewQuotaResponse = response.json().await?;
    Ok(data)
}
