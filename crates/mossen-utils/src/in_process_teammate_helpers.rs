//! # in_process_teammate_helpers — 进程内 Teammate 辅助函数
//!
//! 对应 TypeScript `utils/inProcessTeammateHelpers.ts`。
//! 提供进程内 teammate 集成的辅助功能：
//! - 按代理名查找任务 ID
//! - 处理计划审批响应
//! - 更新 awaitingPlanApproval 状态
//! - 检测权限相关消息

use std::collections::HashMap;

/// 进程内 Teammate 任务状态
#[derive(Debug, Clone)]
pub struct InProcessTeammateTaskState {
    pub id: String,
    pub agent_name: String,
    pub awaiting_plan_approval: bool,
}

/// 任务标识信息
pub trait TaskIdentity {
    fn id(&self) -> &str;
    fn agent_name(&self) -> &str;
    fn is_in_process_teammate(&self) -> bool;
}

/// 计划审批响应消息
#[derive(Debug, Clone)]
pub struct PlanApprovalResponseMessage {
    pub approved: bool,
    pub permission_mode: Option<String>,
}

/// 按代理名查找进程内 teammate 的任务 ID。
///
/// # 参数
/// - `agent_name`: 代理名称（如 "researcher"）
/// - `tasks`: 当前任务映射
///
/// # 返回
/// 如果找到则返回任务 ID
pub fn find_in_process_teammate_task_id<T: TaskIdentity>(
    agent_name: &str,
    tasks: &HashMap<String, T>,
) -> Option<String> {
    for task in tasks.values() {
        if task.is_in_process_teammate() && task.agent_name() == agent_name {
            return Some(task.id().to_string());
        }
    }
    None
}

/// 设置进程内 teammate 的 awaitingPlanApproval 状态。
///
/// # 参数
/// - `task_id`: 进程内 teammate 的任务 ID
/// - `tasks`: 可变任务映射
/// - `awaiting`: 是否等待计划审批
pub fn set_awaiting_plan_approval(
    task_id: &str,
    tasks: &mut HashMap<String, InProcessTeammateTaskState>,
    awaiting: bool,
) {
    if let Some(task) = tasks.get_mut(task_id) {
        task.awaiting_plan_approval = awaiting;
    }
}

/// 处理进程内 teammate 的计划审批响应。
///
/// 当消息回调收到 plan_approval_response 时调用。
/// 将 awaitingPlanApproval 重置为 false。
/// 响应中的 permissionMode 由代理循环另行处理。
///
/// # 参数
/// - `task_id`: 进程内 teammate 的任务 ID
/// - `_response`: 计划审批响应消息（为未来使用保留）
/// - `tasks`: 可变任务映射
pub fn handle_plan_approval_response(
    task_id: &str,
    _response: &PlanApprovalResponseMessage,
    tasks: &mut HashMap<String, InProcessTeammateTaskState>,
) {
    set_awaiting_plan_approval(task_id, tasks, false);
}

// ============ Permission Delegation Helpers ============

/// 权限响应前缀
const PERMISSION_RESPONSE_PREFIX: &str = "permission_response:";
/// 沙箱权限响应前缀
const SANDBOX_PERMISSION_RESPONSE_PREFIX: &str = "sandbox_permission_response:";

/// 检查消息文本是否为权限响应
pub fn is_permission_response(message_text: &str) -> bool {
    message_text.starts_with(PERMISSION_RESPONSE_PREFIX)
}

/// 检查消息文本是否为沙箱权限响应
pub fn is_sandbox_permission_response(message_text: &str) -> bool {
    message_text.starts_with(SANDBOX_PERMISSION_RESPONSE_PREFIX)
}

/// 检查消息是否为权限相关响应。
///
/// 由进程内 teammate 消息处理程序使用，检测和处理
/// 来自团队领导的权限响应。
///
/// 处理工具权限和沙箱（网络主机）权限。
///
/// # 参数
/// - `message_text`: 要检查的原始消息文本
///
/// # 返回
/// 如果消息是权限响应则返回 true
pub fn is_permission_related_response(message_text: &str) -> bool {
    is_permission_response(message_text) || is_sandbox_permission_response(message_text)
}
