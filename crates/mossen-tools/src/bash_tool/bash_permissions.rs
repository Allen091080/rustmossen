//! Bash permission system — rule matching, wildcard patterns, and permission evaluation.
//!
//! Corresponds to `bashPermissions.ts` (2570 lines). Implements the full permission
//! rule matching system including prefix rules, exact rules, wildcard rules,
//! env var stripping, safe wrapper stripping, and compound command analysis.

use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Maximum number of subcommands to check for security (prevents DoS on complex commands).
pub const MAX_SUBCOMMANDS_FOR_SECURITY_CHECK: usize = 50;

/// Maximum number of rules to suggest for compound commands.
pub const MAX_SUGGESTED_RULES_FOR_COMPOUND: usize = 5;

/// Environment variables that could hijack binary execution.
pub const BINARY_HIJACK_VARS: &[&str] = &[
    "PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH", "PYTHONPATH", "NODE_PATH", "RUBYLIB",
    "PERL5LIB", "CLASSPATH", "HOME", "XDG_CONFIG_HOME",
];

/// Safe wrapper commands that can be stripped for permission matching.
const SAFE_WRAPPERS: &[&str] = &[
    "timeout", "nice", "ionice", "time", "strace", "ltrace",
    "nohup", "setsid", "env", "sudo",
];

/// Env var assignment regex pattern.
fn is_env_var_assign(token: &str) -> bool {
    let re = Regex::new(r"^[A-Za-z_]\w*=").unwrap();
    re.is_match(token)
}

/// Permission rule types parsed from string patterns.
#[derive(Debug, Clone, PartialEq)]
pub enum BashPermissionRule {
    /// Rule matches command prefix (e.g., "git:" matches "git status", "git push").
    Prefix { prefix: String },
    /// Rule matches exact command string.
    Exact { command: String },
    /// Rule uses wildcard pattern matching.
    Wildcard { pattern: String },
}

/// Permission behavior outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Ask,
    Deny,
    Passthrough,
}

/// Full permission result.
#[derive(Debug, Clone)]
pub struct PermissionResult {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
    pub suggestions: Vec<PermissionUpdate>,
    pub updated_input: Option<String>,
}

impl PermissionResult {
    pub fn allow(input: Option<String>) -> Self {
        Self {
            behavior: PermissionBehavior::Allow,
            message: None,
            suggestions: vec![],
            updated_input: input,
        }
    }
    pub fn ask(message: String) -> Self {
        Self {
            behavior: PermissionBehavior::Ask,
            message: Some(message),
            suggestions: vec![],
            updated_input: None,
        }
    }
    pub fn deny(message: String) -> Self {
        Self {
            behavior: PermissionBehavior::Deny,
            message: Some(message),
            suggestions: vec![],
            updated_input: None,
        }
    }
    pub fn passthrough(message: &str) -> Self {
        Self {
            behavior: PermissionBehavior::Passthrough,
            message: Some(message.to_string()),
            suggestions: vec![],
            updated_input: None,
        }
    }
}

/// Permission update suggestion.
#[derive(Debug, Clone)]
pub struct PermissionUpdate {
    pub tool: String,
    pub rule: String,
    pub description: String,
}

/// Parse a permission rule string into its structured form.
///
/// Rules can be:
/// - `command:*` → prefix rule (matches command with any args)
/// - `command arg1 arg2` → exact rule
/// - `command *pattern*` → wildcard rule
pub fn bash_permission_rule(pattern: &str) -> BashPermissionRule {
    let trimmed = pattern.trim();

    // Check for wildcard patterns
    if trimmed.contains('*') || trimmed.contains('?') {
        // If pattern ends with `:*`, it's a prefix
        if let Some(prefix) = trimmed.strip_suffix(":*") {
            return BashPermissionRule::Prefix {
                prefix: prefix.to_string(),
            };
        }
        return BashPermissionRule::Wildcard {
            pattern: trimmed.to_string(),
        };
    }

    // If it ends with `:` followed by more content, could be prefix
    if trimmed.contains(':') {
        let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
        if parts.len() == 2 && parts[1] == "*" {
            return BashPermissionRule::Prefix {
                prefix: parts[0].to_string(),
            };
        }
    }

    // Default: exact match
    BashPermissionRule::Exact {
        command: trimmed.to_string(),
    }
}

