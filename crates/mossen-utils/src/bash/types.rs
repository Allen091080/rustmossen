//! Core type definitions for the bash parser.
//!
//! Translated from `bashParser.ts` (TsNode, Token types) and `parser.ts` (ParsedCommandData).

use std::collections::HashSet;

/// A tree-sitter-compatible AST node.
/// `start_index`/`end_index` are UTF-8 byte offsets (not char indices).
#[derive(Debug, Clone, PartialEq)]
pub struct TsNode {
    pub node_type: String,
    pub text: String,
    pub start_index: usize,
    pub end_index: usize,
    pub children: Vec<TsNode>,
}

impl TsNode {
    pub fn new(node_type: impl Into<String>, text: impl Into<String>, start_index: usize, end_index: usize, children: Vec<TsNode>) -> Self {
        Self {
            node_type: node_type.into(),
            text: text.into(),
            start_index,
            end_index,
            children,
        }
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// Token types produced by the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    Word,
    Number,
    Op,
    Newline,
    Comment,
    DQuote,
    SQuote,
    AnsiC,
    Dollar,
    DollarParen,
    DollarBrace,
    DollarDParen,
    Backtick,
    LtParen,
    GtParen,
    Eof,
}

/// A lexer token with position information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    /// UTF-8 byte offset of first char
    pub start: usize,
    /// UTF-8 byte offset one past last char
    pub end: usize,
}

impl Token {
    pub fn new(token_type: TokenType, value: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            token_type,
            value: value.into(),
            start,
            end,
        }
    }
}

/// Parsed command data from tree-sitter analysis.
#[derive(Debug, Clone)]
pub struct ParsedCommandData {
    pub root_node: TsNode,
    pub env_vars: Vec<String>,
    pub command_node: Option<TsNode>,
    pub original_command: String,
}

/// Lexer context for token scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexContext {
    Cmd,
    Arg,
}

/// Pending heredoc information during lexing.
#[derive(Debug, Clone)]
pub struct HeredocPending {
    pub delim: String,
    pub strip_tabs: bool,
    pub quoted: bool,
    /// Filled after body scan
    pub body_start: usize,
    pub body_end: usize,
    pub end_start: usize,
    pub end_end: usize,
}

/// Parser state machine.
#[derive(Debug)]
pub struct ParseState {
    pub src: String,
    pub src_bytes: usize,
    /// True when byte offsets == char indices (no multi-byte UTF-8)
    pub is_ascii: bool,
    pub node_count: usize,
    pub deadline: std::time::Instant,
    pub aborted: bool,
    /// Depth of backtick nesting
    pub in_backtick: usize,
    /// When set, parseSimpleCommand stops at this token (for `[` backtrack)
    pub stop_token: Option<String>,
}

/// Saved lexer state for backtracking. Packed as two usizes.
#[derive(Debug, Clone, Copy)]
pub struct LexSave {
    pub i: usize,
    pub b: usize,
}

/// Parser module interface.
pub struct ParserModule;

impl ParserModule {
    pub fn parse(source: &str, timeout_ms: Option<u64>) -> Option<TsNode> {
        crate::bash::parser_core::parse_source(source, timeout_ms)
    }
}

// ─── Constants ───

pub const PARSE_TIMEOUT_MS: u64 = 50;
pub const MAX_NODES: usize = 50_000;
pub const MAX_COMMAND_LENGTH: usize = 10_000;

lazy_static::lazy_static! {
    pub static ref SPECIAL_VARS: HashSet<char> = {
        let mut s = HashSet::new();
        for c in &['?', '$', '@', '*', '#', '-', '!', '_'] {
            s.insert(*c);
        }
        s
    };

    pub static ref DECL_KEYWORDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("export");
        s.insert("declare");
        s.insert("typeset");
        s.insert("readonly");
        s.insert("local");
        s
    };

    pub static ref SHELL_KEYWORDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for kw in &["if", "then", "elif", "else", "fi", "while", "until",
                    "for", "in", "do", "done", "case", "esac", "function", "select"] {
            s.insert(*kw);
        }
        s
    };

    pub static ref DECLARATION_COMMANDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for kw in &["export", "declare", "typeset", "readonly", "local", "unset", "unsetenv"] {
            s.insert(*kw);
        }
        s
    };

    pub static ref ARGUMENT_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        for t in &["word", "string", "raw_string", "number"] {
            s.insert(*t);
        }
        s
    };

    pub static ref SUBSTITUTION_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("command_substitution");
        s.insert("process_substitution");
        s
    };

    pub static ref COMMAND_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("command");
        s.insert("declaration_command");
        s
    };
}

/// Sentinel for "parser was loaded and attempted, but aborted"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseAborted;

/// Result from parseCommandRaw
pub enum ParseRawResult {
    Success(TsNode),
    Unavailable,
    Aborted,
}
