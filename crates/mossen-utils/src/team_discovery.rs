//! # team_discovery — 团队发现工具
//!
//! 对应 TypeScript `utils/teamDiscovery.ts`。
//! 扫描 ~/.mossen/teams/ 查找当前会话作为 leader 的团队。

use serde::{Deserialize, Serialize};

/// 面板后端类型
pub type PaneBackendType = String;

/// 团队摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSummary {
    pub name: String,
    pub member_count: usize,
    pub running_count: usize,
    pub idle_count: usize,
}

/// 队友状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeammateStatusKind {
    Running,
    Idle,
    Unknown,
}

/// 队友状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeammateStatus {
    pub name: String,
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub status: TeammateStatusKind,
    pub color: Option<String>,
    pub idle_since: Option<String>,
    pub tmux_pane_id: String,
    pub cwd: String,
    pub worktree_path: Option<String>,
    pub is_hidden: Option<bool>,
    pub backend_type: Option<PaneBackendType>,
    pub mode: Option<String>,
}

/// 团队成员结构（来自团队文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub is_active: Option<bool>,
    pub color: Option<String>,
    pub tmux_pane_id: String,
    pub cwd: String,
    pub worktree_path: Option<String>,
    pub backend_type: Option<String>,
    pub mode: Option<String>,
}

/// 团队文件结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamFile {
    pub members: Vec<TeamMember>,
    pub hidden_pane_ids: Option<Vec<String>>,
}

/// 检查字符串是否是有效的面板后端类型
pub fn is_pane_backend(_s: &str) -> bool {
    // 在实际实现中验证后端类型
    true
}

/// 读取团队文件 — 从 `<config_home>/teams/<sanitized_name>/config.json` 读取并反序列化。
///
/// 与 TS `readTeamFile` 对齐：忽略 JSON 解析与 IO 错误，返回 `None` 以便上层
/// 当作 "无团队配置" 处理。
pub fn read_team_file(team_name: &str) -> Option<TeamFile> {
    let path = crate::team_helpers::get_team_file_path(team_name);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 获取团队详细的队友状态列表
pub fn get_teammate_statuses(team_name: &str) -> Vec<TeammateStatus> {
    let team_file = match read_team_file(team_name) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let hidden_pane_ids: std::collections::HashSet<&str> = team_file
        .hidden_pane_ids
        .as_ref()
        .map(|ids| ids.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    let mut statuses = Vec::new();

    for member in &team_file.members {
        // 从列表中排除 team-lead
        if member.name == "team-lead" {
            continue;
        }

        // 从配置中读取 isActive，未定义时默认为 true（活动）
        let is_active = member.is_active.unwrap_or(true);
        let status = if is_active {
            TeammateStatusKind::Running
        } else {
            TeammateStatusKind::Idle
        };

        let backend_type = member
            .backend_type
            .as_ref()
            .filter(|bt| is_pane_backend(bt))
            .cloned();

        statuses.push(TeammateStatus {
            name: member.name.clone(),
            agent_id: member.agent_id.clone(),
            agent_type: member.agent_type.clone(),
            model: member.model.clone(),
            prompt: member.prompt.clone(),
            status,
            color: member.color.clone(),
            idle_since: None,
            tmux_pane_id: member.tmux_pane_id.clone(),
            cwd: member.cwd.clone(),
            worktree_path: member.worktree_path.clone(),
            is_hidden: Some(hidden_pane_ids.contains(member.tmux_pane_id.as_str())),
            backend_type,
            mode: member.mode.clone(),
        });
    }

    statuses
}
