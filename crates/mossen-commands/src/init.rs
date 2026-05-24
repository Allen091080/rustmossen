//! `/init` — Initialize MOSSEN.md with codebase documentation.
//!
//! Translates `commands/init.ts`. This is a prompt-type command that
//! sends a structured prompt to the model to analyze the codebase and
//! create a MOSSEN.md file with project guidance.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// The old init prompt: creates a basic MOSSEN.md with commands and architecture.
fn old_init_prompt(assistant_name: &str) -> String {
    format!(
        r#"Please analyze this codebase and create a MOSSEN.md file, which will be given to future {assistant_name} sessions operating in this repository.

What to add:
1. Commands that will be commonly used, such as how to build, lint, and run tests. Include the necessary commands to develop in this codebase, such as how to run a single test.
2. High-level code architecture and structure so that future instances can be productive more quickly. Focus on the "big picture" architecture that requires reading multiple files to understand.

Usage notes:
- If there's already a MOSSEN.md, suggest improvements to it.
- When you make the initial MOSSEN.md, do not repeat yourself and do not include obvious instructions like "Provide helpful error messages to users", "Write unit tests for all new utilities", "Never include sensitive information (API keys, tokens) in code or commits".
- Avoid listing every component or file structure that can be easily discovered.
- Don't include generic development practices.
- If there are Cursor rules (in .cursor/rules/ or .cursorrules) or Copilot rules (in .github/copilot-instructions.md), make sure to include the important parts.
- If there is a README.md, make sure to include the important parts.
- Do not make up information such as "Common Development Tasks", "Tips for Development", "Support and Documentation" unless this is expressly included in other files that you read.
- Be sure to prefix the file with the following text:

```
# MOSSEN.md

This file provides guidance to {assistant_name} when working with code in this repository.
```"#
    )
}

/// The new init prompt: multi-phase interactive setup with MOSSEN.md, skills, and hooks.
fn new_init_prompt(assistant_name: &str) -> String {
    format!(
        r#"Set up a minimal MOSSEN.md (and optionally skills and hooks) for this repo. MOSSEN.md is loaded into every {assistant_name} session, so it must be concise — only include what the assistant would get wrong without it.

## Phase 1: Ask what to set up

Use AskUserQuestion to find out what the user wants:

- "Which MOSSEN.md files should /init set up?"
  Options: "Project MOSSEN.md" | "Personal MOSSEN.local.md" | "Both project + personal"
  Description for project: "Team-shared instructions checked into source control — architecture, coding standards, common workflows."
  Description for personal: "Your private preferences for this project (gitignored, not shared) — your role, sandbox URLs, preferred test data, workflow quirks."

- "Also set up skills and hooks?"
  Options: "Skills + hooks" | "Skills only" | "Hooks only" | "Neither, just MOSSEN.md"
  Description for skills: "On-demand capabilities you or Mossen invoke with `/skill-name` — good for repeatable workflows and reference knowledge."
  Description for hooks: "Deterministic shell commands that run on tool events (e.g., format after every edit). Mossen can't skip them."

## Phase 2: Explore the codebase

Launch a subagent to survey the codebase, and ask it to read key files to understand the project: manifest files (package.json, Cargo.toml, pyproject.toml, go.mod, pom.xml, etc.), README, Makefile/build configs, CI config, existing MOSSEN.md, .mossen/rules/, AGENTS.md, .cursor/rules or .cursorrules, .github/copilot-instructions.md, .windsurfrules, .clinerules, .mcp.json.

Detect:
- Build, test, and lint commands (especially non-standard ones)
- Languages, frameworks, and package manager
- Project structure (monorepo with workspaces, multi-module, or single project)
- Code style rules that differ from language defaults
- Non-obvious gotchas, required env vars, or workflow quirks
- Existing .mossen/skills/ and .mossen/rules/ directories
- Formatter configuration (prettier, biome, ruff, black, gofmt, rustfmt, or a unified format script like `npm run format` / `make fmt`)
- Git worktree usage: run `git worktree list` to check if this repo has multiple worktrees

Note what you could NOT figure out from code alone — these become interview questions.

## Phase 3: Fill in the gaps

Use AskUserQuestion to gather what you still need.

## Phase 4: Write MOSSEN.md (if user chose project or both)

Write a minimal MOSSEN.md at the project root. Every line must pass this test: "Would removing this cause Mossen to make mistakes?" If no, cut it.

Include:
- Build/test/lint commands Mossen can't guess (non-standard scripts, flags, or sequences)
- Code style rules that DIFFER from language defaults
- Testing instructions and quirks
- Repo etiquette (branch naming, PR conventions, commit style)
- Required env vars or setup steps
- Non-obvious gotchas or architectural decisions

Exclude:
- File-by-file structure or component lists
- Standard language conventions Mossen already knows
- Generic advice ("write clean code", "handle errors")

Prefix the file with:

```
# MOSSEN.md

This file provides guidance to {assistant_name} when working with code in this repository.
```

## Phase 5: Write MOSSEN.local.md (if user chose personal or both)

Write a minimal MOSSEN.local.md at the project root with personal preferences.

## Phase 6: Suggest and create skills (if applicable)

Skills add capabilities Mossen can use on demand. Create each skill at `.mossen/skills/<skill-name>/SKILL.md`.

## Phase 7: Suggest additional optimizations

Check the environment and offer relevant improvements (GitHub CLI, linting, hooks).

## Phase 8: Summary and next steps

Recap what was set up and suggest additional optimizations."#
    )
}

/// `/init` command — prompt-type command that generates MOSSEN.md files.
pub struct InitDirective;

#[async_trait]
impl Directive for InitDirective {
    fn name(&self) -> &str {
        "init"
    }

    fn description(&self) -> &str {
        "Initialize new MOSSEN.md file(s) and optional skills/hooks with codebase documentation"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        ""
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let assistant_name = &ctx.product_name;
        // Use new init prompt for internal users or when NEW_INIT feature is enabled
        let use_new_init = ctx.is_internal_user() || ctx.is_env_truthy("MOSSEN_CODE_NEW_INIT");

        let prompt = if use_new_init {
            new_init_prompt(assistant_name)
        } else {
            old_init_prompt(assistant_name)
        };

        Ok(CommandResult::Text(prompt))
    }
}
