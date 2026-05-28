//! Sub-agent model resolution.
//!
//! Direct translation of `utils/model/agent.ts`.

use mossen_types::permissions::PermissionMode;

use crate::custom_backend::{get_custom_backend_model, is_custom_backend_enabled};
use crate::string_utils::capitalize;

use super::aliases::{ModelAlias, MODEL_ALIASES};
use super::bedrock::{apply_bedrock_region_prefix, get_bedrock_region_prefix};
use super::model::{
    get_canonical_name, get_runtime_main_loop_model, parse_user_specified_model,
    RuntimeMainLoopModelParams,
};
use super::providers::{get_api_provider, APIProvider};

/// `AGENT_MODEL_OPTIONS` — `MODEL_ALIASES` plus the special `"inherit"` value.
pub static AGENT_MODEL_OPTIONS: once_cell::sync::Lazy<Vec<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        let mut v: Vec<&'static str> = MODEL_ALIASES.to_vec();
        v.push("inherit");
        v
    });

/// 对应 TS `AgentModelAlias` 字面量联合类型。Rust 端用字符串别名表示。
pub type AgentModelAlias = String;

#[derive(Debug, Clone)]
pub struct AgentModelOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

/// Get the default subagent model. Subagents inherit from their parent.
pub fn get_default_subagent_model() -> String {
    "inherit".to_string()
}

fn permission_mode_to_str(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::BypassPermissions => "bypassPermissions",
        PermissionMode::Default => "default",
        PermissionMode::DontAsk => "dontAsk",
        PermissionMode::Plan => "plan",
        PermissionMode::Auto => "auto",
        PermissionMode::Bubble => "bubble",
    }
}

/// Get the effective model string for an agent. Mirrors TS `getAgentModel`.
pub fn get_agent_model(
    agent_model: Option<&str>,
    parent_model: &str,
    tool_specified_model: Option<ModelAlias>,
    permission_mode: Option<PermissionMode>,
) -> String {
    if let Ok(env_model) = std::env::var("MOSSEN_CODE_SUBAGENT_MODEL") {
        if !env_model.is_empty() {
            return parse_user_specified_model(&env_model);
        }
    }

    if is_custom_backend_enabled() {
        return get_custom_backend_model().unwrap_or_else(|| parent_model.to_string());
    }

    let parent_region_prefix = get_bedrock_region_prefix(parent_model);

    let apply_parent_region_prefix = |resolved_model: String, original_spec: &str| -> String {
        if let Some(prefix) = parent_region_prefix {
            if get_api_provider() == APIProvider::Bedrock {
                if get_bedrock_region_prefix(original_spec).is_some() {
                    return resolved_model;
                }
                return apply_bedrock_region_prefix(&resolved_model, prefix);
            }
        }
        resolved_model
    };

    if let Some(tool_alias) = tool_specified_model {
        let alias_str = tool_alias.as_str();
        if alias_matches_parent_tier(alias_str, parent_model) {
            return parent_model.to_string();
        }
        let model = parse_user_specified_model(alias_str);
        return apply_parent_region_prefix(model, alias_str);
    }

    let agent_model_with_exp = match agent_model {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => get_default_subagent_model(),
    };

    if agent_model_with_exp == "inherit" {
        return get_runtime_main_loop_model(RuntimeMainLoopModelParams {
            permission_mode: permission_mode_to_str(
                permission_mode.unwrap_or(PermissionMode::Default),
            ),
            main_loop_model: parent_model,
            exceeds_200k_tokens: false,
        });
    }

    if alias_matches_parent_tier(&agent_model_with_exp, parent_model) {
        return parent_model.to_string();
    }
    let model = parse_user_specified_model(&agent_model_with_exp);
    apply_parent_region_prefix(model, &agent_model_with_exp)
}

fn alias_matches_parent_tier(alias: &str, parent_model: &str) -> bool {
    let canonical = get_canonical_name(parent_model);
    match alias.to_lowercase().as_str() {
        "max" => canonical.contains("max"),
        "balanced" => canonical.contains("balanced"),
        "fast" => canonical.contains("fast"),
        _ => false,
    }
}

pub fn get_agent_model_display(model: Option<&str>) -> String {
    let model = match model {
        None => return "Inherit from parent (default)".to_string(),
        Some(m) => m,
    };
    match model {
        "inherit" => "Inherit from parent".to_string(),
        "balanced" => "Mossen Balanced".to_string(),
        "max" => "Mossen Max".to_string(),
        "fast" => "Mossen Fast".to_string(),
        other => capitalize(other),
    }
}

/// Get available model options for agents.
pub fn get_agent_model_options() -> Vec<AgentModelOption> {
    vec![
        AgentModelOption {
            value: "balanced".to_string(),
            label: "Mossen Balanced".to_string(),
            description: "Balanced performance - best for most agents".to_string(),
        },
        AgentModelOption {
            value: "max".to_string(),
            label: "Mossen Max".to_string(),
            description: "Most capable for complex reasoning tasks".to_string(),
        },
        AgentModelOption {
            value: "fast".to_string(),
            label: "Mossen Fast".to_string(),
            description: "Fast and efficient for simple tasks".to_string(),
        },
        AgentModelOption {
            value: "inherit".to_string(),
            label: "Inherit from parent".to_string(),
            description: "Use the same model as the main conversation".to_string(),
        },
    ]
}
