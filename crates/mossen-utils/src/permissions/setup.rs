//! Permission setup and initialization.
//!
//! Translates `utils/permissions/permissionSetup.ts` — dangerous permission
//! detection, permission mode initialization, mode transitions, auto-mode gate
//! checks, and tool permission context initialization.

use std::collections::HashMap;

use super::dangerous_patterns::{CROSS_PLATFORM_CODE_EXEC, dangerous_bash_patterns};
use super::permission_result::{
    AdditionalWorkingDirectory, ExternalPermissionMode, PermissionBehavior, PermissionMode,
    PermissionRule, PermissionRuleSource, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination, ToolPermissionContext, ToolPermissionRulesBySource,
};
use super::permissions::{
    apply_permission_update, apply_permission_updates, convert_rules_to_updates,
    apply_permission_rules_to_context, permission_rule_value_from_string,
    permission_rule_value_to_string, BASH_TOOL_NAME, POWERSHELL_TOOL_NAME, AGENT_TOOL_NAME,
};

// ─── Dangerous Permission Detection ─────────────────────────────────────────

/// Checks if a Bash permission rule is dangerous for auto mode.
pub fn is_dangerous_bash_permission(tool_name: &str, rule_content: Option<&str>) -> bool {
    if tool_name != BASH_TOOL_NAME {
        return false;
    }
    match rule_content {
        None | Some("") => return true,
        Some(content) => {
            let content = content.trim().to_lowercase();
            if content == "*" {
                return true;
            }
            for pattern in dangerous_bash_patterns(false) {
                let lp = pattern.to_lowercase();
                if content == lp {
                    return true;
                }
                if content == format!("{}:*", lp) {
                    return true;
                }
                if content == format!("{}*", lp) {
                    return true;
                }
                if content == format!("{} *", lp) {
                    return true;
                }
                if content.starts_with(&format!("{} -", lp)) && content.ends_with('*') {
                    return true;
                }
            }
        }
    }
    false
}

/// PowerShell-specific dangerous patterns.
const POWERSHELL_DANGEROUS_PATTERNS: &[&str] = &[
    "pwsh",
    "powershell",
    "cmd",
    "wsl",
    "iex",
    "invoke-expression",
    "icm",
    "invoke-command",
    "start-process",
    "saps",
    "start",
    "start-job",
    "sajb",
    "start-threadjob",
    "register-objectevent",
    "register-engineevent",
    "register-wmievent",
    "register-scheduledjob",
    "new-pssession",
    "nsn",
    "enter-pssession",
    "etsn",
    "add-type",
    "new-object",
];

/// Checks if a PowerShell permission rule is dangerous for auto mode.
pub fn is_dangerous_powershell_permission(tool_name: &str, rule_content: Option<&str>) -> bool {
    if tool_name != POWERSHELL_TOOL_NAME {
        return false;
    }
    match rule_content {
        None | Some("") => return true,
        Some(content) => {
            let content = content.trim().to_lowercase();
            if content == "*" {
                return true;
            }

            // Combine cross-platform and PS-specific patterns
            let all_patterns: Vec<&str> = CROSS_PLATFORM_CODE_EXEC
                .iter()
                .copied()
                .chain(POWERSHELL_DANGEROUS_PATTERNS.iter().copied())
                .collect();

            for pattern in &all_patterns {
                let lp = pattern.to_lowercase();

                if content == lp {
                    return true;
                }
                if content == format!("{}:*", lp) {
                    return true;
                }
                if content == format!("{}*", lp) {
                    return true;
                }
                if content == format!("{} *", lp) {
                    return true;
                }
                if content.starts_with(&format!("{} -", lp)) && content.ends_with('*') {
                    return true;
                }
                // .exe variants
                let exe = if let Some(sp) = lp.find(' ') {
                    format!("{}.exe{}", &lp[..sp], &lp[sp..])
                } else {
                    format!("{}.exe", lp)
                };
                if content == exe {
                    return true;
                }
                if content == format!("{}:*", exe) {
                    return true;
                }
                if content == format!("{}*", exe) {
                    return true;
                }
                if content == format!("{} *", exe) {
                    return true;
                }
                if content.starts_with(&format!("{} -", exe)) && content.ends_with('*') {
                    return true;
                }
            }
        }
    }
    false
}

