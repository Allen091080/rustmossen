//! Session memory prompts — prompt templates for memory extraction agent.

/// Build the extraction prompt for auto-only memory (no team memory).
pub fn build_extract_auto_only_prompt(
    new_message_count: usize,
    existing_memories: &str,
    skip_index: bool,
) -> String {
    let manifest = if !existing_memories.is_empty() {
        format!(
            "\n\n## Existing memory files\n\n{}\n\nCheck this list before writing — update an existing file rather than creating a duplicate.",
            existing_memories
        )
    } else {
        String::new()
    };

    let opener = format!(
        "You are now acting as the memory extraction subagent. Analyze the most recent ~{} messages above and use them to update your persistent memory systems.\n\n\
        Available tools: Read, Grep, Glob, read-only Bash (ls/find/cat/stat/wc/head/tail and similar), and Edit/Write for paths inside the memory directory only.\n\n\
        You have a limited turn budget. Edit requires a prior Read of the same file, so the efficient strategy is: turn 1 — issue all Read calls in parallel for every file you might update; turn 2 — issue all Write/Edit calls in parallel.\n\n\
        You MUST only use content from the last ~{} messages to update your persistent memories. Do not waste any turns attempting to investigate or verify that content further.{}",
        new_message_count, new_message_count, manifest
    );

    let how_to_save = if skip_index {
        "## How to save memories\n\n\
        Write each memory to its own file (e.g., `user_role.md`, `feedback_testing.md`) using frontmatter format.\n\n\
        - Organize memory semantically by topic, not chronologically\n\
        - Update or remove memories that turn out to be wrong or outdated\n\
        - Do not write duplicate memories. First check if there is an existing memory you can update before writing a new one."
    } else {
        "## How to save memories\n\n\
        Saving a memory is a two-step process:\n\n\
        **Step 1** — write the memory to its own file (e.g., `user_role.md`, `feedback_testing.md`) using frontmatter format.\n\n\
        **Step 2** — add a pointer to that file in `MEMORY.md`. `MEMORY.md` is an index, not a memory — each entry should be one line, under ~150 characters.\n\n\
        - `MEMORY.md` is always loaded into your system prompt — lines after 200 will be truncated, so keep the index concise\n\
        - Organize memory semantically by topic, not chronologically\n\
        - Update or remove memories that turn out to be wrong or outdated\n\
        - Do not write duplicate memories. First check if there is an existing memory you can update before writing a new one."
    };

    format!(
        "{}\n\n\
        If the user explicitly asks you to remember something, save it immediately as whichever type fits best. If they ask you to forget something, find and remove the relevant entry.\n\
        For project continuity, if the recent messages show work that may be resumed after closing/reopening this project, save or update exactly one concise project handoff memory.\n\n\
        {}",
        opener, how_to_save
    )
}

/// Compose the update prompt — `(existing_memory_dump, messages_json)`.
pub fn build_session_memory_update_prompt(existing_memory: &str, messages_json: &str) -> String {
    format!(
        "Update session memory.\n\nExisting memory:\n{}\n\nMessages JSON:\n{}\n",
        existing_memory, messages_json
    )
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/SessionMemory/prompts.ts` exports.
// ---------------------------------------------------------------------------

/// `prompts.ts` `DEFAULT_SESSION_MEMORY_TEMPLATE`.
pub const DEFAULT_SESSION_MEMORY_TEMPLATE: &str = r#"# Session Memory

<!-- This is a session memory file maintained by Mossen. -->
<!-- Edit it manually only when you want to seed the next session. -->

## Notes

(populated by the agent)
"#;

/// `prompts.ts` `loadSessionMemoryTemplate`.
pub async fn load_session_memory_template() -> String {
    DEFAULT_SESSION_MEMORY_TEMPLATE.to_string()
}

/// `prompts.ts` `loadSessionMemoryPrompt`.
pub async fn load_session_memory_prompt() -> String {
    build_session_memory_update_prompt("", "[]")
}

/// `prompts.ts` `isSessionMemoryEmpty`.
pub async fn is_session_memory_empty(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }
    let stripped = trimmed
        .lines()
        .filter(|l| !l.starts_with("# ") && !l.starts_with("<!--"))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    stripped.is_empty() || stripped == "(populated by the agent)"
}

/// `prompts.ts` `buildSessionMemoryUpdatePrompt` — async wrapper.
pub async fn build_session_memory_update_prompt_async(
    existing_memory: &str,
    messages_json: &str,
) -> String {
    build_session_memory_update_prompt(existing_memory, messages_json)
}

/// `prompts.ts` `truncateSessionMemoryForCompact` — `(truncated_text, was_truncated)`.
pub fn truncate_session_memory_for_compact(content: &str) -> (String, bool) {
    const MAX_CHARS: usize = 16_384;
    if content.chars().count() <= MAX_CHARS {
        return (content.to_string(), false);
    }
    let mut truncated = mossen_utils::string_utils::prefix_chars(content, MAX_CHARS);
    truncated.push_str("\n\n[…session memory truncated for compaction]");
    (truncated, true)
}

/// Build the extraction prompt for combined auto + team memory.
pub fn build_extract_combined_prompt(
    new_message_count: usize,
    existing_memories: &str,
    skip_index: bool,
) -> String {
    // Team memory variant adds scope guidance
    let base = build_extract_auto_only_prompt(new_message_count, existing_memories, skip_index);
    format!(
        "{}\n\n\
        - You MUST avoid saving sensitive data within shared team memories. For example, never save API keys or user credentials.",
        base
    )
}

#[cfg(test)]
mod tests {
    use super::truncate_session_memory_for_compact;

    #[test]
    fn compact_memory_truncates_multibyte_on_char_boundary() {
        let content = "逐行阅读代码".repeat(4096);

        let (truncated, was_truncated) = truncate_session_memory_for_compact(&content);

        assert!(was_truncated);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(truncated.contains("session memory truncated"));
    }
}
