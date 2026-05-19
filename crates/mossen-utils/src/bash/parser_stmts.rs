//! Statement-level parser: parseStatements, parseAndOr, parsePipeline, parseCommand,
//! parseSimpleCommand, and compound statements (if/while/for/case/function/declaration/unset).
//!
//! Translated from `bashParser.ts` lines 769–3697.

use crate::bash::lexer::*;
use crate::bash::parser_core::*;
use crate::bash::parser_exprs;
use crate::bash::types::*;

pub fn parse_statements(p: &mut PState, terminator: Option<&str>) -> Vec<TsNode> {
    let mut out: Vec<TsNode> = Vec::new();
    loop {
        skip_blanks(&mut p.l);
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Eof {
            restore_lex(&mut p.l, save);
            break;
        }
        if t.token_type == TokenType::Newline {
            if !p.l.heredocs.is_empty() {
                parser_exprs::scan_heredoc_bodies(p);
            }
            continue;
        }
        if t.token_type == TokenType::Comment {
            out.push(leaf(p, "comment", &t));
            continue;
        }
        if let Some(term) = terminator {
            if t.token_type == TokenType::Op && t.value == term {
                restore_lex(&mut p.l, save);
                break;
            }
        }
        if t.token_type == TokenType::Op
            && matches!(t.value.as_str(), ")" | "}" | ";;" | ";&" | ";;&" | "))" | "]]" | "]")
        {
            restore_lex(&mut p.l, save);
            break;
        }
        if t.token_type == TokenType::Backtick && p.in_backtick > 0 {
            restore_lex(&mut p.l, save);
            break;
        }
        if t.token_type == TokenType::Word
            && matches!(t.value.as_str(), "then" | "elif" | "else" | "fi" | "do" | "done" | "esac")
        {
            restore_lex(&mut p.l, save);
            break;
        }
        restore_lex(&mut p.l, save);
        let stmt = parse_and_or(p);
        if stmt.is_none() {
            break;
        }
        out.push(stmt.unwrap());
        // Look for separator
        skip_blanks(&mut p.l);
        let save2 = save_lex(&p.l);
        let sep = next_token(&mut p.l, LexCtx::Cmd);
        if sep.token_type == TokenType::Op && (sep.value == ";" || sep.value == "&") {
            let save3 = save_lex(&p.l);
            let after = next_token(&mut p.l, LexCtx::Cmd);
            restore_lex(&mut p.l, save3);
            out.push(leaf(p, &sep.value, &sep));
            if after.token_type == TokenType::Eof
                || (after.token_type == TokenType::Op
                    && matches!(after.value.as_str(), ")" | "}" | ";;" | ";&" | ";;&"))
                || (after.token_type == TokenType::Word
                    && matches!(after.value.as_str(), "then" | "elif" | "else" | "fi" | "do" | "done" | "esac"))
            {
                continue;
            }
        } else if sep.token_type == TokenType::Newline {
            if !p.l.heredocs.is_empty() {
                parser_exprs::scan_heredoc_bodies(p);
            }
            continue;
        } else {
            restore_lex(&mut p.l, save2);
        }
    }
    out
}

pub fn parse_and_or(p: &mut PState) -> Option<TsNode> {
    let mut left = parse_pipeline(p)?;
    loop {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Op && (t.value == "&&" || t.value == "||") {
            let op = leaf(p, &t.value, &t);
            skip_newlines(p);
            let right = parse_pipeline(p);
            if right.is_none() {
                let end = op.end_index;
                left = mk(p, "list", left.start_index, end, vec![left, op]);
                break;
            }
            let right = right.unwrap();
            if right.node_type == "redirected_statement" && right.children.len() >= 2 {
                let inner = right.children[0].clone();
                let redirs: Vec<TsNode> = right.children[1..].to_vec();
                let start = left.start_index;
                let list_node = mk(p, "list", start, inner.end_index, vec![left, op, inner]);
                let last_r = &redirs[redirs.len() - 1];
                let end = last_r.end_index;
                let mut kids = vec![list_node];
                kids.extend(redirs);
                left = mk(p, "redirected_statement", start, end, kids);
            } else {
                let start = left.start_index;
                let end = right.end_index;
                left = mk(p, "list", start, end, vec![left, op, right]);
            }
        } else {
            restore_lex(&mut p.l, save);
            break;
        }
    }
    Some(left)
}

