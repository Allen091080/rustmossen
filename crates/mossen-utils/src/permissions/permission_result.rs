//! Permission result types and helper functions.
//!
//! Re-exports core permission decision types and provides the
//! `get_rule_behavior_description` helper.

use std::collections::HashMap;

// ─── Permission Modes ───────────────────────────────────────────────────────

/// External-facing permission modes (user-addressable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPermissionMode {
    AcceptEdits,
    BypassPermissions,
    Default,
    DontAsk,
    Plan,
}

/// Internal permission mode union (superset of external).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
    Default,
    DontAsk,
    Plan,
    Auto,
    Bubble,
}

/// Permission behavior: allow, deny, or ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

// ─── Permission Rule Source ─────────────────────────────────────────────────

/// Where a permission rule originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRuleSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
    CliArg,
    Command,
    Session,
}

/// The value content of a permission rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRuleValue {
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

/// A complete permission rule with source, behavior, and value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub source: PermissionRuleSource,
    pub rule_behavior: PermissionBehavior,
    pub rule_value: PermissionRuleValue,
}

// ─── Permission Update Destination ──────────────────────────────────────────

/// Where a permission update should be persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    Session,
    CliArg,
}

// ─── Permission Update ──────────────────────────────────────────────────────

/// Update operations for permission configuration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionUpdate {
    #[serde(rename_all = "camelCase")]
    AddRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    #[serde(rename_all = "camelCase")]
    ReplaceRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    #[serde(rename_all = "camelCase")]
    RemoveRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    #[serde(rename_all = "camelCase")]
    SetMode {
        destination: PermissionUpdateDestination,
        mode: ExternalPermissionMode,
    },
    #[serde(rename_all = "camelCase")]
    AddDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    RemoveDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
}

// ─── Additional Working Directory ───────────────────────────────────────────

/// An additional directory included in permission scope.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdditionalWorkingDirectory {
    pub path: String,
    pub source: PermissionRuleSource,
}

// ─── Permission Decision Reason ─────────────────────────────────────────────

/// Explanation of why a permission decision was made.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionDecisionReason {
    Rule {
        rule: PermissionRule,
    },
    Mode {
        mode: PermissionMode,
    },
    SubcommandResults {
        reasons: HashMap<String, PermissionResult>,
    },
    PermissionPromptTool {
        permission_prompt_tool_name: String,
        tool_result: serde_json::Value,
    },
    Hook {
        hook_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hook_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    AsyncAgent {
        reason: String,
    },
    SandboxOverride {
        reason: String,
    },
    Classifier {
        classifier: String,
        reason: String,
    },
    WorkingDir {
        reason: String,
    },
    SafetyCheck {
        reason: String,
        classifier_approvable: bool,
    },
    Other {
        reason: String,
    },
}

// ─── Permission Metadata ────────────────────────────────────────────────────

/// Metadata attached to permission decisions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionCommandMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Optional metadata on a permission decision.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionMetadata {
    pub command: PermissionCommandMetadata,
}

// ─── Permission Decision Variants ───────────────────────────────────────────

/// Result when permission is granted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionAllowDecision {
    pub updated_input: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

/// Result when user should be prompted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionAskDecision {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<PermissionUpdate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PermissionMetadata>,
}

/// Result when permission is denied.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionDenyDecision {
    pub message: String,
    pub decision_reason: PermissionDecisionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

/// A permission decision: allow, ask, or deny.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
}

impl PermissionDecision {
    pub fn behavior(&self) -> PermissionBehavior {
        match self {
            Self::Allow(_) => PermissionBehavior::Allow,
            Self::Ask(_) => PermissionBehavior::Ask,
            Self::Deny(_) => PermissionBehavior::Deny,
        }
    }
}

/// Permission result with additional passthrough option.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionResult {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
    Passthrough {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestions: Option<Vec<PermissionUpdate>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
    },
}

// ─── Tool Permission Context ────────────────────────────────────────────────

/// Mapping of permission rules by their source.
pub type ToolPermissionRulesBySource = HashMap<PermissionRuleSource, Vec<String>>;

/// Context needed for permission checking in tools.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    pub additional_working_directories: HashMap<String, AdditionalWorkingDirectory>,
    pub always_allow_rules: ToolPermissionRulesBySource,
    pub always_deny_rules: ToolPermissionRulesBySource,
    pub always_ask_rules: ToolPermissionRulesBySource,
    pub is_bypass_permissions_mode_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stripped_dangerous_rules: Option<ToolPermissionRulesBySource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should_avoid_permission_prompts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub await_automated_checks_before_dialog: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_plan_mode: Option<PermissionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_auto_mode_available: Option<bool>,
}

// ─── Helper Function ────────────────────────────────────────────────────────

/// Get the appropriate prose description for rule behavior.
pub fn get_rule_behavior_description(behavior: &PermissionBehavior) -> &'static str {
    match behavior {
        PermissionBehavior::Allow => "allowed",
        PermissionBehavior::Deny => "denied",
        PermissionBehavior::Ask => "asked for confirmation for",
    }
}
