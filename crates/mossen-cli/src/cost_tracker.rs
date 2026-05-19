//! 成本追踪 — 对应 TS 的 cost-tracker.ts + costHook.ts。
//!
//! 追踪 API 成本、token 使用量、行变更等，支持会话恢复。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::bootstrap::{BootstrapState, BootstrapStateExtended, ModelUsage};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// API 使用量（从 API 响应中提取）。
#[derive(Debug, Clone, Default)]
pub struct ApiUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
}

/// 存储的成本状态（用于会话持久化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCostState {
    pub total_cost_usd: f64,
    pub total_api_duration: u64,
    pub total_api_duration_without_retries: u64,
    pub total_tool_duration: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub last_duration: Option<u64>,
    pub model_usage: Option<HashMap<String, StoredModelUsage>>,
}

/// 存储的模型使用量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
}

/// 项目配置（用于成本存储/恢复）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectCostConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_api_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_api_duration_without_retries: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tool_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lines_added: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lines_removed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_cache_read_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_total_web_search_requests: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fps_average: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fps_low_1_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_model_usage: Option<HashMap<String, StoredModelUsage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
}

/// FPS 度量。
#[derive(Debug, Clone)]
pub struct FpsMetrics {
    pub average_fps: f64,
    pub low_1_pct_fps: f64,
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// 格式化成本值。
pub fn format_cost(cost: f64, max_decimal_places: u32) -> String {
    if cost > 0.5 {
        let rounded = (cost * 100.0).round() / 100.0;
        format!("${:.2}", rounded)
    } else {
        format!("${:.*}", max_decimal_places as usize, cost)
    }
}

/// 格式化数字（千分位分隔）。
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// 格式化持续时间。
pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, mins % 60, secs % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}.{}s", secs, (ms % 1000) / 100)
    }
}

/// 获取模型的规范短名称。
pub fn get_canonical_name(model: &str) -> &str {
    // 简化实现：返回最后一个 - 之前的部分，或原始名称
    if let Some(pos) = model.rfind('-') {
        // 检查后缀是否是日期格式 (YYYYMMDD)
        let suffix = &model[pos + 1..];
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return &model[..pos];
        }
    }
    model
}

// ---------------------------------------------------------------------------
// Cost tracking functions
// ---------------------------------------------------------------------------

/// 从项目配置中获取存储的会话成本。
pub fn get_stored_session_costs(
    config: &ProjectCostConfig,
    session_id: &str,
) -> Option<StoredCostState> {
    if config.last_session_id.as_deref() != Some(session_id) {
        return None;
    }

    Some(StoredCostState {
        total_cost_usd: config.last_cost.unwrap_or(0.0),
        total_api_duration: config.last_api_duration.unwrap_or(0),
        total_api_duration_without_retries: config.last_api_duration_without_retries.unwrap_or(0),
        total_tool_duration: config.last_tool_duration.unwrap_or(0),
        total_lines_added: config.last_lines_added.unwrap_or(0),
        total_lines_removed: config.last_lines_removed.unwrap_or(0),
        last_duration: config.last_duration,
        model_usage: config.last_model_usage.clone(),
    })
}

/// 恢复会话的成本状态。
pub fn restore_cost_state_for_session(
    config: &ProjectCostConfig,
    session_id: &str,
    state: &mut BootstrapState,
    ext: &mut BootstrapStateExtended,
) -> bool {
    let data = match get_stored_session_costs(config, session_id) {
        Some(d) => d,
        None => return false,
    };

    state.total_cost_usd = data.total_cost_usd;
    state.total_api_duration_ms = data.total_api_duration;
    ext.total_api_duration_without_retries = data.total_api_duration_without_retries;
    state.total_tool_duration_ms = data.total_tool_duration;
    state.total_lines_added = data.total_lines_added;
    state.total_lines_removed = data.total_lines_removed;

    if let Some(usage_map) = data.model_usage {
        for (model, usage) in usage_map {
            ext.model_usage.insert(
                model,
                ModelUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_read_input_tokens: usage.cache_read_input_tokens,
                    cache_creation_input_tokens: usage.cache_creation_input_tokens,
                    web_search_requests: usage.web_search_requests,
                    cost_usd: usage.cost_usd,
                    context_window: 0,
                    max_output_tokens: 0,
                },
            );
        }
    }

    if let Some(dur) = data.last_duration {
        let now = chrono::Utc::now().timestamp_millis();
        state.start_time = now - dur as i64;
    }

    true
}

