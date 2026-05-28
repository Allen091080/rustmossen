//! Code indexing tool detection utilities.
//!
//! Tracks usage of common code indexing solutions like Sourcegraph, Cody, etc.
//! both via CLI commands and MCP server integrations.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Known code indexing tool identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeIndexingTool {
    // Code search engines
    Sourcegraph,
    Hound,
    Seagoat,
    Bloop,
    Gitloop,
    // AI coding assistants with indexing
    Cody,
    Aider,
    Continue,
    GithubCopilot,
    Cursor,
    Tabby,
    Codeium,
    Tabnine,
    Augment,
    Windsurf,
    Aide,
    Pieces,
    Qodo,
    AmazonQ,
    Gemini,
    // MCP code indexing servers
    MossenContext,
    CodeIndexMcp,
    LocalCodeSearch,
    AutodevCodebase,
    // Context providers
    Openctx,
}

impl CodeIndexingTool {
    /// Return the string identifier for analytics events.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sourcegraph => "sourcegraph",
            Self::Hound => "hound",
            Self::Seagoat => "seagoat",
            Self::Bloop => "bloop",
            Self::Gitloop => "gitloop",
            Self::Cody => "cody",
            Self::Aider => "aider",
            Self::Continue => "continue",
            Self::GithubCopilot => "github-copilot",
            Self::Cursor => "cursor",
            Self::Tabby => "tabby",
            Self::Codeium => "codeium",
            Self::Tabnine => "tabnine",
            Self::Augment => "augment",
            Self::Windsurf => "windsurf",
            Self::Aide => "aide",
            Self::Pieces => "pieces",
            Self::Qodo => "qodo",
            Self::AmazonQ => "amazon-q",
            Self::Gemini => "gemini",
            Self::MossenContext => "mossen-context",
            Self::CodeIndexMcp => "code-index-mcp",
            Self::LocalCodeSearch => "local-code-search",
            Self::AutodevCodebase => "autodev-codebase",
            Self::Openctx => "openctx",
        }
    }
}

/// CLI command prefix to code indexing tool mapping.
static CLI_COMMAND_MAPPING: Lazy<HashMap<&'static str, CodeIndexingTool>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("src", CodeIndexingTool::Sourcegraph);
    m.insert("cody", CodeIndexingTool::Cody);
    m.insert("aider", CodeIndexingTool::Aider);
    m.insert("tabby", CodeIndexingTool::Tabby);
    m.insert("tabnine", CodeIndexingTool::Tabnine);
    m.insert("augment", CodeIndexingTool::Augment);
    m.insert("pieces", CodeIndexingTool::Pieces);
    m.insert("qodo", CodeIndexingTool::Qodo);
    m.insert("aide", CodeIndexingTool::Aide);
    m.insert("hound", CodeIndexingTool::Hound);
    m.insert("seagoat", CodeIndexingTool::Seagoat);
    m.insert("bloop", CodeIndexingTool::Bloop);
    m.insert("gitloop", CodeIndexingTool::Gitloop);
    m.insert("q", CodeIndexingTool::AmazonQ);
    m.insert("gemini", CodeIndexingTool::Gemini);
    m
});

/// MCP server name pattern to code indexing tool mapping.
struct McpPattern {
    pattern: Regex,
    tool: CodeIndexingTool,
}

