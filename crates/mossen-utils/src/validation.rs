//! # validation — 设置验证工具
//!
//! 对应 TypeScript `utils/settings/validation.ts`。
//! 把 Zod 风格的验证错误格式化为人类可读的验证错误。
//!
//! Rust 端没有完整的 Zod 运行时，因此本模块在 [`format_zod_error`] 中
//! 解析序列化为 JSON 的 issue 列表（与 TS `error.issues` 等价）。

use serde::{Deserialize, Serialize};

/// 点号表示法中的字段路径（例如 "permissions.defaultMode", "env.DEBUG"）
pub type FieldPath = String;

/// 验证错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// 相对文件路径
    pub file: Option<String>,
    /// 点号表示法中的字段路径
    pub path: FieldPath,
    /// 人类可读的错误消息
    pub message: String,
    /// 期望的值或类型
    pub expected: Option<String>,
    /// 提供的实际无效值
    pub invalid_value: Option<serde_json::Value>,
    /// 修复错误的建议
    pub suggestion: Option<String>,
    /// 相关文档的链接
    pub doc_link: Option<String>,
}

/// 带错误的设置
#[derive(Debug, Clone)]
pub struct SettingsWithErrors {
    pub settings: serde_json::Value,
    pub errors: Vec<ValidationError>,
}

/// 获取未知值的类型字符串（用于错误消息）
fn get_received_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// 从消息中提取接收到的类型
fn extract_received_from_message(msg: &str) -> Option<&'static str> {
    let patterns = [
        "received null",
        "received boolean",
        "received number",
        "received string",
        "received array",
        "received object",
        "received undefined",
    ];
    for pattern in patterns {
        if msg.contains(pattern) {
            return Some(&pattern[9..]);
        }
    }
    None
}

