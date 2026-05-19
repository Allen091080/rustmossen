//! `/install-github-app` — Set up Mossen GitHub Actions for a repository (local-widget).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// GitHub App directive — set up GitHub Actions integration for a repository.
pub struct GithubAppDirective;

/// Check if the GitHub workflow feature is ready.
fn get_github_workflow_readiness(ctx: &CommandContext) -> bool {
    // In the full implementation, this checks the GitHub App installation status
    // and whether the necessary workflows are configured.
    ctx.is_env_truthy("MOSSEN_GITHUB_WORKFLOW_READY")
}

/// Get the GitHub App installation URL.
fn get_github_app_install_url(ctx: &CommandContext) -> String {
    ctx.env_vars
        .get("MOSSEN_GITHUB_APP_INSTALL_URL")
        .cloned()
        .unwrap_or_else(|| "https://github.com/apps/mossen-code".to_string())
}

/// Detect the current repository information.
fn detect_current_repo(ctx: &CommandContext) -> Option<(String, String)> {
    // Try to get owner/repo from git remote
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&ctx.cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_remote(&url)
}

/// Parse a GitHub remote URL into owner/repo.
fn parse_github_remote(url: &str) -> Option<(String, String)> {
    // Handle SSH format: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let repo = rest.trim_end_matches(".git");
        let parts: Vec<&str> = repo.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Handle HTTPS format: https://github.com/owner/repo.git
    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        let repo = after.trim_end_matches(".git");
        let parts: Vec<&str> = repo.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    None
}

/// Execute the GitHub App installation flow.
async fn execute_github_app_flow(ctx: &CommandContext) -> Result<String> {
    let install_url = get_github_app_install_url(ctx);

    // Detect current repository
    let repo_info = detect_current_repo(ctx);

    let mut output = String::new();

    if let Some((owner, repo)) = repo_info {
        output.push_str(&format!(
            "Setting up Mossen GitHub Actions for {}/{}\n\n",
            owner, repo
        ));

        // Check if already installed
        if get_github_workflow_readiness(ctx) {
            output.push_str("✓ GitHub App is already installed for this repository.\n");
            output.push_str("  Workflows are configured and ready.\n");
        } else {
            output.push_str("To set up GitHub Actions integration:\n\n");
            output.push_str(&format!(
                "1. Install the GitHub App: {}\n",
                install_url
            ));
            output.push_str(&format!(
                "2. Select the repository: {}/{}\n",
                owner, repo
            ));
            output.push_str("3. The workflow will be automatically configured.\n");
        }
    } else {
        output.push_str("No GitHub repository detected in the current directory.\n\n");
        output.push_str("To set up Mossen GitHub Actions:\n");
        output.push_str(&format!("1. Install the GitHub App: {}\n", install_url));
        output.push_str("2. Navigate to a directory with a GitHub remote.\n");
        output.push_str("3. Run this command again.\n");
    }

    Ok(output)
}

#[async_trait]
impl Directive for GithubAppDirective {
    fn name(&self) -> &str {
        "install-github-app"
    }

    fn description(&self) -> &str {
        "Set up Mossen GitHub Actions for a repository"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        !ctx.is_env_truthy("DISABLE_INSTALL_GITHUB_APP_COMMAND")
            && (!ctx.is_custom_backend || get_github_workflow_readiness(ctx))
            && !ctx.is_custom_backend
    }

    fn is_hidden(&self) -> bool {
        false
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let result = execute_github_app_flow(ctx).await?;
        Ok(CommandResult::Text(result))
    }
}
