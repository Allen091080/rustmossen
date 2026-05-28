//! # prompt — Agent tool prompt generation
//!
//! Translates `tools/AgentTool/prompt.ts`.
//! Generates the system prompt for the Agent tool, including agent listing,
//! usage examples, and context-aware sections.

use std::env;

use super::constants::AGENT_TOOL_NAME;
use super::fork_subagent::is_fork_subagent_enabled;
use super::load_agents_dir::AgentDefinition;

/// File tool names for cross-references in prompts.
const FILE_READ_TOOL_NAME: &str = "Read";
const FILE_WRITE_TOOL_NAME: &str = "Write";
const GLOB_TOOL_NAME: &str = "Glob";
const GREP_TOOL_NAME: &str = "Grep";
const TASK_OUTPUT_TOOL_NAME: &str = "TaskOutput";

/// Check if embedded search tools are available.
fn has_embedded_search_tools() -> bool {
    env::var("MOSSEN_EMBEDDED_SEARCH_TOOLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Check if the current session is a teammate.
fn is_teammate() -> bool {
    env::var("MOSSEN_IS_TEAMMATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Check if this is an in-process teammate.
fn is_in_process_teammate() -> bool {
    env::var("MOSSEN_IN_PROCESS_TEAMMATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Whether the agent list should be injected as an attachment message instead of inline.
pub fn should_inject_agent_list_in_messages() -> bool {
    env::var("MOSSEN_FEATURE_AGENT_LIST_VIA_ATTACHMENT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Get the tools description for an agent.
fn get_tools_description(agent: &AgentDefinition) -> String {
    let has_allowlist = agent.tools.as_ref().is_some_and(|t| !t.is_empty());
    let has_denylist = agent
        .disallowed_tools
        .as_ref()
        .is_some_and(|t| !t.is_empty());

    if has_allowlist && has_denylist {
        let deny_set: std::collections::HashSet<&str> = agent
            .disallowed_tools
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.as_str())
            .collect();
        let effective: Vec<&str> = agent
            .tools
            .as_ref()
            .unwrap()
            .iter()
            .filter(|t| !deny_set.contains(t.as_str()))
            .map(|s| s.as_str())
            .collect();
        if effective.is_empty() {
            return "None".to_string();
        }
        effective.join(", ")
    } else if has_allowlist {
        agent.tools.as_ref().unwrap().join(", ")
    } else if has_denylist {
        format!(
            "All tools except {}",
            agent.disallowed_tools.as_ref().unwrap().join(", ")
        )
    } else {
        "All tools".to_string()
    }
}

/// Format one agent line for the agent listing:
/// `- type: whenToUse (Tools: ...)`
pub fn format_agent_line(agent: &AgentDefinition) -> String {
    let tools_description = get_tools_description(agent);
    format!(
        "- {}: {} (Tools: {})",
        agent.agent_type, agent.when_to_use, tools_description
    )
}

/// Generate the full Agent tool prompt.
pub fn get_agent_tool_prompt(effective_agents: &[AgentDefinition], is_coordinator: bool) -> String {
    let fork_enabled = is_fork_subagent_enabled();
    let list_via_attachment = should_inject_agent_list_in_messages();

    let agent_list_section = if list_via_attachment {
        "Available agent types are listed in <system-reminder> messages in the conversation."
            .to_string()
    } else {
        format!(
            "Available agent types and the tools they have access to:\n{}",
            effective_agents
                .iter()
                .map(format_agent_line)
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let subagent_type_note = if fork_enabled {
        format!(
            "When using the {} tool, specify a subagent_type to use a specialized agent, \
             or omit it to fork yourself — a fork inherits your full conversation context.",
            AGENT_TOOL_NAME
        )
    } else {
        format!(
            "When using the {} tool, specify a subagent_type parameter to select which agent \
             type to use. If omitted, the general-purpose agent is used.",
            AGENT_TOOL_NAME
        )
    };

    // Shared core prompt
    let shared = format!(
        "Launch a new agent to handle complex, multi-step tasks autonomously.\n\n\
         The {tool} tool launches specialized agents (subprocesses) that autonomously handle \
         complex tasks. Each agent type has specific capabilities and tools available to it.\n\n\
         {agent_list}\n\n\
         {subagent_note}",
        tool = AGENT_TOOL_NAME,
        agent_list = agent_list_section,
        subagent_note = subagent_type_note,
    );

    // Coordinator mode gets the slim prompt
    if is_coordinator {
        return shared;
    }

    // Build full prompt with all sections
    let embedded = has_embedded_search_tools();
    let file_search_hint = if embedded {
        "`find` via the Bash tool".to_string()
    } else {
        format!("the {} tool", GLOB_TOOL_NAME)
    };
    let content_search_hint = if embedded {
        "`grep` via the Bash tool".to_string()
    } else {
        format!("the {} tool", GLOB_TOOL_NAME)
    };

    let when_not_to_use = if fork_enabled {
        String::new()
    } else {
        format!(
            "\nWhen NOT to use the {tool} tool:\n\
             - If you want to read a specific file path, use the {read} tool or {search} \
               instead of the {tool} tool, to find the match more quickly\n\
             - If you are searching for a specific class definition like \"class Foo\", use \
               {content} instead, to find the match more quickly\n\
             - If you are searching for code within a specific file or set of 2-3 files, use \
               the {read} tool instead of the {tool} tool, to find the match more quickly\n\
             - Other tasks that are not related to the agent descriptions above\n",
            tool = AGENT_TOOL_NAME,
            read = FILE_READ_TOOL_NAME,
            search = file_search_hint,
            content = content_search_hint,
        )
    };

    let background_note = if !env::var("MOSSEN_CODE_DISABLE_BACKGROUND_TASKS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        && !is_in_process_teammate()
        && !fork_enabled
    {
        "\n- You can optionally run agents in the background using the run_in_background \
         parameter. When an agent runs in the background, you will be automatically notified \
         when it completes — do NOT sleep, poll, or proactively check on its progress. \
         Continue with other work or respond to the user instead.\n\
         - **Foreground vs background**: Use foreground (default) when you need the agent's \
         results before you can proceed — e.g., research agents whose findings inform your \
         next steps. Use background when you have genuinely independent work to do in parallel."
            .to_string()
    } else {
        String::new()
    };

    let teammate_note = if is_in_process_teammate() {
        "\n- The run_in_background, name, team_name, and mode parameters are not available \
         in this context. Only synchronous subagents are supported."
            .to_string()
    } else if is_teammate() {
        "\n- The name, team_name, and mode parameters are not available in this context — \
         teammates cannot spawn other teammates. Omit them to spawn a subagent."
            .to_string()
    } else {
        String::new()
    };

    format!(
        "{shared}\n\
         {when_not_to_use}\n\
         Usage notes:\n\
         - Always include a short description (3-5 words) summarizing what the agent will do\
         {background_note}\n\
         - When the agent is done, it will return a single message back to you. The result \
           returned by the agent is not visible to the user. To show the user the result, you \
           should send a text message back to the user with a concise summary of the result.\n\
         - If the agent runs in the background, use the returned `task_id` with {task_output}; \
           treat the work as complete only after {task_output} returns a ready completed result.\n\
         - If {task_output} returns `retrieval_status: \"not_ready\"`, the agent is still running; \
           call {task_output} again with the same `task_id` instead of treating the launch as failed \
           or duplicating the work yourself.\n\
         - Each Agent invocation starts fresh — provide a complete task description.\n\
         - The agent's outputs should generally be trusted\n\
         - Clearly tell the agent whether you expect it to write code or just to do research \
           (search, file reads, web fetches, etc.)\n\
         - If the agent description mentions that it should be used proactively, then you should \
           try your best to use it without the user having to ask for it first.\n\
         - If the user specifies that they want you to run agents \"in parallel\", you MUST send \
           a single message with multiple {tool} tool use content blocks.\n\
         - You can optionally set `isolation: \"worktree\"` to run the agent in a temporary git \
           worktree, giving it an isolated copy of the repository.\
         {teammate_note}",
        shared = shared,
        when_not_to_use = when_not_to_use,
        background_note = background_note,
        task_output = TASK_OUTPUT_TOOL_NAME,
        tool = AGENT_TOOL_NAME,
        teammate_note = teammate_note,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_agent_prompt_points_background_results_to_taskoutput() {
        let prompt = get_agent_tool_prompt(&[], false);

        assert!(prompt.contains(TASK_OUTPUT_TOOL_NAME), "{prompt}");
        assert!(prompt.contains("not_ready"), "{prompt}");
        assert!(prompt.contains("duplicating the work yourself"), "{prompt}");
        assert!(
            !prompt.contains("SendMessage"),
            "personal Agent prompt must not advertise unwired continuation tools:\n{prompt}"
        );
    }
}
