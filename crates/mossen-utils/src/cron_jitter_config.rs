//! # cron_jitter_config — Cron 抖动配置
//!
//! 对应 TypeScript `utils/cronJitterConfig.ts`。
//! GrowthBook 支持的 cron 抖动配置。

/// Cron 抖动配置
#[derive(Debug, Clone)]
pub struct CronJitterConfig {
    /// 周期性任务的抖动分数 (0.0-1.0)
    pub recurring_frac: f64,
    /// 周期性任务的抖动上限 (ms)
    pub recurring_cap_ms: u64,
    /// 一次性任务的最大延迟 (ms)
    pub one_shot_max_ms: u64,
    /// 一次性任务的最小延迟 (ms)
    pub one_shot_floor_ms: u64,
    /// 一次性任务的分钟取模值
    pub one_shot_minute_mod: u32,
    /// 周期性任务的最大存活时间 (ms)
    pub recurring_max_age_ms: u64,
}

/// 默认 Cron 抖动配置
pub const DEFAULT_CRON_JITTER_CONFIG: CronJitterConfig = CronJitterConfig {
    recurring_frac: 0.1,
    recurring_cap_ms: 60_000,
    one_shot_max_ms: 300_000,
    one_shot_floor_ms: 0,
    one_shot_minute_mod: 5,
    recurring_max_age_ms: 7 * 24 * 60 * 60 * 1000, // 7 days
};

/// 抖动配置的上限常量
const HALF_HOUR_MS: u64 = 30 * 60 * 1000;
const THIRTY_DAYS_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// 刷新间隔 (ms)
const JITTER_CONFIG_REFRESH_MS: u64 = 60 * 1000;

/// 验证 Cron 抖动配置
fn validate_cron_jitter_config(config: &CronJitterConfig) -> bool {
    config.recurring_frac >= 0.0
        && config.recurring_frac <= 1.0
        && config.recurring_cap_ms <= HALF_HOUR_MS
        && config.one_shot_max_ms <= HALF_HOUR_MS
        && config.one_shot_floor_ms <= HALF_HOUR_MS
        && config.one_shot_minute_mod >= 1
        && config.one_shot_minute_mod <= 60
        && config.recurring_max_age_ms <= THIRTY_DAYS_MS
        && config.one_shot_floor_ms <= config.one_shot_max_ms
}

/// 获取 Cron 抖动配置。
///
/// 从远程配置读取 `tengu_kairos_cron_config`，验证后返回。
/// 配置格式错误或超出范围时回退到默认值。
///
/// 将此函数作为 `get_jitter_config` 传递给 REPL 上下文中的 createCronScheduler。
/// Daemon/SDK 调用者省略 get_jitter_config 并获取默认值。
pub fn get_cron_jitter_config() -> CronJitterConfig {
    // 在实际实现中，这会从 GrowthBook 读取缓存的远程配置
    // 此处直接返回默认配置
    DEFAULT_CRON_JITTER_CONFIG
}

/// 通过远程配置获取抖动设置（带缓存和刷新）
pub fn get_cron_jitter_config_with_refresh(
    raw_config: Option<&CronJitterConfig>,
) -> CronJitterConfig {
    match raw_config {
        Some(config) if validate_cron_jitter_config(config) => config.clone(),
        _ => DEFAULT_CRON_JITTER_CONFIG,
    }
}
