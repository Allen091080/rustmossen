// Scheduled prompts, stored in <project>/.mossen/scheduled_tasks.json.
//
// Tasks come in two flavors:
//   - One-shot (recurring: false/undefined) — fire once, then auto-delete.
//   - Recurring (recurring: true) — fire on schedule, reschedule from now,
//     persist until explicitly deleted via CronDelete or auto-expire.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    /// 5-field cron string (local time).
    pub cron: String,
    /// Prompt to enqueue when the task fires.
    pub prompt: String,
    /// Epoch ms when the task was created.
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    /// Epoch ms of the most recent fire.
    #[serde(rename = "lastFiredAt", skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<u64>,
    /// When true, the task reschedules after firing instead of being deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
    /// When true, the task is exempt from recurringMaxAgeMs auto-expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permanent: Option<bool>,
    /// Runtime-only flag. false → session-scoped (never written to disk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable: Option<bool>,
    /// Runtime-only. When set, the task was created by an in-process teammate.
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CronFile {
    tasks: Vec<CronTask>,
}

const CRON_FILE_REL: &str = ".mossen/scheduled_tasks.json";

/// Cron scheduler tuning knobs.
#[derive(Debug, Clone)]
pub struct CronJitterConfig {
    pub recurring_frac: f64,
    pub recurring_cap_ms: u64,
    pub one_shot_max_ms: u64,
    pub one_shot_floor_ms: u64,
    pub one_shot_minute_mod: u32,
    pub recurring_max_age_ms: u64,
}

pub const DEFAULT_CRON_JITTER_CONFIG: CronJitterConfig = CronJitterConfig {
    recurring_frac: 0.1,
    recurring_cap_ms: 15 * 60 * 1000,
    one_shot_max_ms: 90 * 1000,
    one_shot_floor_ms: 0,
    one_shot_minute_mod: 30,
    recurring_max_age_ms: 7 * 24 * 60 * 60 * 1000,
};

/// Path to the cron file.
pub fn get_cron_file_path(dir: Option<&Path>, project_root: &Path) -> PathBuf {
    let base = dir.unwrap_or(project_root);
    base.join(CRON_FILE_REL)
}

/// Read and parse .mossen/scheduled_tasks.json.
pub async fn read_cron_tasks(dir: Option<&Path>, project_root: &Path) -> Vec<CronTask> {
    let path = get_cron_file_path(dir, project_root);
    let raw = match fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let parsed: CronFile = match serde_json::from_str(&raw) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for t in parsed.tasks {
        if t.id.is_empty() || t.cron.is_empty() || t.prompt.is_empty() || t.created_at == 0 {
            continue;
        }
        // Validate cron string
        if parse_cron_expression(&t.cron).is_none() {
            continue;
        }
        out.push(CronTask {
            id: t.id,
            cron: t.cron,
            prompt: t.prompt,
            created_at: t.created_at,
            last_fired_at: t.last_fired_at,
            recurring: t.recurring,
            permanent: t.permanent,
            durable: None,
            agent_id: None,
        });
    }
    out
}

/// Sync check for whether the cron file has any valid tasks.
pub fn has_cron_tasks_sync(dir: Option<&Path>, project_root: &Path) -> bool {
    let path = get_cron_file_path(dir, project_root);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let parsed: Result<CronFile, _> = serde_json::from_str(&raw);
    match parsed {
        Ok(f) => !f.tasks.is_empty(),
        Err(_) => false,
    }
}

/// Overwrite .mossen/scheduled_tasks.json with the given tasks.
pub async fn write_cron_tasks(
    tasks: &[CronTask],
    dir: Option<&Path>,
    project_root: &Path,
) -> Result<()> {
    let root = dir.unwrap_or(project_root);
    let mossen_dir = root.join(".mossen");
    fs::create_dir_all(&mossen_dir).await?;

    // Strip runtime-only `durable` and `agent_id` fields
    let clean_tasks: Vec<CronTask> = tasks
        .iter()
        .map(|t| CronTask {
            id: t.id.clone(),
            cron: t.cron.clone(),
            prompt: t.prompt.clone(),
            created_at: t.created_at,
            last_fired_at: t.last_fired_at,
            recurring: t.recurring,
            permanent: t.permanent,
            durable: None,
            agent_id: None,
        })
        .collect();

    let body = CronFile { tasks: clean_tasks };
    let content = serde_json::to_string_pretty(&body)? + "\n";
    fs::write(get_cron_file_path(dir, project_root), content).await?;
    Ok(())
}

