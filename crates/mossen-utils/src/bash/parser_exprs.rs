//! Expression-level parser: parseWord, parseDollarLike, parseDoubleQuoted, arithmetic,
//! test expressions, redirects, assignments, heredoc body scanning.
//!
//! Translated from `bashParser.ts` lines 1200–4437.

use crate::bash::lexer::*;
use crate::bash::parser_core::*;
use crate::bash::types::*;
use regex::Regex;

// ─── Redirects & Assignments ───

pub fn try_parse_assignment(p: &mut PState) -> Option<TsNode> {
    let save = save_lex(&p.l);
    skip_blanks(&mut p.l);
    let c = peek_char(&p.l);
    if !is_ident_start(c) {
        restore_lex(&mut p.l, save);
        return None;
    }
    let name_start = p.l.b;
    while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
    let name_end = p.l.b;
    let nc = peek_char(&p.l);
    let is_append;
    if nc == '+' && peek(&p.l, 1) == '=' {
        is_append = true;
        advance(&mut p.l); advance(&mut p.l);
    } else if nc == '=' {
        is_append = false;
        advance(&mut p.l);
    } else if nc == '[' {
        // Array subscript: name[idx]=val
        advance(&mut p.l);
        while p.l.i < p.l.len && peek_char(&p.l) != ']' { advance(&mut p.l); }
        if peek_char(&p.l) == ']' { advance(&mut p.l); }
        let after = peek_char(&p.l);
        if after == '+' && peek(&p.l, 1) == '=' {
            is_append = true;
            advance(&mut p.l); advance(&mut p.l);
        } else if after == '=' {
            is_append = false;
            advance(&mut p.l);
        } else {
            restore_lex(&mut p.l, save);
            return None;
        }
    } else {
        restore_lex(&mut p.l, save);
        return None;
    }
    let _ = is_append;
    let eq_end = p.l.b;
    // Value
    let vn = mk(p, "variable_name", name_start, name_end, vec![]);
    let eq_str = if is_append { "+=" } else { "=" };
    let eq_node = mk(p, eq_str, eq_end - if is_append { 2 } else { 1 }, eq_end, vec![]);
    // Array value: (elem1 elem2 ...)
    if peek_char(&p.l) == '(' {
        let arr_start = p.l.b;
        advance(&mut p.l);
        let open = mk(p, "(", arr_start, p.l.b, vec![]);
        let mut elems = vec![open];
        loop {
            skip_blanks(&mut p.l);
            let c = peek_char(&p.l);
            if c == ')' || c == '\0' { break; }
            if c == '\n' { advance(&mut p.l); continue; }
            if let Some(w) = parse_word(p, "arg") {
                elems.push(w);
            } else {
                break;
            }
        }
        if peek_char(&p.l) == ')' {
            let cs = p.l.b;
            advance(&mut p.l);
            elems.push(mk(p, ")", cs, p.l.b, vec![]));
        }
        let arr_end = p.l.b;
        let arr = mk(p, "array", arr_start, arr_end, elems);
        let end = arr.end_index;
        return Some(mk(p, "variable_assignment", name_start, end, vec![vn, eq_node, arr]));
    }
    // Scalar value
    let val = parse_word(p, "arg");
    let mut kids = vec![vn, eq_node];
    let end;
    if let Some(v) = val {
        end = v.end_index;
        kids.push(v);
    } else {
        end = eq_end;
    }
    Some(mk(p, "variable_assignment", name_start, end, kids))
}

fn is_redirect_literal_start(p: &PState) -> bool {
    let c = peek_char(&p.l);
    c != '\0' && c != '\n' && c != ' ' && c != '\t' && c != ';' && c != '&' && c != '|' && c != ')'
        && c != '>' && c != '<'
        && !(c == '(' && peek(&p.l, 0) == '(') // avoid confusing with subshell
}

pub fn try_parse_redirect(p: &mut PState, greedy: bool) -> Option<TsNode> {
    let save = save_lex(&p.l);
    skip_blanks(&mut p.l);
    // Optional fd number
    let mut fd: Option<TsNode> = None;
    let c = peek_char(&p.l);
    if is_digit(c) {
        let fd_start = p.l.b;
        let mut j = p.l.i;
        while j < p.l.len && is_digit(p.l.src[j]) { j += 1; }
        let after = if j < p.l.len { p.l.src[j] } else { '\0' };
        if after == '>' || after == '<' {
            while p.l.i < j { advance(&mut p.l); }
            fd = Some(mk(p, "file_descriptor", fd_start, p.l.b, vec![]));
        } else {
            restore_lex(&mut p.l, save);
            return None;
        }
    } else if c == '{' {
        // {varname}> redirect
        let fd_start = p.l.b;
        advance(&mut p.l);
        if is_ident_start(peek_char(&p.l)) {
            while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
            if peek_char(&p.l) == '}' {
                advance(&mut p.l);
                let nc = peek_char(&p.l);
                if nc == '>' || nc == '<' {
                    fd = Some(mk(p, "file_descriptor", fd_start, p.l.b, vec![]));
                } else {
                    restore_lex(&mut p.l, save);
                    return None;
                }
            } else {
                restore_lex(&mut p.l, save);
                return None;
            }
        } else {
            restore_lex(&mut p.l, save);
            return None;
        }
    }
    // Operator
    let op_save = save_lex(&p.l);
    let t = next_token(&mut p.l, LexCtx::Arg);
    let v = &t.value;
    if t.token_type != TokenType::Op {
        restore_lex(&mut p.l, save);
        return None;
    }
    // Herestring
    if v == "<<<" {
        restore_lex(&mut p.l, op_save);
        restore_lex(&mut p.l, save);
        return None;
    }
    // Heredoc
    if v == "<<" || v == "<<-" {
        restore_lex(&mut p.l, op_save);
        restore_lex(&mut p.l, save);
        return None;
    }
    // Close fd: >&- <&-
    if v == ">&-" || v == "<&-" {
        let op = leaf(p, v, &t);
        let mut kids = Vec::new();
        if let Some(f) = fd { kids.push(f); }
        kids.push(op.clone());
        let start_idx = kids[0].start_index;
        return Some(mk(p, "file_redirect", start_idx, op.end_index, kids));
    }
    // Standard redirections
    if matches!(v.as_str(), ">" | ">>" | ">&" | ">|" | "&>" | "&>>" | "<" | "<&") {
        let op = leaf(p, v, &t);
        let mut kids = Vec::new();
        if let Some(f) = fd { kids.push(f); }
        kids.push(op.clone());
        let mut end = op.end_index;
        let mut taken = 0;
        loop {
            skip_blanks(&mut p.l);
            let nc = peek_char(&p.l);
            if nc == '\0' || nc == '\n' || nc == ';' || nc == '&' || nc == '|' || nc == ')' { break; }
            if !greedy && taken >= 1 { break; }
            // Process substitution check
            if (nc == '<' || nc == '>') && peek(&p.l, 1) == '(' {
                if let Some(ps) = parse_process_sub(p) {
                    end = ps.end_index;
                    kids.push(ps);
                    taken += 1;
                    continue;
                }
            }
            if let Some(target) = parse_word(p, "arg") {
                end = target.end_index;
                kids.push(target);
                taken += 1;
            } else {
                break;
            }
        }
        let start_idx = kids[0].start_index;
        return Some(mk(p, "file_redirect", start_idx, end, kids));
    }
    restore_lex(&mut p.l, save);
    None
}