/// Checks if an Agent permission rule is dangerous for auto mode.
pub fn is_dangerous_task_permission(tool_name: &str, _rule_content: Option<&str>) -> bool {
    normalize_legacy_tool_name(tool_name) == AGENT_TOOL_NAME
}

/// Normalize legacy tool names (e.g., "Task" → "Agent").
pub fn normalize_legacy_tool_name(name: &str) -> &str {
    match name {
        "Task" => "Agent",
        _ => name,
    }
}

/// Checks if a permission rule is dangerous for auto mode (any tool type).
pub fn is_dangerous_classifier_permission(
    tool_name: &str,
    rule_content: Option<&str>,
    user_type: &str,
) -> bool {
    if user_type == "ant" && tool_name == "Tmux" {
        return true;
    }
    is_dangerous_bash_permission(tool_name, rule_content)
        || is_dangerous_powershell_permission(tool_name, rule_content)
        || is_dangerous_task_permission(tool_name, rule_content)
}

// ─── Dangerous Permission Info ───────────────────────────────────────────────

/// Information about a detected dangerous permission.
#[derive(Debug, Clone)]
pub struct DangerousPermissionInfo {
    pub rule_value: PermissionRuleValue,
    pub source: PermissionRuleSource,
    pub rule_display: String,
    pub source_display: String,
}

/// Finds all dangerous permissions from rules and CLI arguments.
pub fn find_dangerous_classifier_permissions(
    rules: &[PermissionRule],
    cli_allowed_tools: &[String],
    user_type: &str,
) -> Vec<DangerousPermissionInfo> {
    let mut dangerous = Vec::new();

    for rule in rules {
        if rule.rule_behavior == PermissionBehavior::Allow
            && is_dangerous_classifier_permission(
                &rule.rule_value.tool_name,
                rule.rule_value.rule_content.as_deref(),
                user_type,
            )
        {
            let rule_display = match &rule.rule_value.rule_content {
                Some(c) => format!("{}({})", rule.rule_value.tool_name, c),
                None => format!("{}(*)", rule.rule_value.tool_name),
            };
            dangerous.push(DangerousPermissionInfo {
                rule_value: rule.rule_value.clone(),
                source: rule.source,
                rule_display,
                source_display: format!("{:?}", rule.source),
            });
        }
    }

    // Check CLI --allowed-tools
    for tool_spec in cli_allowed_tools {
        let (tool_name, rule_content) = parse_tool_spec(tool_spec);
        if is_dangerous_classifier_permission(&tool_name, rule_content.as_deref(), user_type) {
            let rule_display = match &rule_content {
                Some(_) => tool_spec.clone(),
                None => format!("{}(*)", tool_name),
            };
            dangerous.push(DangerousPermissionInfo {
                rule_value: PermissionRuleValue {
                    tool_name: tool_name.clone(),
                    rule_content,
                },
                source: PermissionRuleSource::CliArg,
                rule_display,
                source_display: "--allowed-tools".to_string(),
            });
        }
    }

    dangerous
}

/// Parse a tool spec like "Bash(pattern)" into (tool_name, rule_content).
fn parse_tool_spec(spec: &str) -> (String, Option<String>) {
    if let Some(paren_idx) = spec.find('(') {
        let tool_name = spec[..paren_idx].trim().to_string();
        let rest = &spec[paren_idx + 1..];
        let rule_content = if let Some(end) = rest.rfind(')') {
            Some(rest[..end].trim().to_string())
        } else {
            Some(rest.trim().to_string())
        };
        (tool_name, rule_content)
    } else {
        (spec.trim().to_string(), None)
    }
}

// ─── Overly Broad Rules ──────────────────────────────────────────────────────

/// Checks if a Bash allow rule is overly broad (allows ALL commands).
pub fn is_overly_broad_bash_allow_rule(rule_value: &PermissionRuleValue) -> bool {
    rule_value.tool_name == BASH_TOOL_NAME && rule_value.rule_content.is_none()
}

/// Checks if a PowerShell allow rule is overly broad.
pub fn is_overly_broad_powershell_allow_rule(rule_value: &PermissionRuleValue) -> bool {
    rule_value.tool_name == POWERSHELL_TOOL_NAME && rule_value.rule_content.is_none()
}