/// Append a task. Returns the generated id.
pub async fn add_cron_task(
    cron: &str,
    prompt: &str,
    recurring: bool,
    _durable: bool,
    _agent_id: Option<&str>,
    project_root: &Path,
) -> Result<String> {
    let id = Uuid::new_v4().to_string()[..8].to_string();
    let now = now_ms();
    let task = CronTask {
        id: id.clone(),
        cron: cron.to_string(),
        prompt: prompt.to_string(),
        created_at: now,
        last_fired_at: None,
        recurring: if recurring { Some(true) } else { None },
        permanent: None,
        durable: None,
        agent_id: None,
    };

    let mut tasks = read_cron_tasks(None, project_root).await;
    tasks.push(task);
    write_cron_tasks(&tasks, None, project_root).await?;
    Ok(id)
}

/// Remove tasks by id.
pub async fn remove_cron_tasks(
    ids: &[String],
    dir: Option<&Path>,
    project_root: &Path,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let id_set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let tasks = read_cron_tasks(dir, project_root).await;
    let remaining: Vec<CronTask> = tasks.into_iter().filter(|t| !id_set.contains(t.id.as_str())).collect();
    write_cron_tasks(&remaining, dir, project_root).await
}

/// Stamp `lastFiredAt` on the given recurring tasks and write back.
pub async fn mark_cron_tasks_fired(
    ids: &[String],
    fired_at: u64,
    dir: Option<&Path>,
    project_root: &Path,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let id_set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut tasks = read_cron_tasks(dir, project_root).await;
    let mut changed = false;
    for t in &mut tasks {
        if id_set.contains(t.id.as_str()) {
            t.last_fired_at = Some(fired_at);
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    write_cron_tasks(&tasks, dir, project_root).await
}

/// File-backed tasks + session-only tasks, merged.
pub async fn list_all_cron_tasks(dir: Option<&Path>, project_root: &Path) -> Vec<CronTask> {
    read_cron_tasks(dir, project_root).await
}

/// Next fire time in epoch ms for a cron string, strictly after `from_ms`.
pub fn next_cron_run_ms(cron: &str, from_ms: u64) -> Option<u64> {
    let fields = parse_cron_expression(cron)?;
    let next = compute_next_cron_run(&fields, from_ms)?;
    Some(next)
}

/// taskId is an 8-hex-char UUID slice → parse as u32 → [0, 1).
fn jitter_frac(task_id: &str) -> f64 {
    let s = &task_id[..std::cmp::min(8, task_id.len())];
    match u32::from_str_radix(s, 16) {
        Ok(n) => n as f64 / 0x1_0000_0000_u64 as f64,
        Err(_) => 0.0,
    }
}

/// Jittered next fire time for recurring tasks.
pub fn jittered_next_cron_run_ms(
    cron: &str,
    from_ms: u64,
    task_id: &str,
    cfg: &CronJitterConfig,
) -> Option<u64> {
    let t1 = next_cron_run_ms(cron, from_ms)?;
    let t2 = next_cron_run_ms(cron, t1)?;
    let jitter = f64::min(
        jitter_frac(task_id) * cfg.recurring_frac * (t2 - t1) as f64,
        cfg.recurring_cap_ms as f64,
    );
    Some(t1 + jitter as u64)
}

/// Jittered next fire time for one-shot tasks (backward jitter).
pub fn one_shot_jittered_next_cron_run_ms(
    cron: &str,
    from_ms: u64,
    task_id: &str,
    cfg: &CronJitterConfig,
) -> Option<u64> {
    let t1 = next_cron_run_ms(cron, from_ms)?;
    // Check minute mod
    let secs = t1 / 1000;
    let mins = (secs / 60) % 60;
    if mins as u32 % cfg.one_shot_minute_mod != 0 {
        return Some(t1);
    }
    let lead = cfg.one_shot_floor_ms as f64
        + jitter_frac(task_id) * (cfg.one_shot_max_ms as f64 - cfg.one_shot_floor_ms as f64);
    Some(std::cmp::max(t1.saturating_sub(lead as u64), from_ms))
}

/// A task is "missed" when its next scheduled run is in the past.
pub fn find_missed_tasks(tasks: &[CronTask], now_ms: u64) -> Vec<&CronTask> {
    tasks
        .iter()
        .filter(|t| {
            if let Some(next) = next_cron_run_ms(&t.cron, t.created_at) {
                next < now_ms
            } else {
                false
            }
        })
        .collect()
}

// --- Cron parsing helpers ---

#[derive(Debug, Clone)]
pub struct CronFields {
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,
    pub months: Vec<u32>,
    pub days_of_week: Vec<u32>,
}

/// Parse a 5-field cron expression into expanded field sets.
pub fn parse_cron_expression(cron: &str) -> Option<CronFields> {
    let parts: Vec<&str> = cron.trim().split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let minutes = parse_field(parts[0], 0, 59)?;
    let hours = parse_field(parts[1], 0, 23)?;
    let days_of_month = parse_field(parts[2], 1, 31)?;
    let months = parse_field(parts[3], 1, 12)?;
    let days_of_week = parse_field(parts[4], 0, 6)?;
    Some(CronFields {
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
    })
}

fn parse_field(field: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    let mut values = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part == "*" {
            return Some((min..=max).collect());
        }
        if let Some(step_part) = part.strip_prefix("*/") {
            let step: u32 = step_part.parse().ok()?;
            if step == 0 {
                return None;
            }
            let mut v = min;
            while v <= max {
                values.push(v);
                v += step;
            }
        } else if part.contains('/') {
            let parts_split: Vec<&str> = part.splitn(2, '/').collect();
            let range_part = parts_split[0];
            let step: u32 = parts_split[1].parse().ok()?;
            if step == 0 {
                return None;
            }
            let (start, end) = parse_range(range_part, min, max)?;
            let mut v = start;
            while v <= end {
                values.push(v);
                v += step;
            }
        } else if part.contains('-') {
            let (start, end) = parse_range(part, min, max)?;
            for v in start..=end {
                values.push(v);
            }
        } else {
            let v: u32 = part.parse().ok()?;
            if v < min || v > max {
                return None;
            }
            values.push(v);
        }
    }
    if values.is_empty() {
        return None;
    }
    values.sort();
    values.dedup();
    Some(values)
}

fn parse_range(s: &str, min: u32, max: u32) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start: u32 = parts[0].parse().ok()?;
    let end: u32 = parts[1].parse().ok()?;
    if start < min || end > max || start > end {
        return None;
    }
    Some((start, end))
}

