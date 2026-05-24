//! `/commit-push-pr` (alias: `/ship`) — Commit, push, and open a PR (prompt command).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Ship directive — generates a prompt to commit, push, and open a PR.
pub struct ShipDirective;

/// Allowed tools for the ship/commit-push-pr command.
const ALLOWED_TOOLS: &[&str] = &[
    "Bash(git checkout --branch:*)",
    "Bash(git checkout -b:*)",
    "Bash(git add:*)",
    "Bash(git status:*)",
    "Bash(git push:*)",
    "Bash(git commit:*)",
    "Bash(gh pr create:*)",
    "Bash(gh pr edit:*)",
    "Bash(gh pr view:*)",
    "Bash(gh pr merge:*)",
    "ToolSearch",
    "mcp__slack__send_message",
    "mcp__hosted_Slack__slack_send_message",
];

/// Check if undercover mode is active.
fn is_undercover(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .get("MOSSEN_UNDERCOVER")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Get undercover instructions.
fn get_undercover_instructions() -> &'static str {
    "IMPORTANT: You are operating in undercover mode. Do NOT include any attribution, \
     co-authored-by lines, or any mention of AI assistance in the commit message or PR body. \
     Everything should appear as if written entirely by the human developer."
}

/// Get attribution text for commit messages.
fn get_commit_attribution(ctx: &CommandContext) -> String {
    if is_undercover(ctx) {
        return String::new();
    }
    let product_name = &ctx.product_name;
    format!("Co-authored-by: {} <noreply@mossen.dev>", product_name)
}

/// Get PR attribution text.
fn get_pr_attribution(ctx: &CommandContext) -> String {
    if is_undercover(ctx) {
        return String::new();
    }
    let product_name = &ctx.product_name;
    format!(
        "---\n_This PR was created with assistance from {}_",
        product_name
    )
}

/// Get the default branch name (placeholder; in production would call git).
fn get_default_branch() -> String {
    "main".to_string()
}

/// Generate the full prompt content for commit-push-pr.
fn get_prompt_content(ctx: &CommandContext, default_branch: &str, pr_attribution: &str) -> String {
    let commit_attribution = get_commit_attribution(ctx);
    let safe_user = ctx.env_vars.get("SAFEUSER").cloned().unwrap_or_default();
    let username = ctx.env_vars.get("USER").cloned().unwrap_or_default();

    let mut prefix = String::new();
    let mut reviewer_arg = String::new();
    let mut add_reviewer_arg = String::new();
    let mut changelog_section = format!(
        r#"

## Changelog
<!-- CHANGELOG:START -->
[If this PR contains user-facing changes, add a changelog entry here. Otherwise, remove this section.]
<!-- CHANGELOG:END -->"#
    );
    let mut slack_step = format!(
        r#"

5. After creating/updating the PR, check if the user's MOSSEN.md mentions posting to Slack channels. If it does, use ToolSearch to search for "slack send message" tools. If ToolSearch finds a Slack tool, ask the user if they'd like you to post the PR URL to the relevant Slack channel. Only post if the user confirms. If ToolSearch returns no results or errors, skip this step silently—do not mention the failure, do not attempt workarounds, and do not try alternative approaches."#
    );

    if ctx.is_internal_user() && is_undercover(ctx) {
        prefix = format!("{}\n", get_undercover_instructions());
        reviewer_arg = String::new();
        add_reviewer_arg = String::new();
        changelog_section = String::new();
        slack_step = String::new();
    }

    let commit_attr_example = if !commit_attribution.is_empty() {
        format!("\\n\\n{}", commit_attribution)
    } else {
        String::new()
    };

    let pr_attr_part = if !pr_attribution.is_empty() {
        format!("\n\n{}", pr_attribution)
    } else {
        String::new()
    };

    format!(
        r#"{prefix}## Context

- `SAFEUSER`: {safe_user}
- `whoami`: {username}
- `git status`: !`git status`
- `git diff HEAD`: !`git diff HEAD`
- `git branch --show-current`: !`git branch --show-current`
- `git diff {default_branch}...HEAD`: !`git diff {default_branch}...HEAD`
- `gh pr view --json number 2>/dev/null || true`: !`gh pr view --json number 2>/dev/null || true`

## Git Safety Protocol

- NEVER update the git config
- NEVER run destructive/irreversible git commands (like push --force, hard reset, etc) unless the user explicitly requests them
- NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it
- NEVER run force push to main/master, warn the user if they request it
- Do not commit files that likely contain secrets (.env, credentials.json, etc)
- Never use git commands with the -i flag (like git rebase -i or git add -i) since they require interactive input which is not supported

## Your task

Analyze all changes that will be included in the pull request, making sure to look at all relevant commits (NOT just the latest commit, but ALL commits that will be included in the pull request from the git diff {default_branch}...HEAD output above).

Based on the above changes:
1. Create a new branch if on {default_branch} (use SAFEUSER from context above for the branch name prefix, falling back to whoami if SAFEUSER is empty, e.g., `username/feature-name`)
2. Create a single commit with an appropriate message using heredoc syntax{commit_attr_note}:
```
git commit -m "$(cat <<'EOF'
Commit message here.{commit_attr_example}
EOF
)"
```
3. Push the branch to origin
4. If a PR already exists for this branch (check the gh pr view output above), update the PR title and body using `gh pr edit` to reflect the current diff{add_reviewer_arg}. Otherwise, create a pull request using `gh pr create` with heredoc syntax for the body{reviewer_arg}.
   - IMPORTANT: Keep PR titles short (under 70 characters). Use the body for details.
```
gh pr create --title "Short, descriptive title" --body "$(cat <<'EOF'
## Summary
<1-3 bullet points>

## Test plan
[Bulleted markdown checklist of TODOs for testing the pull request...]{changelog_section}{pr_attr_part}
EOF
)"
```

You have the capability to call multiple tools in a single response. You MUST do all of the above in a single message.{slack_step}

Return the PR URL when you're done, so the user can see it."#,
        commit_attr_note = if !commit_attribution.is_empty() {
            ", ending with the attribution text shown in the example below"
        } else {
            ""
        },
    )
}

#[async_trait]
impl Directive for ShipDirective {
    fn name(&self) -> &str {
        "commit-push-pr"
    }

    fn aliases(&self) -> &[&str] {
        &["ship", "pr"]
    }

    fn description(&self) -> &str {
        "Commit, push, and open a PR"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        "[instructions]"
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let default_branch = get_default_branch();
        let pr_attribution = get_pr_attribution(ctx);
        let mut prompt_content = get_prompt_content(ctx, &default_branch, &pr_attribution);

        // Append user instructions if args provided
        let trimmed_args = args.join(" ").trim().to_string();
        if !trimmed_args.is_empty() {
            prompt_content.push_str(&format!(
                "\n\n## Additional instructions from user\n\n{}",
                trimmed_args
            ));
        }

        Ok(CommandResult::Text(prompt_content))
    }
}

/// Get the list of allowed tools for the ship command.
pub fn ship_allowed_tools() -> &'static [&'static str] {
    ALLOWED_TOOLS
}