fn parse_process_sub(p: &mut PState) -> Option<TsNode> {
    let c = peek_char(&p.l);
    if (c != '<' && c != '>') || peek(&p.l, 1) != '(' { return None; }
    let start = p.l.b;
    advance(&mut p.l); advance(&mut p.l);
    let open_str = format!("{}(", c);
    let open = mk(p, &open_str, start, p.l.b, vec![]);
    let body = crate::bash::parser_stmts::parse_statements(p, Some(")"));
    skip_blanks(&mut p.l);
    let close = if peek_char(&p.l) == ')' {
        let cs = p.l.b;
        advance(&mut p.l);
        mk(p, ")", cs, p.l.b, vec![])
    } else {
        mk(p, ")", p.l.b, p.l.b, vec![])
    };
    let mut kids = vec![open];
    kids.extend(body);
    let end = close.end_index;
    kids.push(close);
    Some(mk(p, "process_substitution", start, end, kids))
}

// ─── Heredoc Body Scanning ───

pub fn scan_heredoc_bodies(p: &mut PState) {
    // Skip to newline
    while p.l.i < p.l.len && p.l.src[p.l.i] != '\n' { advance(&mut p.l); }
    if p.l.i < p.l.len { advance(&mut p.l); }
    let heredocs = std::mem::take(&mut p.l.heredocs);
    let mut updated = Vec::new();
    for mut hd in heredocs {
        hd.body_start = p.l.b;
        let delim_len = hd.delim.len();
        let delim_chars: Vec<char> = hd.delim.chars().collect();
        while p.l.i < p.l.len {
            let line_start_b = p.l.b;
            let mut check_i = p.l.i;
            if hd.strip_tabs {
                while check_i < p.l.len && p.l.src[check_i] == '\t' { check_i += 1; }
            }
            // Check if this line matches delimiter
            let mut matches_delim = true;
            if check_i + delim_len <= p.l.len {
                for (k, dc) in delim_chars.iter().enumerate() {
                    if p.l.src[check_i + k] != *dc {
                        matches_delim = false;
                        break;
                    }
                }
                if matches_delim {
                    let after_idx = check_i + delim_len;
                    if after_idx >= p.l.len || p.l.src[after_idx] == '\n' || p.l.src[after_idx] == '\r' {
                        hd.body_end = line_start_b;
                        while p.l.i < check_i { advance(&mut p.l); }
                        hd.end_start = p.l.b;
                        for _ in 0..delim_len { advance(&mut p.l); }
                        hd.end_end = p.l.b;
                        if p.l.i < p.l.len && p.l.src[p.l.i] == '\n' { advance(&mut p.l); }
                        updated.push(hd);
                        break;
                    }
                }
            } else {
                matches_delim = false;
            }
            // Consume line
            while p.l.i < p.l.len && p.l.src[p.l.i] != '\n' { advance(&mut p.l); }
            if p.l.i < p.l.len { advance(&mut p.l); }
            if !matches_delim && p.l.i >= p.l.len {
                hd.body_end = p.l.b;
                hd.end_start = p.l.b;
                hd.end_end = p.l.b;
                updated.push(hd);
                break;
            }
        }
    }
    p.l.heredocs = updated;
}

// ─── Word Parsing ───

pub fn parse_word(p: &mut PState, _ctx: &str) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let mut parts: Vec<TsNode> = Vec::new();
    while p.l.i < p.l.len {
        let c = peek_char(&p.l);
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '|' | '&' | ';' | '(' | ')') { break; }
        if (c == '<' || c == '>') && peek(&p.l, 1) != '(' { break; }
        if c == '<' || c == '>' {
            if let Some(ps) = parse_process_sub(p) {
                parts.push(ps);
                continue;
            }
            break;
        }
        if c == '"' {
            parts.push(parse_double_quoted(p));
            continue;
        }
        if c == '\'' {
            let tok = next_token(&mut p.l, LexCtx::Arg);
            parts.push(leaf(p, "raw_string", &tok));
            continue;
        }
        if c == '$' {
            let c1 = peek(&p.l, 1);
            if c1 == '\'' {
                let tok = next_token(&mut p.l, LexCtx::Arg);
                parts.push(leaf(p, "ansi_c_string", &tok));
                continue;
            }
            if c1 == '"' {
                let d_start = p.l.b;
                advance(&mut p.l);
                let d_tok = Token::new(TokenType::Dollar, "$", d_start, p.l.b);
                parts.push(leaf(p, "$", &d_tok));
                parts.push(parse_double_quoted(p));
                continue;
            }
            if c1 == '`' {
                advance(&mut p.l);
                continue;
            }
            if let Some(exp) = parse_dollar_like(p) {
                parts.push(exp);
            }
            continue;
        }
        if c == '`' {
            if p.in_backtick > 0 { break; }
            if let Some(bt) = parse_backtick(p) {
                parts.push(bt);
            }
            continue;
        }
        if c == '{' {
            // Try brace expression {N..M}
            if let Some(be) = try_parse_brace_expr(p) {
                parts.push(be);
                continue;
            }
            let nc = peek(&p.l, 1);
            if matches!(nc, ';' | '|' | '&' | '\n' | '\0' | ')' | ' ' | '\t') {
                let b_start = p.l.b;
                advance(&mut p.l);
                parts.push(mk(p, "word", b_start, p.l.b, vec![]));
                continue;
            }
            // Brace-like concatenation
            if let Some(cat) = try_parse_brace_like_cat(p) {
                parts.extend(cat);
                continue;
            }
        }
        if c == '}' {
            let b_start = p.l.b;
            advance(&mut p.l);
            parts.push(mk(p, "word", b_start, p.l.b, vec![]));
            continue;
        }
        if c == '[' || c == ']' {
            let b_start = p.l.b;
            advance(&mut p.l);
            parts.push(mk(p, "word", b_start, p.l.b, vec![]));
            continue;
        }
        // Bare word fragment
        if let Some(frag) = parse_bare_word(p) {
            parts.push(frag);
        } else {
            break;
        }
    }
    if parts.is_empty() { return None; }
    if parts.len() == 1 { return Some(parts.remove(0)); }
    let start = parts[0].start_index;
    let end = parts.last().unwrap().end_index;
    Some(mk(p, "concatenation", start, end, parts))
}

