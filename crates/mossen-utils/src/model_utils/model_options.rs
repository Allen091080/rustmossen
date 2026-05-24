//! `/model` picker option builders.
//!
//! Direct translation of `utils/model/modelOptions.ts`. The TS source is
//! deeply branched on subscription tier, custom-backend, and 1P/3P provider
//! state; we mirror each helper one-for-one.

use crate::auth::{is_hosted_subscriber, is_max_subscriber, is_team_premium_subscriber};
use crate::config::{get_global_config, ModelOption as ConfigModelOption};
use crate::context::has_1m_context;
use crate::custom_backend::{get_custom_backend_model, is_custom_backend_enabled};
use crate::model_cost::{format_model_pricing, COST_FAST_35, COST_FAST_45, COST_TIER_3_15};
use crate::settings::get_session_settings_cache;

use super::internal_models::get_internal_models;
use super::check_1m_access::{check_max_1m_access, check_balanced_1m_access};
use super::model::{
    get_canonical_name, get_default_fast_model, get_default_main_loop_model_setting,
    get_default_max_model, get_default_balanced_model, get_hosted_user_default_model_description,
    get_marketing_name_for_model, get_max46_pricing_suffix, get_user_specified_model_setting,
    is_max_1m_merge_enabled, render_default_model_setting, ModelSetting,
};
use super::model_allowlist::is_model_allowed;
use super::model_strings::get_model_strings;
use super::providers::{get_api_provider, APIProvider};

/// `ModelOption` — option entry rendered by the `/model` picker.
#[derive(Debug, Clone)]
pub struct ModelOption {
    pub value: ModelSetting,
    pub label: String,
    pub description: String,
    pub description_for_model: Option<String>,
}

impl ModelOption {
    fn with_label_value(value: ModelSetting, label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            description: description.into(),
            description_for_model: None,
        }
    }
}

fn localized_text(en: &str, zh: &str) -> String {
    let zh_preferred = matches!(
        std::env::var("MOSSEN_LANG").ok().as_deref(),
        Some("zh") | Some("zh-CN") | Some("zh-Hans") | Some("zh-Hant") | Some("zh-TW")
    );
    if zh_preferred { zh.to_string() } else { en.to_string() }
}

fn uses_third_party_model_surface() -> bool {
    is_custom_backend_enabled() || get_api_provider() != APIProvider::FirstParty
}

pub fn get_default_option_for_user(fast_mode: bool) -> ModelOption {
    let default_label = localized_text("Default (recommended)", "默认（推荐）");

    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        let current_model = render_default_model_setting(&get_default_main_loop_model_setting());
        return ModelOption {
            value: None,
            label: default_label,
            description: localized_text(
                &format!(
                    "Use the default model for internal users (currently {})",
                    current_model
                ),
                &format!("使用内部默认模型（当前为 {}）", current_model),
            ),
            description_for_model: Some(localized_text(
                &format!("Default model (currently {})", current_model),
                &format!("默认模型（当前为 {}）", current_model),
            )),
        };
    }

    if is_custom_backend_enabled() {
        let model_string = get_custom_backend_model();
        let current_model = match model_string {
            Some(m) if !m.is_empty() => render_default_model_setting(&m),
            _ => "the configured backend default".to_string(),
        };
        return ModelOption {
            value: None,
            label: default_label,
            description: localized_text(
                &format!(
                    "Use the custom backend default (currently {})",
                    current_model
                ),
                &format!("使用自定义后端默认模型（当前为 {}）", current_model),
            ),
            description_for_model: Some(localized_text(
                &format!("Custom backend default model (currently {})", current_model),
                &format!("自定义后端默认模型（当前为 {}）", current_model),
            )),
        };
    }

    if is_hosted_subscriber() {
        return ModelOption {
            value: None,
            label: default_label,
            description: get_hosted_user_default_model_description(fast_mode),
            description_for_model: None,
        };
    }

    let is_3p = uses_third_party_model_surface();
    let default_label_for_value = render_default_model_setting(&get_default_main_loop_model_setting());
    let pricing_suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_TIER_3_15))
    };
    ModelOption {
        value: None,
        label: default_label,
        description: localized_text(
            &format!(
                "Use the default model (currently {}){}",
                default_label_for_value, pricing_suffix
            ),
            &format!(
                "使用默认模型（当前为 {}){}",
                default_label_for_value, pricing_suffix
            ),
        ),
        description_for_model: None,
    }
}

