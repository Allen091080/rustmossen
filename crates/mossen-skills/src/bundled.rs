//! # bundled — 内置技能注册入口
//!
//! 对应 TypeScript `skills/bundled/*.ts`。每个 `register_*_skill` 入口
//! 调用 [`crate::skill::register_bundled_craft`] 把对应的内置技能加入
//! 全局注册表。`BundledCraftDefinition` 仅承载元数据 — 真正的 prompt
//! 渲染由上层在调用时使用 `markdown_content` 完成；这里的 prompt 内容
//! 是从 TS 文件中的常量原样翻译过来的真实文本，不是占位符。
//!
//! 兼容点：
//! - TS 中每个 `register*Skill` 调用 `registerBundledSkill({…})` 注册
//!   定义；Rust 中我们调用 [`register_bundled_craft`] 完成同样的语义。
//! - TS 的 `isEnabled`、`getPromptForCommand` 等闭包字段在 Rust 侧由
//!   [`BundledCraftDefinition`] 之外的元数据 + 调用方调度处理。本模块
//!   只负责注册元数据本身，保持与 TS 入口的一致性。

use std::collections::HashMap;

use mossen_types::command::ExecutionContext;

use crate::skill::{register_bundled_craft, BundledCraftDefinition};

fn definition(
    name: &str,
    description: &str,
    when_to_use: Option<&str>,
    argument_hint: Option<&str>,
    allowed_tools: Option<Vec<&str>>,
    files: Option<HashMap<String, String>>,
) -> BundledCraftDefinition {
    BundledCraftDefinition {
        name: name.to_string(),
        description: description.to_string(),
        aliases: None,
        when_to_use: when_to_use.map(String::from),
        argument_hint: argument_hint.map(String::from),
        allowed_tools: allowed_tools.map(|v| v.into_iter().map(String::from).collect()),
        model: None,
        disable_model_invocation: false,
        user_invocable: true,
        hooks: None,
        context: Some(ExecutionContext::Inline),
        agent: None,
        files,
    }
}

/// `skills/bundled/loop.ts` — `/loop` 调度技能。
pub fn register_loop_skill() {
    register_bundled_craft(definition(
        "loop",
        "Run a prompt or slash command on a recurring interval (e.g. /loop 5m /foo, defaults to 10m)",
        Some("When the user wants to set up a recurring task, poll for status, or run something repeatedly on an interval (e.g. \"check the deploy every 5 minutes\", \"keep running /babysit-prs\"). Do NOT invoke for one-off tasks."),
        Some("[interval] <prompt>"),
        Some(vec!["CronCreate", "CronDelete", "CronList"]),
        None,
    ));
}

/// `skills/bundled/debug.ts` — `/debug` 排错技能。
pub fn register_debug_skill() {
    register_bundled_craft(definition(
        "debug",
        "Diagnose and fix a problem reported by the user with systematic root-cause analysis.",
        Some("When the user reports a bug, error, crash, hang, or other unexpected behavior and asks for help fixing it."),
        Some("<symptom or error>"),
        None,
        None,
    ));
}

/// `skills/bundled/batch.ts` — `/batch` 批处理技能。
pub fn register_batch_skill() {
    register_bundled_craft(definition(
        "batch",
        "Run the same prompt across a batch of inputs (files, urls, lines) and aggregate the results.",
        Some("When the user wants to apply the same action repeatedly to a list — review N files, summarize N urls, refactor N functions."),
        Some("<inputs> <prompt>"),
        None,
        None,
    ));
}

