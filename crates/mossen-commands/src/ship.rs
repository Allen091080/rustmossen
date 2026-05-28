//! `/commit-push-pr` (alias: `/ship`) — Commit, push, and open a PR (prompt command).

use anyhow::Result;
use async_trait::async_trait;
use tokio::process::Command;

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

async fn run_git_text(ctx: &CommandContext, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(&ctx.cwd)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Resolve the default branch from the current git repository.
async fn get_default_branch(ctx: &CommandContext) -> String {
    if let Some(remote_head) = run_git_text(
        ctx,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    {
        let branch = remote_head
            .strip_prefix("origin/")
            .unwrap_or(remote_head.as_str())
            .trim();
        if !branch.is_empty() {
            return branch.to_string();
        }
    }

    for branch in ["main", "master"] {
        let ref_name = format!("refs/heads/{branch}");
        if run_git_text(
            ctx,
            &["rev-parse", "--verify", "--quiet", ref_name.as_str()],
        )
        .await
        .is_some()
        {
            return branch.to_string();
        }
    }

    if let Some(configured) = run_git_text(ctx, &["config", "--get", "init.defaultBranch"]).await {
        return configured;
    }

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
    let mut changelog_section = r#"

## Changelog
<!-- CHANGELOG:START -->
[If this PR contains user-facing changes, add a changelog entry here. Otherwise, remove this section.]
<!-- CHANGELOG:END -->"#.to_string();
    if ctx.is_internal_user() && is_undercover(ctx) {
        prefix = format!("{}\n", get_undercover_instructions());
        reviewer_arg = String::new();
        add_reviewer_arg = String::new();
        changelog_section = String::new();
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

You have the capability to call multiple tools in a single response. You MUST do all of the above in a single message.

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
        let default_branch = get_default_branch(ctx).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    #[tokio::test]
    async fn ship_uses_origin_head_default_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let status = std::process::Command::new("git")
            .arg("init")
            .current_dir(temp.path())
            .status()
            .expect("git init");
        assert!(status.success());
        let status = std::process::Command::new("git")
            .args([
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ])
            .current_dir(temp.path())
            .status()
            .expect("git symbolic-ref");
        assert!(status.success());

        let result = ShipDirective
            .execute(&[], &test_context(temp.path().to_path_buf()))
            .await
            .expect("ship prompt");
        let CommandResult::Text(prompt) = result else {
            panic!("expected prompt text");
        };
        assert!(prompt.contains("git diff trunk...HEAD"));
    }

    #[tokio::test]
    async fn ship_prompt_does_not_surface_hosted_slack_workflow_by_default() {
        let temp = tempfile::tempdir().expect("tempdir");
        let result = ShipDirective
            .execute(&[], &test_context(temp.path().to_path_buf()))
            .await
            .expect("ship prompt");
        let CommandResult::Text(prompt) = result else {
            panic!("expected prompt text");
        };

        assert!(!prompt.contains("Slack"), "{prompt}");
        assert!(!prompt.contains("ToolSearch"), "{prompt}");
        assert!(
            ship_allowed_tools()
                .iter()
                .all(|tool| !tool.to_ascii_lowercase().contains("slack")),
            "{:?}",
            ship_allowed_tools()
        );
    }
}
