//! Analytics event logging — public API for event logging.
//!
//! Events are queued until attach_analytics_sink() is called during app initialization.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// Metadata type for log events — no raw strings to avoid accidentally logging code/filepaths.
pub type LogEventMetadata = HashMap<String, Value>;

/// Queued event awaiting sink attachment.
#[derive(Debug, Clone)]
struct QueuedEvent {
    event_name: String,
    metadata: LogEventMetadata,
    is_async: bool,
}

/// Sink interface for the analytics backend.
pub trait AnalyticsSink: Send + Sync {
    fn log_event(&self, event_name: &str, metadata: &LogEventMetadata);
    fn log_event_async(
        &self,
        event_name: &str,
        metadata: &LogEventMetadata,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

/// Global analytics state.
static EVENT_QUEUE: Mutex<Vec<QueuedEvent>> = Mutex::new(Vec::new());
static SINK: Mutex<Option<Box<dyn AnalyticsSink>>> = Mutex::new(None);

/// Attach the analytics sink that will receive all events.
/// Queued events are drained. Idempotent.
pub fn attach_analytics_sink(new_sink: Box<dyn AnalyticsSink>) {
    let mut sink_slot = SINK.lock().unwrap();
    if sink_slot.is_some() {
        return;
    }

    let mut queue = EVENT_QUEUE.lock().unwrap();
    let queued_events: Vec<QueuedEvent> = queue.drain(..).collect();
    *sink_slot = Some(new_sink);

    // Drain queued events
    if let Some(sink) = sink_slot.as_ref() {
        for event in queued_events {
            sink.log_event(&event.event_name, &event.metadata);
        }
    }
}

/// Log an event to analytics backends (synchronous).
pub fn log_event(event_name: &str, metadata: LogEventMetadata) {
    let sink_slot = SINK.lock().unwrap();
    if let Some(sink) = sink_slot.as_ref() {
        sink.log_event(event_name, &metadata);
    } else {
        drop(sink_slot);
        let mut queue = EVENT_QUEUE.lock().unwrap();
        queue.push(QueuedEvent {
            event_name: event_name.to_string(),
            metadata,
            is_async: false,
        });
    }
}

/// Log an event to analytics backends (asynchronous).
pub async fn log_event_async(event_name: &str, metadata: LogEventMetadata) {
    let sink_slot = SINK.lock().unwrap();
    if let Some(sink) = sink_slot.as_ref() {
        sink.log_event(event_name, &metadata);
    } else {
        drop(sink_slot);
        let mut queue = EVENT_QUEUE.lock().unwrap();
        queue.push(QueuedEvent {
            event_name: event_name.to_string(),
            metadata,
            is_async: true,
        });
    }
}

/// Strip `_PROTO_*` keys from a payload destined for general-access storage.
pub fn strip_proto_fields(metadata: &mut LogEventMetadata) {
    metadata.retain(|k, _| !k.starts_with("_PROTO_"));
}

/// Reset analytics state for testing purposes only.
pub fn reset_for_testing() {
    let mut sink_slot = SINK.lock().unwrap();
    *sink_slot = None;
    let mut queue = EVENT_QUEUE.lock().unwrap();
    queue.clear();
}

/// TS `initializeAnalyticsGates` — wires the feature-gate evaluation for
/// analytics-on/off decisions. No-op in the Rust port: gates are evaluated
/// at call time against the live config.
pub fn initialize_analytics_gates() {
    // Intentional no-op: the gate resolution happens lazily on first event.
}

/// TS `initializeAnalyticsSink` — install the default Datadog / Statsig sink
/// chain. The default sink chain is installed via `attach_analytics_sink`
/// from the agent bootstrap; this entry point exists for TS-export parity and
/// is a safe re-entry point (idempotent).
pub fn initialize_analytics_sink() {
    // No-op for export-name parity. The actual sink chain is wired by the
    // agent bootstrap once the kill-switch context is known.
}
