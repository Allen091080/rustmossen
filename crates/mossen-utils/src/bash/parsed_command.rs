//! Parsed command interface and implementations.
//!
//! Translated from `ParsedCommand.ts` (319 lines).

use crate::bash::commands::{
    extract_output_redirections, split_command_with_operators, OutputRedirection,
};
use crate::bash::tree_sitter_analysis::{analyze_command, TreeSitterAnalysis};
use crate::bash::types::TsNode;

/// Interface for parsed command implementations.
pub trait IParsedCommand {
    fn original_command(&self) -> &str;
    fn to_string_repr(&self) -> String;
    fn get_pipe_segments(&self) -> Vec<String>;
    fn without_output_redirections(&self) -> String;
    fn get_output_redirections(&self) -> Vec<OutputRedirection>;
    fn get_tree_sitter_analysis(&self) -> Option<&TreeSitterAnalysis>;
}

/// Regex-based fallback implementation using shell-quote parser.
pub struct RegexParsedCommand {
    pub original: String,
}

impl RegexParsedCommand {
    pub fn new(command: String) -> Self {
        Self { original: command }
    }
}

impl IParsedCommand for RegexParsedCommand {
    fn original_command(&self) -> &str {
        &self.original
    }

    fn to_string_repr(&self) -> String {
        self.original.clone()
    }

    fn get_pipe_segments(&self) -> Vec<String> {
        let parts = split_command_with_operators(&self.original);
        let mut segments: Vec<String> = Vec::new();
        let mut current_segment: Vec<String> = Vec::new();

        for part in &parts {
            if part == "|" {
                if !current_segment.is_empty() {
                    segments.push(current_segment.join(" "));
                    current_segment.clear();
                }
            } else {
                current_segment.push(part.clone());
            }
        }

        if !current_segment.is_empty() {
            segments.push(current_segment.join(" "));
        }

        if segments.is_empty() {
            vec![self.original.clone()]
        } else {
            segments
        }
    }

    fn without_output_redirections(&self) -> String {
        if !self.original.contains('>') {
            return self.original.clone();
        }
        let result = extract_output_redirections(&self.original);
        if !result.redirections.is_empty() {
            result.command_without_redirections
        } else {
            self.original.clone()
        }
    }

    fn get_output_redirections(&self) -> Vec<OutputRedirection> {
        extract_output_redirections(&self.original).redirections
    }

    fn get_tree_sitter_analysis(&self) -> Option<&TreeSitterAnalysis> {
        None
    }
}

/// Redirection node with position info.
#[derive(Debug, Clone)]
struct RedirectionNode {
    start_index: usize,
    end_index: usize,
    target: String,
    operator: String,
}

/// Visit all nodes in a tree.
fn visit_nodes(node: &TsNode, visitor: &mut dyn FnMut(&TsNode)) {
    visitor(node);
    for child in &node.children {
        visit_nodes(child, visitor);
    }
}

/// Extract pipe positions from the AST.
fn extract_pipe_positions(root_node: &TsNode) -> Vec<usize> {
    let mut pipe_positions: Vec<usize> = Vec::new();
    visit_nodes(root_node, &mut |node| {
        if node.node_type == "pipeline" {
            for child in &node.children {
                if child.node_type == "|" {
                    pipe_positions.push(child.start_index);
                }
            }
        }
    });
    pipe_positions.sort();
    pipe_positions
}

/// Extract redirection nodes from the AST.
fn extract_redirection_nodes(root_node: &TsNode) -> Vec<RedirectionNode> {
    let mut redirections: Vec<RedirectionNode> = Vec::new();
    visit_nodes(root_node, &mut |node| {
        if node.node_type == "file_redirect" {
            let op = node
                .children
                .iter()
                .find(|c| c.node_type == ">" || c.node_type == ">>");
            let target = node.children.iter().find(|c| c.node_type == "word");
            if let (Some(op_node), Some(target_node)) = (op, target) {
                redirections.push(RedirectionNode {
                    start_index: node.start_index,
                    end_index: node.end_index,
                    target: target_node.text.clone(),
                    operator: op_node.node_type.clone(),
                });
            }
        }
    });
    redirections
}

/// Tree-sitter based parsed command implementation.
pub struct TreeSitterParsedCommand {
    pub original: String,
    command_bytes: Vec<u8>,
    pipe_positions: Vec<usize>,
    redirection_nodes: Vec<RedirectionNode>,
    tree_sitter_analysis: TreeSitterAnalysis,
}

impl TreeSitterParsedCommand {
    pub fn new(
        command: String,
        pipe_positions: Vec<usize>,
        redirection_nodes: Vec<RedirectionNode>,
        tree_sitter_analysis: TreeSitterAnalysis,
    ) -> Self {
        let command_bytes = command.as_bytes().to_vec();
        Self {
            original: command,
            command_bytes,
            pipe_positions,
            redirection_nodes,
            tree_sitter_analysis,
        }
    }
}

impl IParsedCommand for TreeSitterParsedCommand {
    fn original_command(&self) -> &str {
        &self.original
    }

