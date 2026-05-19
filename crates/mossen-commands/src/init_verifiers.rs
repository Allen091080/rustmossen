//! `/init-verifiers` — Create verifier skills for automated code change verification.
//!
//! Translates `commands/init-verifiers.ts`. This is a prompt-type command that
//! sends a structured prompt to the model to analyze the project and create
//! verifier skills for Playwright, CLI, or API testing.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Build the init-verifiers prompt text.
fn init_verifiers_prompt() -> &'static str {
    r#"Use the TodoWrite tool to track your progress through this multi-step task.

## Goal

Create one or more verifier skills that can be used by the Verify agent to automatically verify code changes in this project or folder. You may create multiple verifiers if the project has different verification needs (e.g., both web UI and API endpoints).

**Do NOT create verifiers for unit tests or typechecking.** Those are already handled by the standard build/test workflow and don't need dedicated verifier skills. Focus on functional verification: web UI (Playwright), CLI (Tmux), and API (HTTP) verifiers.

## Phase 1: Auto-Detection

Analyze the project to detect what's in different subdirectories. The project may contain multiple sub-projects or areas that need different verification approaches.

1. **Scan top-level directories** to identify distinct project areas:
   - Look for separate package.json, Cargo.toml, pyproject.toml, go.mod in subdirectories
   - Identify distinct application types in different folders

2. **For each area, detect:**
   a. **Project type and stack** — Primary language(s) and frameworks, package managers
   b. **Application type** — Web app → Playwright verifier, CLI → Tmux verifier, API → HTTP verifier
   c. **Existing verification tools** — Test frameworks, E2E tools, dev server scripts
   d. **Dev server configuration** — How to start, URL, ready signal

3. **Installed verification packages** (for web apps)
   - Check if Playwright is installed
   - Check MCP configuration for browser automation tools

## Phase 2: Verification Tool Setup

Based on Phase 1, help the user set up appropriate verification tools.

### For Web Applications
1. If browser automation tools are detected, ask which to use
2. If none detected, offer to install: Playwright (recommended), Chrome DevTools MCP, Mossen Chrome Extension, or None
3. If Playwright chosen, install via appropriate package manager
4. If MCP-based option, configure .mcp.json

### For CLI Tools
1. Check for asciinema availability
2. Tmux is typically system-installed

### For API Services
1. Check for curl/httpie availability

## Phase 3: Interactive Q&A

For each distinct area, confirm:
1. **Verifier name** — Single area: verifier-playwright/cli/api. Multiple: verifier-<project>-<type>
2. **Project-specific questions** — Dev server command, URL, ready signal, entry point, etc.
3. **Authentication & Login** — Whether auth is required, login method, test credentials

## Phase 4: Generate Verifier Skill

Write skill files to `.mossen/skills/<verifier-name>/SKILL.md`.

### Allowed Tools by Type

**verifier-playwright**: Bash(npm:*), Bash(yarn:*), mcp__playwright__*, Read, Glob, Grep
**verifier-cli**: Tmux, Bash(asciinema:*), Read, Glob, Grep
**verifier-api**: Bash(curl:*), Bash(http:*), Bash(npm:*), Read, Glob, Grep

## Phase 5: Confirm Creation

After writing skills, inform the user:
1. Where each skill was created
2. How the Verify agent discovers them (folder name must contain "verifier")
3. They can edit skills to customize
4. They can run /init-verifiers again to add more
5. The verifier will offer to self-update if outdated"#
}

/// `/init-verifiers` command — creates verifier skills for automated verification.
pub struct InitVerifiersDirective;

#[async_trait]
impl Directive for InitVerifiersDirective {
    fn name(&self) -> &str {
        "init-verifiers"
    }

    fn description(&self) -> &str {
        "Create verifier skill(s) for automated verification of code changes"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        ""
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        Ok(CommandResult::Text(init_verifiers_prompt().to_string()))
    }
}
