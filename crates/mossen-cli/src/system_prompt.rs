//! System prompt assembly — Rust port of the `MOSSEN_CODE_SIMPLE` layered
//! prompt path from `constants/prompts.ts::getSystemPrompt`.
//!
//! All section builders already exist in `mossen_types::constants::prompts`.
//! This module's job is to call them in the right order with the right
//! arguments (cwd, model, enabled tools, language preference, …) and emit a
//! `Vec<String>` that becomes the `system` array on the API request.
//!
//! Why a separate module: `mossen-agent` cannot reach `mossen-tools` (cycle),
//! and we want to keep the assembly logic close to where it is actually
//! injected into `OrchestratorConfig` / `EngineConfig`.

use std::collections::HashSet;

use mossen_types::constants::prompts as p;
use mossen_types::constants::system as sys_consts;

use mossen_agent::types::SystemBlock;

/// Inputs to the composer. Anything the CLI/TUI launcher already knows by
/// the time it builds an `EngineConfig` belongs here — keeps the function
/// free of global state so it stays testable and obvious.
pub struct SystemPromptInputs<'a> {
    pub cwd: &'a str,
    pub model: &'a str,
    /// Marketing-friendly model name (e.g. "MiniMax M2.7"); falls back to a
    /// generic phrasing when None.
    pub model_marketing_name: Option<&'a str>,
    /// `--oneshot`, `--print`, or any non-TUI driver.
    pub is_non_interactive: bool,
    pub is_custom_backend: bool,
    /// USER_TYPE=ant unlocks a few additional sections (numeric anchors,
    /// stricter code-style rules). Off by default.
    pub is_ant: bool,
    /// True when this process is in a git repo. Surfaced inside the env
    /// info block so the model can decide whether to suggest git workflows.
    pub is_git_repo: bool,
    /// Display name of the product (used for first-party / custom backend
    /// branding in a few section bodies).
    pub product_name: &'a str,
    /// Names of tools that will be exposed on this request — used by the
    /// session-specific guidance and using-your-tools sections so the
    /// generated copy actually matches the tool surface the model sees.
    pub enabled_tools: &'a [String],
    /// Optional language hint ("Chinese", "Spanish", …). When set, the
    /// language section tells the assistant to respond in that language by
    /// default. The TUI infers this from locale / past conversations.
    pub language_preference: Option<&'a str>,
    /// Pre-rendered project / user memory block. The launcher walks
    /// `MOSSEN.md`, `~/.mossen/MOSSEN.md`, auto-memory dirs, etc. and
    /// hands the concatenated text in here so the composer can drop it
    /// straight after the env-info block. Empty string = no memory.
    pub memory_text: &'a str,
}

/// Build the system prompt as a `Vec<SystemBlock>` ready to drop onto
/// `EngineConfig.system_prompt` or `PromptParams.system_prompt`.
///
/// Each block is a single section so that future cache-control insertion
/// (per `build_system_prompt_blocks` in `mossen_agent::api::mossen_api`) can
/// pick its own breakpoints rather than having to split a monolith.
pub fn assemble(inputs: &SystemPromptInputs<'_>) -> Vec<SystemBlock> {
    // 1. Identity prefix — drives every downstream "I am Mossen…" disclaimer.
    let api_provider = if inputs.is_custom_backend {
        "custom"
    } else {
        "anthropic"
    };
    let prefix = sys_consts::get_cli_sysprompt_prefix(
        inputs.is_custom_backend,
        inputs.is_non_interactive,
        false, // we do not support --append-system-prompt yet
        api_provider,
        inputs.product_name,
        inputs.product_name,
    );

    // 2. Optional language hint.
    let language = p::get_language_section(inputs.language_preference);

    // 3. Intro / system / doing-tasks / actions / using-tools / tone — these
    //    mirror the layered call in TS's `MOSSEN_CODE_SIMPLE` fast path.
    let intro = p::get_simple_intro_section(false);
    let system_section = p::get_simple_system_section();
    let doing_tasks = p::get_simple_doing_tasks_section(
        inputs.product_name,
        inputs.is_ant,
        inputs.is_custom_backend,
        "report the issue at https://github.com/anthropics/claude-code/issues",
    );
    let actions = p::get_actions_section();

    // Build the tool-name HashSet that the section builders expect. The
    // values must outlive the call, so collect into a Vec we can borrow.
    let tool_set: HashSet<&str> = inputs.enabled_tools.iter().map(String::as_str).collect();
    let using_tools = p::get_using_your_tools_section(&tool_set, false, false);
    let tone = p::get_simple_tone_and_style_section(inputs.is_ant);

    // 4. Session-specific guidance — uses the enabled tools to phrase
    //    advice that actually matches what the model can call.
    let session_guidance = p::get_session_specific_guidance_section(
        &tool_set,
        0,                          // no skills wired into this composer yet
        inputs.is_non_interactive,
        false,                      // not a fork subagent
        false,                      // no embedded search
        true,                       // explore/plan agents available
        "Explore",                  // explore agent type name
        3,                          // min queries to justify Explore
        false,                      // verification agent not enabled
    );

    // 5. Final "summarize tool results" reminder.
    let summarize = p::SUMMARIZE_TOOL_RESULTS_SECTION.to_string();

    // 6. Environment info — cwd, platform, model, knowledge cutoff. This is
    //    what makes the assistant aware of where it's running.
    let platform = std::env::consts::OS;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let shell_line = p::get_shell_info_line(&shell, platform);
    let uname = p::get_uname_sr();
    let knowledge_cutoff = p::get_knowledge_cutoff(inputs.model);
    let model_family = p::get_model_family_guidance(
        inputs.is_custom_backend,
        inputs.model,
        inputs.model,
        inputs.model,
    );
    let product_avail =
        p::get_product_availability_guidance(inputs.is_custom_backend, inputs.product_name);
    let fast_mode = p::get_fast_mode_guidance(inputs.is_custom_backend);
    let env_info = p::compute_simple_env_info(
        inputs.cwd,
        inputs.is_git_repo,
        false, // not a worktree
        platform,
        &shell_line,
        &uname,
        inputs.model,
        inputs.model_marketing_name,
        &[],
        knowledge_cutoff,
        false,
        inputs.is_custom_backend,
        &model_family,
        &product_avail,
        &fast_mode,
    );

    let mut blocks: Vec<SystemBlock> = Vec::with_capacity(16);
    let push = |out: &mut Vec<SystemBlock>, text: String| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        out.push(SystemBlock {
            text: trimmed.to_string(),
            cache_control: None,
        });
    };
    push(&mut blocks, prefix);
    if let Some(lang) = language {
        push(&mut blocks, lang);
    }
    push(&mut blocks, intro);
    push(&mut blocks, system_section);
    push(&mut blocks, doing_tasks);
    push(&mut blocks, actions);
    push(&mut blocks, using_tools);
    push(&mut blocks, tone);
    if let Some(g) = session_guidance {
        push(&mut blocks, g);
    }
    push(&mut blocks, summarize);
    push(&mut blocks, env_info);
    // Memory comes last so it sits right next to the env info (cwd/platform)
    // — the model reads section-local context as a unit and is more likely
    // to honour project-specific instructions when they appear after env.
    if !inputs.memory_text.trim().is_empty() {
        push(&mut blocks, inputs.memory_text.to_string());
    }
    blocks
}

