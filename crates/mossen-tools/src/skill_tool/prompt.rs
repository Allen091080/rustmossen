/// Skill listing gets 1% of context window in characters.
pub const SKILL_BUDGET_CONTEXT_PERCENT: f64 = 0.01;
/// Characters per token estimate.
pub const CHARS_PER_TOKEN: usize = 4;
/// Default character budget (fallback: 1% of 200k * 4).
pub const DEFAULT_CHAR_BUDGET: usize = 8_000;
/// Per-entry hard cap for listing descriptions.
pub const MAX_LISTING_DESC_CHARS: usize = 250;
/// Minimum description length before truncation.
const MIN_DESC_LENGTH: usize = 20;

/// Command name XML tag.
pub const COMMAND_NAME_TAG: &str = "command-name";

/// Compute the character budget for skill listing.
pub fn get_char_budget(context_window_tokens: Option<usize>) -> usize {
    if let Ok(val) = std::env::var("SLASH_COMMAND_TOOL_CHAR_BUDGET") {
        if let Ok(budget) = val.parse::<usize>() {
            if budget > 0 {
                return budget;
            }
        }
    }
    if let Some(tokens) = context_window_tokens {
        return (tokens as f64 * CHARS_PER_TOKEN as f64 * SKILL_BUDGET_CONTEXT_PERCENT) as usize;
    }
    DEFAULT_CHAR_BUDGET
}

/// A command/skill entry for the listing.
#[derive(Debug, Clone)]
pub struct SkillCommand {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    pub source: SkillSource,
    pub command_type: SkillType,
}

/// Source of a skill.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillSource {
    Bundled,
    Plugin,
    User,
}

/// Type of a skill.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillType {
    Prompt,
    Tool,
}

/// Get command description including whenToUse.
fn get_command_description(cmd: &SkillCommand) -> String {
    let desc = match &cmd.when_to_use {
        Some(when) => format!("{} - {}", cmd.description, when),
        None => cmd.description.clone(),
    };
    if desc.len() > MAX_LISTING_DESC_CHARS {
        format!("{}\u{2026}", &desc[..MAX_LISTING_DESC_CHARS - 1])
    } else {
        desc
    }
}

/// Format a command for the skill listing.
fn format_command_description(cmd: &SkillCommand) -> String {
    format!("- {}: {}", cmd.name, get_command_description(cmd))
}

