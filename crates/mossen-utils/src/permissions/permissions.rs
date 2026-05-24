//! Main permission checking logic.
//!
//! Translates `utils/permissions/permissions.ts` — the primary permission pipeline
//! including `hasPermissionsToUseTool`, rule-based checks, auto-mode classifier
//! integration, denial tracking, and permission rule CRUD operations.

use std::collections::{HashMap, HashSet};

use super::denial_tracking::{
    create_denial_tracking_state, record_denial, record_success, should_fallback_to_prompting,
    DenialLimits, DenialTrackingState,
};
use super::permission_result::{
    ExternalPermissionMode, PermissionAllowDecision, PermissionAskDecision, PermissionBehavior,
    PermissionDecision, PermissionDecisionReason, PermissionDenyDecision, PermissionMode,
    PermissionResult, PermissionRule, PermissionRuleSource, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination, ToolPermissionContext,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Classifier fail-closed refresh interval (30 minutes).
const CLASSIFIER_FAIL_CLOSED_REFRESH_MS: u64 = 30 * 60 * 1000;

/// All permission rule sources in priority order.
pub const PERMISSION_RULE_SOURCES: &[PermissionRuleSource] = &[
    PermissionRuleSource::UserSettings,
    PermissionRuleSource::ProjectSettings,
    PermissionRuleSource::LocalSettings,
    PermissionRuleSource::FlagSettings,
    PermissionRuleSource::PolicySettings,
    PermissionRuleSource::CliArg,
    PermissionRuleSource::Command,
    PermissionRuleSource::Session,
];

pub const BASH_TOOL_NAME: &str = "Bash";
pub const POWERSHELL_TOOL_NAME: &str = "PowerShell";
pub const AGENT_TOOL_NAME: &str = "Agent";
pub const REPL_TOOL_NAME: &str = "REPL";

// ─── Rule Retrieval ──────────────────────────────────────────────────────────

/// Get display string for a permission rule source.
pub fn permission_rule_source_display_string(source: PermissionRuleSource) -> &'static str {
    match source {
        PermissionRuleSource::UserSettings => "user settings",
        PermissionRuleSource::ProjectSettings => "project settings",
        PermissionRuleSource::LocalSettings => "local settings",
        PermissionRuleSource::FlagSettings => "flag settings",
        PermissionRuleSource::PolicySettings => "policy settings",
        PermissionRuleSource::CliArg => "CLI argument",
        PermissionRuleSource::Command => "command",
        PermissionRuleSource::Session => "session",
    }
}

/// Parse a rule string into a PermissionRuleValue.
pub fn permission_rule_value_from_string(rule_string: &str) -> PermissionRuleValue {
    if let Some(colon_idx) = rule_string.find(':') {
        PermissionRuleValue {
            tool_name: rule_string[..colon_idx].to_string(),
            rule_content: Some(rule_string[colon_idx + 1..].to_string()),
        }
    } else {
        PermissionRuleValue {
            tool_name: rule_string.to_string(),
            rule_content: None,
        }
    }
}

/// Serialize a PermissionRuleValue back to string form.
pub fn permission_rule_value_to_string(value: &PermissionRuleValue) -> String {
    match &value.rule_content {
        Some(content) => format!("{}:{}", value.tool_name, content),
        None => value.tool_name.clone(),
    }
}

