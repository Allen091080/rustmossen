//! # cost_tracker — 成本追踪
//!
//! 对应 TS `cost-tracker.ts`，负责 Token 用量统计与 API 调用成本计算。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::ApiUsage;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 每 1M input token 成本（USD），按模型分。
const DEFAULT_INPUT_COST_PER_MILLION: f64 = 3.0;
/// 每 1M output token 成本（USD），按模型分。
const DEFAULT_OUTPUT_COST_PER_MILLION: f64 = 15.0;
/// 缓存读取折扣因子。
const CACHE_READ_DISCOUNT: f64 = 0.1;
/// 缓存创建附加因子。
const CACHE_CREATION_SURCHARGE: f64 = 1.25;

// ---------------------------------------------------------------------------
// 模型用量
// ---------------------------------------------------------------------------

/// 每个模型的使用量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub context_window: u64,
    pub max_output_tokens: u64,
}

// ---------------------------------------------------------------------------
// 成本状态
// ---------------------------------------------------------------------------

/// 持久化到项目配置的成本状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostState {
    pub total_cost_usd: f64,
    pub total_api_duration_ms: u64,
    pub total_api_duration_without_retries_ms: u64,
    pub total_tool_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub last_duration_ms: Option<u64>,
    pub model_usage: HashMap<String, ModelUsage>,
}

/// 持久化到项目配置的存储格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCostState {
    pub total_cost_usd: f64,
    pub total_api_duration: u64,
    pub total_api_duration_without_retries: u64,
    pub total_tool_duration: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub last_duration: Option<u64>,
    pub model_usage: Option<HashMap<String, ModelUsage>>,
}

// ---------------------------------------------------------------------------
// 成本计算
// ---------------------------------------------------------------------------

/// 计算一次 API 调用的 USD 成本。
pub fn calculate_usd_cost(model: &str, usage: &ApiUsage) -> f64 {
    let (input_rate, output_rate) = cost_rates_for_model(model);

    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * output_rate;
    let cache_read_cost = (usage.cache_read_input_tokens.unwrap_or(0) as f64 / 1_000_000.0)
        * input_rate
        * CACHE_READ_DISCOUNT;
    let cache_create_cost = (usage.cache_creation_input_tokens.unwrap_or(0) as f64 / 1_000_000.0)
        * input_rate
        * CACHE_CREATION_SURCHARGE;

    input_cost + output_cost + cache_read_cost + cache_create_cost
}

/// 获取模型的成本费率（input, output）每 1M token。
fn cost_rates_for_model(model: &str) -> (f64, f64) {
    // 按模型族匹配
    if model.contains("opus") {
        (15.0, 75.0)
    } else if model.contains("sonnet") {
        (3.0, 15.0)
    } else if model.contains("haiku") {
        (0.25, 1.25)
    } else {
        (
            DEFAULT_INPUT_COST_PER_MILLION,
            DEFAULT_OUTPUT_COST_PER_MILLION,
        )
    }
}

/// 累加到总会话成本。
pub fn add_to_total_session_cost(
    cost: f64,
    usage: &ApiUsage,
    model: &str,
    state: &mut CostState,
) -> f64 {
    let model_usage = state.model_usage.entry(model.to_string()).or_default();
    model_usage.input_tokens += usage.input_tokens;
    model_usage.output_tokens += usage.output_tokens;
    model_usage.cache_read_input_tokens += usage.cache_read_input_tokens.unwrap_or(0);
    model_usage.cache_creation_input_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
    model_usage.cost_usd += cost;
    state.total_cost_usd += cost;

    cost
}

/// 记录 API 调用持续时间。
pub fn record_api_duration(state: &mut CostState, duration_ms: u64, is_retry: bool) {
    state.total_api_duration_ms += duration_ms;
    if !is_retry {
        state.total_api_duration_without_retries_ms += duration_ms;
    }
    state.last_duration_ms = Some(duration_ms);
}

/// 记录工具执行持续时间。
pub fn record_tool_duration(state: &mut CostState, duration_ms: u64) {
    state.total_tool_duration_ms += duration_ms;
}

/// 记录代码变更行数。
pub fn record_line_changes(state: &mut CostState, added: u64, removed: u64) {
    state.total_lines_added += added;
    state.total_lines_removed += removed;
}

/// 转换为存储格式。
impl From<&CostState> for StoredCostState {
    fn from(s: &CostState) -> Self {
        Self {
            total_cost_usd: s.total_cost_usd,
            total_api_duration: s.total_api_duration_ms,
            total_api_duration_without_retries: s.total_api_duration_without_retries_ms,
            total_tool_duration: s.total_tool_duration_ms,
            total_lines_added: s.total_lines_added,
            total_lines_removed: s.total_lines_removed,
            last_duration: s.last_duration_ms,
            model_usage: Some(s.model_usage.clone()),
        }
    }
}

/// 从存储格式恢复。
impl From<StoredCostState> for CostState {
    fn from(s: StoredCostState) -> Self {
        Self {
            total_cost_usd: s.total_cost_usd,
            total_api_duration_ms: s.total_api_duration,
            total_api_duration_without_retries_ms: s.total_api_duration_without_retries,
            total_tool_duration_ms: s.total_tool_duration,
            total_lines_added: s.total_lines_added,
            total_lines_removed: s.total_lines_removed,
            last_duration_ms: s.last_duration,
            model_usage: s.model_usage.unwrap_or_default(),
        }
    }
}

/// 格式化成本为人类可读字符串。
pub fn format_cost(cost_usd: f64) -> String {
    if cost_usd < 0.01 {
        format!("${:.4}", cost_usd)
    } else {
        format!("${:.2}", cost_usd)
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `cost-tracker.ts` exports.
// ---------------------------------------------------------------------------

use std::sync::Mutex;
use once_cell::sync::Lazy;

/// `cost-tracker.ts` `StoredSessionCosts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredSessionCosts {
    pub session_id: String,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub api_durations_ms: u64,
}

static SESSION_COST_STORE: Lazy<Mutex<HashMap<String, StoredSessionCosts>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// `cost-tracker.ts` `getStoredSessionCosts`.
pub fn get_stored_session_costs(session_id: &str) -> Option<StoredSessionCosts> {
    SESSION_COST_STORE.lock().unwrap().get(session_id).cloned()
}

/// `cost-tracker.ts` `restoreCostStateForSession`.
pub fn restore_cost_state_for_session(session_id: &str) -> bool {
    SESSION_COST_STORE.lock().unwrap().contains_key(session_id)
}

/// `cost-tracker.ts` `saveCurrentSessionCosts`.
pub fn save_current_session_costs(session_id: &str, costs: StoredSessionCosts) {
    SESSION_COST_STORE
        .lock()
        .unwrap()
        .insert(session_id.to_string(), costs);
}

/// `cost-tracker.ts` `formatTotalCost`.
pub fn format_total_cost(total_usd: f64) -> String {
    format_cost(total_usd)
}

/// `cost-tracker.ts` `addToTotalSessionCost` — per-session ledger update
/// keyed by session id.
pub fn add_to_total_session_cost_for(session_id: &str, additional_usd: f64) {
    let mut store = SESSION_COST_STORE.lock().unwrap();
    let entry = store
        .entry(session_id.to_string())
        .or_insert(StoredSessionCosts {
            session_id: session_id.to_string(),
            ..Default::default()
        });
    entry.total_cost_usd += additional_usd;
}