fn parse_bare_word(p: &mut PState) -> Option<TsNode> {
    let start = p.l.b;
    let start_i = p.l.i;
    while p.l.i < p.l.len {
        let c = peek_char(&p.l);
        if c == '\\' {
            if p.l.i + 1 >= p.l.len { break; }
            let nx = p.l.src[p.l.i + 1];
            if nx == '\n' || (nx == '\r' && p.l.i + 2 < p.l.len && p.l.src[p.l.i + 2] == '\n') {
                break;
            }
            advance(&mut p.l); advance(&mut p.l);
            continue;
        }
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '|' | '&' | ';' | '(' | ')' |
            '<' | '>' | '"' | '\'' | '$' | '`' | '{' | '}' | '[' | ']') {
            break;
        }
        advance(&mut p.l);
    }
    if p.l.b == start { return None; }
    let text: String = p.l.src[start_i..p.l.i].iter().collect();
    let re = Regex::new(r"^-?\d+$").unwrap();
    let node_type = if re.is_match(&text) { "number" } else { "word" };
    Some(mk(p, node_type, start, p.l.b, vec![]))
}

fn try_parse_brace_expr(p: &mut PState) -> Option<TsNode> {
    let save = save_lex(&p.l);
    if peek_char(&p.l) != '{' { return None; }
    let o_start = p.l.b;
    advance(&mut p.l);
    let o_end = p.l.b;
    let p1_start = p.l.b;
    while is_digit(peek_char(&p.l)) || is_ident_start(peek_char(&p.l)) { advance(&mut p.l); }
    let p1_end = p.l.b;
    if p1_end == p1_start || peek_char(&p.l) != '.' || peek(&p.l, 1) != '.' {
        restore_lex(&mut p.l, save);
        return None;
    }
    let dot_start = p.l.b;
    advance(&mut p.l); advance(&mut p.l);
    let dot_end = p.l.b;
    let p2_start = p.l.b;
    while is_digit(peek_char(&p.l)) || is_ident_start(peek_char(&p.l)) { advance(&mut p.l); }
    let p2_end = p.l.b;
    if p2_end == p2_start || peek_char(&p.l) != '}' {
        restore_lex(&mut p.l, save);
        return None;
    }
    let c_start = p.l.b;
    advance(&mut p.l);
    let c_end = p.l.b;
    let p1_text = slice_bytes(p, p1_start, p1_end);
    let p2_text = slice_bytes(p, p2_start, p2_end);
    let p1_is_num = p1_text.chars().all(|c| c.is_ascii_digit());
    let p2_is_num = p2_text.chars().all(|c| c.is_ascii_digit());
    if p1_is_num != p2_is_num {
        restore_lex(&mut p.l, save);
        return None;
    }
    if !p1_is_num && (p1_text.len() != 1 || p2_text.len() != 1) {
        restore_lex(&mut p.l, save);
        return None;
    }
    let p1_type = if p1_is_num { "number" } else { "word" };
    let p2_type = if p2_is_num { "number" } else { "word" };
    let n_open = mk(p, "{", o_start, o_end, vec![]);
    let n_p1 = mk(p, p1_type, p1_start, p1_end, vec![]);
    let n_dot = mk(p, "..", dot_start, dot_end, vec![]);
    let n_p2 = mk(p, p2_type, p2_start, p2_end, vec![]);
    let n_close = mk(p, "}", c_start, c_end, vec![]);
    Some(mk(p, "brace_expression", o_start, c_end, vec![
        n_open, n_p1, n_dot, n_p2, n_close,
    ]))
}

fn try_parse_brace_like_cat(p: &mut PState) -> Option<Vec<TsNode>> {
    if peek_char(&p.l) != '{' { return None; }
    let o_start = p.l.b;
    advance(&mut p.l);
    let mut inner = vec![mk(p, "word", o_start, p.l.b, vec![])];
    while p.l.i < p.l.len {
        let bc = peek_char(&p.l);
        if matches!(bc, '}' | '\n' | ';' | '|' | '&' | ' ' | '\t' | '<' | '>' | '(' | ')') { break; }
        if bc == '[' || bc == ']' {
            let b_start = p.l.b;
            advance(&mut p.l);
            inner.push(mk(p, "word", b_start, p.l.b, vec![]));
            continue;
        }
        let mid_start = p.l.b;
        let mid_start_i = p.l.i;
        while p.l.i < p.l.len {
            let mc = peek_char(&p.l);
            if matches!(mc, '}' | '\n' | ';' | '|' | '&' | ' ' | '\t' | '<' | '>' | '(' | ')' | '[' | ']') { break; }
            advance(&mut p.l);
        }
        if p.l.b > mid_start {
            let text: String = p.l.src[mid_start_i..p.l.i].iter().collect();
            let re = Regex::new(r"^-?\d+$").unwrap();
            let mid_type = if re.is_match(&text) { "number" } else { "word" };
            inner.push(mk(p, mid_type, mid_start, p.l.b, vec![]));
        } else {
            break;
        }
    }
    if peek_char(&p.l) == '}' {
        let c_start = p.l.b;
        advance(&mut p.l);
        inner.push(mk(p, "word", c_start, p.l.b, vec![]));
    }
    Some(inner)
}