/// Get all allow rules from context.
pub fn get_allow_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        if let Some(rule_strings) = context.always_allow_rules.get(source) {
            for rule_string in rule_strings {
                rules.push(PermissionRule {
                    source: *source,
                    rule_behavior: PermissionBehavior::Allow,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

/// Get all deny rules from context.
pub fn get_deny_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        if let Some(rule_strings) = context.always_deny_rules.get(source) {
            for rule_string in rule_strings {
                rules.push(PermissionRule {
                    source: *source,
                    rule_behavior: PermissionBehavior::Deny,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

/// Get all ask rules from context.
pub fn get_ask_rules(context: &ToolPermissionContext) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    for source in PERMISSION_RULE_SOURCES {
        if let Some(rule_strings) = context.always_ask_rules.get(source) {
            for rule_string in rule_strings {
                rules.push(PermissionRule {
                    source: *source,
                    rule_behavior: PermissionBehavior::Ask,
                    rule_value: permission_rule_value_from_string(rule_string),
                });
            }
        }
    }
    rules
}

// ─── Tool Matching ───────────────────────────────────────────────────────────

/// Info extracted from an MCP tool string.
pub struct McpInfo {
    pub server_name: String,
    pub tool_name: Option<String>,
}

/// Parse MCP info from a tool name string like "mcp__server__tool".
pub fn mcp_info_from_string(name: &str) -> Option<McpInfo> {
    if !name.starts_with("mcp__") {
        return None;
    }
    let rest = &name[5..]; // after "mcp__"
    if let Some(idx) = rest.find("__") {
        Some(McpInfo {
            server_name: rest[..idx].to_string(),
            tool_name: Some(rest[idx + 2..].to_string()),
        })
    } else {
        Some(McpInfo {
            server_name: rest.to_string(),
            tool_name: None,
        })
    }
}

/// Get the tool name used for permission check (handles MCP prefix).
pub fn get_tool_name_for_permission_check(tool_name: &str, mcp_info: Option<&McpInfo>) -> String {
    // If tool has MCP info, use the fully qualified name
    if mcp_info.is_some() && tool_name.starts_with("mcp__") {
        return tool_name.to_string();
    }
    tool_name.to_string()
}

/// Check if the entire tool matches a rule (no content = whole tool).
fn tool_matches_rule(tool_name: &str, rule: &PermissionRule) -> bool {
    if rule.rule_value.rule_content.is_some() {
        return false;
    }
    let name_for_match = tool_name;

    // Direct match
    if rule.rule_value.tool_name == name_for_match {
        return true;
    }

    // MCP server-level match
    let rule_info = mcp_info_from_string(&rule.rule_value.tool_name);
    let tool_info = mcp_info_from_string(name_for_match);

    if let (Some(ri), Some(ti)) = (rule_info, tool_info) {
        if (ri.tool_name.is_none() || ri.tool_name.as_deref() == Some("*"))
            && ri.server_name == ti.server_name
        {
            return true;
        }
    }
    false
}

/// Check if the entire tool is in the always-allow rules.
pub fn tool_always_allowed_rule(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_allow_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Get the deny rule for a tool (whole-tool deny).
pub fn get_deny_rule_for_tool(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_deny_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Get the ask rule for a tool (whole-tool ask).
pub fn get_ask_rule_for_tool(
    context: &ToolPermissionContext,
    tool_name: &str,
) -> Option<PermissionRule> {
    get_ask_rules(context)
        .into_iter()
        .find(|rule| tool_matches_rule(tool_name, rule))
}

/// Check if a specific agent is denied via Agent(agentType) syntax.
pub fn get_deny_rule_for_agent(
    context: &ToolPermissionContext,
    agent_tool_name: &str,
    agent_type: &str,
) -> Option<PermissionRule> {
    get_deny_rules(context).into_iter().find(|rule| {
        rule.rule_value.tool_name == agent_tool_name
            && rule.rule_value.rule_content.as_deref() == Some(agent_type)
    })
}

/// Filter agents to exclude those denied by rules.
pub fn filter_denied_agents(
    agent_types: &[String],
    context: &ToolPermissionContext,
    agent_tool_name: &str,
) -> Vec<String> {
    let deny_rules = get_deny_rules(context);
    let denied: HashSet<&str> = deny_rules
        .iter()
        .filter(|rule| rule.rule_value.tool_name == agent_tool_name)
        .filter_map(|rule| rule.rule_value.rule_content.as_deref())
        .collect();
    agent_types
        .iter()
        .filter(|at| !denied.contains(at.as_str()))
        .cloned()
        .collect()
}

/// Map of rule contents to associated rule for a given tool name and behavior.
pub fn get_rule_by_contents_for_tool_name(
    context: &ToolPermissionContext,
    tool_name: &str,
    behavior: &str,
) -> HashMap<String, PermissionRule> {
    let rules = match behavior {
        "allow" => get_allow_rules(context),
        "deny" => get_deny_rules(context),
        "ask" => get_ask_rules(context),
        _ => Vec::new(),
    };

    let perm_behavior = match behavior {
        "allow" => PermissionBehavior::Allow,
        "deny" => PermissionBehavior::Deny,
        _ => PermissionBehavior::Ask,
    };

    let mut result = HashMap::new();
    for rule in rules {
        if rule.rule_value.tool_name == tool_name
            && rule.rule_value.rule_content.is_some()
            && rule.rule_behavior == perm_behavior
        {
            if let Some(ref content) = rule.rule_value.rule_content {
                result.insert(content.clone(), rule);
            }
        }
    }
    result
}

// ─── Permission Request Message ──────────────────────────────────────────────

/// Creates a human-readable permission request message.
pub fn create_permission_request_message(
    tool_name: &str,
    decision_reason: Option<&PermissionDecisionReason>,
) -> String {
    if let Some(reason) = decision_reason {
        match reason {
            PermissionDecisionReason::Classifier {
                classifier,
                reason: r,
            } => {
                return format!(
                    "Classifier '{}' requires approval for this {} command: {}",
                    classifier, tool_name, r
                );
            }
            PermissionDecisionReason::Hook {
                hook_name,
                reason: r,
                ..
            } => {
                return match r {
                    Some(msg) => format!("Hook '{}' blocked this action: {}", hook_name, msg),
                    None => format!(
                        "Hook '{}' requires approval for this {} command",
                        hook_name, tool_name
                    ),
                };
            }
            PermissionDecisionReason::Rule { rule } => {
                let rule_string = permission_rule_value_to_string(&rule.rule_value);
                let source_string = permission_rule_source_display_string(rule.source);
                return format!(
                    "Permission rule '{}' from {} requires approval for this {} command",
                    rule_string, source_string, tool_name
                );
            }
            PermissionDecisionReason::SubcommandResults { reasons } => {
                let needs_approval: Vec<&String> = reasons
                    .iter()
                    .filter(|(_, result)| {
                        matches!(
                            result,
                            PermissionResult::Ask(_) | PermissionResult::Passthrough { .. }
                        )
                    })
                    .map(|(cmd, _)| cmd)
                    .collect();
                if !needs_approval.is_empty() {
                    let n = needs_approval.len();
                    let parts = if n == 1 { "part" } else { "parts" };
                    let verb = if n == 1 { "requires" } else { "require" };
                    return format!(
                        "This {} command contains multiple operations. The following {} {} approval: {}",
                        tool_name,
                        parts,
                        verb,
                        needs_approval
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                return format!(
                    "This {} command contains multiple operations that require approval",
                    tool_name
                );
            }
            PermissionDecisionReason::PermissionPromptTool {
                permission_prompt_tool_name,
                ..
            } => {
                return format!(
                    "Tool '{}' requires approval for this {} command",
                    permission_prompt_tool_name, tool_name
                );
            }
            PermissionDecisionReason::SandboxOverride { .. } => {
                return "Run outside of the sandbox".to_string();
            }
            PermissionDecisionReason::WorkingDir { reason } => {
                return reason.clone();
            }
            PermissionDecisionReason::SafetyCheck { reason, .. } => {
                return reason.clone();
            }
            PermissionDecisionReason::Other { reason } => {
                return reason.clone();
            }
            PermissionDecisionReason::Mode { mode } => {
                let mode_title = permission_mode_title(*mode);
                return format!(
                    "Current permission mode ({}) requires approval for this {} command",
                    mode_title, tool_name
                );
            }
            PermissionDecisionReason::AsyncAgent { reason } => {
                return reason.clone();
            }
        }
    }

    format!(
        "Mossen requested permissions to use {}, but you haven't granted it yet.",
        tool_name
    )
}

fn permission_mode_title(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::AcceptEdits => "Accept Edits",
        PermissionMode::BypassPermissions => "Bypass Permissions",
        PermissionMode::Default => "Default",
        PermissionMode::DontAsk => "Don't Ask",
        PermissionMode::Plan => "Plan",
        PermissionMode::Auto => "Auto",
        PermissionMode::Bubble => "Bubble",
    }
}

// ─── Tool Use Context ────────────────────────────────────────────────────────

/// Context for tool-use permission checking.
pub struct ToolUseContext {
    pub tool_permission_context: ToolPermissionContext,
    pub denial_tracking: Option<DenialTrackingState>,
    pub local_denial_tracking: Option<DenialTrackingState>,
    pub should_avoid_permission_prompts: bool,
    pub aborted: bool,
    pub messages: Vec<serde_json::Value>,
    pub tools: Vec<String>,
}

/// Result from the YOLO classifier.
pub struct ClassifierResult {
    pub should_block: bool,
    pub unavailable: bool,
    pub reason: String,
    pub transcript_too_long: bool,
    pub model: Option<String>,
    pub duration_ms: Option<u64>,
    pub error_dump_path: Option<String>,
    pub usage: Option<ClassifierUsage>,
}

pub struct ClassifierUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

// ─── Main Permission Pipeline ────────────────────────────────────────────────

/// Configuration for the permission pipeline.
pub struct PermissionPipelineConfig {
    pub transcript_classifier_enabled: bool,
    pub powershell_auto_mode_enabled: bool,
    pub iron_gate_closed: bool,
    pub is_auto_mode_allowlisted_tool: Box<dyn Fn(&str) -> bool + Send + Sync>,
    pub classify_yolo_action: Box<
        dyn Fn(&[serde_json::Value], &str, &[String], &ToolPermissionContext) -> ClassifierResult
            + Send
            + Sync,
    >,
}

/// Main entry point: check if we have permissions to use a tool.
pub fn has_permissions_to_use_tool(
    tool_name: &str,
    input: &HashMap<String, serde_json::Value>,
    context: &ToolUseContext,
    tool_permission_result: &PermissionResult,
    config: &PermissionPipelineConfig,
) -> PermissionDecision {
    let inner_result =
        has_permissions_to_use_tool_inner(tool_name, input, context, tool_permission_result);

    // Reset denial tracking on allow
    if matches!(&inner_result, PermissionDecision::Allow(_)) {
        if config.transcript_classifier_enabled {
            if context.tool_permission_context.mode == PermissionMode::Auto {
                if let Some(ref state) = context.denial_tracking {
                    if state.consecutive_denials > 0 {
                        let _new_state = record_success(state);
                        // Caller is responsible for persisting
                    }
                }
            }
        }
        return inner_result;
    }

    // Apply dontAsk mode: ask → deny
    if let PermissionDecision::Ask(ref ask) = inner_result {
        if context.tool_permission_context.mode == PermissionMode::DontAsk {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!(
                    "Permission denied: {} requires approval, but permission mode is 'Don't Ask'.",
                    tool_name
                ),
                decision_reason: PermissionDecisionReason::Mode {
                    mode: PermissionMode::DontAsk,
                },
                tool_use_id: None,
            });
        }

        // Auto mode classifier
        if config.transcript_classifier_enabled
            && (context.tool_permission_context.mode == PermissionMode::Auto
                || context.tool_permission_context.mode == PermissionMode::Plan)
        {
            // Non-classifier-approvable safety checks are bypass-immune
            if let Some(PermissionDecisionReason::SafetyCheck {
                classifier_approvable,
                reason,
                ..
            }) = &ask.decision_reason
            {
                if !classifier_approvable {
                    if context.should_avoid_permission_prompts {
                        return PermissionDecision::Deny(PermissionDenyDecision {
                            message: reason.clone(),
                            decision_reason: PermissionDecisionReason::AsyncAgent {
                                reason: "Safety check requires interactive approval".to_string(),
                            },
                            tool_use_id: None,
                        });
                    }
                    return inner_result;
                }
            }

            // PowerShell guard (unless POWERSHELL_AUTO_MODE enabled)
            if tool_name == POWERSHELL_TOOL_NAME && !config.powershell_auto_mode_enabled {
                if context.should_avoid_permission_prompts {
                    return PermissionDecision::Deny(PermissionDenyDecision {
                        message: "PowerShell tool requires interactive approval".to_string(),
                        decision_reason: PermissionDecisionReason::AsyncAgent {
                            reason: "PowerShell tool requires interactive approval".to_string(),
                        },
                        tool_use_id: None,
                    });
                }
                return inner_result;
            }

            let denial_state = context
                .local_denial_tracking
                .as_ref()
                .or(context.denial_tracking.as_ref())
                .cloned()
                .unwrap_or_else(create_denial_tracking_state);

            // Allowlisted tools skip classifier
            if (config.is_auto_mode_allowlisted_tool)(tool_name) {
                let _new_state = record_success(&denial_state);
                return PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: Some(input.clone()),
                    decision_reason: Some(PermissionDecisionReason::Mode {
                        mode: PermissionMode::Auto,
                    }),
                    tool_use_id: None,
                });
            }

            // Run classifier
            let action = format_action_for_classifier(tool_name, input);
            let classifier_result = (config.classify_yolo_action)(
                &context.messages,
                &action,
                &context.tools,
                &context.tool_permission_context,
            );

            if classifier_result.should_block {
                if classifier_result.transcript_too_long {
                    if context.should_avoid_permission_prompts {
                        return PermissionDecision::Deny(PermissionDenyDecision {
                            message: "Auto mode classifier transcript exceeded context window"
                                .to_string(),
                            decision_reason: PermissionDecisionReason::Other {
                                reason: "Transcript too long for classifier".to_string(),
                            },
                            tool_use_id: None,
                        });
                    }
                    return inner_result;
                }

                if classifier_result.unavailable {
                    if config.iron_gate_closed {
                        return PermissionDecision::Deny(PermissionDenyDecision {
                            message: format!(
                                "Auto mode classifier unavailable. Please retry or approve manually."
                            ),
                            decision_reason: PermissionDecisionReason::Classifier {
                                classifier: "auto-mode".to_string(),
                                reason: "Classifier unavailable".to_string(),
                            },
                            tool_use_id: None,
                        });
                    }
                    return inner_result;
                }

                // Track denial
                let new_denial_state = record_denial(&denial_state);

                // Check denial limits
                if should_fallback_to_prompting(&new_denial_state) {
                    if context.should_avoid_permission_prompts {
                        return PermissionDecision::Deny(PermissionDenyDecision {
                            message: "Too many classifier denials".to_string(),
                            decision_reason: PermissionDecisionReason::Classifier {
                                classifier: "auto-mode".to_string(),
                                reason: "Denial limit exceeded".to_string(),
                            },
                            tool_use_id: None,
                        });
                    }
                    let warning = if new_denial_state.total_denials >= DenialLimits::MAX_TOTAL {
                        format!(
                            "{} actions were blocked this session. Please review.",
                            new_denial_state.total_denials
                        )
                    } else {
                        format!(
                            "{} consecutive actions were blocked. Please review.",
                            new_denial_state.consecutive_denials
                        )
                    };
                    return PermissionDecision::Ask(PermissionAskDecision {
                        message: ask.message.clone(),
                        updated_input: ask.updated_input.clone(),
                        decision_reason: Some(PermissionDecisionReason::Classifier {
                            classifier: "auto-mode".to_string(),
                            reason: format!(
                                "{}\n\nLatest blocked action: {}",
                                warning, classifier_result.reason
                            ),
                        }),
                        suggestions: ask.suggestions.clone(),
                        blocked_path: ask.blocked_path.clone(),
                        metadata: ask.metadata.clone(),
                    });
                }

                return PermissionDecision::Deny(PermissionDenyDecision {
                    message: format!(
                        "Action blocked by auto mode classifier: {}",
                        classifier_result.reason
                    ),
                    decision_reason: PermissionDecisionReason::Classifier {
                        classifier: "auto-mode".to_string(),
                        reason: classifier_result.reason,
                    },
                    tool_use_id: None,
                });
            }

            // Classifier allowed
            let _new_state = record_success(&denial_state);
            return PermissionDecision::Allow(PermissionAllowDecision {
                updated_input: Some(input.clone()),
                decision_reason: Some(PermissionDecisionReason::Classifier {
                    classifier: "auto-mode".to_string(),
                    reason: classifier_result.reason,
                }),
                tool_use_id: None,
            });
        }

        // Headless/async agent: deny without prompts
        if context.should_avoid_permission_prompts {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!(
                    "Permission denied: {} requires approval, but prompts are not available.",
                    tool_name
                ),
                decision_reason: PermissionDecisionReason::AsyncAgent {
                    reason: "Permission prompts are not available in this context".to_string(),
                },
                tool_use_id: None,
            });
        }
    }

    inner_result
}

/// Inner permission checking (rule-based + mode-based).
fn has_permissions_to_use_tool_inner(
    tool_name: &str,
    input: &HashMap<String, serde_json::Value>,
    context: &ToolUseContext,
    tool_permission_result: &PermissionResult,
) -> PermissionDecision {
    // 1a. Entire tool denied
    if let Some(deny_rule) = get_deny_rule_for_tool(&context.tool_permission_context, tool_name) {
        return PermissionDecision::Deny(PermissionDenyDecision {
            message: format!("Permission to use {} has been denied.", tool_name),
            decision_reason: PermissionDecisionReason::Rule { rule: deny_rule },
            tool_use_id: None,
        });
    }

    // 1b. Entire tool has ask rule
    if let Some(ask_rule) = get_ask_rule_for_tool(&context.tool_permission_context, tool_name) {
        return PermissionDecision::Ask(PermissionAskDecision {
            message: create_permission_request_message(tool_name, None),
            updated_input: None,
            decision_reason: Some(PermissionDecisionReason::Rule { rule: ask_rule }),
            suggestions: None,
            blocked_path: None,
            metadata: None,
        });
    }

    // 1c-1g. Process tool permission result
    match tool_permission_result {
        PermissionResult::Deny(d) => {
            return PermissionDecision::Deny(d.clone());
        }
        PermissionResult::Ask(a) => {
            // 1f. Content-specific ask rules
            if let Some(PermissionDecisionReason::Rule { ref rule }) = a.decision_reason {
                if rule.rule_behavior == PermissionBehavior::Ask {
                    return PermissionDecision::Ask(a.clone());
                }
            }
            // 1g. Safety checks are bypass-immune
            if let Some(PermissionDecisionReason::SafetyCheck { .. }) = &a.decision_reason {
                return PermissionDecision::Ask(a.clone());
            }
        }
        _ => {}
    }

    // 2a. Bypass permissions mode
    let should_bypass = context.tool_permission_context.mode == PermissionMode::BypassPermissions
        || (context.tool_permission_context.mode == PermissionMode::Plan
            && context
                .tool_permission_context
                .is_bypass_permissions_mode_available);
    if should_bypass {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(get_updated_input_or_fallback(tool_permission_result, input)),
            decision_reason: Some(PermissionDecisionReason::Mode {
                mode: context.tool_permission_context.mode,
            }),
            tool_use_id: None,
        });
    }

    // 2b. Entire tool always allowed
    if let Some(allow_rule) = tool_always_allowed_rule(&context.tool_permission_context, tool_name)
    {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(get_updated_input_or_fallback(tool_permission_result, input)),
            decision_reason: Some(PermissionDecisionReason::Rule { rule: allow_rule }),
            tool_use_id: None,
        });
    }

    // 3. Convert passthrough to ask
    match tool_permission_result {
        PermissionResult::Passthrough {
            message: _,
            decision_reason,
            suggestions,
            blocked_path,
        } => PermissionDecision::Ask(PermissionAskDecision {
            message: create_permission_request_message(tool_name, decision_reason.as_ref()),
            updated_input: None,
            decision_reason: decision_reason.clone(),
            suggestions: suggestions.clone(),
            blocked_path: blocked_path.clone(),
            metadata: None,
        }),
        PermissionResult::Allow(a) => PermissionDecision::Allow(a.clone()),
        PermissionResult::Ask(a) => PermissionDecision::Ask(a.clone()),
        PermissionResult::Deny(d) => PermissionDecision::Deny(d.clone()),
    }
}

