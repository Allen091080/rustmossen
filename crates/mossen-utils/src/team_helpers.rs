//! # team_helpers — 团队辅助函数
//!
//! 对应 TypeScript `utils/swarm/teamHelpers.ts`。
//! 团队文件管理、工作树清理等辅助函数。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 团队允许的路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAllowedPath {
    pub path: String,
    pub tool_name: String,
    pub added_by: String,
    pub added_at: u64,
}

/// 团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub agent_id: String,
    pub name: String,
    #[serde(rename = "agentType")]
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub color: Option<String>,
    #[serde(rename = "planModeRequired")]
    pub plan_mode_required: Option<bool>,
    #[serde(rename = "joinedAt")]
    pub joined_at: u64,
    #[serde(rename = "tmuxPaneId")]
    pub tmux_pane_id: String,
    pub cwd: String,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub subscriptions: Vec<String>,
    #[serde(rename = "backendType")]
    pub backend_type: Option<String>,
    #[serde(rename = "isActive")]
    pub is_active: Option<bool>,
    pub mode: Option<String>,
}

/// 团队文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamFile {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    #[serde(rename = "leadAgentId")]
    pub lead_agent_id: String,
    #[serde(rename = "leadSessionId")]
    pub lead_session_id: Option<String>,
    #[serde(rename = "hiddenPaneIds")]
    pub hidden_pane_ids: Option<Vec<String>>,
    #[serde(rename = "teamAllowedPaths")]
    pub team_allowed_paths: Option<Vec<TeamAllowedPath>>,
    pub members: Vec<TeamMember>,
}

/// 清理用于 tmux 窗口名称、工作树路径和文件路径的名称。
/// 将所有非字母数字字符替换为连字符并转为小写。
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
}

/// 清理用于确定性代理 ID 的代理名称。
/// 将 @ 替换为 - 以防止 agentName@teamName 格式中的歧义。
pub fn sanitize_agent_name(name: &str) -> String {
    name.replace('@', "-")
}

/// 获取所有团队的根目录（对应 TS `getTeamsDir`）。
/// 与 TS 行为一致：基于 [`crate::env::get_mossen_config_home_dir`] 拼接 `teams`。
pub fn get_teams_dir() -> PathBuf {
    crate::env::get_mossen_config_home_dir().join("teams")
}

/// 获取团队目录的路径
pub fn get_team_dir(team_name: &str) -> PathBuf {
    get_teams_dir().join(sanitize_name(team_name))
}

/// 获取团队 config.json 文件的路径
pub fn get_team_file_path(team_name: &str) -> PathBuf {
    get_team_dir(team_name).join("config.json")
}

/// 读取团队文件（同步 — 用于同步上下文如 React 渲染路径）
pub fn read_team_file(team_name: &str) -> Option<TeamFile> {
    let path = get_team_file_path(team_name);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 读取团队文件（异步 — 用于工具处理器和其他异步上下文）
pub async fn read_team_file_async(team_name: &str) -> Option<TeamFile> {
    let path = get_team_file_path(team_name);
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// 写入团队文件（同步 — 用于同步上下文）
fn write_team_file(team_name: &str, team_file: &TeamFile) -> std::io::Result<()> {
    let team_dir = get_team_dir(team_name);
    fs::create_dir_all(&team_dir)?;
    let content = serde_json::to_string_pretty(team_file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(get_team_file_path(team_name), content)
}

/// 写入团队文件（异步 — 用于工具处理器）
pub async fn write_team_file_async(team_name: &str, team_file: &TeamFile) -> std::io::Result<()> {
    let team_dir = get_team_dir(team_name);
    tokio::fs::create_dir_all(&team_dir).await?;
    let content = serde_json::to_string_pretty(team_file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    tokio::fs::write(get_team_file_path(team_name), content).await
}

/// 从团队文件中按代理 ID 或名称移除队友。
pub fn remove_teammate_from_team_file(
    team_name: &str,
    agent_id: Option<&str>,
    name: Option<&str>,
) -> bool {
    let Some(_identifier) = agent_id.or(name) else {
        return false;
    };
    let team_file = match read_team_file(team_name) {
        Some(tf) => tf,
        None => return false,
    };

    let original_length = team_file.members.len();
    let members: Vec<TeamMember> = team_file
        .members
        .into_iter()
        .filter(|m| {
            if let Some(aid) = agent_id {
                if m.agent_id == aid {
                    return false;
                }
            }
            if let Some(n) = name {
                if m.name == n {
                    return false;
                }
            }
            true
        })
        .collect();

    if members.len() == original_length {
        return false;
    }

    let new_team_file = TeamFile {
        members,
        ..team_file
    };

    write_team_file(team_name, &new_team_file).is_ok()
}

/// 从团队配置文件中按窗格 ID 移除队友。
/// 同时从 hiddenPaneIds 中移除（如果存在）。
pub fn remove_member_from_team(team_name: &str, tmux_pane_id: &str) -> bool {
    let mut team_file = match read_team_file(team_name) {
        Some(tf) => tf,
        None => return false,
    };

    let member_index = team_file
        .members
        .iter()
        .position(|m| m.tmux_pane_id == tmux_pane_id);

    let Some(idx) = member_index else {
        return false;
    };

    // 从 members 数组中移除
    team_file.members.remove(idx);

    // 同时从 hiddenPaneIds 中移除（如果存在）
    if let Some(ref mut hidden) = team_file.hidden_pane_ids {
        if let Some(hidden_idx) = hidden.iter().position(|id| *id == tmux_pane_id) {
            hidden.remove(hidden_idx);
        }
    }

    write_team_file(team_name, &team_file).is_ok()
}

/// 按代理 ID 从团队的成员列表中移除队友。
pub fn remove_member_by_agent_id(team_name: &str, agent_id: &str) -> bool {
    let mut team_file = match read_team_file(team_name) {
        Some(tf) => tf,
        None => return false,
    };

    let member_index = team_file
        .members
        .iter()
        .position(|m| m.agent_id == agent_id);

    let Some(idx) = member_index else {
        return false;
    };

    team_file.members.remove(idx);
    write_team_file(team_name, &team_file).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("My Team!"), "my-team-");
        assert_eq!(sanitize_name("test123"), "test123");
    }

    #[test]
    fn test_sanitize_agent_name() {
        assert_eq!(sanitize_agent_name("agent@team"), "agent-team");
    }
}

/// 对应 TS `Input`：teamHelpers 模块导出的输入类型别名（JSON-shaped）。
pub type Input = serde_json::Value;
/// 对应 TS `Output`：teamHelpers 模块导出的输出类型别名（JSON-shaped）。
pub type Output = serde_json::Value;
