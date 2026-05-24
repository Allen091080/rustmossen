//! # extra_usage — 额外用量计费判断
//!
//! 对应 TypeScript `utils/extraUsage.ts`。

/// 判断当前请求是否应计为额外用量。
pub fn is_billed_as_extra_usage(
    model: Option<&str>,
    is_fast_mode: bool,
    is_max_1m_merged: bool,
    is_hosted_subscriber: bool,
) -> bool {
    if !is_hosted_subscriber {
        return false;
    }
    if is_fast_mode {
        return true;
    }
    let Some(model) = model else {
        return false;
    };
    if !has_1m_context(model) {
        return false;
    }

    let m = model
        .to_lowercase()
        .trim_end_matches("[1m]")
        .trim()
        .to_string();
    let is_max_46 = m == "max" || m.contains("max-4-6");
    let is_balanced_46 = m == "balanced" || m.contains("balanced-4-6");

    if is_max_46 && is_max_1m_merged {
        return false;
    }

    is_max_46 || is_balanced_46
}

/// 检查模型是否支持 1M context（简化实现）。
fn has_1m_context(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("[1m]") || lower.contains("1m")
}