/// Finds all overly broad Bash allow rules.
pub fn find_overly_broad_bash_permissions(
    rules: &[PermissionRule],
    cli_allowed_tools: &[String],
) -> Vec<DangerousPermissionInfo> {
    let mut result = Vec::new();
    for rule in rules {
        if rule.rule_behavior == PermissionBehavior::Allow
            && is_overly_broad_bash_allow_rule(&rule.rule_value)
        {
            result.push(DangerousPermissionInfo {
                rule_value: rule.rule_value.clone(),
                source: rule.source,
                rule_display: format!("{}(*)", BASH_TOOL_NAME),
                source_display: format!("{:?}", rule.source),
            });
        }
    }
    for tool_spec in cli_allowed_tools {
        let parsed = permission_rule_value_from_string(tool_spec);
        if is_overly_broad_bash_allow_rule(&parsed) {
            result.push(DangerousPermissionInfo {
                rule_value: parsed,
                source: PermissionRuleSource::CliArg,
                rule_display: format!("{}(*)", BASH_TOOL_NAME),
                source_display: "--allowed-tools".to_string(),
            });
        }
    }
    result
}

/// Finds all overly broad PowerShell allow rules.
pub fn find_overly_broad_powershell_permissions(
    rules: &[PermissionRule],
    cli_allowed_tools: &[String],
) -> Vec<DangerousPermissionInfo> {
    let mut result = Vec::new();
    for rule in rules {
        if rule.rule_behavior == PermissionBehavior::Allow
            && is_overly_broad_powershell_allow_rule(&rule.rule_value)
        {
            result.push(DangerousPermissionInfo {
                rule_value: rule.rule_value.clone(),
                source: rule.source,
                rule_display: format!("{}(*)", POWERSHELL_TOOL_NAME),
                source_display: format!("{:?}", rule.source),
            });
        }
    }
    for tool_spec in cli_allowed_tools {
        let parsed = permission_rule_value_from_string(tool_spec);
        if is_overly_broad_powershell_allow_rule(&parsed) {
            result.push(DangerousPermissionInfo {
                rule_value: parsed,
                source: PermissionRuleSource::CliArg,
                rule_display: format!("{}(*)", POWERSHELL_TOOL_NAME),
                source_display: "--allowed-tools".to_string(),
            });
        }
    }
    result
}

// ─── Permission Removal ──────────────────────────────────────────────────────

fn is_permission_update_destination(source: PermissionRuleSource) -> bool {
    matches!(
        source,
        PermissionRuleSource::UserSettings
            | PermissionRuleSource::ProjectSettings
            | PermissionRuleSource::LocalSettings
            | PermissionRuleSource::Session
            | PermissionRuleSource::CliArg
    )
}

/// Removes dangerous permissions from the in-memory context.
pub fn remove_dangerous_permissions(
    context: &ToolPermissionContext,
    dangerous_permissions: &[DangerousPermissionInfo],
) -> ToolPermissionContext {
    let mut rules_by_source: HashMap<PermissionUpdateDestination, Vec<PermissionRuleValue>> =
        HashMap::new();
    for perm in dangerous_permissions {
        if !is_permission_update_destination(perm.source) {
            continue;
        }
        let dest = source_to_update_destination(perm.source);
        if let Some(d) = dest {
            rules_by_source.entry(d).or_default().push(perm.rule_value.clone());
        }
    }

    let mut ctx = context.clone();
    for (destination, rules) in rules_by_source {
        ctx = apply_permission_update(
            &ctx,
            &PermissionUpdate::RemoveRules {
                destination,
                rules,
                behavior: PermissionBehavior::Allow,
            },
        );
    }
    ctx
}

fn source_to_update_destination(source: PermissionRuleSource) -> Option<PermissionUpdateDestination> {
    match source {
        PermissionRuleSource::UserSettings => Some(PermissionUpdateDestination::UserSettings),
        PermissionRuleSource::ProjectSettings => Some(PermissionUpdateDestination::ProjectSettings),
        PermissionRuleSource::LocalSettings => Some(PermissionUpdateDestination::LocalSettings),
        PermissionRuleSource::Session => Some(PermissionUpdateDestination::Session),
        PermissionRuleSource::CliArg => Some(PermissionUpdateDestination::CliArg),
        _ => None,
    }
}