static MCP_SERVER_PATTERNS: Lazy<Vec<McpPattern>> = Lazy::new(|| {
    vec![
        McpPattern {
            pattern: Regex::new(r"(?i)^sourcegraph$").unwrap(),
            tool: CodeIndexingTool::Sourcegraph,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^cody$").unwrap(),
            tool: CodeIndexingTool::Cody,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^openctx$").unwrap(),
            tool: CodeIndexingTool::Openctx,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^aider$").unwrap(),
            tool: CodeIndexingTool::Aider,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^continue$").unwrap(),
            tool: CodeIndexingTool::Continue,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^github[-_]?copilot$").unwrap(),
            tool: CodeIndexingTool::GithubCopilot,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^copilot$").unwrap(),
            tool: CodeIndexingTool::GithubCopilot,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^cursor$").unwrap(),
            tool: CodeIndexingTool::Cursor,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^tabby$").unwrap(),
            tool: CodeIndexingTool::Tabby,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^codeium$").unwrap(),
            tool: CodeIndexingTool::Codeium,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^tabnine$").unwrap(),
            tool: CodeIndexingTool::Tabnine,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^augment[-_]?code$").unwrap(),
            tool: CodeIndexingTool::Augment,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^augment$").unwrap(),
            tool: CodeIndexingTool::Augment,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^windsurf$").unwrap(),
            tool: CodeIndexingTool::Windsurf,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^aide$").unwrap(),
            tool: CodeIndexingTool::Aide,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^codestory$").unwrap(),
            tool: CodeIndexingTool::Aide,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^pieces$").unwrap(),
            tool: CodeIndexingTool::Pieces,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^qodo$").unwrap(),
            tool: CodeIndexingTool::Qodo,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^amazon[-_]?q$").unwrap(),
            tool: CodeIndexingTool::AmazonQ,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^gemini[-_]?code[-_]?assist$").unwrap(),
            tool: CodeIndexingTool::Gemini,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^gemini$").unwrap(),
            tool: CodeIndexingTool::Gemini,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^hound$").unwrap(),
            tool: CodeIndexingTool::Hound,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^seagoat$").unwrap(),
            tool: CodeIndexingTool::Seagoat,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^bloop$").unwrap(),
            tool: CodeIndexingTool::Bloop,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^gitloop$").unwrap(),
            tool: CodeIndexingTool::Gitloop,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^mossen[-_]?context$").unwrap(),
            tool: CodeIndexingTool::MossenContext,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^code[-_]?index[-_]?mcp$").unwrap(),
            tool: CodeIndexingTool::CodeIndexMcp,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^code[-_]?index$").unwrap(),
            tool: CodeIndexingTool::CodeIndexMcp,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^local[-_]?code[-_]?search$").unwrap(),
            tool: CodeIndexingTool::LocalCodeSearch,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^codebase$").unwrap(),
            tool: CodeIndexingTool::AutodevCodebase,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^autodev[-_]?codebase$").unwrap(),
            tool: CodeIndexingTool::AutodevCodebase,
        },
        McpPattern {
            pattern: Regex::new(r"(?i)^code[-_]?context$").unwrap(),
            tool: CodeIndexingTool::MossenContext,
        },
    ]
});

/// Detects if a bash command is using a code indexing CLI tool.
///
/// # Examples
/// ```
/// use mossen_utils::code_indexing::detect_code_indexing_from_command;
/// assert_eq!(detect_code_indexing_from_command("src search \"pattern\"").map(|t| t.as_str()), Some("sourcegraph"));
/// assert!(detect_code_indexing_from_command("ls -la").is_none());
/// ```
pub fn detect_code_indexing_from_command(command: &str) -> Option<CodeIndexingTool> {
    let trimmed = command.trim();
    let mut words = trimmed.split_whitespace();
    let first_word = words.next()?.to_lowercase();

    // Check for npx/bunx prefixed commands
    if first_word == "npx" || first_word == "bunx" {
        if let Some(second_word) = words.next() {
            let second_lower = second_word.to_lowercase();
            if let Some(&tool) = CLI_COMMAND_MAPPING.get(second_lower.as_str()) {
                return Some(tool);
            }
        }
    }

    CLI_COMMAND_MAPPING.get(first_word.as_str()).copied()
}

/// Detects if an MCP tool is from a code indexing server.
///
/// MCP tool names follow the format: mcp__serverName__toolName
pub fn detect_code_indexing_from_mcp_tool(tool_name: &str) -> Option<CodeIndexingTool> {
    if !tool_name.starts_with("mcp__") {
        return None;
    }

    let parts: Vec<&str> = tool_name.split("__").collect();
    if parts.len() < 3 {
        return None;
    }

    let server_name = parts.get(1)?;
    if server_name.is_empty() {
        return None;
    }

    for pattern in MCP_SERVER_PATTERNS.iter() {
        if pattern.pattern.is_match(server_name) {
            return Some(pattern.tool);
        }
    }

    None
}

/// Detects if an MCP server name corresponds to a code indexing tool.
pub fn detect_code_indexing_from_mcp_server_name(server_name: &str) -> Option<CodeIndexingTool> {
    for pattern in MCP_SERVER_PATTERNS.iter() {
        if pattern.pattern.is_match(server_name) {
            return Some(pattern.tool);
        }
    }
    None
}
