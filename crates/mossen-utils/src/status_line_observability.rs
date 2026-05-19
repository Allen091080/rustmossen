use serde::{Deserialize, Serialize};

/// Token estimation options
pub struct TokenOptions {
    pub ignore_empty_usage: bool,
}

pub const STATUS_LINE_TOKEN_OPTIONS: TokenOptions = TokenOptions {
    ignore_empty_usage: true,
};

/// Model tier: local or cloud
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Local,
    Cloud,
}

/// Worktree observability snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSnapshot {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
    pub original_cwd: String,
    pub original_branch: Option<String>,
}

/// Profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesInfo {
    pub execution: String,
    pub reasoning: String,
    pub effort_level: String,
}

/// Context observability data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObservability {
    pub pressure_percent: u32,
    pub auto_compact_enabled: bool,
    pub auto_compact_threshold_percent: Option<u32>,
    pub auto_compact_threshold_tokens: Option<u64>,
    pub threshold_reached: bool,
    pub recent_compact: String,
}

/// Full status line observability input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLineObservabilityInput {
    pub model_tier: ModelTier,
    pub interactive_language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeSnapshot>,
    pub profiles: ProfilesInfo,
    pub context_observability: ContextObservability,
}

/// Options for building status line observability input
pub struct BuildStatusLineOptions {
    pub auto_compact_enabled: Option<bool>,
    pub model_tier: Option<ModelTier>,
}

/// Determine model tier from a base URL
pub fn get_displayed_model_tier_from_base_url(base_url: Option<&str>) -> ModelTier {
    let base_url = match base_url {
        Some(url) if !url.is_empty() => url,
        _ => return ModelTier::Cloud,
    };

    if let Ok(parsed) = url::Url::parse(base_url) {
        if let Some(host) = parsed.host_str() {
            let hostname = host.to_lowercase();
            if hostname == "localhost"
                || hostname == "0.0.0.0"
                || hostname == "::1"
                || hostname.starts_with("127.")
            {
                return ModelTier::Local;
            }
        }
    }

    ModelTier::Cloud
}

/// Determine model tier based on custom backend configuration
pub fn get_displayed_model_tier(
    is_custom_backend_enabled: bool,
    custom_backend_base_url: Option<&str>,
) -> ModelTier {
    if !is_custom_backend_enabled {
        return ModelTier::Cloud;
    }
    get_displayed_model_tier_from_base_url(custom_backend_base_url)
}

/// Build the full status line observability input
pub fn build_status_line_observability_input(
    current_tokens: u64,
    effective_window: u64,
    auto_compact_enabled: bool,
    auto_compact_threshold: Option<u64>,
    threshold_reached: bool,
    messages_since_compact: Option<usize>,
    execution_profile: &str,
    reasoning_profile: &str,
    effort_level: &str,
    worktree_snapshot: Option<WorktreeSnapshot>,
    interactive_language: &str,
    model_tier: ModelTier,
) -> StatusLineObservabilityInput {
    let context_percent = if effective_window > 0 {
        std::cmp::min(100, ((current_tokens as f64 / effective_window as f64) * 100.0).round() as u32)
    } else {
        0
    };

    let auto_compact_threshold_percent = auto_compact_threshold.map(|threshold| {
        if effective_window > 0 {
            ((threshold as f64 / effective_window as f64) * 100.0).round() as u32
        } else {
            0
        }
    });

    let recent_compact = match messages_since_compact {
        None => "none".to_string(),
        Some(count) => format!("{} messages since last compact", count),
    };

    StatusLineObservabilityInput {
        model_tier,
        interactive_language: interactive_language.to_string(),
        worktree: worktree_snapshot,
        profiles: ProfilesInfo {
            execution: execution_profile.to_string(),
            reasoning: reasoning_profile.to_string(),
            effort_level: effort_level.to_string(),
        },
        context_observability: ContextObservability {
            pressure_percent: context_percent,
            auto_compact_enabled,
            auto_compact_threshold_percent,
            auto_compact_threshold_tokens: auto_compact_threshold,
            threshold_reached,
            recent_compact,
        },
    }
}
