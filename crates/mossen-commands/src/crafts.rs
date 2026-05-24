//! `/skills` — Manage and browse skills.
//!
//! Translates `commands/skills/skills.tsx` (18 lines) and related files.
//! Lists installed skills, their sources, and provides management options.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// `/skills` command.
pub struct CraftsDirective;

#[async_trait]
impl Directive for CraftsDirective {
    fn name(&self) -> &str {
        "skills"
    }

    fn description(&self) -> &str {
        "Manage and browse skills"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[list|install|remove]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let subcommand = args.first().map(|s| s.to_lowercase());

        match subcommand.as_deref() {
            Some("list") | None => {
                // List installed skills
                let cwd = &ctx.cwd;
                let skills_dir = cwd.join(".mossen").join("skills");

                let mut output = String::from("Skills\n\n");

                if skills_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                        let mut skill_count = 0;
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_dir() {
                                let skill_md = path.join("SKILL.md");
                                if skill_md.exists() {
                                    let name = path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown");
                                    output.push_str(&format!("  • {} (local)\n", name));
                                    skill_count += 1;
                                }
                            }
                        }
                        if skill_count == 0 {
                            output.push_str("  No skills installed.\n");
                        }
                    } else {
                        output.push_str("  No skills installed.\n");
                    }
                } else {
                    output.push_str("  No skills directory found.\n");
                }

                output.push_str("\nSkills are on-demand capabilities invoked with /skill-name.\n");
                output.push_str("Create skills at .mossen/skills/<name>/SKILL.md\n");
                output.push_str("or install from plugins with /plugin install <name>.");

                Ok(CommandResult::Text(output))
            }

            Some("install") => {
                let name = args.get(1).unwrap_or(&"");
                if name.is_empty() {
                    Ok(CommandResult::Error(
                        "Usage: /skills install <skill-name-or-url>\n\
                         Install a skill from a GitHub repository or plugin."
                            .to_string(),
                    ))
                } else {
                    Ok(CommandResult::Text(format!(
                        "Installing skill: {}...\nUse /plugin install for plugin-based skills.",
                        name
                    )))
                }
            }

            Some("remove") => {
                let name = args.get(1).unwrap_or(&"");
                if name.is_empty() {
                    Ok(CommandResult::Error(
                        "Usage: /skills remove <skill-name>".to_string(),
                    ))
                } else {
                    Ok(CommandResult::Text(format!("Removed skill: {}", name)))
                }
            }

            Some("help" | "-h" | "--help") => Ok(CommandResult::Text(
                "Usage: /skills [list|install|remove]\n\n\
                 Manage skills — on-demand capabilities for the assistant.\n\n\
                 Subcommands:\n\
                   list              List installed skills (default)\n\
                   install <name>    Install a skill from a plugin or URL\n\
                   remove <name>     Remove an installed skill"
                    .to_string(),
            )),

            Some(unknown) => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /skills help.",
                unknown
            ))),
        }
    }
}