fn read_env_string(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn get_custom_balanced_option() -> Option<ModelOption> {
    let is_3p = uses_third_party_model_surface();
    let custom_balanced_model = read_env_string("MOSSEN_CODE_DEFAULT_BALANCED_MODEL")?;
    if !is_3p {
        return None;
    }
    let is1m = has_1m_context(&custom_balanced_model);
    let default_description = localized_text(
        if is1m {
            "Custom balanced model (1M context)"
        } else {
            "Custom balanced model"
        },
        if is1m {
            "自定义均衡模型（1M 上下文）"
        } else {
            "自定义均衡模型"
        },
    );
    let label = read_env_string("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_NAME")
        .unwrap_or_else(|| custom_balanced_model.clone());
    let description = read_env_string("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    let dfm = read_env_string("MOSSEN_CODE_DEFAULT_BALANCED_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    Some(ModelOption {
        value: Some("balanced".to_string()),
        label,
        description,
        description_for_model: Some(format!("{} ({})", dfm, custom_balanced_model)),
    })
}

fn get_balanced46_option() -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_TIER_3_15))
    };
    let value = if is_3p {
        Some(get_model_strings().balanced46)
    } else {
        Some("balanced".to_string())
    };
    ModelOption {
        value,
        label: "Mossen Balanced".to_string(),
        description: localized_text(
            &format!("Mossen Balanced 4.6 · Best for everyday tasks{}", suffix),
            &format!("Mossen Balanced 4.6 · 适合日常任务{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Balanced 4.6 - best for everyday tasks. Generally recommended for most coding tasks",
            "Mossen Balanced 4.6 - 适合日常任务，通常推荐用于大多数编码任务",
        )),
    }
}

fn get_custom_max_option() -> Option<ModelOption> {
    let is_3p = uses_third_party_model_surface();
    let custom_max_model = read_env_string("MOSSEN_CODE_DEFAULT_MAX_MODEL")?;
    if !is_3p {
        return None;
    }
    let is1m = has_1m_context(&custom_max_model);
    let default_description = localized_text(
        if is1m {
            "Custom max model (1M context)"
        } else {
            "Custom max model"
        },
        if is1m {
            "自定义前沿模型（1M 上下文）"
        } else {
            "自定义前沿模型"
        },
    );
    let label = read_env_string("MOSSEN_CODE_DEFAULT_MAX_MODEL_NAME")
        .unwrap_or_else(|| custom_max_model.clone());
    let description = read_env_string("MOSSEN_CODE_DEFAULT_MAX_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    let dfm = read_env_string("MOSSEN_CODE_DEFAULT_MAX_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    Some(ModelOption {
        value: Some("max".to_string()),
        label,
        description,
        description_for_model: Some(format!("{} ({})", dfm, custom_max_model)),
    })
}

fn get_max41_option() -> ModelOption {
    ModelOption {
        value: Some("max".to_string()),
        label: "Mossen Max 4.1".to_string(),
        description: localized_text("Mossen Max 4.1 · Legacy", "Mossen Max 4.1 · 旧版"),
        description_for_model: Some(localized_text(
            "Mossen Max 4.1 - legacy version",
            "Mossen Max 4.1 - 旧版",
        )),
    }
}

fn get_max46_option(fast_mode: bool) -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let value = if is_3p {
        Some(get_model_strings().max46)
    } else {
        Some("max".to_string())
    };
    let suffix = get_max46_pricing_suffix(fast_mode);
    ModelOption {
        value,
        label: "Mossen Max".to_string(),
        description: localized_text(
            &format!("Mossen Max 4.6 · Most capable for complex work{}", suffix),
            &format!("Mossen Max 4.6 · 最适合复杂任务{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Max 4.6 - most capable for complex work",
            "Mossen Max 4.6 - 最适合复杂任务",
        )),
    }
}