fn parse_pipeline(p: &mut PState) -> Option<TsNode> {
    let first = parse_command(p)?;
    let mut parts: Vec<TsNode> = vec![first];
    loop {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Op && (t.value == "|" || t.value == "|&") {
            let op = leaf(p, &t.value, &t);
            skip_newlines(p);
            let next_cmd = parse_command(p);
            if next_cmd.is_none() {
                parts.push(op);
                break;
            }
            let next_cmd = next_cmd.unwrap();
            if next_cmd.node_type == "redirected_statement" && next_cmd.children.len() >= 2 && !parts.is_empty() {
                let inner = next_cmd.children[0].clone();
                let redirs: Vec<TsNode> = next_cmd.children[1..].to_vec();
                let mut pipe_kids: Vec<TsNode> = parts.drain(..).collect();
                pipe_kids.push(op);
                pipe_kids.push(inner.clone());
                let pipe_start = pipe_kids[0].start_index;
                let pipe_node = mk(p, "pipeline", pipe_start, inner.end_index, pipe_kids);
                let last_r = &redirs[redirs.len() - 1];
                let mut wrapped_kids = vec![pipe_node];
                wrapped_kids.extend(redirs.clone());
                let wrapped = mk(p, "redirected_statement", pipe_start, last_r.end_index, wrapped_kids);
                parts.push(wrapped);
                continue;
            }
            parts.push(op);
            parts.push(next_cmd);
        } else {
            restore_lex(&mut p.l, save);
            break;
        }
    }
    if parts.len() == 1 {
        return Some(parts.remove(0));
    }
    let start = parts[0].start_index;
    let end = parts.last().unwrap().end_index;
    Some(mk(p, "pipeline", start, end, parts))
}

pub fn parse_command(p: &mut PState) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let save = save_lex(&p.l);
    let t = next_token(&mut p.l, LexCtx::Cmd);

    if t.token_type == TokenType::Eof {
        restore_lex(&mut p.l, save);
        return None;
    }

    // Negation
    if t.token_type == TokenType::Op && t.value == "!" {
        let bang = leaf(p, "!", &t);
        let inner = parse_command(p);
        if inner.is_none() {
            restore_lex(&mut p.l, save);
            return None;
        }
        let inner = inner.unwrap();
        if inner.node_type == "redirected_statement" && inner.children.len() >= 2 {
            let cmd = inner.children[0].clone();
            let redirs: Vec<TsNode> = inner.children[1..].to_vec();
            let neg = mk(p, "negated_command", bang.start_index, cmd.end_index, vec![bang, cmd]);
            let last_r = &redirs[redirs.len() - 1];
            let mut kids = vec![neg];
            kids.extend(redirs.clone());
            return Some(mk(p, "redirected_statement", kids[0].start_index, last_r.end_index, kids));
        }
        let end = inner.end_index;
        return Some(mk(p, "negated_command", bang.start_index, end, vec![bang, inner]));
    }

    // Subshell
    if t.token_type == TokenType::Op && t.value == "(" {
        let open = leaf(p, "(", &t);
        let body = parse_statements(p, Some(")"));
        let close_tok = next_token(&mut p.l, LexCtx::Cmd);
        let close = if close_tok.token_type == TokenType::Op && close_tok.value == ")" {
            leaf(p, ")", &close_tok)
        } else {
            mk(p, ")", open.end_index, open.end_index, vec![])
        };
        let mut kids = vec![open];
        kids.extend(body);
        let end = close.end_index;
        kids.push(close);
        let node = mk(p, "subshell", kids[0].start_index, end, kids);
        return maybe_redirect(p, node);
    }

    // (( arithmetic ))
    if t.token_type == TokenType::Op && t.value == "((" {
        let open = leaf(p, "((", &t);
        let exprs = parser_exprs::parse_arith_comma_list(p, "))", parser_exprs::ArithMode::Var);
        let close_tok = next_token(&mut p.l, LexCtx::Cmd);
        let close = if close_tok.value == "))" {
            leaf(p, "))", &close_tok)
        } else {
            mk(p, "))", open.end_index, open.end_index, vec![])
        };
        let mut kids = vec![open];
        kids.extend(exprs);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "compound_statement", kids[0].start_index, end, kids));
    }

    // { compound }
    if t.token_type == TokenType::Op && t.value == "{" {
        let open = leaf(p, "{", &t);
        let body = parse_statements(p, Some("}"));
        let close_tok = next_token(&mut p.l, LexCtx::Cmd);
        let close = if close_tok.token_type == TokenType::Op && close_tok.value == "}" {
            leaf(p, "}", &close_tok)
        } else {
            mk(p, "}", open.end_index, open.end_index, vec![])
        };
        let mut kids = vec![open];
        kids.extend(body);
        let end = close.end_index;
        kids.push(close);
        let node = mk(p, "compound_statement", kids[0].start_index, end, kids);
        return maybe_redirect(p, node);
    }

    // [[ and [ test command
    if t.token_type == TokenType::Op && (t.value == "[" || t.value == "[[") {
        let open = leaf(p, &t.value, &t);
        let closer = if t.value == "[" { "]" } else { "]]" };
        let expr_save = save_lex(&p.l);
        let mut expr = parser_exprs::parse_test_expr(p, closer);
        skip_blanks(&mut p.l);
        if t.value == "[" && peek_char(&p.l) != ']' {
            restore_lex(&mut p.l, expr_save);
            let prev_stop = p.stop_token.clone();
            p.stop_token = Some("]".to_string());
            let rstmt = parse_command(p);
            p.stop_token = prev_stop;
            if let Some(ref rs) = rstmt {
                if rs.node_type == "redirected_statement" {
                    expr = rstmt;
                } else {
                    restore_lex(&mut p.l, expr_save);
                    expr = parser_exprs::parse_test_expr(p, closer);
                }
            } else {
                restore_lex(&mut p.l, expr_save);
                expr = parser_exprs::parse_test_expr(p, closer);
            }
            skip_blanks(&mut p.l);
        }
        let close_tok = next_token(&mut p.l, LexCtx::Arg);
        let close = if close_tok.value == closer {
            leaf(p, closer, &close_tok)
        } else {
            mk(p, closer, open.end_index, open.end_index, vec![])
        };
        let mut kids = vec![open.clone()];
        if let Some(e) = expr {
            kids.push(e);
        }
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "test_command", open.start_index, end, kids));
    }

    if t.token_type == TokenType::Word {
        match t.value.as_str() {
            "if" => { let node = parse_if(p, &t); return maybe_redirect_compound(p, node); }
            "while" | "until" => { let node = parse_while(p, &t); return maybe_redirect_compound(p, node); }
            "for" | "select" => { let node = parse_for(p, &t); return maybe_redirect_compound(p, node); }
            "case" => { let node = parse_case(p, &t); return maybe_redirect_compound(p, node); }
            "function" => return Some(parse_function(p, &t)),
            kw if DECL_KEYWORDS.contains(kw) => {
                let node = parse_declaration(p, &t);
                return maybe_redirect(p, node);
            }
            "unset" | "unsetenv" => {
                let node = parse_unset(p, &t);
                return maybe_redirect(p, node);
            }
            _ => {}
        }
    }

    restore_lex(&mut p.l, save);
    parse_simple_command(p)
}

