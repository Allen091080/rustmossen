//! # Bootstrap API
//!
//! 翻译自 `services/api/bootstrap.ts` (148行)
//! 获取引导数据（客户端配置、额外模型选项）。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error};

/// A model option from the bootstrap response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdditionalModelOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

/// Bootstrap response from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResponse {
    pub client_data: Option<Value>,
    pub additional_model_options: Option<Vec<BootstrapModelOption>>,
}

/// Raw model option from API before transform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapModelOption {
    pub model: String,
    pub name: String,
    pub description: String,
}

impl BootstrapModelOption {
    /// Transform to the display format.
    pub fn to_additional_model_option(&self) -> AdditionalModelOption {
        AdditionalModelOption {
            value: self.model.clone(),
            label: self.name.clone(),
            description: self.description.clone(),
        }
    }
}

/// Configuration for bootstrap fetch.
pub struct BootstrapConfig {
    pub is_essential_traffic_only: bool,
    pub is_custom_backend: bool,
    pub api_provider: String,
    pub has_usable_oauth: bool,
    pub api_key: Option<String>,
    pub base_api_url: String,
    pub oauth_beta_header: String,
    pub user_agent: String,
}

/// Fetch bootstrap data from the API.
/// Returns None if bootstrap is not needed/available.
async fn fetch_bootstrap_api(
    client: &Client,
    config: &BootstrapConfig,
    get_access_token: impl Fn() -> Option<String>,
) -> Result<Option<BootstrapResponse>, anyhow::Error> {
    if config.is_essential_traffic_only {
        debug!("[Bootstrap] Skipped: Nonessential traffic disabled");
        return Ok(None);
    }

    if config.is_custom_backend {
        debug!("[Bootstrap] Skipped: custom backend mode");
        return Ok(None);
    }

    if config.api_provider != "firstParty" {
        debug!("[Bootstrap] Skipped: 3P provider");
        return Ok(None);
    }

    if !config.has_usable_oauth && config.api_key.is_none() {
        debug!("[Bootstrap] Skipped: no usable OAuth or API key");
        return Ok(None);
    }

    let endpoint = format!("{}/api/mossen/bootstrap", config.base_api_url);

    // Build auth headers
    let token = get_access_token();
    let mut auth_headers: Vec<(String, String)> = Vec::new();

    if let Some(ref token) = token {
        if config.has_usable_oauth {
            auth_headers.push(("Authorization".into(), format!("Bearer {}", token)));
            auth_headers.push(("mossen-beta".into(), config.oauth_beta_header.clone()));
        }
    }

    if auth_headers.is_empty() {
        if let Some(ref api_key) = config.api_key {
            auth_headers.push(("x-api-key".into(), api_key.clone()));
        } else {
            debug!("[Bootstrap] No auth available on retry, aborting");
            return Ok(None);
        }
    }

    debug!("[Bootstrap] Fetching");

    let mut request = client
        .get(&endpoint)
        .header("Content-Type", "application/json")
        .header("User-Agent", &config.user_agent)
        .timeout(std::time::Duration::from_secs(5));

    for (key, value) in &auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;
    let status = response.status();

    if !status.is_success() {
        debug!("[Bootstrap] Fetch failed: {}", status.as_u16());
        return Err(anyhow::anyhow!(
            "Bootstrap fetch failed with status {}",
            status
        ));
    }

    let data: BootstrapResponse = response.json().await.map_err(|e| {
        debug!("[Bootstrap] Response failed validation: {}", e);
        anyhow::anyhow!("Bootstrap response validation failed: {}", e)
    })?;

    debug!("[Bootstrap] Fetch ok");
    Ok(Some(data))
}

/// Fetch bootstrap data from the API and return the parsed result.
/// The caller is responsible for persisting to disk cache.
pub async fn fetch_bootstrap_data(
    client: &Client,
    config: &BootstrapConfig,
    get_access_token: impl Fn() -> Option<String>,
    current_client_data: Option<&Value>,
    current_model_options: &[AdditionalModelOption],
) -> Result<Option<(Option<Value>, Vec<AdditionalModelOption>)>, ()> {
    let response = match fetch_bootstrap_api(client, config, get_access_token).await {
        Ok(Some(resp)) => resp,
        Ok(None) => return Ok(None),
        Err(e) => {
            error!("Bootstrap fetch error: {}", e);
            return Err(());
        }
    };

    let client_data = response.client_data;
    let additional_model_options: Vec<AdditionalModelOption> = response
        .additional_model_options
        .unwrap_or_default()
        .iter()
        .map(|m| m.to_additional_model_option())
        .collect();

    // Only return data if it actually changed
    let client_data_unchanged = match (&client_data, current_client_data) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    };

    let model_options_unchanged = additional_model_options == current_model_options;

    if client_data_unchanged && model_options_unchanged {
        debug!("[Bootstrap] Cache unchanged, skipping write");
        return Ok(None);
    }

    debug!("[Bootstrap] Cache updated, persisting to disk");
    Ok(Some((client_data, additional_model_options)))
}
