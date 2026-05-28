//! Permission prompt tool result schema and conversion.
//!
//! Handles validation and conversion of permission prompt tool results
//! into permission decisions.

use std::collections::HashMap;

use super::permission_result::{
    PermissionAllowDecision, PermissionDecision, PermissionDecisionReason, PermissionDenyDecision,
    PermissionUpdate,
};

/// Input schema for permission prompt tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionPromptInput {
    pub tool_name: String,
    pub input: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

/// Decision classification from SDK hosts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionClassification {
    UserTemporary,
    UserPermanent,
    UserReject,
}

/// Allow result from permission prompt tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAllowResult {
    pub behavior: String, // "allow"
    pub updated_input: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<Vec<PermissionUpdate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_classification: Option<DecisionClassification>,
}

/// Deny result from permission prompt tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDenyResult {
    pub behavior: String, // "deny"
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_classification: Option<DecisionClassification>,
}

/// Output from permission prompt tool (union of allow/deny).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionPromptOutput {
    #[serde(rename = "allow")]
    Allow(PermissionAllowResult),
    #[serde(rename = "deny")]
    Deny(PermissionDenyResult),
}

/// Normalizes the result of a permission prompt tool to a PermissionDecision.
///
/// Returns the decision and optionally a list of permission updates to apply
/// to the context (caller must apply them).
pub fn permission_prompt_tool_result_to_permission_decision(
    result: &PermissionPromptOutput,
    tool_name: &str,
    input: &HashMap<String, serde_json::Value>,
) -> (PermissionDecision, Option<Vec<PermissionUpdate>>, bool) {
    let decision_reason = PermissionDecisionReason::PermissionPromptTool {
        permission_prompt_tool_name: tool_name.to_string(),
        tool_result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    };

    match result {
        PermissionPromptOutput::Allow(allow_result) => {
            let updated_permissions = allow_result.updated_permissions.clone();

            // Mobile clients responding from a push notification don't have the
            // original tool input, so they send `{}` to satisfy the schema. Treat an
            // empty object as "use original" so the tool doesn't run with no args.
            let updated_input = if allow_result.updated_input.is_empty() {
                Some(input.clone())
            } else {
                Some(allow_result.updated_input.clone())
            };

            let decision = PermissionDecision::Allow(PermissionAllowDecision {
                updated_input,
                decision_reason: Some(decision_reason),
                tool_use_id: allow_result.tool_use_id.clone(),
            });

            (decision, updated_permissions, false)
        }
        PermissionPromptOutput::Deny(deny_result) => {
            let should_abort = deny_result.interrupt.unwrap_or(false);

            let decision = PermissionDecision::Deny(PermissionDenyDecision {
                message: deny_result.message.clone(),
                decision_reason,
                tool_use_id: deny_result.tool_use_id.clone(),
            });

            (decision, None, should_abort)
        }
    }
}