/// Compute the next cron run time (epoch ms) strictly after `from_ms`.
pub fn compute_next_cron_run(fields: &CronFields, from_ms: u64) -> Option<u64> {
    use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Timelike};

    let from_secs = (from_ms / 1000) as i64;
    let naive = NaiveDateTime::from_timestamp_opt(from_secs, 0)?;
    let dt = Local.from_utc_datetime(&naive);

    let mut year = dt.year();
    let mut month = dt.month();
    let mut day = dt.day();
    let mut hour = dt.hour();
    let mut minute = dt.minute() + 1; // strictly after

    let max_iterations = 366 * 24 * 60;
    for _ in 0..max_iterations {
        // Normalize overflow
        if minute >= 60 {
            minute = 0;
            hour += 1;
        }
        if hour >= 24 {
            hour = 0;
            day += 1;
        }
        let days_in_month = days_in_month_of(year, month);
        if day > days_in_month {
            day = 1;
            month += 1;
        }
        if month > 12 {
            month = 1;
            year += 1;
        }
        if year > dt.year() + 1 {
            return None;
        }

        if !fields.months.contains(&month) {
            day = 1;
            hour = 0;
            minute = 0;
            month += 1;
            continue;
        }
        if !fields.days_of_month.contains(&day) {
            hour = 0;
            minute = 0;
            day += 1;
            continue;
        }
        // Check day of week
        if let Some(local_dt) = Local.with_ymd_and_hms(year, month, day, 0, 0, 0).single() {
            let dow = local_dt.weekday().num_days_from_sunday();
            if !fields.days_of_week.contains(&dow) {
                hour = 0;
                minute = 0;
                day += 1;
                continue;
            }
        } else {
            day += 1;
            continue;
        }
        if !fields.hours.contains(&hour) {
            minute = 0;
            hour += 1;
            continue;
        }
        if !fields.minutes.contains(&minute) {
            minute += 1;
            continue;
        }
        // Found a match
        if let Some(local_dt) = Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
        {
            return Some(local_dt.timestamp() as u64 * 1000);
        }
        minute += 1;
    }
    None
}

fn days_in_month_of(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