// ─── Rule-Based Permission Check ─────────────────────────────────────────────

/// Check only rule-based permissions (subset used by bypassPermissions mode).
pub fn check_rule_based_permissions(
    tool_name: &str,
    tool_permission_context: &ToolPermissionContext,
    tool_permission_result: &PermissionResult,
) -> Option<PermissionDecision> {
    // 1a. Entire tool denied
    if let Some(deny_rule) = get_deny_rule_for_tool(tool_permission_context, tool_name) {
        return Some(PermissionDecision::Deny(PermissionDenyDecision {
            message: format!("Permission to use {} has been denied.", tool_name),
            decision_reason: PermissionDecisionReason::Rule { rule: deny_rule },
            tool_use_id: None,
        }));
    }

    // 1b. Entire tool has ask rule
    if let Some(ask_rule) = get_ask_rule_for_tool(tool_permission_context, tool_name) {
        return Some(PermissionDecision::Ask(PermissionAskDecision {
            message: create_permission_request_message(tool_name, None),
            updated_input: None,
            decision_reason: Some(PermissionDecisionReason::Rule { rule: ask_rule }),
            suggestions: None,
            blocked_path: None,
            metadata: None,
        }));
    }

    // 1c/1d. Tool denied
    if let PermissionResult::Deny(d) = tool_permission_result {
        return Some(PermissionDecision::Deny(d.clone()));
    }

    // 1f. Content-specific ask rules
    if let PermissionResult::Ask(a) = tool_permission_result {
        if let Some(PermissionDecisionReason::Rule { ref rule }) = a.decision_reason {
            if rule.rule_behavior == PermissionBehavior::Ask {
                return Some(PermissionDecision::Ask(a.clone()));
            }
        }
        // 1g. Safety checks
        if let Some(PermissionDecisionReason::SafetyCheck { .. }) = &a.decision_reason {
            return Some(PermissionDecision::Ask(a.clone()));
        }
    }

    None
}