// ─── Double Quoted String ───

pub fn parse_double_quoted(p: &mut PState) -> TsNode {
    let q_start = p.l.b;
    advance(&mut p.l);
    let q_end = p.l.b;
    let open_q = mk(p, "\"", q_start, q_end, vec![]);
    let mut parts = vec![open_q];
    let mut content_start = p.l.b;
    let mut content_start_i = p.l.i;
    let flush_content = |p: &mut PState, parts: &mut Vec<TsNode>, cs: usize, csi: usize| {
        if p.l.b > cs {
            let txt: String = p.l.src[csi..p.l.i].iter().collect();
            let re = Regex::new(r"^[ \t]+$").unwrap();
            if !re.is_match(&txt) {
                parts.push(mk(p, "string_content", cs, p.l.b, vec![]));
            }
        }
    };
    while p.l.i < p.l.len {
        let c = peek_char(&p.l);
        if c == '"' { break; }
        if c == '\\' && p.l.i + 1 < p.l.len {
            advance(&mut p.l); advance(&mut p.l);
            continue;
        }
        if c == '\n' {
            flush_content(p, &mut parts, content_start, content_start_i);
            advance(&mut p.l);
            content_start = p.l.b;
            content_start_i = p.l.i;
            continue;
        }
        if c == '$' {
            let c1 = peek(&p.l, 1);
            if c1 == '(' || c1 == '{' || is_ident_start(c1) || SPECIAL_VARS.contains(&c1) || is_digit(c1) {
                flush_content(p, &mut parts, content_start, content_start_i);
                if let Some(exp) = parse_dollar_like(p) {
                    parts.push(exp);
                }
                content_start = p.l.b;
                content_start_i = p.l.i;
                continue;
            }
            if c1 != '"' && c1 != '\0' {
                flush_content(p, &mut parts, content_start, content_start_i);
                let d_s = p.l.b;
                advance(&mut p.l);
                parts.push(mk(p, "$", d_s, p.l.b, vec![]));
                content_start = p.l.b;
                content_start_i = p.l.i;
                continue;
            }
        }
        if c == '`' {
            flush_content(p, &mut parts, content_start, content_start_i);
            if let Some(bt) = parse_backtick(p) {
                parts.push(bt);
            }
            content_start = p.l.b;
            content_start_i = p.l.i;
            continue;
        }
        advance(&mut p.l);
    }
    flush_content(p, &mut parts, content_start, content_start_i);
    let close = if peek_char(&p.l) == '"' {
        let c_start = p.l.b;
        advance(&mut p.l);
        mk(p, "\"", c_start, p.l.b, vec![])
    } else {
        mk(p, "\"", p.l.b, p.l.b, vec![])
    };
    let end = close.end_index;
    parts.push(close);
    mk(p, "string", q_start, end, parts)
}

// ─── Dollar-like expansions ───

pub fn parse_dollar_like(p: &mut PState) -> Option<TsNode> {
    let c1 = peek(&p.l, 1);
    let d_start = p.l.b;
    if c1 == '(' && peek(&p.l, 2) == '(' {
        // $((arithmetic))
        advance(&mut p.l); advance(&mut p.l); advance(&mut p.l);
        let open = mk(p, "$((", d_start, p.l.b, vec![]);
        let exprs = parse_arith_comma_list(p, "))", ArithMode::Var);
        skip_blanks(&mut p.l);
        let close = if peek_char(&p.l) == ')' && peek(&p.l, 1) == ')' {
            let cs = p.l.b;
            advance(&mut p.l); advance(&mut p.l);
            mk(p, "))", cs, p.l.b, vec![])
        } else {
            mk(p, "))", p.l.b, p.l.b, vec![])
        };
        let mut kids = vec![open];
        kids.extend(exprs);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "arithmetic_expansion", d_start, end, kids));
    }
    if c1 == '[' {
        // $[arithmetic] legacy
        advance(&mut p.l); advance(&mut p.l);
        let open = mk(p, "$[", d_start, p.l.b, vec![]);
        let exprs = parse_arith_comma_list(p, "]", ArithMode::Var);
        skip_blanks(&mut p.l);
        let close = if peek_char(&p.l) == ']' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, "]", cs, p.l.b, vec![])
        } else {
            mk(p, "]", p.l.b, p.l.b, vec![])
        };
        let mut kids = vec![open];
        kids.extend(exprs);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "arithmetic_expansion", d_start, end, kids));
    }
    if c1 == '(' {
        // $(command)
        advance(&mut p.l); advance(&mut p.l);
        let open = mk(p, "$(", d_start, p.l.b, vec![]);
        let mut body = crate::bash::parser_stmts::parse_statements(p, Some(")"));
        skip_blanks(&mut p.l);
        let close = if peek_char(&p.l) == ')' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, ")", cs, p.l.b, vec![])
        } else {
            mk(p, ")", p.l.b, p.l.b, vec![])
        };
        // $(< file) shorthand
        if body.len() == 1
            && body[0].node_type == "redirected_statement"
            && body[0].children.len() == 1
            && body[0].children[0].node_type == "file_redirect"
        {
            body = body.remove(0).children;
        }
        let mut kids = vec![open];
        kids.extend(body);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "command_substitution", d_start, end, kids));
    }
    if c1 == '{' {
        // ${expansion}
        advance(&mut p.l); advance(&mut p.l);
        let open = mk(p, "${", d_start, p.l.b, vec![]);
        let inner = parse_expansion_body(p);
        let close = if peek_char(&p.l) == '}' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, "}", cs, p.l.b, vec![])
        } else {
            mk(p, "}", p.l.b, p.l.b, vec![])
        };
        let mut kids = vec![open];
        kids.extend(inner);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "expansion", d_start, end, kids));
    }
    // Simple expansion $VAR or $? $$ etc
    advance(&mut p.l);
    let d_end = p.l.b;
    let dollar = mk(p, "$", d_start, d_end, vec![]);
    let nc = peek_char(&p.l);
    if nc == '_' && !is_ident_char(peek(&p.l, 1)) {
        let v_start = p.l.b;
        advance(&mut p.l);
        let vn = mk(p, "special_variable_name", v_start, p.l.b, vec![]);
        return Some(mk(p, "simple_expansion", d_start, p.l.b, vec![dollar, vn]));
    }
    if is_ident_start(nc) {
        let v_start = p.l.b;
        while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
        let vn = mk(p, "variable_name", v_start, p.l.b, vec![]);
        return Some(mk(p, "simple_expansion", d_start, p.l.b, vec![dollar, vn]));
    }
    if is_digit(nc) {
        let v_start = p.l.b;
        advance(&mut p.l);
        let vn = mk(p, "variable_name", v_start, p.l.b, vec![]);
        return Some(mk(p, "simple_expansion", d_start, p.l.b, vec![dollar, vn]));
    }
    if SPECIAL_VARS.contains(&nc) {
        let v_start = p.l.b;
        advance(&mut p.l);
        let vn = mk(p, "special_variable_name", v_start, p.l.b, vec![]);
        return Some(mk(p, "simple_expansion", d_start, p.l.b, vec![dollar, vn]));
    }
    Some(dollar)
}

