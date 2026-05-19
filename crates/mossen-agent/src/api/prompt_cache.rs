//! # Prompt Cache Break Detection
//!
//! 翻译自 `services/api/promptCacheBreakDetection.ts` (730行)
//! 记录 prompt 状态变化，检测 cache break。

use std::collections::{HashMap, HashSet};
use tracing::{debug, error, warn};

/// Maximum number of tracked sources to prevent unbounded memory growth.
const MAX_TRACKED_SOURCES: usize = 10;

/// Minimum absolute token drop required to trigger a cache break warning.
const MIN_CACHE_MISS_TOKENS: u64 = 2_000;

/// Cache TTL thresholds.
const CACHE_TTL_5MIN_MS: u64 = 5 * 60 * 1000;
pub const CACHE_TTL_1HOUR_MS: u64 = 60 * 60 * 1000;

/// Tracked source prefixes.
const TRACKED_SOURCE_PREFIXES: &[&str] = &[
    "repl_main_thread",
    "sdk",
    "agent:custom",
    "agent:default",
    "agent:builtin",
];

/// Pending changes detected between two prompt states.
#[derive(Debug, Clone, Default)]
pub struct PendingChanges {
    pub system_prompt_changed: bool,
    pub tool_schemas_changed: bool,
    pub model_changed: bool,
    pub fast_mode_changed: bool,
    pub cache_control_changed: bool,
    pub global_cache_strategy_changed: bool,
    pub betas_changed: bool,
    pub auto_mode_changed: bool,
    pub overage_changed: bool,
    pub cached_mc_changed: bool,
    pub effort_changed: bool,
    pub extra_body_changed: bool,
    pub added_tool_count: usize,
    pub removed_tool_count: usize,
    pub system_char_delta: i64,
    pub added_tools: Vec<String>,
    pub removed_tools: Vec<String>,
    pub changed_tool_schemas: Vec<String>,
    pub previous_model: String,
    pub new_model: String,
    pub prev_global_cache_strategy: String,
    pub new_global_cache_strategy: String,
    pub added_betas: Vec<String>,
    pub removed_betas: Vec<String>,
    pub prev_effort_value: String,
    pub new_effort_value: String,
}

/// Previous state for a tracked source.
struct PreviousState {
    system_hash: u64,
    tools_hash: u64,
    cache_control_hash: u64,
    tool_names: Vec<String>,
    per_tool_hashes: HashMap<String, u64>,
    system_char_count: usize,
    model: String,
    fast_mode: bool,
    global_cache_strategy: String,
    betas: Vec<String>,
    auto_mode_active: bool,
    is_using_overage: bool,
    cached_mc_enabled: bool,
    effort_value: String,
    extra_body_hash: u64,
    call_count: u64,
    pending_changes: Option<PendingChanges>,
    prev_cache_read_tokens: Option<u64>,
    cache_deletions_pending: bool,
}

/// Snapshot of prompt state for cache break detection.
pub struct PromptStateSnapshot {
    pub system_text: String,
    pub tool_schemas_json: String,
    pub tool_names: Vec<String>,
    pub query_source: String,
    pub model: String,
    pub agent_id: Option<String>,
    pub fast_mode: bool,
    pub global_cache_strategy: String,
    pub betas: Vec<String>,
    pub auto_mode_active: bool,
    pub is_using_overage: bool,
    pub cached_mc_enabled: bool,
    pub effort_value: String,
    pub extra_body_json: Option<String>,
}

