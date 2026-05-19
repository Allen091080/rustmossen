//! Parser interface — parseCommand, findCommandNode, extractCommandArguments.
//!
//! Translated from `parser.ts` (231 lines).

use std::collections::HashSet;

use crate::bash::types::{ParsedCommandData, ParseAborted, ParseRawResult, TsNode, MAX_COMMAND_LENGTH, PARSE_TIMEOUT_MS};

lazy_static::lazy_static! {
    static ref DECLARATION_COMMANDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for kw in &["export", "declare", "typeset", "readonly", "local", "unset", "unsetenv"] {
            s.insert(*kw);
        }
        s
    };
    static ref ARGUMENT_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for t in &["word", "string", "raw_string", "number"] {
            s.insert(*t);
        }
        s
    };
    static ref SUBSTITUTION_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("command_substitution");
        s.insert("process_substitution");
        s
    };
    static ref COMMAND_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("command");
        s.insert("declaration_command");
        s
    };
}

/// Parse a command string into a tree-sitter AST with command info.
pub fn parse_command(command: &str) -> Option<ParsedCommandData> {
    if command.is_empty() || command.len() > MAX_COMMAND_LENGTH {
        return None;
    }

    let root_node = crate::bash::types::ParserModule::parse(command, Some(PARSE_TIMEOUT_MS))?;
    let command_node = find_command_node(&root_node, None);
    let env_vars = extract_env_vars(command_node.as_ref());

    Some(ParsedCommandData {
        root_node,
        env_vars,
        command_node,
        original_command: command.to_string(),
    })
}

/// Raw parse — skips findCommandNode/extractEnvVars.
pub fn parse_command_raw(command: &str) -> ParseRawResult {
    if command.is_empty() || command.len() > MAX_COMMAND_LENGTH {
        return ParseRawResult::Unavailable;
    }

    match crate::bash::types::ParserModule::parse(command, Some(PARSE_TIMEOUT_MS)) {
        Some(node) => ParseRawResult::Success(node),
        None => ParseRawResult::Aborted,
    }
}

/// Find the first command node in the AST.
fn find_command_node(node: &TsNode, parent: Option<&TsNode>) -> Option<TsNode> {
    let node_type = node.node_type.as_str();

    if COMMAND_TYPES.contains(node_type) {
        return Some(node.clone());
    }

    // Variable assignment followed by command
    if node_type == "variable_assignment" {
        if let Some(parent_node) = parent {
            return parent_node
                .children
                .iter()
                .find(|c| COMMAND_TYPES.contains(c.node_type.as_str()) && c.start_index > node.start_index)
                .cloned();
        }
    }

    // Pipeline: recurse into first child
    if node_type == "pipeline" {
        for child in &node.children {
            let result = find_command_node(child, Some(node));
            if result.is_some() {
                return result;
            }
        }
        return None;
    }

    // Redirected statement: find the command inside
    if node_type == "redirected_statement" {
        return node.children
            .iter()
            .find(|c| COMMAND_TYPES.contains(c.node_type.as_str()))
            .cloned();
    }

    // Recursive search
    for child in &node.children {
        let result = find_command_node(child, Some(node));
        if result.is_some() {
            return result;
        }
    }

    None
}

/// Extract environment variables from a command node.
fn extract_env_vars(command_node: Option<&TsNode>) -> Vec<String> {
    let command_node = match command_node {
        Some(n) if n.node_type == "command" => n,
        _ => return Vec::new(),
    };

    let mut env_vars: Vec<String> = Vec::new();
    for child in &command_node.children {
        if child.node_type == "variable_assignment" {
            env_vars.push(child.text.clone());
        } else if child.node_type == "command_name" || child.node_type == "word" {
            break;
        }
    }
    env_vars
}

/// Extract command arguments from a command node.
pub fn extract_command_arguments(command_node: &TsNode) -> Vec<String> {
    // Declaration commands
    if command_node.node_type == "declaration_command" {
        if let Some(first_child) = command_node.children.first() {
            if DECLARATION_COMMANDS.contains(first_child.text.as_str()) {
                return vec![first_child.text.clone()];
            }
        }
        return Vec::new();
    }

    let mut args: Vec<String> = Vec::new();
    let mut found_command_name = false;

    for child in &command_node.children {
        if child.node_type == "variable_assignment" {
            continue;
        }

        // Command name
        if child.node_type == "command_name" || (!found_command_name && child.node_type == "word") {
            found_command_name = true;
            args.push(child.text.clone());
            continue;
        }

        // Arguments
        if ARGUMENT_TYPES.contains(child.node_type.as_str()) {
            args.push(strip_quotes(&child.text));
        } else if SUBSTITUTION_TYPES.contains(child.node_type.as_str()) {
            break;
        }
    }
    args
}

/// Strip surrounding quotes from a text value.
fn strip_quotes(text: &str) -> String {
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        if (bytes[0] == b'"' && bytes[text.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[text.len() - 1] == b'\'')
        {
            return text[1..text.len() - 1].to_string();
        }
    }
    text.to_string()
}

/// 对应 TS `ensureInitialized`：保证 bash parser 已初始化（幂等）。
pub async fn ensure_initialized() {}

/// 对应 TS `Node`：bash AST 节点别名（JSON-shaped）。
pub type Node = serde_json::Value;

/// 对应 TS `ensureParserInitialized`：等价于 [`ensure_initialized`]。
pub async fn ensure_parser_initialized() {
    ensure_initialized().await
}

/// 对应 TS `getParserModule`：返回 parser 模块标识符。
pub fn get_parser_module() -> &'static str {
    "tree-sitter-bash"
}

/// 对应 TS `SHELL_KEYWORDS`：bash 保留字集合。
pub const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until", "do",
    "done", "in", "function", "time", "[[", "]]", "!", "{", "}",
];
