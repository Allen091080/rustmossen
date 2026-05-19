//! Tokenizer for the bash parser.
//!
//! Translated from `bashParser.ts` lines 48–591 (Tokenizer section).

use crate::bash::types::{HeredocPending, LexSave, Token, TokenType, SPECIAL_VARS};

/// Lexer state. Tracks both string char index and UTF-8 byte offset.
#[derive(Debug, Clone)]
pub struct Lexer {
    pub src: Vec<char>,
    pub len: usize,
    /// Char index
    pub i: usize,
    /// UTF-8 byte offset
    pub b: usize,
    /// Pending heredoc delimiters awaiting body scan
    pub heredocs: Vec<HeredocPending>,
    /// Precomputed byte offset for each char index (lazy)
    pub byte_table: Option<Vec<u32>>,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let len = chars.len();
        Self {
            src: chars,
            len,
            i: 0,
            b: 0,
            heredocs: Vec::new(),
            byte_table: None,
        }
    }
}

/// Advance one char, updating byte offset for UTF-8.
pub fn advance(l: &mut Lexer) {
    if l.i >= l.len {
        return;
    }
    let c = l.src[l.i];
    l.i += 1;
    let cp = c as u32;
    if cp < 0x80 {
        l.b += 1;
    } else if cp < 0x800 {
        l.b += 2;
    } else if cp >= 0xD800 && cp <= 0xDBFF {
        // High surrogate — skip low surrogate too
        l.b += 4;
        l.i += 1;
    } else {
        l.b += 3;
    }
}

pub fn peek(l: &Lexer, off: usize) -> char {
    if l.i + off < l.len {
        l.src[l.i + off]
    } else {
        '\0'
    }
}

pub fn peek_char(l: &Lexer) -> char {
    peek(l, 0)
}

pub fn byte_at(l: &mut Lexer, _char_idx: usize) -> usize {
    if l.byte_table.is_some() {
        return l.byte_table.as_ref().unwrap()[_char_idx] as usize;
    }
    // Build table
    let mut t = vec![0u32; l.len + 1];
    let mut b: u32 = 0;
    let mut i = 0;
    while i < l.len {
        t[i] = b;
        let cp = l.src[i] as u32;
        if cp < 0x80 {
            b += 1;
            i += 1;
        } else if cp < 0x800 {
            b += 2;
            i += 1;
        } else if cp >= 0xD800 && cp <= 0xDBFF {
            if i + 1 < l.len {
                t[i + 1] = b + 2;
            }
            b += 4;
            i += 2;
        } else {
            b += 3;
            i += 1;
        }
    }
    t[l.len] = b;
    l.byte_table = Some(t);
    l.byte_table.as_ref().unwrap()[_char_idx] as usize
}

pub fn is_word_char(c: char) -> bool {
    matches!(c,
        'a'..='z' | 'A'..='Z' | '0'..='9' |
        '_' | '/' | '.' | '-' | '+' | ':' | '@' | '%' | ',' |
        '~' | '^' | '?' | '*' | '!' | '=' | '[' | ']'
    )
}

pub fn is_word_start(c: char) -> bool {
    is_word_char(c) || c == '\\'
}

pub fn is_ident_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_')
}

pub fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || matches!(c, '0'..='9')
}

pub fn is_digit(c: char) -> bool {
    matches!(c, '0'..='9')
}

pub fn is_hex_digit(c: char) -> bool {
    is_digit(c) || matches!(c, 'a'..='f' | 'A'..='F')
}

pub fn is_base_digit(c: char) -> bool {
    is_ident_char(c) || c == '@'
}

pub fn is_heredoc_delim_char(c: char) -> bool {
    c != '\0'
        && c != ' '
        && c != '\t'
        && c != '\n'
        && c != '<'
        && c != '>'
        && c != '|'
        && c != '&'
        && c != ';'
        && c != '('
        && c != ')'
        && c != '\''
        && c != '"'
        && c != '`'
        && c != '\\'
}

