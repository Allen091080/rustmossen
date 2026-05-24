//! # System Prompt Sections (systemPromptSections.ts)
//!
//! System prompt section 类型和解析函数。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// Compute function type for system prompt sections.
pub type ComputeFn =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

/// A system prompt section definition.
pub struct SystemPromptSection {
    pub name: String,
    pub compute: ComputeFn,
    pub cache_break: bool,
}

/// Create a memoized system prompt section.
/// Computed once, cached until /clear or /compact.
pub fn system_prompt_section(name: &str, compute: ComputeFn) -> SystemPromptSection {
    SystemPromptSection {
        name: name.to_string(),
        compute,
        cache_break: false,
    }
}

/// Create a volatile system prompt section that recomputes every turn.
/// This WILL break the prompt cache when the value changes.
/// Requires a reason explaining why cache-breaking is necessary.
pub fn dangerous_uncached_system_prompt_section(
    name: &str,
    compute: ComputeFn,
    _reason: &str,
) -> SystemPromptSection {
    SystemPromptSection {
        name: name.to_string(),
        compute,
        cache_break: true,
    }
}

/// Resolve all system prompt sections, returning prompt strings.
pub async fn resolve_system_prompt_sections(
    sections: &[SystemPromptSection],
    cache: &mut HashMap<String, Option<String>>,
) -> Vec<Option<String>> {
    let mut results = Vec::with_capacity(sections.len());
    for s in sections {
        if !s.cache_break {
            if let Some(cached) = cache.get(&s.name) {
                results.push(cached.clone());
                continue;
            }
        }
        let value = (s.compute)().await;
        cache.insert(s.name.clone(), value.clone());
        results.push(value);
    }
    results
}

/// Clear all system prompt section state. Called on /clear and /compact.
/// Also resets beta header latches so a fresh conversation gets fresh
/// evaluation of AFK/fast-mode/cache-editing headers.
pub fn clear_system_prompt_sections(cache: &mut HashMap<String, Option<String>>) {
    cache.clear();
}
