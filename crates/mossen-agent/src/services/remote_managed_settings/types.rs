//! Remote managed settings types.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A remote managed settings response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSettingsResponse {
    pub settings: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
}

/// Managed setting with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedSetting {
    pub key: String,
    pub value: serde_json::Value,
    pub source: SettingSource,
    pub enforced: bool,
}

/// Source of a managed setting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettingSource {
    Organization,
    Workspace,
    Default,
}

// === TS-parity exports ===

/// TS `export type RemoteManagedSettingsResponse = { uuid, checksum, settings }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteManagedSettingsResponse {
    pub uuid: String,
    pub checksum: String,
    pub settings: HashMap<String, serde_json::Value>,
}

/// TS `export type RemoteManagedSettingsFetchResult`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteManagedSettingsFetchResult {
    pub success: bool,
    /// `None` here represents 304 Not Modified (cache is valid). The outer
    /// `Option` distinguishes "not present" from "present-but-null".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Option<HashMap<String, serde_json::Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When `true`, do not retry on failure (e.g. auth errors).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_retry: bool,
}

/// TS exports a Zod-built schema. We mirror this as a lazily-compiled JSON
/// validator built once via `once_cell`.
pub static RemoteManagedSettingsResponseSchema: Lazy<RemoteManagedSettingsSchemaValidator> =
    Lazy::new(RemoteManagedSettingsSchemaValidator::new);

/// Validator for `RemoteManagedSettingsResponse` payloads. Validates required
/// `uuid: string`, `checksum: string`, `settings: object`.
pub struct RemoteManagedSettingsSchemaValidator;

impl RemoteManagedSettingsSchemaValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validate a JSON value against the schema. Returns the parsed struct or
    /// a human-readable error message.
    pub fn parse(
        &self,
        value: &serde_json::Value,
    ) -> Result<RemoteManagedSettingsResponse, String> {
        let obj = value.as_object().ok_or("expected object")?;
        let uuid = obj
            .get("uuid")
            .and_then(|v| v.as_str())
            .ok_or("missing uuid")?
            .to_string();
        let checksum = obj
            .get("checksum")
            .and_then(|v| v.as_str())
            .ok_or("missing checksum")?
            .to_string();
        let settings_value = obj.get("settings").ok_or("missing settings")?;
        let settings_obj = settings_value
            .as_object()
            .ok_or("settings must be object")?;
        let settings = settings_obj
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(RemoteManagedSettingsResponse {
            uuid,
            checksum,
            settings,
        })
    }

    /// `safeParse` equivalent — returns `Ok(value)` or `Err(message)`.
    pub fn safe_parse(
        &self,
        value: &serde_json::Value,
    ) -> Result<RemoteManagedSettingsResponse, String> {
        self.parse(value)
    }
}

impl Default for RemoteManagedSettingsSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}
