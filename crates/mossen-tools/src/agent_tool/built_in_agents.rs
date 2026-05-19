//! # built_in_agents — Built-in agent registry
//!
//! Translates `tools/AgentTool/builtInAgents.ts`.
//! Provides access to the list of built-in agents and feature gates.

use std::env;

use super::built_in::{
    explore_agent, general_purpose_agent, mossen_code_guide_agent, plan_agent,
    statusline_setup_agent, verification_agent,
};
use super::load_agents_dir::AgentDefinition;

/// Check if a feature is enabled via environment variable.
fn is_feature_enabled(feature_name: &str) -> bool {
    env::var(format!("MOSSEN_FEATURE_{}", feature_name))
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Check if an environment variable is truthy.
fn is_env_truthy(val: Option<&str>) -> bool {
    val.map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Check if the current session is non-interactive (SDK/API usage).
fn get_is_non_interactive_session() -> bool {
    is_env_truthy(env::var("MOSSEN_NON_INTERACTIVE").ok().as_deref())
}

/// Whether explore and plan agents are enabled.
pub fn are_explore_plan_agents_enabled() -> bool {
    if is_feature_enabled("BUILTIN_EXPLORE_PLAN_AGENTS") {
        // Default: true — A/B test treatment sets false to measure impact of removal.
        return true;
    }
    false
}

/// Get the list of built-in agents available in the current session.
pub fn get_built_in_agents() -> Vec<AgentDefinition> {
    // Allow disabling all built-in agents via env var (useful for SDK users who want a blank slate)
    // Only applies in noninteractive mode (SDK/API usage)
    if is_env_truthy(
        env::var("MOSSEN_AGENT_SDK_DISABLE_BUILTIN_AGENTS")
            .ok()
            .as_deref(),
    ) && get_is_non_interactive_session()
    {
        return Vec::new();
    }

    // Check coordinator mode
    if is_feature_enabled("COORDINATOR_MODE")
        && is_env_truthy(env::var("MOSSEN_CODE_COORDINATOR_MODE").ok().as_deref())
    {
        // In coordinator mode, return coordinator-specific agents
        // (would be loaded from coordinator module)
        return Vec::new();
    }

    let mut agents = vec![
        general_purpose_agent::definition(),
        statusline_setup_agent::definition(),
    ];

    if are_explore_plan_agents_enabled() {
        agents.push(explore_agent::definition());
        agents.push(plan_agent::definition());
    }

    // Include Code Guide agent for non-SDK entrypoints
    let entrypoint = env::var("MOSSEN_CODE_ENTRYPOINT").unwrap_or_default();
    let is_non_sdk_entrypoint = entrypoint != "sdk-ts"
        && entrypoint != "sdk-py"
        && entrypoint != "sdk-cli";

    if is_non_sdk_entrypoint {
        agents.push(mossen_code_guide_agent::definition());
    }

    // Verification agent (feature-gated)
    if is_feature_enabled("VERIFICATION_AGENT") {
        agents.push(verification_agent::definition());
    }

    agents
}
