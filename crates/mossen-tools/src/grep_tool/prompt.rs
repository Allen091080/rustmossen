/// Grep tool name.
pub const GREP_TOOL_NAME: &str = "Grep";

/// Agent tool name reference.
const AGENT_TOOL_NAME: &str = "Agent";
/// Bash tool name reference.
const BASH_TOOL_NAME: &str = "Bash";

/// Get the grep tool description/prompt.
pub fn get_description() -> String {
    format!(
        "A powerful search tool built on ripgrep\n\n\
         Usage:\n\
         - ALWAYS use {grep} for search tasks. NEVER invoke `grep` or `rg` as a {bash} command. \
         The {grep} tool has been optimized for correct permissions and access.\n\
         - Supports full regex syntax (e.g., \"log.*Error\", \"function\\\\s+\\\\w+\")\n\
         - Filter files with glob parameter (e.g., \"*.js\", \"**/*.tsx\") or type parameter \
         (e.g., \"js\", \"py\", \"rust\")\n\
         - Output modes: \"content\" shows matching lines, \"files_with_matches\" shows only file paths \
         (default), \"count\" shows match counts\n\
         - Use {agent} tool for open-ended searches requiring multiple rounds\n\
         - Pattern syntax: Uses ripgrep (not grep) - literal braces need escaping \
         (use `interface\\{{\\}}` to find `interface{{}}` in Go code)\n\
         - Multiline matching: By default patterns match within single lines only. For cross-line \
         patterns like `struct \\{{[\\\\s\\\\S]*?field`, use `multiline: true`",
        grep = GREP_TOOL_NAME,
        bash = BASH_TOOL_NAME,
        agent = AGENT_TOOL_NAME,
    )
}
