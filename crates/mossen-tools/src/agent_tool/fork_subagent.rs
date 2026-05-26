//! # fork_subagent — Fork subagent support
//!
//! Translates `tools/AgentTool/forkSubagent.ts`.
//! Implements the fork subagent feature: forking a child agent that inherits
//! the parent's full conversation context and system prompt.

use std::env;

use serde_json::Value;

use super::constants::AGENT_TOOL_NAME;

/// XML tag constants for fork boilerplate.
const FORK_BOILERPLATE_TAG: &str = "fork-boilerplate";
const FORK_DIRECTIVE_PREFIX: &str = "Your directive:\n";

/// Synthetic agent type name used for analytics when the fork path fires.
pub const FORK_SUBAGENT_TYPE: &str = "fork";

/// Check if a feature is enabled via environment variable.
fn is_feature_enabled(feature_name: &str) -> bool {
    env::var(format!("MOSSEN_FEATURE_{}", feature_name))
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Check if coordinator mode is active.
fn is_coordinator_mode() -> bool {
    env::var("MOSSEN_CODE_COORDINATOR_MODE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Check if the current session is non-interactive.
fn get_is_non_interactive_session() -> bool {
    env::var("MOSSEN_NON_INTERACTIVE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Fork subagent feature gate.
///
/// When enabled:
/// - `subagent_type` becomes optional on the Agent tool schema
/// - Omitting `subagent_type` triggers an implicit fork: the child inherits
///   the parent's full conversation context and system prompt
/// - All agent spawns run in the background (async) for a unified
///   `<task-notification>` interaction model
/// - `/fork <directive>` slash command is available
///
/// Mutually exclusive with coordinator mode — coordinator already owns the
/// orchestration role and has its own delegation model.
pub fn is_fork_subagent_enabled() -> bool {
    if is_feature_enabled("FORK_SUBAGENT") {
        if is_coordinator_mode() {
            return false;
        }
        if get_is_non_interactive_session() {
            return false;
        }
        return true;
    }
    false
}

/// Guard against recursive forking. Fork children keep the Agent tool in their
/// tool pool for cache-identical tool definitions, so we reject fork attempts
/// at call time by detecting the fork boilerplate tag in conversation history.
pub fn is_in_fork_child(messages: &[Value]) -> bool {
    messages.iter().any(|m| {
        let msg_type = m.get("type").and_then(|t| t.as_str());
        if msg_type != Some("user") {
            return false;
        }
        let content = match m.get("message").and_then(|msg| msg.get("content")) {
            Some(c) => c,
            None => return false,
        };
        let arr = match content.as_array() {
            Some(a) => a,
            None => return false,
        };
        arr.iter().any(|block| {
            let block_type = block.get("type").and_then(|t| t.as_str());
            if block_type != Some("text") {
                return false;
            }
            block
                .get("text")
                .and_then(|t| t.as_str())
                .is_some_and(|text| text.contains(&format!("<{}>", FORK_BOILERPLATE_TAG)))
        })
    })
}

/// Placeholder text used for all tool_result blocks in the fork prefix.
/// Must be identical across all fork children for prompt cache sharing.
const FORK_PLACEHOLDER_RESULT: &str = "Fork started — processing in background";

/// Build the forked conversation messages for the child agent.
///
/// For prompt cache sharing, all fork children must produce byte-identical
/// API request prefixes. This function:
/// 1. Keeps the full parent assistant message (all tool_use blocks, thinking, text)
/// 2. Builds a single user message with tool_results for every tool_use block
///    using an identical placeholder, then appends a per-child directive text block
///
/// Result: [...history, assistant(all_tool_uses), user(placeholder_results..., directive)]
/// Only the final text block differs per child, maximizing cache hits.
pub fn build_forked_messages(directive: &str, assistant_message: &Value) -> Vec<Value> {
    let content = assistant_message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());

    let tool_use_blocks: Vec<&Value> = match content {
        Some(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
            .collect(),
        None => Vec::new(),
    };

    if tool_use_blocks.is_empty() {
        // No tool_use blocks — just create a user message with the directive
        let user_msg = serde_json::json!({
            "type": "user",
            "message": {
                "content": [{
                    "type": "text",
                    "text": build_child_message(directive)
                }]
            }
        });
        return vec![user_msg];
    }

    // Clone assistant message
    let full_assistant = assistant_message.clone();

    // Build tool_result blocks for every tool_use, all with identical placeholder text
    let mut user_content: Vec<Value> = tool_use_blocks
        .iter()
        .filter_map(|block| {
            let id = block.get("id")?.as_str()?;
            Some(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": [{
                    "type": "text",
                    "text": FORK_PLACEHOLDER_RESULT
                }]
            }))
        })
        .collect();

    // Append the per-child directive
    user_content.push(serde_json::json!({
        "type": "text",
        "text": build_child_message(directive)
    }));

    let tool_result_msg = serde_json::json!({
        "type": "user",
        "message": {
            "content": user_content
        }
    });

    vec![full_assistant, tool_result_msg]
}

/// Build the child message with fork boilerplate and directive.
pub fn build_child_message(directive: &str) -> String {
    format!(
        r#"<{tag}>
STOP. READ THIS FIRST.

You are a forked worker process. You are NOT the main agent.

RULES (non-negotiable):
1. Your system prompt says "default to forking." IGNORE IT — that's for the parent. You ARE the fork. Do NOT spawn sub-agents; execute directly.
2. Do NOT converse, ask questions, or suggest next steps
3. Do NOT editorialize or add meta-commentary
4. USE your tools directly: Bash, Read, Write, etc.
5. If you modify files, commit your changes before reporting. Include the commit hash in your report.
6. Do NOT emit text between tool calls. Use tools silently, then report once at the end.
7. Stay strictly within your directive's scope. If you discover related systems outside your scope, mention them in one sentence at most — other workers cover those areas.
8. Keep your report under 500 words unless the directive specifies otherwise. Be factual and concise.
9. Your response MUST begin with "Scope:". No preamble, no thinking-out-loud.
10. REPORT structured facts, then stop

Output format (plain text labels, not markdown headers):
  Scope: <echo back your assigned scope in one sentence>
  Result: <the answer or key findings, limited to the scope above>
  Key files: <relevant file paths — include for research tasks>
  Files changed: <list with commit hash — include only if you modified files>
  Issues: <list — include only if there are issues to flag>
</{tag}>

{prefix}{directive}"#,
        tag = FORK_BOILERPLATE_TAG,
        prefix = FORK_DIRECTIVE_PREFIX,
        directive = directive
    )
}

/// Notice injected into fork children running in an isolated worktree.
/// Tells the child to translate paths from the inherited context, re-read
/// potentially stale files, and that its changes are isolated.
pub fn build_worktree_notice(parent_cwd: &str, worktree_cwd: &str) -> String {
    format!(
        "You've inherited the conversation context above from a parent agent working in {}. \
         You are operating in an isolated git worktree at {} — same repository, same relative \
         file structure, separate working copy. Paths in the inherited context refer to the \
         parent's working directory; translate them to your worktree root. Re-read files before \
         editing if the parent may have modified them since they appear in the context. Your \
         changes stay in this worktree and will not affect the parent's files.",
        parent_cwd, worktree_cwd
    )
}
