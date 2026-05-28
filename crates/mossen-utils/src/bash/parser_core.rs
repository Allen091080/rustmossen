//! Core parser infrastructure: ParseState, mk, sliceBytes, parseSource, parseProgram.
//!
//! Translated from `bashParser.ts` lines 593–752.

use crate::bash::lexer::*;
use crate::bash::types::*;

/// Parser state machine.
pub struct PState {
    pub l: Lexer,
    pub src: String,
    pub src_bytes: usize,
    pub is_ascii: bool,
    pub node_count: usize,
    pub deadline: std::time::Instant,
    pub aborted: bool,
    pub in_backtick: usize,
    pub stop_token: Option<String>,
}

pub fn parse_source(source: &str, timeout_ms: Option<u64>) -> Option<TsNode> {
    let l = Lexer::new(source);
    let src_bytes = byte_length_utf8(source);
    let timeout = timeout_ms.unwrap_or(PARSE_TIMEOUT_MS);
    let deadline = if timeout == u64::MAX {
        std::time::Instant::now() + std::time::Duration::from_secs(3600)
    } else {
        std::time::Instant::now() + std::time::Duration::from_millis(timeout)
    };
    let mut p = PState {
        l,
        src: source.to_string(),
        src_bytes,
        is_ascii: src_bytes == source.len(),
        node_count: 0,
        deadline,
        aborted: false,
        in_backtick: 0,
        stop_token: None,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse_program(&mut p)));
    match result {
        Ok(program) => {
            if p.aborted {
                None
            } else {
                Some(program)
            }
        }
        Err(_) => None,
    }
}

fn byte_length_utf8(s: &str) -> usize {
    s.len() // Rust strings are already UTF-8 bytes
}

pub fn check_budget(p: &mut PState) {
    p.node_count += 1;
    if p.node_count > MAX_NODES {
        p.aborted = true;
        panic!("budget exceeded");
    }
    if (p.node_count & 0x7f) == 0 && std::time::Instant::now() > p.deadline {
        p.aborted = true;
        panic!("timeout");
    }
}

/// Build a node. Slices text from source by byte range.
pub fn mk(
    p: &mut PState,
    node_type: &str,
    start: usize,
    end: usize,
    children: Vec<TsNode>,
) -> TsNode {
    check_budget(p);
    let text = slice_bytes(p, start, end);
    TsNode::new(node_type, text, start, end, children)
}

pub fn slice_bytes(p: &mut PState, start_byte: usize, end_byte: usize) -> String {
    if p.is_ascii {
        return p.src[start_byte..end_byte.min(p.src.len())].to_string();
    }
    // Non-ASCII: binary search in byte table
    if p.l.byte_table.is_none() {
        byte_at(&mut p.l, 0);
    }
    let t = p.l.byte_table.as_ref().unwrap();
    let sc = match t.binary_search(&(start_byte as u32)) {
        Ok(pos) => pos,
        Err(pos) => pos,
    };
    let ec = match t[sc..].binary_search(&(end_byte as u32)) {
        Ok(pos) => sc + pos,
        Err(pos) => sc + pos,
    };
    let chars: &[char] = &p.l.src[sc..ec.min(p.l.src.len())];
    chars.iter().collect()
}

pub fn leaf(p: &mut PState, node_type: &str, tok: &Token) -> TsNode {
    mk(p, node_type, tok.start, tok.end, vec![])
}

fn parse_program(p: &mut PState) -> TsNode {
    let children = &mut Vec::new();
    skip_blanks(&mut p.l);
    loop {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Newline {
            skip_blanks(&mut p.l);
            continue;
        }
        restore_lex(&mut p.l, save);
        break;
    }
    let prog_start = p.l.b;
    while p.l.i < p.l.len {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type == TokenType::Eof {
            break;
        }
        if t.token_type == TokenType::Newline {
            continue;
        }
        if t.token_type == TokenType::Comment {
            children.push(leaf(p, "comment", &t));
            continue;
        }
        restore_lex(&mut p.l, save);
        let stmts = crate::bash::parser_stmts::parse_statements(p, None);
        if stmts.is_empty() {
            let err_tok = next_token(&mut p.l, LexCtx::Cmd);
            if err_tok.token_type == TokenType::Eof {
                break;
            }
            if err_tok.token_type == TokenType::Op && err_tok.value == ";;" && !children.is_empty()
            {
                continue;
            }
            children.push(mk(p, "ERROR", err_tok.start, err_tok.end, vec![]));
        } else {
            for s in stmts {
                children.push(s);
            }
        }
    }
    let prog_end = if !children.is_empty() {
        p.src_bytes
    } else {
        prog_start
    };
    let kids = std::mem::take(children);
    mk(p, "program", prog_start, prog_end, kids)
}

pub fn skip_newlines(p: &mut PState) {
    loop {
        let save = save_lex(&p.l);
        let t = next_token(&mut p.l, LexCtx::Cmd);
        if t.token_type != TokenType::Newline {
            restore_lex(&mut p.l, save);
            break;
        }
    }
}