fn maybe_redirect(p: &mut PState, node: TsNode) -> Option<TsNode> {
    maybe_redirect_inner(p, node, false)
}

fn maybe_redirect_compound(p: &mut PState, node: TsNode) -> Option<TsNode> {
    maybe_redirect_inner(p, node, true)
}

fn maybe_redirect_inner(p: &mut PState, node: TsNode, greedy: bool) -> Option<TsNode> {
    let mut redirs = Vec::new();
    loop {
        skip_blanks(&mut p.l);
        let r = parser_exprs::try_parse_redirect(p, greedy);
        if let Some(redir) = r {
            redirs.push(redir);
        } else {
            break;
        }
    }
    if redirs.is_empty() {
        return Some(node);
    }
    let start = node.start_index;
    let end = redirs.last().unwrap().end_index;
    let mut kids = vec![node];
    kids.extend(redirs);
    Some(mk(p, "redirected_statement", start, end, kids))
}

fn parse_simple_command(p: &mut PState) -> Option<TsNode> {
    let mut assignments: Vec<TsNode> = Vec::new();
    let mut pre_redirects: Vec<TsNode> = Vec::new();

    loop {
        skip_blanks(&mut p.l);
        if let Some(a) = parser_exprs::try_parse_assignment(p) {
            assignments.push(a);
            continue;
        }
        if let Some(r) = parser_exprs::try_parse_redirect(p, false) {
            pre_redirects.push(r);
            continue;
        }
        break;
    }

    skip_blanks(&mut p.l);
    let save = save_lex(&p.l);
    let name_tok = next_token(&mut p.l, LexCtx::Cmd);
    if name_tok.token_type == TokenType::Eof
        || name_tok.token_type == TokenType::Newline
        || name_tok.token_type == TokenType::Comment
        || (name_tok.token_type == TokenType::Op
            && name_tok.value != "{"
            && name_tok.value != "["
            && name_tok.value != "[[")
        || (name_tok.token_type == TokenType::Word
            && SHELL_KEYWORDS.contains(name_tok.value.as_str())
            && name_tok.value != "in")
    {
        restore_lex(&mut p.l, save);
        if assignments.len() == 1 && pre_redirects.is_empty() {
            return Some(assignments.remove(0));
        }
        if !pre_redirects.is_empty() && assignments.is_empty() {
            let start = pre_redirects[0].start_index;
            let end = pre_redirects.last().unwrap().end_index;
            return Some(mk(p, "redirected_statement", start, end, pre_redirects));
        }
        if assignments.len() > 1 && pre_redirects.is_empty() {
            let start = assignments[0].start_index;
            let end = assignments.last().unwrap().end_index;
            return Some(mk(p, "variable_assignments", start, end, assignments));
        }
        if !assignments.is_empty() || !pre_redirects.is_empty() {
            let mut kids = Vec::new();
            kids.extend(assignments);
            kids.extend(pre_redirects);
            let start = kids[0].start_index;
            let end = kids.last().unwrap().end_index;
            return Some(mk(p, "command", start, end, kids));
        }
        return None;
    }
    restore_lex(&mut p.l, save);

    // Parse command name
    let cmd_word = parser_exprs::parse_word(p, "cmd");
    if cmd_word.is_none() && assignments.is_empty() && pre_redirects.is_empty() {
        return None;
    }

    let mut kids: Vec<TsNode> = Vec::new();
    kids.extend(assignments);
    kids.extend(pre_redirects.clone());

    if let Some(cw) = cmd_word {
        let name_node = mk(p, "command_name", cw.start_index, cw.end_index, vec![cw]);
        kids.push(name_node);
    }

    // Check if command name is a function definition: name()
    if kids.last().map(|k| k.node_type.as_str()) == Some("command_name") {
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == '(' && peek(&p.l, 1) == ')' {
            // Function definition without `function` keyword
            let name_node = kids.pop().unwrap();
            let name_inner = if name_node.children.is_empty() {
                name_node.clone()
            } else {
                name_node.children[0].clone()
            };
            let o_tok = next_token(&mut p.l, LexCtx::Cmd);
            let c_tok = next_token(&mut p.l, LexCtx::Cmd);
            let mut fn_kids = vec![name_inner, leaf(p, "(", &o_tok), leaf(p, ")", &c_tok)];
            skip_blanks(&mut p.l);
            skip_newlines(p);
            let body = parse_command(p);
            if let Some(b) = body {
                if b.node_type == "redirected_statement"
                    && b.children.len() >= 2
                    && b.children[0].node_type == "compound_statement"
                {
                    fn_kids.extend(b.children);
                } else {
                    fn_kids.push(b);
                }
            }
            let start = fn_kids[0].start_index;
            let end = fn_kids.last().unwrap().end_index;
            return Some(mk(p, "function_definition", start, end, fn_kids));
        }
    }

    // Parse arguments and post-redirects
    loop {
        skip_blanks(&mut p.l);
        let c = peek_char(&p.l);
        if c == '\0' || c == '\n' || c == ';' || c == '&' || c == '|' || c == ')' {
            break;
        }
        if c == '#' {
            break;
        }
        // Check stop_token
        if let Some(ref st) = p.stop_token {
            let sv = save_lex(&p.l);
            let tk = next_token(&mut p.l, LexCtx::Arg);
            restore_lex(&mut p.l, sv);
            if tk.value == *st {
                break;
            }
        }
        // Try redirect
        if let Some(r) = parser_exprs::try_parse_redirect(p, true) {
            kids.push(r);
            continue;
        }
        // Heredoc operators
        let sv = save_lex(&p.l);
        let tk = next_token(&mut p.l, LexCtx::Arg);
        if tk.token_type == TokenType::Op && (tk.value == "<<" || tk.value == "<<-" || tk.value == "<<<") {
            if tk.value == "<<<" {
                // Herestring
                let op_node = leaf(p, "<<<", &tk);
                skip_blanks(&mut p.l);
                let content = parser_exprs::parse_word(p, "arg");
                let mut hkids = vec![op_node.clone()];
                if let Some(c_node) = content {
                    let end = c_node.end_index;
                    hkids.push(c_node);
                    kids.push(mk(p, "herestring_redirect", op_node.start_index, end, hkids));
                } else {
                    kids.push(mk(p, "herestring_redirect", op_node.start_index, op_node.end_index, hkids));
                }
            } else {
                // Heredoc
                let strip_tabs = tk.value == "<<-";
                let op_node = leaf(p, &tk.value, &tk);
                skip_blanks(&mut p.l);
                let (delim, quoted) = parse_heredoc_delim(p);
                let delim_start = p.l.b;
                // Register pending heredoc
                p.l.heredocs.push(HeredocPending {
                    delim: delim.clone(),
                    strip_tabs,
                    quoted,
                    body_start: 0,
                    body_end: 0,
                    end_start: 0,
                    end_end: 0,
                });
                let delim_node = mk(p, "heredoc_start", op_node.end_index, delim_start, vec![]);
                let hkids = vec![op_node.clone(), delim_node.clone()];
                kids.push(mk(p, "heredoc_redirect", op_node.start_index, delim_node.end_index, hkids));
            }
            continue;
        }
        restore_lex(&mut p.l, sv);

        // Parse argument word
        let arg = parser_exprs::parse_word(p, "arg");
        if let Some(a) = arg {
            kids.push(a);
        } else {
            break;
        }
    }

    if kids.is_empty() {
        return None;
    }
    let start = kids[0].start_index;
    let end = kids.last().unwrap().end_index;
    // Separate into command body + trailing redirects
    let mut cmd_kids = Vec::new();
    let mut trail_redirs = Vec::new();
    for k in kids {
        if k.node_type == "file_redirect" || k.node_type == "heredoc_redirect" || k.node_type == "herestring_redirect" {
            trail_redirs.push(k);
        } else {
            cmd_kids.push(k);
        }
    }
    if trail_redirs.is_empty() {
        return Some(mk(p, "command", start, end, cmd_kids));
    }
    let cmd_end = cmd_kids.last().map(|k| k.end_index).unwrap_or(start);
    let cmd_node = mk(p, "command", start, cmd_end, cmd_kids);
    let redir_end = trail_redirs.last().unwrap().end_index;
    let mut all = vec![cmd_node];
    all.extend(trail_redirs);
    Some(mk(p, "redirected_statement", start, redir_end, all))
}

