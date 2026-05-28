//! # teammate_init — 队友初始化
//!
//! 对应 TypeScript `utils/swarm/teammateInit.ts`。
//! 处理在 swarm 中作为队友运行的 Mossen 实例的初始化。

use crate::team_helpers::read_team_file;
use crate::teammate_mailbox::{
    create_idle_notification, get_inbox_path, IdleReason, TeammateMessage,
};

/// 团队信息
#[derive(Debug, Clone)]
pub struct TeamInfo {
    pub team_name: String,
    pub agent_id: String,
    pub agent_name: String,
}

/// 团队范围允许路径条目（与 [`crate::team_helpers::TeamAllowedPath`] 的展开形式）。
#[derive(Debug, Clone)]
pub struct AppliedTeamRule {
    pub tool_name: String,
    pub rule_content: String,
}

/// 初始化队友的状态：成功调用一次后返回需要由 AppState 层进一步生效的规则列表
/// 以及是否注册了 Stop hook（用于发送 idle 通知）。
#[derive(Debug, Clone, Default)]
pub struct TeammateInitResult {
    pub applied_rules: Vec<AppliedTeamRule>,
    pub leader_inbox_path: Option<std::path::PathBuf>,
    pub leader_agent_name: Option<String>,
    pub is_leader: bool,
}

/// 初始化队友的 hooks。
///
/// 应在 AppState 可用后尽早调用。与 TS 端 `initializeTeammateHooks` 对齐：
/// 1. 读取团队文件获取领导 ID；
/// 2. 计算 `teamAllowedPaths` 展开成 `toolName + ruleContent` 列表（调用方传入
///    AppState 后用 `applyPermissionUpdate` 真正生效）；
/// 3. 计算领导邮箱路径，便于 Stop hook 写入。
///
/// Rust 端没有 React-style `setAppState`，因此把效果以数据形式回传，由调用方
/// 写入会话状态、并自行注册 Stop hook（参见 [`build_idle_notification_message`]）。
pub fn initialize_teammate_hooks(team_info: TeamInfo) -> TeammateInitResult {
    let TeamInfo {
        team_name,
        agent_id,
        ..
    } = &team_info;

    let team_file = match read_team_file(team_name) {
        Some(f) => f,
        None => {
            tracing::debug!("[TeammateInit] Team file not found for team: {}", team_name);
            return TeammateInitResult::default();
        }
    };

    let lead_agent_id = team_file.lead_agent_id.clone();
    let is_leader = agent_id == &lead_agent_id;

    let mut applied_rules = Vec::new();
    if let Some(paths) = &team_file.team_allowed_paths {
        for allowed in paths {
            let rule_content = if allowed.path.starts_with('/') {
                format!("/{}/**", allowed.path)
            } else {
                format!("{}/**", allowed.path)
            };
            tracing::debug!(
                "[TeammateInit] Applying team permission: {} allowed in {} (rule: {})",
                allowed.tool_name,
                allowed.path,
                rule_content
            );
            applied_rules.push(AppliedTeamRule {
                tool_name: allowed.tool_name.clone(),
                rule_content,
            });
        }
    }

    if is_leader {
        tracing::debug!("[TeammateInit] Leader agent, skipping idle hook setup");
        return TeammateInitResult {
            applied_rules,
            leader_inbox_path: None,
            leader_agent_name: None,
            is_leader: true,
        };
    }

    let lead_member = team_file
        .members
        .iter()
        .find(|m| m.agent_id == lead_agent_id);
    let lead_agent_name = lead_member
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "team-lead".to_string());

    let teams_dir = crate::team_helpers::get_teams_dir();
    let leader_inbox = get_inbox_path(
        &lead_agent_name,
        Some(team_name),
        &teams_dir,
        Some(team_name),
    );

    TeammateInitResult {
        applied_rules,
        leader_inbox_path: Some(leader_inbox),
        leader_agent_name: Some(lead_agent_name),
        is_leader: false,
    }
}

/// 构造发往领导的 idle 通知。调用方在 Stop hook 中获取最近的 peer-DM
/// 摘要后调用本函数，再写入 [`TeammateInitResult::leader_inbox_path`]。
pub fn build_idle_notification_message(
    agent_name: &str,
    summary: Option<String>,
    color: Option<String>,
) -> TeammateMessage {
    let notification = create_idle_notification(
        agent_name,
        Some(IdleReason::Available),
        summary.clone(),
        None,
        None,
        None,
    );
    let text = serde_json::to_string(&notification).unwrap_or_else(|_| String::from("{}"));
    TeammateMessage {
        from: agent_name.to_string(),
        text,
        timestamp: chrono::Utc::now().to_rfc3339(),
        color,
        read: false,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_team_file_returns_default() {
        let info = TeamInfo {
            team_name: "definitely-not-a-real-team".to_string(),
            agent_id: "agent-x".to_string(),
            agent_name: "agent-x".to_string(),
        };
        let result = initialize_teammate_hooks(info);
        assert!(result.applied_rules.is_empty());
        assert!(result.leader_inbox_path.is_none());
    }
}
