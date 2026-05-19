//! # deferred_slash_commands — 延迟斜杠命令控制
//!
//! 对应 TypeScript `utils/deferredSlashCommands.ts`。

/// 检查是否启用了指定的延迟斜杠命令。
pub fn is_deferred_slash_command_enabled(name: &str) -> bool {
    // 检查全局启用标志
    if is_env_truthy(&std::env::var("MOSSEN_CODE_ENABLE_DEFERRED_SLASH_COMMANDS").unwrap_or_default()) {
        return true;
    }
    // 检查特定命令的启用标志
    is_env_truthy(&std::env::var(env_name_for_command(name)).unwrap_or_default())
}

/// 生成命令对应的环境变量名。
fn env_name_for_command(name: &str) -> String {
    format!("MOSSEN_CODE_ENABLE_{}_COMMAND", name.replace('-', "_").to_uppercase())
}

/// 检查环境变量是否为真值。
fn is_env_truthy(value: &str) -> bool {
    !value.is_empty() && value != "0" && value.to_lowercase() != "false"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_name_for_command() {
        assert_eq!(env_name_for_command("my-command"), "MOSSEN_CODE_ENABLE_MY_COMMAND_COMMAND");
        assert_eq!(env_name_for_command("test"), "MOSSEN_CODE_ENABLE_TEST_COMMAND");
    }

    #[test]
    fn test_is_env_truthy() {
        assert!(is_env_truthy("true"));
        assert!(is_env_truthy("1"));
        assert!(is_env_truthy("yes"));
        assert!(!is_env_truthy(""));
        assert!(!is_env_truthy("0"));
        assert!(!is_env_truthy("false"));
    }
}