fn parse_expansion_body(p: &mut PState) -> Vec<TsNode> {
    let mut out = Vec::new();
    skip_blanks(&mut p.l);
    // Optional # prefix for length
    if peek_char(&p.l) == '#' {
        let s = p.l.b;
        advance(&mut p.l);
        out.push(mk(p, "#", s, p.l.b, vec![]));
    }
    // Optional ! = ~ prefix
    let pc = peek_char(&p.l);
    if (pc == '!' || pc == '=' || pc == '~') && (is_ident_start(peek(&p.l, 1)) || is_digit(peek(&p.l, 1))) {
        let s = p.l.b;
        advance(&mut p.l);
        out.push(mk(p, &pc.to_string(), s, p.l.b, vec![]));
    }
    skip_blanks(&mut p.l);
    // Variable name
    if is_ident_start(peek_char(&p.l)) {
        let s = p.l.b;
        while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
        out.push(mk(p, "variable_name", s, p.l.b, vec![]));
    } else if is_digit(peek_char(&p.l)) {
        let s = p.l.b;
        while is_digit(peek_char(&p.l)) { advance(&mut p.l); }
        out.push(mk(p, "variable_name", s, p.l.b, vec![]));
    } else if SPECIAL_VARS.contains(&peek_char(&p.l)) {
        let s = p.l.b;
        advance(&mut p.l);
        out.push(mk(p, "special_variable_name", s, p.l.b, vec![]));
    }
    // Optional subscript [idx]
    if peek_char(&p.l) == '[' {
        let br_open = p.l.b;
        advance(&mut p.l);
        let br_open_node = mk(p, "[", br_open, p.l.b, vec![]);
        // Parse subscript content inline
        let mut depth = 1;
        let idx_start = p.l.b;
        while p.l.i < p.l.len && depth > 0 {
            let c = peek_char(&p.l);
            if c == '[' { depth += 1; }
            else if c == ']' { depth -= 1; if depth == 0 { break; } }
            advance(&mut p.l);
        }
        let idx_end = p.l.b;
        let br_close = p.l.b;
        if peek_char(&p.l) == ']' { advance(&mut p.l); }
        let br_close_node = mk(p, "]", br_close, p.l.b, vec![]);
        if let Some(var_node) = out.last().cloned() {
            out.pop();
            let mut sub_kids = vec![var_node.clone(), br_open_node];
            if idx_end > idx_start {
                sub_kids.push(mk(p, "word", idx_start, idx_end, vec![]));
            }
            sub_kids.push(br_close_node);
            out.push(mk(p, "subscript", var_node.start_index, p.l.b, sub_kids));
        }
    }
    skip_blanks(&mut p.l);
    // Trailing * or @ for indirect expansion
    let tc = peek_char(&p.l);
    if (tc == '*' || tc == '@') && peek(&p.l, 1) == '}' {
        let s = p.l.b;
        advance(&mut p.l);
        out.push(mk(p, &tc.to_string(), s, p.l.b, vec![]));
        return out;
    }
    // Operator handling (simplified)
    let c = peek_char(&p.l);
    if matches!(c, ':' | '#' | '%' | '/' | '^' | ',' | '-' | '=' | '?' | '+') {
        let s = p.l.b;
        let c1 = peek(&p.l, 1);
        let mut op = c.to_string();
        if c == ':' && matches!(c1, '-' | '=' | '?' | '+') {
            advance(&mut p.l); advance(&mut p.l);
            op = format!("{}{}", c, c1);
        } else if matches!(c, '#' | '%' | '/' | '^' | ',') && c1 == c {
            advance(&mut p.l); advance(&mut p.l);
            op = format!("{}{}", c, c);
        } else {
            advance(&mut p.l);
        }
        out.push(mk(p, &op, s, p.l.b, vec![]));
        // Rest: consume until } or newline
        let rest_start = p.l.b;
        let mut brace_depth = 0i32;
        while p.l.i < p.l.len {
            let rc = peek_char(&p.l);
            if rc == '\n' { break; }
            if brace_depth == 0 && rc == '}' { break; }
            if rc == '\\' && p.l.i + 1 < p.l.len {
                advance(&mut p.l); advance(&mut p.l);
                continue;
            }
            if rc == '$' && peek(&p.l, 1) == '{' {
                brace_depth += 1;
                advance(&mut p.l); advance(&mut p.l);
                continue;
            }
            if rc == '{' { brace_depth += 1; }
            else if rc == '}' && brace_depth > 0 { brace_depth -= 1; }
            advance(&mut p.l);
        }
        if p.l.b > rest_start {
            let node_type = if matches!(op.as_str(), "#" | "##" | "%" | "%%" | "/" | "//") {
                "regex"
            } else {
                "word"
            };
            out.push(mk(p, node_type, rest_start, p.l.b, vec![]));
        }
    }
    out
}

// ─── Backtick ───

