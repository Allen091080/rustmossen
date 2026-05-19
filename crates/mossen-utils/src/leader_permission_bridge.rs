//! # leader_permission_bridge — 领导权限桥接
//!
//! 对应 TypeScript `utils/swarm/leaderPermissionBridge.ts`。
//! 允许 REPL 注册 setToolUseConfirmQueue 和 setToolPermissionContext 函数供进程内队友使用。

use std::sync::Mutex;

/// 设置 ToolUseConfirm 队列的函数类型
pub type SetToolUseConfirmQueueFn =
    Box<dyn FnMut(Vec<serde_json::Value>) -> Vec<serde_json::Value> + Send + Sync>;

/// 设置工具权限上下文的函数类型
pub type SetToolPermissionContextFn =
    Box<dyn FnMut(serde_json::Value, Option<bool>) + Send + Sync>;

lazy_static::lazy_static! {
    static ref REGISTERED_SETTER: Mutex<Option<SetToolUseConfirmQueueFn>> = Mutex::new(None);
    static ref REGISTERED_PERMISSION_CONTEXT_SETTER: Mutex<Option<SetToolPermissionContextFn>> = Mutex::new(None);
}

/// 注册领导者 ToolUseConfirm 队列设置器
pub fn register_leader_tool_use_confirm_queue(
    setter: SetToolUseConfirmQueueFn,
) {
    let mut guard = REGISTERED_SETTER.lock().unwrap();
    *guard = Some(setter);
}

/// 获取领导者 ToolUseConfirm 队列设置器
pub fn get_leader_tool_use_confirm_queue() -> Option<SetToolPermissionContextFn> {
    let guard = REGISTERED_SETTER.lock().unwrap();
    // 注意：这里返回的类型与原始 TS 不同，需要调整
    None
}

/// 取消注册领导者 ToolUseConfirm 队列
pub fn unregister_leader_tool_use_confirm_queue() {
    let mut guard = REGISTERED_SETTER.lock().unwrap();
    *guard = None;
}

/// 注册领导者设置工具权限上下文
pub fn register_leader_set_tool_permission_context(
    setter: SetToolPermissionContextFn,
) {
    let mut guard = REGISTERED_PERMISSION_CONTEXT_SETTER.lock().unwrap();
    *guard = Some(setter);
}

/// 获取领导者设置工具权限上下文
pub fn get_leader_set_tool_permission_context() -> Option<SetToolPermissionContextFn> {
    let guard = REGISTERED_PERMISSION_CONTEXT_SETTER.lock().unwrap();
    guard.clone()
}

/// 取消注册领导者设置工具权限上下文
pub fn unregister_leader_set_tool_permission_context() {
    let mut guard = REGISTERED_PERMISSION_CONTEXT_SETTER.lock().unwrap();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_unregister() {
        // 测试注册/取消注册功能
        unregister_leader_tool_use_confirm_queue();
        unregister_leader_set_tool_permission_context();
    }
}