fn parse_heredoc_delim(p: &mut PState) -> (String, bool) {
    let c = peek_char(&p.l);
    if c == '\'' {
        // Quoted delimiter
        advance(&mut p.l);
        let mut delim = String::new();
        while p.l.i < p.l.len && peek_char(&p.l) != '\'' {
            delim.push(peek_char(&p.l));
            advance(&mut p.l);
        }
        if p.l.i < p.l.len { advance(&mut p.l); }
        return (delim, true);
    }
    if c == '"' {
        advance(&mut p.l);
        let mut delim = String::new();
        while p.l.i < p.l.len && peek_char(&p.l) != '"' {
            delim.push(peek_char(&p.l));
            advance(&mut p.l);
        }
        if p.l.i < p.l.len { advance(&mut p.l); }
        return (delim, true);
    }
    // Unquoted
    let mut delim = String::new();
    let mut has_backslash = false;
    while p.l.i < p.l.len {
        let ch = peek_char(&p.l);
        if ch == '\\' {
            has_backslash = true;
            advance(&mut p.l);
            if p.l.i < p.l.len {
                delim.push(peek_char(&p.l));
                advance(&mut p.l);
            }
            continue;
        }
        if !is_heredoc_delim_char(ch) { break; }
        delim.push(ch);
        advance(&mut p.l);
    }
    (delim, has_backslash)
}