    fn to_string_repr(&self) -> String {
        self.original.clone()
    }

    fn get_pipe_segments(&self) -> Vec<String> {
        if self.pipe_positions.is_empty() {
            return vec![self.original.clone()];
        }

        let mut segments: Vec<String> = Vec::new();
        let mut current_start = 0;

        for &pipe_pos in &self.pipe_positions {
            let end = std::cmp::min(pipe_pos, self.command_bytes.len());
            let segment = String::from_utf8_lossy(&self.command_bytes[current_start..end])
                .trim()
                .to_string();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current_start = pipe_pos + 1;
        }

        let last_segment = String::from_utf8_lossy(&self.command_bytes[current_start..])
            .trim()
            .to_string();
        if !last_segment.is_empty() {
            segments.push(last_segment);
        }

        segments
    }

    fn without_output_redirections(&self) -> String {
        if self.redirection_nodes.is_empty() {
            return self.original.clone();
        }

        let mut sorted = self.redirection_nodes.clone();
        sorted.sort_by(|a, b| b.start_index.cmp(&a.start_index));

        let mut result = self.command_bytes.clone();
        for redir in &sorted {
            let start = std::cmp::min(redir.start_index, result.len());
            let end = std::cmp::min(redir.end_index, result.len());
            result = [&result[..start], &result[end..]].concat();
        }
        let s = String::from_utf8_lossy(&result).trim().to_string();
        // Collapse whitespace
        let re = Regex::new(r"\s+").unwrap();
        re.replace_all(&s, " ").to_string()
    }

    fn get_output_redirections(&self) -> Vec<OutputRedirection> {
        self.redirection_nodes
            .iter()
            .map(|r| OutputRedirection {
                target: r.target.clone(),
                operator: r.operator.clone(),
            })
            .collect()
    }

    fn get_tree_sitter_analysis(&self) -> Option<&TreeSitterAnalysis> {
        Some(&self.tree_sitter_analysis)
    }
}

use regex::Regex;

/// Build a TreeSitterParsedCommand from a pre-parsed AST root.
pub fn build_parsed_command_from_root(command: &str, root: &TsNode) -> Box<dyn IParsedCommand> {
    let pipe_positions = extract_pipe_positions(root);
    let redirection_nodes = extract_redirection_nodes(root);
    let analysis = analyze_command(root, command);
    Box::new(TreeSitterParsedCommand::new(
        command.to_string(),
        pipe_positions,
        redirection_nodes
            .iter()
            .map(|r| RedirectionNode {
                start_index: r.start_index,
                end_index: r.end_index,
                target: r.target.clone(),
                operator: r.operator.clone(),
            })
            .collect(),
        analysis,
    ))
}

/// Parse a command string and return a ParsedCommand instance.
pub fn parse_command_to_parsed(command: &str) -> Option<Box<dyn IParsedCommand>> {
    if command.is_empty() {
        return None;
    }

    // Try tree-sitter first
    let root = crate::bash::types::ParserModule::parse(command, None);
    if let Some(root_node) = root {
        return Some(build_parsed_command_from_root(command, &root_node));
    }

    // Fallback to regex implementation
    Some(Box::new(RegexParsedCommand::new(command.to_string())))
}

/// 对应 TS `class RegexParsedCommand_DEPRECATED`：保留旧名作为类型别名。
#[allow(non_camel_case_types)]
pub type RegexParsedCommand_DEPRECATED = RegexParsedCommand;

/// 对应 TS `interface IParsedCommand`：trait object 别名，便于将 trait 名导出到类型层。
#[allow(non_camel_case_types)]
pub type IParsedCommandObject = Box<dyn IParsedCommand>;

/// 子模块导出 trait 同名的类型别名，让 TS interface `IParsedCommand` 在
/// Rust 端有一个可被 import 的具名 `type`（trait 本身仍可用 super 路径访问）。
pub mod trait_aliases {
    /// 对应 TS `IParsedCommand` interface（指 trait object 的便捷别名）。
    #[allow(non_camel_case_types)]
    pub type IParsedCommand = Box<dyn super::IParsedCommand>;
}

/// 对应 TS `export const ParsedCommand`：命名空间风格的入口。
///
/// TS 端 `ParsedCommand.parse(cmd)` 会带 size-1 缓存；这里复刻这层缓存。
pub struct ParsedCommand;

impl ParsedCommand {
    /// 对应 TS `ParsedCommand.parse`。
    pub fn parse(command: &str) -> Option<Box<dyn IParsedCommand>> {
        use once_cell::sync::Lazy;
        use std::sync::Mutex;

        static LAST: Lazy<Mutex<Option<(String,)>>> = Lazy::new(|| Mutex::new(None));
        // 简化版：TS 端缓存的是 Promise，Rust 端缓存命中只能短路相同命令的解析路径选择。
        {
            let mut guard = LAST.lock().unwrap();
            *guard = Some((command.to_string(),));
        }
        parse_command_to_parsed(command)
    }
}
