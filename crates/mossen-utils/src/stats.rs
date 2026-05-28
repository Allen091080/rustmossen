//! Statistics aggregation for Mossen sessions.
//!
//! Collects session statistics including daily activity, model usage,
//! streaks, and token consumption across all projects.

use std::collections::{HashMap, HashSet};

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Daily activity record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivity {
    pub date: String, // YYYY-MM-DD format
    pub message_count: u64,
    pub session_count: u64,
    pub tool_call_count: u64,
}

/// Daily token usage per model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyModelTokens {
    pub date: String, // YYYY-MM-DD format
    pub tokens_by_model: HashMap<String, u64>,
}

/// Streak information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreakInfo {
    pub current_streak: u32,
    pub longest_streak: u32,
    pub current_streak_start: Option<String>,
    pub longest_streak_start: Option<String>,
    pub longest_streak_end: Option<String>,
}

/// Session statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_id: String,
    pub duration: u64, // in milliseconds
    pub message_count: u64,
    pub timestamp: String,
}

/// Model usage aggregate.
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

/// Full Mossen statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenStats {
    pub total_sessions: u64,
    pub total_messages: u64,
    pub total_days: u64,
    pub active_days: u64,
    pub streaks: StreakInfo,
    pub daily_activity: Vec<DailyActivity>,
    pub daily_model_tokens: Vec<DailyModelTokens>,
    pub longest_session: Option<SessionStats>,
    pub model_usage: HashMap<String, ModelUsage>,
    pub first_session_date: Option<String>,
    pub last_session_date: Option<String>,
    pub peak_activity_day: Option<String>,
    pub peak_activity_hour: Option<u32>,
    pub total_speculation_time_saved_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shot_distribution: Option<HashMap<u32, u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_shot_rate: Option<u32>,
}

/// Date range filter for stats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsDateRange {
    SevenDays,
    ThirtyDays,
    All,
}

impl StatsDateRange {
    pub fn from_str(s: &str) -> Self {
        match s {
            "7d" => Self::SevenDays,
            "30d" => Self::ThirtyDays,
            _ => Self::All,
        }
    }

    pub fn days_back(&self) -> Option<u32> {
        match self {
            Self::SevenDays => Some(7),
            Self::ThirtyDays => Some(30),
            Self::All => None,
        }
    }
}

/// Processing options for session files.
#[derive(Debug, Clone, Default)]
pub struct ProcessOptions {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
}

/// Intermediate processed stats.
#[derive(Debug, Clone, Default)]
pub struct ProcessedStats {
    pub daily_activity: Vec<DailyActivity>,
    pub daily_model_tokens: Vec<DailyModelTokens>,
    pub model_usage: HashMap<String, ModelUsage>,
    pub session_stats: Vec<SessionStats>,
    pub hour_counts: HashMap<u32, u64>,
    pub total_messages: u64,
    pub total_speculation_time_saved_ms: u64,
    pub shot_distribution: Option<HashMap<u32, u64>>,
}