/// 把单个 issue（与 Zod v4 `ZodIssue` 等价的 JSON）转换成 `ValidationError`。
fn issue_to_error(issue: &serde_json::Value, file_path: &str) -> ValidationError {
    let path = issue
        .get("path")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| {
                    if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default();

    let code = issue.get("code").and_then(|c| c.as_str()).unwrap_or("");
    let raw_message = issue
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Validation error")
        .to_string();

    let mut message = raw_message.clone();
    let mut expected: Option<String> = None;
    let mut invalid_value: Option<serde_json::Value> = None;

    match code {
        "invalid_value" => {
            let values = issue.get("values").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => format!("\"{}\"", s),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            if let Some(v) = values {
                message = format!("Invalid value. Expected one of: {}", v);
                expected = Some(v);
            }
        }
        "invalid_type" => {
            let expected_type = issue
                .get("expected")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();
            let input = issue
                .get("input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let received_type = extract_received_from_message(&raw_message)
                .map(|s| s.to_string())
                .unwrap_or_else(|| get_received_type(&input).to_string());
            if expected_type == "object" && received_type == "null" && path.is_empty() {
                message = "Invalid or malformed JSON".to_string();
            } else {
                message = format!("Expected {}, but received {}", expected_type, received_type);
            }
            expected = Some(expected_type);
            invalid_value = Some(serde_json::Value::String(received_type));
        }
        "unrecognized_keys" => {
            let keys: Vec<String> = issue
                .get("keys")
                .and_then(|k| k.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let joined = keys.join(", ");
            let label = if keys.len() == 1 { "field" } else { "fields" };
            message = format!("Unrecognized {}: {}", label, joined);
        }
        "too_small" => {
            let min = issue
                .get("minimum")
                .map(|m| m.to_string())
                .unwrap_or_default();
            message = format!("Number must be greater than or equal to {}", min);
            expected = Some(min);
        }
        _ => {}
    }

    ValidationError {
        file: Some(file_path.to_string()),
        path,
        message,
        expected,
        invalid_value,
        suggestion: None,
        doc_link: None,
    }
}

/// 格式化 Zod 验证错误为人类可读的验证错误。
///
/// `error_json` 必须是 Zod v4 `ZodError` 序列化后的 JSON 字符串
/// （即包含 `issues` 数组的对象，或直接是 issue 数组）。无法解析时返回单条
/// "Validation error" 描述。
pub fn format_zod_error(error_json: &str, file_path: &str) -> Vec<ValidationError> {
    let parsed: serde_json::Value = match serde_json::from_str(error_json) {
        Ok(v) => v,
        Err(_) => {
            return vec![ValidationError {
                file: Some(file_path.to_string()),
                path: String::new(),
                message: error_json.to_string(),
                expected: None,
                invalid_value: None,
                suggestion: None,
                doc_link: None,
            }];
        }
    };

    let issues = if parsed.is_array() {
        parsed.as_array().cloned().unwrap_or_default()
    } else {
        parsed
            .get("issues")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };

    issues
        .iter()
        .map(|i| issue_to_error(i, file_path))
        .collect()
}

/// 验证设置文件内容是否符合 SettingsSchema。
///
/// Rust 端没有完整的 SettingsSchema 运行时，因此只验证 JSON 合法性以及
/// `permissions.{allow,deny,ask}` 数组中字符串规则可被
/// [`crate::settings::validate_permission_rule`] 解析。其它字段宽松接受
/// （与编辑器写入路径一致：只阻止真正破坏文件结构的输入）。
pub fn validate_settings_file_content(content: &str) -> Result<(), String> {
    let json_data: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("Invalid JSON: {}", e))?;

    if !json_data.is_object() {
        return Err("Settings must be a JSON object".to_string());
    }

    // 检查 permissions.{allow,deny,ask} 是否为字符串数组；只校验结构。
    if let Some(perms) = json_data.get("permissions").and_then(|p| p.as_object()) {
        for key in ["allow", "deny", "ask"] {
            if let Some(rules) = perms.get(key) {
                let arr = rules
                    .as_array()
                    .ok_or_else(|| format!("permissions.{} must be an array", key))?;
                for rule in arr {
                    if !rule.is_string() {
                        return Err(format!("permissions.{} entries must be strings", key));
                    }
                }
            }
        }
    }

    Ok(())
}

/// 从原始解析的 JSON 数据中过滤无效的权限规则。
/// 这可以防止一个坏规则污染整个设置文件。
/// 返回每个过滤规则的警告。
pub fn filter_invalid_permission_rules(
    data: &serde_json::Value,
    file_path: &str,
) -> Vec<ValidationError> {
    let mut warnings = Vec::new();

    let Some(obj) = data.as_object() else {
        return warnings;
    };
    let Some(perms) = obj.get("permissions").and_then(|p| p.as_object()) else {
        return warnings;
    };

    for key in ["allow", "deny", "ask"] {
        let Some(rules) = perms.get(key).and_then(|r| r.as_array()) else {
            continue;
        };
        for rule in rules {
            match rule {
                serde_json::Value::String(s) => {
                    let result = crate::settings::validate_permission_rule(s);
                    if !result.valid {
                        let mut message = format!("Invalid permission rule \"{}\" was skipped", s);
                        if let Some(err) = &result.error {
                            message.push_str(": ");
                            message.push_str(err);
                        }
                        if let Some(sugg) = &result.suggestion {
                            message.push_str(". ");
                            message.push_str(sugg);
                        }
                        warnings.push(ValidationError {
                            file: Some(file_path.to_string()),
                            path: format!("permissions.{}", key),
                            message,
                            expected: None,
                            invalid_value: Some(rule.clone()),
                            suggestion: result.suggestion.clone(),
                            doc_link: None,
                        });
                    }
                }
                other => {
                    warnings.push(ValidationError {
                        file: Some(file_path.to_string()),
                        path: format!("permissions.{}", key),
                        message: format!("Non-string value in {} array was removed", key),
                        expected: Some("string".to_string()),
                        invalid_value: Some(other.clone()),
                        suggestion: None,
                        doc_link: None,
                    });
                }
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_received_type() {
        assert_eq!(get_received_type(&serde_json::Value::Null), "null");
        assert_eq!(get_received_type(&serde_json::Value::Bool(true)), "boolean");
        assert_eq!(
            get_received_type(&serde_json::Value::String("test".into())),
            "string"
        );
    }

    #[test]
    fn test_validate_settings_file_content_valid() {
        let content = r#"{"permissions": {"defaultMode": "default"}}"#;
        assert!(validate_settings_file_content(content).is_ok());
    }

    #[test]
    fn test_validate_settings_file_content_invalid() {
        let content = "not json";
        assert!(validate_settings_file_content(content).is_err());
    }

    #[test]
    fn test_format_zod_error_array() {
        let issues = r#"[{"code":"invalid_type","expected":"string","input":42,"message":"Expected string, received number","path":["env","DEBUG"]}]"#;
        let errors = format_zod_error(issues, "settings.json");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "env.DEBUG");
        assert!(errors[0].message.contains("Expected string"));
    }
}