/// Strips dangerous permissions for auto mode.
pub fn strip_dangerous_permissions_for_auto_mode(
    context: &ToolPermissionContext,
    user_type: &str,
) -> ToolPermissionContext {
    let mut rules: Vec<PermissionRule> = Vec::new();
    for (source, rule_strings) in &context.always_allow_rules {
        for rule_string in rule_strings {
            let rule_value = permission_rule_value_from_string(rule_string);
            rules.push(PermissionRule {
                source: *source,
                rule_behavior: PermissionBehavior::Allow,
                rule_value,
            });
        }
    }
    let dangerous = find_dangerous_classifier_permissions(&rules, &[], user_type);
    if dangerous.is_empty() {
        let mut ctx = context.clone();
        if ctx.stripped_dangerous_rules.is_none() {
            ctx.stripped_dangerous_rules = Some(HashMap::new());
        }
        return ctx;
    }

    // Build the stash of stripped rules
    let mut stripped: ToolPermissionRulesBySource = HashMap::new();
    for perm in &dangerous {
        if is_permission_update_destination(perm.source) {
            stripped
                .entry(perm.source)
                .or_default()
                .push(permission_rule_value_to_string(&perm.rule_value));
        }
    }

    let mut ctx = remove_dangerous_permissions(context, &dangerous);
    ctx.stripped_dangerous_rules = Some(stripped);
    ctx
}

/// Restores dangerous permissions previously stripped.
pub fn restore_dangerous_permissions(context: &ToolPermissionContext) -> ToolPermissionContext {
    let stash = match &context.stripped_dangerous_rules {
        Some(s) => s.clone(),
        None => return context.clone(),
    };

    let mut ctx = context.clone();
    for (source, rule_strings) in &stash {
        if rule_strings.is_empty() {
            continue;
        }
        let rules: Vec<PermissionRuleValue> = rule_strings
            .iter()
            .map(|s| permission_rule_value_from_string(s))
            .collect();
        if let Some(dest) = source_to_update_destination(*source) {
            ctx = apply_permission_update(
                &ctx,
                &PermissionUpdate::AddRules {
                    destination: dest,
                    rules,
                    behavior: PermissionBehavior::Allow,
                },
            );
        }
    }
    ctx.stripped_dangerous_rules = None;
    ctx
}

// ─── Permission Mode Transitions ─────────────────────────────────────────────

/// Configuration for mode transitions.
pub struct ModeTransitionConfig {
    pub transcript_classifier_enabled: bool,
    pub is_auto_mode_active: bool,
    pub is_auto_mode_gate_enabled: bool,
    pub user_type: String,
}

/// Handles state transitions when switching permission modes.
pub fn transition_permission_mode(
    from_mode: PermissionMode,
    to_mode: PermissionMode,
    context: &ToolPermissionContext,
    config: &ModeTransitionConfig,
) -> ToolPermissionContext {
    if from_mode == to_mode {
        return context.clone();
    }

    let mut ctx = context.clone();

    if config.transcript_classifier_enabled {
        if to_mode == PermissionMode::Plan && from_mode != PermissionMode::Plan {
            return prepare_context_for_plan_mode(&ctx, config);
        }

        let from_uses_classifier = from_mode == PermissionMode::Auto
            || (from_mode == PermissionMode::Plan && config.is_auto_mode_active);
        let to_uses_classifier = to_mode == PermissionMode::Auto;

        if to_uses_classifier && !from_uses_classifier {
            if !config.is_auto_mode_gate_enabled {
                // Cannot transition — return unchanged
                return ctx;
            }
            ctx = strip_dangerous_permissions_for_auto_mode(&ctx, &config.user_type);
        } else if from_uses_classifier && !to_uses_classifier {
            ctx = restore_dangerous_permissions(&ctx);
        }
    }

    // Clear pre_plan_mode when leaving plan
    if from_mode == PermissionMode::Plan && to_mode != PermissionMode::Plan {
        ctx.pre_plan_mode = None;
    }

    ctx
}

