//! # git_settings — Git 相关设置
//!
//! 对应 TypeScript `utils/gitSettings.ts`。
//! 依赖于用户设置的 Git 相关行为。

/// 判断是否应包含 Git 指令。
pub fn should_include_git_instructions() -> bool {
    // 检查环境变量
    if let Ok(val) = std::env::var("MOSSEN_CODE_DISABLE_GIT_INSTRUCTIONS") {
        if val.is_empty() || val.to_lowercase() == "false" || val == "0" {
            return true;
        }
        if val.to_lowercase() == "true" || val == "1" {
            return false;
        }
    }
    // 回退到 settings 快照：`getInitialSettings().includeGitInstructions ?? true`。
    let snapshot = crate::settings_config::get_initial_settings();
    snapshot
        .get("includeGitInstructions")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_instructions_default_true() {
        std::env::remove_var("MOSSEN_CODE_DISABLE_GIT_INSTRUCTIONS");
        assert!(should_include_git_instructions());
    }
}
