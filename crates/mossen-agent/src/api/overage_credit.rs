//! # Overage Credit Grant API
//!
//! 翻译自 `services/api/overageCreditGrant.ts` (138行)
//! 超额信用额度查询与缓存。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::error;

const CACHE_TTL_MS: u64 = 60 * 60 * 1000; // 1 hour

/// Overage credit grant info from the backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverageCreditGrantInfo {
    pub available: bool,
    pub eligible: bool,
    pub granted: bool,
    pub amount_minor_units: Option<u64>,
    pub currency: Option<String>,
}

/// Cached grant entry with timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedGrantEntry {
    pub info: OverageCreditGrantInfo,
    pub timestamp: u64,
}

/// Overage credit grant cache (org_id -> entry).
pub type OverageCreditGrantCache = HashMap<String, CachedGrantEntry>;

/// Fetch the current user's overage credit grant eligibility from the backend.
async fn fetch_overage_credit_grant(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Option<OverageCreditGrantInfo> {
    let url = format!(
        "{}/api/oauth/organizations/{}/overage_credit_grant",
        base_api_url, org_uuid
    );

    match client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
    {
        Ok(resp) => match resp.json::<OverageCreditGrantInfo>().await {
            Ok(info) => Some(info),
            Err(e) => {
                error!("Failed to parse overage credit grant response: {}", e);
                None
            }
        },
        Err(e) => {
            error!("Failed to fetch overage credit grant: {}", e);
            None
        }
    }
}

/// Get cached grant info. Returns None if no cache or cache is stale.
pub fn get_cached_overage_credit_grant(
    cache: &OverageCreditGrantCache,
    org_id: Option<&str>,
    now_ms: u64,
) -> Option<OverageCreditGrantInfo> {
    let org_id = org_id?;
    let cached = cache.get(org_id)?;
    if now_ms - cached.timestamp > CACHE_TTL_MS {
        return None;
    }
    Some(cached.info.clone())
}

/// Drop the current org's cached entry so the next read refetches.
pub fn invalidate_overage_credit_grant_cache(
    cache: &mut OverageCreditGrantCache,
    org_id: Option<&str>,
) {
    if let Some(org_id) = org_id {
        cache.remove(org_id);
    }
}

/// Fetch and cache grant info.
/// Returns the updated cache entry if successful.
pub async fn refresh_overage_credit_grant_cache(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    cache: &mut OverageCreditGrantCache,
    is_essential_traffic_only: bool,
    now_ms: u64,
) -> Option<OverageCreditGrantInfo> {
    if is_essential_traffic_only {
        return None;
    }

    let info = fetch_overage_credit_grant(client, base_api_url, access_token, org_uuid).await?;

    // Skip rewriting if grant data is unchanged and cache is still fresh
    if let Some(prev_cached) = cache.get(org_uuid) {
        let existing = &prev_cached.info;
        let data_unchanged = existing.available == info.available
            && existing.eligible == info.eligible
            && existing.granted == info.granted
            && existing.amount_minor_units == info.amount_minor_units
            && existing.currency == info.currency;

        if data_unchanged && now_ms - prev_cached.timestamp <= CACHE_TTL_MS {
            return Some(info);
        }

        // Use existing info if unchanged, just refresh timestamp
        let final_info = if data_unchanged {
            existing.clone()
        } else {
            info.clone()
        };

        cache.insert(
            org_uuid.to_string(),
            CachedGrantEntry {
                info: final_info,
                timestamp: now_ms,
            },
        );
    } else {
        cache.insert(
            org_uuid.to_string(),
            CachedGrantEntry {
                info: info.clone(),
                timestamp: now_ms,
            },
        );
    }

    Some(info)
}

/// Format the grant amount for display.
/// Returns None if amount isn't available or currency we don't know how to format.
pub fn format_grant_amount(info: &OverageCreditGrantInfo) -> Option<String> {
    let amount_minor_units = info.amount_minor_units?;
    let currency = info.currency.as_ref()?;

    if currency.to_uppercase() == "USD" {
        let dollars = amount_minor_units as f64 / 100.0;
        if dollars == dollars.floor() {
            Some(format!("${}", dollars as u64))
        } else {
            Some(format!("${:.2}", dollars))
        }
    } else {
        None
    }
}