/// 保存当前会话的成本到项目配置。
pub fn save_current_session_costs(
    state: &BootstrapState,
    ext: &BootstrapStateExtended,
    fps_metrics: Option<&FpsMetrics>,
) -> ProjectCostConfig {
    let total_duration = state.total_duration_ms();

    let model_usage: HashMap<String, StoredModelUsage> = ext
        .model_usage
        .iter()
        .map(|(model, usage)| {
            (
                model.clone(),
                StoredModelUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_read_input_tokens: usage.cache_read_input_tokens,
                    cache_creation_input_tokens: usage.cache_creation_input_tokens,
                    web_search_requests: usage.web_search_requests,
                    cost_usd: usage.cost_usd,
                },
            )
        })
        .collect();

    ProjectCostConfig {
        last_cost: Some(state.total_cost_usd),
        last_api_duration: Some(state.total_api_duration_ms),
        last_api_duration_without_retries: Some(ext.total_api_duration_without_retries),
        last_tool_duration: Some(state.total_tool_duration_ms),
        last_duration: Some(total_duration),
        last_lines_added: Some(state.total_lines_added),
        last_lines_removed: Some(state.total_lines_removed),
        last_total_input_tokens: Some(ext.get_total_input_tokens()),
        last_total_output_tokens: Some(ext.get_total_output_tokens()),
        last_total_cache_creation_input_tokens: Some(ext.get_total_cache_creation_input_tokens()),
        last_total_cache_read_input_tokens: Some(ext.get_total_cache_read_input_tokens()),
        last_total_web_search_requests: Some(ext.get_total_web_search_requests()),
        last_fps_average: fps_metrics.map(|m| m.average_fps),
        last_fps_low_1_pct: fps_metrics.map(|m| m.low_1_pct_fps),
        last_model_usage: if model_usage.is_empty() {
            None
        } else {
            Some(model_usage)
        },
        last_session_id: Some(state.session_id.clone()),
    }
}

/// 格式化模型使用量报告。
pub fn format_model_usage(ext: &BootstrapStateExtended) -> String {
    if ext.model_usage.is_empty() {
        return "Usage:                 0 input, 0 output, 0 cache read, 0 cache write".to_string();
    }

    // 按短名称累计使用量
    let mut by_short_name: HashMap<String, ModelUsage> = HashMap::new();
    for (model, usage) in &ext.model_usage {
        let short_name = get_canonical_name(model).to_string();
        let acc = by_short_name.entry(short_name).or_default();
        acc.input_tokens += usage.input_tokens;
        acc.output_tokens += usage.output_tokens;
        acc.cache_read_input_tokens += usage.cache_read_input_tokens;
        acc.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        acc.web_search_requests += usage.web_search_requests;
        acc.cost_usd += usage.cost_usd;
    }

    let mut result = "Usage by model:".to_string();
    for (short_name, usage) in &by_short_name {
        let usage_string = format!(
            "  {} input, {} output, {} cache read, {} cache write{}  ({})",
            format_number(usage.input_tokens),
            format_number(usage.output_tokens),
            format_number(usage.cache_read_input_tokens),
            format_number(usage.cache_creation_input_tokens),
            if usage.web_search_requests > 0 {
                format!(", {} web search", format_number(usage.web_search_requests))
            } else {
                String::new()
            },
            format_cost(usage.cost_usd, 4),
        );
        result.push('\n');
        result.push_str(&format!("{:>21}:{}", short_name, usage_string));
    }
    result
}

/// 格式化总成本报告。
pub fn format_total_cost(state: &BootstrapState, ext: &BootstrapStateExtended) -> String {
    let cost_display = if ext.has_unknown_model_cost {
        format!(
            "{} (costs may be inaccurate due to usage of unknown models)",
            format_cost(state.total_cost_usd, 4)
        )
    } else {
        format_cost(state.total_cost_usd, 4)
    };

    let model_usage_display = format_model_usage(ext);
    let total_duration = state.total_duration_ms();

    format!(
        "Total cost:            {}\n\
         Total duration (API):  {}\n\
         Total duration (wall): {}\n\
         Total code changes:    {} {} added, {} {} removed\n\
         {}",
        cost_display,
        format_duration(state.total_api_duration_ms),
        format_duration(total_duration),
        state.total_lines_added,
        if state.total_lines_added == 1 { "line" } else { "lines" },
        state.total_lines_removed,
        if state.total_lines_removed == 1 { "line" } else { "lines" },
        model_usage_display,
    )
}

/// 累加模型使用量并返回更新后的 ModelUsage。
pub fn add_to_total_model_usage(
    ext: &mut BootstrapStateExtended,
    cost: f64,
    usage: &ApiUsage,
    model: &str,
) -> ModelUsage {
    let model_usage = ext
        .model_usage
        .entry(model.to_string())
        .or_default();

    model_usage.input_tokens += usage.input_tokens;
    model_usage.output_tokens += usage.output_tokens;
    model_usage.cache_read_input_tokens += usage.cache_read_input_tokens;
    model_usage.cache_creation_input_tokens += usage.cache_creation_input_tokens;
    model_usage.web_search_requests += usage.web_search_requests;
    model_usage.cost_usd += cost;

    model_usage.clone()
}

/// 累加会话总成本。
pub fn add_to_total_session_cost(
    state: &mut BootstrapState,
    ext: &mut BootstrapStateExtended,
    cost: f64,
    usage: &ApiUsage,
    model: &str,
) -> f64 {
    let model_usage = add_to_total_model_usage(ext, cost, usage, model);
    state.add_cost(cost);
    ext.add_to_total_cost_state(model, model_usage);
    cost
}

// ---------------------------------------------------------------------------
// Cost hook (对应 costHook.ts)
// ---------------------------------------------------------------------------

/// 注册退出时输出成本摘要的清理逻辑。
pub fn register_cost_summary_cleanup(
    state: &BootstrapState,
    ext: &BootstrapStateExtended,
    has_billing_access: bool,
) {
    if has_billing_access {
        let summary = format_total_cost(state, ext);
        println!("\n{}", summary);
    }
}
