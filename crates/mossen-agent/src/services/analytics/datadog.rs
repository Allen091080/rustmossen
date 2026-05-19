//! Datadog telemetry client — sends events to Datadog for monitoring.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error};

use super::config::is_analytics_disabled;

/// Datadog event payload.
#[derive(Debug, Clone, Serialize)]
struct DatadogEvent {
    pub metric: String,
    pub points: Vec<DatadogPoint>,
    pub tags: Vec<String>,
    #[serde(rename = "type")]
    pub metric_type: String,
}

#[derive(Debug, Clone, Serialize)]
struct DatadogPoint {
    pub timestamp: u64,
    pub value: f64,
}

/// Datadog client for sending metrics.
pub struct DatadogClient {
    client: Client,
    api_key: Option<String>,
    base_url: String,
    enabled: bool,
    batch: Mutex<Vec<DatadogEvent>>,
    last_flush: Mutex<Instant>,
    flush_interval: Duration,
}

impl DatadogClient {
    /// Create a new Datadog client.
    pub fn new(api_key: Option<String>) -> Self {
        let enabled = api_key.is_some() && !is_analytics_disabled();
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.datadoghq.com".to_string(),
            enabled,
            batch: Mutex::new(Vec::new()),
            last_flush: Mutex::new(Instant::now()),
            flush_interval: Duration::from_secs(10),
        }
    }

    /// Record a count metric.
    pub fn increment(&self, metric: &str, tags: &[&str]) {
        if !self.enabled {
            return;
        }
        let event = DatadogEvent {
            metric: metric.to_string(),
            points: vec![DatadogPoint {
                timestamp: chrono::Utc::now().timestamp() as u64,
                value: 1.0,
            }],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            metric_type: "count".to_string(),
        };
        let mut batch = self.batch.lock().unwrap();
        batch.push(event);
        self.maybe_flush(&mut batch);
    }

    /// Record a gauge metric.
    pub fn gauge(&self, metric: &str, value: f64, tags: &[&str]) {
        if !self.enabled {
            return;
        }
        let event = DatadogEvent {
            metric: metric.to_string(),
            points: vec![DatadogPoint {
                timestamp: chrono::Utc::now().timestamp() as u64,
                value,
            }],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            metric_type: "gauge".to_string(),
        };
        let mut batch = self.batch.lock().unwrap();
        batch.push(event);
        self.maybe_flush(&mut batch);
    }

    /// Record a distribution metric.
    pub fn distribution(&self, metric: &str, value: f64, tags: &[&str]) {
        if !self.enabled {
            return;
        }
        let event = DatadogEvent {
            metric: metric.to_string(),
            points: vec![DatadogPoint {
                timestamp: chrono::Utc::now().timestamp() as u64,
                value,
            }],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            metric_type: "distribution".to_string(),
        };
        let mut batch = self.batch.lock().unwrap();
        batch.push(event);
        self.maybe_flush(&mut batch);
    }

    fn maybe_flush(&self, batch: &mut Vec<DatadogEvent>) {
        let mut last_flush = self.last_flush.lock().unwrap();
        if last_flush.elapsed() >= self.flush_interval || batch.len() >= 100 {
            let events = std::mem::take(batch);
            *last_flush = Instant::now();
            drop(last_flush);
            // Fire and forget flush
            let client = self.client.clone();
            let api_key = self.api_key.clone();
            let base_url = self.base_url.clone();
            tokio::spawn(async move {
                if let Err(e) = flush_batch(&client, &base_url, api_key.as_deref(), &events).await {
                    debug!("Datadog flush error: {}", e);
                }
            });
        }
    }

    /// Force flush all pending events.
    pub async fn flush(&self) {
        let events = {
            let mut batch = self.batch.lock().unwrap();
            std::mem::take(&mut *batch)
        };
        if events.is_empty() {
            return;
        }
        if let Err(e) =
            flush_batch(&self.client, &self.base_url, self.api_key.as_deref(), &events).await
        {
            debug!("Datadog flush error: {}", e);
        }
    }
}

async fn flush_batch(
    client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    events: &[DatadogEvent],
) -> anyhow::Result<()> {
    let api_key = api_key.ok_or_else(|| anyhow::anyhow!("No Datadog API key"))?;
    let url = format!("{}/api/v2/series", base_url);
    let body = serde_json::json!({ "series": events });

    let resp = client
        .post(&url)
        .header("DD-API-KEY", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Datadog API error {}: {}", status, text));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/analytics/datadog.ts` exports.
// ---------------------------------------------------------------------------

/// `datadog.ts` `initializeDatadog`.
pub fn initialize_datadog() {
    debug!("initializeDatadog");
}

/// `datadog.ts` `shutdownDatadog`.
pub async fn shutdown_datadog() {
    debug!("shutdownDatadog");
}

/// `datadog.ts` `trackDatadogEvent`.
pub fn track_datadog_event(event_name: &str, payload: serde_json::Value) {
    let _ = (event_name, payload);
}
