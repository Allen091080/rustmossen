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
use mossen_utils::hooks_utils::{
    execute_instructions_loaded_hooks, has_instructions_loaded_hook, HooksContext,
    InstructionsLoadReason, InstructionsMemoryType, TOOL_HOOK_EXECUTION_TIMEOUT_MS,
};

/// Inputs to the composer. Anything the CLI/TUI launcher already knows by
/// the time it builds an `EngineConfig` belongs here — keeps the function
/// free of global state so it stays testable and obvious.
pub struct SystemPromptInputs<'a> {
    pub cwd: &'a str,
    pub model: &'a str,
    /// Marketing-friendly model name (e.g. "Example Fast"); falls back to a
    /// generic phrasing when None.
    pub model_marketing_name: Option<&'a str>,
    /// `--oneshot`, `--print`, or any non-TUI driver.
    pub is_non_interactive: bool,
    /// True when this process is the background fork created by Agent.
    pub is_fork_subagent: bool,
    pub is_custom_backend: bool,
    /// USER_TYPE=internal unlocks a few additional sections (numeric anchors,
    /// stricter code-style rules). Off by default.
    pub is_internal: bool,
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
    /// Number of user-invocable skills available through the Skill tool.
    pub skill_commands_count: usize,
    /// Preformatted loaded-skill list, already constrained to the prompt
    /// budget. Empty string means no skills are available.
    pub skill_commands_text: &'a str,
    /// Whether the built-in explore/plan subagents are available in this
    /// runtime. Keep this tied to the actual agent registry; otherwise the
    /// prompt can steer the model into a dead subagent_type.
    pub are_explore_plan_agents: bool,
    /// Exact subagent_type for broad exploration guidance.
    pub explore_agent_type: &'a str,
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
        "provider"
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
        inputs.is_internal,
        inputs.is_custom_backend,
        "report the issue at https://github.com/providers/cli/issues",
    );
    let actions = p::get_actions_section();

    // Build the tool-name HashSet that the section builders expect. The
    // values must outlive the call, so collect into a Vec we can borrow.
    let tool_set: HashSet<&str> = inputs.enabled_tools.iter().map(String::as_str).collect();
    let using_tools = p::get_using_your_tools_section(&tool_set, false, false);
    let tone = p::get_simple_tone_and_style_section(inputs.is_internal);

    // 4. Session-specific guidance — uses the enabled tools to phrase
    //    advice that actually matches what the model can call.
    let session_guidance = p::get_session_specific_guidance_section(p::SessionSpecificGuidance {
        enabled_tools: &tool_set,
        skill_commands_count: inputs.skill_commands_count,
        is_non_interactive: inputs.is_non_interactive,
        is_fork_subagent: inputs.is_fork_subagent,
        has_embedded_search: false,
        are_explore_plan_agents: inputs.are_explore_plan_agents,
        explore_agent_type: inputs.explore_agent_type,
        explore_agent_min_queries: 3,
        is_verification_agent_enabled: false,
    });

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
    let env_info = p::compute_simple_env_info(p::SimpleEnvInfo {
        cwd: inputs.cwd,
        is_git: inputs.is_git_repo,
        is_worktree: false,
        platform,
        shell_info_line: &shell_line,
        uname_sr: &uname,
        model_id: inputs.model,
        marketing_name: inputs.model_marketing_name,
        additional_dirs: &[],
        knowledge_cutoff,
        is_undercover: false,
        model_family_guidance: &model_family,
        product_availability_guidance: &product_avail,
        fast_mode_guidance: &fast_mode,
    });

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
    if inputs.skill_commands_count > 0
        && tool_set.contains("Skill")
        && !inputs.skill_commands_text.trim().is_empty()
    {
        push(
            &mut blocks,
            format!(
                "# User-invocable skills\nThe following skills are loaded in this session. When the user's request matches one of these skills, invoke the Skill tool with that exact skill name before answering:\n{}",
                inputs.skill_commands_text.trim()
            ),
        );
    }
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
/// 递归展开 `@<file>` import。语义：
/// - 一行（前后允许空白）只包含 `@<path>` 就替换为该文件内容
/// - path 是相对路径时，相对 `parent`（被 include 源文件的目录）解析
/// - 防循环：`visited` 跟踪已展开的 canonical 路径，已访问的直接保留原行
/// - 最大递归深度 5；超出后保留原文（防爆栈 + 防恶意构造）
/// - 文件读不到也保留原文（保守，避免静默丢内容）
async fn expand_at_includes(
    raw: &str,
    parent: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    depth: u8,
    hooks_context: Option<&HooksContext>,
    memory_type: InstructionsMemoryType,
    parent_file_path: Option<&std::path::Path>,
) -> String {
    if depth >= 5 {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let trimmed = line.trim();
        let include_target = trimmed.strip_prefix('@').and_then(|rest| {
            if !rest.is_empty() && !rest.contains(char::is_whitespace) {
                Some(rest)
            } else {
                None
            }
        });

        if let Some(target) = include_target {
            let candidate = if std::path::Path::new(target).is_absolute() {
                std::path::PathBuf::from(target)
            } else {
                parent.join(target)
            };
            let canon = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if visited.contains(&canon) {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            visited.insert(canon.clone());

            match tokio::fs::read_to_string(&candidate).await {
                Ok(child_raw) => {
                    run_instructions_loaded_hook(
                        hooks_context,
                        &candidate,
                        memory_type,
                        InstructionsLoadReason::Include,
                        parent_file_path,
                    )
                    .await;
                    let child_parent = candidate
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| parent.to_path_buf());
                    let expanded = Box::pin(expand_at_includes(
                        &child_raw,
                        &child_parent,
                        visited,
                        depth + 1,
                        hooks_context,
                        memory_type,
                        Some(&candidate),
                    ))
                    .await;
                    out.push_str(&expanded);
                    if !expanded.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Err(_) => {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub async fn gather_memory_text(cwd: &std::path::Path) -> String {
    gather_memory_text_with_hooks(cwd, None).await
}

async fn run_instructions_loaded_hook(
    hooks_context: Option<&HooksContext>,
    file_path: &std::path::Path,
    memory_type: InstructionsMemoryType,
    load_reason: InstructionsLoadReason,
    parent_file_path: Option<&std::path::Path>,
) {
    let Some(ctx) = hooks_context else {
        return;
    };
    if !has_instructions_loaded_hook(ctx) {
        return;
    }

    let file_path = file_path.to_string_lossy();
    let parent_file_path = parent_file_path.map(|path| path.to_string_lossy().to_string());
    execute_instructions_loaded_hooks(
        ctx,
        &file_path,
        memory_type,
        load_reason,
        None,
        None,
        parent_file_path.as_deref(),
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;
}

pub async fn gather_memory_text_with_hooks(
    cwd: &std::path::Path,
    hooks_context: Option<&HooksContext>,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    // 1. User-global instructions.
    if let Some(home) = dirs::home_dir() {
        for path in [home.join(".mossen").join("MOSSEN.md")] {
            if let Ok(text) = tokio::fs::read_to_string(&path).await {
                run_instructions_loaded_hook(
                    hooks_context,
                    &path,
                    InstructionsMemoryType::User,
                    InstructionsLoadReason::SessionStart,
                    None,
                )
                .await;
                let parent = path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let mut visited = std::collections::HashSet::new();
                if let Ok(canon) = std::fs::canonicalize(&path) {
                    visited.insert(canon);
                }
                let expanded = expand_at_includes(
                    &text,
                    &parent,
                    &mut visited,
                    0,
                    hooks_context,
                    InstructionsMemoryType::User,
                    Some(&path),
                )
                .await;
                let trimmed = expanded.trim();
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
    //    MOSSEN.md filename are honoured so existing projects keep working.
    for filename in ["MOSSEN.md", "MOSSEN.local.md"] {
        let p = cwd.join(filename);
        if let Ok(text) = tokio::fs::read_to_string(&p).await {
            let memory_type = if filename.ends_with(".local.md") {
                InstructionsMemoryType::Local
            } else {
                InstructionsMemoryType::Project
            };
            run_instructions_loaded_hook(
                hooks_context,
                &p,
                memory_type,
                InstructionsLoadReason::SessionStart,
                None,
            )
            .await;
            let parent = p
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let mut visited = std::collections::HashSet::new();
            if let Ok(canon) = std::fs::canonicalize(&p) {
                visited.insert(canon);
            }
            let expanded = expand_at_includes(
                &text,
                &parent,
                &mut visited,
                0,
                hooks_context,
                memory_type,
                Some(&p),
            )
            .await;
            let trimmed = expanded.trim();
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
            let memory_type = if nested
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".local.md"))
                .unwrap_or(false)
            {
                InstructionsMemoryType::Local
            } else {
                InstructionsMemoryType::Project
            };
            run_instructions_loaded_hook(
                hooks_context,
                &nested,
                memory_type,
                InstructionsLoadReason::SessionStart,
                None,
            )
            .await;
            let parent = nested
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let mut visited = std::collections::HashSet::new();
            if let Ok(canon) = std::fs::canonicalize(&nested) {
                visited.insert(canon);
            }
            let expanded = expand_at_includes(
                &text,
                &parent,
                &mut visited,
                0,
                hooks_context,
                memory_type,
                Some(&nested),
            )
            .await;
            let trimmed = expanded.trim();
            if !trimmed.is_empty() {
                sections.push(format!("Contents of {}:\n\n{}", nested.display(), trimmed));
            }
        }
    }

    // 4. File-based memory instructions. This is intentionally
    // loaded after MOSSEN.md so project instructions stay visible while the
    // final system-prompt memory block also tells the model where persistent
    // memories must be read and written.
    if let Some(memory_prompt) = crate::memdir::load_memory_prompt(cwd).await {
        let trimmed = memory_prompt.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    let header = "# mossenMd\nCodebase and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.";
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
    use mossen_utils::hooks_utils::{HookMatcher, HooksContext};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    const MEMORY_ENV_KEYS: &[&str] = &[
        "HOME",
        "MOSSEN_CONFIG_DIR",
        "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE",
        "MOSSEN_CODE_DISABLE_AUTO_MEMORY",
        "MOSSEN_CODE_SIMPLE",
        "MOSSEN_CODE_REMOTE",
        "MOSSEN_CODE_REMOTE_MEMORY_DIR",
        "MOSSEN_CODE_DISABLE_TEAM_MEMORY",
        "MOSSEN_CODE_ENABLE_TEAM_MEMORY",
        "MOSSEN_TEAM_MEMORY",
        "MOSSEN_MEMORY_TEAM_MEMORY_ENABLED",
        "MOSSEN_TEAM_MEMORY_ENABLED",
    ];

    struct MemoryEnvGuard(Vec<(&'static str, Option<String>)>);

    impl Drop for MemoryEnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn memory_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn isolate_memory_env(root: &std::path::Path) -> MemoryEnvGuard {
        let guard = MemoryEnvGuard(
            MEMORY_ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect(),
        );
        for key in MEMORY_ENV_KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("MOSSEN_CONFIG_DIR", root.join("home").join(".mossen"));
        std::env::set_var("MOSSEN_CODE_DISABLE_AUTO_MEMORY", "1");
        std::env::set_var("MOSSEN_CODE_DISABLE_TEAM_MEMORY", "1");
        guard
    }

    fn instructions_hook_context(cwd: &std::path::Path, command: String) -> HooksContext {
        let mut registered_hooks = HashMap::new();
        registered_hooks.insert(
            "InstructionsLoaded".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![serde_json::json!({
                    "type": "command",
                    "command": command,
                    "timeout": 1
                })],
                plugin_root: None,
                plugin_id: None,
                plugin_name: None,
                skill_root: None,
                skill_name: None,
            }],
        );

        HooksContext {
            session_id: "instructions-loaded-hook-test".to_string(),
            original_cwd: cwd.to_string_lossy().to_string(),
            project_root: cwd.to_string_lossy().to_string(),
            is_non_interactive: true,
            trust_accepted: true,
            hooks_config_snapshot: None,
            registered_hooks: Some(registered_hooks),
            disable_all_hooks: false,
            managed_hooks_only: false,
            main_thread_agent_type: Some("main".to_string()),
            custom_backend_enabled: false,
            simple_mode: false,
            get_transcript_path: Arc::new(|session_id| format!("/tmp/{session_id}.jsonl")),
            get_agent_transcript_path: Arc::new(|agent_id| format!("/tmp/agent-{agent_id}.jsonl")),
            log_debug: Arc::new(|_| {}),
            log_error: Arc::new(|_| {}),
            log_event: Arc::new(|_, _| {}),
            get_settings: Arc::new(|| None),
            get_settings_for_source: Arc::new(|_| None),
            invalidate_session_env_cache: Arc::new(|| {}),
            dynamic_hook_executor: None,
            subprocess_env: std::env::vars().collect(),
            allowed_official_marketplace_names: HashSet::new(),
        }
    }

    #[test]
    fn assemble_emits_identity_and_env() {
        let tools: Vec<String> = vec!["Bash".into(), "Read".into(), "Edit".into()];
        let inputs = SystemPromptInputs {
            cwd: "/tmp/test",
            model: "example-fast",
            model_marketing_name: Some("Example Fast"),
            is_non_interactive: false,
            is_fork_subagent: false,
            is_custom_backend: true,
            is_internal: false,
            is_git_repo: false,
            product_name: "Mossen",
            enabled_tools: &tools,
            skill_commands_count: 0,
            skill_commands_text: "",
            are_explore_plan_agents: false,
            explore_agent_type: "explore",
            language_preference: Some("Chinese"),
            memory_text: "# mossenMd\nProject rule: respond in Chinese.",
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
            joined.contains("example-fast") || joined.contains("Example Fast"),
            "model id/name missing"
        );
        assert!(
            joined.contains("respond in Chinese"),
            "memory_text block missing from assembled prompt"
        );
    }

    #[test]
    fn assemble_includes_loaded_skill_inventory_when_skill_tool_enabled() {
        let tools: Vec<String> = vec!["Skill".into(), "Read".into()];
        let inputs = SystemPromptInputs {
            cwd: "/tmp/test",
            model: "example-fast",
            model_marketing_name: None,
            is_non_interactive: false,
            is_fork_subagent: false,
            is_custom_backend: true,
            is_internal: false,
            is_git_repo: false,
            product_name: "Mossen",
            enabled_tools: &tools,
            skill_commands_count: 1,
            skill_commands_text: "- audit: Audit the current change",
            are_explore_plan_agents: false,
            explore_agent_type: "explore",
            language_preference: None,
            memory_text: "",
        };

        let joined = assemble(&inputs)
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(joined.contains("# User-invocable skills"), "{joined}");
        assert!(
            joined.contains("- audit: Audit the current change"),
            "{joined}"
        );
        assert!(joined.contains("Use the Skill tool"), "{joined}");
    }

    #[test]
    fn assemble_only_mentions_explore_agent_when_runtime_enables_it() {
        let tools: Vec<String> = vec!["Agent".into(), "Glob".into(), "Grep".into()];
        let disabled = SystemPromptInputs {
            cwd: "/tmp/test",
            model: "example-fast",
            model_marketing_name: None,
            is_non_interactive: false,
            is_fork_subagent: false,
            is_custom_backend: true,
            is_internal: false,
            is_git_repo: false,
            product_name: "Mossen",
            enabled_tools: &tools,
            skill_commands_count: 0,
            skill_commands_text: "",
            are_explore_plan_agents: false,
            explore_agent_type: "explore",
            language_preference: None,
            memory_text: "",
        };
        let disabled_prompt = assemble(&disabled)
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(!disabled_prompt.contains("subagent_type=Explore"));
        assert!(!disabled_prompt.contains("subagent_type=explore"));

        let enabled = SystemPromptInputs {
            are_explore_plan_agents: true,
            ..disabled
        };
        let enabled_prompt = assemble(&enabled)
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(enabled_prompt.contains("subagent_type=explore"));
        assert!(!enabled_prompt.contains("subagent_type=Explore"));
    }

    #[test]
    fn assemble_marks_fork_subagent_as_non_delegating_worker() {
        let tools: Vec<String> = vec!["Agent".into(), "Read".into(), "Glob".into()];
        let inputs = SystemPromptInputs {
            cwd: "/tmp/test",
            model: "example-fast",
            model_marketing_name: None,
            is_non_interactive: true,
            is_fork_subagent: true,
            is_custom_backend: true,
            is_internal: false,
            is_git_repo: false,
            product_name: "Mossen",
            enabled_tools: &tools,
            skill_commands_count: 0,
            skill_commands_text: "",
            are_explore_plan_agents: false,
            explore_agent_type: "explore",
            language_preference: None,
            memory_text: "",
        };

        let joined = assemble(&inputs)
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(joined.contains("If you ARE the fork"), "{joined}");
        assert!(joined.contains("do not re-delegate"), "{joined}");
    }

    #[tokio::test]
    async fn at_include_expands_one_level() {
        let _lock = memory_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_memory_env(temp.path());
        let root = temp.path();
        std::fs::write(root.join("MOSSEN.md"), "@included.md\n").expect("write root memory");
        std::fs::write(root.join("included.md"), "This is the included content.\n")
            .expect("write included memory");

        let text = gather_memory_text(root).await;
        assert!(
            text.contains("This is the included content."),
            "got:\n{}",
            text
        );
        assert!(!text.contains("@included.md"), "got:\n{}", text);
    }

    #[tokio::test]
    async fn instructions_loaded_hook_runs_for_project_and_include_files() {
        let _lock = memory_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_memory_env(temp.path());
        let root = temp.path();
        let marker = root.join("instructions-loaded.log");
        std::fs::write(root.join("MOSSEN.md"), "@included.md\nProject rules\n")
            .expect("write root memory");
        std::fs::write(root.join("included.md"), "Included rules\n")
            .expect("write included memory");

        let command = format!("cat >> {}", marker.display());
        let ctx = instructions_hook_context(root, command);

        let text = gather_memory_text_with_hooks(root, Some(&ctx)).await;
        assert!(text.contains("Included rules"), "{text}");

        let log = std::fs::read_to_string(marker).expect("hook marker");
        assert!(
            log.contains(r#""hook_event_name":"InstructionsLoaded""#)
                && log.contains(r#""memory_type":"Project""#)
                && log.contains(r#""load_reason":"session_start""#)
                && log.contains("MOSSEN.md"),
            "{log}"
        );
        assert!(
            log.contains(r#""load_reason":"include""#)
                && log.contains("included.md")
                && log.contains(r#""parent_file_path":"#),
            "{log}"
        );
    }

    #[tokio::test]
    async fn global_user_memory_is_shared_across_cwds() {
        let _lock = memory_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_memory_env(temp.path());
        let home_mossen = temp.path().join("home").join(".mossen");
        let project_a = temp.path().join("worktree-a");
        let project_b = temp.path().join("worktree-b");
        std::fs::create_dir_all(&home_mossen).expect("home mossen dir");
        std::fs::create_dir_all(&project_a).expect("project a");
        std::fs::create_dir_all(&project_b).expect("project b");

        let marker = "MOSSEN_M5_3_GLOBAL_USER_MEMORY_MARKER";
        std::fs::write(home_mossen.join("MOSSEN.md"), marker).expect("write global memory");

        let text_a = gather_memory_text(&project_a).await;
        let text_b = gather_memory_text(&project_b).await;

        assert!(text_a.contains(marker), "{text_a}");
        assert!(text_b.contains(marker), "{text_b}");
    }

    #[tokio::test]
    async fn project_memory_is_loaded_for_fresh_window() {
        let _lock = memory_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_memory_env(temp.path());
        let project = temp.path().join("project");
        std::fs::create_dir_all(&project).expect("project dir");

        let marker = "MOSSEN_M5_4_PROJECT_MEMORY_MARKER";
        std::fs::write(project.join("MOSSEN.md"), marker).expect("write project memory");

        let text = gather_memory_text(&project).await;

        assert!(text.contains(marker), "{text}");
    }

    #[tokio::test]
    async fn project_memory_reload_reads_updated_file() {
        let _lock = memory_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = isolate_memory_env(temp.path());
        let project = temp.path().join("project");
        std::fs::create_dir_all(&project).expect("project dir");
        let memory_file = project.join("MOSSEN.md");

        let first_marker = "MOSSEN_M5_6_PROJECT_MEMORY_V1";
        let second_marker = "MOSSEN_M5_6_PROJECT_MEMORY_V2";
        std::fs::write(&memory_file, first_marker).expect("write first memory");
        let first = gather_memory_text(&project).await;
        assert!(first.contains(first_marker), "{first}");

        std::fs::write(&memory_file, second_marker).expect("write second memory");
        let second = gather_memory_text(&project).await;

        assert!(second.contains(second_marker), "{second}");
        assert!(!second.contains(first_marker), "{second}");
    }
}
