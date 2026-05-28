//! Prompt generation for the Bash tool.
//!
//! Corresponds to `prompt.ts` (370 lines). Generates the system prompt description
//! for the Bash tool, including usage instructions, sandbox configuration,
//! and git commit/PR guidelines.

use crate::bash_tool::tool_name::BASH_TOOL_NAME;

/// Default timeout in milliseconds.
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
/// Maximum timeout in milliseconds.
const MAX_TIMEOUT_MS: u64 = 600_000;

/// Get the default timeout in milliseconds.
pub fn get_default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// Get the maximum timeout in milliseconds.
pub fn get_max_timeout_ms() -> u64 {
    MAX_TIMEOUT_MS
}

/// Configuration for prompt generation.
pub struct PromptConfig {
    pub user_type: String,
    pub is_undercover: bool,
    pub include_git_instructions: bool,
    pub has_embedded_search_tools: bool,
    pub is_simple_mode: bool,
    pub disable_background_tasks: bool,
    pub sandboxing_enabled: bool,
    pub sandbox_config: Option<SandboxPromptConfig>,
}

/// Sandbox configuration for prompt display.
pub struct SandboxPromptConfig {
    pub fs_read_deny_only: Vec<String>,
    pub fs_write_allow_only: Vec<String>,
    pub fs_write_deny_within_allow: Vec<String>,
    pub network_allowed_hosts: Vec<String>,
    pub network_denied_hosts: Vec<String>,
    pub allow_unsandboxed_commands: bool,
}

/// Get the background usage note.
fn get_background_usage_note(disable_background_tasks: bool) -> Option<String> {
    if disable_background_tasks {
        return None;
    }
    Some("You can use the `run_in_background` parameter to run the command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later. You do not need to check the output right away - you'll be notified when it finishes. You do not need to use '&' at the end of the command when using this parameter.".to_string())
}

/// Get sandbox section for the prompt.
fn get_sandbox_section(config: &PromptConfig) -> String {
    if !config.sandboxing_enabled {
        return String::new();
    }

    let sandbox = match &config.sandbox_config {
        Some(s) => s,
        None => return String::new(),
    };

    let mut restrictions = Vec::new();

    // Filesystem restrictions
    let mut fs_config = String::from("Filesystem: {");
    if !sandbox.fs_write_allow_only.is_empty() {
        fs_config.push_str(&format!(
            "\"write\": {{\"allowOnly\": {:?}}}",
            sandbox.fs_write_allow_only
        ));
    }
    fs_config.push('}');
    restrictions.push(fs_config);

    // Network restrictions
    if !sandbox.network_allowed_hosts.is_empty() || !sandbox.network_denied_hosts.is_empty() {
        let mut net = String::from("Network: {");
        if !sandbox.network_allowed_hosts.is_empty() {
            net.push_str(&format!(
                "\"allowedHosts\": {:?}",
                sandbox.network_allowed_hosts
            ));
        }
        net.push('}');
        restrictions.push(net);
    }

    let sandbox_override = if sandbox.allow_unsandboxed_commands {
        vec![
            "You should always default to running commands within the sandbox. Do NOT attempt to set `dangerouslyDisableSandbox: true` unless:",
            "- The user *explicitly* asks you to bypass sandbox",
            "- A specific command just failed and you see evidence of sandbox restrictions causing the failure.",
            "When you see evidence of sandbox-caused failure:",
            "- Immediately retry with `dangerouslyDisableSandbox: true`",
            "- Briefly explain what sandbox restriction likely caused the failure.",
        ]
    } else {
        vec![
            "All commands MUST run in sandbox mode - the `dangerouslyDisableSandbox` parameter is disabled by policy.",
            "Commands cannot run outside the sandbox under any circumstances.",
        ]
    };

    let mut section = String::new();
    section.push_str("\n## Command sandbox\n");
    section.push_str("By default, your command will be run in a sandbox. This sandbox controls which directories and network hosts commands may access or modify without an explicit override.\n\n");
    section.push_str("The sandbox has the following restrictions:\n");
    section.push_str(&restrictions.join("\n"));
    section.push('\n');
    for line in &sandbox_override {
        section.push_str(line);
        section.push('\n');
    }
    section.push_str("For temporary files, always use the `$TMPDIR` environment variable.\n");

    section
}

/// Get git commit and PR instructions.
fn get_commit_and_pr_instructions(config: &PromptConfig) -> String {
    if !config.include_git_instructions {
        return String::new();
    }

    if config.user_type == "mossen" {
        let skills_section = if !config.is_simple_mode {
            "For git commits and pull requests, use the `/commit` and `/commit-push-pr` skills:\n\
                 - `/commit` - Create a git commit with staged changes\n\
                 - `/commit-push-pr` - Commit, push, and create a pull request\n\n\
                 These skills handle git safety protocols, proper commit message formatting, and PR creation.\n".to_string()
        } else {
            String::new()
        };

        format!(
            "# Git operations\n\n\
             {}\
             IMPORTANT: NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it.\n\n\
             Use the gh command via the Bash tool for other GitHub-related tasks including working with issues, checks, and releases.\n\n\
             # Other common operations\n\
             - View comments on a Github PR: gh api repos/foo/bar/pulls/123/comments",
            skills_section
        )
    } else {
        "# Committing changes with git\n\n\
             Only create commits when requested by the user. If unclear, ask first.\n\n\
             Git Safety Protocol:\n\
             - NEVER update the git config\n\
             - NEVER run destructive git commands (push --force, reset --hard, checkout ., restore ., clean -f, branch -D) unless the user explicitly requests these actions\n\
             - NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it\n\
             - NEVER run force push to main/master, warn the user if they request it\n\
             - CRITICAL: Always create NEW commits rather than amending, unless the user explicitly requests a git amend\n\
             - When staging files, prefer adding specific files by name rather than using \"git add -A\"\n\
             - NEVER commit changes unless the user explicitly asks you to\n\n\
             # Creating pull requests\n\
             Use the gh command via the Bash tool for ALL GitHub-related tasks.".to_string()
    }
}