pub fn get_balanced46_1m_option() -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let value = if is_3p {
        Some(format!("{}[1m]", get_model_strings().balanced46))
    } else {
        Some("balanced[1m]".to_string())
    };
    let suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_TIER_3_15))
    };
    ModelOption {
        value,
        label: localized_text(
            "Mossen Balanced (1M context)",
            "Mossen Balanced（1M 上下文）",
        ),
        description: localized_text(
            &format!("Mossen Balanced 4.6 for long sessions{}", suffix),
            &format!("Mossen Balanced 4.6 · 适合长会话{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Balanced 4.6 with 1M context window - for long sessions with large codebases",
            "Mossen Balanced 4.6 · 1M 上下文窗口，适合大型代码库的长会话",
        )),
    }
}

pub fn get_max46_1m_option(fast_mode: bool) -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let value = if is_3p {
        Some(format!("{}[1m]", get_model_strings().max46))
    } else {
        Some("max[1m]".to_string())
    };
    let suffix = get_max46_pricing_suffix(fast_mode);
    ModelOption {
        value,
        label: localized_text(
            "Mossen Max (1M context)",
            "Mossen Max（1M 上下文）",
        ),
        description: localized_text(
            &format!("Mossen Max 4.6 for long sessions{}", suffix),
            &format!("Mossen Max 4.6 · 适合长会话{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Max 4.6 with 1M context window - for long sessions with large codebases",
            "Mossen Max 4.6 · 1M 上下文窗口，适合大型代码库的长会话",
        )),
    }
}

fn get_custom_fast_option() -> Option<ModelOption> {
    let is_3p = uses_third_party_model_surface();
    let custom_fast_model = read_env_string("MOSSEN_CODE_DEFAULT_FAST_MODEL")?;
    if !is_3p {
        return None;
    }
    let default_description = localized_text("Custom fast model", "自定义快速模型");
    let label = read_env_string("MOSSEN_CODE_DEFAULT_FAST_MODEL_NAME")
        .unwrap_or_else(|| custom_fast_model.clone());
    let description = read_env_string("MOSSEN_CODE_DEFAULT_FAST_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    let dfm = read_env_string("MOSSEN_CODE_DEFAULT_FAST_MODEL_DESCRIPTION")
        .unwrap_or_else(|| default_description.clone());
    Some(ModelOption {
        value: Some("fast".to_string()),
        label,
        description,
        description_for_model: Some(format!("{} ({})", dfm, custom_fast_model)),
    })
}

fn get_fast45_option() -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_FAST_45))
    };
    ModelOption {
        value: Some("fast".to_string()),
        label: "Mossen Fast".to_string(),
        description: localized_text(
            &format!("Mossen Fast 4.5 · Fastest for quick answers{}", suffix),
            &format!("Mossen Fast 4.5 · 最适合快速回答{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Fast 4.5 - fastest for quick answers. Lower cost but less capable than Mossen Balanced 4.6.",
            "Mossen Fast 4.5 - 最适合快速回答，成本更低，但能力弱于 Mossen Balanced 4.6。",
        )),
    }
}

fn get_fast35_option() -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_FAST_35))
    };
    ModelOption {
        value: Some("fast".to_string()),
        label: "Mossen Fast".to_string(),
        description: localized_text(
            &format!("Mossen Fast 3.5 for simple tasks{}", suffix),
            &format!("Mossen Fast 3.5 · 适合简单任务{}", suffix),
        ),
        description_for_model: Some(localized_text(
            "Mossen Fast 3.5 - faster and lower cost, but less capable than Mossen Balanced. Use for simple tasks.",
            "Mossen Fast 3.5 - 更快且成本更低，但能力弱于 Mossen Balanced，适合简单任务。",
        )),
    }
}

fn get_fast_option() -> ModelOption {
    let fast_model = get_default_fast_model();
    let ms = get_model_strings();
    if fast_model == ms.fast45 {
        get_fast45_option()
    } else {
        get_fast35_option()
    }
}