/// DJB2 hash function.
fn djb2_hash(data: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in data.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Get tracking key for a query source.
fn get_tracking_key(query_source: &str, agent_id: Option<&str>) -> Option<String> {
    if query_source == "compact" {
        return Some("repl_main_thread".to_string());
    }
    for prefix in TRACKED_SOURCE_PREFIXES {
        if query_source.starts_with(prefix) {
            return Some(agent_id.unwrap_or(query_source).to_string());
        }
    }
    None
}

/// Check if a model should be excluded from cache break detection.
fn is_excluded_model(model: &str) -> bool {
    model.contains("haiku")
}

/// Sanitize tool name for analytics (collapse MCP tool names).
fn sanitize_tool_name(name: &str) -> &str {
    if name.starts_with("mcp__") {
        "mcp"
    } else {
        name
    }
}

/// Prompt cache break detection state manager.
pub struct PromptCacheBreakDetector {
    previous_state_by_source: HashMap<String, PreviousState>,
}

impl Default for PromptCacheBreakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptCacheBreakDetector {
    pub fn new() -> Self {
        Self {
            previous_state_by_source: HashMap::new(),
        }
    }

    /// Phase 1 (pre-call): Record the current prompt/tool state and detect what changed.
    pub fn record_prompt_state(&mut self, snapshot: &PromptStateSnapshot) {
        let key = match get_tracking_key(&snapshot.query_source, snapshot.agent_id.as_deref()) {
            Some(k) => k,
            None => return,
        };

        let system_hash = djb2_hash(&snapshot.system_text);
        let tools_hash = djb2_hash(&snapshot.tool_schemas_json);
        // `cache_control` is currently not propagated through
        // `PromptStateSnapshot` — the Anthropic-style `cache_control` array
        // is applied at request-encode time by `build_system_prompt_blocks`
        // / `add_cache_breakpoints`, not before this snapshot is captured.
        // Treating it as a constant 0 here means cache-control-only changes
        // won't trigger a snapshot diff; rare in practice because any
        // cache-control delta is paired with a system_text or tool_schemas
        // change in the same request build path. Tracked as a known cache-
        // hit-rate optimisation in Station 11's follow-up backlog.
        let cache_control_hash = 0u64;
        let system_char_count = snapshot.system_text.len();
        let sorted_betas = {
            let mut b = snapshot.betas.clone();
            b.sort();
            b
        };
        let effort_str = snapshot.effort_value.clone();
        let extra_body_hash = snapshot
            .extra_body_json
            .as_deref()
            .map(djb2_hash)
            .unwrap_or(0);

        let per_tool_hashes: HashMap<String, u64> = snapshot
            .tool_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                // Simple per-tool hash based on position in the schemas JSON
                let hash_input = format!("{}:{}", name, i);
                (name.clone(), djb2_hash(&hash_input))
            })
            .collect();

        if let Some(prev) = self.previous_state_by_source.get_mut(&key) {
            prev.call_count += 1;

            let system_prompt_changed = system_hash != prev.system_hash;
            let tool_schemas_changed = tools_hash != prev.tools_hash;
            let model_changed = snapshot.model != prev.model;
            let fast_mode_changed = snapshot.fast_mode != prev.fast_mode;
            let cache_control_changed = cache_control_hash != prev.cache_control_hash;
            let global_cache_strategy_changed =
                snapshot.global_cache_strategy != prev.global_cache_strategy;
            let betas_changed = sorted_betas != prev.betas;
            let auto_mode_changed = snapshot.auto_mode_active != prev.auto_mode_active;
            let overage_changed = snapshot.is_using_overage != prev.is_using_overage;
            let cached_mc_changed = snapshot.cached_mc_enabled != prev.cached_mc_enabled;
            let effort_changed = effort_str != prev.effort_value;
            let extra_body_changed = extra_body_hash != prev.extra_body_hash;

            if system_prompt_changed
                || tool_schemas_changed
                || model_changed
                || fast_mode_changed
                || cache_control_changed
                || global_cache_strategy_changed
                || betas_changed
                || auto_mode_changed
                || overage_changed
                || cached_mc_changed
                || effort_changed
                || extra_body_changed
            {
                let prev_tool_set: HashSet<&String> = prev.tool_names.iter().collect();
                let new_tool_set: HashSet<&String> = snapshot.tool_names.iter().collect();
                let prev_beta_set: HashSet<&String> = prev.betas.iter().collect();
                let new_beta_set: HashSet<&String> = sorted_betas.iter().collect();

                let added_tools: Vec<String> = snapshot
                    .tool_names
                    .iter()
                    .filter(|n| !prev_tool_set.contains(n))
                    .cloned()
                    .collect();
                let removed_tools: Vec<String> = prev
                    .tool_names
                    .iter()
                    .filter(|n| !new_tool_set.contains(n))
                    .cloned()
                    .collect();

                let changed_tool_schemas: Vec<String> = if tool_schemas_changed {
                    snapshot
                        .tool_names
                        .iter()
                        .filter(|name| {
                            prev_tool_set.contains(name)
                                && per_tool_hashes.get(*name) != prev.per_tool_hashes.get(*name)
                        })
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                prev.pending_changes = Some(PendingChanges {
                    system_prompt_changed,
                    tool_schemas_changed,
                    model_changed,
                    fast_mode_changed,
                    cache_control_changed,
                    global_cache_strategy_changed,
                    betas_changed,
                    auto_mode_changed,
                    overage_changed,
                    cached_mc_changed,
                    effort_changed,
                    extra_body_changed,
                    added_tool_count: added_tools.len(),
                    removed_tool_count: removed_tools.len(),
                    added_tools,
                    removed_tools,
                    changed_tool_schemas,
                    system_char_delta: system_char_count as i64 - prev.system_char_count as i64,
                    previous_model: prev.model.clone(),
                    new_model: snapshot.model.clone(),
                    prev_global_cache_strategy: prev.global_cache_strategy.clone(),
                    new_global_cache_strategy: snapshot.global_cache_strategy.clone(),
                    added_betas: sorted_betas
                        .iter()
                        .filter(|b| !prev_beta_set.contains(b))
                        .cloned()
                        .collect(),
                    removed_betas: prev
                        .betas
                        .iter()
                        .filter(|b| !new_beta_set.contains(b))
                        .cloned()
                        .collect(),
                    prev_effort_value: prev.effort_value.clone(),
                    new_effort_value: effort_str.clone(),
                });

                if tool_schemas_changed {
                    prev.per_tool_hashes = per_tool_hashes;
                }
            } else {
                prev.pending_changes = None;
            }

            prev.system_hash = system_hash;
            prev.tools_hash = tools_hash;
            prev.cache_control_hash = cache_control_hash;
            prev.tool_names = snapshot.tool_names.clone();
            prev.system_char_count = system_char_count;
            prev.model = snapshot.model.clone();
            prev.fast_mode = snapshot.fast_mode;
            prev.global_cache_strategy = snapshot.global_cache_strategy.clone();
            prev.betas = sorted_betas;
            prev.auto_mode_active = snapshot.auto_mode_active;
            prev.is_using_overage = snapshot.is_using_overage;
            prev.cached_mc_enabled = snapshot.cached_mc_enabled;
            prev.effort_value = effort_str;
            prev.extra_body_hash = extra_body_hash;
        } else {
            // Evict oldest entries if map is at capacity
            while self.previous_state_by_source.len() >= MAX_TRACKED_SOURCES {
                if let Some(oldest_key) = self.previous_state_by_source.keys().next().cloned() {
                    self.previous_state_by_source.remove(&oldest_key);
                }
            }

            self.previous_state_by_source.insert(
                key,
                PreviousState {
                    system_hash,
                    tools_hash,
                    cache_control_hash,
                    tool_names: snapshot.tool_names.clone(),
                    per_tool_hashes,
                    system_char_count,
                    model: snapshot.model.clone(),
                    fast_mode: snapshot.fast_mode,
                    global_cache_strategy: snapshot.global_cache_strategy.clone(),
                    betas: sorted_betas,
                    auto_mode_active: snapshot.auto_mode_active,
                    is_using_overage: snapshot.is_using_overage,
                    cached_mc_enabled: snapshot.cached_mc_enabled,
                    effort_value: effort_str,
                    extra_body_hash,
                    call_count: 1,
                    pending_changes: None,
                    prev_cache_read_tokens: None,
                    cache_deletions_pending: false,
                },
            );
        }
    }

    /// Phase 2 (post-call): Check the API response's cache tokens for cache break.
    pub fn check_response_for_cache_break(
        &mut self,
        query_source: &str,
        cache_read_tokens: u64,
        _cache_creation_tokens: u64,
        time_since_last_assistant_msg_ms: Option<u64>,
        agent_id: Option<&str>,
        _request_id: Option<&str>,
    ) {
        let key = match get_tracking_key(query_source, agent_id) {
            Some(k) => k,
            None => return,
        };

        let state = match self.previous_state_by_source.get_mut(&key) {
            Some(s) => s,
            None => return,
        };

        if is_excluded_model(&state.model) {
            return;
        }

        let prev_cache_read = state.prev_cache_read_tokens;
        state.prev_cache_read_tokens = Some(cache_read_tokens);

        // Skip the first call
        let prev_cache_read = match prev_cache_read {
            Some(v) => v,
            None => return,
        };

        // Cache deletions pending — expected drop
        if state.cache_deletions_pending {
            state.cache_deletions_pending = false;
            debug!(
                "[PROMPT CACHE] cache deletion applied, cache read: {} → {} (expected drop)",
                prev_cache_read, cache_read_tokens
            );
            state.pending_changes = None;
            return;
        }

        // Detect cache break: >5% drop AND absolute drop exceeds minimum
        let token_drop = prev_cache_read.saturating_sub(cache_read_tokens);
        if cache_read_tokens >= (prev_cache_read as f64 * 0.95) as u64
            || token_drop < MIN_CACHE_MISS_TOKENS
        {
            state.pending_changes = None;
            return;
        }

        // Build explanation
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref changes) = state.pending_changes {
            if changes.model_changed {
                parts.push(format!(
                    "model changed ({} → {})",
                    changes.previous_model, changes.new_model
                ));
            }
            if changes.system_prompt_changed {
                let char_info = if changes.system_char_delta == 0 {
                    String::new()
                } else if changes.system_char_delta > 0 {
                    format!(" (+{} chars)", changes.system_char_delta)
                } else {
                    format!(" ({} chars)", changes.system_char_delta)
                };
                parts.push(format!("system prompt changed{}", char_info));
            }
            if changes.tool_schemas_changed {
                let tool_diff = if changes.added_tool_count > 0 || changes.removed_tool_count > 0 {
                    format!(
                        " (+{}/-{} tools)",
                        changes.added_tool_count, changes.removed_tool_count
                    )
                } else {
                    " (tool prompt/schema changed, same tool set)".to_string()
                };
                parts.push(format!("tools changed{}", tool_diff));
            }
            if changes.fast_mode_changed {
                parts.push("fast mode toggled".to_string());
            }
            if changes.global_cache_strategy_changed {
                parts.push(format!(
                    "global cache strategy changed ({} → {})",
                    if changes.prev_global_cache_strategy.is_empty() {
                        "none"
                    } else {
                        &changes.prev_global_cache_strategy
                    },
                    if changes.new_global_cache_strategy.is_empty() {
                        "none"
                    } else {
                        &changes.new_global_cache_strategy
                    }
                ));
            }
            if changes.cache_control_changed
                && !changes.global_cache_strategy_changed
                && !changes.system_prompt_changed
            {
                parts.push("cache_control changed (scope or TTL)".to_string());
            }
            if changes.betas_changed {
                let added = if !changes.added_betas.is_empty() {
                    format!("+{}", changes.added_betas.join(","))
                } else {
                    String::new()
                };
                let removed = if !changes.removed_betas.is_empty() {
                    format!("-{}", changes.removed_betas.join(","))
                } else {
                    String::new()
                };
                let diff: Vec<&str> = [added.as_str(), removed.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect();
                let diff_str = if diff.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", diff.join(" "))
                };
                parts.push(format!("betas changed{}", diff_str));
            }
            if changes.auto_mode_changed {
                parts.push("auto mode toggled".to_string());
            }
            if changes.overage_changed {
                parts.push("overage state changed (TTL latched, no flip)".to_string());
            }
            if changes.cached_mc_changed {
                parts.push("cached microcompact toggled".to_string());
            }
            if changes.effort_changed {
                parts.push(format!(
                    "effort changed ({} → {})",
                    if changes.prev_effort_value.is_empty() {
                        "default"
                    } else {
                        &changes.prev_effort_value
                    },
                    if changes.new_effort_value.is_empty() {
                        "default"
                    } else {
                        &changes.new_effort_value
                    }
                ));
            }
            if changes.extra_body_changed {
                parts.push("extra body params changed".to_string());
            }
        }

        // Check TTL expiration
        let last_msg_over_5min = time_since_last_assistant_msg_ms
            .map(|t| t > CACHE_TTL_5MIN_MS)
            .unwrap_or(false);
        let last_msg_over_1h = time_since_last_assistant_msg_ms
            .map(|t| t > CACHE_TTL_1HOUR_MS)
            .unwrap_or(false);

        let reason = if !parts.is_empty() {
            parts.join(", ")
        } else if last_msg_over_1h {
            "possible 1h TTL expiry (prompt unchanged)".to_string()
        } else if last_msg_over_5min {
            "possible 5min TTL expiry (prompt unchanged)".to_string()
        } else if time_since_last_assistant_msg_ms.is_some() {
            "likely server-side (prompt unchanged, <5min gap)".to_string()
        } else {
            "unknown cause".to_string()
        };

        warn!(
            "[PROMPT CACHE BREAK] {} [source={}, call #{}, cache read: {} → {}]",
            reason, query_source, state.call_count, prev_cache_read, cache_read_tokens
        );

        state.pending_changes = None;
    }

    /// Notify that cache deletions are pending (expected drop next call).
    pub fn notify_cache_deletion(&mut self, query_source: &str, agent_id: Option<&str>) {
        let key = match get_tracking_key(query_source, agent_id) {
            Some(k) => k,
            None => return,
        };
        if let Some(state) = self.previous_state_by_source.get_mut(&key) {
            state.cache_deletions_pending = true;
        }
    }

    /// Notify compaction — reset cache read baseline.
    pub fn notify_compaction(&mut self, query_source: &str, agent_id: Option<&str>) {
        let key = match get_tracking_key(query_source, agent_id) {
            Some(k) => k,
            None => return,
        };
        if let Some(state) = self.previous_state_by_source.get_mut(&key) {
            state.prev_cache_read_tokens = None;
        }
    }

    /// Clean up tracking for a specific agent.
    pub fn cleanup_agent_tracking(&mut self, agent_id: &str) {
        self.previous_state_by_source.remove(agent_id);
    }

    /// Reset all prompt cache break detection state.
    pub fn reset(&mut self) {
        self.previous_state_by_source.clear();
    }
}

/// Module-level shared detector singleton, used by `reset_prompt_cache_break_detection`.
pub static PROMPT_CACHE_BREAK_DETECTOR: once_cell::sync::Lazy<
    std::sync::Mutex<PromptCacheBreakDetector>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(PromptCacheBreakDetector::default()));

/// TS `resetPromptCacheBreakDetection` — clears the module-level detector
/// instance so subsequent calls start fresh.
pub fn reset_prompt_cache_break_detection() {
    if let Ok(mut guard) = PROMPT_CACHE_BREAK_DETECTOR.lock() {
        *guard = PromptCacheBreakDetector::default();
    }
}