fn parse_if(p: &mut PState, if_tok: &Token) -> TsNode {
    let if_kw = leaf(p, "if", if_tok);
    let mut kids: Vec<TsNode> = vec![if_kw.clone()];
    let cond = parse_statements(p, None);
    kids.extend(cond);
    consume_keyword(p, "then", &mut kids);
    let body = parse_statements(p, None);
    kids.extend(body);
    loop {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Word && t.value == "elif" {
            let e_kw = leaf(p, "elif", &t);
            let e_cond = parse_statements(p, None);
            let mut e_kids: Vec<TsNode> = vec![e_kw.clone()];
            e_kids.extend(e_cond);
            consume_keyword(p, "then", &mut e_kids);
            let e_body = parse_statements(p, None);
            e_kids.extend(e_body);
            let end = e_kids.last().unwrap().end_index;
            kids.push(mk(p, "elif_clause", e_kw.start_index, end, e_kids));
        } else if t.token_type == TokenType::Word && t.value == "else" {
            let el_kw = leaf(p, "else", &t);
            let el_body = parse_statements(p, None);
            let end = if el_body.is_empty() { el_kw.end_index } else { el_body.last().unwrap().end_index };
            let mut el_kids = vec![el_kw.clone()];
            el_kids.extend(el_body);
            kids.push(mk(p, "else_clause", el_kw.start_index, end, el_kids));
        } else {
            restore_lex(&mut p.l, save);
            break;
        }
    }
    consume_keyword(p, "fi", &mut kids);
    let end = kids.last().unwrap().end_index;
    mk(p, "if_statement", if_kw.start_index, end, kids)
}

