//! Core model resolution & display helpers.
//!
//! Direct translation of `utils/model/model.ts`.

use std::sync::{Mutex, OnceLock};

use mossen_types::constants::figures::LIGHTNING_BOLT;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::auth::{
    get_subscription_type, is_hosted_subscriber, is_max_subscriber, is_pro_subscriber,
    is_team_premium_subscriber,
};
use crate::context::{has_1m_context, is_1m_context_disabled, model_supports_1m};
use crate::custom_backend::{
    custom_backend_capability_applies_to_model, get_custom_backend_max_input_tokens,
    get_custom_backend_model, is_custom_backend_enabled,
};
use crate::env::is_env_truthy;
use crate::model_cost::{format_model_pricing, get_max_46_cost_tier};
use crate::settings::get_session_settings_cache;
use crate::string_utils::capitalize;

use super::aliases::{is_model_alias, ModelAlias};
use super::internal_models::{get_internal_model_override_config, resolve_internal_model};
use super::external_provider_ids::{
    external_provider_model_prefix, external_provider_model_stem_from_mossen_id,
    external_provider_model_stem_pattern,
};
use super::model_allowlist::is_model_allowed;
use super::model_strings::{get_model_strings, resolve_overridden_model};
use super::mossen_catalog::LEGACY_MAX_FIRSTPARTY_MODEL_IDS;
use super::providers::{get_api_provider, APIProvider};

pub type ModelShortName = String;
pub type ModelName = String;

/// Mirrors TS `ModelSetting = ModelName | ModelAlias | null`. `None` here is
/// the TS `null` (use the default model); strings carry both literal model IDs
/// and alias strings.
pub type ModelSetting = Option<String>;

// ─── Bootstrap state ─────────────────────────────────────────────────────────

/// Mirrors `bootstrap/state.ts`'s `mainLoopModelOverride` slot. The TS code
/// uses `undefined` to mean "no in-session override", and `null` to mean
/// "user pressed default". We use `Option<Option<String>>` to model that.
fn main_loop_model_override_slot() -> &'static Mutex<Option<Option<String>>> {
    static SLOT: OnceLock<Mutex<Option<Option<String>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Set the in-session model override. `value = Some(None)` clears it back to
/// the default; `value = None` removes the slot entirely (matches TS
/// `undefined`).
pub fn set_main_loop_model_override(value: Option<Option<String>>) {
    *main_loop_model_override_slot().lock().unwrap() = value;
}

pub fn get_main_loop_model_override() -> Option<Option<String>> {
    main_loop_model_override_slot().lock().unwrap().clone()
}

fn get_custom_backend_default_model() -> Option<ModelName> {
    if !is_custom_backend_enabled() {
        return None;
    }
    get_custom_backend_model().filter(|s| !s.is_empty())
}

pub fn get_small_fast_model() -> ModelName {
    if let Ok(v) = std::env::var("MOSSEN_CODE_SMALL_FAST_MODEL") {
        if !v.is_empty() {
            return v;
        }
    }
    if let Some(m) = get_custom_backend_default_model() {
        return m;
    }
    get_default_fast_model()
}

pub fn is_non_custom_max_model(model: &str) -> bool {
    let ms = get_model_strings();
    model == ms.max40 || model == ms.max41 || model == ms.max45 || model == ms.max46
}

/// Settings-derived helpers
fn settings_model() -> Option<String> {
    get_session_settings_cache()
        .and_then(|s| s.settings.model.clone())
        .filter(|s| !s.is_empty())
}

/// Helper to get the model from /model (including via /config), the --model
/// flag, environment variable, or the saved settings.
pub fn get_user_specified_model_setting() -> Option<ModelSetting> {
    let specified_model: Option<String>;

    if let Some(override_val) = get_main_loop_model_override() {
        // TS distinguishes undefined vs null: undefined => use other sources;
        // null => "use default". We've already filtered out the
        // "no override" state by checking the outer Option.
        specified_model = override_val;
    } else {
        specified_model = get_custom_backend_model()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("MOSSEN_CODE_MODEL").ok().filter(|s| !s.is_empty()))
            .or_else(settings_model);
    }

    if let Some(ref m) = specified_model {
        if !is_model_allowed(m) {
            return None;
        }
    }

    // TS returns `undefined` when no source supplied a model; here we collapse
    // that into `None`. A `Some(None)` would mean "explicit default" but the
    // model-loop callers don't rely on that distinction past this boundary.
    if specified_model.is_none() {
        return None;
    }
    Some(specified_model)
}

