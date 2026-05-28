//! # ndjson — NDJSON 编解码
//!
//! 提供 Newline-Delimited JSON (NDJSON) 的安全序列化与解析。
//! 对应 TS `cli/ndjsonSafeStringify.ts`。
//!
//! 主要解决 U+2028（LINE SEPARATOR）和 U+2029（PARAGRAPH SEPARATOR）
//! 在 JSON 字符串中导致行分割错误的问题。

use serde::Serialize;

/// 将 U+2028 和 U+2029 转义为 `\uXXXX` 形式。
///
/// JSON.stringify 会原样输出这两个字符，但接收端在按行分割时
/// 可能将它们视为行终止符，导致 JSON 被截断。
/// `\uXXXX` 形式在 JSON 中等价，但不会被误认为行终止符。
fn escape_js_line_terminators(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    for ch in json.chars() {
        match ch {
            '\u{2028}' => result.push_str("\\u2028"),
            '\u{2029}' => result.push_str("\\u2029"),
            _ => result.push(ch),
        }
    }
    result
}

/// NDJSON 安全序列化。
///
/// 将值序列化为 JSON 字符串，并转义 U+2028/U+2029，
/// 确保输出不会被行分割接收端截断。
///
/// # 示例
/// ```
/// use mossen_remote::ndjson::ndjson_safe_stringify;
/// use serde_json::json;
///
/// let msg = json!({"type": "keep_alive"});
/// let line = ndjson_safe_stringify(&msg).unwrap();
/// assert!(!line.contains('\n'));
/// ```
pub fn ndjson_safe_stringify<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(escape_js_line_terminators(&json))
}

/// NDJSON 安全序列化并追加换行符。
///
/// 等价于 `ndjson_safe_stringify(value) + "\n"`。
pub fn ndjson_safe_line<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut line = ndjson_safe_stringify(value)?;
    line.push('\n');
    Ok(line)
}

/// 解析 NDJSON 行。
///
/// 跳过空行，返回解析后的 JSON 值。
pub fn parse_ndjson_line(line: &str) -> Option<serde_json::Result<serde_json::Value>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(serde_json::from_str(trimmed))
}

/// 从 NDJSON 字符串流中提取所有行。
///
/// 返回已解析的 JSON 值列表以及剩余的不完整缓冲区。
pub fn parse_ndjson_buffer(buffer: &str) -> (Vec<serde_json::Value>, String) {
    let mut values = Vec::new();
    let mut remaining = String::new();

    for (i, line) in buffer.split('\n').enumerate() {
        // 最后一段可能是不完整行
        if i == buffer.matches('\n').count() && !buffer.ends_with('\n') {
            remaining = line.to_string();
            break;
        }
        if let Some(Ok(value)) = parse_ndjson_line(line) {
            values.push(value);
        }
    }

    (values, remaining)
}

/// SSE 帧解析器。
///
/// 增量解析 Server-Sent Events 流数据。
/// 对应 TS `cli/transports/SSETransport.ts` 中的 `parseSSEFrames`。
#[derive(Debug, Clone)]
pub struct SseFrame {
    /// 事件类型。
    pub event: Option<String>,
    /// 事件 ID。
    pub id: Option<String>,
    /// 事件数据。
    pub data: Option<String>,
}

/// 增量解析 SSE 帧。
///
/// SSE 帧以双换行符 `\n\n` 分隔。
/// 返回已解析的帧和剩余的不完整缓冲区。
pub fn parse_sse_frames(buffer: &str) -> (Vec<SseFrame>, String) {
    let mut frames = Vec::new();
    let mut pos = 0;

    while let Some(idx) = buffer[pos..].find("\n\n") {
        let raw_frame = &buffer[pos..pos + idx];
        pos += idx + 2;

        let trimmed = raw_frame.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut frame = SseFrame {
            event: None,
            id: None,
            data: None,
        };

        for line in raw_frame.split('\n') {
            // SSE 注释行以 ':' 开头
            if line.starts_with(':') {
                continue;
            }

            if let Some(colon_idx) = line.find(':') {
                let field = &line[..colon_idx];
                // 按 SSE 规范，冒号后的一个前导空格应被忽略
                let value = if line.as_bytes().get(colon_idx + 1) == Some(&b' ') {
                    &line[colon_idx + 2..]
                } else {
                    &line[colon_idx + 1..]
                };

                match field {
                    "event" => frame.event = Some(value.to_string()),
                    "id" => frame.id = Some(value.to_string()),
                    "data" => {
                        // 多行 data 字段需要拼接
                        if let Some(ref mut existing) = frame.data {
                            existing.push('\n');
                            existing.push_str(value);
                        } else {
                            frame.data = Some(value.to_string());
                        }
                    }
                    _ => {} // 忽略未知字段
                }
            }
        }

        frames.push(frame);
    }

    let remaining = buffer[pos..].to_string();
    (frames, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_line_terminators() {
        let input = "hello\u{2028}world\u{2029}";
        let escaped = escape_js_line_terminators(input);
        assert_eq!(escaped, "hello\\u2028world\\u2029");
    }

    #[test]
    fn test_ndjson_safe_stringify() {
        use serde_json::json;
        let val = json!({"type": "test", "data": "a\u{2028}b"});
        let s = ndjson_safe_stringify(&val).unwrap();
        assert!(!s.contains('\u{2028}'));
        assert!(s.contains("\\u2028"));
    }

    #[test]
    fn test_parse_sse_frames() {
        let buffer = "event: message\ndata: hello\n\nevent: done\ndata: bye\n\npartial";
        let (frames, remaining) = parse_sse_frames(buffer);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event.as_deref(), Some("message"));
        assert_eq!(frames[0].data.as_deref(), Some("hello"));
        assert_eq!(frames[1].event.as_deref(), Some("done"));
        assert_eq!(remaining, "partial");
    }
}