fn parse_while(p: &mut PState, kw_tok: &Token) -> TsNode {
    let kw = leaf(p, &kw_tok.value, kw_tok);
    let mut kids: Vec<TsNode> = vec![kw.clone()];
    let cond = parse_statements(p, None);
    kids.extend(cond);
    if let Some(dg) = parse_do_group(p) {
        kids.push(dg);
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "while_statement", kw.start_index, end, kids)
}

fn parse_for(p: &mut PState, for_tok: &Token) -> TsNode {
    let for_kw = leaf(p, &for_tok.value, for_tok);
    skip_blanks(&mut p.l);
    // C-style for (( ; ; ))
    if for_tok.value == "for" && peek_char(&p.l) == '(' && peek(&p.l, 1) == '(' {
        let o_start = p.l.b;
        advance(&mut p.l); advance(&mut p.l);
        let open = mk(p, "((", o_start, p.l.b, vec![]);
        let mut kids: Vec<TsNode> = vec![for_kw.clone(), open];
        for k in 0..3 {
            skip_blanks(&mut p.l);
            let stop = if k < 2 { ";" } else { "))" };
            let es = parser_exprs::parse_arith_comma_list(p, stop, parser_exprs::ArithMode::Assign);
            kids.extend(es);
            if k < 2 {
                if peek_char(&p.l) == ';' {
                    let s = p.l.b;
                    advance(&mut p.l);
                    kids.push(mk(p, ";", s, p.l.b, vec![]));
                }
            }
        }
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == ')' && peek(&p.l, 1) == ')' {
            let c_start = p.l.b;
            advance(&mut p.l); advance(&mut p.l);
            kids.push(mk(p, "))", c_start, p.l.b, vec![]));
        }
        let save = save_lex(&p.l);
        let sep = next_token(&mut p.l, LexCtx::Cmd);
        if sep.token_type == TokenType::Op && sep.value == ";" {
            kids.push(leaf(p, ";", &sep));
        } else if sep.token_type != TokenType::Newline {
            restore_lex(&mut p.l, save);
        }
        if let Some(dg) = parse_do_group(p) {
            kids.push(dg);
        } else {
            skip_newlines(p);
            skip_blanks(&mut p.l);
            if peek_char(&p.l) == '{' {
                let b_open = p.l.b;
                advance(&mut p.l);
                let brace = mk(p, "{", b_open, p.l.b, vec![]);
                let body = parse_statements(p, Some("}"));
                let b_close = if peek_char(&p.l) == '}' {
                    let cs = p.l.b;
                    advance(&mut p.l);
                    mk(p, "}", cs, p.l.b, vec![])
                } else {
                    mk(p, "}", p.l.b, p.l.b, vec![])
                };
                let mut cs_kids = vec![brace];
                cs_kids.extend(body);
                let end = b_close.end_index;
                cs_kids.push(b_close);
                kids.push(mk(p, "compound_statement", cs_kids[0].start_index, end, cs_kids));
            }
        }
        let end = kids.last().unwrap().end_index;
        return mk(p, "c_style_for_statement", for_kw.start_index, end, kids);
    }

    // Regular for
    let mut kids: Vec<TsNode> = vec![for_kw.clone()];
    let var_tok = next_token(&mut p.l, LexCtx::Arg);
    kids.push(mk(p, "variable_name", var_tok.start, var_tok.end, vec![]));
    skip_blanks(&mut p.l);
    let save = save_lex(&p.l);
    let in_tok = next_token(&mut p.l, LexCtx::Arg);
    if in_tok.token_type == TokenType::Word && in_tok.value == "in" {
        kids.push(leaf(p, "in", &in_tok));
        loop {
            skip_blanks(&mut p.l);
            let c = peek_char(&p.l);
            if c == ';' || c == '\n' || c == '\0' { break; }
            let w = parser_exprs::parse_word(p, "arg");
            if let Some(word) = w {
                kids.push(word);
            } else {
                break;
            }
        }
    } else {
        restore_lex(&mut p.l, save);
    }
    let save2 = save_lex(&p.l);
    let sep = next_token(&mut p.l, LexCtx::Cmd);
    if sep.token_type == TokenType::Op && sep.value == ";" {
        kids.push(leaf(p, ";", &sep));
    } else if sep.token_type != TokenType::Newline {
        restore_lex(&mut p.l, save2);
    }
    if let Some(dg) = parse_do_group(p) {
        kids.push(dg);
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "for_statement", for_kw.start_index, end, kids)
}

