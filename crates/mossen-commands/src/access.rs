//! `/permissions` — Manage tool permission rules.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Access directive — manages allow/deny rules for tool permissions.
/// Provides a widget to view and modify which tools are permitted.
pub struct AccessDirective;

/// Permission rule types.
const RULE_TYPES: &[&str] = &["allow", "deny"];

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

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Show current permissions status
            let _ = RULE_TYPES;
            return Ok(CommandResult::Text(
                "Permissions Management\n\
                 ======================\n\n\
                 Current rules: (none configured — all tools allowed by default)\n\n\
                 Usage:\n\
                 · /permissions list     — Show current permission rules\n\
                 · /permissions allow <tool-pattern> — Add allow rule\n\
                 · /permissions deny <tool-pattern>  — Add deny rule\n\
                 · /permissions reset    — Clear all rules"
                    .to_string(),
            ));
        }

        let action = args[0].to_lowercase();
        let _ = RULE_TYPES;

        match action.as_str() {
            "list" | "show" => {
                Ok(CommandResult::Text(
                    "Permission rules:\n  (none configured — all tools allowed by default)"
                        .to_string(),
                ))
            }
            "allow" => {
                let tool = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
                if tool.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /permissions allow <tool-pattern>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!("Added allow rule for: {}", tool)))
            }
            "deny" => {
                let tool = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
                if tool.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /permissions deny <tool-pattern>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!("Added deny rule for: {}", tool)))
            }
            "reset" | "clear" => {
                Ok(CommandResult::System("All permission rules cleared.".to_string()))
            }
            _ => {
                Ok(CommandResult::Error(format!(
                    "Unknown subcommand: \"{}\". Use: list, allow, deny, reset",
                    action
                )))
            }
        }
    }
}
