//! # standalone_agent — 独立 agent 工具
//!
//! 对应 TypeScript `utils/standaloneAgent.ts`。

/// 独立 agent 上下文。
#[derive(Debug, Clone)]
pub struct StandaloneAgentContext {
    pub name: Option<String>,
    pub color: Option<String>,
}

/// 获取独立 agent 名称（如果设置且不在 swarm 团队中）。
///
/// 使用 team_name 来判断是否属于 swarm，如果在团队中则返回 None。
pub fn get_standalone_agent_name(
    standalone_context: Option<&StandaloneAgentContext>,
    team_name: Option<&str>,
) -> Option<String> {
    // 如果在团队(swarm)中，不返回独立名称
    if team_name.is_some() {
        return None;
    }
    standalone_context.and_then(|ctx| ctx.name.clone())
}