// ─── Permission Rule CRUD ────────────────────────────────────────────────────

/// Delete a permission rule from context.
pub fn delete_permission_rule(
    context: &ToolPermissionContext,
    rule: &PermissionRule,
) -> Result<ToolPermissionContext, String> {
    if matches!(
        rule.source,
        PermissionRuleSource::PolicySettings
            | PermissionRuleSource::FlagSettings
            | PermissionRuleSource::Command
    ) {
        return Err("Cannot delete permission rules from read-only settings".to_string());
    }

    let destination = source_to_destination(rule.source)?;
    Ok(apply_permission_update(
        context,
        &PermissionUpdate::RemoveRules {
            destination,
            rules: vec![rule.rule_value.clone()],
            behavior: rule.rule_behavior,
        },
    ))
}

/// Convert rules to permission updates grouped by source+behavior.
pub fn convert_rules_to_updates(
    rules: &[PermissionRule],
    update_type: &str,
) -> Vec<PermissionUpdate> {
    let mut grouped: HashMap<String, Vec<PermissionRuleValue>> = HashMap::new();
    for rule in rules {
        let key = format!("{:?}:{:?}", rule.source, rule.rule_behavior);
        grouped
            .entry(key.clone())
            .or_default()
            .push(rule.rule_value.clone());
    }

    let mut updates = Vec::new();
    for (key, rule_values) in grouped {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let source = parse_source(parts[0]);
        let behavior = parse_behavior(parts[1]);
        if let (Some(src), Some(beh)) = (source, behavior) {
            if let Ok(dest) = source_to_destination(src) {
                let update = match update_type {
                    "addRules" => PermissionUpdate::AddRules {
                        destination: dest,
                        rules: rule_values,
                        behavior: beh,
                    },
                    _ => PermissionUpdate::ReplaceRules {
                        destination: dest,
                        rules: rule_values,
                        behavior: beh,
                    },
                };
                updates.push(update);
            }
        }
    }
    updates
}