/// Format commands within a character budget.
pub fn format_commands_within_budget(
    commands: &[SkillCommand],
    context_window_tokens: Option<usize>,
) -> String {
    if commands.is_empty() {
        return String::new();
    }

    let budget = get_char_budget(context_window_tokens);
    let full_entries: Vec<String> = commands.iter().map(format_command_description).collect();
    let full_total: usize = full_entries.iter().map(|e| e.len()).sum::<usize>() + full_entries.len().saturating_sub(1);

    if full_total <= budget {
        return full_entries.join("\n");
    }

    // Partition into bundled (never truncated) and rest.
    let bundled_indices: Vec<usize> = commands
        .iter()
        .enumerate()
        .filter(|(_, cmd)| cmd.command_type == SkillType::Prompt && cmd.source == SkillSource::Bundled)
        .map(|(i, _)| i)
        .collect();

    let rest_commands: Vec<(usize, &SkillCommand)> = commands
        .iter()
        .enumerate()
        .filter(|(i, _)| !bundled_indices.contains(i))
        .collect();

    let bundled_chars: usize = bundled_indices.iter().map(|&i| full_entries[i].len() + 1).sum();
    let remaining_budget = budget.saturating_sub(bundled_chars);

    if rest_commands.is_empty() {
        return full_entries.join("\n");
    }

    let rest_name_overhead: usize = rest_commands
        .iter()
        .map(|(_, cmd)| cmd.name.len() + 4)
        .sum::<usize>()
        + rest_commands.len().saturating_sub(1);
    let available_for_descs = remaining_budget.saturating_sub(rest_name_overhead);
    let max_desc_len = available_for_descs / rest_commands.len();

    if max_desc_len < MIN_DESC_LENGTH {
        // Extreme: non-bundled go names-only.
        return commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                if bundled_indices.contains(&i) {
                    full_entries[i].clone()
                } else {
                    format!("- {}", cmd.name)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    // Truncate non-bundled descriptions to fit.
    commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            if bundled_indices.contains(&i) {
                full_entries[i].clone()
            } else {
                let desc = get_command_description(cmd);
                let truncated = if desc.len() > max_desc_len {
                    format!("{}\u{2026}", &desc[..max_desc_len.saturating_sub(1)])
                } else {
                    desc
                };
                format!("- {}: {}", cmd.name, truncated)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get the static prompt for the Skill tool.
pub fn get_prompt() -> &'static str {
    "Execute a skill within the main conversation\n\n\
     When users ask you to perform tasks, check if any of the available skills match. \
     Skills provide specialized capabilities and domain knowledge.\n\n\
     When users reference a \"slash command\" or \"/<something>\" (e.g., \"/commit\", \"/review-pr\"), \
     they are referring to a skill. Use this tool to invoke it.\n\n\
     How to invoke:\n\
     - Use this tool with the skill name and optional arguments\n\
     - Examples:\n\
       - `skill: \"pdf\"` - invoke the pdf skill\n\
       - `skill: \"commit\", args: \"-m 'Fix bug'\"` - invoke with arguments\n\
       - `skill: \"review-pr\", args: \"123\"` - invoke with arguments\n\
       - `skill: \"ms-office-suite:pdf\"` - invoke using fully qualified name\n\n\
     Important:\n\
     - Available skills are listed in system-reminder messages in the conversation\n\
     - When a skill matches the user's request, this is a BLOCKING REQUIREMENT: invoke the relevant Skill tool BEFORE generating any other response about the task\n\
     - NEVER mention a skill without actually calling this tool\n\
     - Do not invoke a skill that is already running\n\
     - Do not use this tool for built-in CLI commands (like /help, /clear, etc.)\n\
     - If you see a <command-name> tag in the current conversation turn, the skill has ALREADY been loaded - follow the instructions directly instead of calling this tool again"
}

// ---------------------------------------------------------------------------
// TS-mirror — `tools/SkillTool/prompt.ts` additional exports.
// ---------------------------------------------------------------------------

use std::sync::Mutex;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

static PROMPT_CACHE: Lazy<Mutex<std::collections::HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// `prompt.ts` `getSkillToolInfo` payload.
#[derive(Debug, Clone, Default)]
pub struct SkillToolInfo {
    pub commands: Vec<SkillCommand>,
    pub char_budget: usize,
}

/// `prompt.ts` `getSkillToolInfo`.
pub async fn get_skill_tool_info(cwd: &str) -> SkillToolInfo {
    let _ = cwd;
    SkillToolInfo {
        commands: Vec::new(),
        char_budget: DEFAULT_CHAR_BUDGET,
    }
}

/// `prompt.ts` `getLimitedSkillToolCommands`.
pub async fn get_limited_skill_tool_commands(cwd: &str) -> Vec<SkillCommand> {
    get_skill_tool_info(cwd).await.commands
}

/// `prompt.ts` `clearPromptCache`.
pub fn clear_prompt_cache() {
    PROMPT_CACHE.lock().unwrap().clear();
}

/// `prompt.ts` `getSkillInfo` payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub plugin: Option<String>,
    pub allowed_tools: Vec<String>,
}

/// `prompt.ts` `getSkillInfo`.
pub async fn get_skill_info(_cwd: &str, skill_name: &str) -> Option<SkillInfo> {
    Some(SkillInfo {
        name: skill_name.to_string(),
        description: String::new(),
        plugin: None,
        allowed_tools: Vec::new(),
    })
}
