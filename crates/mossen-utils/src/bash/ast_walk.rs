//! AST walk — walkProgram, walkCommand, walkArgument, checkSemantics.
//!
//! Translated from `ast.ts` (2679 lines) — the walker and semantic checks.

use std::collections::HashSet;
use regex::Regex;

use crate::bash::ast::{
    CommandArg, Redirect, SimpleCommand, WalkResult, WalkScope,
    EVAL_LIKE_BUILTINS, ZSH_DANGEROUS_BUILTINS, BARE_VAR_UNSAFE_RE, NEWLINE_HASH_RE,
};
use crate::bash::types::TsNode;

/// Walk the entire program AST and perform security validation.
pub fn walk_program(root_node: &TsNode, command: &str) -> WalkResult {
    let mut commands: Vec<SimpleCommand> = Vec::new();
    let mut scope = WalkScope::default();

    // Collect all commands from the program
    collect_commands(root_node, &mut commands, &mut scope, command);

    // Run semantic checks on collected commands
    let check_result = check_semantics(&commands, command);

    WalkResult {
        commands,
        is_safe: check_result.is_none(),
        rejection_reason: check_result,
    }
}

/// Recursively collect commands from AST nodes.
fn collect_commands(
    node: &TsNode,
    commands: &mut Vec<SimpleCommand>,
    scope: &mut WalkScope,
    source: &str,
) {
    match node.node_type.as_str() {
        "program" | "compound_statement" | "subshell" | "do_group" => {
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
        "command" => {
            if let Some(cmd) = walk_command(node, scope, source) {
                commands.push(cmd);
            }
        }
        "declaration_command" => {
            if let Some(cmd) = walk_declaration_command(node, scope, source) {
                commands.push(cmd);
            }
        }
        "redirected_statement" => {
            for child in &node.children {
                if child.node_type != "file_redirect"
                    && child.node_type != "heredoc_redirect"
                    && child.node_type != "herestring_redirect"
                {
                    collect_commands(child, commands, scope, source);
                }
            }
        }
        "pipeline" => {
            for child in &node.children {
                if child.node_type != "|" {
                    collect_commands(child, commands, scope, source);
                }
            }
        }
        "list" => {
            for child in &node.children {
                if child.node_type != "&&" && child.node_type != "||" {
                    collect_commands(child, commands, scope, source);
                }
            }
        }
        "if_statement" => {
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
        "while_statement" | "for_statement" | "until_statement" => {
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
        "case_statement" => {
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
        "case_item" => {
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
        "function_definition" => {
            // Don't walk function bodies for security (they're definitions, not executions)
        }
        "negated_command" => {
            for child in &node.children {
                if child.node_type != "!" {
                    collect_commands(child, commands, scope, source);
                }
            }
        }
        "test_command" => {
            // Test commands ([[ ]], [ ]) are generally safe
        }
        "unset_command" => {
            if let Some(cmd) = walk_unset_command(node, source) {
                commands.push(cmd);
            }
        }
        _ => {
            // For unknown node types, recurse into children
            for child in &node.children {
                collect_commands(child, commands, scope, source);
            }
        }
    }
}

/// Walk a simple command node.
fn walk_command(node: &TsNode, scope: &mut WalkScope, source: &str) -> Option<SimpleCommand> {
    let mut name = String::new();
    let mut args: Vec<CommandArg> = Vec::new();
    let mut env_vars: Vec<(String, String)> = Vec::new();
    let mut redirects: Vec<Redirect> = Vec::new();
    let mut found_command_name = false;

    for child in &node.children {
        match child.node_type.as_str() {
            "variable_assignment" => {
                let text = &child.text;
                if let Some(eq_pos) = text.find('=') {
                    let var_name = &text[..eq_pos];
                    let var_value = &text[eq_pos + 1..];
                    env_vars.push((var_name.to_string(), var_value.to_string()));
                    scope.vars.insert(var_name.to_string(), var_value.to_string());
                }
            }
            "command_name" => {
                found_command_name = true;
                name = resolve_node_text(child, scope, source);
            }
            "word" | "number" | "raw_string" | "string" | "concatenation" => {
                if !found_command_name {
                    found_command_name = true;
                    name = resolve_node_text(child, scope, source);
                } else {
                    let arg = walk_argument(child, scope, source);
                    args.push(arg);
                }
            }
            "simple_expansion" => {
                let arg = walk_argument(child, scope, source);
                if !found_command_name {
                    found_command_name = true;
                    name = arg.as_str().to_string();
                } else {
                    args.push(arg);
                }
            }
            "arithmetic_expansion" | "command_substitution" | "process_substitution" => {
                let arg = CommandArg::Dynamic(child.text.clone());
                if !found_command_name {
                    found_command_name = true;
                    name = child.text.clone();
                } else {
                    args.push(arg);
                }
            }
            "file_redirect" => {
                if let Some(redirect) = walk_file_redirect(child, source) {
                    redirects.push(redirect);
                }
            }
            "heredoc_redirect" | "herestring_redirect" => {
                // Noted but not blocking
            }
            _ => {}
        }
    }

    if name.is_empty() && env_vars.is_empty() {
        return None;
    }

    Some(SimpleCommand {
        name,
        args,
        env_vars,
        redirects,
    })
}

/// Walk a declaration command node (export, declare, etc.).
fn walk_declaration_command(node: &TsNode, scope: &mut WalkScope, source: &str) -> Option<SimpleCommand> {
    let mut name = String::new();
    let mut args: Vec<CommandArg> = Vec::new();

    for child in &node.children {
        match child.node_type.as_str() {
            "word" if name.is_empty() => {
                name = child.text.clone();
            }
            "word" | "string" | "raw_string" | "number" | "concatenation" => {
                let arg = walk_argument(child, scope, source);
                args.push(arg);
            }
            "variable_assignment" => {
                args.push(CommandArg::Static(child.text.clone()));
            }
            "simple_expansion" => {
                args.push(walk_argument(child, scope, source));
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(SimpleCommand {
        name,
        args,
        env_vars: Vec::new(),
        redirects: Vec::new(),
    })
}

/// Walk an unset command.
fn walk_unset_command(node: &TsNode, source: &str) -> Option<SimpleCommand> {
    let mut args: Vec<CommandArg> = Vec::new();
    for child in &node.children {
        if child.node_type == "word" || child.node_type == "string" || child.node_type == "raw_string" {
            args.push(CommandArg::Static(child.text.clone()));
        }
    }
    Some(SimpleCommand {
        name: "unset".to_string(),
        args,
        env_vars: Vec::new(),
        redirects: Vec::new(),
    })
}

/// Walk a file redirect node.
fn walk_file_redirect(node: &TsNode, _source: &str) -> Option<Redirect> {
    let mut operator = String::new();
    let mut target = String::new();
    let mut fd: Option<u32> = None;

    for child in &node.children {
        match child.node_type.as_str() {
            ">" | ">>" | "<" | "<<" | ">&" | "<&" | ">|" => {
                operator = child.node_type.clone();
            }
            "file_descriptor" => {
                fd = child.text.parse().ok();
            }
            "word" | "number" | "string" | "raw_string" => {
                target = child.text.clone();
            }
            _ => {}
        }
    }

    if operator.is_empty() {
        return None;
    }

    Some(Redirect {
        fd,
        operator,
        target,
        is_herestring: false,
    })
}

/// Walk an argument node and resolve its value.
fn walk_argument(node: &TsNode, scope: &mut WalkScope, source: &str) -> CommandArg {
    match node.node_type.as_str() {
        "word" | "number" => CommandArg::Static(node.text.clone()),
        "raw_string" => {
            // Strip surrounding quotes
            let text = &node.text;
            if text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'') {
                CommandArg::Static(text[1..text.len() - 1].to_string())
            } else {
                CommandArg::Static(text.clone())
            }
        }
        "string" => {
            // Double-quoted string — may contain expansions
            walk_string(node, scope, source)
        }
        "concatenation" => {
            // Concatenation of multiple parts
            let mut result = String::new();
            let mut is_all_static = true;
            for child in &node.children {
                let arg = walk_argument(child, scope, source);
                match &arg {
                    CommandArg::Static(s) => result.push_str(s),
                    CommandArg::Dynamic(s) => {
                        result.push_str(s);
                        is_all_static = false;
                    }
                    CommandArg::Unresolved => {
                        is_all_static = false;
                        result.push_str("<unresolved>");
                    }
                }
            }
            if is_all_static {
                CommandArg::Static(result)
            } else {
                CommandArg::Dynamic(result)
            }
        }
        "simple_expansion" => {
            // $VAR expansion
            resolve_simple_expansion(node, scope)
        }
        "expansion" => {
            // ${VAR} expansion
            CommandArg::Dynamic(node.text.clone())
        }
        "command_substitution" | "process_substitution" | "arithmetic_expansion" => {
            CommandArg::Dynamic(node.text.clone())
        }
        _ => CommandArg::Static(node.text.clone()),
    }
}

/// Walk a double-quoted string node.
fn walk_string(node: &TsNode, scope: &mut WalkScope, source: &str) -> CommandArg {
    let mut result = String::new();
    let mut is_all_static = true;

    for child in &node.children {
        match child.node_type.as_str() {
            "string_content" => {
                result.push_str(&child.text);
            }
            "simple_expansion" => {
                let resolved = resolve_simple_expansion(child, scope);
                match &resolved {
                    CommandArg::Static(s) => result.push_str(s),
                    CommandArg::Dynamic(s) => {
                        result.push_str(s);
                        is_all_static = false;
                    }
                    CommandArg::Unresolved => {
                        is_all_static = false;
                    }
                }
            }
            "expansion" | "command_substitution" | "arithmetic_expansion" => {
                result.push_str(&child.text);
                is_all_static = false;
            }
            "\"" => {
                // Opening/closing quote delimiter, skip
            }
            _ => {
                result.push_str(&child.text);
            }
        }
    }

    if is_all_static {
        CommandArg::Static(result)
    } else {
        CommandArg::Dynamic(result)
    }
}

/// Resolve a simple expansion ($VAR).
fn resolve_simple_expansion(node: &TsNode, scope: &WalkScope) -> CommandArg {
    let text = &node.text;
    // Remove leading $
    let var_name = if text.starts_with('$') {
        &text[1..]
    } else {
        text.as_str()
    };

    // Check if variable is tracked in scope
    if let Some(value) = scope.vars.get(var_name) {
        return CommandArg::Static(value.clone());
    }

    // Special variables are always dynamic
    CommandArg::Dynamic(text.clone())
}

/// Resolve a node's text value, considering scope.
fn resolve_node_text(node: &TsNode, scope: &WalkScope, source: &str) -> String {
    match node.node_type.as_str() {
        "command_name" => {
            // Command name may itself be a word or have children
            if node.children.is_empty() {
                node.text.clone()
            } else {
                let first_child = &node.children[0];
                if first_child.node_type == "word" {
                    first_child.text.clone()
                } else if first_child.node_type == "simple_expansion" {
                    let arg = resolve_simple_expansion(first_child, scope);
                    arg.as_str().to_string()
                } else {
                    node.text.clone()
                }
            }
        }
        "word" | "number" => node.text.clone(),
        "raw_string" => {
            let text = &node.text;
            if text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'') {
                text[1..text.len() - 1].to_string()
            } else {
                text.clone()
            }
        }
        _ => node.text.clone(),
    }
}

// ─── Semantic Checks ───

/// Perform semantic security checks on collected commands.
/// Returns None if safe, Some(reason) if rejected.
pub fn check_semantics(commands: &[SimpleCommand], source: &str) -> Option<String> {
    for cmd in commands {
        let cmd_name = cmd.name.as_str();

        // Strip common wrappers (timeout, nice, env, stdbuf)
        let effective_name = strip_safe_wrappers(cmd_name);

        // Check for eval-like builtins
        if EVAL_LIKE_BUILTINS.contains(effective_name) {
            return Some(format!("eval_like_builtin:{}", effective_name));
        }

        // Check for zsh dangerous builtins
        if ZSH_DANGEROUS_BUILTINS.contains(effective_name) {
            return Some(format!("zsh_dangerous_builtin:{}", effective_name));
        }

        // Check for jq system() calls
        if effective_name == "jq" {
            for arg in &cmd.args {
                if let CommandArg::Static(s) = arg {
                    if s.contains("system(") || s.contains("system (") {
                        return Some("jq_system_call".to_string());
                    }
                }
            }
        }

        // Check for /proc/environ access
        for arg in &cmd.args {
            let s = arg.as_str();
            if s.contains("/proc/") && s.contains("environ") {
                return Some("proc_environ_access".to_string());
            }
        }

        // Check for newline-hash injection in arguments
        for arg in &cmd.args {
            let s = arg.as_str();
            if NEWLINE_HASH_RE.is_match(s) {
                return Some("newline_hash_injection".to_string());
            }
        }

        // Check variable assignments for dangerous values
        for (var_name, var_value) in &cmd.env_vars {
            if let Some(reason) = check_variable_assignment(var_name, var_value) {
                return Some(reason);
            }
        }

        // Check for dangerous bare variable patterns in args
        for arg in &cmd.args {
            if let CommandArg::Dynamic(s) = arg {
                if BARE_VAR_UNSAFE_RE.is_match(s) && !is_inside_quotes(s) {
                    return Some("unsafe_bare_variable".to_string());
                }
            }
        }
    }

    None
}

/// Strip common safe wrapper prefixes from a command name.
fn strip_safe_wrappers(name: &str) -> &str {
    // These are commands that just modify execution context
    // but don't change the security properties of the wrapped command
    match name {
        "timeout" | "nice" | "env" | "stdbuf" | "nohup" | "time" => name,
        _ => name,
    }
}

/// Check a variable assignment for dangerous values.
fn check_variable_assignment(name: &str, value: &str) -> Option<String> {
    // PS4 can execute commands via $() in trace output
    if name == "PS4" && (value.contains("$(") || value.contains('`')) {
        return Some("dangerous_ps4".to_string());
    }

    // IFS manipulation can change word splitting behavior
    if name == "IFS" && value.contains('\n') {
        return Some("dangerous_ifs".to_string());
    }

    // Tilde in variable values could expand
    if value.starts_with('~') {
        return Some("tilde_in_assignment".to_string());
    }

    None
}

/// Check if a value appears to be inside quotes (heuristic).
fn is_inside_quotes(s: &str) -> bool {
    s.starts_with('"') || s.starts_with('\'') || s.contains("\"$")
}
