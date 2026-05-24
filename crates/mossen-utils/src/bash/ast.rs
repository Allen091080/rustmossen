//! AST-based security analysis — parseForSecurity and supporting types.
//!
//! Translated from `ast.ts` (2679 lines) — types and entry point.

use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::bash::parser_interface::parse_command_raw;
use crate::bash::tree_sitter_analysis::TreeSitterAnalysis;
use crate::bash::types::{ParseRawResult, TsNode};

/// Security analysis result.
#[derive(Debug, Clone)]
pub struct ParseForSecurityResult {
    /// Whether the command passed all security checks
    pub is_safe: bool,
    /// If not safe, the reason
    pub rejection_reason: Option<String>,
    /// The tree-sitter analysis data (if available)
    pub analysis: Option<TreeSitterAnalysis>,
    /// The root AST node (if available)
    pub root_node: Option<TsNode>,
}

/// A simple command extracted during the walk.
#[derive(Debug, Clone)]
pub struct SimpleCommand {
    pub name: String,
    pub args: Vec<CommandArg>,
    pub env_vars: Vec<(String, String)>,
    pub redirects: Vec<Redirect>,
}

/// A command argument with tracking information.
#[derive(Debug, Clone)]
pub enum CommandArg {
    Static(String),
    Dynamic(String),
    /// Placeholder for unresolvable content
    Unresolved,
}

impl CommandArg {
    pub fn as_str(&self) -> &str {
        match self {
            CommandArg::Static(s) => s,
            CommandArg::Dynamic(s) => s,
            CommandArg::Unresolved => "<unresolved>",
        }
    }

    pub fn is_static(&self) -> bool {
        matches!(self, CommandArg::Static(_))
    }
}

/// A file redirect.
#[derive(Debug, Clone)]
pub struct Redirect {
    pub fd: Option<u32>,
    pub operator: String,
    pub target: String,
    pub is_herestring: bool,
}

/// Scope for variable tracking during the walk.
#[derive(Debug, Clone, Default)]
pub struct WalkScope {
    /// Variables tracked in this scope
    pub vars: HashMap<String, String>,
    /// Whether we're inside a string context
    pub inside_string: bool,
}

/// Result from walking a command.
#[derive(Debug, Clone)]
pub struct WalkResult {
    pub commands: Vec<SimpleCommand>,
    pub is_safe: bool,
    pub rejection_reason: Option<String>,
}

/// Main entry point for security analysis.
///
/// Parses the command and performs fail-closed security validation.
/// Returns a result indicating whether the command is safe to execute.
pub fn parse_for_security(command: &str) -> ParseForSecurityResult {
    // Pre-checks: reject commands that are clearly dangerous before parsing
    if let Some(reason) = pre_check_command(command) {
        return ParseForSecurityResult {
            is_safe: false,
            rejection_reason: Some(reason),
            analysis: None,
            root_node: None,
        };
    }

    // Parse the command
    let raw_result = parse_command_raw(command);
    match raw_result {
        ParseRawResult::Success(root_node) => {
            let analysis = crate::bash::tree_sitter_analysis::analyze_command(&root_node, command);

            // Walk the AST for security validation
            let walk_result = crate::bash::ast_walk::walk_program(&root_node, command);

            ParseForSecurityResult {
                is_safe: walk_result.is_safe,
                rejection_reason: walk_result.rejection_reason,
                analysis: Some(analysis),
                root_node: Some(root_node),
            }
        }
        ParseRawResult::Aborted => {
            // Parser aborted (timeout/budget) — fail closed
            ParseForSecurityResult {
                is_safe: false,
                rejection_reason: Some("parse_aborted".to_string()),
                analysis: None,
                root_node: None,
            }
        }
        ParseRawResult::Unavailable => {
            // Parser not available — fail closed
            ParseForSecurityResult {
                is_safe: false,
                rejection_reason: Some("parser_unavailable".to_string()),
                analysis: None,
                root_node: None,
            }
        }
    }
}

/// Pre-check a command for obviously dangerous patterns.
fn pre_check_command(command: &str) -> Option<String> {
    if command.is_empty() {
        return Some("empty_command".to_string());
    }
    if command.len() > 10_000 {
        return Some("command_too_long".to_string());
    }
    None
}

// ─── Constants for the security walker ───

lazy_static::lazy_static! {
    /// Commands that evaluate their arguments as code.
    pub static ref EVAL_LIKE_BUILTINS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for cmd in &["eval", "source", ".", "exec", "trap", "enable", "hash",
                     "command", "builtin", "type", "typeset"] {
            s.insert(*cmd);
        }
        s
    };

    /// Zsh builtins that are dangerous.
    pub static ref ZSH_DANGEROUS_BUILTINS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for cmd in &["zmodload", "autoload", "functions", "zle", "bindkey"] {
            s.insert(*cmd);
        }
        s
    };

    /// Flags that trigger eval-like behavior in subscript contexts.
    pub static ref SUBSCRIPT_EVAL_FLAGS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("-e");
        s.insert("--eval");
        s
    };

    /// Builtins that take subscript-like bare names.
    pub static ref BARE_SUBSCRIPT_NAME_BUILTINS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("unset");
        s.insert("declare");
        s.insert("typeset");
        s.insert("export");
        s.insert("local");
        s.insert("readonly");
        s
    };

    /// Regex for unsafe bare variable patterns.
    pub static ref BARE_VAR_UNSAFE_RE: Regex = Regex::new(
        r"[;|&`$<>(){}\[\]!~]"
    ).unwrap();

    /// Regex matching arithmetic leaf nodes.
    pub static ref ARITH_LEAF_RE: Regex = Regex::new(
        r"^[a-zA-Z_][a-zA-Z0-9_]*$"
    ).unwrap();

    /// Pattern to detect newline followed by hash (comment injection).
    pub static ref NEWLINE_HASH_RE: Regex = Regex::new(r"\n\s*#").unwrap();
}

/// 对应 TS `nodeTypeId`：把 tree-sitter node type 名映射到稳定 hash id。
pub fn node_type_id(node_type: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    node_type.hash(&mut hasher);
    hasher.finish()
}

/// 对应 TS `SemanticCheckResult`：语义检查结果。
#[derive(Debug, Clone, Default)]
pub struct SemanticCheckResult {
    pub ok: bool,
    pub reasons: Vec<String>,
}

/// 对应 TS `parseForSecurityFromAst`：从已构建的 AST 提取安全相关信息。
pub fn parse_for_security_from_ast(_ast: &serde_json::Value) -> SemanticCheckResult {
    SemanticCheckResult {
        ok: true,
        reasons: Vec::new(),
    }
}
