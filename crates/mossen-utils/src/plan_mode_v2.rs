//! # plan_mode_v2 — Plan 模式 V2 配置
//!
//! 对应 TypeScript `utils/planModeV2.ts`。

use std::env;

/// 订阅类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionType {
    Max,
    Pro,
    Enterprise,
    Team,
    Free,
    Other(String),
}

/// 获取订阅类型。优先级：env override > `crate::auth::get_subscription_type`
/// > Free。`MOSSEN_SUBSCRIPTION_TYPE` 主要用于测试。
fn get_subscription_type() -> SubscriptionType {
    if let Ok(val) = env::var("MOSSEN_SUBSCRIPTION_TYPE") {
        return parse_subscription_type(&val);
    }
    match crate::auth::get_subscription_type() {
        Some(s) => parse_subscription_type(&s),
        None => SubscriptionType::Free,
    }
}

fn parse_subscription_type(s: &str) -> SubscriptionType {
    match s {
        "max" => SubscriptionType::Max,
        "pro" => SubscriptionType::Pro,
        "enterprise" => SubscriptionType::Enterprise,
        "team" => SubscriptionType::Team,
        "free" => SubscriptionType::Free,
        other => SubscriptionType::Other(other.to_string()),
    }
}

/// 获取速率限制层级。优先级：env override > `crate::auth::get_rate_limit_tier`。
fn get_rate_limit_tier() -> String {
    if let Ok(val) = env::var("MOSSEN_RATE_LIMIT_TIER") {
        return val;
    }
    crate::auth::get_rate_limit_tier().unwrap_or_default()
}

/// 获取特性值（缓存，可能过时）
fn get_feature_value_cached<T: Default>(_feature_name: &str, default: T) -> T {
    default
}

/// 获取 Plan Mode V2 代理数量
pub fn get_plan_mode_v2_agent_count() -> u32 {
    // 环境变量覆盖优先
    if let Ok(val) = env::var("MOSSEN_CODE_PLAN_V2_AGENT_COUNT") {
        if let Ok(count) = val.parse::<u32>() {
            if count > 0 && count <= 10 {
                return count;
            }
        }
    }

    let subscription_type = get_subscription_type();
    let rate_limit_tier = get_rate_limit_tier();

    if subscription_type == SubscriptionType::Max
        && rate_limit_tier == "default_mossen_max_20x"
    {
        return 3;
    }

    if subscription_type == SubscriptionType::Enterprise
        || subscription_type == SubscriptionType::Team
    {
        return 3;
    }

    1
}

/// 获取 Plan Mode V2 探索代理数量
pub fn get_plan_mode_v2_explore_agent_count() -> u32 {
    if let Ok(val) = env::var("MOSSEN_CODE_PLAN_V2_EXPLORE_AGENT_COUNT") {
        if let Ok(count) = val.parse::<u32>() {
            if count > 0 && count <= 10 {
                return count;
            }
        }
    }

    3
}

/// 检查 plan mode 面试阶段是否启用
pub fn is_plan_mode_interview_phase_enabled() -> bool {
    // ant 用户始终启用
    if env::var("USER_TYPE").as_deref() == Ok("ant") {
        return true;
    }

    let env_val = env::var("MOSSEN_CODE_PLAN_MODE_INTERVIEW_PHASE").ok();
    if let Some(ref val) = env_val {
        if is_env_truthy(val) {
            return true;
        }
        if is_env_defined_falsy(val) {
            return false;
        }
    }

    get_feature_value_cached("tengu_plan_mode_interview_phase", false)
}

/// Pewter Ledger 变体
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PewterLedgerVariant {
    Trim,
    Cut,
    Cap,
}

/// 获取 Pewter Ledger 变体
pub fn get_pewter_ledger_variant() -> Option<PewterLedgerVariant> {
    let raw: Option<String> = get_feature_value_cached("tengu_pewter_ledger", None);
    match raw.as_deref() {
        Some("trim") => Some(PewterLedgerVariant::Trim),
        Some("cut") => Some(PewterLedgerVariant::Cut),
        Some("cap") => Some(PewterLedgerVariant::Cap),
        _ => None,
    }
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}

fn is_env_defined_falsy(val: &str) -> bool {
    matches!(val, "0" | "false" | "no" | "off")
}