/// Prepare context for entering plan mode.
pub fn prepare_context_for_plan_mode(
    context: &ToolPermissionContext,
    config: &ModeTransitionConfig,
) -> ToolPermissionContext {
    let current_mode = context.mode;
    if current_mode == PermissionMode::Plan {
        return context.clone();
    }

    if config.transcript_classifier_enabled {
        let should_plan_use_auto = config.is_auto_mode_gate_enabled;

        if current_mode == PermissionMode::Auto {
            if should_plan_use_auto {
                let mut ctx = context.clone();
                ctx.pre_plan_mode = Some(PermissionMode::Auto);
                return ctx;
            }
            let mut ctx = restore_dangerous_permissions(context);
            ctx.pre_plan_mode = Some(PermissionMode::Auto);
            return ctx;
        }
        if should_plan_use_auto && current_mode != PermissionMode::BypassPermissions {
            let mut ctx = strip_dangerous_permissions_for_auto_mode(context, &config.user_type);
            ctx.pre_plan_mode = Some(current_mode);
            return ctx;
        }
    }

    let mut ctx = context.clone();
    ctx.pre_plan_mode = Some(current_mode);
    ctx
}

/// Reconciles auto-mode state during plan mode after settings change.
pub fn transition_plan_auto_mode(
    context: &ToolPermissionContext,
    config: &ModeTransitionConfig,
    want_auto: bool,
    have_auto: bool,
) -> ToolPermissionContext {
    if !config.transcript_classifier_enabled {
        return context.clone();
    }
    if context.mode != PermissionMode::Plan {
        return context.clone();
    }
    if context.pre_plan_mode == Some(PermissionMode::BypassPermissions) {
        return context.clone();
    }

    if want_auto && have_auto {
        return strip_dangerous_permissions_for_auto_mode(context, &config.user_type);
    }
    if !want_auto && !have_auto {
        return context.clone();
    }
    if want_auto {
        return strip_dangerous_permissions_for_auto_mode(context, &config.user_type);
    }
    restore_dangerous_permissions(context)
}

// ─── Permission Mode from CLI ────────────────────────────────────────────────

/// Result of initial permission mode computation from CLI.
pub struct InitialPermissionModeResult {
    pub mode: PermissionMode,
    pub notification: Option<String>,
}

/// Parse permission mode from a string.
pub fn permission_mode_from_string(s: &str) -> PermissionMode {
    match s {
        "acceptEdits" => PermissionMode::AcceptEdits,
        "bypassPermissions" => PermissionMode::BypassPermissions,
        "default" => PermissionMode::Default,
        "dontAsk" => PermissionMode::DontAsk,
        "plan" => PermissionMode::Plan,
        "auto" => PermissionMode::Auto,
        "bubble" => PermissionMode::Bubble,
        _ => PermissionMode::Default,
    }
}

/// Configuration for initial permission mode determination.
pub struct PermissionModeCliConfig {
    pub permission_mode_cli: Option<String>,
    pub dangerously_skip_permissions: bool,
    pub disable_bypass_permissions_mode: bool,
    pub auto_mode_circuit_broken: bool,
    pub transcript_classifier_enabled: bool,
    pub default_mode_setting: Option<String>,
    pub is_remote: bool,
}

/// Safely convert CLI flags to a PermissionMode.
pub fn initial_permission_mode_from_cli(
    config: &PermissionModeCliConfig,
) -> InitialPermissionModeResult {
    let mut ordered_modes: Vec<PermissionMode> = Vec::new();
    let mut notification: Option<String> = None;

    if config.dangerously_skip_permissions {
        ordered_modes.push(PermissionMode::BypassPermissions);
    }

    if let Some(ref mode_str) = config.permission_mode_cli {
        let parsed = permission_mode_from_string(mode_str);
        if config.transcript_classifier_enabled && parsed == PermissionMode::Auto {
            if !config.auto_mode_circuit_broken {
                ordered_modes.push(PermissionMode::Auto);
            }
        } else {
            ordered_modes.push(parsed);
        }
    }

    if let Some(ref settings_mode) = config.default_mode_setting {
        let mode = permission_mode_from_string(settings_mode);
        if config.is_remote
            && !matches!(
                mode,
                PermissionMode::AcceptEdits | PermissionMode::Plan | PermissionMode::Default
            )
        {
            // Ignore unsupported modes in remote
        } else if config.transcript_classifier_enabled && mode == PermissionMode::Auto {
            if !config.auto_mode_circuit_broken {
                ordered_modes.push(PermissionMode::Auto);
            }
        } else {
            ordered_modes.push(mode);
        }
    }

    for mode in &ordered_modes {
        if *mode == PermissionMode::BypassPermissions && config.disable_bypass_permissions_mode {
            notification = Some(
                "Bypass permissions mode was disabled by policy".to_string(),
            );
            continue;
        }
        return InitialPermissionModeResult {
            mode: *mode,
            notification,
        };
    }

    InitialPermissionModeResult {
        mode: PermissionMode::Default,
        notification,
    }
}