/// Apply permission rules to context (additive).
pub fn apply_permission_rules_to_context(
    context: &ToolPermissionContext,
    rules: &[PermissionRule],
) -> ToolPermissionContext {
    let updates = convert_rules_to_updates(rules, "addRules");
    apply_permission_updates(context, &updates)
}

/// Sync permission rules from disk (replacement).
pub fn sync_permission_rules_from_disk(
    context: &ToolPermissionContext,
    rules: &[PermissionRule],
    allow_managed_only: bool,
) -> ToolPermissionContext {
    let mut ctx = context.clone();

    if allow_managed_only {
        let sources_to_clear = &[
            PermissionUpdateDestination::UserSettings,
            PermissionUpdateDestination::ProjectSettings,
            PermissionUpdateDestination::LocalSettings,
            PermissionUpdateDestination::CliArg,
            PermissionUpdateDestination::Session,
        ];
        let behaviors = &[
            PermissionBehavior::Allow,
            PermissionBehavior::Deny,
            PermissionBehavior::Ask,
        ];
        for &source in sources_to_clear {
            for &behavior in behaviors {
                ctx = apply_permission_update(
                    &ctx,
                    &PermissionUpdate::ReplaceRules {
                        destination: source,
                        rules: vec![],
                        behavior,
                    },
                );
            }
        }
    }

    // Clear disk-based sources
    let disk_sources = &[
        PermissionUpdateDestination::UserSettings,
        PermissionUpdateDestination::ProjectSettings,
        PermissionUpdateDestination::LocalSettings,
    ];
    let behaviors = &[
        PermissionBehavior::Allow,
        PermissionBehavior::Deny,
        PermissionBehavior::Ask,
    ];
    for &source in disk_sources {
        for &behavior in behaviors {
            ctx = apply_permission_update(
                &ctx,
                &PermissionUpdate::ReplaceRules {
                    destination: source,
                    rules: vec![],
                    behavior,
                },
            );
        }
    }

    let updates = convert_rules_to_updates(rules, "replaceRules");
    apply_permission_updates(&ctx, &updates)
}