fn get_max_max_option(fast_mode: bool) -> ModelOption {
    let suffix = if fast_mode {
        get_max46_pricing_suffix(true)
    } else {
        String::new()
    };
    ModelOption::with_label_value(
        Some("max".to_string()),
        "Mossen Max",
        format!(
            "Mossen Max 4.6 · Most capable for complex work{}",
            suffix
        ),
    )
}

pub fn get_max_balanced46_1m_option() -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let billing_info = if is_hosted_subscriber() {
        " · Billed as extra usage"
    } else {
        ""
    };
    let pricing_suffix = if is_3p {
        String::new()
    } else {
        format!(" · {}", format_model_pricing(&COST_TIER_3_15))
    };
    ModelOption::with_label_value(
        Some("balanced[1m]".to_string()),
        "Mossen Balanced (1M context)",
        format!(
            "Mossen Balanced 4.6 with 1M context{}{}",
            billing_info, pricing_suffix
        ),
    )
}

pub fn get_max_max46_1m_option(fast_mode: bool) -> ModelOption {
    let billing_info = if is_hosted_subscriber() {
        " · Billed as extra usage"
    } else {
        ""
    };
    let pricing_suffix = get_max46_pricing_suffix(fast_mode);
    ModelOption::with_label_value(
        Some("max[1m]".to_string()),
        "Mossen Max (1M context)",
        format!(
            "Mossen Max 4.6 with 1M context{}{}",
            billing_info, pricing_suffix
        ),
    )
}

fn get_merged_max_1m_option(fast_mode: bool) -> ModelOption {
    let is_3p = uses_third_party_model_surface();
    let value = if is_3p {
        Some(format!("{}[1m]", get_model_strings().max46))
    } else {
        Some("max[1m]".to_string())
    };
    let pricing = if !is_3p && fast_mode {
        get_max46_pricing_suffix(fast_mode)
    } else {
        String::new()
    };
    ModelOption {
        value,
        label: "Mossen Max (1M context)".to_string(),
        description: format!(
            "Mossen Max 4.6 with 1M context · Most capable for complex work{}",
            pricing
        ),
        description_for_model: Some(
            "Mossen Max 4.6 with 1M context - most capable for complex work".to_string(),
        ),
    }
}

fn max_balanced46_option() -> ModelOption {
    ModelOption::with_label_value(
        Some("balanced".to_string()),
        "Mossen Balanced",
        "Mossen Balanced 4.6 · Best for everyday tasks",
    )
}

fn max_fast45_option() -> ModelOption {
    ModelOption::with_label_value(
        Some("fast".to_string()),
        "Mossen Fast",
        "Mossen Fast 4.5 · Fastest for quick answers",
    )
}

fn get_max_plan_option() -> ModelOption {
    ModelOption::with_label_value(
        Some("maxplan".to_string()),
        "Mossen Plan Mode",
        "Use Mossen Max 4.6 in plan mode, Mossen Balanced 4.6 otherwise",
    )
}

