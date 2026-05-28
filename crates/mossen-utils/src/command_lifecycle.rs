//! # command_lifecycle — 命令生命周期
//!
//! 对应 TypeScript `utils/commandLifecycle.ts`。
//! 命令生命周期监听器。

use std::sync::Mutex;

/// 命令生命周期状态。
#[derive(Debug, Clone, PartialEq)]
pub enum CommandLifecycleState {
    Started,
    Completed,
}

/// 命令生命周期监听器类型。
pub type CommandLifecycleListener = Box<dyn Fn(String, CommandLifecycleState) + Send + Sync>;

static LISTENER: Mutex<Option<Box<dyn Fn(String, CommandLifecycleState) + Send + Sync>>> =
    Mutex::new(None);

/// 设置命令生命周期监听器。
pub fn set_command_lifecycle_listener(
    cb: Option<Box<dyn Fn(String, CommandLifecycleState) + Send + Sync>>,
) {
    let mut listener = LISTENER.lock().unwrap();
    *listener = cb;
}

/// 通知命令生命周期事件。
pub fn notify_command_lifecycle(uuid: String, state: CommandLifecycleState) {
    let listener = LISTENER.lock().unwrap();
    if let Some(cb) = listener.as_ref() {
        cb(uuid, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_notify() {
        let called = std::sync::Arc::new(Mutex::new(false));
        let called_clone = std::sync::Arc::clone(&called);

        set_command_lifecycle_listener(Some(Box::new(move |_, _| {
            *called_clone.lock().unwrap() = true;
        })));

        notify_command_lifecycle("test-uuid".to_string(), CommandLifecycleState::Started);

        assert!(*called.lock().unwrap());
    }
}
