use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Advisor server tool use block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorServerToolUseBlock {
    #[serde(rename = "type")]
    pub block_type: String, // always "server_tool_use"
    pub id: String,
    pub name: String, // always "advisor"
    pub input: HashMap<String, serde_json::Value>,
}

/// Content variants for AdvisorToolResultBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AdvisorToolResultContent {
    #[serde(rename = "advisor_result")]
    Result { text: String },
    #[serde(rename = "advisor_redacted_result")]
    RedactedResult { encrypted_content: String },
    #[serde(rename = "advisor_tool_result_error")]
    Error { error_code: String },
}

/// Advisor tool result block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorToolResultBlock {
    #[serde(rename = "type")]
    pub block_type: String, // always "advisor_tool_result"
    pub tool_use_id: String,
    pub content: AdvisorToolResultContent,
}

/// Combined advisor block enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdvisorBlock {
    ServerToolUse(AdvisorServerToolUseBlock),
    ToolResult(AdvisorToolResultBlock),
}

/// Check if a block is an advisor block
pub fn is_advisor_block(block_type: &str, name: Option<&str>) -> bool {
    block_type == "advisor_tool_result"
        || (block_type == "server_tool_use" && name == Some("advisor"))
}

/// Advisor configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvisorConfig {
    pub enabled: Option<bool>,
    pub can_user_configure: Option<bool>,
    pub base_model: Option<String>,
    pub advisor_model: Option<String>,
}

/// Check if advisor is enabled
pub fn is_advisor_enabled(
    disable_env: Option<&str>,
    should_include_first_party_only_betas: bool,
    config: &AdvisorConfig,
) -> bool {
    if let Some(val) = disable_env {
        if is_env_truthy(val) {
            return false;
        }
    }
    if !should_include_first_party_only_betas {
        return false;
    }
    config.enabled.unwrap_or(false)
}

/// Check if user can configure advisor
pub fn can_user_configure_advisor(advisor_enabled: bool, config: &AdvisorConfig) -> bool {
    advisor_enabled && config.can_user_configure.unwrap_or(false)
}

/// Get experiment advisor models
pub fn get_experiment_advisor_models(
    advisor_enabled: bool,
    can_user_configure: bool,
    config: &AdvisorConfig,
) -> Option<(String, String)> {
    if advisor_enabled && !can_user_configure {
        if let (Some(base), Some(advisor)) = (&config.base_model, &config.advisor_model) {
            return Some((base.clone(), advisor.clone()));
        }
    }
    None
}

/// Check whether the main loop model supports calling the advisor tool
pub fn model_supports_advisor(model: &str, user_type: Option<&str>) -> bool {
    let m = model.to_lowercase();
    m.contains("max-4-6") || m.contains("balanced-4-6") || user_type == Some("internal")
}

/// Check if a model is valid as an advisor model
pub fn is_valid_advisor_model(model: &str, user_type: Option<&str>) -> bool {
    let m = model.to_lowercase();
    m.contains("max-4-6") || m.contains("balanced-4-6") || user_type == Some("internal")
}

/// Get advisor usage from API usage response
pub fn get_advisor_usage(iterations: Option<&[serde_json::Value]>) -> Vec<serde_json::Value> {
    match iterations {
        Some(iters) => iters
            .iter()
            .filter(|it| {
                it.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "advisor_message")
                    .unwrap_or(false)
            })
            .cloned()
            .collect(),
        None => Vec::new(),
    }
}

/// 对应 TS `getInitialAdvisorSetting`：在 advisor 开启时返回初始设置中的模型名。
///
/// Rust 端尚未集中暴露 `getInitialSettings`，因此读取环境变量
/// `MOSSEN_INITIAL_ADVISOR_MODEL`（在 init 阶段由 settings 写入）。返回 `None`
/// 表示用户未设置或 advisor 未启用。
pub fn get_initial_advisor_setting(advisor_enabled: bool) -> Option<String> {
    if !advisor_enabled {
        return None;
    }
    std::env::var("MOSSEN_INITIAL_ADVISOR_MODEL").ok()
}

/// Advisor tool instructions constant
pub const ADVISOR_TOOL_INSTRUCTIONS: &str = r#"# Advisor Tool

You have access to an `advisor` tool backed by a stronger reviewer model. It takes NO parameters -- when you call it, your entire conversation history is automatically forwarded. The advisor sees the task, every tool call you've made, every result you've seen.

Call advisor BEFORE substantive work -- before writing code, before committing to an interpretation, before building on an assumption. If the task requires orientation first (finding files, reading code, seeing what's there), do that, then call advisor. Orientation is not substantive work. Writing, editing, and declaring an answer are.

Also call advisor:
- When you believe the task is complete. BEFORE this call, make your deliverable durable: write the file, stage the change, save the result. The advisor call takes time; if the session ends during it, a durable result persists and an unwritten one doesn't.
- When stuck -- errors recurring, approach not converging, results that don't fit.
- When considering a change of approach.

On tasks longer than a few steps, call advisor at least once before committing to an approach and once before declaring done. On short reactive tasks where the next action is dictated by tool output you just read, you don't need to keep calling -- the advisor adds most of its value on the first call, before the approach crystallizes.

Give the advice serious weight. If you follow a step and it fails empirically, or you have primary-source evidence that contradicts a specific claim (the file says X, the code does Y), adapt. A passing self-test is not evidence the advice is wrong -- it's evidence your test doesn't check what the advice is checking.

If you've already retrieved data pointing one way and the advisor points another: don't silently switch. Surface the conflict in one more advisor call -- "I found X, you suggest Y, which constraint breaks the tie?" The advisor saw your evidence but may have underweighted it; a reconcile call is cheaper than committing to the wrong branch."#;

fn is_env_truthy(val: &str) -> bool {
    matches!(val.to_lowercase().as_str(), "1" | "true" | "yes")
}