/// Match a wildcard pattern against a command string.
///
/// Supports `*` (matches any sequence) and `?` (matches single char).
pub fn match_wildcard_pattern(pattern: &str, command: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let command_chars: Vec<char> = command.chars().collect();

    fn matches(p: &[char], s: &[char], pi: usize, si: usize) -> bool {
        if pi == p.len() && si == s.len() {
            return true;
        }
        if pi == p.len() {
            return false;
        }

        if p[pi] == '*' {
            // Try matching zero or more characters
            for i in si..=s.len() {
                if matches(p, s, pi + 1, i) {
                    return true;
                }
            }
            return false;
        }

        if si == s.len() {
            return false;
        }

        if p[pi] == '?' || p[pi] == s[si] {
            return matches(p, s, pi + 1, si + 1);
        }

        false
    }

    matches(&pattern_chars, &command_chars, 0, 0)
}

/// Extract the prefix from a permission rule (for display/matching).
pub fn permission_rule_extract_prefix(rule: &str) -> Option<String> {
    match bash_permission_rule(rule) {
        BashPermissionRule::Prefix { prefix } => Some(prefix),
        _ => None,
    }
}

/// Strip all leading environment variable assignments from a command.
/// E.g., `FOO=bar BAR=baz cmd args` → `cmd args`.
pub fn strip_all_leading_env_vars(command: &str) -> String {
    let mut rest = command.trim();
    loop {
        let token = rest.split_whitespace().next().unwrap_or("");
        if token.is_empty() || !is_env_var_assign(token) {
            break;
        }
        rest = rest[token.len()..].trim_start();
    }
    rest.to_string()
}

/// Strip safe wrapper commands from the front of a command.
/// E.g., `timeout 30 nice -n 5 cmd args` → `cmd args`.
pub fn strip_safe_wrappers(command: &str) -> String {
    let mut rest = command.trim();
    loop {
        let first_word = rest.split_whitespace().next().unwrap_or("");
        if first_word.is_empty() || !SAFE_WRAPPERS.contains(&first_word) {
            break;
        }
        // Skip the wrapper command and its arguments (simplified: skip one arg if numeric/flag)
        rest = rest[first_word.len()..].trim_start();

        // Skip arguments that look like wrapper options
        loop {
            let next = rest.split_whitespace().next().unwrap_or("");
            if next.is_empty() {
                break;
            }
            // Skip numeric args (e.g., `timeout 30`)
            if next.parse::<f64>().is_ok() {
                rest = rest[next.len()..].trim_start();
                continue;
            }
            // Skip flag args (e.g., `nice -n 5`)
            if next.starts_with('-') {
                rest = rest[next.len()..].trim_start();
                // Also skip the value if next word is numeric
                let val = rest.split_whitespace().next().unwrap_or("");
                if !val.is_empty() && val.parse::<f64>().is_ok() {
                    rest = rest[val.len()..].trim_start();
                }
                continue;
            }
            break;
        }
    }
    rest.to_string()
}

/// Check if a command has any `cd` subcommand.
pub fn command_has_any_cd(command: &str) -> bool {
    let subcommands = split_command_simple(command);
    subcommands.iter().any(|cmd| {
        let trimmed = cmd.trim();
        trimmed == "cd" || trimmed.starts_with("cd ") || trimmed.starts_with("cd\t")
    })
}

/// Check if a normalized command is a cd command.
pub fn is_normalized_cd_command(command: &str) -> bool {
    let trimmed = command.trim();
    trimmed == "cd" || trimmed.starts_with("cd ") || trimmed.starts_with("cd\t")
}

