//! Tree-sitter AST analysis utilities for bash command security validation.
//!
//! Translated from `treeSitterAnalysis.ts` (507 lines).

use crate::bash::types::TsNode;

/// Quote context extracted from the AST.
#[derive(Debug, Clone)]
pub struct QuoteContext {
    /// Command text with single-quoted content removed (double-quoted content preserved)
    pub with_double_quotes: String,
    /// Command text with all quoted content removed
    pub fully_unquoted: String,
    /// Like fully_unquoted but preserves quote characters (', ")
    pub unquoted_keep_quote_chars: String,
}

/// Compound command structure.
#[derive(Debug, Clone)]
pub struct CompoundStructure {
    /// Whether the command has compound operators (&&, ||, ;) at the top level
    pub has_compound_operators: bool,
    /// Whether the command has pipelines
    pub has_pipeline: bool,
    /// Whether the command has subshells
    pub has_subshell: bool,
    /// Whether the command has command groups ({...})
    pub has_command_group: bool,
    /// Top-level compound operator types found
    pub operators: Vec<String>,
    /// Individual command segments split by compound operators
    pub segments: Vec<String>,
}

/// Dangerous patterns found in the AST.
#[derive(Debug, Clone)]
pub struct DangerousPatterns {
    /// Has $() or backtick command substitution
    pub has_command_substitution: bool,
    /// Has <() or >() process substitution
    pub has_process_substitution: bool,
    /// Has ${...} parameter expansion
    pub has_parameter_expansion: bool,
    /// Has heredoc
    pub has_heredoc: bool,
    /// Has comment
    pub has_comment: bool,
}

/// Complete tree-sitter analysis result.
#[derive(Debug, Clone)]
pub struct TreeSitterAnalysis {
    pub quote_context: QuoteContext,
    pub compound_structure: CompoundStructure,
    /// Whether actual operator nodes (;, &&, ||) exist
    pub has_actual_operator_nodes: bool,
    pub dangerous_patterns: DangerousPatterns,
}

// ─── Quote span types ───

struct QuoteSpans {
    raw: Vec<(usize, usize)>,
    ansi_c: Vec<(usize, usize)>,
    double: Vec<(usize, usize)>,
    heredoc: Vec<(usize, usize)>,
}

/// Single-pass collection of all quote-related spans.
fn collect_quote_spans(node: &TsNode, out: &mut QuoteSpans, in_double: bool) {
    match node.node_type.as_str() {
        "raw_string" => {
            out.raw.push((node.start_index, node.end_index));
            return; // literal body, no nested quotes possible
        }
        "ansi_c_string" => {
            out.ansi_c.push((node.start_index, node.end_index));
            return; // literal body
        }
        "string" => {
            if !in_double {
                out.double.push((node.start_index, node.end_index));
            }
            for child in &node.children {
                collect_quote_spans(child, out, true);
            }
            return;
        }
        "heredoc_redirect" => {
            let mut is_quoted = false;
            for child in &node.children {
                if child.node_type == "heredoc_start" {
                    if let Some(first) = child.text.chars().next() {
                        is_quoted = first == '\'' || first == '"' || first == '\\';
                    }
                    break;
                }
            }
            if is_quoted {
                out.heredoc.push((node.start_index, node.end_index));
                return; // literal body, no nested quote nodes
            }
            // Unquoted: recurse into heredoc_body
        }
        _ => {}
    }

    for child in &node.children {
        collect_quote_spans(child, out, in_double);
    }
}

/// Builds a set of all character positions covered by the given spans.
fn build_position_set(spans: &[(usize, usize)]) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    for &(start, end) in spans {
        for i in start..end {
            set.insert(i);
        }
    }
    set
}

/// Drops spans that are fully contained within another span.
fn drop_contained_spans(spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    spans
        .iter()
        .enumerate()
        .filter(|&(i, s)| {
            !spans.iter().enumerate().any(|(j, other)| {
                j != i && other.0 <= s.0 && other.1 >= s.1 && (other.0 < s.0 || other.1 > s.1)
            })
        })
        .map(|(_, s)| *s)
        .collect()
}

/// Removes spans from a string.
fn remove_spans(command: &str, spans: &[(usize, usize)]) -> String {
    if spans.is_empty() {
        return command.to_string();
    }
    let mut sorted = drop_contained_spans(spans);
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = command.to_string();
    for (start, end) in sorted {
        let s = std::cmp::min(start, result.len());
        let e = std::cmp::min(end, result.len());
        result = format!("{}{}", &result[..s], &result[e..]);
    }
    result
}

