//! # slice_ansi — ANSI 转义序列感知的字符串切片
//!
//! 对应 TypeScript `utils/sliceAnsi.ts`。

/// ANSI 代码类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiCode {
    pub code: String,
    pub end_code: String,
}

/// Token 类型
#[derive(Debug, Clone)]
pub enum Token {
    Ansi { code: String, ansi_code: AnsiCode },
    Text { value: String, full_width: bool },
}

/// 判断一个 ANSI code 是否是"结束码"
fn is_end_code(code: &AnsiCode) -> bool {
    code.code == code.end_code
}

/// 过滤仅保留"起始码"（非结束码）
fn filter_start_codes(codes: &[AnsiCode]) -> Vec<AnsiCode> {
    codes.iter().filter(|c| !is_end_code(c)).cloned().collect()
}

/// 归约 ANSI 代码（移除已被对应结束码取消的代码）
fn reduce_ansi_codes(codes: &[AnsiCode]) -> Vec<AnsiCode> {
    let mut active: Vec<AnsiCode> = Vec::new();
    for code in codes {
        if is_end_code(code) {
            // 结束码移除对应的起始码
            active.retain(|c| c.end_code != code.code);
        } else {
            active.push(code.clone());
        }
    }
    active
}

/// 将 ANSI 代码转为字符串
fn ansi_codes_to_string(codes: &[AnsiCode]) -> String {
    codes.iter().map(|c| c.code.as_str()).collect::<String>()
}

/// 生成撤销 ANSI 代码的字符串
fn undo_ansi_codes(codes: &[AnsiCode]) -> String {
    codes.iter().map(|c| c.end_code.as_str()).collect::<String>()
}

/// 计算字符串的显示宽度
fn string_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// 简单 ANSI tokenizer
fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    let mut text_start = 0;

    while let Some(&(i, ch)) = chars.peek() {
        if ch == '\x1b' {
            // 如果有之前的文本，先产出
            if i > text_start {
                let text = &input[text_start..i];
                tokens.push(Token::Text {
                    value: text.to_string(),
                    full_width: false,
                });
            }

            // 解析 ANSI 序列
            let start = i;
            chars.next(); // consume ESC

            if let Some(&(_, next_ch)) = chars.peek() {
                if next_ch == '[' {
                    // CSI sequence
                    chars.next();
                    while let Some(&(_, c)) = chars.peek() {
                        chars.next();
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                } else if next_ch == ']' {
                    // OSC sequence (e.g., hyperlinks)
                    chars.next();
                    while let Some(&(_, c)) = chars.peek() {
                        chars.next();
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            if let Some(&(_, '\\')) = chars.peek() {
                                chars.next();
                                break;
                            }
                        }
                    }
                } else {
                    chars.next();
                }
            }

            let end = chars.peek().map(|&(i, _)| i).unwrap_or(input.len());
            let code_str = &input[start..end];
            tokens.push(Token::Ansi {
                code: code_str.to_string(),
                ansi_code: AnsiCode {
                    code: code_str.to_string(),
                    end_code: "\x1b[0m".to_string(),
                },
            });
            text_start = end;
        } else {
            chars.next();
        }
    }

    // 剩余文本
    if text_start < input.len() {
        let text = &input[text_start..];
        tokens.push(Token::Text {
            value: text.to_string(),
            full_width: false,
        });
    }

    tokens
}

/// 切片包含 ANSI 转义码的字符串。
///
/// 与 slice-ansi 包不同，此实现正确处理 OSC 8 超链接序列，
/// 因为 tokenizer 正确解析它们。
pub fn slice_ansi(str: &str, start: usize, end: Option<usize>) -> String {
    let tokens = tokenize(str);
    let mut active_codes: Vec<AnsiCode> = Vec::new();
    let mut position: usize = 0;
    let mut result = String::new();
    let mut include = false;

    for token in &tokens {
        let width = match token {
            Token::Ansi { .. } => 0,
            Token::Text { value, full_width } => {
                if *full_width {
                    2
                } else {
                    string_width(value)
                }
            }
        };

        // 超过 end 边界时中断
        if let Some(end_val) = end {
            if position >= end_val {
                match token {
                    Token::Ansi { .. } => break,
                    Token::Text { .. } => {
                        if width > 0 || !include {
                            break;
                        }
                    }
                }
            }
        }

        match token {
            Token::Ansi { code, ansi_code } => {
                active_codes.push(ansi_code.clone());
                if include {
                    result.push_str(code);
                }
            }
            Token::Text { value, .. } => {
                if !include && position >= start {
                    // 跳过起始边界处的零宽标记
                    if start > 0 && width == 0 {
                        continue;
                    }
                    include = true;
                    // 归约并过滤仅保留活跃的起始码
                    let reduced = reduce_ansi_codes(&active_codes);
                    let start_codes = filter_start_codes(&reduced);
                    active_codes = start_codes.clone();
                    result = ansi_codes_to_string(&start_codes);
                }

                if include {
                    result.push_str(value);
                }

                position += width;
            }
        }
    }

    // 仅撤销仍然活跃的起始码
    let reduced = reduce_ansi_codes(&active_codes);
    let active_start_codes = filter_start_codes(&reduced);
    result.push_str(&undo_ansi_codes(&active_start_codes));
    result
}
