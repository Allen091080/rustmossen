//! # privacy_level — 隐私级别控制
//!
//! 对应 TypeScript `utils/privacyLevel.ts`。
//! 控制 Mossen 生成的非必要网络流量和遥测。

/// 隐私级别，按限制性排序：default < no-telemetry < essential-traffic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyLevel {
    /// 所有功能启用。
    Default,
    /// 分析/遥测禁用（Datadog、1P 事件、反馈调查）。
    NoTelemetry,
    /// 所有非必要网络流量禁用（遥测 + 自动更新、grove、发布说明、模型能力等）。
    EssentialTraffic,
}

/// 获取当前隐私级别。
///
/// 解析的级别取最严格的信号：
/// - MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC → essential-traffic
/// - DISABLE_TELEMETRY → no-telemetry
pub fn get_privacy_level() -> PrivacyLevel {
    if std::env::var("MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC").is_ok() {
        return PrivacyLevel::EssentialTraffic;
    }
    if std::env::var("DISABLE_TELEMETRY").is_ok() {
        return PrivacyLevel::NoTelemetry;
    }
    PrivacyLevel::Default
}

/// 所有非必要网络流量是否应被抑制。
/// 等同于旧的 `process.env.MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC` 检查。
pub fn is_essential_traffic_only() -> bool {
    get_privacy_level() == PrivacyLevel::EssentialTraffic
}

/// 遥测/分析是否应被抑制。
/// 在 `no-telemetry` 和 `essential-traffic` 级别都为 true。
pub fn is_telemetry_disabled() -> bool {
    get_privacy_level() != PrivacyLevel::Default
}

/// 返回导致当前 essential-traffic 限制的环境变量名，
/// 未受限时返回 None。用于面向用户的"取消设置 X 以重新启用"消息。
pub fn get_essential_traffic_only_reason() -> Option<&'static str> {
    if std::env::var("MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC").is_ok() {
        return Some("MOSSEN_CODE_DISABLE_NONESSENTIAL_TRAFFIC");
    }
    None
}