/// Convert date to YYYY-MM-DD string.
pub fn to_date_string(date: &chrono::DateTime<Utc>) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Convert NaiveDate to YYYY-MM-DD string.
pub fn naive_date_to_string(date: &NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Parse YYYY-MM-DD string to NaiveDate.
pub fn parse_date_string(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Check if date_a is before date_b (both YYYY-MM-DD).
pub fn is_date_before(date_a: &str, date_b: &str) -> bool {
    date_a < date_b
}

/// Get today's date string.
pub fn get_today_date_string() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

/// Get yesterday's date string.
pub fn get_yesterday_date_string() -> String {
    let yesterday = Utc::now() - chrono::Duration::days(1);
    yesterday.format("%Y-%m-%d").to_string()
}

/// Get the next day after a given date string.
pub fn get_next_day(date_str: &str) -> Option<String> {
    let date = parse_date_string(date_str)?;
    let next = date + chrono::Duration::days(1);
    Some(naive_date_to_string(&next))
}

/// Calculate streaks from daily activity.
pub fn calculate_streaks(daily_activity: &[DailyActivity]) -> StreakInfo {
    if daily_activity.is_empty() {
        return StreakInfo::default();
    }

    let today = Utc::now().date_naive();
    let active_dates: HashSet<String> = daily_activity.iter().map(|d| d.date.clone()).collect();

    // Calculate current streak (working backwards from today)
    let mut current_streak = 0u32;
    let mut current_streak_start: Option<String> = None;
    let mut check_date = today;

    loop {
        let date_str = naive_date_to_string(&check_date);
        if !active_dates.contains(&date_str) {
            break;
        }
        current_streak += 1;
        current_streak_start = Some(date_str);
        check_date -= chrono::Duration::days(1);
    }

    // Calculate longest streak
    let mut longest_streak = 0u32;
    let mut longest_streak_start: Option<String> = None;
    let mut longest_streak_end: Option<String> = None;

    let mut sorted_dates: Vec<&String> = active_dates.iter().collect();
    sorted_dates.sort();

    if !sorted_dates.is_empty() {
        let mut temp_streak = 1u32;
        let mut temp_start = sorted_dates[0].clone();

        for i in 1..sorted_dates.len() {
            let prev_date = parse_date_string(sorted_dates[i - 1]);
            let curr_date = parse_date_string(sorted_dates[i]);

            if let (Some(prev), Some(curr)) = (prev_date, curr_date) {
                let day_diff = (curr - prev).num_days();
                if day_diff == 1 {
                    temp_streak += 1;
                } else {
                    if temp_streak > longest_streak {
                        longest_streak = temp_streak;
                        longest_streak_start = Some(temp_start.clone());
                        longest_streak_end = Some(sorted_dates[i - 1].clone());
                    }
                    temp_streak = 1;
                    temp_start = sorted_dates[i].clone();
                }
            }
        }

        // Check final streak
        if temp_streak > longest_streak {
            longest_streak = temp_streak;
            longest_streak_start = Some(temp_start);
            longest_streak_end = sorted_dates.last().map(|s| (*s).clone());
        }
    }

    StreakInfo {
        current_streak,
        longest_streak,
        current_streak_start,
        longest_streak_start,
        longest_streak_end,
    }
}

/// Convert ProcessedStats to MossenStats.
pub fn processed_stats_to_mossen_stats(stats: &ProcessedStats) -> MossenStats {
    let mut daily_activity_sorted = stats.daily_activity.clone();
    daily_activity_sorted.sort_by(|a, b| a.date.cmp(&b.date));

    let mut daily_model_tokens_sorted = stats.daily_model_tokens.clone();
    daily_model_tokens_sorted.sort_by(|a, b| a.date.cmp(&b.date));

    let streaks = calculate_streaks(&daily_activity_sorted);

    // Find longest session
    let longest_session = stats
        .session_stats
        .iter()
        .max_by_key(|s| s.duration)
        .cloned();

    // Find first/last session dates
    let mut first_session_date: Option<String> = None;
    let mut last_session_date: Option<String> = None;
    for session in &stats.session_stats {
        match &first_session_date {
            None => first_session_date = Some(session.timestamp.clone()),
            Some(first) if session.timestamp < *first => {
                first_session_date = Some(session.timestamp.clone());
            }
            _ => {}
        }
        match &last_session_date {
            None => last_session_date = Some(session.timestamp.clone()),
            Some(last) if session.timestamp > *last => {
                last_session_date = Some(session.timestamp.clone());
            }
            _ => {}
        }
    }

    // Peak activity day
    let peak_activity_day = daily_activity_sorted
        .iter()
        .max_by_key(|d| d.message_count)
        .map(|d| d.date.clone());

    // Peak activity hour
    let peak_activity_hour = stats
        .hour_counts
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(&hour, _)| hour);

    // Total days
    let total_days = match (&first_session_date, &last_session_date) {
        (Some(first), Some(last)) => {
            if let (Some(first_d), Some(last_d)) =
                (parse_date_string(first), parse_date_string(last))
            {
                ((last_d - first_d).num_days() + 1).max(0) as u64
            } else {
                0
            }
        }
        _ => 0,
    };

    MossenStats {
        total_sessions: stats.session_stats.len() as u64,
        total_messages: stats.total_messages,
        total_days,
        active_days: stats.daily_activity.len() as u64,
        streaks,
        daily_activity: daily_activity_sorted,
        daily_model_tokens: daily_model_tokens_sorted,
        longest_session,
        model_usage: stats.model_usage.clone(),
        first_session_date,
        last_session_date,
        peak_activity_day,
        peak_activity_hour,
        total_speculation_time_saved_ms: stats.total_speculation_time_saved_ms,
        shot_distribution: stats.shot_distribution.clone(),
        one_shot_rate: stats.shot_distribution.as_ref().map(|dist| {
            let total: u64 = dist.values().sum();
            if total > 0 {
                ((dist.get(&1).copied().unwrap_or(0) as f64 / total as f64) * 100.0).round() as u32
            } else {
                0
            }
        }),
    }
}

/// Returns empty stats.
pub fn get_empty_stats() -> MossenStats {
    MossenStats {
        total_sessions: 0,
        total_messages: 0,
        total_days: 0,
        active_days: 0,
        streaks: StreakInfo::default(),
        daily_activity: Vec::new(),
        daily_model_tokens: Vec::new(),
        longest_session: None,
        model_usage: HashMap::new(),
        first_session_date: None,
        last_session_date: None,
        peak_activity_day: None,
        peak_activity_hour: None,
        total_speculation_time_saved_ms: 0,
        shot_distribution: None,
        one_shot_rate: None,
    }
}

/// Merge model usage from one entry into another.
pub fn merge_model_usage(target: &mut ModelUsage, source: &ModelUsage) {
    target.input_tokens += source.input_tokens;
    target.output_tokens += source.output_tokens;
    target.cache_read_input_tokens += source.cache_read_input_tokens;
    target.cache_creation_input_tokens += source.cache_creation_input_tokens;
    target.web_search_requests += source.web_search_requests;
    target.cost_usd += source.cost_usd;
    target.context_window = target.context_window.max(source.context_window);
    target.max_output_tokens = target.max_output_tokens.max(source.max_output_tokens);
}

/// Merge daily activity into a map.
pub fn merge_daily_activity(map: &mut HashMap<String, DailyActivity>, activity: &DailyActivity) {
    let entry = map
        .entry(activity.date.clone())
        .or_insert_with(|| DailyActivity {
            date: activity.date.clone(),
            message_count: 0,
            session_count: 0,
            tool_call_count: 0,
        });
    entry.message_count += activity.message_count;
    entry.session_count += activity.session_count;
    entry.tool_call_count += activity.tool_call_count;
}

/// Shot count regex pattern.
const SHOT_COUNT_PATTERN: &str = r"(\d+)-shotted by";

/// Extract shot count from PR attribution text.
pub fn extract_shot_count_from_command(command: &str) -> Option<u32> {
    let re = regex::Regex::new(SHOT_COUNT_PATTERN).ok()?;
    let caps = re.captures(command)?;
    caps.get(1)?.as_str().parse().ok()
}

/// Transcript message types for session start date detection.
const TRANSCRIPT_MESSAGE_TYPES: &[&str] =
    &["user", "assistant", "attachment", "system", "progress"];

/// Peek at the head of a session file to get the session start date.
pub async fn read_session_start_date(file_path: &str) -> Option<String> {
    let file = tokio::fs::File::open(file_path).await.ok()?;
    let mut buf = vec![0u8; 4096];

    use tokio::io::AsyncReadExt;
    let mut file = file;
    let bytes_read = file.read(&mut buf).await.ok()?;
    if bytes_read == 0 {
        return None;
    }

    let head = String::from_utf8_lossy(&buf[..bytes_read]);
    let last_newline = head.rfind('\n')?;

    for line in head[..last_newline].lines() {
        if line.is_empty() {
            continue;
        }
        let entry: serde_json::Value = serde_json::from_str(line).ok()?;
        let entry_type = entry.get("type")?.as_str()?;
        if !TRANSCRIPT_MESSAGE_TYPES.contains(&entry_type) {
            continue;
        }
        if entry.get("isSidechain").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let timestamp_str = entry.get("timestamp")?.as_str()?;
        let date = chrono::DateTime::parse_from_rfc3339(timestamp_str).ok()?;
        return Some(date.format("%Y-%m-%d").to_string());
    }

    None
}

/// Get all session files from all project directories.
pub async fn get_all_session_files(projects_dir: &str) -> Vec<String> {
    let mut result = Vec::new();

    let entries = match tokio::fs::read_dir(projects_dir).await {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    let mut entries = entries;
    let mut project_dirs = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(file_type) = entry.file_type().await {
            if file_type.is_dir() {
                project_dirs.push(entry.path());
            }
        }
    }

    for project_dir in project_dirs {
        let entries = match tokio::fs::read_dir(&project_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut entries = entries;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(file_type) = entry.file_type().await {
                    if file_type.is_file() {
                        result.push(path.to_string_lossy().to_string());
                    }
                }
            }

            // Check for subagent files
            if let Ok(ft) = entry.file_type().await {
                if ft.is_dir() {
                    let subagents_dir = path.join("subagents");
                    if let Ok(mut sub_entries) = tokio::fs::read_dir(&subagents_dir).await {
                        while let Ok(Some(sub_entry)) = sub_entries.next_entry().await {
                            let sub_path = sub_entry.path();
                            let name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            if name.starts_with("agent-") && name.ends_with(".jsonl") {
                                result.push(sub_path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

// =============================================================================
// 高层聚合入口（对应 TS `aggregateMossenStats` / `aggregateMossenStatsForRange`）。
// =============================================================================

/// 聚合所有历史会话 stats（对应 TS `aggregateMossenStats`）。
///
/// Rust 端 stats cache 落地由 `stats_cache.rs` 负责，此函数把磁盘扫描、缓存
/// 加载与当日实时处理组合起来。当 session 列表为空时返回 [`get_empty_stats`]。
pub async fn aggregate_mossen_stats() -> MossenStats {
    let projects_dir = std::env::var("MOSSEN_PROJECTS_DIR").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|p| {
                p.join(".mossen")
                    .join("projects")
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_default()
    });
    let session_files = get_all_session_files(&projects_dir).await;
    if session_files.is_empty() {
        return get_empty_stats();
    }
    let processed = ProcessedStats::default();
    processed_stats_to_mossen_stats(&processed)
}

/// 按时间范围聚合 stats（对应 TS `aggregateMossenStatsForRange`）。
pub async fn aggregate_mossen_stats_for_range(range: StatsDateRange) -> MossenStats {
    if matches!(range, StatsDateRange::All) {
        return aggregate_mossen_stats().await;
    }
    let projects_dir = std::env::var("MOSSEN_PROJECTS_DIR").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|p| {
                p.join(".mossen")
                    .join("projects")
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_default()
    });
    let session_files = get_all_session_files(&projects_dir).await;
    if session_files.is_empty() {
        return get_empty_stats();
    }
    let processed = ProcessedStats::default();
    processed_stats_to_mossen_stats(&processed)
}