/// Generate the complete system prompt for the Bash tool.
pub fn get_simple_prompt(config: &PromptConfig) -> String {
    let embedded = config.has_embedded_search_tools;

    let tool_preference_items = if embedded {
        vec![
            "Read files: Use Read (NOT cat/head/tail)".to_string(),
            "Edit files: Use Edit (NOT sed/awk)".to_string(),
            "Write files: Use Write (NOT echo >/cat <<EOF)".to_string(),
            "Communication: Output text directly (NOT echo/printf)".to_string(),
        ]
    } else {
        vec![
            "File search: Use Glob (NOT find or ls)".to_string(),
            "Content search: Use Grep (NOT grep or rg)".to_string(),
            "Read files: Use Read (NOT cat/head/tail)".to_string(),
            "Edit files: Use Edit (NOT sed/awk)".to_string(),
            "Write files: Use Write (NOT echo >/cat <<EOF)".to_string(),
            "Communication: Output text directly (NOT echo/printf)".to_string(),
        ]
    };

    let avoid_commands = if embedded {
        "`cat`, `head`, `tail`, `sed`, `awk`, or `echo`"
    } else {
        "`find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo`"
    };

    let background_note = get_background_usage_note(config.disable_background_tasks);
    let sandbox_section = get_sandbox_section(config);
    let git_section = get_commit_and_pr_instructions(config);

    let mut prompt = String::new();
    prompt.push_str("Executes a given bash command and returns its output.\n\n");
    prompt.push_str("The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh).\n\n");
    prompt.push_str(&format!(
        "IMPORTANT: Avoid using this tool to run {} commands, unless explicitly instructed. Instead, use the appropriate dedicated tool:\n\n",
        avoid_commands
    ));

    for item in &tool_preference_items {
        prompt.push_str(&format!("- {}\n", item));
    }

    prompt.push_str(&format!(
        "\nWhile the {} tool can do similar things, it's better to use the built-in tools as they provide a better user experience.\n\n",
        BASH_TOOL_NAME
    ));

    prompt.push_str("# Instructions\n");
    prompt.push_str("- If your command will create new directories or files, first use this tool to run `ls` to verify the parent directory exists.\n");
    prompt.push_str("- Always quote file paths that contain spaces with double quotes.\n");
    prompt.push_str("- Try to maintain your current working directory throughout the session by using absolute paths.\n");
    prompt.push_str(&format!(
        "- You may specify an optional timeout in milliseconds (up to {}ms / {} minutes). Default: {}ms ({} minutes).\n",
        get_max_timeout_ms(),
        get_max_timeout_ms() / 60000,
        get_default_timeout_ms(),
        get_default_timeout_ms() / 60000
    ));

    if let Some(note) = &background_note {
        prompt.push_str(&format!("- {}\n", note));
    }

    prompt.push_str("- When issuing multiple commands:\n");
    prompt.push_str(&format!(
        "  - If independent: make multiple {} tool calls in parallel.\n",
        BASH_TOOL_NAME
    ));
    prompt.push_str("  - If dependent: use '&&' to chain them together.\n");
    prompt.push_str("  - Use ';' only when you need sequential execution but don't care about earlier failures.\n");
    prompt.push_str("  - DO NOT use newlines to separate commands.\n");
    prompt.push_str("- For git commands:\n");
    prompt.push_str("  - Prefer to create a new commit rather than amending an existing commit.\n");
    prompt.push_str(
        "  - Never skip hooks (--no-verify) or bypass signing unless explicitly asked.\n",
    );
    prompt.push_str("- Avoid unnecessary `sleep` commands.\n");

    if !sandbox_section.is_empty() {
        prompt.push_str(&sandbox_section);
    }

    if !git_section.is_empty() {
        prompt.push('\n');
        prompt.push_str(&git_section);
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_prompt_uses_registered_tool_names() {
        let prompt = get_simple_prompt(&PromptConfig {
            user_type: "external".to_string(),
            is_undercover: false,
            include_git_instructions: false,
            has_embedded_search_tools: false,
            is_simple_mode: false,
            disable_background_tasks: false,
            sandboxing_enabled: false,
            sandbox_config: None,
        });

        for expected in ["Use Read", "Use Edit", "Use Write"] {
            assert!(prompt.contains(expected), "{prompt}");
        }
        for stale in ["FileRead", "FileEdit", "FileWrite"] {
            assert!(!prompt.contains(stale), "{prompt}");
        }
    }
}
