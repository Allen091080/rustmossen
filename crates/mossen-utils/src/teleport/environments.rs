//! Teleport environments — translated from `utils/teleport/environments.ts`.
//!
//! Fetches the list of available remote environments and creates new
//! Mossen-cloud environments for users who don't have any yet.

use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::api::get_oauth_headers;

const CCR_BYOC_BETA: &str = "ccr-byoc-2025-07-29";

/// Type of remote environment provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    /// Mossen-hosted (cloud) environment.
    MossenCloud,
    /// Bring-your-own-cloud environment.
    Byoc,
    /// Bridge environment (local adapter).
    Bridge,
}

/// Lifecycle state of an environment. The TS schema currently only allows
/// `"active"`, but the enum is open for forward-compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentState {
    Active,
}

/// An environment record returned by the Environment API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentResource {
    pub kind: EnvironmentKind,
    pub environment_id: String,
    pub name: String,
    pub created_at: String,
    pub state: EnvironmentState,
}

/// Paginated response wrapper returned by `GET /v1/environment_providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentListResponse {
    pub environments: Vec<EnvironmentResource>,
    pub has_more: bool,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
}

/// Fetch the list of available environments.
///
/// Mirrors TS `fetchEnvironments`. The caller is responsible for providing the
/// OAuth access token and organization UUID — in Rust we don't have ambient
/// auth state, so these are explicit parameters.
pub async fn fetch_environments(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    org_uuid: &str,
) -> Result<Vec<EnvironmentResource>> {
    let url = format!("{}/v1/environment_providers", base_url.trim_end_matches('/'));
    let mut request = client.get(&url).timeout(Duration::from_secs(15));
    for (key, value) in get_oauth_headers(access_token) {
        request = request.header(key.as_str(), value.as_str());
    }
    request = request.header("x-organization-uuid", org_uuid);

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        bail!("Failed to fetch environments: {} {}", status, text);
    }

    let payload: EnvironmentListResponse = response.json().await?;
    Ok(payload.environments)
}

/// Create a default Mossen cloud environment for users who have none.
///
/// Mirrors TS `createDefaultCloudEnvironment`.
pub async fn create_default_cloud_environment(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    org_uuid: &str,
    name: &str,
) -> Result<EnvironmentResource> {
    let url = format!(
        "{}/v1/environment_providers/cloud/create",
        base_url.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "name": name,
        "kind": "mossen_cloud",
        "description": "",
        "config": {
            "environment_type": "mossen",
            "cwd": "/home/user",
            "init_script": null,
            "environment": {},
            "languages": [
                { "name": "python", "version": "3.11" },
                { "name": "node", "version": "20" }
            ],
            "network_config": {
                "allowed_hosts": [],
                "allow_default_hosts": true
            }
        }
    });

    let mut request = client
        .post(&url)
        .timeout(Duration::from_secs(15))
        .json(&body);

    for (key, value) in get_oauth_headers(access_token) {
        request = request.header(key.as_str(), value.as_str());
    }
    request = request
        .header("mossen-beta", CCR_BYOC_BETA)
        .header("x-organization-uuid", org_uuid);

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        bail!("Failed to create cloud environment: {} {}", status, text);
    }

    let env: EnvironmentResource = response.json().await?;
    Ok(env)
}
