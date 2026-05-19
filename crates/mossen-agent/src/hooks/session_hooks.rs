//! # session_hooks — 会话级 Hook 管理
//!
//! 对应 TS `utils/hooks/sessionHooks.ts`。
//! 管理临时、内存中的 session-scoped hooks。
//! 会话结束时自动清除。

use std::collections::HashMap;

use mossen_types::hooks::HookEvent;
use parking_lot::RwLock;
use tracing::debug;

use super::settings::{is_hook_equal, HookCommand, HookMatcher};

/// 会话 Hook 匹配器 — 包含匹配器和关联的 Hook 列表。
#[derive(Debug, Clone)]
pub struct SessionHookMatcher {
    /// 匹配器表达式。
    pub matcher: String,
    /// 可选的 skill 根目录。
    pub skill_root: Option<String>,
    /// 关联的 Hook 命令列表。
    pub hooks: Vec<HookCommand>,
}

/// 会话 Hook 存储 — 单个会话的 Hook 集合。
#[derive(Debug, Clone, Default)]
pub struct SessionHookStore {
    /// 事件 → 匹配器列表的映射。
    pub hooks: HashMap<HookEvent, Vec<SessionHookMatcher>>,
}

/// 会话 Hooks 管理器 — 管理所有会话的 Hook 注册。
///
/// 对应 TS `SessionHooksState = Map<string, SessionStore>`。
/// 使用 RwLock 保护并发访问（TS 中使用 Map 的 O(1) set/delete 优化）。
pub struct SessionHooksManager {
    /// 会话 ID → 存储的映射。
    stores: RwLock<HashMap<String, SessionHookStore>>,
}

impl SessionHooksManager {
    /// 创建新的管理器。
    pub fn new() -> Self {
        Self {
            stores: RwLock::new(HashMap::new()),
        }
    }

    /// 添加会话 Hook。
    ///
    /// 对应 TS `addSessionHook()`。
    pub fn add_session_hook(
        &self,
        session_id: &str,
        event: HookEvent,
        matcher: &str,
        hook: HookCommand,
        skill_root: Option<String>,
    ) {
        let mut stores = self.stores.write();
        let store = stores.entry(session_id.to_string()).or_default();
        let event_matchers = store.hooks.entry(event).or_default();

        // 查找已存在的匹配器
        let existing = event_matchers
            .iter_mut()
            .find(|m| m.matcher == matcher && m.skill_root == skill_root);

        if let Some(existing_matcher) = existing {
            existing_matcher.hooks.push(hook);
        } else {
            event_matchers.push(SessionHookMatcher {
                matcher: matcher.to_string(),
                skill_root,
                hooks: vec![hook],
            });
        }

        debug!(
            session_id = session_id,
            event = %event,
            "Added session hook"
        );
    }

    /// 移除会话 Hook。
    ///
    /// 对应 TS `removeSessionHook()`。
    pub fn remove_session_hook(&self, session_id: &str, event: HookEvent, hook: &HookCommand) {
        let mut stores = self.stores.write();
        let store = match stores.get_mut(session_id) {
            Some(s) => s,
            None => return,
        };

        if let Some(event_matchers) = store.hooks.get_mut(&event) {
            for matcher in event_matchers.iter_mut() {
                matcher.hooks.retain(|h| !is_hook_equal(h, hook));
            }
            event_matchers.retain(|m| !m.hooks.is_empty());
            if event_matchers.is_empty() {
                store.hooks.remove(&event);
            }
        }

        debug!(
            session_id = session_id,
            event = %event,
            "Removed session hook"
        );
    }

    /// 获取会话的所有 Hook（转为 HookMatcher 格式）。
    ///
    /// 对应 TS `getSessionHooks()`。
    pub fn get_session_hooks(
        &self,
        session_id: &str,
        event: Option<HookEvent>,
    ) -> HashMap<HookEvent, Vec<HookMatcher>> {
        let stores = self.stores.read();
        let store = match stores.get(session_id) {
            Some(s) => s,
            None => return HashMap::new(),
        };

        let mut result = HashMap::new();

        let events: Box<dyn Iterator<Item = &HookEvent>> = match event {
            Some(ref e) => Box::new(std::iter::once(e)),
            None => Box::new(store.hooks.keys()),
        };

        for evt in events {
            if let Some(matchers) = store.hooks.get(evt) {
                let hook_matchers: Vec<HookMatcher> = matchers
                    .iter()
                    .map(|sm| HookMatcher {
                        matcher: Some(sm.matcher.clone()),
                        hooks: sm.hooks.clone(),
                    })
                    .collect();
                if !hook_matchers.is_empty() {
                    result.insert(*evt, hook_matchers);
                }
            }
        }

        result
    }

    /// 清除指定会话的所有 Hook。
    ///
    /// 对应 TS `clearSessionHooks()`。
    pub fn clear_session_hooks(&self, session_id: &str) {
        let mut stores = self.stores.write();
        stores.remove(session_id);
        debug!(session_id = session_id, "Cleared all session hooks");
    }

    /// 获取注册的会话数量。
    pub fn session_count(&self) -> usize {
        self.stores.read().len()
    }
}

impl Default for SessionHooksManager {
    fn default() -> Self {
        Self::new()
    }
}