pub fn parse_backtick(p: &mut PState) -> Option<TsNode> {
    let start = p.l.b;
    advance(&mut p.l);
    let open = mk(p, "`", start, p.l.b, vec![]);
    p.in_backtick += 1;
    let mut body: Vec<TsNode> = Vec::new();
    loop {
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == '`' || peek_char(&p.l) == '\0' { break; }
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Eof || t.token_type == TokenType::Backtick {
            restore_lex(&mut p.l, save);
            break;
        }
        if t.token_type == TokenType::Newline { continue; }
        restore_lex(&mut p.l, save);
        if let Some(stmt) = crate::bash::parser_stmts::parse_and_or(p) {
            body.push(stmt);
        } else {
            break;
        }
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == '`' { break; }
        let save2 = save_lex(&p.l);
        let sep = next_token(&mut p.l, LexCtx::Cmd);
        if sep.token_type == TokenType::Op && (sep.value == ";" || sep.value == "&") {
            body.push(leaf(p, &sep.value, &sep));
        } else if sep.token_type != TokenType::Newline {
            restore_lex(&mut p.l, save2);
        }
    }
    p.in_backtick -= 1;
    let close = if peek_char(&p.l) == '`' {
        let cs = p.l.b;
        advance(&mut p.l);
        mk(p, "`", cs, p.l.b, vec![])
    } else {
        mk(p, "`", p.l.b, p.l.b, vec![])
    };
    if body.is_empty() { return None; }
    let mut kids = vec![open];
    kids.extend(body);
    let end = close.end_index;
    kids.push(close);
    Some(mk(p, "command_substitution", start, end, kids))
}

// ─── Arithmetic Expressions ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithMode { Var, Word, Assign }

pub fn parse_arith_comma_list(p: &mut PState, stop: &str, mode: ArithMode) -> Vec<TsNode> {
    let mut out = Vec::new();
    loop {
        if let Some(e) = parse_arith_ternary(p, stop, mode) {
            out.push(e);
        }
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == ',' && !is_arith_stop(p, stop) {
            advance(&mut p.l);
            continue;
        }
        break;
    }
    out
}

fn parse_arith_ternary(p: &mut PState, stop: &str, mode: ArithMode) -> Option<TsNode> {
    let cond = parse_arith_binary(p, stop, 0, mode)?;
    skip_blanks(&mut p.l);
    if peek_char(&p.l) == '?' {
        let qs = p.l.b;
        advance(&mut p.l);
        let q = mk(p, "?", qs, p.l.b, vec![]);
        let t_expr = parse_arith_binary(p, ":", 0, mode);
        skip_blanks(&mut p.l);
        let colon = if peek_char(&p.l) == ':' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, ":", cs, p.l.b, vec![])
        } else {
            mk(p, ":", p.l.b, p.l.b, vec![])
        };
        let f_expr = parse_arith_ternary(p, stop, mode);
        let last = f_expr.as_ref().unwrap_or(&colon);
        let end = last.end_index;
        let mut kids = vec![cond, q];
        if let Some(t) = t_expr { kids.push(t); }
        kids.push(colon);
        if let Some(f) = f_expr { kids.push(f); }
        return Some(mk(p, "ternary_expression", kids[0].start_index, end, kids));
    }
    Some(cond)
}

fn parse_arith_binary(p: &mut PState, stop: &str, min_prec: u8, mode: ArithMode) -> Option<TsNode> {
    let mut left = parse_arith_unary(p, stop, mode)?;
    loop {
        skip_blanks(&mut p.l);
        if is_arith_stop(p, stop) { break; }
        if peek_char(&p.l) == ',' { break; }
        let op_info = scan_arith_op(p);
        if op_info.is_none() { break; }
        let (op_text, op_len) = op_info.unwrap();
        let prec = arith_prec(&op_text);
        if prec < min_prec { break; }
        let os = p.l.b;
        for _ in 0..op_len { advance(&mut p.l); }
        let op = mk(p, &op_text, os, p.l.b, vec![]);
        let next_min = if is_right_assoc(&op_text) { prec } else { prec + 1 };
        let right = parse_arith_binary(p, stop, next_min, mode);
        if right.is_none() { break; }
        let right = right.unwrap();
        let end = right.end_index;
        left = mk(p, "binary_expression", left.start_index, end, vec![left, op, right]);
    }
    Some(left)
}

fn parse_arith_unary(p: &mut PState, stop: &str, mode: ArithMode) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    if is_arith_stop(p, stop) { return None; }
    let c = peek_char(&p.l);
    let c1 = peek(&p.l, 1);
    if (c == '+' && c1 == '+') || (c == '-' && c1 == '-') {
        let s = p.l.b;
        advance(&mut p.l); advance(&mut p.l);
        let op = mk(p, &format!("{}{}", c, c1), s, p.l.b, vec![]);
        let inner = parse_arith_unary(p, stop, mode);
        if inner.is_none() { return Some(op); }
        let inner = inner.unwrap();
        let end = inner.end_index;
        return Some(mk(p, "unary_expression", op.start_index, end, vec![op, inner]));
    }
    if matches!(c, '-' | '+' | '!' | '~') {
        if mode != ArithMode::Var && c == '-' && is_digit(c1) {
            let s = p.l.b;
            advance(&mut p.l);
            while is_digit(peek_char(&p.l)) { advance(&mut p.l); }
            return Some(mk(p, "number", s, p.l.b, vec![]));
        }
        let s = p.l.b;
        advance(&mut p.l);
        let op = mk(p, &c.to_string(), s, p.l.b, vec![]);
        let inner = parse_arith_unary(p, stop, mode);
        if inner.is_none() { return Some(op); }
        let inner = inner.unwrap();
        let end = inner.end_index;
        return Some(mk(p, "unary_expression", op.start_index, end, vec![op, inner]));
    }
    parse_arith_postfix(p, stop, mode)
}

fn parse_arith_postfix(p: &mut PState, stop: &str, mode: ArithMode) -> Option<TsNode> {
    let prim = parse_arith_primary(p, stop, mode)?;
    let c = peek_char(&p.l);
    let c1 = peek(&p.l, 1);
    if (c == '+' && c1 == '+') || (c == '-' && c1 == '-') {
        let s = p.l.b;
        advance(&mut p.l); advance(&mut p.l);
        let op = mk(p, &format!("{}{}", c, c1), s, p.l.b, vec![]);
        let end = op.end_index;
        return Some(mk(p, "postfix_expression", prim.start_index, end, vec![prim, op]));
    }
    Some(prim)
}