fn get_model_options_base(fast_mode: bool) -> Vec<ModelOption> {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
        let internal_model_options: Vec<ModelOption> = get_internal_models()
            .into_iter()
            .map(|m| ModelOption {
                value: Some(m.alias.clone()),
                label: m.label.clone(),
                description: m.description.unwrap_or_else(|| {
                    format!("[INTERNAL] {} ({})", m.label, m.model)
                }),
                description_for_model: None,
            })
            .collect();

        let mut out = vec![get_default_option_for_user(false)];
        out.extend(internal_model_options);
        out.push(get_merged_max_1m_option(fast_mode));
        out.push(get_balanced46_option());
        out.push(get_balanced46_1m_option());
        out.push(get_fast45_option());
        return out;
    }

    if is_custom_backend_enabled() {
        return vec![get_default_option_for_user(fast_mode)];
    }

    if is_hosted_subscriber() {
        if is_max_subscriber() || is_team_premium_subscriber() {
            let mut premium = vec![get_default_option_for_user(fast_mode)];
            if !is_max_1m_merge_enabled() && check_max_1m_access() {
                premium.push(get_max_max46_1m_option(fast_mode));
            }
            premium.push(max_balanced46_option());
            if check_balanced_1m_access() {
                premium.push(get_max_balanced46_1m_option());
            }
            premium.push(max_fast45_option());
            return premium;
        }

        let mut standard = vec![get_default_option_for_user(fast_mode)];
        if check_balanced_1m_access() {
            standard.push(get_max_balanced46_1m_option());
        }
        if is_max_1m_merge_enabled() {
            standard.push(get_merged_max_1m_option(fast_mode));
        } else {
            standard.push(get_max_max_option(fast_mode));
            if check_max_1m_access() {
                standard.push(get_max_max46_1m_option(fast_mode));
            }
        }
        standard.push(max_fast45_option());
        return standard;
    }

    if !uses_third_party_model_surface() {
        let mut payg1p = vec![get_default_option_for_user(fast_mode)];
        if check_balanced_1m_access() {
            payg1p.push(get_balanced46_1m_option());
        }
        if is_max_1m_merge_enabled() {
            payg1p.push(get_merged_max_1m_option(fast_mode));
        } else {
            payg1p.push(get_max46_option(fast_mode));
            if check_max_1m_access() {
                payg1p.push(get_max46_1m_option(fast_mode));
            }
        }
        payg1p.push(get_fast45_option());
        return payg1p;
    }

    let mut payg3p = vec![get_default_option_for_user(fast_mode)];

    if let Some(custom_balanced) = get_custom_balanced_option() {
        payg3p.push(custom_balanced);
    } else {
        payg3p.push(get_balanced46_option());
        if check_balanced_1m_access() {
            payg3p.push(get_balanced46_1m_option());
        }
    }

    if let Some(custom_max) = get_custom_max_option() {
        payg3p.push(custom_max);
    } else {
        payg3p.push(get_max41_option());
        payg3p.push(get_max46_option(fast_mode));
        if check_max_1m_access() {
            payg3p.push(get_max46_1m_option(fast_mode));
        }
    }
    if let Some(custom_fast) = get_custom_fast_option() {
        payg3p.push(custom_fast);
    } else {
        payg3p.push(get_fast_option());
    }
    payg3p
}

/// `getModelFamilyInfo` — alias label + marketing string for the current
/// default that the alias resolves to.
fn get_model_family_info(model: &str) -> Option<(String, String)> {
    let canonical = get_canonical_name(model);

    if canonical.contains("mossen-balanced-4-6")
        || canonical.contains("mossen-balanced-4-5")
        || canonical.contains("mossen-balanced-4-")
        || canonical.contains("mossen-3-7-balanced")
        || canonical.contains("mossen-3-5-balanced")
    {
        if let Some(name) = get_marketing_name_for_model(&get_default_balanced_model()) {
            return Some(("Mossen Balanced".to_string(), name));
        }
    }

    if canonical.contains("mossen-max-4") {
        if let Some(name) = get_marketing_name_for_model(&get_default_max_model()) {
            return Some(("Mossen Max".to_string(), name));
        }
    }

    if canonical.contains("mossen-fast") || canonical.contains("mossen-3-5-fast") {
        if let Some(name) = get_marketing_name_for_model(&get_default_fast_model()) {
            return Some(("Mossen Fast".to_string(), name));
        }
    }

    None
}

fn get_known_model_option(model: &str) -> Option<ModelOption> {
    let marketing_name = get_marketing_name_for_model(model)?;
    let family_info = get_model_family_info(model);
    let description = match &family_info {
        Some((alias, current_version_name)) if &marketing_name != current_version_name => {
            format!(
                "Newer version available · select {} for {}",
                alias, current_version_name
            )
        }
        _ => model.to_string(),
    };
    Some(ModelOption {
        value: Some(model.to_string()),
        label: marketing_name,
        description,
        description_for_model: None,
    })
}