/// Gather MOSSEN.md / .mossen/MOSSEN.md / user global instructions into one
/// system-prompt-ready block. Returns an empty string when no memory was
/// found so the composer can no-op without an Option dance.
///
/// Inspired by TS `getNestedMemoryAttachmentsForFile` + the system prompt
/// memory layer. We deliberately stay simple: project root, project's
/// `.mossen/` folder, and the user's home `~/.mossen/MOSSEN.md`.
pub async fn gather_memory_text(cwd: &std::path::Path) -> String {
    let mut sections: Vec<String> = Vec::new();

    // 1. User-global instructions.
    if let Some(home) = dirs::home_dir() {
        for path in [
            home.join(".mossen").join("MOSSEN.md"),
            home.join(".claude").join("CLAUDE.md"),
        ] {
            if let Ok(text) = tokio::fs::read_to_string(&path).await {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    sections.push(format!(
                        "Contents of {} (user's private global instructions for all projects):\n\n{}",
                        path.display(),
                        trimmed
                    ));
                }
            }
        }
    }

    // 2. Project-root instructions — both MOSSEN.md and the historical
    //    CLAUDE.md filename are honoured so existing projects keep working.
    for filename in ["MOSSEN.md", "MOSSEN.local.md", "CLAUDE.md", "CLAUDE.local.md"] {
        let p = cwd.join(filename);
        if let Ok(text) = tokio::fs::read_to_string(&p).await {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                sections.push(format!("Contents of {}:\n\n{}", p.display(), trimmed));
            }
        }
    }

    // 3. Nested `.mossen/MOSSEN.md` for projects that prefer to hide their
    //    agent instructions in a dotted subdir.
    for nested in [
        cwd.join(".mossen").join("MOSSEN.md"),
        cwd.join(".mossen").join("MOSSEN.local.md"),
    ] {
        if let Ok(text) = tokio::fs::read_to_string(&nested).await {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                sections.push(format!(
                    "Contents of {}:\n\n{}",
                    nested.display(),
                    trimmed
                ));
            }
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    let header = "# claudeMd\nCodebase and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.";
    let body = sections.join("\n\n");
    format!("{}\n\n{}", header, body)
}

/// Try to detect whether `cwd` is inside a git repository. Best-effort: any
/// I/O failure → `false` so we don't block startup on a missing `.git/` lookup.
pub fn detect_git_repo(cwd: &std::path::Path) -> bool {
    let mut p = cwd.to_path_buf();
    loop {
        if p.join(".git").exists() {
            return true;
        }
        if !p.pop() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_emits_identity_and_env() {
        let tools: Vec<String> = vec!["Bash".into(), "Read".into(), "Edit".into()];
        let inputs = SystemPromptInputs {
            cwd: "/tmp/test",
            model: "MiniMax-M2.7",
            model_marketing_name: Some("MiniMax M2.7"),
            is_non_interactive: false,
            is_custom_backend: true,
            is_ant: false,
            is_git_repo: false,
            product_name: "Mossen",
            enabled_tools: &tools,
            language_preference: Some("Chinese"),
            memory_text: "# claudeMd\nProject rule: respond in Chinese.",
        };
        let blocks = assemble(&inputs);
        assert!(!blocks.is_empty(), "system prompt must be non-empty");
        let joined = blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        // Identity, cwd, and model identity must all reach the model.
        assert!(joined.contains("Mossen"), "identity prefix missing");
        assert!(joined.contains("/tmp/test"), "cwd missing from env block");
        assert!(
            joined.contains("MiniMax-M2.7") || joined.contains("MiniMax M2.7"),
            "model id/name missing"
        );
        assert!(
            joined.contains("respond in Chinese"),
            "memory_text block missing from assembled prompt"
        );
    }
}
