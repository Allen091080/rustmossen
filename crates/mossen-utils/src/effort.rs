//! Effort Level Management
//!
//! Manages effort levels for model thinking/reasoning, including parsing,
//! persistence, resolution, and model support checks.

/// Effort level string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Low => "Quick, straightforward implementation with minimal overhead",
            Self::Medium => "Balanced approach with standard implementation and testing",
            Self::High => "Comprehensive implementation with extensive testing and documentation",
            Self::Max => "Maximum capability with deepest reasoning (Opus 4.6 only)",
        }
    }
}

/// Effort value — either a named level or a numeric value (internal use).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffortValue {
    Level(EffortLevel),
    Numeric(i32),
}

impl EffortValue {
    pub fn to_level(&self) -> EffortLevel {
        match self {
            Self::Level(l) => *l,
            Self::Numeric(v) => {
                if *v <= 50 { EffortLevel::Low }
                else if *v <= 85 { EffortLevel::Medium }
                else if *v <= 100 { EffortLevel::High }
                else { EffortLevel::Max }
            }
        }
    }
}

/// All valid effort levels.
pub const EFFORT_LEVELS: &[EffortLevel] = &[
    EffortLevel::Low,
    EffortLevel::Medium,
    EffortLevel::High,
    EffortLevel::Max,
];

/// Check if a string is a valid effort level.
pub fn is_effort_level(value: &str) -> bool {
    EffortLevel::from_str(value).is_some()
}

/// Check if a numeric effort value is valid.
pub fn is_valid_numeric_effort(value: i32) -> bool {
    // Any integer is valid
    true
}

/// Parse an effort value from various input types.
pub fn parse_effort_value(value: &str) -> Option<EffortValue> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let lower = value.to_lowercase();
    if let Some(level) = EffortLevel::from_str(&lower) {
        return Some(EffortValue::Level(level));
    }

    if let Ok(num) = value.parse::<i32>() {
        if is_valid_numeric_effort(num) {
            return Some(EffortValue::Numeric(num));
        }
    }

    None
}

/// Numeric values are model-default only and not persisted.
/// Returns the persistable effort level or None.
pub fn to_persistable_effort(value: Option<EffortValue>, is_ant: bool) -> Option<EffortLevel> {
    match value {
        Some(EffortValue::Level(EffortLevel::Low)) => Some(EffortLevel::Low),
        Some(EffortValue::Level(EffortLevel::Medium)) => Some(EffortLevel::Medium),
        Some(EffortValue::Level(EffortLevel::High)) => Some(EffortLevel::High),
        Some(EffortValue::Level(EffortLevel::Max)) if is_ant => Some(EffortLevel::Max),
        _ => None,
    }
}

/// Check if a model supports the effort parameter.
pub fn model_supports_effort(model: &str, always_enable: bool, api_provider: &str) -> bool {
    if always_enable {
        return true;
    }
    let m = model.to_lowercase();
    if m.contains("opus-4-6") || m.contains("sonnet-4-6") {
        return true;
    }
    if m.contains("haiku") || m.contains("sonnet") || m.contains("opus") {
        return false;
    }
    api_provider == "firstParty"
}

/// Check if a model supports 'max' effort.
pub fn model_supports_max_effort(model: &str, is_ant: bool) -> bool {
    if model.to_lowercase().contains("opus-4-6") {
        return true;
    }
    if is_ant {
        return true;
    }
    false
}

/// Resolve the effort value that will actually be sent to the API.
pub fn resolve_applied_effort(
    model: &str,
    app_state_effort: Option<EffortValue>,
    env_override: Option<EffortValue>,
    env_is_unset: bool,
    default_effort: Option<EffortValue>,
    is_ant: bool,
) -> Option<EffortValue> {
    if env_is_unset {
        return None;
    }
    let resolved = env_override
        .or(app_state_effort)
        .or(default_effort);

    // Downgrade 'max' for non-supported models
    if let Some(EffortValue::Level(EffortLevel::Max)) = resolved {
        if !model_supports_max_effort(model, is_ant) {
            return Some(EffortValue::Level(EffortLevel::High));
        }
    }

    resolved
}

/// Convert effort value to display level.
pub fn convert_effort_value_to_level(value: EffortValue, is_ant: bool) -> EffortLevel {
    match value {
        EffortValue::Level(l) => l,
        EffortValue::Numeric(v) if is_ant => {
            if v <= 50 { EffortLevel::Low }
            else if v <= 85 { EffortLevel::Medium }
            else if v <= 100 { EffortLevel::High }
            else { EffortLevel::Max }
        }
        _ => EffortLevel::High,
    }
}

