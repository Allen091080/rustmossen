//! # immediate_command — 即时命令控制
//!
//! 对应 TypeScript `utils/immediateCommand.ts`。
//! 推理配置命令（/model, /fast, /effort）是否应立即执行。

/// 返回推理配置命令是否应立即执行。
/// 在运行中的查询期间执行，而不是等待当前回合结束。
/// 对 ants 始终启用；对外部用户由实验控制。
pub fn should_inference_config_command_be_immediate() -> bool {
    // 检查是否为 ant 用户
    if std::env::var("USER_TYPE").ok().as_deref() == Some("ant") {
        return true;
    }
    // 与 TS `getFeatureValue_CACHED_MAY_BE_STALE('tengu_immediate_model_command', false)` 对齐：
    // Rust 端尚未集成 GrowthBook 客户端，使用对应环境变量作为本地覆盖。
    matches!(
        std::env::var("MOSSEN_FEATURE_TENGU_IMMEDIATE_MODEL_COMMAND")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_immediate_command_defaults_to_false() {
        // 当 USER_TYPE 不是 ant 时，默认返回 false
        assert!(!should_inference_config_command_be_immediate());
    }
}
