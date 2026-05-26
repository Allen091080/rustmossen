use serde::{Deserialize, Serialize};

/// Reasoning profiles
pub const REASONING_PROFILES: &[&str] = &["fast", "standard", "deep"];

/// Execution profiles
pub const EXECUTION_PROFILES: &[&str] = &["coding", "review", "long-context", "low-cost"];

/// Reasoning profile type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningProfile {
    Fast,
    Standard,
    Deep,
}

/// Execution profile type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionProfile {
    Coding,
    Review,
    LongContext,
    LowCost,
}

/// Effort level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    Low,
    Medium,
    High,
}

/// Effort value can be a named level or a numeric value
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffortValue {
    Low,
    Medium,
    High,
    Max,
    Numeric(u32),
}

/// Execution profile defaults
pub struct ExecutionProfileDefaults {
    pub reasoning_profile: ReasoningProfile,
    pub description: &'static str,
}

/// Get defaults for an execution profile
pub fn get_execution_profile_defaults(profile: ExecutionProfile) -> ExecutionProfileDefaults {
    match profile {
        ExecutionProfile::Coding => ExecutionProfileDefaults {
            reasoning_profile: ReasoningProfile::Standard,
            description: "Balanced day-to-day coding and debugging",
        },
        ExecutionProfile::Review => ExecutionProfileDefaults {
            reasoning_profile: ReasoningProfile::Deep,
            description: "Thorough analysis and review with more reasoning budget",
        },
        ExecutionProfile::LongContext => ExecutionProfileDefaults {
            reasoning_profile: ReasoningProfile::Standard,
            description: "Favor continuity in long-running sessions and large context",
        },
        ExecutionProfile::LowCost => ExecutionProfileDefaults {
            reasoning_profile: ReasoningProfile::Fast,
            description: "Prefer faster, lighter reasoning for lower cost and latency",
        },
    }
}

/// Check if a string is a valid reasoning profile
pub fn is_reasoning_profile(value: &str) -> bool {
    REASONING_PROFILES.contains(&value)
}

/// Check if a string is a valid execution profile
pub fn is_execution_profile(value: &str) -> bool {
    EXECUTION_PROFILES.contains(&value)
}

/// Parse a string into a ReasoningProfile
pub fn parse_reasoning_profile(value: &str) -> Option<ReasoningProfile> {
    match value {
        "fast" => Some(ReasoningProfile::Fast),
        "standard" => Some(ReasoningProfile::Standard),
        "deep" => Some(ReasoningProfile::Deep),
        _ => None,
    }
}

/// Parse a string into an ExecutionProfile
pub fn parse_execution_profile(value: &str) -> Option<ExecutionProfile> {
    match value {
        "coding" => Some(ExecutionProfile::Coding),
        "review" => Some(ExecutionProfile::Review),
        "long-context" => Some(ExecutionProfile::LongContext),
        "low-cost" => Some(ExecutionProfile::LowCost),
        _ => None,
    }
}

/// Convert reasoning profile to effort level
pub fn reasoning_profile_to_effort(profile: ReasoningProfile) -> EffortLevel {
    match profile {
        ReasoningProfile::Fast => EffortLevel::Low,
        ReasoningProfile::Standard => EffortLevel::Medium,
        ReasoningProfile::Deep => EffortLevel::High,
    }
}

/// Convert effort value to reasoning profile
pub fn effort_value_to_reasoning_profile(value: Option<EffortValue>) -> ReasoningProfile {
    match value {
        Some(EffortValue::Low) => ReasoningProfile::Fast,
        Some(EffortValue::Medium) => ReasoningProfile::Standard,
        Some(EffortValue::High) | Some(EffortValue::Max) => ReasoningProfile::Deep,
        Some(EffortValue::Numeric(n)) => {
            if n <= 50 {
                ReasoningProfile::Fast
            } else if n <= 85 {
                ReasoningProfile::Standard
            } else {
                ReasoningProfile::Deep
            }
        }
        None => ReasoningProfile::Standard,
    }
}

/// Get reasoning profile description
pub fn get_reasoning_profile_description(profile: ReasoningProfile) -> &'static str {
    match profile {
        ReasoningProfile::Fast => "Quick responses with lighter reasoning",
        ReasoningProfile::Standard => "Balanced reasoning for everyday work",
        ReasoningProfile::Deep => "Deeper reasoning for harder coding tasks",
    }
}

/// Get execution profile description
pub fn get_execution_profile_description(profile: ExecutionProfile) -> &'static str {
    get_execution_profile_defaults(profile).description
}

/// Profile settings subset
pub struct ProfileSettings {
    pub execution_profile: Option<String>,
    pub reasoning_profile: Option<String>,
    pub effort_level: Option<EffortValue>,
}

/// Get explicit reasoning profile from settings
fn get_explicit_reasoning_profile(settings: &ProfileSettings) -> Option<ReasoningProfile> {
    if let Some(ref rp) = settings.reasoning_profile {
        if let Some(profile) = parse_reasoning_profile(rp) {
            return Some(profile);
        }
    }
    if let Some(effort) = settings.effort_level {
        return Some(effort_value_to_reasoning_profile(Some(effort)));
    }
    None
}

/// Get configured reasoning profile (with default)
pub fn get_configured_reasoning_profile(settings: &ProfileSettings) -> ReasoningProfile {
    get_explicit_reasoning_profile(settings).unwrap_or(ReasoningProfile::Standard)
}

/// Get configured execution profile (with default)
pub fn get_configured_execution_profile(settings: &ProfileSettings) -> ExecutionProfile {
    if let Some(ref ep) = settings.execution_profile {
        if let Some(profile) = parse_execution_profile(ep) {
            return profile;
        }
    }
    ExecutionProfile::Coding
}

/// Get current reasoning profile from app state effort and settings
pub fn get_current_reasoning_profile(
    app_state_effort: Option<EffortValue>,
    settings: &ProfileSettings,
) -> ReasoningProfile {
    if let Some(effort) = app_state_effort {
        return effort_value_to_reasoning_profile(Some(effort));
    }
    get_configured_reasoning_profile(settings)
}

/// Get explicit persisted reasoning effort from settings
pub fn get_explicit_persisted_reasoning_effort(settings: &ProfileSettings) -> Option<EffortLevel> {
    get_explicit_reasoning_profile(settings).map(reasoning_profile_to_effort)
}