fn parse_do_group(p: &mut PState) -> Option<TsNode> {
    skip_newlines(p);
    let save = save_lex(&p.l);
    let do_tok = next_token(&mut p.l, LexCtx::Cmd);
    if do_tok.token_type != TokenType::Word || do_tok.value != "do" {
        restore_lex(&mut p.l, save);
        return None;
    }
    let do_kw = leaf(p, "do", &do_tok);
    let body = parse_statements(p, None);
    let mut kids: Vec<TsNode> = vec![do_kw.clone()];
    kids.extend(body);
    consume_keyword(p, "done", &mut kids);
    let end = kids.last().unwrap().end_index;
    Some(mk(p, "do_group", do_kw.start_index, end, kids))
}

fn parse_case(p: &mut PState, case_tok: &Token) -> TsNode {
    let case_kw = leaf(p, "case", case_tok);
    let mut kids: Vec<TsNode> = vec![case_kw.clone()];
    skip_blanks(&mut p.l);
    if let Some(word) = parser_exprs::parse_word(p, "arg") {
        kids.push(word);
    }
    skip_blanks(&mut p.l);
    consume_keyword(p, "in", &mut kids);
    skip_newlines(p);
    loop {
        skip_blanks(&mut p.l);
        skip_newlines(p);
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Arg);
        if t.token_type == TokenType::Word && t.value == "esac" {
            kids.push(leaf(p, "esac", &t));
            break;
        }
        if t.token_type == TokenType::Eof { break; }
        restore_lex(&mut p.l, save);
        if let Some(item) = parse_case_item(p) {
            kids.push(item);
        } else {
            break;
        }
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "case_statement", case_kw.start_index, end, kids)
}

fn parse_case_item(p: &mut PState) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let start = p.l.b;
    let mut kids: Vec<TsNode> = Vec::new();
    if peek_char(&p.l) == '(' {
        let s = p.l.b;
        advance(&mut p.l);
        kids.push(mk(p, "(", s, p.l.b, vec![]));
    }
    // Parse patterns
    loop {
        skip_blanks(&mut p.l);
        let c = peek_char(&p.l);
        if c == ')' || c == '\0' { break; }
        let pat = parse_case_pattern(p);
        if pat.is_empty() { break; }
        kids.extend(pat);
        skip_blanks(&mut p.l);
        // Line continuation
        if peek_char(&p.l) == '\\' && peek(&p.l, 1) == '\n' {
            advance(&mut p.l); advance(&mut p.l);
            skip_blanks(&mut p.l);
        }
        if peek_char(&p.l) == '|' {
            let s = p.l.b;
            advance(&mut p.l);
            kids.push(mk(p, "|", s, p.l.b, vec![]));
            if peek_char(&p.l) == '\\' && peek(&p.l, 1) == '\n' {
                advance(&mut p.l); advance(&mut p.l);
            }
        } else {
            break;
        }
    }
    if peek_char(&p.l) == ')' {
        let s = p.l.b;
        advance(&mut p.l);
        kids.push(mk(p, ")", s, p.l.b, vec![]));
    }
    let body = parse_statements(p, None);
    kids.extend(body.clone());
    let save = save_lex(&p.l);
    let term = next_token(&mut p.l, LexCtx::Cmd);
    if term.token_type == TokenType::Op && matches!(term.value.as_str(), ";;" | ";&" | ";;&") {
        kids.push(leaf(p, &term.value, &term));
    } else {
        restore_lex(&mut p.l, save);
    }
    if kids.is_empty() { return None; }
    let end = kids.last().unwrap().end_index;
    Some(mk(p, "case_item", start, end, kids))
}

fn parse_case_pattern(p: &mut PState) -> Vec<TsNode> {
    skip_blanks(&mut p.l);
    let start = p.l.b;
    let start_i = p.l.i;
    let mut paren_depth = 0;
    let mut has_quote = false;
    while p.l.i < p.l.len {
        let c = peek_char(&p.l);
        if c == '\\' && p.l.i + 1 < p.l.len {
            advance(&mut p.l); advance(&mut p.l);
            continue;
        }
        if c == '"' || c == '\'' {
            has_quote = true;
            advance(&mut p.l);
            while p.l.i < p.l.len && peek_char(&p.l) != c {
                if peek_char(&p.l) == '\\' && p.l.i + 1 < p.l.len { advance(&mut p.l); }
                advance(&mut p.l);
            }
            if peek_char(&p.l) == c { advance(&mut p.l); }
            continue;
        }
        if c == '(' { paren_depth += 1; advance(&mut p.l); continue; }
        if paren_depth > 0 {
            if c == ')' { paren_depth -= 1; advance(&mut p.l); continue; }
            if c == '\n' { break; }
            advance(&mut p.l);
            continue;
        }
        if matches!(c, ')' | '|' | ' ' | '\t' | '\n') { break; }
        advance(&mut p.l);
    }
    if p.l.b == start { return vec![]; }
    let text: String = p.l.src[start_i..p.l.i].iter().collect();
    let has_extglob = regex::Regex::new(r"[*?+@!]\(").unwrap().is_match(&text);
    if has_quote && !has_extglob {
        // Would need segmented parse; simplified: emit as word
        return vec![mk(p, "word", start, p.l.b, vec![])];
    }
    let node_type = if has_extglob || text.contains('*') || text.contains('?') {
        "extglob_pattern"
    } else {
        "word"
    };
    vec![mk(p, node_type, start, p.l.b, vec![])]
}

