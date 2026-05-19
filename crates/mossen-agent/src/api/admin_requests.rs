//! # Admin Requests API
//!
//! 翻译自 `services/api/adminRequests.ts` (120行)
//! 管理员请求 CRUD（限制增加、座位升级）。

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Admin request types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminRequestType {
    LimitIncrease,
    SeatUpgrade,
}

impl AdminRequestType {
    pub fn as_str(&self) -> &str {
        match self {
            AdminRequestType::LimitIncrease => "limit_increase",
            AdminRequestType::SeatUpgrade => "seat_upgrade",
        }
    }
}

/// Admin request status values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminRequestStatus {
    Pending,
    Approved,
    Dismissed,
}

impl AdminRequestStatus {
    pub fn as_str(&self) -> &str {
        match self {
            AdminRequestStatus::Pending => "pending",
            AdminRequestStatus::Approved => "approved",
            AdminRequestStatus::Dismissed => "dismissed",
        }
    }
}

/// Seat upgrade details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRequestSeatUpgradeDetails {
    pub message: Option<String>,
    pub current_seat_tier: Option<String>,
}

/// Parameters for creating an admin request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRequestCreateParams {
    pub request_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<AdminRequestSeatUpgradeDetails>,
}

/// An admin request record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRequest {
    pub uuid: String,
    pub status: String,
    pub requester_uuid: Option<String>,
    pub created_at: String,
    pub request_type: String,
    pub details: Option<AdminRequestSeatUpgradeDetails>,
}

/// Eligibility check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRequestEligibilityResponse {
    pub request_type: String,
    pub is_allowed: bool,
}

/// Create an admin request (limit increase or seat upgrade).
///
/// For Team/Enterprise users who don't have billing/admin permissions,
/// this creates a request that their admin can act on.
///
/// If a pending request of the same type already exists for this user,
/// returns the existing request instead of creating a new one.
pub async fn create_admin_request(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    params: &AdminRequestCreateParams,
) -> Result<AdminRequest, anyhow::Error> {
    let url = format!(
        "{}/api/oauth/organizations/{}/admin_requests",
        base_api_url, org_uuid
    );

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .json(params)
        .send()
        .await?;

    let data: AdminRequest = response.error_for_status()?.json().await?;
    Ok(data)
}

/// Get pending admin request of a specific type for the current user.
/// Returns the pending requests if any exist, otherwise None.
pub async fn get_my_admin_requests(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    request_type: &AdminRequestType,
    statuses: &[AdminRequestStatus],
) -> Result<Option<Vec<AdminRequest>>, anyhow::Error> {
    let mut url = format!(
        "{}/api/oauth/organizations/{}/admin_requests/me?request_type={}",
        base_api_url, org_uuid, request_type.as_str()
    );
    for status in statuses {
        url.push_str(&format!("&statuses={}", status.as_str()));
    }

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .send()
        .await?;

    let data: Option<Vec<AdminRequest>> = response.error_for_status()?.json().await?;
    Ok(data)
}

/// Check if a specific admin request type is allowed for this org.
pub async fn check_admin_request_eligibility(
    client: &Client,
    base_api_url: &str,
    access_token: &str,
    org_uuid: &str,
    request_type: &AdminRequestType,
) -> Result<AdminRequestEligibilityResponse, anyhow::Error> {
    let url = format!(
        "{}/api/oauth/organizations/{}/admin_requests/eligibility?request_type={}",
        base_api_url, org_uuid, request_type.as_str()
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-organization-uuid", org_uuid)
        .send()
        .await?;

    let data: AdminRequestEligibilityResponse = response.error_for_status()?.json().await?;
    Ok(data)
}
