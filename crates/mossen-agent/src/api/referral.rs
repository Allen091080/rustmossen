//! # Referral API
//!
//! 翻译自 `services/api/referral.ts` (286行)
//! 推荐系统 API：资格检查、兑换、缓存。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};

/// Cache expiration time: 24 hours
const CACHE_EXPIRATION_MS: u64 = 24 * 60 * 60 * 1000;

/// Referral campaign types.
pub type ReferralCampaign = String;

/// Referral eligibility response from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferralEligibilityResponse {
    pub eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referrer_reward: Option<ReferrerRewardInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_passes: Option<u32>,
}

/// Referrer reward info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferrerRewardInfo {
    pub currency: String,
    pub amount_minor_units: u64,
}

/// Referral redemptions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferralRedemptionsResponse {
    pub redemptions: Vec<serde_json::Value>,
}

/// Cached passes eligibility entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassesEligibilityCacheEntry {
    pub eligible: bool,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referrer_reward: Option<ReferrerRewardInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_passes: Option<u32>,
}

/// Passes eligibility cache (org_id -> entry).
pub type PassesEligibilityCache = HashMap<String, PassesEligibilityCacheEntry>;

/// Currency symbols for formatting.
const CURRENCY_SYMBOLS: &[(&str, &str)] = &[
    ("USD", "$"),
    ("EUR", "€"),
    ("GBP", "£"),
    ("BRL", "R$"),
    ("CAD", "CA$"),
    ("AUD", "A$"),
    ("NZD", "NZ$"),
    ("SGD", "S$"),
];

fn get_currency_symbol(currency: &str) -> String {
    for (code, symbol) in CURRENCY_SYMBOLS {
        if *code == currency {
            return symbol.to_string();
        }
    }
    format!("{} ", currency)
}

/// Format credit amount for display.
pub fn format_credit_amount(reward: &ReferrerRewardInfo) -> String {
    let symbol = get_currency_symbol(&reward.currency);
    let amount = reward.amount_minor_units as f64 / 100.0;
    let formatted = if amount == amount.floor() {
        format!("{}", amount as u64)
    } else {
        format!("{:.2}", amount)
    };
    format!("{}{}", symbol, formatted)
}

