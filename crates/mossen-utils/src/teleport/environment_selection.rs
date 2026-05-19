//! Environment selection logic — translated from
//! `utils/teleport/environmentSelection.ts`.
//!
//! Computes which environment a teleport request will target given the list
//! of available environments and the user's resolved settings (CLI override,
//! per-project, user-global, etc.).

use serde::{Deserialize, Serialize};

use super::environments::{EnvironmentKind, EnvironmentResource};

/// Identifies which settings layer supplied the chosen environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingSource {
    /// Command-line flag.
    Cli,
    /// Project-local settings (`.mossen/settings.json`).
    Project,
    /// User-global settings (`~/.mossen/settings.json`).
    User,
    /// Machine-wide / managed settings.
    Managed,
    /// Settings supplied by remote feature flags.
    FlagSettings,
}

impl SettingSource {
    /// Iterator over sources in descending priority order (highest first).
    /// Mirrors the TS `SETTING_SOURCES` ordering.
    pub fn priority_order() -> &'static [SettingSource] {
        &[
            SettingSource::Cli,
            SettingSource::Project,
            SettingSource::User,
            SettingSource::Managed,
            SettingSource::FlagSettings,
        ]
    }
}

/// Snapshot of an environment-selection decision.
///
/// Mirrors TS `EnvironmentSelectionInfo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSelectionInfo {
    pub available_environments: Vec<EnvironmentResource>,
    pub selected_environment: Option<EnvironmentResource>,
    pub selected_environment_source: Option<SettingSource>,
}

/// Resolves the environment that would be used given the provided settings
/// inputs. Mirrors TS `getEnvironmentSelectionInfo`, but the I/O (HTTP fetch,
/// settings loading) is lifted into the caller so we can keep this pure and
/// testable.
///
/// * `environments` — list returned by [`super::environments::fetch_environments`].
/// * `merged_default_environment_id` — the `remote.defaultEnvironmentId` value
///   from the fully-merged settings, if any.
/// * `per_source_defaults` — for each source (in any order), the
///   `remote.defaultEnvironmentId` value present at that level. Used to find
///   which source contributed the winning value.
pub fn select_environment(
    environments: Vec<EnvironmentResource>,
    merged_default_environment_id: Option<&str>,
    per_source_defaults: &[(SettingSource, Option<String>)],
) -> EnvironmentSelectionInfo {
    if environments.is_empty() {
        return EnvironmentSelectionInfo {
            available_environments: environments,
            selected_environment: None,
            selected_environment_source: None,
        };
    }

    // Default: first non-bridge environment, falling back to the first item.
    let mut selected_environment: EnvironmentResource = environments
        .iter()
        .find(|env| env.kind != EnvironmentKind::Bridge)
        .cloned()
        .unwrap_or_else(|| environments[0].clone());

    let mut selected_environment_source: Option<SettingSource> = None;

    if let Some(target_id) = merged_default_environment_id {
        if let Some(matching) = environments
            .iter()
            .find(|env| env.environment_id == target_id)
        {
            selected_environment = matching.clone();

            // Walk priority order (highest first) and pick the first source
            // that declared this exact ID. Skip `flagSettings`, mirroring TS.
            for source in SettingSource::priority_order() {
                if matches!(source, SettingSource::FlagSettings) {
                    continue;
                }
                let matched = per_source_defaults.iter().any(|(s, v)| {
                    s == source && v.as_deref() == Some(target_id)
                });
                if matched {
                    selected_environment_source = Some(*source);
                    break;
                }
            }
        }
    }

    EnvironmentSelectionInfo {
        available_environments: environments,
        selected_environment: Some(selected_environment),
        selected_environment_source,
    }
}

/// API parity alias for [`select_environment`]. The TS export name is
/// `getEnvironmentSelectionInfo`; the Rust port renames it for clarity
/// but keeps this wrapper available so generated bindings and consumers
/// that mirror the TS surface keep working.
pub fn get_environment_selection_info(
    environments: Vec<EnvironmentResource>,
    merged_default_environment_id: Option<&str>,
    per_source_defaults: &[(SettingSource, Option<String>)],
) -> EnvironmentSelectionInfo {
    select_environment(environments, merged_default_environment_id, per_source_defaults)
}
