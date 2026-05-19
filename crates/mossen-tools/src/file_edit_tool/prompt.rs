use super::constants::FILE_EDIT_TOOL_NAME;

/// File read tool name reference (avoids circular dependency).
const FILE_READ_TOOL_NAME: &str = "Read";

fn get_pre_read_instruction() -> String {
    format!(
        "\n- You must use your `{}` tool at least once in the conversation before editing. \
         This tool will error if you attempt an edit without reading the file. ",
        FILE_READ_TOOL_NAME
    )
}

/// Returns the description for the edit tool.
pub fn get_edit_tool_description() -> String {
    get_default_edit_description()
}

fn get_default_edit_description() -> String {
    let is_compact = is_compact_line_prefix_enabled();
    let prefix_format = if is_compact {
        "line number + tab"
    } else {
        "spaces + line number + arrow"
    };

    let minimal_uniqueness_hint =
        if std::env::var("USER_TYPE").unwrap_or_default() == "mossen" {
            "\n- Use the smallest old_string that's clearly unique — usually 2-4 adjacent lines \
             is sufficient. Avoid including 10+ lines of context when less uniquely identifies the target."
        } else {
            ""
        };

    format!(
        "Performs exact string replacements in files.\n\n\
         Usage:{pre_read}\n\
         - When editing text from Read tool output, ensure you preserve the exact indentation \
         (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: \
         {prefix_format}. Everything after that is the actual file content to match. Never include \
         any part of the line number prefix in the old_string or new_string.\n\
         - ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n\
         - Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.\n\
         - The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string \
         with more surrounding context to make it unique or use `replace_all` to change every instance \
         of `old_string`.{uniqueness_hint}\n\
         - Use `replace_all` for replacing and renaming strings across the file. This parameter is \
         useful if you want to rename a variable for instance.",
        pre_read = get_pre_read_instruction(),
        prefix_format = prefix_format,
        uniqueness_hint = minimal_uniqueness_hint,
    )
}

/// Whether compact line prefix format (line_number + tab) is enabled.
fn is_compact_line_prefix_enabled() -> bool {
    std::env::var("MOSSEN_COMPACT_LINE_PREFIX")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Returns the tool name for display purposes.
pub fn user_facing_name() -> &'static str {
    FILE_EDIT_TOOL_NAME
}
