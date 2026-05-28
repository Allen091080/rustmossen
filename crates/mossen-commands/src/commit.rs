//! `/commit` — Create a git commit (prompt command).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Commit directive — generates a prompt for the model to create a git commit.
pub struct CommitDirective;

/// Allowed tools for commit operations.
const ALLOWED_TOOLS: &[&str] = &[
    "Bash(git add:*)",
    "Bash(git status:*)",
    "Bash(git commit:*)",
];

/// Check if the user is an internal user for commit purposes.
fn is_commit_internal_user(ctx: &CommandContext) -> bool {
    ctx.is_internal_user()
}

/// Check if undercover mode is active.
fn is_undercover(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .get("MOSSEN_UNDERCOVER")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Get undercover instructions for hiding attribution.
fn get_undercover_instructions() -> &'static str {
    "IMPORTANT: You are operating in undercover mode. Do NOT include any attribution, \
     co-authored-by lines, or any mention of AI assistance in the commit message. \
     The commit should appear as if written entirely by the human developer."
}

/// Get attribution text for commit messages.
fn get_commit_attribution(ctx: &CommandContext) -> String {
    if is_undercover(ctx) {
        return String::new();
    }
    let product_name = &ctx.product_name;
    format!("Co-authored-by: {} <noreply@mossen.dev>", product_name)
}

/// Generate the commit prompt content.
fn get_prompt_content(ctx: &CommandContext) -> String {
    let commit_attribution = get_commit_attribution(ctx);

    let mut prefix = String::new();
    if is_commit_internal_user(ctx) && is_undercover(ctx) {
        prefix = format!("{}\n", get_undercover_instructions());
    }

    let attribution_example = if !commit_attribution.is_empty() {
        format!("\n\n{}", commit_attribution)
    } else {
        String::new()
    };

    format!(
        r#"{prefix}## Context

- Current git status: !`git status`
- Current git diff (staged and unstaged changes): !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`

## Git Safety Protocol

- NEVER update the git config
- NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it
- CRITICAL: ALWAYS create NEW commits. NEVER use git commit --amend, unless the user explicitly requests it
- Do not commit files that likely contain secrets (.env, credentials.json, etc). Warn the user if they specifically request to commit those files
- If there are no changes to commit (i.e., no untracked files and no modifications), do not create an empty commit
- Never use git commands with the -i flag (like git rebase -i or git add -i) since they require interactive input which is not supported

## Your task

Based on the above changes, create a single git commit:

1. Analyze all staged changes and draft a commit message:
   - Look at the recent commits above to follow this repository's commit message style
   - Summarize the nature of the changes (new feature, enhancement, bug fix, refactoring, test, docs, etc.)
   - Ensure the message accurately reflects the changes and their purpose (i.e. "add" means a wholly new feature, "update" means an enhancement to an existing feature, "fix" means a bug fix, etc.)
   - Draft a concise (1-2 sentences) commit message that focuses on the "why" rather than the "what"

2. Stage relevant files and create the commit using HEREDOC syntax:
```
git commit -m "$(cat <<'EOF'
Commit message here.{attribution_example}
EOF
)"
```

You have the capability to call multiple tools in a single response. Stage and create the commit using a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls."#
    )
}

#[async_trait]
impl Directive for CommitDirective {
    fn name(&self) -> &str {
        "commit"
    }

    fn description(&self) -> &str {
        "Create a git commit"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        ""
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let prompt_content = get_prompt_content(ctx);
        Ok(CommandResult::Text(prompt_content))
    }
}

/// Get the list of allowed tools for the commit command.
pub fn commit_allowed_tools() -> &'static [&'static str] {
    ALLOWED_TOOLS
}
