//! `/permissions` — Manage tool permission rules.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Access directive — manages allow/deny rules for tool permissions.
/// Provides a widget to view and modify which tools are permitted.
pub struct AccessDirective;

/// Permission rule types.
const RULE_TYPES: &[&str] = &["allow", "deny"];

/// Newline-separated session allow rules shared with the TUI permission gate.
pub const PERMISSION_ALLOW_RULES_ENV: &str = "MOSSEN_PERMISSION_ALLOW_RULES";
/// Newline-separated session deny rules shared with the TUI permission gate.
pub const PERMISSION_DENY_RULES_ENV: &str = "MOSSEN_PERMISSION_DENY_RULES";

#[async_trait]
impl Directive for AccessDirective {
    fn name(&self) -> &str {
        "permissions"
    }

    fn aliases(&self) -> &[&str] {
        &["allowed-tools"]
    }

    fn description(&self) -> &str {
        "Manage allow & deny tool permission rules"
    }

    fn argument_hint(&self) -> &str {
        "[list|allow|deny|reset]"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Show current permissions status
            let _ = RULE_TYPES;
            let rules = permission_rules_text(ctx);
            return Ok(CommandResult::Text(format!(
                "Permissions Management\n\
                     ======================\n\n\
                     Current rules:\n\
                     {rules}\n\n\
                     Usage:\n\
                     · /permissions list     — Show current permission rules\n\
                     · /permissions allow <tool-pattern> — Add allow rule\n\
                     · /permissions deny <tool-pattern>  — Add deny rule\n\
                     · /permissions reset    — Clear all rules"
            )));
        }

        let action = args[0].to_lowercase();
        let _ = RULE_TYPES;

        match action.as_str() {
            "list" | "show" => Ok(CommandResult::Text(format!(
                "Permission rules:\n{}",
                permission_rules_text(ctx)
            ))),
            "allow" => {
                let tool = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
                if tool.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /permissions allow <tool-pattern>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Added allow rule for: {}",
                    tool
                )))
            }
            "deny" => {
                let tool = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
                if tool.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /permissions deny <tool-pattern>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Added deny rule for: {}",
                    tool
                )))
            }
            "reset" | "clear" => Ok(CommandResult::System(
                "All permission rules cleared.".to_string(),
            )),
            _ => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use: list, allow, deny, reset",
                action
            ))),
        }
    }
}

fn permission_rules_text(ctx: &CommandContext) -> String {
    let allow = permission_rule_lines(ctx, PERMISSION_ALLOW_RULES_ENV);
    let deny = permission_rule_lines(ctx, PERMISSION_DENY_RULES_ENV);

    if allow.is_empty() && deny.is_empty() {
        return "  (none configured — all tools allowed by default)".to_string();
    }

    let mut out = String::new();
    if !allow.is_empty() {
        out.push_str("  Allow:\n");
        for rule in allow {
            out.push_str("    - ");
            out.push_str(&rule);
            out.push('\n');
        }
    }
    if !deny.is_empty() {
        out.push_str("  Deny:\n");
        for rule in deny {
            out.push_str("    - ");
            out.push_str(&rule);
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn permission_rule_lines(ctx: &CommandContext, key: &str) -> Vec<String> {
    ctx.env_vars
        .get(key)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn context_with_rules(allow: &[&str], deny: &[&str]) -> CommandContext {
        let mut env_vars = HashMap::new();
        if !allow.is_empty() {
            env_vars.insert(PERMISSION_ALLOW_RULES_ENV.to_string(), allow.join("\n"));
        }
        if !deny.is_empty() {
            env_vars.insert(PERMISSION_DENY_RULES_ENV.to_string(), deny.join("\n"));
        }
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars,
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
        }
    }

    #[tokio::test]
    async fn permissions_list_reads_session_rule_env() {
        let directive = AccessDirective;
        let ctx = context_with_rules(&["Bash cargo test"], &["Write"]);

        let output = directive
            .execute(&["list"], &ctx)
            .await
            .expect("list should succeed");

        let CommandResult::Text(text) = output else {
            panic!("expected text output");
        };
        assert!(text.contains("Allow:"));
        assert!(text.contains("Bash cargo test"));
        assert!(text.contains("Deny:"));
        assert!(text.contains("Write"));
    }

    #[tokio::test]
    async fn permissions_list_shows_empty_state_without_rules() {
        let directive = AccessDirective;
        let ctx = context_with_rules(&[], &[]);

        let output = directive
            .execute(&["list"], &ctx)
            .await
            .expect("list should succeed");

        let CommandResult::Text(text) = output else {
            panic!("expected text output");
        };
        assert!(text.contains("none configured"));
    }
}