fn additional_model_options() -> Vec<ModelOption> {
    let cfg = get_global_config();
    let Some(extras) = cfg.additional_model_options_cache else {
        return Vec::new();
    };
    extras
        .into_iter()
        .filter_map(|raw: ConfigModelOption| convert_extras_entry(raw))
        .collect()
}

fn convert_extras_entry(raw: ConfigModelOption) -> Option<ModelOption> {
    let extra = raw.extra;
    let value = extra
        .get("value")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let label = extra.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if label.is_empty() {
        return None;
    }
    let description = extra
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description_for_model = extra
        .get("descriptionForModel")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some(ModelOption {
        value,
        label,
        description,
        description_for_model,
    })
}

fn initial_main_loop_model() -> ModelSetting {
    // TS `getInitialMainLoopModel` reads from a bootstrap-state slot populated
    // at process start; we mirror it with a static slot in `model::main_loop_model_override_slot`
    // initialized by the entry point.
    super::model::get_main_loop_model_override().and_then(|x| x)
}

pub fn get_model_options(fast_mode: bool) -> Vec<ModelOption> {
    let mut options = get_model_options_base(fast_mode);

    if let Ok(env_custom_model) = std::env::var("MOSSEN_CODE_CUSTOM_MODEL_OPTION") {
        if !env_custom_model.is_empty()
            && !options
                .iter()
                .any(|existing| existing.value.as_deref() == Some(env_custom_model.as_str()))
        {
            options.push(ModelOption {
                value: Some(env_custom_model.clone()),
                label: std::env::var("MOSSEN_CODE_CUSTOM_MODEL_OPTION_NAME")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| env_custom_model.clone()),
                description: std::env::var("MOSSEN_CODE_CUSTOM_MODEL_OPTION_DESCRIPTION")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        localized_text(
                            &format!("Custom model ({})", env_custom_model),
                            &format!("自定义模型（{}）", env_custom_model),
                        )
                    }),
                description_for_model: None,
            });
        }
    }

    for opt in additional_model_options() {
        if !options
            .iter()
            .any(|existing| existing.value == opt.value)
        {
            options.push(opt);
        }
    }

    let mut custom_model: ModelSetting = None;
    let current_main_loop_model = get_user_specified_model_setting().and_then(|x| x);
    let initial_main_loop_model = initial_main_loop_model();
    if let Some(m) = current_main_loop_model {
        custom_model = Some(m);
    } else if let Some(m) = initial_main_loop_model {
        custom_model = Some(m);
    }

    let custom_model = match custom_model {
        None => return filter_model_options_by_allowlist(options),
        Some(m) => m,
    };

    if options
        .iter()
        .any(|opt| opt.value.as_deref() == Some(custom_model.as_str()))
    {
        return filter_model_options_by_allowlist(options);
    }

    if custom_model == "maxplan" {
        let mut new_options = options;
        new_options.push(get_max_plan_option());
        return filter_model_options_by_allowlist(new_options);
    }
    if custom_model == "max" && !uses_third_party_model_surface() {
        let mut new_options = options;
        new_options.push(get_max_max_option(fast_mode));
        return filter_model_options_by_allowlist(new_options);
    }
    if custom_model == "max[1m]" && !uses_third_party_model_surface() {
        let mut new_options = options;
        new_options.push(get_merged_max_1m_option(fast_mode));
        return filter_model_options_by_allowlist(new_options);
    }
    if let Some(known_option) = get_known_model_option(&custom_model) {
        options.push(known_option);
    } else {
        options.push(ModelOption {
            value: Some(custom_model.clone()),
            label: custom_model.clone(),
            description: localized_text("Custom model", "自定义模型"),
            description_for_model: None,
        });
    }
    filter_model_options_by_allowlist(options)
}

fn filter_model_options_by_allowlist(options: Vec<ModelOption>) -> Vec<ModelOption> {
    let settings = get_session_settings_cache();
    let has_available_models = settings
        .and_then(|s| s.settings.available_models)
        .is_some();
    if !has_available_models {
        return options;
    }
    options
        .into_iter()
        .filter(|opt| match &opt.value {
            None => true,
            Some(v) => is_model_allowed(v),
        })
        .collect()
}
