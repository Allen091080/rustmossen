//! # swarm_constants — Swarm 配置常量
//!
//! 对应 TypeScript `utils/swarm/constants.ts`。
//! Swarm 多智能体系统的配置常量。

use std::env;

/// Team Lead 的默认名称
pub const TEAM_LEAD_NAME: &str = "team-lead";

/// Swarm session 的默认名称
pub const SWARM_SESSION_NAME: &str = "mossen-swarm";

/// Swarm 视图窗口名称
pub const SWARM_VIEW_WINDOW_NAME: &str = "swarm-view";

/// Tmux 命令名称
pub const TMUX_COMMAND: &str = "tmux";

/// 隐藏的 session 名称
pub const HIDDEN_SESSION_NAME: &str = "mossen-hidden";

/// 获取外部 swarm session 的 socket 名称（当用户不在 tmux 中时使用）。
/// 使用单独的 socket 来隔离 swarm 操作和用户的 tmux session。
/// 包含 PID 以确保多个 Mossen 实例不会冲突。
pub fn get_swarm_socket_name() -> String {
    format!("mossen-swarm-{}", std::process::id())
}

/// 覆盖用于生成队友实例的命令的环境变量。
/// 如果未设置，默认为 process.execPath（当前 Mossen 二进制文件）。
/// 这允许为不同环境或测试进行自定义。
pub const TEAMMATE_COMMAND_ENV_VAR: &str = "MOSSEN_CODE_TEAMMATE_COMMAND";

/// 设置在生成的队友上的环境变量，用于指示其分配的颜色。
/// 用于彩色输出和窗格标识。
pub const TEAMMATE_COLOR_ENV_VAR: &str = "MOSSEN_CODE_AGENT_COLOR";

/// 设置在生成的队友上要求在实现前进入计划模式的环境变量。
/// 当设置为 'true' 时，队友必须进入计划模式并在写入代码之前获得批准。
pub const PLAN_MODE_REQUIRED_ENV_VAR: &str = "MOSSEN_CODE_PLAN_MODE_REQUIRED";

/// 获取 teammate 命令路径（从环境变量或默认路径）
pub fn get_teammate_command() -> Option<String> {
    env::var(TEAMMATE_COMMAND_ENV_VAR).ok()
}

/// 获取 teammate 颜色（从环境变量）
pub fn get_teammate_color() -> Option<String> {
    env::var(TEAMMATE_COLOR_ENV_VAR).ok()
}

/// 检查是否需要计划模式（从环境变量）
pub fn is_plan_mode_required() -> bool {
    env::var(PLAN_MODE_REQUIRED_ENV_VAR)
        .map(|v| v == "true")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_lead_name() {
        assert_eq!(TEAM_LEAD_NAME, "team-lead");
    }

    #[test]
    fn test_swarm_socket_name() {
        let name = get_swarm_socket_name();
        assert!(name.starts_with("mossen-swarm-"));
    }
}