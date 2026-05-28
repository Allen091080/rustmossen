//! # registry — HookRegistry 事件→处理器映射
//!
//! 对应 TS `utils/hooks/hooksConfigManager.ts` 中的
//! `groupHooksByEventAndMatcher()`、`getSortedMatchersForEvent()`、`getHooksForMatcher()`。
//!
//! 按文档 12 命名：
//! - `groupHooksByEventAndMatcher` → `index_watchers()`
//! - `getSortedMatchersForEvent` → `ranked_filters_for()`
//! - `getHooksForMatcher` → `watchers_for_filter()`

use std::collections::HashMap;

use mossen_types::hooks::HookEvent;

use super::settings::{HookSource, IndividualHookConfig};

/// HookRegistry — 按事件和匹配器索引的 Hook 注册表。
///
/// 对应 TS `groupHooksByEventAndMatcher()` 返回的数据结构。
#[derive(Debug, Clone)]
pub struct HookRegistry {
    /// 事件 → (匹配器 → Hook 配置列表) 的嵌套映射。
    grouped: HashMap<HookEvent, HashMap<String, Vec<IndividualHookConfig>>>,
}

impl HookRegistry {
    /// 创建空的 HookRegistry。
    pub fn new() -> Self {
        Self {
            grouped: HashMap::new(),
        }
    }

    /// 索引所有观察者 — 按事件和匹配器分组 Hook。
    ///
    /// 对应 TS `groupHooksByEventAndMatcher()` → Rust `index_watchers()`。
    pub fn index_watchers(hooks: Vec<IndividualHookConfig>) -> Self {
        let mut grouped: HashMap<HookEvent, HashMap<String, Vec<IndividualHookConfig>>> =
            HashMap::new();

        for hook in hooks {
            let matcher_key = hook.matcher.clone().unwrap_or_default();
            grouped
                .entry(hook.event)
                .or_default()
                .entry(matcher_key)
                .or_default()
                .push(hook);
        }

        Self { grouped }
    }

    /// 获取事件的排序过滤器列表。
    ///
    /// 对应 TS `getSortedMatchersForEvent()` → Rust `ranked_filters_for()`。
    pub fn ranked_filters_for(&self, event: HookEvent) -> Vec<String> {
        let event_group = match self.grouped.get(&event) {
            Some(g) => g,
            None => return vec![],
        };

        let mut matchers: Vec<String> = event_group.keys().cloned().collect();
        matchers.sort_by(|a, b| {
            let a_hooks = event_group.get(a).map(|h| h.as_slice()).unwrap_or(&[]);
            let b_hooks = event_group.get(b).map(|h| h.as_slice()).unwrap_or(&[]);

            let a_priority = a_hooks
                .iter()
                .map(|h| source_priority(h.source))
                .min()
                .unwrap_or(999);
            let b_priority = b_hooks
                .iter()
                .map(|h| source_priority(h.source))
                .min()
                .unwrap_or(999);

            a_priority.cmp(&b_priority).then_with(|| a.cmp(b))
        });

        matchers
    }

    /// 获取指定事件和匹配器的 Hook 列表。
    ///
    /// 对应 TS `getHooksForMatcher()` → Rust `watchers_for_filter()`。
    pub fn watchers_for_filter(
        &self,
        event: HookEvent,
        matcher: Option<&str>,
    ) -> Vec<&IndividualHookConfig> {
        let matcher_key = matcher.unwrap_or("");
        self.grouped
            .get(&event)
            .and_then(|event_group| event_group.get(matcher_key))
            .map(|hooks| hooks.iter().collect())
            .unwrap_or_default()
    }

    /// 获取指定事件的所有 Hook（所有匹配器扁平化）。
    pub fn all_hooks_for_event(&self, event: HookEvent) -> Vec<&IndividualHookConfig> {
        self.grouped
            .get(&event)
            .map(|event_group| event_group.values().flatten().collect())
            .unwrap_or_default()
    }

    /// 注册一个 Hook。
    pub fn register(&mut self, hook: IndividualHookConfig) {
        let matcher_key = hook.matcher.clone().unwrap_or_default();
        self.grouped
            .entry(hook.event)
            .or_default()
            .entry(matcher_key)
            .or_default()
            .push(hook);
    }

    /// 检查注册表是否为空。
    pub fn is_empty(&self) -> bool {
        self.grouped
            .values()
            .all(|m| m.values().all(|h| h.is_empty()))
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 来源优先级（数字越小优先级越高）。
fn source_priority(source: HookSource) -> u32 {
    match source {
        HookSource::UserSettings => 0,
        HookSource::ProjectSettings => 1,
        HookSource::LocalSettings => 2,
        HookSource::PolicySettings => 3,
        HookSource::SessionHook => 4,
        HookSource::PluginHook => 999,
        HookSource::BuiltinHook => 999,
    }
}
