// Stats cache persistence, version migration, and merge logic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use chrono::{Datelike, Local, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::Mutex as AsyncMutex;

pub const STATS_CACHE_VERSION: u32 = 3;
const MIN_MIGRATABLE_VERSION: u32 = 1;
const STATS_CACHE_FILENAME: &str = "stats-cache.json";

static STATS_CACHE_LOCK: once_cell::sync::Lazy<AsyncMutex<()>> =
    once_cell::sync::Lazy::new(|| AsyncMutex::new(()));

/// Execute a function while holding the stats cache lock.
pub async fn with_stats_cache_lock<F, Fut, T>(f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let _guard = STATS_CACHE_LOCK.lock().await;
    f().await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    #[serde(rename = "inputTokens")]
    pub input_tokens: u64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: u64,
    #[serde(rename = "cacheReadInputTokens")]
    pub cache_read_input_tokens: u64,
    #[serde(rename = "cacheCreationInputTokens")]
    pub cache_creation_input_tokens: u64,
    #[serde(rename = "webSearchRequests")]
    pub web_search_requests: u64,
    #[serde(rename = "costUSD")]
    pub cost_usd: f64,
    #[serde(rename = "contextWindow")]
    pub context_window: u64,
    #[serde(rename = "maxOutputTokens")]
    pub max_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivity {
    pub date: String,
    #[serde(rename = "messageCount")]
    pub message_count: u64,
    #[serde(rename = "sessionCount")]
    pub session_count: u64,
    #[serde(rename = "toolCallCount")]
    pub tool_call_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyModelTokens {
    pub date: String,
    #[serde(rename = "tokensByModel")]
    pub tokens_by_model: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub timestamp: String,
    pub duration: u64,
    #[serde(rename = "messageCount")]
    pub message_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedStatsCache {
    pub version: u32,
    #[serde(rename = "lastComputedDate")]
    pub last_computed_date: Option<String>,
    #[serde(rename = "dailyActivity")]
    pub daily_activity: Vec<DailyActivity>,
    #[serde(rename = "dailyModelTokens")]
    pub daily_model_tokens: Vec<DailyModelTokens>,
    #[serde(rename = "modelUsage")]
    pub model_usage: HashMap<String, ModelUsage>,
    #[serde(rename = "totalSessions")]
    pub total_sessions: u64,
    #[serde(rename = "totalMessages")]
    pub total_messages: u64,
    #[serde(rename = "longestSession")]
    pub longest_session: Option<SessionStats>,
    #[serde(rename = "firstSessionDate")]
    pub first_session_date: Option<String>,
    #[serde(rename = "hourCounts")]
    pub hour_counts: HashMap<u32, u64>,
    #[serde(rename = "totalSpeculationTimeSavedMs")]
    pub total_speculation_time_saved_ms: u64,
    #[serde(rename = "shotDistribution", skip_serializing_if = "Option::is_none")]
    pub shot_distribution: Option<HashMap<u32, u64>>,
}

pub fn get_stats_cache_path(config_home: &Path) -> PathBuf {
    config_home.join(STATS_CACHE_FILENAME)
}

fn get_empty_cache() -> PersistedStatsCache {
    PersistedStatsCache {
        version: STATS_CACHE_VERSION,
        last_computed_date: None,
        daily_activity: Vec::new(),
        daily_model_tokens: Vec::new(),
        model_usage: HashMap::new(),
        total_sessions: 0,
        total_messages: 0,
        longest_session: None,
        first_session_date: None,
        hour_counts: HashMap::new(),
        total_speculation_time_saved_ms: 0,
        shot_distribution: Some(HashMap::new()),
    }
}

/// Migrate an older cache to the current schema.
fn migrate_stats_cache(parsed: &serde_json::Value) -> Option<PersistedStatsCache> {
    let version = parsed.get("version")?.as_u64()? as u32;
    if version < MIN_MIGRATABLE_VERSION || version > STATS_CACHE_VERSION {
        return None;
    }
    // Validate required arrays
    if !parsed.get("dailyActivity")?.is_array()
        || !parsed.get("dailyModelTokens")?.is_array()
    {
        return None;
    }
    if parsed.get("totalSessions")?.as_u64().is_none()
        || parsed.get("totalMessages")?.as_u64().is_none()
    {
        return None;
    }

    // Deserialize with defaults
    let result: PersistedStatsCache = serde_json::from_value(parsed.clone()).ok()?;
    Some(PersistedStatsCache {
        version: STATS_CACHE_VERSION,
        ..result
    })
}

/// Load the stats cache from disk.
pub async fn load_stats_cache(config_home: &Path) -> PersistedStatsCache {
    let cache_path = get_stats_cache_path(config_home);
    let content = match fs::read_to_string(&cache_path).await {
        Ok(c) => c,
        Err(_) => return get_empty_cache(),
    };

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(p) => p,
        Err(_) => return get_empty_cache(),
    };

    let version = parsed
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if version != STATS_CACHE_VERSION {
        match migrate_stats_cache(&parsed) {
            Some(migrated) => {
                let _ = save_stats_cache(&migrated, config_home).await;
                return migrated;
            }
            None => return get_empty_cache(),
        }
    }

    match serde_json::from_value::<PersistedStatsCache>(parsed) {
        Ok(cache) => cache,
        Err(_) => get_empty_cache(),
    }
}

/// Save the stats cache to disk atomically.
pub async fn save_stats_cache(cache: &PersistedStatsCache, config_home: &Path) -> Result<()> {
    let cache_path = get_stats_cache_path(config_home);
    let mut rng = rand::thread_rng();
    let random_suffix: u64 = rng.gen();
    let temp_path = format!("{}.{:016x}.tmp", cache_path.display(), random_suffix);

    let _ = fs::create_dir_all(config_home).await;

    let content = serde_json::to_string_pretty(cache)?;
    fs::write(&temp_path, &content).await?;
    fs::rename(&temp_path, &cache_path).await?;
    Ok(())
}

/// Merge new stats into an existing cache.
pub fn merge_cache_with_new_stats(
    existing_cache: &PersistedStatsCache,
    new_daily_activity: &[DailyActivity],
    new_daily_model_tokens: &[DailyModelTokens],
    new_model_usage: &HashMap<String, ModelUsage>,
    new_session_stats: &[SessionStats],
    new_hour_counts: &HashMap<u32, u64>,
    new_total_speculation_time_saved_ms: u64,
    new_shot_distribution: Option<&HashMap<u32, u64>>,
    new_last_computed_date: &str,
) -> PersistedStatsCache {
    // Merge daily activity by date
    let mut daily_activity_map: HashMap<String, DailyActivity> = HashMap::new();
    for day in &existing_cache.daily_activity {
        daily_activity_map.insert(day.date.clone(), day.clone());
    }
    for day in new_daily_activity {
        let entry = daily_activity_map
            .entry(day.date.clone())
            .or_insert_with(|| DailyActivity {
                date: day.date.clone(),
                message_count: 0,
                session_count: 0,
                tool_call_count: 0,
            });
        entry.message_count += day.message_count;
        entry.session_count += day.session_count;
        entry.tool_call_count += day.tool_call_count;
    }

    // Merge daily model tokens by date
    let mut daily_model_tokens_map: HashMap<String, HashMap<String, u64>> = HashMap::new();
    for day in &existing_cache.daily_model_tokens {
        daily_model_tokens_map.insert(day.date.clone(), day.tokens_by_model.clone());
    }
    for day in new_daily_model_tokens {
        let entry = daily_model_tokens_map
            .entry(day.date.clone())
            .or_default();
        for (model, tokens) in &day.tokens_by_model {
            *entry.entry(model.clone()).or_insert(0) += tokens;
        }
    }

    // Merge model usage
    let mut model_usage = existing_cache.model_usage.clone();
    for (model, usage) in new_model_usage {
        let entry = model_usage.entry(model.clone()).or_insert_with(|| ModelUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            web_search_requests: 0,
            cost_usd: 0.0,
            context_window: 0,
            max_output_tokens: 0,
        });
        entry.input_tokens += usage.input_tokens;
        entry.output_tokens += usage.output_tokens;
        entry.cache_read_input_tokens += usage.cache_read_input_tokens;
        entry.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        entry.web_search_requests += usage.web_search_requests;
        entry.cost_usd += usage.cost_usd;
        entry.context_window = entry.context_window.max(usage.context_window);
        entry.max_output_tokens = entry.max_output_tokens.max(usage.max_output_tokens);
    }

    // Merge hour counts
    let mut hour_counts = existing_cache.hour_counts.clone();
    for (hour, count) in new_hour_counts {
        *hour_counts.entry(*hour).or_insert(0) += count;
    }

    // Update session aggregates
    let total_sessions = existing_cache.total_sessions + new_session_stats.len() as u64;
    let total_messages = existing_cache.total_messages
        + new_session_stats.iter().map(|s| s.message_count).sum::<u64>();

    // Find longest session
    let mut longest_session = existing_cache.longest_session.clone();
    for session in new_session_stats {
        if longest_session
            .as_ref()
            .map_or(true, |l| session.duration > l.duration)
        {
            longest_session = Some(session.clone());
        }
    }

    // Find first session date
    let mut first_session_date = existing_cache.first_session_date.clone();
    for session in new_session_stats {
        if first_session_date
            .as_ref()
            .map_or(true, |d| session.timestamp < *d)
        {
            first_session_date = Some(session.timestamp.clone());
        }
    }

    // Sort daily activity
    let mut sorted_daily_activity: Vec<DailyActivity> =
        daily_activity_map.into_values().collect();
    sorted_daily_activity.sort_by(|a, b| a.date.cmp(&b.date));

    // Sort daily model tokens
    let mut sorted_daily_model_tokens: Vec<DailyModelTokens> = daily_model_tokens_map
        .into_iter()
        .map(|(date, tokens_by_model)| DailyModelTokens {
            date,
            tokens_by_model,
        })
        .collect();
    sorted_daily_model_tokens.sort_by(|a, b| a.date.cmp(&b.date));

    // Merge shot distribution
    let shot_distribution = if let Some(new_sd) = new_shot_distribution {
        let mut sd = existing_cache.shot_distribution.clone().unwrap_or_default();
        for (count, sessions) in new_sd {
            *sd.entry(*count).or_insert(0) += sessions;
        }
        Some(sd)
    } else {
        existing_cache.shot_distribution.clone()
    };

    PersistedStatsCache {
        version: STATS_CACHE_VERSION,
        last_computed_date: Some(new_last_computed_date.to_string()),
        daily_activity: sorted_daily_activity,
        daily_model_tokens: sorted_daily_model_tokens,
        model_usage,
        total_sessions,
        total_messages,
        longest_session,
        first_session_date,
        hour_counts,
        total_speculation_time_saved_ms: existing_cache.total_speculation_time_saved_ms
            + new_total_speculation_time_saved_ms,
        shot_distribution,
    }
}

/// Extract the date portion (YYYY-MM-DD) from a chrono DateTime.
pub fn to_date_string(date: &chrono::DateTime<Utc>) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Get today's date in YYYY-MM-DD format.
pub fn get_today_date_string() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Get yesterday's date in YYYY-MM-DD format.
pub fn get_yesterday_date_string() -> String {
    let yesterday = Local::now() - chrono::Duration::days(1);
    yesterday.format("%Y-%m-%d").to_string()
}

/// Check if a date string is before another date string.
pub fn is_date_before(date1: &str, date2: &str) -> bool {
    date1 < date2
}
