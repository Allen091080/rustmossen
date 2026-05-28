//! # reconnection — Swarm 重连模块
//!
//! 对应 TypeScript `utils/swarm/reconnection.ts`。
//! 处理队友的 swarm 上下文初始化。

use serde::{Deserialize, Serialize};

use crate::team_helpers::{get_team_file_path, read_team_file};
use crate::teammate::get_dynamic_team_context;

/// 团队上下文
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamContext {
    pub team_name: String,
    pub team_file_path: String,
    pub lead_agent_id: String,
    pub self_agent_id: Option<String>,
    pub self_agent_name: String,
    pub is_leader: bool,
    pub teammates: std::collections::HashMap<String, serde_json::Value>,
}

/// 计算 AppState 的初始 teamContext。
///
/// 这在 main.tsx 中同步调用以在首次渲染之前计算 teamContext，
/// 消除了对 useEffect 工作 around 的需要。
///
/// 与 TS 对齐：
/// - 优先使用 [`get_dynamic_team_context`]（CLI 启动时设置的全局上下文）。
/// - 调用方仍可显式传入 `team_name`/`agent_name`，作为 dynamic context 缺失时的回退。
/// - `is_leader` 由 `agent_id` 是否为空推出：无 agent_id 表示为 leader。
pub fn compute_initial_team_context(
    team_name_override: Option<&str>,
    agent_name_override: Option<&str>,
) -> Option<TeamContext> {
    let dynamic = get_dynamic_team_context();

    let (team_name, agent_name, agent_id) = match (
        &dynamic,
        team_name_override,
        agent_name_override,
    ) {
        (Some(ctx), _, _) if !ctx.team_name.is_empty() && !ctx.agent_name.is_empty() => (
            ctx.team_name.clone(),
            ctx.agent_name.clone(),
            // TS: agentId 可能是空字符串（leader），同样处理为 None。
            if ctx.agent_id.is_empty() {
                None
            } else {
                Some(ctx.agent_id.clone())
            },
        ),
        (_, Some(tn), Some(an)) => (tn.to_string(), an.to_string(), None),
        _ => {
            tracing::debug!(
                "[Reconnection] compute_initial_team_context: 未设置 teammate 上下文（非 teammate 会话）"
            );
            return None;
        }
    };

    let team_file = read_team_file(&team_name).or_else(|| {
        tracing::error!(
            "[compute_initial_team_context] Could not read team file for {}",
            team_name
        );
        None
    })?;

    let team_file_path = get_team_file_path(&team_name).to_string_lossy().to_string();

    // 无 agent_id 表示是 leader
    let is_leader = agent_id.is_none();

    Some(TeamContext {
        team_name,
        team_file_path,
        lead_agent_id: team_file.lead_agent_id,
        self_agent_id: agent_id,
        self_agent_name: agent_name,
        is_leader,
        teammates: std::collections::HashMap::new(),
    })
}

/// 从恢复的 session 初始化队友上下文。
/// 这在恢复具有存储的 teamName/agentName 的 session 时调用。
pub fn initialize_teammate_context_from_session(
    team_name: &str,
    agent_name: &str,
) -> Option<TeamContext> {
    let team_file = read_team_file(team_name)?;
    let team_file_path = get_team_file_path(team_name).to_string_lossy().to_string();

    // 在团队文件中查找成员以获取他们的 agent_id
    let member = team_file.members.iter().find(|m| m.name == agent_name);
    let agent_id = member.map(|m| m.agent_id.clone());

    Some(TeamContext {
        team_name: team_name.to_string(),
        team_file_path,
        lead_agent_id: team_file.lead_agent_id,
        self_agent_id: agent_id,
        self_agent_name: agent_name.to_string(),
        is_leader: false,
        teammates: std::collections::HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_context_default() {
        let ctx = TeamContext::default();
        assert!(ctx.team_name.is_empty());
        assert!(!ctx.is_leader);
    }
}
