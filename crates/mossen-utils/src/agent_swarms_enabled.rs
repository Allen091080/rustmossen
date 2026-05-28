//! # agent_swarms_enabled — Agent 团队功能开关
//!
//! 对应 TypeScript `utils/agentSwarmsEnabled.ts`。

/// 检查环境变量值是否为 truthy。
fn is_truthy(val: Option<&str>) -> bool {
    val.map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// 检查 --agent-teams 标志是否通过 CLI 提供。
fn is_agent_teams_flag_set() -> bool {
    std::env::args().any(|arg| arg == "--agent-teams")
}

/// 集中式运行时检查 agent 团队/队友功能是否启用。
///
/// 这是唯一的门控检查点——所有引用队友的地方都应检查此函数。
///
/// - Internal 构建：始终启用
/// - 外部构建需要同时满足：
///   1. 通过环境变量或 --agent-teams 标志选择加入
///   2. GrowthBook gate 'mossen_amber_flint' 启用（killswitch）
pub fn is_agent_swarms_enabled() -> bool {
    let user_type = std::env::var("USER_TYPE").unwrap_or_default();
    // Internal: 始终开启
    if user_type == "internal" {
        return true;
    }

    // 外部用户：需要通过环境变量或标志选择加入
    let env_opt_in = std::env::var("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS").ok();
    if !is_truthy(env_opt_in.as_deref()) && !is_agent_teams_flag_set() {
        return false;
    }

    // Killswitch — 外部用户始终尊重
    let killswitch = std::env::var("MOSSEN_AMBER_FLINT")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    if !killswitch {
        return false;
    }

    true
}