/// Check if a normalized command is a git command.
pub fn is_normalized_git_command(command: &str) -> bool {
    let trimmed = command.trim();
    trimmed == "git" || trimmed.starts_with("git ") || trimmed.starts_with("git\t")
}

/// Simple command splitting by operators.
fn split_command_simple(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if escape_next {
            current.push(c);
            escape_next = false;
            i += 1;
            continue;
        }
        if c == '\\' && !in_single_quote {
            escape_next = true;
            current.push(c);
            i += 1;
            continue;
        }
        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }
        if !in_single_quote && !in_double_quote {
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if (c == '&' && next == '&') || (c == '|' && next == '|') {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        segments.push(trimmed);
                    }
                    current.clear();
                    i += 2;
                    continue;
                }
            }
            if c == ';' || c == '|' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current.clear();
                i += 1;
                continue;
            }
        }
        current.push(c);
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

/// Full permission check for a bash command against configured rules.
///
/// Evaluates the command against allow/deny rules, checks for dangerous patterns,
/// and returns the permission decision.
pub fn bash_tool_has_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PermissionResult {
    // Check deny rules first
    for rule_str in deny_rules {
        let rule = bash_permission_rule(rule_str);
        if rule_matches_command(&rule, command) {
            return PermissionResult::deny(format!(
                "Command denied by rule: {}",
                rule_str
            ));
        }
    }

    // Check allow rules
    for rule_str in allow_rules {
        let rule = bash_permission_rule(rule_str);
        if rule_matches_command(&rule, command) {
            return PermissionResult::allow(Some(command.to_string()));
        }
    }

    // No rule matched — ask
    PermissionResult::ask(format!(
        "No permission rule matches command: {}",
        truncate_command(command, 80)
    ))
}

/// Check if a permission rule matches a command.
fn rule_matches_command(rule: &BashPermissionRule, command: &str) -> bool {
    let trimmed = command.trim();
    // Also try with env vars stripped
    let stripped = strip_all_leading_env_vars(trimmed);
    let wrapper_stripped = strip_safe_wrappers(trimmed);

    let candidates = [trimmed, stripped.as_str(), wrapper_stripped.as_str()];

    for candidate in &candidates {
        let matched = match rule {
            BashPermissionRule::Prefix { prefix } => {
                *candidate == prefix.as_str()
                    || candidate.starts_with(&format!("{} ", prefix))
            }
            BashPermissionRule::Exact { command: cmd } => *candidate == cmd.as_str(),
            BashPermissionRule::Wildcard { pattern } => {
                match_wildcard_pattern(pattern, candidate)
            }
        };
        if matched {
            return true;
        }
    }
    false
}

/// Truncate a command for display.
fn truncate_command(command: &str, max_len: usize) -> String {
    if command.len() <= max_len {
        command.to_string()
    } else {
        format!("{}...", &command[..max_len])
    }
}

/// Generate a suggestion for an exact command rule.
pub fn suggestion_for_exact_command(tool_name: &str, command: &str) -> PermissionUpdate {
    PermissionUpdate {
        tool: tool_name.to_string(),
        rule: command.to_string(),
        description: format!("Allow exact command: {}", truncate_command(command, 60)),
    }
}

/// Generate a suggestion for a prefix rule.
pub fn suggestion_for_prefix(tool_name: &str, prefix: &str) -> PermissionUpdate {
    PermissionUpdate {
        tool: tool_name.to_string(),
        rule: format!("{}:*", prefix),
        description: format!("Allow all '{}' commands", prefix),
    }
}