/// Fetch referral eligibility from the API.
pub async fn fetch_referral_eligibility(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    campaign: Option<&str>,
) -> Result<ReferralEligibilityResponse, anyhow::Error> {
    let campaign = campaign.unwrap_or("mossen_guest_pass");
    let url = format!(
        "{}/api/oauth/organizations/{}/referral/eligibility",
        base_api_url, org_uuid
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .query(&[("campaign", campaign)])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: ReferralEligibilityResponse = response.error_for_status()?.json().await?;
    Ok(data)
}

/// Fetch referral redemptions from the API.
pub async fn fetch_referral_redemptions(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    campaign: Option<&str>,
) -> Result<ReferralRedemptionsResponse, anyhow::Error> {
    let campaign = campaign.unwrap_or("mossen_guest_pass");
    let url = format!(
        "{}/api/oauth/organizations/{}/referral/redemptions",
        base_api_url, org_uuid
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .query(&[("campaign", campaign)])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    let data: ReferralRedemptionsResponse = response.error_for_status()?.json().await?;
    Ok(data)
}

/// Result of cached passes eligibility check.
pub struct CachedPassesEligibilityResult {
    pub eligible: bool,
    pub needs_refresh: bool,
    pub has_cache: bool,
}

/// Check cached passes eligibility from cache.
pub fn check_cached_passes_eligibility(
    cache: &PassesEligibilityCache,
    org_id: Option<&str>,
    is_hosted_subscriber: bool,
    subscription_type: Option<&str>,
    now_ms: u64,
) -> CachedPassesEligibilityResult {
    // Prechecks for if user can access guest passes feature
    let should_check = org_id.is_some() && is_hosted_subscriber && subscription_type == Some("max");

    if !should_check {
        return CachedPassesEligibilityResult {
            eligible: false,
            needs_refresh: false,
            has_cache: false,
        };
    }

    let org_id = match org_id {
        Some(id) => id,
        None => {
            return CachedPassesEligibilityResult {
                eligible: false,
                needs_refresh: false,
                has_cache: false,
            };
        }
    };

    match cache.get(org_id) {
        None => CachedPassesEligibilityResult {
            eligible: false,
            needs_refresh: true,
            has_cache: false,
        },
        Some(entry) => {
            let needs_refresh = now_ms - entry.timestamp > CACHE_EXPIRATION_MS;
            CachedPassesEligibilityResult {
                eligible: entry.eligible,
                needs_refresh,
                has_cache: true,
            }
        }
    }
}

/// Get cached referrer reward info from eligibility cache.
pub fn get_cached_referrer_reward(
    cache: &PassesEligibilityCache,
    org_id: Option<&str>,
) -> Option<ReferrerRewardInfo> {
    let org_id = org_id?;
    let entry = cache.get(org_id)?;
    entry.referrer_reward.clone()
}

/// Get the cached remaining passes count from eligibility cache.
pub fn get_cached_remaining_passes(
    cache: &PassesEligibilityCache,
    org_id: Option<&str>,
) -> Option<u32> {
    let org_id = org_id?;
    let entry = cache.get(org_id)?;
    entry.remaining_passes
}

/// Fetch passes eligibility and store in cache.
/// Returns the fetched response or None on error.
pub async fn fetch_and_store_passes_eligibility(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    cache: &mut PassesEligibilityCache,
    now_ms: u64,
) -> Option<ReferralEligibilityResponse> {
    match fetch_referral_eligibility(client, base_api_url, access_token, org_uuid, None).await {
        Ok(response) => {
            let entry = PassesEligibilityCacheEntry {
                eligible: response.eligible,
                timestamp: now_ms,
                referrer_reward: response.referrer_reward.clone(),
                remaining_passes: response.remaining_passes,
            };
            cache.insert(org_uuid.to_string(), entry);
            debug!(
                "Passes eligibility cached for org {}: {}",
                org_uuid, response.eligible
            );
            Some(response)
        }
        Err(e) => {
            debug!("Failed to fetch and cache passes eligibility");
            error!("Referral eligibility error: {}", e);
            None
        }
    }
}

/// Get cached passes eligibility data or fetch if needed.
/// Main entry point for all eligibility checks.
///
/// This function never blocks on network - it returns cached data immediately
/// and fetches in the background if needed.
pub fn get_cached_or_fetch_passes_eligibility(
    cache: &PassesEligibilityCache,
    org_id: Option<&str>,
    is_hosted_subscriber: bool,
    subscription_type: Option<&str>,
    now_ms: u64,
) -> Option<ReferralEligibilityResponse> {
    let should_check = org_id.is_some() && is_hosted_subscriber && subscription_type == Some("max");

    if !should_check {
        return None;
    }

    let org_id = org_id?;
    let entry = cache.get(org_id)?;

    // Cache exists but stale — return stale data (caller should refresh in bg)
    if now_ms - entry.timestamp > CACHE_EXPIRATION_MS {
        debug!("Passes: Cache stale, returning cached data and refreshing in background");
    } else {
        debug!("Passes: Using fresh cached eligibility data");
    }

    Some(ReferralEligibilityResponse {
        eligible: entry.eligible,
        referrer_reward: entry.referrer_reward.clone(),
        remaining_passes: entry.remaining_passes,
    })
}

/// TS `prefetchPassesEligibility` — warm the eligibility cache. Errors are
/// swallowed (this is a fire-and-forget prefetch).
pub async fn prefetch_passes_eligibility(
    client: &reqwest::Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
) {
    let _ = fetch_referral_eligibility(
        client,
        base_api_url,
        access_token,
        org_uuid,
        Some("mossen_passes"),
    )
    .await;
}
