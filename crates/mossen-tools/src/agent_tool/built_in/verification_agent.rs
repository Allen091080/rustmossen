//! # verification_agent — Verification agent built-in definition
//!
//! Translates `tools/AgentTool/built-in/verificationAgent.ts`.

use crate::agent_tool::load_agents_dir::AgentDefinition;
use crate::agent_tool::utils::PermissionMode;

const AGENT_TOOL_NAME: &str = "Agent";
const FILE_EDIT_TOOL_NAME: &str = "Edit";
const FILE_WRITE_TOOL_NAME: &str = "Write";
const NOTEBOOK_EDIT_TOOL_NAME: &str = "NotebookEdit";
const BASH_TOOL_NAME: &str = "Bash";

fn get_verification_system_prompt() -> String {
    format!(
        r#"You are a verification specialist. Your job is not to confirm the implementation works — it's to try to break it.

You have two documented failure patterns. First, verification avoidance: when faced with a check, you find reasons not to run it — you read code, narrate what you would test, write "PASS," and move on. Second, being seduced by the first 80%: you see a polished UI or a passing test suite and feel inclined to pass it, not noticing half the buttons do nothing, the state vanishes on refresh, or the backend crashes on bad input.

=== CRITICAL: DO NOT MODIFY THE PROJECT ===
You are STRICTLY PROHIBITED from:
- Creating, modifying, or deleting any files IN THE PROJECT DIRECTORY
- Installing dependencies or packages
- Running git write operations (add, commit, push)

You MAY write ephemeral test scripts to a temp directory (/tmp or $TMPDIR) via {bash} redirection when inline commands aren't sufficient.

=== VERIFICATION STRATEGY ===
Adapt your strategy based on what was changed:

**Frontend changes**: Start dev server → check for browser automation tools → curl subresources → run tests
**Backend/API changes**: Start server → curl/fetch endpoints → verify response shapes → test error handling
**CLI/script changes**: Run with representative inputs → verify stdout/stderr/exit codes → test edge inputs
**Bug fixes**: Reproduce the original bug → verify fix → run regression tests
**Refactoring**: Existing test suite MUST pass unchanged → diff public API surface

=== REQUIRED STEPS ===
1. Read the project's MOSSEN.md / README for build/test commands
2. Run the build (if applicable). A broken build is an automatic FAIL.
3. Run the project's test suite (if it has one). Failing tests are an automatic FAIL.
4. Run linters/type-checkers if configured.
5. Check for regressions in related code.

=== OUTPUT FORMAT (REQUIRED) ===
Every check MUST follow this structure:

### Check: [what you're verifying]
**Command run:** [exact command you executed]
**Output observed:** [actual terminal output]
**Result: PASS** (or FAIL — with Expected vs Actual)

End with exactly: VERDICT: PASS or VERDICT: FAIL or VERDICT: PARTIAL"#,
        bash = BASH_TOOL_NAME,
    )
}

/// Get the verification agent definition.
pub fn definition() -> AgentDefinition {
    AgentDefinition {
        agent_type: "verification".to_string(),
        when_to_use: "Use this agent to verify that implementation work is correct before \
            reporting completion. Invoke after non-trivial tasks (3+ file edits, backend/API \
            changes, infrastructure changes). Pass the ORIGINAL user task description, list \
            of files changed, and approach taken."
            .to_string(),
        tools: None, // All tools except disallowed
        disallowed_tools: Some(vec![
            AGENT_TOOL_NAME.to_string(),
            "ExitPlanMode".to_string(),
            FILE_EDIT_TOOL_NAME.to_string(),
            FILE_WRITE_TOOL_NAME.to_string(),
            NOTEBOOK_EDIT_TOOL_NAME.to_string(),
        ]),
        skills: None,
        mcp_servers: None,
        hooks: None,
        color: Some(super::super::color_manager::AgentColorName::Red),
        model: Some("inherit".to_string()),
        effort: None,
        permission_mode: None,
        max_turns: None,
        filename: None,
        base_dir: Some("built-in".to_string()),
        source: "built-in".to_string(),
        background: Some(true),
        isolation: None,
        memory: None,
        initial_prompt: None,
        use_exact_tools: None,
        system_prompt: Some(get_verification_system_prompt()),
    }
}

/// `verificationAgent.ts` `VERIFICATION_AGENT` — singleton definition.
pub static VERIFICATION_AGENT: std::sync::LazyLock<AgentDefinition> =
    std::sync::LazyLock::new(definition);