// ─── Tool List Parsing ───────────────────────────────────────────────────────

/// Parse a tool list from CLI arguments (handles commas, spaces, parens).
pub fn parse_tool_list_from_cli(tools: &[String]) -> Vec<String> {
    if tools.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for tool_string in tools {
        if tool_string.is_empty() {
            continue;
        }
        let mut current = String::new();
        let mut in_parens = false;

        for ch in tool_string.chars() {
            match ch {
                '(' => {
                    in_parens = true;
                    current.push(ch);
                }
                ')' => {
                    in_parens = false;
                    current.push(ch);
                }
                ',' => {
                    if in_parens {
                        current.push(ch);
                    } else {
                        let trimmed = current.trim().to_string();
                        if !trimmed.is_empty() {
                            result.push(trimmed);
                        }
                        current.clear();
                    }
                }
                ' ' => {
                    if in_parens {
                        current.push(ch);
                    } else {
                        let trimmed = current.trim().to_string();
                        if !trimmed.is_empty() {
                            result.push(trimmed);
                        }
                        current.clear();
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            result.push(trimmed);
        }
    }
    result
}

// ─── Initialize Tool Permission Context ──────────────────────────────────────

/// Configuration for initializing the tool permission context.
pub struct InitializePermissionContextConfig {
    pub allowed_tools_cli: Vec<String>,
    pub disallowed_tools_cli: Vec<String>,
    pub permission_mode: PermissionMode,
    pub allow_dangerously_skip_permissions: bool,
    pub add_dirs: Vec<String>,
    pub rules_from_disk: Vec<PermissionRule>,
    pub user_type: String,
    pub is_remote: bool,
    pub transcript_classifier_enabled: bool,
    pub is_auto_mode_gate_enabled: bool,
    pub disable_bypass_permissions_mode: bool,
    pub has_skip_dangerous_mode_prompt: bool,
    pub process_pwd: Option<String>,
    pub original_cwd: String,
}

/// Result of permission context initialization.
pub struct InitializePermissionContextResult {
    pub tool_permission_context: ToolPermissionContext,
    pub warnings: Vec<String>,
    pub dangerous_permissions: Vec<DangerousPermissionInfo>,
    pub overly_broad_bash_permissions: Vec<DangerousPermissionInfo>,
}

/// Initialize the tool permission context from all sources.
pub fn initialize_tool_permission_context(
    config: InitializePermissionContextConfig,
) -> InitializePermissionContextResult {
    let parsed_allowed = parse_tool_list_from_cli(&config.allowed_tools_cli)
        .into_iter()
        .map(|s| {
            let parsed = permission_rule_value_from_string(&s);
            permission_rule_value_to_string(&parsed)
        })
        .collect::<Vec<_>>();
    let parsed_disallowed = parse_tool_list_from_cli(&config.disallowed_tools_cli);

    let is_bypass_available = (config.permission_mode == PermissionMode::BypassPermissions
        || config.allow_dangerously_skip_permissions
        || config.has_skip_dangerous_mode_prompt)
        && !config.disable_bypass_permissions_mode;

    // Detect overly broad rules
    let mut overly_broad = Vec::new();
    if !config.is_remote {
        overly_broad = find_overly_broad_bash_permissions(&config.rules_from_disk, &parsed_allowed);
        overly_broad.extend(find_overly_broad_powershell_permissions(
            &config.rules_from_disk,
            &parsed_allowed,
        ));
    }

    // Detect dangerous classifier permissions
    let mut dangerous = Vec::new();
    if config.transcript_classifier_enabled && config.permission_mode == PermissionMode::Auto {
        dangerous = find_dangerous_classifier_permissions(
            &config.rules_from_disk,
            &parsed_allowed,
            &config.user_type,
        );
    }

    // Build initial context
    let mut additional_dirs = HashMap::new();
    if let Some(ref pwd) = config.process_pwd {
        if pwd != &config.original_cwd {
            additional_dirs.insert(
                pwd.clone(),
                AdditionalWorkingDirectory {
                    path: pwd.clone(),
                    source: PermissionRuleSource::Session,
                },
            );
        }
    }

    let initial_context = ToolPermissionContext {
        mode: config.permission_mode,
        additional_working_directories: additional_dirs,
        always_allow_rules: {
            let mut m = HashMap::new();
            m.insert(PermissionRuleSource::CliArg, parsed_allowed);
            m
        },
        always_deny_rules: {
            let mut m = HashMap::new();
            m.insert(PermissionRuleSource::CliArg, parsed_disallowed);
            m
        },
        always_ask_rules: HashMap::new(),
        is_bypass_permissions_mode_available: is_bypass_available,
        stripped_dangerous_rules: None,
        should_avoid_permission_prompts: None,
        await_automated_checks_before_dialog: None,
        pre_plan_mode: None,
        is_auto_mode_available: if config.transcript_classifier_enabled {
            Some(config.is_auto_mode_gate_enabled)
        } else {
            None
        },
    };

    let tool_permission_context =
        apply_permission_rules_to_context(&initial_context, &config.rules_from_disk);

    InitializePermissionContextResult {
        tool_permission_context,
        warnings: Vec::new(),
        dangerous_permissions: dangerous,
        overly_broad_bash_permissions: overly_broad,
    }
}

// ─── Auto Mode Gate ──────────────────────────────────────────────────────────

/// Auto mode enabled state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoModeEnabledState {
    Enabled,
    Disabled,
    OptIn,
}

/// Parse auto mode enabled state from a string.
pub fn parse_auto_mode_enabled_state(value: Option<&str>) -> AutoModeEnabledState {
    match value {
        Some("enabled") => AutoModeEnabledState::Enabled,
        Some("disabled") => AutoModeEnabledState::Disabled,
        Some("opt-in") => AutoModeEnabledState::OptIn,
        _ => AutoModeEnabledState::Disabled, // default
    }
}

/// Reason auto mode is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoModeUnavailableReason {
    Settings,
    CircuitBreaker,
    Model,
}

/// Get notification message for auto mode unavailability.
pub fn get_auto_mode_unavailable_notification(
    reason: AutoModeUnavailableReason,
    user_type: &str,
) -> String {
    let base = match reason {
        AutoModeUnavailableReason::Settings => "auto mode disabled by settings",
        AutoModeUnavailableReason::CircuitBreaker => "auto mode is unavailable for your plan",
        AutoModeUnavailableReason::Model => "auto mode unavailable for this model",
    };
    if user_type == "ant" {
        format!("{} · #mossen-feedback", base)
    } else {
        base.to_string()
    }
}

/// Creates context with bypass permissions disabled.
pub fn create_disabled_bypass_permissions_context(
    context: &ToolPermissionContext,
) -> ToolPermissionContext {
    let mut ctx = if context.mode == PermissionMode::BypassPermissions {
        apply_permission_update(
            context,
            &PermissionUpdate::SetMode {
                destination: PermissionUpdateDestination::Session,
                mode: ExternalPermissionMode::Default,
            },
        )
    } else {
        context.clone()
    };
    ctx.is_bypass_permissions_mode_available = false;
    ctx
}

/// Whether plan mode should use auto mode semantics.
pub fn should_plan_use_auto_mode(
    has_auto_mode_opt_in: bool,
    is_gate_enabled: bool,
    use_auto_during_plan: bool,
) -> bool {
    has_auto_mode_opt_in && is_gate_enabled && use_auto_during_plan
}

/// Check if default permission mode is auto.
pub fn is_default_permission_mode_auto(
    settings_default_mode: Option<&str>,
    transcript_classifier_enabled: bool,
) -> bool {
    if transcript_classifier_enabled {
        return settings_default_mode == Some("auto");
    }
    false
}

/// Check if bypass permissions mode is currently disabled.
pub fn is_bypass_permissions_mode_disabled(
    gate_disabled: bool,
    settings_disabled: bool,
) -> bool {
    gate_disabled || settings_disabled
}

// =============================================================================
// Auto-mode gate API — TS exports a small set of helpers that combine settings
// + circuit breaker + model gate. The Rust side mirrors the shape; the actual
// gate state is wired from settings/circuit-breaker modules.
// =============================================================================

/// 对应 TS `AutoModeGateCheckResult`：gate 检查结果。
#[derive(Debug, Clone)]
pub struct AutoModeGateCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
}