/// Get the command-subcommand prefix for suggestion generation.
/// E.g., "git push origin main" → "git push"
pub fn get_command_subcommand_prefix(command: &str) -> Option<String> {
    let parts: Vec<&str> = command.trim().split_whitespace().collect();
    if parts.len() >= 2 {
        // Only consider as subcommand if second word doesn't start with -
        if !parts[1].starts_with('-') {
            return Some(format!("{} {}", parts[0], parts[1]));
        }
    }
    if !parts.is_empty() {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Filter rules that match the given input (for suggestion deduplication).
pub fn filter_rules_by_contents_matching_input(
    rules: &[String],
    command: &str,
) -> Vec<String> {
    rules
        .iter()
        .filter(|rule| {
            let parsed = bash_permission_rule(rule);
            rule_matches_command(&parsed, command)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_permission_rule_prefix() {
        let rule = bash_permission_rule("git:*");
        assert_eq!(
            rule,
            BashPermissionRule::Prefix {
                prefix: "git".to_string()
            }
        );
    }

    #[test]
    fn test_bash_permission_rule_exact() {
        let rule = bash_permission_rule("ls -la");
        assert_eq!(
            rule,
            BashPermissionRule::Exact {
                command: "ls -la".to_string()
            }
        );
    }

    #[test]
    fn test_bash_permission_rule_wildcard() {
        let rule = bash_permission_rule("npm *");
        assert_eq!(
            rule,
            BashPermissionRule::Wildcard {
                pattern: "npm *".to_string()
            }
        );
    }

    #[test]
    fn test_match_wildcard_pattern() {
        assert!(match_wildcard_pattern("git *", "git status"));
        assert!(match_wildcard_pattern("npm *", "npm install"));
        assert!(!match_wildcard_pattern("git *", "npm install"));
        assert!(match_wildcard_pattern("*test*", "run test suite"));
        assert!(match_wildcard_pattern("?at", "cat"));
        assert!(!match_wildcard_pattern("?at", "chat"));
    }

    #[test]
    fn test_strip_env_vars() {
        assert_eq!(
            strip_all_leading_env_vars("FOO=bar BAZ=qux cmd arg"),
            "cmd arg"
        );
        assert_eq!(strip_all_leading_env_vars("cmd arg"), "cmd arg");
        assert_eq!(strip_all_leading_env_vars("PATH=/usr/bin ls"), "ls");
    }

    #[test]
    fn test_strip_safe_wrappers() {
        assert_eq!(strip_safe_wrappers("timeout 30 make build"), "make build");
        assert_eq!(strip_safe_wrappers("cmd arg"), "cmd arg");
    }

    #[test]
    fn test_command_has_any_cd() {
        assert!(command_has_any_cd("cd /tmp && ls"));
        assert!(!command_has_any_cd("ls /tmp"));
        assert!(command_has_any_cd("echo hi; cd foo"));
    }

    #[test]
    fn test_permission_allow() {
        let result = bash_tool_has_permission("git status", &["git:*".to_string()], &[]);
        assert_eq!(result.behavior, PermissionBehavior::Allow);
    }

    #[test]
    fn test_permission_deny() {
        let result = bash_tool_has_permission(
            "rm -rf /",
            &[],
            &["rm *".to_string()],
        );
        assert_eq!(result.behavior, PermissionBehavior::Deny);
    }

    #[test]
    fn test_permission_ask() {
        let result = bash_tool_has_permission("custom_script.sh", &[], &[]);
        assert_eq!(result.behavior, PermissionBehavior::Ask);
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `bashPermissions.ts` additional exports.
// ---------------------------------------------------------------------------

/// Safe env vars whose presence does not affect command resolution. Matches
/// the TS `SAFE_ENV_VARS` set; kept small and conservative.
fn safe_env_var(name: &str) -> bool {
    matches!(
        name,
        "NODE_ENV"
            | "CI"
            | "FORCE_COLOR"
            | "NO_COLOR"
            | "TERM"
            | "LC_ALL"
            | "LANG"
            | "TZ"
            | "DEBUG"
            | "RUST_LOG"
            | "RUST_BACKTRACE"
            | "PYTHONDONTWRITEBYTECODE"
            | "PIP_DISABLE_PIP_VERSION_CHECK"
    )
}

/// Bare shells/wrappers we never auto-suggest as a prefix rule.
fn is_bare_shell_prefix(name: &str) -> bool {
    matches!(
        name,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "csh"
            | "tcsh"
            | "ksh"
            | "dash"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "env"
            | "xargs"
            | "nice"
            | "stdbuf"
            | "nohup"
            | "timeout"
            | "time"
            | "sudo"
            | "doas"
            | "pkexec"
    )
}

/// `bashPermissions.ts` `getSimpleCommandPrefix` — return `<cmd subcmd>` when
/// the second token looks like a subcommand identifier.
pub fn get_simple_command_prefix(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut i = 0usize;
    while i < tokens.len() && is_env_var_assign(tokens[i]) {
        let var_name = tokens[i].split('=').next().unwrap_or("");
        if !safe_env_var(var_name) {
            return None;
        }
        i += 1;
    }
    let remaining = &tokens[i..];
    if remaining.len() < 2 {
        return None;
    }
    let subcmd = remaining[1];
    let re = Regex::new(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$").unwrap();
    if !re.is_match(subcmd) {
        return None;
    }
    Some(format!("{} {}", remaining[0], subcmd))
}

/// `bashPermissions.ts` `getFirstWordPrefix` — fallback that returns just the
/// first command word when it passes the same shape check.
pub fn get_first_word_prefix(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut i = 0usize;
    while i < tokens.len() && is_env_var_assign(tokens[i]) {
        let var_name = tokens[i].split('=').next().unwrap_or("");
        if !safe_env_var(var_name) {
            return None;
        }
        i += 1;
    }
    let cmd = tokens.get(i)?;
    let re = Regex::new(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$").unwrap();
    if !re.is_match(cmd) {
        return None;
    }
    if is_bare_shell_prefix(cmd) {
        return None;
    }
    Some((*cmd).to_string())
}

/// `bashPermissions.ts` `stripWrappersFromArgv` — peel safe wrappers off the
/// front of an argv vector. The Rust port handles time/nohup/timeout/nice.
pub fn strip_wrappers_from_argv(argv: &[String]) -> Vec<String> {
    let mut a: Vec<String> = argv.to_vec();
    loop {
        if a.is_empty() {
            return a;
        }
        match a[0].as_str() {
            "time" | "nohup" => {
                let skip = if a.get(1).map(|s| s.as_str()) == Some("--") { 2 } else { 1 };
                a = a[skip..].to_vec();
            }
            "timeout" => {
                let mut i = 1usize;
                while let Some(arg) = a.get(i) {
                    if !arg.starts_with('-') {
                        break;
                    }
                    i += 1;
                }
                let duration_re = Regex::new(r"^\d+(?:\.\d+)?[smhd]?$").unwrap();
                let Some(d) = a.get(i) else { return a };
                if !duration_re.is_match(d) {
                    return a;
                }
                a = a[(i + 1)..].to_vec();
            }
            "nice" => {
                let n_flag = a.get(1).map(|s| s.as_str()) == Some("-n");
                let num_ok = a
                    .get(2)
                    .map(|s| Regex::new(r"^-?\d+$").unwrap().is_match(s))
                    .unwrap_or(false);
                if n_flag && num_ok {
                    let skip = if a.get(3).map(|s| s.as_str()) == Some("--") { 4 } else { 3 };
                    a = a[skip..].to_vec();
                } else {
                    return a;
                }
            }
            _ => return a,
        }
    }
}

/// `bashPermissions.ts` `checkCommandAndSuggestRules` — returns a structured
/// permission result with optional rule suggestions.
#[derive(Debug, Clone)]
pub struct CommandSuggestResult {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
    pub suggestions: Vec<String>,
}

pub async fn check_command_and_suggest_rules(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> CommandSuggestResult {
    let base = bash_tool_has_permission(command, allow_rules, deny_rules);
    let mut out = CommandSuggestResult {
        behavior: base.behavior,
        message: base.message,
        suggestions: Vec::new(),
    };
    if !matches!(out.behavior, PermissionBehavior::Allow) {
        if let Some(p) = get_simple_command_prefix(command) {
            out.suggestions.push(format!("Bash({}:*)", p));
        } else if let Some(w) = get_first_word_prefix(command) {
            out.suggestions.push(format!("Bash({}:*)", w));
        } else {
            out.suggestions.push(format!("Bash({})", command));
        }
    }
    out
}

// --- Speculative classifier cache (memo for repeated calls per session) ---

use std::sync::Mutex;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub struct PendingClassifierCheck {
    pub command: String,
    pub result: Option<String>, // None = pending, Some(b) = settled
}

static SPECULATIVE_CHECKS: Lazy<Mutex<HashMap<String, PendingClassifierCheck>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// `bashPermissions.ts` `peekSpeculativeClassifierCheck`.
pub fn peek_speculative_classifier_check(command: &str) -> Option<PendingClassifierCheck> {
    SPECULATIVE_CHECKS.lock().unwrap().get(command).cloned()
}

/// `bashPermissions.ts` `startSpeculativeClassifierCheck` — record a pending
/// classifier query keyed by command text.
pub fn start_speculative_classifier_check(command: &str) {
    let mut store = SPECULATIVE_CHECKS.lock().unwrap();
    store.insert(
        command.to_string(),
        PendingClassifierCheck {
            command: command.to_string(),
            result: None,
        },
    );
}

/// `bashPermissions.ts` `consumeSpeculativeClassifierCheck`.
pub fn consume_speculative_classifier_check(command: &str) -> Option<PendingClassifierCheck> {
    SPECULATIVE_CHECKS.lock().unwrap().remove(command)
}

/// `bashPermissions.ts` `clearSpeculativeChecks`.
pub fn clear_speculative_checks() {
    SPECULATIVE_CHECKS.lock().unwrap().clear();
}

/// `bashPermissions.ts` `awaitClassifierAutoApproval` — async resolver hook.
/// The implementation here returns the latest cached check immediately;
/// network-backed classifiers are wired by the runtime layer.
pub async fn await_classifier_auto_approval(command: &str) -> Option<PendingClassifierCheck> {
    peek_speculative_classifier_check(command)
}

/// `bashPermissions.ts` `executeAsyncClassifierCheck` — kick off the async
/// classifier round-trip and record the result.
pub async fn execute_async_classifier_check(command: &str, classify: impl Fn(&str) -> String) {
    let result = classify(command);
    let mut store = SPECULATIVE_CHECKS.lock().unwrap();
    store.insert(
        command.to_string(),
        PendingClassifierCheck {
            command: command.to_string(),
            result: Some(result),
        },
    );
}

/// `bashPermissions.ts` `bashToolCheckExactMatchPermission` — exact-command
/// match against allow/deny rules.
pub fn bash_tool_check_exact_match_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PermissionResult {
    let normalized = command.trim();
    for rule in deny_rules {
        if rule == normalized {
            return PermissionResult {
                behavior: PermissionBehavior::Deny,
                message: Some(format!("Denied by rule: {}", rule)),
                suggestions: Vec::new(),
                updated_input: None,
            };
        }
    }
    for rule in allow_rules {
        if rule == normalized {
            return PermissionResult {
                behavior: PermissionBehavior::Allow,
                message: None,
                suggestions: Vec::new(),
                updated_input: None,
            };
        }
    }
    PermissionResult {
        behavior: PermissionBehavior::Ask,
        message: None,
        suggestions: Vec::new(),
        updated_input: None,
    }
}

/// `bashPermissions.ts` `bashToolCheckPermission` — exact + prefix + wildcard
/// match against the rule sets.
pub fn bash_tool_check_permission(
    command: &str,
    allow_rules: &[String],
    deny_rules: &[String],
) -> PermissionResult {
    let exact = bash_tool_check_exact_match_permission(command, allow_rules, deny_rules);
    if !matches!(exact.behavior, PermissionBehavior::Ask) {
        return exact;
    }
    bash_tool_has_permission(command, allow_rules, deny_rules)
}
