//! First-party event logger — structured event logging with enrichment.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::metadata::EventMetadata;
use super::sink::LogEventMetadata;

/// Logger that enriches events with session/user metadata before export.
pub struct FirstPartyEventLogger {
    session_id: String,
    user_id: String,
    device_id: String,
    app_version: String,
    platform: String,
}

impl FirstPartyEventLogger {
    pub fn new(
        session_id: String,
        user_id: String,
        device_id: String,
        app_version: String,
        platform: String,
    ) -> Self {
        Self { session_id, user_id, device_id, app_version, platform }
    }

    /// Enrich event metadata with session context.
    pub fn enrich_metadata(&self, metadata: &LogEventMetadata) -> EventMetadata {
        let mut enriched = metadata.clone();
        enriched.insert("session_id".to_string(), Value::String(self.session_id.clone()));
        enriched.insert("user_id".to_string(), Value::String(self.user_id.clone()));
        enriched.insert("device_id".to_string(), Value::String(self.device_id.clone()));
        enriched.insert("app_version".to_string(), Value::String(self.app_version.clone()));
        enriched.insert("platform".to_string(), Value::String(self.platform.clone()));
        enriched.insert(
            "timestamp_ms".to_string(),
            Value::Number(serde_json::Number::from(chrono::Utc::now().timestamp_millis() as u64)),
        );
        enriched
    }

    /// Log an event with full enrichment.
    pub fn log_event(&self, event_name: &str, metadata: &LogEventMetadata) {
        let _enriched = self.enrich_metadata(metadata);
        // In production, would forward to FirstPartyEventExporter
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — services/analytics/firstPartyEventLogger.ts exports.
// ---------------------------------------------------------------------------

use std::sync::{Mutex, OnceLock};

/// `firstPartyEventLogger.ts` `EventSamplingConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventSamplingConfig {
    /// Per-event-name sampling rate in [0.0, 1.0]. Missing keys default to 1.0.
    pub per_event: HashMap<String, f64>,
    /// Global default sampling rate when no entry exists.
    pub default_rate: f64,
}

fn sampling_config_cell() -> &'static Mutex<EventSamplingConfig> {
    static CFG: OnceLock<Mutex<EventSamplingConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        Mutex::new(EventSamplingConfig {
            per_event: HashMap::new(),
            default_rate: 1.0,
        })
    })
}

/// `firstPartyEventLogger.ts` `getEventSamplingConfig`.
pub fn get_event_sampling_config() -> EventSamplingConfig {
    sampling_config_cell().lock().unwrap().clone()
}

/// Replace the in-memory sampling config (test/install hook).
pub fn set_event_sampling_config(cfg: EventSamplingConfig) {
    *sampling_config_cell().lock().unwrap() = cfg;
}

/// `firstPartyEventLogger.ts` `shouldSampleEvent` — returns:
/// - `Some(rate)` to record at the given rate
/// - `None` to drop the event
pub fn should_sample_event(event_name: &str) -> Option<f64> {
    let cfg = get_event_sampling_config();
    let rate = cfg.per_event.get(event_name).copied().unwrap_or(cfg.default_rate);
    if rate <= 0.0 {
        return None;
    }
    if rate >= 1.0 {
        return Some(1.0);
    }
    // Stateless uniform sample.
    let roll: f64 = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen()
    };
    if roll < rate {
        Some(rate)
    } else {
        None
    }
}

fn enabled_cell() -> &'static Mutex<bool> {
    static E: OnceLock<Mutex<bool>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(false))
}

/// `firstPartyEventLogger.ts` `is1PEventLoggingEnabled`.
pub fn is_1p_event_logging_enabled() -> bool {
    *enabled_cell().lock().unwrap()
}

/// `firstPartyEventLogger.ts` `initialize1PEventLogging`.
///
/// 同时把 sink 注册到 `mossen_utils::api`，让 utils 层的 `log_context_metrics`
/// 等函数能透过回调发到 1P pipeline（utils 不能 import agent，依赖架构上的
/// 单向）。
pub fn initialize_1p_event_logging() {
    *enabled_cell().lock().unwrap() = true;
    mossen_utils::api::set_analytics_sink(std::sync::Arc::new(
        |event_name: &str, payload: HashMap<String, Value>| {
            log_event_to_1p(event_name, payload);
        },
    ));
}

/// `firstPartyEventLogger.ts` `shutdown1PEventLogging`.
pub async fn shutdown_1p_event_logging() {
    *enabled_cell().lock().unwrap() = false;
}

/// `firstPartyEventLogger.ts` `reinitialize1PEventLoggingIfConfigChanged`.
pub async fn reinitialize_1p_event_logging_if_config_changed() {
    if !is_1p_event_logging_enabled() {
        initialize_1p_event_logging();
    }
}

/// `firstPartyEventLogger.ts` `logEventTo1P` — buffered emit.
///
/// 投递到已注册的全局 [`super::first_party_event_exporter::FirstPartyEventExporter`]，
/// 未注册时落到 tracing（保持可观测，但不阻塞 caller）。
pub fn log_event_to_1p(event_name: &str, payload: HashMap<String, Value>) {
    if !is_1p_event_logging_enabled() {
        return;
    }
    let metadata_value: Value = serde_json::to_value(&payload).unwrap_or(Value::Null);
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    if let Some(exporter) = super::first_party_event_exporter::get_global_exporter() {
        let event = super::first_party_event_exporter::FirstPartyEvent {
            event_name: event_name.to_string(),
            metadata: payload.clone(),
            timestamp_ms,
            session_id: std::env::var("MOSSEN_SESSION_ID").unwrap_or_default(),
            user_id: std::env::var("MOSSEN_USER_ID").unwrap_or_default(),
            device_id: std::env::var("MOSSEN_DEVICE_ID").unwrap_or_default(),
        };
        exporter.add_event(event);
    } else {
        tracing::debug!(
            target = "analytics.1p",
            event_name,
            metadata = %metadata_value,
            "1P event (no exporter registered)"
        );
    }
}

/// `firstPartyEventLogger.ts` `GrowthBookExperimentData`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrowthBookExperimentData {
    pub experiment_key: String,
    pub variation_id: String,
    pub user_id: String,
    pub additional: HashMap<String, Value>,
}

/// `firstPartyEventLogger.ts` `logGrowthBookExperimentTo1P`.
pub fn log_growth_book_experiment_to_1p(data: GrowthBookExperimentData) {
    if !is_1p_event_logging_enabled() {
        return;
    }
    let _ = data;
}