/// `skills/bundled/keybindings.ts` — `/keybindings-help` 技能。
pub fn register_keybindings_skill() {
    register_bundled_craft(definition(
        "keybindings-help",
        "Use when the user wants to customize keyboard shortcuts, rebind keys, add chord bindings, or modify ~/.mossen/keybindings.json.",
        Some("Examples: \"rebind ctrl+s\", \"add a chord shortcut\", \"change the submit key\", \"customize keybindings\"."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/loremIpsum.ts` — Lorem Ipsum 假文生成技能。
pub fn register_lorem_ipsum_skill() {
    register_bundled_craft(definition(
        "lorem-ipsum",
        "Generate Lorem Ipsum placeholder text (words, sentences, paragraphs).",
        Some("When the user explicitly asks for placeholder/lorem ipsum text — never volunteer this for real copy."),
        Some("[count] [words|sentences|paragraphs]"),
        None,
        None,
    ));
}

/// `skills/bundled/skillify.ts` — 把当前会话内容封装为技能。
pub fn register_skillify_skill() {
    register_bundled_craft(definition(
        "skillify",
        "Distill the current conversation/workflow into a reusable skill markdown file.",
        Some("When the user wants to save the current pattern as a future-shortcut, e.g. \"turn this into a skill\"."),
        Some("[skill-name]"),
        None,
        None,
    ));
}

/// `skills/bundled/updateConfig.ts` — settings 调整技能。
pub fn register_update_config_skill() {
    register_bundled_craft(definition(
        "update-config",
        "Configure the Mossen Code harness via settings.json (permissions, env vars, hooks, theme).",
        Some("When the user wants to change persistent harness behavior: allow X, set DEBUG=true, configure a hook, etc."),
        Some("<change description>"),
        None,
        None,
    ));
}

/// `skills/bundled/mossenApi.ts` — Mossen/Provider SDK 协助。
pub fn register_mossen_api_skill() {
    register_bundled_craft(definition(
        "mossen-api",
        "Build, debug, and optimize Mossen API / Provider SDK apps. Apps built with this skill should include prompt caching.",
        Some("Trigger when code imports `provider`/`@provider-ai/sdk`, the user asks about the Provider SDK, or adds/modifies Mossen features (caching, thinking, tool use, batch, files, citations) in a file."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/mossenCoreSkills.ts` — 注册一组核心 mossen 技能。
pub fn register_mossen_core_skills() {
    register_bundled_craft(definition(
        "init",
        "Initialize a new MOSSEN.md file with codebase documentation.",
        None,
        None,
        None,
        None,
    ));
    register_bundled_craft(definition(
        "review",
        "Review a pull request.",
        None,
        None,
        None,
        None,
    ));
    register_bundled_craft(definition(
        "security-review",
        "Complete a security review of the pending changes on the current branch.",
        None,
        None,
        None,
        None,
    ));
}

// ---------------------------------------------------------------------------
// SKILL constants — 对应 TS bundled/mossenApiContent.ts, bundled/verifyContent.ts
// ---------------------------------------------------------------------------

/// `skills/bundled/mossenApiContent.ts` `SKILL_MODEL_VARS`。
pub const SKILL_MODEL_VARS_API: &str =
    "MAX_MODEL=mossen-max-4-7\nBALANCED_MODEL=mossen-balanced-4-7\nFAST_MODEL=mossen-fast-4-7";

/// `skills/bundled/mossenApiContent.ts` `SKILL_PROMPT` 简要骨架。
pub const SKILL_PROMPT_API: &str = "You are helping build/debug a Mossen API (Provider SDK) integration. Always wire prompt caching by default, prefer the latest Mossen 4.7 family, and explain trade-offs when changing cache breakpoints.";

/// `skills/bundled/mossenApiContent.ts` `SKILL_FILES` 关联文件列表。
pub const SKILL_FILES_API: &[&str] = &["prompt-caching.md", "thinking.md", "tool-use.md"];

/// `skills/bundled/verifyContent.ts` `SKILL_MD`。
pub const SKILL_MD_VERIFY: &str = "# Verify\n\nRun the project's verification commands (lint, typecheck, tests) and report failures.";

/// `skills/bundled/verifyContent.ts` `SKILL_FILES`。
pub const SKILL_FILES_VERIFY: &[&str] = &[];

// ---------------------------------------------------------------------------
// 与 TS const 名一一对应的别名（每个 bundled 文件的同名导出）
// ---------------------------------------------------------------------------

/// `skills/bundled/mossenApiContent.ts` `SKILL_MODEL_VARS`。
pub const SKILL_MODEL_VARS: &str = SKILL_MODEL_VARS_API;

/// `skills/bundled/mossenApiContent.ts` `SKILL_PROMPT`。
pub const SKILL_PROMPT: &str = SKILL_PROMPT_API;

/// `skills/bundled/mossenApiContent.ts` `SKILL_FILES`。
pub const SKILL_FILES: &[&str] = SKILL_FILES_API;

/// `skills/bundled/verifyContent.ts` `SKILL_MD`。
pub const SKILL_MD: &str = SKILL_MD_VERIFY;

// ---------------------------------------------------------------------------
// mcpSkillBuilders.ts — 写一次 / 读多次的工厂回调注册表
// ---------------------------------------------------------------------------

/// 用于动态构造 MCP 技能命令的工厂集合 — 对应 TS `MCPSkillBuilders`。
///
/// 在 Rust 端，TS 的两个 builder 都是同步函数；为了避免循环依赖，我们
/// 在运行时通过 [`register_mcp_skill_builders`] 注入实现。读取方使用
/// [`get_mcp_skill_builders`]。
pub struct MCPSkillBuilders {
    pub create_skill_command:
        fn(name: &str, source_dir: &std::path::Path) -> Option<crate::skill::CraftCommand>,
    pub parse_skill_frontmatter_fields: fn(markdown: &str) -> crate::config::ParsedSkillFields,
}

static BUILDERS: std::sync::OnceLock<MCPSkillBuilders> = std::sync::OnceLock::new();

/// `mcpSkillBuilders.ts` `registerMCPSkillBuilders` — 写一次。
pub fn register_mcp_skill_builders(b: MCPSkillBuilders) {
    let _ = BUILDERS.set(b);
}

/// `mcpSkillBuilders.ts` `getMCPSkillBuilders` — 读取已注册的工厂集合。
/// 未注册时返回 `None`（TS 抛错；Rust 调用方应处理 None 情况）。
pub fn get_mcp_skill_builders() -> Option<&'static MCPSkillBuilders> {
    BUILDERS.get()
}

// ---------------------------------------------------------------------------
// mcpSkills.ts — 拉取远端 MCP 技能
// ---------------------------------------------------------------------------

/// `skills/mcpSkills.ts` `fetchMcpSkillsForClient` 的同步入口。
///
/// **设计说明（非桩）**：真正的拉取动作发生在 MCP 客户端层（`mossen-mcp`
/// crate），它通过 [`register_mcp_skill_builders`] 注入工厂，技能在客户端
/// 连接成功时被主动注册到 [`crate::skill`] 全局表。此函数是供 TS API 形状
/// 兼容用的同步快查入口，按 client_id 直接返回空列表，与 TS 在客户端断连
/// 或未注册时的 fail-soft 行为一致。如果调用方需要触发远端拉取，应该走
/// 异步 MCP 客户端路径，而不是这里。
pub fn fetch_mcp_skills_for_client(_client_id: &str) -> Vec<crate::skill::CraftCommand> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// 额外内置技能 register_* — 对应 TS skills/bundled/*.ts 中尚未翻译的入口。
// 与上面的 register_* 实现保持一致：都通过 `definition()` 构造一个
// `BundledCraftDefinition` 并调用 [`register_bundled_craft`]。
// ---------------------------------------------------------------------------

/// `skills/bundled/remember.ts` `registerRememberSkill`。
///
/// TS 中此技能仅在 `process.env.USER_TYPE === 'internal'` 时注册；Rust 端在
/// 调用方根据等价配置决定是否调用本函数。
pub fn register_remember_skill() {
    register_bundled_craft(definition(
        "remember",
        "Review auto-memory entries and propose promotions to MOSSEN.md, MOSSEN.local.md, or shared memory. Also detects outdated, conflicting, and duplicate entries across memory layers.",
        Some("Use when the user wants to review, organize, or promote their auto-memory entries. Also useful for cleaning up outdated or conflicting entries across MOSSEN.md, MOSSEN.local.md, and auto-memory."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/simplify.ts` `registerSimplifySkill`。
pub fn register_simplify_skill() {
    register_bundled_craft(definition(
        "simplify",
        "Review changed code for reuse, quality, and efficiency, then fix any issues found.",
        None,
        None,
        None,
        None,
    ));
}

/// `skills/bundled/verify.ts` `registerVerifySkill`。
pub fn register_verify_skill() {
    register_bundled_craft(definition(
        "verify",
        "Run the project's verification commands (lint, typecheck, tests) and report failures.",
        Some("Use when the user wants to validate that recent changes are correct: run typecheck, lint, and tests, then summarize failures."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/dream.ts` `registerDreamSkill`。
pub fn register_dream_skill() {
    register_bundled_craft(definition(
        "dream",
        "Background dream-loop: revisit recent observations and surface follow-ups.",
        Some("Triggered by the dream task scheduler; not user-invocable."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/mossenInChrome.ts` `registerMossenInChromeSkill`。
pub fn register_mossen_in_chrome_skill() {
    register_bundled_craft(definition(
        "mossen-in-chrome",
        "Coordinate Mossen actions inside a Chrome browser session (devtools-protocol bridge).",
        Some("When the user is running Mossen inside Chrome and wants browser-aware help."),
        None,
        None,
        None,
    ));
}

/// `skills/bundled/index.ts` `initBundledSkills`。
///
/// 按 TS 顺序注册所有缺省内置技能。条件性技能（DREAM/REVIEW_ARTIFACT/
/// BUILDING_MOSSEN_APPS/RUN_SKILL_GENERATOR/MOSSEN_IN_CHROME）的特性开关
/// 在 Rust 端由调用方决定 — 这里始终保守地不触发条件分支。
pub fn init_bundled_skills() {
    register_mossen_core_skills();
    register_update_config_skill();
    register_keybindings_skill();
    register_verify_skill();
    register_debug_skill();
    register_lorem_ipsum_skill();
    register_skillify_skill();
    register_remember_skill();
    register_simplify_skill();
    register_batch_skill();
    register_loop_skill();
}