pub fn get_main_loop_model() -> ModelName {
    if let Some(Some(model)) = get_user_specified_model_setting() {
        return parse_user_specified_model(&model);
    }
    get_default_main_loop_model()
}

pub fn get_best_model() -> ModelName {
    get_default_max_model()
}

/// `getDefaultMaxModel` — default Max model for the active provider.
pub fn get_default_max_model() -> ModelName {
    if let Some(custom) = get_custom_backend_default_model() {
        return custom;
    }
    if let Ok(env_model) = std::env::var("MOSSEN_CODE_DEFAULT_MAX_MODEL") {
        if !env_model.is_empty() {
            return env_model;
        }
    }
    let ms = get_model_strings();
    if is_custom_backend_enabled() || get_api_provider() != APIProvider::FirstParty {
        return ms.max46.clone();
    }
    ms.max46.clone()
}

pub fn get_default_balanced_model() -> ModelName {
    if let Some(custom) = get_custom_backend_default_model() {
        return custom;
    }
    if let Ok(env_model) = std::env::var("MOSSEN_CODE_DEFAULT_BALANCED_MODEL") {
        if !env_model.is_empty() {
            return env_model;
        }
    }
    let ms = get_model_strings();
    if is_custom_backend_enabled() || get_api_provider() != APIProvider::FirstParty {
        return ms.balanced45.clone();
    }
    ms.balanced46.clone()
}

pub fn get_default_fast_model() -> ModelName {
    if let Some(custom) = get_custom_backend_default_model() {
        return custom;
    }
    if let Ok(env_model) = std::env::var("MOSSEN_CODE_DEFAULT_FAST_MODEL") {
        if !env_model.is_empty() {
            return env_model;
        }
    }
    let ms = get_model_strings();
    ms.fast45.clone()
}

/// Subset of the runtime context needed to decide the active main-loop model.
/// Matches TS function `getRuntimeMainLoopModel` params shape.
#[derive(Debug, Clone)]
pub struct RuntimeMainLoopModelParams<'a> {
    pub permission_mode: &'a str,
    pub main_loop_model: &'a str,
    pub exceeds_200k_tokens: bool,
}

pub fn get_runtime_main_loop_model(params: RuntimeMainLoopModelParams<'_>) -> ModelName {
    let user_setting = get_user_specified_model_setting();

    let is_plan = params.permission_mode == "plan";

    if matches!(user_setting, Some(Some(ref s)) if s == "maxplan")
        && is_plan
        && !params.exceeds_200k_tokens
    {
        return get_default_max_model();
    }

    if matches!(user_setting, Some(Some(ref s)) if s == "fast") && is_plan {
        return get_default_balanced_model();
    }

    params.main_loop_model.to_string()
}

/// `getDefaultMainLoopModelSetting` — built-in default. May return a model
/// alias string (e.g. `max[1m]`) rather than a canonical ID.
pub fn get_default_main_loop_model_setting() -> String {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        if let Some(cfg) = get_internal_model_override_config() {
            if let Some(default) = cfg.default_model {
                return default;
            }
        }
        return get_default_max_model() + "[1m]";
    }

    if is_max_subscriber() {
        let suffix = if is_max_1m_merge_enabled() { "[1m]" } else { "" };
        return get_default_max_model() + suffix;
    }

    if is_team_premium_subscriber() {
        let suffix = if is_max_1m_merge_enabled() { "[1m]" } else { "" };
        return get_default_max_model() + suffix;
    }

    get_default_balanced_model()
}

pub fn get_default_main_loop_model() -> ModelName {
    parse_user_specified_model(&get_default_main_loop_model_setting())
}

struct CanonicalPattern {
    canonical: &'static str,
    first_party_needles: Vec<String>,
    external_provider_needles: Vec<String>,
}