pub fn skip_blanks(l: &mut Lexer) {
    while l.i < l.len {
        let c = l.src[l.i];
        if c == ' ' || c == '\t' || c == '\r' {
            advance(l);
        } else if c == '\\' {
            let nx = if l.i + 1 < l.len { l.src[l.i + 1] } else { '\0' };
            if nx == '\n' || (nx == '\r' && l.i + 2 < l.len && l.src[l.i + 2] == '\n') {
                advance(l);
                advance(l);
                if nx == '\r' {
                    advance(l);
                }
            } else if nx == ' ' || nx == '\t' {
                advance(l);
                advance(l);
            } else {
                break;
            }
        } else {
            break;
        }
    }
}

/// Context for token scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexCtx {
    Cmd,
    Arg,
}

/// Scan next token. Context-sensitive.
pub fn next_token(l: &mut Lexer, ctx: LexCtx) -> Token {
    skip_blanks(l);
    let start = l.b;
    if l.i >= l.len {
        return Token::new(TokenType::Eof, "", start, start);
    }

    let c = l.src[l.i];
    let c1 = peek(l, 1);
    let c2 = peek(l, 2);

    if c == '\n' {
        advance(l);
        return Token::new(TokenType::Newline, "\n", start, l.b);
    }

    if c == '#' {
        let si = l.i;
        while l.i < l.len && l.src[l.i] != '\n' {
            advance(l);
        }
        let value: String = l.src[si..l.i].iter().collect();
        return Token::new(TokenType::Comment, value, start, l.b);
    }

    // Multi-char operators (longest match first)
    if c == '&' && c1 == '&' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "&&", start, l.b);
    }
    if c == '|' && c1 == '|' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "||", start, l.b);
    }
    if c == '|' && c1 == '&' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "|&", start, l.b);
    }
    if c == ';' && c1 == ';' && c2 == '&' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, ";;&", start, l.b);
    }
    if c == ';' && c1 == ';' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, ";;", start, l.b);
    }
    if c == ';' && c1 == '&' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, ";&", start, l.b);
    }
    if c == '>' && c1 == '>' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, ">>", start, l.b);
    }
    if c == '>' && c1 == '&' && c2 == '-' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, ">&-", start, l.b);
    }
    if c == '>' && c1 == '&' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, ">&", start, l.b);
    }
    if c == '>' && c1 == '|' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, ">|", start, l.b);
    }
    if c == '&' && c1 == '>' && c2 == '>' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, "&>>", start, l.b);
    }
    if c == '&' && c1 == '>' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "&>", start, l.b);
    }
    if c == '<' && c1 == '<' && c2 == '<' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, "<<<", start, l.b);
    }
    if c == '<' && c1 == '<' && c2 == '-' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, "<<-", start, l.b);
    }
    if c == '<' && c1 == '<' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "<<", start, l.b);
    }
    if c == '<' && c1 == '&' && c2 == '-' {
        advance(l); advance(l); advance(l);
        return Token::new(TokenType::Op, "<&-", start, l.b);
    }
    if c == '<' && c1 == '&' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "<&", start, l.b);
    }
    if c == '<' && c1 == '(' {
        advance(l); advance(l);
        return Token::new(TokenType::LtParen, "<(", start, l.b);
    }
    if c == '>' && c1 == '(' {
        advance(l); advance(l);
        return Token::new(TokenType::GtParen, ">(", start, l.b);
    }
    if c == '(' && c1 == '(' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "((", start, l.b);
    }
    if c == ')' && c1 == ')' {
        advance(l); advance(l);
        return Token::new(TokenType::Op, "))", start, l.b);
    }

    if matches!(c, '|' | '&' | ';' | '>' | '<') {
        advance(l);
        return Token::new(TokenType::Op, c.to_string(), start, l.b);
    }
    if c == '(' || c == ')' {
        advance(l);
        return Token::new(TokenType::Op, c.to_string(), start, l.b);
    }

    // In cmd position, [ [[ { start test/group
    if ctx == LexCtx::Cmd {
        if c == '[' && c1 == '[' {
            advance(l); advance(l);
            return Token::new(TokenType::Op, "[[", start, l.b);
        }
        if c == '[' {
            advance(l);
            return Token::new(TokenType::Op, "[", start, l.b);
        }
        if c == '{' && (c1 == ' ' || c1 == '\t' || c1 == '\n') {
            advance(l);
            return Token::new(TokenType::Op, "{", start, l.b);
        }
        if c == '}' {
            advance(l);
            return Token::new(TokenType::Op, "}", start, l.b);
        }
        if c == '!' && (c1 == ' ' || c1 == '\t') {
            advance(l);
            return Token::new(TokenType::Op, "!", start, l.b);
        }
    }

    if c == '"' {
        advance(l);
        return Token::new(TokenType::DQuote, "\"", start, l.b);
    }
    if c == '\'' {
        let si = l.i;
        advance(l);
        while l.i < l.len && l.src[l.i] != '\'' {
            advance(l);
        }
        if l.i < l.len {
            advance(l);
        }
        let value: String = l.src[si..l.i].iter().collect();
        return Token::new(TokenType::SQuote, value, start, l.b);
    }

    if c == '$' {
        if c1 == '(' && c2 == '(' {
            advance(l); advance(l); advance(l);
            return Token::new(TokenType::DollarDParen, "$((", start, l.b);
        }
        if c1 == '(' {
            advance(l); advance(l);
            return Token::new(TokenType::DollarParen, "$(", start, l.b);
        }
        if c1 == '{' {
            advance(l); advance(l);
            return Token::new(TokenType::DollarBrace, "${", start, l.b);
        }
        if c1 == '\'' {
            let si = l.i;
            advance(l); advance(l);
            while l.i < l.len && l.src[l.i] != '\'' {
                if l.src[l.i] == '\\' && l.i + 1 < l.len {
                    advance(l);
                }
                advance(l);
            }
            if l.i < l.len {
                advance(l);
            }
            let value: String = l.src[si..l.i].iter().collect();
            return Token::new(TokenType::AnsiC, value, start, l.b);
        }
        advance(l);
        return Token::new(TokenType::Dollar, "$", start, l.b);
    }

    if c == '`' {
        advance(l);
        return Token::new(TokenType::Backtick, "`", start, l.b);
    }

    // File descriptor before redirect
    if is_digit(c) {
        let mut j = l.i;
        while j < l.len && is_digit(l.src[j]) {
            j += 1;
        }
        let after = if j < l.len { l.src[j] } else { '\0' };
        if after == '>' || after == '<' {
            let si = l.i;
            while l.i < j {
                advance(l);
            }
            let value: String = l.src[si..l.i].iter().collect();
            return Token::new(TokenType::Word, value, start, l.b);
        }
    }

    // Word / number
    if is_word_start(c) || c == '{' || c == '}' {
        let si = l.i;
        while l.i < l.len {
            let ch = l.src[l.i];
            if ch == '\\' {
                if l.i + 1 >= l.len {
                    break;
                }
                if l.src[l.i + 1] == '\n' {
                    advance(l);
                    advance(l);
                    continue;
                }
                advance(l);
                advance(l);
                continue;
            }
            if !is_word_char(ch) && ch != '{' && ch != '}' {
                break;
            }
            advance(l);
        }
        if l.i > si {
            let v: String = l.src[si..l.i].iter().collect();
            if is_number_str(&v) {
                return Token::new(TokenType::Number, v, start, l.b);
            }
            return Token::new(TokenType::Word, v, start, l.b);
        }
    }

    // Unknown char — consume as single-char word
    advance(l);
    return Token::new(TokenType::Word, c.to_string(), start, l.b);
}

fn is_number_str(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = if bytes[0] == b'-' { 1 } else { 0 };
    if start >= bytes.len() {
        return false;
    }
    bytes[start..].iter().all(|b| b.is_ascii_digit())
}

pub fn save_lex(l: &Lexer) -> LexSave {
    LexSave { i: l.i, b: l.b }
}

pub fn restore_lex(l: &mut Lexer, s: LexSave) {
    l.i = s.i;
    l.b = s.b;
}