// ─── Permission Update Application ──────────────────────────────────────────

/// Apply a single permission update to context.
pub fn apply_permission_update(
    context: &ToolPermissionContext,
    update: &PermissionUpdate,
) -> ToolPermissionContext {
    let mut ctx = context.clone();
    match update {
        PermissionUpdate::AddRules {
            destination,
            rules,
            behavior,
        } => {
            let source = destination_to_source(*destination);
            let target = get_rules_map_mut(&mut ctx, *behavior);
            let entry = target.entry(source).or_default();
            for rule_value in rules {
                let rule_str = permission_rule_value_to_string(rule_value);
                if !entry.contains(&rule_str) {
                    entry.push(rule_str);
                }
            }
        }
        PermissionUpdate::ReplaceRules {
            destination,
            rules,
            behavior,
        } => {
            let source = destination_to_source(*destination);
            let target = get_rules_map_mut(&mut ctx, *behavior);
            let new_rules: Vec<String> = rules
                .iter()
                .map(|rv| permission_rule_value_to_string(rv))
                .collect();
            target.insert(source, new_rules);
        }
        PermissionUpdate::RemoveRules {
            destination,
            rules,
            behavior,
        } => {
            let source = destination_to_source(*destination);
            let target = get_rules_map_mut(&mut ctx, *behavior);
            if let Some(existing) = target.get_mut(&source) {
                for rule_value in rules {
                    let rule_str = permission_rule_value_to_string(rule_value);
                    existing.retain(|r| r != &rule_str);
                }
            }
        }
        PermissionUpdate::SetMode { mode, .. } => {
            ctx.mode = external_to_internal_mode(*mode);
        }
        PermissionUpdate::AddDirectories {
            destination,
            directories,
        } => {
            let source = destination_to_source(*destination);
            for dir in directories {
                ctx.additional_working_directories
                    .entry(dir.clone())
                    .or_insert(super::permission_result::AdditionalWorkingDirectory {
                        path: dir.clone(),
                        source,
                    });
            }
        }
        PermissionUpdate::RemoveDirectories { directories, .. } => {
            for dir in directories {
                ctx.additional_working_directories.remove(dir);
            }
        }
    }
    ctx
}