fn parse_arith_primary(p: &mut PState, stop: &str, mode: ArithMode) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    if is_arith_stop(p, stop) { return None; }
    let c = peek_char(&p.l);
    if c == '(' {
        let s = p.l.b;
        advance(&mut p.l);
        let open = mk(p, "(", s, p.l.b, vec![]);
        let inners = parse_arith_comma_list(p, ")", mode);
        skip_blanks(&mut p.l);
        let close = if peek_char(&p.l) == ')' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, ")", cs, p.l.b, vec![])
        } else {
            mk(p, ")", p.l.b, p.l.b, vec![])
        };
        let mut kids = vec![open];
        kids.extend(inners);
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "parenthesized_expression", kids[0].start_index, end, kids));
    }
    if c == '"' {
        return Some(parse_double_quoted(p));
    }
    if c == '$' {
        return parse_dollar_like(p);
    }
    if is_digit(c) {
        let s = p.l.b;
        while is_digit(peek_char(&p.l)) { advance(&mut p.l); }
        if p.l.b - s == 1 && c == '0' && (peek_char(&p.l) == 'x' || peek_char(&p.l) == 'X') {
            advance(&mut p.l);
            while is_hex_digit(peek_char(&p.l)) { advance(&mut p.l); }
        } else if peek_char(&p.l) == '#' {
            advance(&mut p.l);
            while is_base_digit(peek_char(&p.l)) { advance(&mut p.l); }
        }
        return Some(mk(p, "number", s, p.l.b, vec![]));
    }
    if is_ident_start(c) {
        let s = p.l.b;
        while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
        // Subscript
        if peek_char(&p.l) == '[' {
            let vn = mk(p, "variable_name", s, p.l.b, vec![]);
            let br_s = p.l.b;
            advance(&mut p.l);
            let br_open = mk(p, "[", br_s, p.l.b, vec![]);
            let idx = parse_arith_ternary(p, "]", ArithMode::Var);
            skip_blanks(&mut p.l);
            let br_close = if peek_char(&p.l) == ']' {
                let cs = p.l.b;
                advance(&mut p.l);
                mk(p, "]", cs, p.l.b, vec![])
            } else {
                mk(p, "]", p.l.b, p.l.b, vec![])
            };
            let mut kids = vec![vn, br_open];
            if let Some(i) = idx { kids.push(i); }
            let end = br_close.end_index;
            kids.push(br_close);
            return Some(mk(p, "subscript", s, end, kids));
        }
        let ident_type = if mode == ArithMode::Var { "variable_name" } else { "word" };
        return Some(mk(p, ident_type, s, p.l.b, vec![]));
    }
    None
}

fn scan_arith_op(p: &PState) -> Option<(String, usize)> {
    let c = peek_char(&p.l);
    let c1 = peek(&p.l, 1);
    let c2 = peek(&p.l, 2);
    // 3-char
    if c == '<' && c1 == '<' && c2 == '=' { return Some(("<<=".to_string(), 3)); }
    if c == '>' && c1 == '>' && c2 == '=' { return Some((">>=".to_string(), 3)); }
    // 2-char
    if c == '*' && c1 == '*' { return Some(("**".to_string(), 2)); }
    if c == '<' && c1 == '<' { return Some(("<<".to_string(), 2)); }
    if c == '>' && c1 == '>' { return Some((">>".to_string(), 2)); }
    if c == '=' && c1 == '=' { return Some(("==".to_string(), 2)); }
    if c == '!' && c1 == '=' { return Some(("!=".to_string(), 2)); }
    if c == '<' && c1 == '=' { return Some(("<=".to_string(), 2)); }
    if c == '>' && c1 == '=' { return Some((">=".to_string(), 2)); }
    if c == '&' && c1 == '&' { return Some(("&&".to_string(), 2)); }
    if c == '|' && c1 == '|' { return Some(("||".to_string(), 2)); }
    if c == '+' && c1 == '=' { return Some(("+=".to_string(), 2)); }
    if c == '-' && c1 == '=' { return Some(("-=".to_string(), 2)); }
    if c == '*' && c1 == '=' { return Some(("*=".to_string(), 2)); }
    if c == '/' && c1 == '=' { return Some(("/=".to_string(), 2)); }
    if c == '%' && c1 == '=' { return Some(("%=".to_string(), 2)); }
    if c == '&' && c1 == '=' { return Some(("&=".to_string(), 2)); }
    if c == '^' && c1 == '=' { return Some(("^=".to_string(), 2)); }
    if c == '|' && c1 == '=' { return Some(("|=".to_string(), 2)); }
    // 1-char
    if c == '+' && c1 != '+' { return Some(("+".to_string(), 1)); }
    if c == '-' && c1 != '-' { return Some(("-".to_string(), 1)); }
    if c == '*' { return Some(("*".to_string(), 1)); }
    if c == '/' { return Some(("/".to_string(), 1)); }
    if c == '%' { return Some(("%".to_string(), 1)); }
    if c == '<' { return Some(("<".to_string(), 1)); }
    if c == '>' { return Some((">".to_string(), 1)); }
    if c == '&' { return Some(("&".to_string(), 1)); }
    if c == '|' { return Some(("|".to_string(), 1)); }
    if c == '^' { return Some(("^".to_string(), 1)); }
    if c == '=' { return Some(("=".to_string(), 1)); }
    None
}

fn arith_prec(op: &str) -> u8 {
    match op {
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|=" => 2,
        "||" => 4,
        "&&" => 5,
        "|" => 6,
        "^" => 7,
        "&" => 8,
        "==" | "!=" => 9,
        "<" | ">" | "<=" | ">=" => 10,
        "<<" | ">>" => 11,
        "+" | "-" => 12,
        "*" | "/" | "%" => 13,
        "**" => 14,
        _ => 0,
    }
}

fn is_right_assoc(op: &str) -> bool {
    matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|=" | "**")
}

