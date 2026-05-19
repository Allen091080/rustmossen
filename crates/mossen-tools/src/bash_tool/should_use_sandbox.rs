//! Determines whether a bash command should run inside the sandbox.
//!
//! Corresponds to `shouldUseSandbox.ts` — checks sandbox configuration,
//! user overrides, and excluded-command patterns.

use crate::bash_tool::bash_permissions::{
    bash_permission_rule, match_wildcard_pattern, strip_all_leading_env_vars,
    strip_safe_wrappers, BashPermissionRule, BINARY_HIJACK_VARS,
};
use std::collections::HashSet;

/// Input for sandbox determination.
pub struct SandboxInput {
    pub command: Option<String>,
    pub dangerously_disable_sandbox: bool,
}

/// Sandbox configuration (injected from higher layers).
pub trait SandboxConfig: Send + Sync {
    fn is_sandboxing_enabled(&self) -> bool;
    fn are_unsandboxed_commands_allowed(&self) -> bool;
    fn get_excluded_commands(&self) -> Vec<String>;
    fn get_disabled_commands_config(&self) -> DisabledCommandsConfig;
    fn get_user_type(&self) -> String;
}

/// Dynamic config for disabled commands (Mossen-only).
pub struct DisabledCommandsConfig {
    pub commands: Vec<String>,
    pub substrings: Vec<String>,
}

/// Split command by `&&`, `||`, `;` (deprecated simple split).
fn split_command_deprecated(command: &str) -> Vec<String> {
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
            if c == ';' {
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

/// Check if a command contains an excluded command pattern.
/// NOTE: excludedCommands is a user-facing convenience feature, not a security boundary.
fn contains_excluded_command(command: &str, config: &dyn SandboxConfig) -> bool {
    let user_type = config.get_user_type();

    // Check dynamic config for disabled commands and substrings (Mossen-only)
    if user_type == "mossen" {
        let disabled = config.get_disabled_commands_config();

        // Check if command contains any disabled substrings
        for substring in &disabled.substrings {
            if command.contains(substring.as_str()) {
                return true;
            }
        }

        // Check if command starts with any disabled commands
        if let Ok(parts) = std::panic::catch_unwind(|| split_command_deprecated(command)) {
            for part in &parts {
                let base_command = part.trim().split(' ').next().unwrap_or("");
                if !base_command.is_empty() && disabled.commands.contains(&base_command.to_string())
                {
                    return true;
                }
            }
        }
    }

    // Check user-configured excluded commands from settings
    let user_excluded_commands = config.get_excluded_commands();
    if user_excluded_commands.is_empty() {
        return false;
    }

    // Split compound commands and check each one
    let subcommands = match std::panic::catch_unwind(|| split_command_deprecated(command)) {
        Ok(cmds) => cmds,
        Err(_) => vec![command.to_string()],
    };

    for subcommand in &subcommands {
        let trimmed = subcommand.trim();

        // Iteratively strip env vars and wrapper commands (fixed-point)
        let mut candidates = vec![trimmed.to_string()];
        let mut seen: HashSet<String> = candidates.iter().cloned().collect();
        let mut start_idx = 0;

        while start_idx < candidates.len() {
            let end_idx = candidates.len();
            for i in start_idx..end_idx {
                let cmd = candidates[i].clone();

                let env_stripped = strip_all_leading_env_vars(&cmd);
                if !seen.contains(&env_stripped) {
                    candidates.push(env_stripped.clone());
                    seen.insert(env_stripped);
                }

                let wrapper_stripped = strip_safe_wrappers(&cmd);
                if !seen.contains(&wrapper_stripped) {
                    candidates.push(wrapper_stripped.clone());
                    seen.insert(wrapper_stripped);
                }
            }
            start_idx = end_idx;
        }

        for pattern in &user_excluded_commands {
            let rule = bash_permission_rule(pattern);
            for cand in &candidates {
                match &rule {
                    BashPermissionRule::Prefix { prefix } => {
                        if cand == prefix || cand.starts_with(&format!("{} ", prefix)) {
                            return true;
                        }
                    }
                    BashPermissionRule::Exact { command } => {
                        if cand == command {
                            return true;
                        }
                    }
                    BashPermissionRule::Wildcard { pattern } => {
                        if match_wildcard_pattern(pattern, cand) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Determine if a command should use the sandbox.
pub fn should_use_sandbox(input: &SandboxInput, config: &dyn SandboxConfig) -> bool {
    if !config.is_sandboxing_enabled() {
        return false;
    }

    // Don't sandbox if explicitly overridden AND unsandboxed commands are allowed
    if input.dangerously_disable_sandbox && config.are_unsandboxed_commands_allowed() {
        return false;
    }

    let command = match &input.command {
        Some(cmd) => cmd,
        None => return false,
    };

    // Don't sandbox if the command contains user-configured excluded commands
    if contains_excluded_command(command, config) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSandboxConfig {
        enabled: bool,
        unsandboxed_allowed: bool,
        excluded: Vec<String>,
    }

    impl SandboxConfig for MockSandboxConfig {
        fn is_sandboxing_enabled(&self) -> bool {
            self.enabled
        }
        fn are_unsandboxed_commands_allowed(&self) -> bool {
            self.unsandboxed_allowed
        }
        fn get_excluded_commands(&self) -> Vec<String> {
            self.excluded.clone()
        }
        fn get_disabled_commands_config(&self) -> DisabledCommandsConfig {
            DisabledCommandsConfig {
                commands: vec![],
                substrings: vec![],
            }
        }
        fn get_user_type(&self) -> String {
            "external".to_string()
        }
    }

    #[test]
    fn test_sandbox_disabled() {
        let config = MockSandboxConfig {
            enabled: false,
            unsandboxed_allowed: false,
            excluded: vec![],
        };
        let input = SandboxInput {
            command: Some("ls".to_string()),
            dangerously_disable_sandbox: false,
        };
        assert!(!should_use_sandbox(&input, &config));
    }

    #[test]
    fn test_sandbox_enabled_normal() {
        let config = MockSandboxConfig {
            enabled: true,
            unsandboxed_allowed: false,
            excluded: vec![],
        };
        let input = SandboxInput {
            command: Some("make build".to_string()),
            dangerously_disable_sandbox: false,
        };
        assert!(should_use_sandbox(&input, &config));
    }

    #[test]
    fn test_sandbox_override_allowed() {
        let config = MockSandboxConfig {
            enabled: true,
            unsandboxed_allowed: true,
            excluded: vec![],
        };
        let input = SandboxInput {
            command: Some("make build".to_string()),
            dangerously_disable_sandbox: true,
        };
        assert!(!should_use_sandbox(&input, &config));
    }

    #[test]
    fn test_excluded_command() {
        let config = MockSandboxConfig {
            enabled: true,
            unsandboxed_allowed: false,
            excluded: vec!["docker".to_string()],
        };
        let input = SandboxInput {
            command: Some("docker ps".to_string()),
            dangerously_disable_sandbox: false,
        };
        assert!(!should_use_sandbox(&input, &config));
    }
}
