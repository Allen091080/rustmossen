//! ANSI control characters and constants (ansi.ts).

/// SGR parameter separator byte.
pub const SEP: u8 = b';';

/// C0 control characters.
pub struct C0;
impl C0 {
    pub const NUL: u8 = 0x00;
    pub const SOH: u8 = 0x01;
    pub const STX: u8 = 0x02;
    pub const ETX: u8 = 0x03;
    pub const EOT: u8 = 0x04;
    pub const ENQ: u8 = 0x05;
    pub const ACK: u8 = 0x06;
    pub const BEL: u8 = 0x07;
    pub const BS: u8 = 0x08;
    pub const HT: u8 = 0x09;
    pub const LF: u8 = 0x0A;
    pub const VT: u8 = 0x0B;
    pub const FF: u8 = 0x0C;
    pub const CR: u8 = 0x0D;
    pub const SO: u8 = 0x0E;
    pub const SI: u8 = 0x0F;
    pub const ESC: u8 = 0x1B;
    pub const DEL: u8 = 0x7F;
}

/// ESC sequence type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscType { Csi, Osc, Dcs, Apc, Ss3, Other }

/// Classify the byte after ESC.
pub fn esc_type(byte: u8) -> EscType {
    match byte {
        b'[' => EscType::Csi,
        b']' => EscType::Osc,
        b'P' => EscType::Dcs,
        b'_' => EscType::Apc,
        b'O' => EscType::Ss3,
        _ => EscType::Other,
    }
}

/// Check if byte is valid ESC sequence final byte.
pub fn is_esc_final(byte: u8) -> bool {
    (0x40..=0x7E).contains(&byte)
}

/// Check if byte is a C0 control character.
pub fn is_c0(byte: u8) -> bool {
    byte < 0x20 || byte == C0::DEL
}
