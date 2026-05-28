//! # session_url — 会话 URL 解析工具
//!
//! 对应 TypeScript `utils/sessionUrl.ts`。
//! 解析会话恢复标识符（URL、UUID 或 JSONL 文件路径）。

use uuid::Uuid;

/// 解析后的会话 URL 信息。
#[derive(Debug, Clone)]
pub struct ParsedSessionUrl {
    pub session_id: Uuid,
    pub ingress_url: Option<String>,
    pub is_url: bool,
    pub jsonl_file: Option<String>,
    pub is_jsonl_file: bool,
}

/// 解析会话恢复标识符。
///
/// 支持以下格式：
/// - 以 `.jsonl` 结尾的文件路径
/// - 纯 UUID 字符串
/// - 包含会话 ID 的 URL
///
/// # 参数
/// - `resume_identifier`: 要解析的 URL 或会话 ID
///
/// # 返回
/// 解析后的会话信息，无效输入返回 None
pub fn parse_session_identifier(resume_identifier: &str) -> Option<ParsedSessionUrl> {
    // 检查是否为 JSONL 文件路径（在 URL 解析之前，因为 Windows 绝对路径
    // 如 C:\path\file.jsonl 会被解析为带 C: 协议的有效 URL）
    if resume_identifier.to_lowercase().ends_with(".jsonl") {
        return Some(ParsedSessionUrl {
            session_id: Uuid::new_v4(),
            ingress_url: None,
            is_url: false,
            jsonl_file: Some(resume_identifier.to_string()),
            is_jsonl_file: true,
        });
    }

    // 检查是否为纯 UUID
    if let Ok(uuid) = Uuid::parse_str(resume_identifier) {
        return Some(ParsedSessionUrl {
            session_id: uuid,
            ingress_url: None,
            is_url: false,
            jsonl_file: None,
            is_jsonl_file: false,
        });
    }

    // 检查是否为 URL
    if let Ok(parsed_url) = url::Url::parse(resume_identifier) {
        let scheme = parsed_url.scheme();
        if scheme == "http" || scheme == "https" {
            return Some(ParsedSessionUrl {
                session_id: Uuid::new_v4(),
                ingress_url: Some(parsed_url.to_string()),
                is_url: true,
                jsonl_file: None,
                is_jsonl_file: false,
            });
        }
    }

    None
}