fn is_arith_stop(p: &PState, stop: &str) -> bool {
    let c = peek_char(&p.l);
    match stop {
        "))" => c == ')' && peek(&p.l, 1) == ')',
        ")" => c == ')',
        ";" => c == ';',
        ":" => c == ':',
        "]" => c == ']',
        "}" => c == '}',
        ":}" => c == ':' || c == '}',
        _ => c == '\0' || c == '\n',
    }
}

// ─── Test Expressions ───

pub fn parse_test_expr(p: &mut PState, closer: &str) -> Option<TsNode> {
    parse_test_or(p, closer)
}

fn parse_test_or(p: &mut PState, closer: &str) -> Option<TsNode> {
    let mut left = parse_test_and(p, closer)?;
    loop {
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == '|' && peek(&p.l, 1) == '|' {
            let save = save_lex(&p.l);
            let s = p.l.b;
            advance(&mut p.l); advance(&mut p.l);
            let op = mk(p, "||", s, p.l.b, vec![]);
            let right = parse_test_and(p, closer);
            if right.is_none() {
                restore_lex(&mut p.l, save);
                break;
            }
            let right = right.unwrap();
            let end = right.end_index;
            left = mk(p, "binary_expression", left.start_index, end, vec![left, op, right]);
        } else {
            break;
        }
    }
    Some(left)
}

fn parse_test_and(p: &mut PState, closer: &str) -> Option<TsNode> {
    let mut left = parse_test_unary(p, closer)?;
    loop {
        skip_blanks(&mut p.l);
        if peek_char(&p.l) == '&' && peek(&p.l, 1) == '&' {
            let s = p.l.b;
            advance(&mut p.l); advance(&mut p.l);
            let op = mk(p, "&&", s, p.l.b, vec![]);
            let right = parse_test_unary(p, closer);
            if right.is_none() { break; }
            let right = right.unwrap();
            let end = right.end_index;
            left = mk(p, "binary_expression", left.start_index, end, vec![left, op, right]);
        } else {
            break;
        }
    }
    Some(left)
}

fn parse_test_unary(p: &mut PState, closer: &str) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let c = peek_char(&p.l);
    if c == '(' {
        let s = p.l.b;
        advance(&mut p.l);
        let open = mk(p, "(", s, p.l.b, vec![]);
        let inner = parse_test_or(p, closer);
        skip_blanks(&mut p.l);
        let close = if peek_char(&p.l) == ')' {
            let cs = p.l.b;
            advance(&mut p.l);
            mk(p, ")", cs, p.l.b, vec![])
        } else {
            mk(p, ")", p.l.b, p.l.b, vec![])
        };
        let mut kids = vec![open];
        if let Some(i) = inner { kids.push(i); }
        let end = close.end_index;
        kids.push(close);
        return Some(mk(p, "parenthesized_expression", kids[0].start_index, end, kids));
    }
    parse_test_binary(p, closer)
}

fn parse_test_binary(p: &mut PState, closer: &str) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let left = parse_test_negatable_primary(p, closer)?;
    skip_blanks(&mut p.l);
    let c = peek_char(&p.l);
    let c1 = peek(&p.l, 1);
    let os = p.l.b;
    let op = if c == '=' && c1 == '=' {
        advance(&mut p.l); advance(&mut p.l);
        Some(mk(p, "==", os, p.l.b, vec![]))
    } else if c == '!' && c1 == '=' {
        advance(&mut p.l); advance(&mut p.l);
        Some(mk(p, "!=", os, p.l.b, vec![]))
    } else if c == '=' && c1 == '~' {
        advance(&mut p.l); advance(&mut p.l);
        Some(mk(p, "=~", os, p.l.b, vec![]))
    } else if c == '=' && c1 != '=' {
        advance(&mut p.l);
        Some(mk(p, "=", os, p.l.b, vec![]))
    } else if c == '<' && c1 != '<' {
        advance(&mut p.l);
        Some(mk(p, "<", os, p.l.b, vec![]))
    } else if c == '>' && c1 != '>' {
        advance(&mut p.l);
        Some(mk(p, ">", os, p.l.b, vec![]))
    } else if c == '-' && is_ident_start(c1) {
        advance(&mut p.l);
        while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
        Some(mk(p, "test_operator", os, p.l.b, vec![]))
    } else {
        None
    };
    if op.is_none() { return Some(left); }
    let op = op.unwrap();
    skip_blanks(&mut p.l);
    // Simplified RHS: parse as word
    let right = parse_test_primary(p, closer);
    if right.is_none() { return Some(left); }
    let right = right.unwrap();
    let end = right.end_index;
    Some(mk(p, "binary_expression", left.start_index, end, vec![left, op, right]))
}

fn parse_test_negatable_primary(p: &mut PState, closer: &str) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    let c = peek_char(&p.l);
    if c == '!' {
        let s = p.l.b;
        advance(&mut p.l);
        let bang = mk(p, "!", s, p.l.b, vec![]);
        let inner = parse_test_negatable_primary(p, closer);
        if inner.is_none() { return Some(bang); }
        let inner = inner.unwrap();
        let end = inner.end_index;
        return Some(mk(p, "unary_expression", bang.start_index, end, vec![bang, inner]));
    }
    if c == '-' && is_ident_start(peek(&p.l, 1)) {
        let s = p.l.b;
        advance(&mut p.l);
        while is_ident_char(peek_char(&p.l)) { advance(&mut p.l); }
        let op = mk(p, "test_operator", s, p.l.b, vec![]);
        skip_blanks(&mut p.l);
        let arg = parse_test_primary(p, closer);
        if arg.is_none() { return Some(op); }
        let arg = arg.unwrap();
        let end = arg.end_index;
        return Some(mk(p, "unary_expression", op.start_index, end, vec![op, arg]));
    }
    parse_test_primary(p, closer)
}

fn parse_test_primary(p: &mut PState, closer: &str) -> Option<TsNode> {
    skip_blanks(&mut p.l);
    if closer == "]" && peek_char(&p.l) == ']' { return None; }
    if closer == "]]" && peek_char(&p.l) == ']' && peek(&p.l, 1) == ']' { return None; }
    parse_word(p, "arg")
}

// Helper for subscript parsing
fn parse_subscript_index_inline(_p: &mut PState) -> Option<TsNode> {
    // Simplified: skip to ]
    None
}
