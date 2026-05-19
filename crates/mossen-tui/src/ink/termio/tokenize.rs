//! Input tokenizer — escape sequence boundary detection (tokenize.ts).

/// A token from the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Text(String),
    Sequence(String),
}

/// Tokenizer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State { Ground, Escape, EscapeIntermediate, Csi, Ss3, Osc, Dcs, Apc }

/// Streaming tokenizer for terminal input.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    state: State,
    buffer: String,
    x10_mouse: bool,
}

impl Tokenizer {
    pub fn new(x10_mouse: bool) -> Self {
        Self { state: State::Ground, buffer: String::new(), x10_mouse }
    }

    /// Feed input and get resulting tokens.
    pub fn feed(&mut self, input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut text_acc = String::new();
        let bytes = input.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            let byte = bytes[i];
            match self.state {
                State::Ground => {
                    if byte == super::ansi::C0::ESC {
                        if !text_acc.is_empty() {
                            tokens.push(Token::Text(std::mem::take(&mut text_acc)));
                        }
                        self.buffer.clear();
                        self.buffer.push(byte as char);
                        self.state = State::Escape;
                    } else if byte < 0x20 && byte != super::ansi::C0::HT && byte != super::ansi::C0::LF && byte != super::ansi::C0::CR {
                        if !text_acc.is_empty() {
                            tokens.push(Token::Text(std::mem::take(&mut text_acc)));
                        }
                        tokens.push(Token::Sequence(String::from(byte as char)));
                    } else {
                        text_acc.push(byte as char);
                    }
                }
                State::Escape => {
                    self.buffer.push(byte as char);
                    match byte {
                        b'[' => self.state = State::Csi,
                        b']' => self.state = State::Osc,
                        b'O' => self.state = State::Ss3,
                        b'P' => self.state = State::Dcs,
                        b'_' => self.state = State::Apc,
                        0x20..=0x2F => self.state = State::EscapeIntermediate,
                        _ => {
                            tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                            self.state = State::Ground;
                        }
                    }
                }
                State::EscapeIntermediate => {
                    self.buffer.push(byte as char);
                    if (0x30..=0x7E).contains(&byte) {
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    }
                }
                State::Csi => {
                    self.buffer.push(byte as char);
                    if super::csi::is_csi_final(byte) {
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    }
                }
                State::Ss3 => {
                    self.buffer.push(byte as char);
                    tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                    self.state = State::Ground;
                }
                State::Osc => {
                    if byte == super::ansi::C0::BEL || (byte == b'\\' && self.buffer.ends_with('\x1b')) {
                        if byte != super::ansi::C0::BEL { self.buffer.push(byte as char); }
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    } else {
                        self.buffer.push(byte as char);
                    }
                }
                State::Dcs | State::Apc => {
                    if byte == b'\\' && self.buffer.ends_with('\x1b') {
                        self.buffer.push(byte as char);
                        tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
                        self.state = State::Ground;
                    } else {
                        self.buffer.push(byte as char);
                    }
                }
            }
            i += 1;
        }

        if !text_acc.is_empty() {
            tokens.push(Token::Text(text_acc));
        }
        tokens
    }

    /// Flush buffered incomplete sequences.
    pub fn flush(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        if !self.buffer.is_empty() {
            tokens.push(Token::Sequence(std::mem::take(&mut self.buffer)));
            self.state = State::Ground;
        }
        tokens
    }

    /// Reset tokenizer state.
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.buffer.clear();
    }

    /// Get buffered content.
    pub fn buffer(&self) -> &str { &self.buffer }
}

impl Default for Tokenizer { fn default() -> Self { Self::new(false) } }

/// Build a fresh tokenizer (matches the TS factory name).
pub fn create_tokenizer(x10_mouse: bool) -> Tokenizer {
    Tokenizer::new(x10_mouse)
}
