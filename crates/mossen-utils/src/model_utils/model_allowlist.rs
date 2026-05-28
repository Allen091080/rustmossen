//! Available-models allowlist resolution.
//!
//! Direct translation of `utils/model/modelAllowlist.ts`.

use crate::settings::get_session_settings_cache;

use super::aliases::{is_model_alias, is_model_family_alias};
use super::external_provider_ids::external_provider_model_prefix;
use super::model_strings::resolve_overridden_model;

/// Resolve a model string the same way `parseUserSpecifiedModel` does, but
/// without a runtime dependency on `model::parse_user_specified_model` (which
/// would create an import cycle). This delegates to the public function via a
/// late binding through `super::model`.
fn resolve_user_specified_model(model: &str) -> String {
    super::model::parse_user_specified_model(model)
}

/// Check if a model belongs to a given family by checking if its name (or
/// resolved name) contains the family identifier.
fn model_belongs_to_family(model: &str, family: &str) -> bool {
    if model.contains(family) {
        return true;
    }
    if is_model_alias(model) {
        let resolved = resolve_user_specified_model(model).to_lowercase();
        return resolved.contains(family);
    }
    false
}

/// Check if a model name starts with a prefix at a segment boundary.
fn prefix_matches_model(model_name: &str, prefix: &str) -> bool {
    if !model_name.starts_with(prefix) {
        return false;
    }
    model_name.len() == prefix.len()
        || model_name.as_bytes().get(prefix.len()).copied() == Some(b'-')
}

/// Check if a model matches a version-prefix entry in the allowlist.
fn model_matches_version_prefix(model: &str, entry: &str) -> bool {
    let resolved_model = if is_model_alias(model) {
        resolve_user_specified_model(model).to_lowercase()
    } else {
        model.to_string()
    };

    if prefix_matches_model(&resolved_model, entry) {
        return true;
    }
    if !entry.starts_with("mossen-")
        && prefix_matches_model(&resolved_model, &format!("mossen-{}", entry))
    {
        return true;
    }
    let ext_prefix = external_provider_model_prefix();
    let ext_dashed = format!("{}-", ext_prefix);
    if !entry.starts_with(&ext_dashed)
        && prefix_matches_model(&resolved_model, &format!("{}-{}", ext_prefix, entry))
    {
        return true;
    }
    false
}

/// Check if a family alias is narrowed by more specific entries in the
/// allowlist.
fn family_has_specific_entries(family: &str, allowlist: &[String]) -> bool {
    for entry in allowlist {
        if is_model_family_alias(entry) {
            continue;
        }
        let idx = match entry.find(family) {
            Some(i) => i,
            None => continue,
        };
        let after_family = idx + family.len();
        if after_family == entry.len()
            || entry.as_bytes().get(after_family).copied() == Some(b'-')
        {
            return true;
        }
    }
    false
}

/// `isModelAllowed` — check if a model is allowed by the availableModels
/// allowlist in settings.
pub fn is_model_allowed(model: &str) -> bool {
    let settings = get_session_settings_cache();
    let available_models = match settings.and_then(|s| s.settings.available_models.clone()) {
        Some(am) => am,
        None => return true,
    };
    if available_models.is_empty() {
        return false;
    }

    let resolved_model = resolve_overridden_model(model);
    let normalized_model = resolved_model.trim().to_lowercase();
    let normalized_allowlist: Vec<String> = available_models
        .iter()
        .map(|m| m.trim().to_lowercase())
        .collect();

    // Direct match.
    if normalized_allowlist.iter().any(|m| m == &normalized_model) {
        if !is_model_family_alias(&normalized_model)
            || !family_has_specific_entries(&normalized_model, &normalized_allowlist)
        {
            return true;
        }
    }

    // Family-level aliases (wildcards).
    for entry in &normalized_allowlist {
        if is_model_family_alias(entry)
            && !family_has_specific_entries(entry, &normalized_allowlist)
            && model_belongs_to_family(&normalized_model, entry)
        {
            return true;
        }
    }

    // Bidirectional alias resolution for non-family entries.
    if is_model_alias(&normalized_model) {
        let resolved = resolve_user_specified_model(&normalized_model).to_lowercase();
        if normalized_allowlist.iter().any(|m| m == &resolved) {
            return true;
        }
    }

    for entry in &normalized_allowlist {
        if !is_model_family_alias(entry) && is_model_alias(entry) {
            let resolved = resolve_user_specified_model(entry).to_lowercase();
            if resolved == normalized_model {
                return true;
            }
        }
    }

    // Version-prefix matching at a segment boundary.
    for entry in &normalized_allowlist {
        if !is_model_family_alias(entry)
            && !is_model_alias(entry)
            && model_matches_version_prefix(&normalized_model, entry)
        {
            return true;
        }
    }

    false
}
