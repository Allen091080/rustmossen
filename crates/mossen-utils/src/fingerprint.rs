//! # fingerprint — Mossen 归属指纹计算
//!
//! 对应 TypeScript `utils/fingerprint.ts`。
//! 计算用于 API 归属的 3 字符指纹。

use sha2::{Digest, Sha256};

/// 硬编码的盐值（后端验证使用）
pub const FINGERPRINT_SALT: &str = "59cf53e54c78";

/// 消息内容块类型
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text { text: String },
    Other,
}

/// 消息类型
#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Assistant,
}

/// 消息内容
#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// 简化的消息结构
#[derive(Debug, Clone)]
pub struct SimpleMessage {
    pub msg_type: MessageType,
    pub content: MessageContent,
}

/// 从第一条用户消息中提取文本内容。
///
/// # 参数
/// - `messages`: 内部消息类型数组
///
/// # 返回
/// 第一条文本内容，未找到则返回空字符串
pub fn extract_first_message_text(messages: &[SimpleMessage]) -> String {
    let first_user_message = messages
        .iter()
        .find(|msg| matches!(msg.msg_type, MessageType::User));

    let first_user_message = match first_user_message {
        Some(msg) => msg,
        None => return String::new(),
    };

    match &first_user_message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Blocks(blocks) => {
            for block in blocks {
                if let ContentBlock::Text { text } = block {
                    return text.clone();
                }
            }
            String::new()
        }
    }
}

/// 计算 Mossen 归属的 3 字符指纹。
///
/// 算法: SHA256(SALT + msg[4] + msg[7] + msg[20] + version)[:3]
///
/// 重要: 不要在不与 1P 和 3P (Bedrock, Vertex, Azure) API 仔细协调的情况下更改此方法。
///
/// # 参数
/// - `message_text`: 第一条用户消息文本
/// - `version`: 版本字符串
///
/// # 返回
/// 3 字符十六进制指纹
pub fn compute_fingerprint(message_text: &str, version: &str) -> String {
    // 提取索引 [4, 7, 20] 处的字符，索引不存在时使用 "0"
    let indices = [4, 7, 20];
    let chars: String = indices
        .iter()
        .map(|&i| message_text.chars().nth(i).unwrap_or('0'))
        .collect();

    let fingerprint_input = format!("{}{}{}", FINGERPRINT_SALT, chars, version);

    // SHA256 哈希，返回前 3 个十六进制字符
    let mut hasher = Sha256::new();
    hasher.update(fingerprint_input.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..2])[..3].to_string()
}

/// 从第一条用户消息计算指纹。
///
/// # 参数
/// - `messages`: 规范化消息数组
/// - `version`: 版本字符串
///
/// # 返回
/// 3 字符十六进制指纹
pub fn compute_fingerprint_from_messages(messages: &[SimpleMessage], version: &str) -> String {
    let first_message_text = extract_first_message_text(messages);
    compute_fingerprint(&first_message_text, version)
}