/// Replaces spans with just the quote delimiters (preserving ' and " characters).
fn replace_spans_keep_quotes(command: &str, spans: &[(usize, usize, &str, &str)]) -> String {
    if spans.is_empty() {
        return command.to_string();
    }
    // Drop contained and sort descending
    let filtered: Vec<_> = spans
        .iter()
        .enumerate()
        .filter(|&(i, s)| {
            !spans.iter().enumerate().any(|(j, other)| {
                j != i && other.0 <= s.0 && other.1 >= s.1 && (other.0 < s.0 || other.1 > s.1)
            })
        })
        .map(|(_, s)| *s)
        .collect();
    let mut sorted = filtered;
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = command.to_string();
    for (start, end, open, close) in sorted {
        let s = std::cmp::min(start, result.len());
        let e = std::cmp::min(end, result.len());
        result = format!("{}{}{}{}", &result[..s], open, close, &result[e..]);
    }
    result
}

/// Extract quote context from the tree-sitter AST.
pub fn extract_quote_context(root_node: &TsNode, command: &str) -> QuoteContext {
    let mut spans = QuoteSpans {
        raw: Vec::new(),
        ansi_c: Vec::new(),
        double: Vec::new(),
        heredoc: Vec::new(),
    };
    collect_quote_spans(root_node, &mut spans, false);

    let single_quote_set = build_position_set(
        &[
            spans.raw.clone(),
            spans.ansi_c.clone(),
            spans.heredoc.clone(),
        ]
        .concat(),
    );
    let mut double_quote_delim_set = std::collections::HashSet::new();
    for &(start, end) in &spans.double {
        double_quote_delim_set.insert(start);
        if end > 0 {
            double_quote_delim_set.insert(end - 1);
        }
    }

    let mut with_double_quotes = String::new();
    for (i, ch) in command.char_indices() {
        if single_quote_set.contains(&i) {
            continue;
        }
        if double_quote_delim_set.contains(&i) {
            continue;
        }
        with_double_quotes.push(ch);
    }

    let all_quote_spans: Vec<(usize, usize)> = [
        spans.raw.clone(),
        spans.ansi_c.clone(),
        spans.double.clone(),
        spans.heredoc.clone(),
    ]
    .concat();
    let fully_unquoted = remove_spans(command, &all_quote_spans);

    let mut spans_with_quote_chars: Vec<(usize, usize, &str, &str)> = Vec::new();
    for &(start, end) in &spans.raw {
        spans_with_quote_chars.push((start, end, "'", "'"));
    }
    for &(start, end) in &spans.ansi_c {
        spans_with_quote_chars.push((start, end, "$'", "'"));
    }
    for &(start, end) in &spans.double {
        spans_with_quote_chars.push((start, end, "\"", "\""));
    }
    for &(start, end) in &spans.heredoc {
        spans_with_quote_chars.push((start, end, "", ""));
    }
    let unquoted_keep_quote_chars = replace_spans_keep_quotes(command, &spans_with_quote_chars);

    QuoteContext {
        with_double_quotes,
        fully_unquoted,
        unquoted_keep_quote_chars,
    }
}