/// 解析 CLI `--allowed-tools` 等参数中的基础工具列表（对应 TS `parseBaseToolsFromCLI`）。
pub fn parse_base_tools_from_cli(base_tools: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in base_tools {
        for piece in raw.split(',') {
            let piece = piece.trim();
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
        }
    }
    out
}

/// 校验 auto-mode 是否可用（对应 TS `verifyAutoModeGateAccess`）。
///
/// Rust 端的真实实现会综合查询 settings/circuit-breaker；此函数提供入口，
/// 调用方传入预先收集的 gate 信号。
pub async fn verify_auto_mode_gate_access(
    settings_enabled: bool,
    circuit_breaker_open: bool,
    model_supports_auto_mode: bool,
) -> AutoModeGateCheckResult {
    if !settings_enabled {
        return AutoModeGateCheckResult {
            allowed: false,
            reason: Some("settings".into()),
        };
    }
    if circuit_breaker_open {
        return AutoModeGateCheckResult {
            allowed: false,
            reason: Some("circuit-breaker".into()),
        };
    }
    if !model_supports_auto_mode {
        return AutoModeGateCheckResult {
            allowed: false,
            reason: Some("model".into()),
        };
    }
    AutoModeGateCheckResult { allowed: true, reason: None }
}

/// 是否应该禁用 bypass permissions（对应 TS `shouldDisableBypassPermissions`）。
pub async fn should_disable_bypass_permissions() -> bool {
    matches!(
        std::env::var("MOSSEN_DISABLE_BYPASS_PERMISSIONS").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// auto-mode gate 是否开启。
pub fn is_auto_mode_gate_enabled() -> bool {
    !matches!(
        std::env::var("MOSSEN_DISABLE_AUTO_MODE").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 当前不可用原因（对应 TS `getAutoModeUnavailableReason`）。
pub fn get_auto_mode_unavailable_reason() -> Option<String> {
    if !is_auto_mode_gate_enabled() {
        Some("settings".to_string())
    } else {
        None
    }
}

static AUTO_MODE_STATE_CACHE: once_cell::sync::Lazy<std::sync::Mutex<Option<AutoModeEnabledState>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

/// 异步查询 auto-mode 开启状态（对应 TS `getAutoModeEnabledState`）。
pub async fn get_auto_mode_enabled_state() -> AutoModeEnabledState {
    let state = if is_auto_mode_gate_enabled() {
        AutoModeEnabledState::Enabled
    } else {
        AutoModeEnabledState::Disabled
    };
    *AUTO_MODE_STATE_CACHE.lock().unwrap() = Some(state);
    state
}

/// 同步读取缓存的 auto-mode 状态（对应 TS `getAutoModeEnabledStateIfCached`）。
pub fn get_auto_mode_enabled_state_if_cached() -> Option<AutoModeEnabledState> {
    *AUTO_MODE_STATE_CACHE.lock().unwrap()
}

/// 任意来源是否声明加入了 auto-mode（对应 TS `hasAutoModeOptInAnySource`）。
pub fn has_auto_mode_opt_in_any_source(
    settings_enabled: bool,
    env_enabled: bool,
    cli_enabled: bool,
) -> bool {
    settings_enabled || env_enabled || cli_enabled
}

/// 当 bypass permissions 不再合法时，禁用并返回是否真的发生了变化。
pub async fn check_and_disable_bypass_permissions() -> bool {
    if should_disable_bypass_permissions().await {
        // SAFETY: 初始化阶段单线程写环境变量。
        unsafe {
            std::env::set_var("MOSSEN_BYPASS_PERMISSIONS", "0");
        }
        return true;
    }
    false
}
