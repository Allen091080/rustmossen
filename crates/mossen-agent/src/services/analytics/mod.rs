//! Analytics service — event logging, feature flags, and telemetry.
//!
//! Translates: services/analytics/ (13 files)

pub mod config;
pub mod datadog;
pub mod event_queue;
pub mod event_storage;
pub mod first_party_event_exporter;
pub mod first_party_event_logger;
pub mod growthbook;
pub mod metadata;
pub mod plugin_metadata;
pub mod retry_scheduler;
pub mod sink;
pub mod sink_killswitch;

// === PII-tagged metadata marker types ===
//
// TS exports these as `type … = never` markers. They carry no data — their job
// is to flag a string field as "verified not code/filepaths" or "PII-tagged"
// for downstream review tooling. We mirror the same intent in Rust with two
// uninhabited marker enums.

/// Marker for analytics metadata that has been verified to NOT contain code
/// or filepaths. Mirrors TS `AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS`.
#[allow(non_camel_case_types)]
pub enum AnalyticsMetadata_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {}

/// Marker for analytics metadata that has been PII-tagged (i.e. reviewed and
/// allowed to contain user-identifying data). Mirrors TS
/// `AnalyticsMetadata_I_VERIFIED_THIS_IS_PII_TAGGED`.
#[allow(non_camel_case_types)]
pub enum AnalyticsMetadata_I_VERIFIED_THIS_IS_PII_TAGGED {}