/// Apply multiple permission updates.
pub fn apply_permission_updates(
    context: &ToolPermissionContext,
    updates: &[PermissionUpdate],
) -> ToolPermissionContext {
    let mut ctx = context.clone();
    for update in updates {
        ctx = apply_permission_update(&ctx, update);
    }
    ctx
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn get_rules_map_mut(
    ctx: &mut ToolPermissionContext,
    behavior: PermissionBehavior,
) -> &mut HashMap<PermissionRuleSource, Vec<String>> {
    match behavior {
        PermissionBehavior::Allow => &mut ctx.always_allow_rules,
        PermissionBehavior::Deny => &mut ctx.always_deny_rules,
        PermissionBehavior::Ask => &mut ctx.always_ask_rules,
    }
}

fn destination_to_source(dest: PermissionUpdateDestination) -> PermissionRuleSource {
    match dest {
        PermissionUpdateDestination::UserSettings => PermissionRuleSource::UserSettings,
        PermissionUpdateDestination::ProjectSettings => PermissionRuleSource::ProjectSettings,
        PermissionUpdateDestination::LocalSettings => PermissionRuleSource::LocalSettings,
        PermissionUpdateDestination::Session => PermissionRuleSource::Session,
        PermissionUpdateDestination::CliArg => PermissionRuleSource::CliArg,
    }
}

fn source_to_destination(
    source: PermissionRuleSource,
) -> Result<PermissionUpdateDestination, String> {
    match source {
        PermissionRuleSource::UserSettings => Ok(PermissionUpdateDestination::UserSettings),
        PermissionRuleSource::ProjectSettings => Ok(PermissionUpdateDestination::ProjectSettings),
        PermissionRuleSource::LocalSettings => Ok(PermissionUpdateDestination::LocalSettings),
        PermissionRuleSource::Session => Ok(PermissionUpdateDestination::Session),
        PermissionRuleSource::CliArg => Ok(PermissionUpdateDestination::CliArg),
        _ => Err("Cannot convert read-only source to destination".to_string()),
    }
}

fn external_to_internal_mode(mode: ExternalPermissionMode) -> PermissionMode {
    match mode {
        ExternalPermissionMode::AcceptEdits => PermissionMode::AcceptEdits,
        ExternalPermissionMode::BypassPermissions => PermissionMode::BypassPermissions,
        ExternalPermissionMode::Default => PermissionMode::Default,
        ExternalPermissionMode::DontAsk => PermissionMode::DontAsk,
        ExternalPermissionMode::Plan => PermissionMode::Plan,
    }
}

fn get_updated_input_or_fallback(
    result: &PermissionResult,
    fallback: &HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    match result {
        PermissionResult::Allow(a) => a.updated_input.clone().unwrap_or_else(|| fallback.clone()),
        _ => fallback.clone(),
    }
}

fn format_action_for_classifier(
    tool_name: &str,
    input: &HashMap<String, serde_json::Value>,
) -> String {
    format!(
        "{}({})",
        tool_name,
        serde_json::to_string(input).unwrap_or_default()
    )
}

fn parse_source(s: &str) -> Option<PermissionRuleSource> {
    match s {
        "UserSettings" => Some(PermissionRuleSource::UserSettings),
        "ProjectSettings" => Some(PermissionRuleSource::ProjectSettings),
        "LocalSettings" => Some(PermissionRuleSource::LocalSettings),
        "FlagSettings" => Some(PermissionRuleSource::FlagSettings),
        "PolicySettings" => Some(PermissionRuleSource::PolicySettings),
        "CliArg" => Some(PermissionRuleSource::CliArg),
        "Command" => Some(PermissionRuleSource::Command),
        "Session" => Some(PermissionRuleSource::Session),
        _ => None,
    }
}

fn parse_behavior(s: &str) -> Option<PermissionBehavior> {
    match s {
        "Allow" => Some(PermissionBehavior::Allow),
        "Deny" => Some(PermissionBehavior::Deny),
        "Ask" => Some(PermissionBehavior::Ask),
        _ => None,
    }
}

/// 对应 TS `getRuleByContentsForTool`：根据 rule 字符串和工具名查找匹配规则。
///
/// Rust 端真实匹配在 [`apply_permission_rules_to_context`] 内部完成；这里提供
/// 一个轻量入口：把 rule 拆分成 tool/content 后做朴素匹配。
pub fn get_rule_by_contents_for_tool(
    rules: &[String],
    tool_name: &str,
    content: &str,
) -> Option<String> {
    let target = format!("{}({})", tool_name, content);
    rules.iter().find(|r| r.as_str() == target).cloned()
}

/// 对应 TS `applyPermissionRulesToPermissionContext`：把规则列表合并到 context 中。
pub fn apply_permission_rules_to_permission_context(
    ctx: &super::permission_result::ToolPermissionContext,
    rules: Vec<super::permission_result::PermissionRule>,
) -> super::permission_result::ToolPermissionContext {
    apply_permission_rules_to_context(ctx, &rules)
}