fn get_canonical_model_patterns() -> Vec<CanonicalPattern> {
    let canonicals = [
        "mossen-max-4-6",
        "mossen-max-4-5",
        "mossen-max-4-1",
        "mossen-max-4",
        "mossen-balanced-4-6",
        "mossen-balanced-4-5",
        "mossen-balanced-4",
        "mossen-fast-4-5",
        "mossen-3-7-balanced",
        "mossen-3-5-balanced",
        "mossen-3-5-fast",
        "mossen-3-max",
        "mossen-3-balanced",
        "mossen-3-fast",
    ];
    canonicals
        .iter()
        .map(|c| CanonicalPattern {
            canonical: c,
            first_party_needles: vec![c.to_string()],
            external_provider_needles: vec![external_provider_model_stem_from_mossen_id(c)],
        })
        .collect()
}

static MOSSEN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(mossen-[a-z0-9]+(?:-[a-z0-9]+)*)").expect("mossen pattern regex")
});

pub fn first_party_name_to_canonical(name: &str) -> ModelShortName {
    let name = name.to_lowercase();
    for pattern in get_canonical_model_patterns() {
        let all_needles = pattern
            .first_party_needles
            .iter()
            .chain(pattern.external_provider_needles.iter());
        for needle in all_needles {
            if name.contains(needle.as_str()) {
                return pattern.canonical.to_string();
            }
        }
    }
    if let Some(cap) = MOSSEN_PATTERN.captures(&name) {
        if let Some(matched) = cap.get(1) {
            return matched.as_str().to_string();
        }
    }
    if let Some(cap) = external_provider_model_stem_pattern().captures(&name) {
        if let Some(matched) = cap.get(1) {
            let prefix = format!("{}-", external_provider_model_prefix());
            return matched.as_str().replacen(&prefix, "mossen-", 1);
        }
    }
    name
}

pub fn get_canonical_name(full_model_name: &str) -> ModelShortName {
    first_party_name_to_canonical(&resolve_overridden_model(full_model_name))
}

pub fn get_hosted_user_default_model_description(fast_mode: bool) -> String {
    if is_max_subscriber() || is_team_premium_subscriber() {
        let suffix = if fast_mode {
            get_max46_pricing_suffix(true)
        } else {
            String::new()
        };
        if is_max_1m_merge_enabled() {
            return format!(
                "Mossen Max 4.6 with 1M context · Most capable for complex work{}",
                suffix
            );
        }
        return format!("Mossen Max 4.6 · Most capable for complex work{}", suffix);
    }
    "Mossen Balanced 4.6 · Best for everyday tasks".to_string()
}

pub fn render_default_model_setting(setting: &str) -> String {
    if setting == "maxplan" {
        return "Mossen Max 4.6 in plan mode, else Mossen Balanced 4.6".to_string();
    }
    render_model_name(&parse_user_specified_model(setting))
}

pub fn get_max46_pricing_suffix(fast_mode: bool) -> String {
    if get_api_provider() != APIProvider::FirstParty {
        return String::new();
    }
    let costs = get_max_46_cost_tier(fast_mode);
    let pricing = format_model_pricing(&costs);
    let fast_indicator = if fast_mode {
        format!(" ({})", LIGHTNING_BOLT)
    } else {
        String::new()
    };
    format!(" ·{} {}", fast_indicator, pricing)
}

pub fn is_max_1m_merge_enabled() -> bool {
    if is_1m_context_disabled()
        || is_pro_subscriber()
        || get_api_provider() != APIProvider::FirstParty
    {
        return false;
    }
    if is_hosted_subscriber() && get_subscription_type().is_none() {
        return false;
    }
    true
}

pub fn render_model_setting(setting: &str) -> String {
    match setting {
        "maxplan" => "Mossen Plan".to_string(),
        "max" => "Mossen Max".to_string(),
        "balanced" => "Mossen Balanced".to_string(),
        "fast" => "Mossen Fast".to_string(),
        s if is_model_alias(s) => capitalize(s),
        s => render_model_name(s),
    }
}