fn parse_function(p: &mut PState, fn_tok: &Token) -> TsNode {
    let fn_kw = leaf(p, "function", fn_tok);
    skip_blanks(&mut p.l);
    let name_tok = next_token(&mut p.l, LexCtx::Arg);
    let name = mk(p, "word", name_tok.start, name_tok.end, vec![]);
    let mut kids: Vec<TsNode> = vec![fn_kw.clone(), name];
    skip_blanks(&mut p.l);
    if peek_char(&p.l) == '(' && peek(&p.l, 1) == ')' {
        let o = next_token(&mut p.l, LexCtx::Cmd);
        let c = next_token(&mut p.l, LexCtx::Cmd);
        kids.push(leaf(p, "(", &o));
        kids.push(leaf(p, ")", &c));
    }
    skip_blanks(&mut p.l);
    skip_newlines(p);
    if let Some(body) = parse_command(p) {
        if body.node_type == "redirected_statement"
            && body.children.len() >= 2
            && body.children[0].node_type == "compound_statement"
        {
            kids.extend(body.children);
        } else {
            kids.push(body);
        }
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "function_definition", fn_kw.start_index, end, kids)
}

fn parse_declaration(p: &mut PState, kw_tok: &Token) -> TsNode {
    let kw = leaf(p, &kw_tok.value, kw_tok);
    let mut kids: Vec<TsNode> = vec![kw.clone()];
    loop {
        skip_blanks(&mut p.l);
        let c = peek_char(&p.l);
        if matches!(c, '\0' | '\n' | ';' | '&' | '|' | ')' | '<' | '>') { break; }
        if let Some(a) = parser_exprs::try_parse_assignment(p) {
            kids.push(a);
            continue;
        }
        if c == '"' || c == '\'' || c == '$' {
            if let Some(w) = parser_exprs::parse_word(p, "arg") {
                kids.push(w);
                continue;
            }
            break;
        }
        let save = save_lex(&p.l);
        let tok = next_token(&mut p.l, LexCtx::Arg);
        if tok.token_type == TokenType::Word || tok.token_type == TokenType::Number {
            if tok.value.starts_with('-') {
                kids.push(leaf(p, "word", &tok));
            } else if is_ident_start(tok.value.chars().next().unwrap_or('\0')) {
                kids.push(mk(p, "variable_name", tok.start, tok.end, vec![]));
            } else {
                kids.push(leaf(p, "word", &tok));
            }
        } else {
            restore_lex(&mut p.l, save);
            break;
        }
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "declaration_command", kw.start_index, end, kids)
}

fn parse_unset(p: &mut PState, kw_tok: &Token) -> TsNode {
    let kw = leaf(p, "unset", kw_tok);
    let mut kids: Vec<TsNode> = vec![kw.clone()];
    loop {
        skip_blanks(&mut p.l);
        let c = peek_char(&p.l);
        if matches!(c, '\0' | '\n' | ';' | '&' | '|' | ')' | '<' | '>') { break; }
        let arg = parser_exprs::parse_word(p, "arg");
        if let Some(a) = arg {
            if a.node_type == "word" {
                if a.text.starts_with('-') {
                    kids.push(a);
                } else {
                    kids.push(mk(p, "variable_name", a.start_index, a.end_index, vec![]));
                }
            } else {
                kids.push(a);
            }
        } else {
            break;
        }
    }
    let end = kids.last().unwrap().end_index;
    mk(p, "unset_command", kw.start_index, end, kids)
}

fn consume_keyword(p: &mut PState, name: &str, kids: &mut Vec<TsNode>) {
    skip_newlines(p);
    let save = save_lex(&p.l);
    let t = next_token(&mut p.l, LexCtx::Cmd);
    if t.token_type == TokenType::Word && t.value == name {
        kids.push(leaf(p, name, &t));
    } else {
        restore_lex(&mut p.l, save);
    }
}
