//! Bash command parsing and security analysis module.
//!
//! This module provides a complete pure-Rust bash parser producing tree-sitter-bash-compatible
//! ASTs, along with security analysis utilities for command validation.
//!
//! Translated from the TypeScript `utils/bash/` directory.

pub mod types;
pub mod lexer;
pub mod parser_core;
pub mod parser_stmts;
pub mod parser_exprs;
pub mod ast;
pub mod ast_walk;
pub mod commands;
pub mod heredoc;
pub mod shell_quote;
pub mod shell_quoting;
pub mod tree_sitter_analysis;
pub mod parsed_command;
pub mod pipe_command;
pub mod prefix;
pub mod parser_interface;
pub mod shell_completion;
pub mod shell_prefix;
pub mod shell_snapshot;
pub mod registry;
pub mod specs;