pub fn get_public_model_display_name(model: &str) -> Option<String> {
    let ms = get_model_strings();
    let name = match model {
        m if m == ms.max46 => "Mossen Max 4.6",
        m if m == format!("{}[1m]", ms.max46) => "Mossen Max 4.6 (1M context)",
        m if m == ms.max45 => "Mossen Max 4.5",
        m if m == ms.max41 => "Mossen Max 4.1",
        m if m == ms.max40 => "Mossen Max 4",
        m if m == format!("{}[1m]", ms.balanced46) => "Mossen Balanced 4.6 (1M context)",
        m if m == ms.balanced46 => "Mossen Balanced 4.6",
        m if m == format!("{}[1m]", ms.balanced45) => "Mossen Balanced 4.5 (1M context)",
        m if m == ms.balanced45 => "Mossen Balanced 4.5",
        m if m == ms.balanced40 => "Mossen Balanced 4",
        m if m == format!("{}[1m]", ms.balanced40) => "Mossen Balanced 4 (1M context)",
        m if m == ms.balanced37 => "Mossen Balanced 3.7",
        m if m == ms.balanced35 => "Mossen Balanced 3.5",
        m if m == ms.fast45 => "Mossen Fast 4.5",
        m if m == ms.fast35 => "Mossen Fast 3.5",
        _ => return None,
    };
    Some(name.to_string())
}

fn mask_model_codename(base_name: &str) -> String {
    let mut parts = base_name.split('-');
    let codename = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    let masked_codename = {
        let head: String = codename.chars().take(3).collect();
        let stars: String = "*".repeat(codename.chars().count().saturating_sub(3));
        format!("{}{}", head, stars)
    };
    if rest.is_empty() {
        masked_codename
    } else {
        format!("{}-{}", masked_codename, rest.join("-"))
    }
}

static M1M_SUFFIX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\[1m\]$").unwrap());

pub fn render_model_name(model: &str) -> String {
    if let Some(public_name) = get_public_model_display_name(model) {
        return public_name;
    }
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        let resolved = parse_user_specified_model(model);
        if let Some(internal_model) = resolve_internal_model(Some(model)) {
            let base_name = M1M_SUFFIX.replace(&internal_model.model, "").to_string();
            let masked = mask_model_codename(&base_name);
            let suffix = if has_1m_context(&resolved) { "[1m]" } else { "" };
            return format!("{}{}", masked, suffix);
        }
        if resolved != model {
            return format!("{} ({})", model, resolved);
        }
        return resolved;
    }
    model.to_string()
}

pub fn get_public_model_name(model: &str) -> String {
    if let Some(public_name) = get_public_model_display_name(model) {
        if public_name.starts_with("Mossen ") {
            return public_name;
        }
        return format!("Mossen {}", public_name);
    }
    format!("Mossen ({})", model)
}

pub fn parse_user_specified_model(model_input: &str) -> ModelName {
    let trimmed = model_input.trim();
    let normalized = trimmed.to_lowercase();

    let has_1m_tag = has_1m_context(&normalized);
    let model_string = if has_1m_tag {
        M1M_SUFFIX.replace(&normalized, "").trim().to_string()
    } else {
        normalized.clone()
    };

    if is_model_alias(&model_string) {
        let suffix = if has_1m_tag { "[1m]" } else { "" };
        if let Some(alias) = ModelAlias::from_str(&model_string) {
            match alias {
                ModelAlias::MaxPlan => {
                    // Balanced is default, Max in plan mode.
                    return format!("{}{}", get_default_balanced_model(), suffix);
                }
                ModelAlias::Balanced => return format!("{}{}", get_default_balanced_model(), suffix),
                ModelAlias::Fast => return format!("{}{}", get_default_fast_model(), suffix),
                ModelAlias::Max => return format!("{}{}", get_default_max_model(), suffix),
                ModelAlias::Best => return get_best_model(),
                _ => {}
            }
        }
    }

    if !is_custom_backend_enabled()
        && get_api_provider() == APIProvider::FirstParty
        && is_legacy_max_first_party(&model_string)
        && is_legacy_model_remap_enabled()
    {
        let suffix = if has_1m_tag { "[1m]" } else { "" };
        return format!("{}{}", get_default_max_model(), suffix);
    }

    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        let has_1m_internal_tag = has_1m_context(&normalized);
        let base_internal_model = M1M_SUFFIX.replace(&normalized, "").trim().to_string();

        if let Some(internal_model) = resolve_internal_model(Some(&base_internal_model)) {
            let suffix = if has_1m_internal_tag { "[1m]" } else { "" };
            return format!("{}{}", internal_model.model, suffix);
        }
    }

    if has_1m_tag {
        let stripped = M1M_SUFFIX.replace(trimmed, "").trim().to_string();
        return format!("{}[1m]", stripped);
    }
    trimmed.to_string()
}

