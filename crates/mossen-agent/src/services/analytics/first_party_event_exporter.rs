//! First-party event exporter — exports events to the first-party analytics pipeline.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use super::config::is_analytics_disabled;
use super::metadata::EventMetadata;

/// 全局已注册的 1P 事件 exporter。`log_event_to_1p` 在没有 exporter 时退化到
/// tracing；agent 在 bootstrap 中调用 [`set_global_exporter`] 绑定真正的实例。
static GLOBAL_EXPORTER: OnceLock<RwLock<Option<Arc<FirstPartyEventExporter>>>> =
    OnceLock::new();

fn global_cell() -> &'static RwLock<Option<Arc<FirstPartyEventExporter>>> {
    GLOBAL_EXPORTER.get_or_init(|| RwLock::new(None))
}

/// 注册全局 1P exporter。
pub fn set_global_exporter(exporter: Arc<FirstPartyEventExporter>) {
    if let Ok(mut g) = global_cell().write() {
        *g = Some(exporter);
    }
}

/// 获取全局 1P exporter（未注册时返回 None）。
pub fn get_global_exporter() -> Option<Arc<FirstPartyEventExporter>> {
    global_cell().read().ok().and_then(|g| g.clone())
}

/// 清除全局 1P exporter（用于测试）。
pub fn clear_global_exporter() {
    if let Ok(mut g) = global_cell().write() {
        *g = None;
    }
}

/// Batch configuration for the exporter.
const BATCH_SIZE: usize = 50;
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// First-party event for the analytics pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstPartyEvent {
    pub event_name: String,
    pub metadata: EventMetadata,
    pub timestamp_ms: u64,
    pub session_id: String,
    pub user_id: String,
    pub device_id: String,
}

/// Exporter that batches and sends events to the 1P pipeline.
pub struct FirstPartyEventExporter {
    client: Client,
    endpoint: String,
    batch: Mutex<Vec<FirstPartyEvent>>,
    last_flush: Mutex<Instant>,
    enabled: bool,
}

impl FirstPartyEventExporter {
    /// Create a new exporter with the given endpoint URL.
    pub fn new(endpoint: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            batch: Mutex::new(Vec::with_capacity(BATCH_SIZE)),
            last_flush: Mutex::new(Instant::now()),
            enabled: !is_analytics_disabled(),
        }
    }

    /// Add an event to the batch.
    pub fn add_event(&self, event: FirstPartyEvent) {
        if !self.enabled {
            return;
        }
        let mut batch = self.batch.lock().unwrap();
        batch.push(event);
        if batch.len() >= BATCH_SIZE {
            let events = std::mem::take(&mut *batch);
            drop(batch);
            self.spawn_flush(events);
        }
    }

    /// Check if it's time for a periodic flush.
    pub fn maybe_periodic_flush(&self) {
        let last_flush = self.last_flush.lock().unwrap();
        if last_flush.elapsed() < FLUSH_INTERVAL {
            return;
        }
        drop(last_flush);
        let events = {
            let mut batch = self.batch.lock().unwrap();
            std::mem::take(&mut *batch)
        };
        if !events.is_empty() {
            self.spawn_flush(events);
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
        self.send_batch(&events).await;
    }

    fn spawn_flush(&self, events: Vec<FirstPartyEvent>) {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        tokio::spawn(async move {
            let _ = send_batch_impl(&client, &endpoint, &events).await;
        });
        let mut last_flush = self.last_flush.lock().unwrap();
        *last_flush = Instant::now();
    }

    async fn send_batch(&self, events: &[FirstPartyEvent]) {
        let _ = send_batch_impl(&self.client, &self.endpoint, events).await;
        let mut last_flush = self.last_flush.lock().unwrap();
        *last_flush = Instant::now();
    }
}

async fn send_batch_impl(
    client: &Client,
    endpoint: &str,
    events: &[FirstPartyEvent],
) -> anyhow::Result<()> {
    let body = serde_json::json!({ "events": events });
    for attempt in 0..MAX_RETRY_ATTEMPTS {
        let resp = client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) if r.status().is_server_error() && attempt < MAX_RETRY_ATTEMPTS - 1 => {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                continue;
            }
            Ok(r) => {
                debug!("1P event export failed: status {}", r.status());
                return Err(anyhow::anyhow!("1P export failed: {}", r.status()));
            }
            Err(e) if attempt < MAX_RETRY_ATTEMPTS - 1 => {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// TS `type FirstPartyEventLoggingEvent` — the wire shape an exporter writes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FirstPartyEventLoggingEvent {
    pub event: String,
    pub metadata: serde_json::Value,
    pub timestamp: i64,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
}
