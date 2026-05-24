//! Sink killswitch — per-sink analytics disable via remote config.
//!
//! Translates: services/analytics/sinkKillswitch.ts

use std::collections::HashMap;

/// Analytics sink names that can be individually disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkName {
    Datadog,
    FirstParty,
}

impl SinkName {
    pub fn as_str(&self) -> &'static str {
        match self {
            SinkName::Datadog => "datadog",
            SinkName::FirstParty => "firstParty",
        }
    }
}

/// Mangled config name for the per-sink killswitch.
const SINK_KILLSWITCH_CONFIG_NAME: &str = "mossen_frond_boric";

/// Trait for accessing dynamic config (GrowthBook).
pub trait SinkKillswitchContext: Send + Sync {
    /// Get dynamic config value (cached, may be stale).
    fn get_dynamic_config_cached(&self, config_name: &str) -> Option<HashMap<String, bool>>;
}

/// Check whether a specific analytics sink is killed (disabled) via remote config.
///
/// GrowthBook JSON config that disables individual analytics sinks.
/// Shape: `{ "datadog": true, "firstParty": true }`
/// A value of `true` for a key stops all dispatch to that sink.
/// Default `{}` (nothing killed). Fail-open: missing/malformed config = sink stays on.
///
/// NOTE: Must NOT be called from inside is_1p_event_logging_enabled() —
/// growthbook's is_growthbook_enabled() calls that, so a lookup here would recurse.
/// Call at per-event dispatch sites instead.
pub fn is_sink_killed(ctx: &dyn SinkKillswitchContext, sink: SinkName) -> bool {
    let config = ctx.get_dynamic_config_cached(SINK_KILLSWITCH_CONFIG_NAME);
    match config {
        Some(map) => map.get(sink.as_str()).copied() == Some(true),
        None => false,
    }
}