pub fn resolve_skill_model_override(skill_model: &str, current_model: &str) -> String {
    if has_1m_context(skill_model) || !has_1m_context(current_model) {
        return skill_model.to_string();
    }
    let resolved = parse_user_specified_model(skill_model);
    let canonical = get_canonical_name(&resolved);
    if model_supports_1m(
        &resolved,
        is_custom_backend_enabled(),
        custom_backend_capability_applies_to_model(&resolved),
        get_custom_backend_max_input_tokens(),
        &canonical,
    ) {
        return format!("{}[1m]", skill_model);
    }
    skill_model.to_string()
}

fn is_legacy_max_first_party(model: &str) -> bool {
    LEGACY_MAX_FIRSTPARTY_MODEL_IDS
        .iter()
        .any(|m| m.as_str() == model)
}

pub fn is_legacy_model_remap_enabled() -> bool {
    !is_env_truthy(
        std::env::var("MOSSEN_CODE_DISABLE_LEGACY_MODEL_REMAP")
            .ok()
            .as_deref(),
    )
}

pub fn model_display_string(model: ModelSetting) -> String {
    if model.is_none() {
        if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
            return format!(
                "Default for internal users ({})",
                render_default_model_setting(&get_default_main_loop_model_setting())
            );
        }
        if is_hosted_subscriber() {
            return format!(
                "Default ({})",
                get_hosted_user_default_model_description(false)
            );
        }
        return format!("Default ({})", get_default_main_loop_model());
    }
    let model = model.unwrap();
    let resolved_model = parse_user_specified_model(&model);
    if model == resolved_model {
        resolved_model
    } else {
        format!("{} ({})", model, resolved_model)
    }
}

pub fn get_marketing_name_for_model(model_id: &str) -> Option<String> {
    if get_api_provider() == APIProvider::Foundry {
        return None;
    }
    let has1m = model_id.to_lowercase().contains("[1m]");
    let canonical = get_canonical_name(model_id);

    if canonical.contains("mossen-max-4-6") {
        return Some(
            if has1m {
                "Mossen Max 4.6 (with 1M context)"
            } else {
                "Mossen Max 4.6"
            }
            .to_string(),
        );
    }
    if canonical.contains("mossen-max-4-5") {
        return Some("Mossen Max 4.5".to_string());
    }
    if canonical.contains("mossen-max-4-1") {
        return Some("Mossen Max 4.1".to_string());
    }
    if canonical.contains("mossen-max-4") {
        return Some("Mossen Max 4".to_string());
    }
    if canonical.contains("mossen-balanced-4-6") {
        return Some(
            if has1m {
                "Mossen Balanced 4.6 (with 1M context)"
            } else {
                "Mossen Balanced 4.6"
            }
            .to_string(),
        );
    }
    if canonical.contains("mossen-balanced-4-5") {
        return Some(
            if has1m {
                "Mossen Balanced 4.5 (with 1M context)"
            } else {
                "Mossen Balanced 4.5"
            }
            .to_string(),
        );
    }
    if canonical.contains("mossen-balanced-4") {
        return Some(
            if has1m {
                "Mossen Balanced 4 (with 1M context)"
            } else {
                "Mossen Balanced 4"
            }
            .to_string(),
        );
    }
    if canonical.contains("mossen-3-7-balanced") {
        return Some("Mossen Balanced 3.7".to_string());
    }
    if canonical.contains("mossen-3-5-balanced") {
        return Some("Mossen Balanced 3.5".to_string());
    }
    if canonical.contains("mossen-fast-4-5") {
        return Some("Mossen Fast 4.5".to_string());
    }
    if canonical.contains("mossen-3-5-fast") {
        return Some("Mossen Fast 3.5".to_string());
    }
    None
}

static M1M_2M_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\[(1|2)m\]").unwrap());

pub fn normalize_model_string_for_api(model: &str) -> String {
    M1M_2M_NORMALIZE.replace_all(model, "").to_string()
}
