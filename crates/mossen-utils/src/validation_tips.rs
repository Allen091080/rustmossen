//! # validation_tips — 验证提示工具
//!
//! 对应 TypeScript `utils/settings/validationTips.ts`。
//! 提供特定验证错误的修复建议。

/// 验证提示
#[derive(Debug, Clone)]
pub struct ValidationTip {
    pub suggestion: Option<String>,
    pub doc_link: Option<String>,
}

/// 提示上下文
#[derive(Debug, Clone)]
pub struct TipContext {
    pub path: String,
    pub code: String,
    pub expected: Option<String>,
    pub received: Option<String>,
    pub enum_values: Option<Vec<String>>,
    pub message: Option<String>,
    pub value: Option<String>,
}

/// 文档基础 URL
const DOCUMENTATION_BASE: &str = "https://mossen.invalid/docs";

/// 获取验证提示
pub fn get_validation_tip(context: TipContext) -> Option<ValidationTip> {
    // 实现匹配规则
    let suggestion = match context.path.as_str() {
        "permissions.defaultMode" if context.code == "invalid_value" => {
            Some("Valid modes: \"acceptEdits\" (ask before file changes), \"plan\" (analysis only), \"bypassPermissions\" (auto-accept all), or \"default\" (standard behavior)".to_string())
        }
        "apiKeyHelper" if context.code == "invalid_type" => {
            Some("Provide a shell command that outputs your API key to stdout. The script should output only the API key. Example: \"/bin/generate_temp_api_key.sh\"".to_string())
        }
        "cleanupPeriodDays" if context.code == "too_small" => {
            Some("Must be 0 or greater. Set a positive number for days to retain transcripts (default is 30). Setting 0 disables session persistence entirely.".to_string())
        }
        path if path.starts_with("env.") && context.code == "invalid_type" => {
            Some("Environment variables must be strings. Wrap numbers and booleans in quotes. Example: \"DEBUG\": \"true\", \"PORT\": \"3000\"".to_string())
        }
        path if (path == "permissions.allow" || path == "permissions.deny") && context.code == "invalid_type" => {
            Some("Permission rules must be in an array. Format: [\"Tool(specifier)\"]. Examples: [\"Bash(npm run build)\", \"Edit(docs/**)\", \"Read(~/.zshrc)\"]. Use * for wildcards.".to_string())
        }
        path if path.contains("hooks") && context.code == "invalid_type" => {
            Some("Hooks use a matcher + hooks array. The matcher is a string: a tool name (\"Bash\"), pipe-separated list (\"Edit|Write\"), or empty to match all.".to_string())
        }
        _ if context.code == "unrecognized_keys" => {
            Some("Check for typos or refer to the documentation for valid fields".to_string())
        }
        _ => None,
    };

    // 确定文档链接
    let doc_link = context.path.split('.').next().and_then(|prefix| {
        match prefix {
            "permissions" => Some(format!("{}/iam#configuring-permissions", DOCUMENTATION_BASE)),
            "env" => Some(format!("{}/settings#environment-variables", DOCUMENTATION_BASE)),
            "hooks" => Some(format!("{}/hooks", DOCUMENTATION_BASE)),
            _ => None,
        }
    });

    if suggestion.is_some() || doc_link.is_some() {
        Some(ValidationTip { suggestion, doc_link })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_validation_tip_permissions() {
        let tip = get_validation_tip(TipContext {
            path: "permissions.defaultMode".to_string(),
            code: "invalid_value".to_string(),
            expected: None,
            received: None,
            enum_values: None,
            message: None,
            value: None,
        });
        assert!(tip.is_some());
        assert!(tip.unwrap().suggestion.is_some());
    }

    #[test]
    fn test_get_validation_tip_env() {
        let tip = get_validation_tip(TipContext {
            path: "env.DEBUG".to_string(),
            code: "invalid_type".to_string(),
            expected: None,
            received: None,
            enum_values: None,
            message: None,
            value: None,
        });
        assert!(tip.is_some());
    }
}