/// Extract compound command structure from the AST.
pub fn extract_compound_structure(root_node: &TsNode, command: &str) -> CompoundStructure {
    let mut operators: Vec<String> = Vec::new();
    let mut segments: Vec<String> = Vec::new();
    let mut has_subshell = false;
    let mut has_command_group = false;
    let mut has_pipeline = false;

    fn walk_top_level(
        node: &TsNode,
        operators: &mut Vec<String>,
        segments: &mut Vec<String>,
        has_subshell: &mut bool,
        has_command_group: &mut bool,
        has_pipeline: &mut bool,
    ) {
        for child in &node.children {
            match child.node_type.as_str() {
                "list" => {
                    for list_child in &child.children {
                        match list_child.node_type.as_str() {
                            "&&" | "||" => {
                                operators.push(list_child.node_type.clone());
                            }
                            "list" | "redirected_statement" => {
                                let wrapper = TsNode::new(
                                    &node.node_type,
                                    &node.text,
                                    node.start_index,
                                    node.end_index,
                                    vec![list_child.clone()],
                                );
                                walk_top_level(
                                    &wrapper,
                                    operators,
                                    segments,
                                    has_subshell,
                                    has_command_group,
                                    has_pipeline,
                                );
                            }
                            "pipeline" => {
                                *has_pipeline = true;
                                segments.push(list_child.text.clone());
                            }
                            "subshell" => {
                                *has_subshell = true;
                                segments.push(list_child.text.clone());
                            }
                            "compound_statement" => {
                                *has_command_group = true;
                                segments.push(list_child.text.clone());
                            }
                            _ => {
                                segments.push(list_child.text.clone());
                            }
                        }
                    }
                }
                ";" => {
                    operators.push(";".to_string());
                }
                "pipeline" => {
                    *has_pipeline = true;
                    segments.push(child.text.clone());
                }
                "subshell" => {
                    *has_subshell = true;
                    segments.push(child.text.clone());
                }
                "compound_statement" => {
                    *has_command_group = true;
                    segments.push(child.text.clone());
                }
                "command" | "declaration_command" | "variable_assignment" => {
                    segments.push(child.text.clone());
                }
                "redirected_statement" => {
                    let mut found_inner = false;
                    for inner in &child.children {
                        if inner.node_type == "file_redirect" {
                            continue;
                        }
                        found_inner = true;
                        let wrapper = TsNode::new(
                            &child.node_type,
                            &child.text,
                            child.start_index,
                            child.end_index,
                            vec![inner.clone()],
                        );
                        walk_top_level(
                            &wrapper,
                            operators,
                            segments,
                            has_subshell,
                            has_command_group,
                            has_pipeline,
                        );
                    }
                    if !found_inner {
                        segments.push(child.text.clone());
                    }
                }
                "negated_command" => {
                    segments.push(child.text.clone());
                    walk_top_level(
                        child,
                        operators,
                        segments,
                        has_subshell,
                        has_command_group,
                        has_pipeline,
                    );
                }
                "if_statement"
                | "while_statement"
                | "for_statement"
                | "case_statement"
                | "function_definition" => {
                    segments.push(child.text.clone());
                    walk_top_level(
                        child,
                        operators,
                        segments,
                        has_subshell,
                        has_command_group,
                        has_pipeline,
                    );
                }
                _ => {}
            }
        }
    }

    walk_top_level(
        root_node,
        &mut operators,
        &mut segments,
        &mut has_subshell,
        &mut has_command_group,
        &mut has_pipeline,
    );

    if segments.is_empty() {
        segments.push(command.to_string());
    }

    CompoundStructure {
        has_compound_operators: !operators.is_empty(),
        has_pipeline,
        has_subshell,
        has_command_group,
        operators,
        segments,
    }
}

/// Check whether the AST contains actual operator nodes (;, &&, ||).
pub fn has_actual_operator_nodes(root_node: &TsNode) -> bool {
    fn walk(node: &TsNode) -> bool {
        match node.node_type.as_str() {
            ";" | "&&" | "||" => return true,
            "list" => return true,
            _ => {}
        }
        for child in &node.children {
            if walk(child) {
                return true;
            }
        }
        false
    }
    walk(root_node)
}

/// Extract dangerous pattern information from the AST.
pub fn extract_dangerous_patterns(root_node: &TsNode) -> DangerousPatterns {
    let mut has_command_substitution = false;
    let mut has_process_substitution = false;
    let mut has_parameter_expansion = false;
    let mut has_heredoc = false;
    let mut has_comment = false;

    fn walk(
        node: &TsNode,
        cmd_sub: &mut bool,
        proc_sub: &mut bool,
        param_exp: &mut bool,
        heredoc: &mut bool,
        comment: &mut bool,
    ) {
        match node.node_type.as_str() {
            "command_substitution" => *cmd_sub = true,
            "process_substitution" => *proc_sub = true,
            "expansion" => *param_exp = true,
            "heredoc_redirect" => *heredoc = true,
            "comment" => *comment = true,
            _ => {}
        }
        for child in &node.children {
            walk(child, cmd_sub, proc_sub, param_exp, heredoc, comment);
        }
    }

    walk(
        root_node,
        &mut has_command_substitution,
        &mut has_process_substitution,
        &mut has_parameter_expansion,
        &mut has_heredoc,
        &mut has_comment,
    );

    DangerousPatterns {
        has_command_substitution,
        has_process_substitution,
        has_parameter_expansion,
        has_heredoc,
        has_comment,
    }
}

/// Perform complete tree-sitter analysis of a command.
pub fn analyze_command(root_node: &TsNode, command: &str) -> TreeSitterAnalysis {
    TreeSitterAnalysis {
        quote_context: extract_quote_context(root_node, command),
        compound_structure: extract_compound_structure(root_node, command),
        has_actual_operator_nodes: has_actual_operator_nodes(root_node),
        dangerous_patterns: extract_dangerous_patterns(root_node),
    }
}