/// Get the displayed effort level for UI.
pub fn get_displayed_effort_level(
    model: &str,
    app_state_effort: Option<EffortValue>,
    env_override: Option<EffortValue>,
    env_is_unset: bool,
    default_effort: Option<EffortValue>,
    is_ant: bool,
) -> EffortLevel {
    let resolved = resolve_applied_effort(model, app_state_effort, env_override, env_is_unset, default_effort, is_ant);
    match resolved {
        Some(v) => convert_effort_value_to_level(v, is_ant),
        None => EffortLevel::High,
    }
}

/// Build the effort suffix shown in Logo/Spinner.
pub fn get_effort_suffix(resolved: Option<EffortValue>, is_ant: bool) -> String {
    match resolved {
        None => String::new(),
        Some(v) => {
            let level = convert_effort_value_to_level(v, is_ant);
            format!(" with {} effort", level.as_str())
        }
    }
}

/// Resolve picker effort persistence.
pub fn resolve_picker_effort_persistence(
    picked: Option<EffortLevel>,
    model_default: EffortLevel,
    prior_persisted: Option<EffortLevel>,
    toggled_in_picker: bool,
) -> Option<EffortLevel> {
    let had_explicit = prior_persisted.is_some() || toggled_in_picker;
    if had_explicit || picked != Some(model_default) {
        picked
    } else {
        None
    }
}

// =============================================================================
// 额外导出 — 对应 TS effort.ts 中尚未覆盖的入口。
// =============================================================================

/// 对应 TS `OpusDefaultEffortConfig`：Opus 默认 effort 推荐对话框配置。
#[derive(Debug, Clone)]
pub struct OpusDefaultEffortConfig {
    pub enabled: bool,
    pub dialog_title: String,
    pub dialog_description: String,
}

impl Default for OpusDefaultEffortConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dialog_title: "We recommend medium effort for Opus".to_string(),
            dialog_description: "Effort determines how long Mossen thinks for when completing your task. We recommend medium effort for most tasks to balance speed and intelligence and maximize rate limits. Use ultrathink to trigger high effort when needed.".to_string(),
        }
    }
}

/// 获取 Opus 默认 effort 推荐配置（对应 TS `getOpusDefaultEffortConfig`）。
pub fn get_opus_default_effort_config() -> OpusDefaultEffortConfig {
    OpusDefaultEffortConfig::default()
}

/// 获取初始 effort 设置（对应 TS `getInitialEffortSetting`）。
///
/// 从环境变量 `MOSSEN_INITIAL_EFFORT_LEVEL` 读取；未配置返回 `None`。
pub fn get_initial_effort_setting() -> Option<EffortLevel> {
    let raw = std::env::var("MOSSEN_INITIAL_EFFORT_LEVEL").ok()?;
    EffortLevel::from_str(&raw)
}

/// 读取环境变量覆盖（对应 TS `getEffortEnvOverride`）。
pub fn get_effort_env_override() -> Option<EffortValue> {
    let raw = std::env::var("MOSSEN_EFFORT").ok()?;
    parse_effort_value(&raw)
}

/// 获取 effort 级别的人类可读说明（对应 TS `getEffortLevelDescription`）。
pub fn get_effort_level_description(level: EffortLevel) -> &'static str {
    level.description()
}

/// 获取 effort 值的描述（对应 TS `getEffortValueDescription`）。
pub fn get_effort_value_description(value: EffortValue, is_ant: bool) -> String {
    match value {
        EffortValue::Numeric(n) if is_ant => format!("[Internal effort] Numeric value of {n}"),
        EffortValue::Level(l) => get_effort_level_description(l).to_string(),
        _ => "Balanced approach with standard implementation and testing".to_string(),
    }
}

/// 根据模型返回默认 effort（对应 TS `getDefaultEffortForModel`）。
///
/// Rust 端没有 ant 内部模型覆盖配置，因此只覆盖 Opus 4.6 的默认值。
pub fn get_default_effort_for_model(
    model: &str,
    opus_default_enabled: bool,
    is_pro: bool,
    is_max_or_team: bool,
) -> Option<EffortValue> {
    let lower = model.to_lowercase();
    if lower.contains("opus-4-6") {
        if is_pro {
            return Some(EffortValue::Level(EffortLevel::Medium));
        }
        if opus_default_enabled && is_max_or_team {
            return Some(EffortValue::Level(EffortLevel::Medium));
        }
    }
    None
}
