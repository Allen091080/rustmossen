//! GitHub-style activity heatmap for the terminal.
//!
//! Generates a weekly activity grid with colored block characters
//! representing message counts at percentile-based intensity levels.

use chrono::{Datelike, Duration, Local, NaiveDate};

/// Options for heatmap generation.
#[derive(Debug, Clone)]
pub struct HeatmapOptions {
    pub terminal_width: usize,
    pub show_month_labels: bool,
}

impl Default for HeatmapOptions {
    fn default() -> Self {
        Self {
            terminal_width: 80,
            show_month_labels: true,
        }
    }
}

/// Daily activity record.
#[derive(Debug, Clone)]
pub struct DailyActivity {
    pub date: String, // YYYY-MM-DD
    pub message_count: u32,
}

/// Percentile values from activity data.
struct Percentiles {
    p25: u32,
    p50: u32,
    p75: u32,
}

/// Calculate percentiles from activity data.
fn calculate_percentiles(daily_activity: &[DailyActivity]) -> Option<Percentiles> {
    let mut counts: Vec<u32> = daily_activity
        .iter()
        .map(|a| a.message_count)
        .filter(|&c| c > 0)
        .collect();

    if counts.is_empty() {
        return None;
    }

    counts.sort_unstable();
    let len = counts.len();

    Some(Percentiles {
        p25: counts[(len as f64 * 0.25) as usize],
        p50: counts[(len as f64 * 0.5) as usize],
        p75: counts[(len as f64 * 0.75) as usize],
    })
}

/// Format a date as YYYY-MM-DD string.
pub fn to_date_string(date: &NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Get intensity level (0-4) based on message count and percentiles.
fn get_intensity(message_count: u32, percentiles: Option<&Percentiles>) -> u8 {
    if message_count == 0 {
        return 0;
    }
    let Some(p) = percentiles else {
        return 0;
    };
    if message_count >= p.p75 {
        4
    } else if message_count >= p.p50 {
        3
    } else if message_count >= p.p25 {
        2
    } else {
        1
    }
}

/// ANSI escape for the Mossen orange color (#da7756).
fn mossen_orange(text: &str) -> String {
    format!("\x1b[38;2;218;119;86m{text}\x1b[0m")
}

/// ANSI escape for gray.
fn gray(text: &str) -> String {
    format!("\x1b[90m{text}\x1b[0m")
}

/// Get the colored heatmap character for a given intensity.
fn get_heatmap_char(intensity: u8) -> String {
    match intensity {
        0 => gray("·"),
        1 => mossen_orange("░"),
        2 => mossen_orange("▒"),
        3 => mossen_orange("▓"),
        4 => mossen_orange("█"),
        _ => gray("·"),
    }
}

/// Generates a GitHub-style activity heatmap for the terminal.
pub fn generate_heatmap(
    daily_activity: &[DailyActivity],
    options: &HeatmapOptions,
) -> String {
    let day_label_width = 4;
    let available_width = options.terminal_width.saturating_sub(day_label_width);
    let width = available_width.max(10).min(52);

    // Build activity map by date
    let mut activity_map = std::collections::HashMap::new();
    for activity in daily_activity {
        activity_map.insert(activity.date.clone(), activity.message_count);
    }

    let percentiles = calculate_percentiles(daily_activity);

    // Calculate date range
    let today = Local::now().date_naive();
    let today_weekday = today.weekday().num_days_from_sunday();
    let current_week_start = today - Duration::days(today_weekday as i64);
    let start_date = current_week_start - Duration::weeks((width - 1) as i64);

    // Generate grid (7 rows x width columns)
    let mut grid: Vec<Vec<String>> = vec![vec![String::new(); width]; 7];
    let mut month_starts: Vec<(u32, usize)> = Vec::new(); // (month, week)
    let mut last_month: Option<u32> = None;

    let mut current_date = start_date;
    for week in 0..width {
        for day in 0..7usize {
            if current_date > today {
                grid[day][week] = " ".to_string();
                current_date += Duration::days(1);
                continue;
            }

            let date_str = to_date_string(&current_date);
            let count = activity_map.get(&date_str).copied().unwrap_or(0);

            // Track month changes
            if day == 0 {
                let month = current_date.month();
                if last_month != Some(month) {
                    month_starts.push((month, week));
                    last_month = Some(month);
                }
            }

            let intensity = get_intensity(count, percentiles.as_ref());
            grid[day][week] = get_heatmap_char(intensity);

            current_date += Duration::days(1);
        }
    }

    // Build output
    let mut lines: Vec<String> = Vec::new();

    // Month labels
    if options.show_month_labels {
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
            "Nov", "Dec",
        ];

        let unique_months: Vec<u32> = month_starts.iter().map(|(m, _)| *m).collect();
        let label_width = width / unique_months.len().max(1);
        let month_labels: String = unique_months
            .iter()
            .map(|&month| {
                let name = month_names[(month - 1) as usize];
                format!("{:<width$}", name, width = label_width)
            })
            .collect();

        lines.push(format!("    {month_labels}"));
    }

    // Day labels
    let day_labels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

    // Grid rows
    for day in 0..7usize {
        let label = if [1, 3, 5].contains(&day) {
            format!("{:<3}", day_labels[day])
        } else {
            "   ".to_string()
        };
        let row: String = grid[day].iter().map(|s| s.as_str()).collect::<Vec<_>>().join("");
        lines.push(format!("{label} {row}"));
    }

    // Legend
    lines.push(String::new());
    lines.push(format!(
        "    Less {} {} {} {} More",
        mossen_orange("░"),
        mossen_orange("▒"),
        mossen_orange("▓"),
        mossen_orange("█"),
    ));

    lines.join("\n")
}
