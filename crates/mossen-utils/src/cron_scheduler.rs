use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

use crate::cron_tasks::{CronJitterConfig, CronTask, DEFAULT_CRON_JITTER_CONFIG};

const CHECK_INTERVAL_MS: u64 = 1000;
const FILE_STABILITY_MS: u64 = 300;
const LOCK_PROBE_INTERVAL_MS: u64 = 5000;

/// Returns true when a recurring task was created more than `max_age_ms` ago and should
/// be deleted on its next fire. Permanent tasks never age. `max_age_ms == 0`
/// means unlimited (never ages out).
pub fn is_recurring_task_aged(task: &CronTask, now_ms: u64, max_age_ms: u64) -> bool {
    if max_age_ms == 0 {
        return false;
    }
    task.recurring.unwrap_or(false)
        && !task.permanent.unwrap_or(false)
        && now_ms.saturating_sub(task.created_at) >= max_age_ms
}

/// Options for creating a cron scheduler.
pub struct CronSchedulerOptions {
    /// Called when a task fires (regular or missed-on-startup).
    pub on_fire: Box<dyn Fn(String) + Send + Sync>,
    /// While true, firing is deferred to the next tick.
    pub is_loading: Box<dyn Fn() -> bool + Send + Sync>,
    /// When true, bypasses the isLoading gate in check().
    pub assistant_mode: bool,
    /// When provided, receives the full CronTask on normal fires.
    pub on_fire_task: Option<Box<dyn Fn(CronTask) + Send + Sync>>,
    /// When provided, receives the missed one-shot tasks on initial load.
    pub on_missed: Option<Box<dyn Fn(Vec<CronTask>) + Send + Sync>>,
    /// Directory containing .mossen/scheduled_tasks.json.
    pub dir: Option<String>,
    /// Owner key written into the lock file.
    pub lock_identity: Option<String>,
    /// Returns the cron jitter config to use for this tick.
    pub get_jitter_config: Option<Box<dyn Fn() -> CronJitterConfig + Send + Sync>>,
    /// Killswitch: polled once per check() tick.
    pub is_killed: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    /// Per-task gate applied before any side effect.
    pub filter: Option<Box<dyn Fn(&CronTask) -> bool + Send + Sync>>,
}

/// The cron scheduler interface.
pub struct CronScheduler {
    stopped: Arc<AtomicBool>,
    is_owner: Arc<AtomicBool>,
    tasks: Arc<Mutex<Vec<CronTask>>>,
    next_fire_at: Arc<Mutex<HashMap<String, u64>>>,
    missed_asked: Arc<Mutex<HashSet<String>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    options: Arc<CronSchedulerOptions>,
}

impl CronScheduler {
    pub fn new(options: CronSchedulerOptions) -> Self {
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            is_owner: Arc::new(AtomicBool::new(false)),
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_fire_at: Arc::new(Mutex::new(HashMap::new())),
            missed_asked: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            options: Arc::new(options),
        }
    }

    /// Start the scheduler.
    pub fn start(&self) {
        self.stopped.store(false, Ordering::SeqCst);

        let stopped = Arc::clone(&self.stopped);
        let is_owner = Arc::clone(&self.is_owner);
        let tasks = Arc::clone(&self.tasks);
        let next_fire_at = Arc::clone(&self.next_fire_at);
        let _missed_asked = Arc::clone(&self.missed_asked);
        let in_flight = Arc::clone(&self.in_flight);
        let options = Arc::clone(&self.options);

        // If dir is explicitly given, enable immediately
        if options.dir.is_some() {
            let stopped_clone = Arc::clone(&stopped);
            let is_owner_clone = Arc::clone(&is_owner);
            let tasks_clone = Arc::clone(&tasks);
            let next_fire_at_clone = Arc::clone(&next_fire_at);
            let in_flight_clone = Arc::clone(&in_flight);
            let options_clone = Arc::clone(&options);

            tokio::spawn(async move {
                Self::enable_loop(
                    stopped_clone,
                    is_owner_clone,
                    tasks_clone,
                    next_fire_at_clone,
                    in_flight_clone,
                    options_clone,
                )
                .await;
            });
            return;
        }

        // Poll until enabled, then start check loop
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(CHECK_INTERVAL_MS));
            loop {
                interval.tick().await;
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                // In a real implementation, would check getScheduledTasksEnabled()
                // For now, enable immediately if assistant_mode
                if options.assistant_mode {
                    Self::enable_loop(stopped, is_owner, tasks, next_fire_at, in_flight, options)
                        .await;
                    break;
                }
            }
        });
    }

    /// Stop the scheduler.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.is_owner.store(false, Ordering::SeqCst);
    }

    /// Get the next fire time across all loaded tasks, or None if nothing is scheduled.
    pub async fn get_next_fire_time(&self) -> Option<u64> {
        let next_fire_at = self.next_fire_at.lock().await;
        let mut min = u64::MAX;
        for &t in next_fire_at.values() {
            if t < min {
                min = t;
            }
        }
        if min == u64::MAX {
            None
        } else {
            Some(min)
        }
    }

    async fn enable_loop(
        stopped: Arc<AtomicBool>,
        is_owner: Arc<AtomicBool>,
        tasks: Arc<Mutex<Vec<CronTask>>>,
        next_fire_at: Arc<Mutex<HashMap<String, u64>>>,
        in_flight: Arc<Mutex<HashSet<String>>>,
        options: Arc<CronSchedulerOptions>,
    ) {
        if stopped.load(Ordering::SeqCst) {
            return;
        }

        // Attempt to acquire scheduler lock
        is_owner.store(true, Ordering::SeqCst);

        // Load tasks initially
        Self::load_tasks(&tasks, &options, true, &next_fire_at, &in_flight).await;

        // Start check loop
        let mut interval = time::interval(Duration::from_millis(CHECK_INTERVAL_MS));
        loop {
            interval.tick().await;
            if stopped.load(Ordering::SeqCst) {
                break;
            }
            Self::check(&is_owner, &tasks, &next_fire_at, &in_flight, &options).await;
        }
    }

    async fn load_tasks(
        tasks: &Arc<Mutex<Vec<CronTask>>>,
        _options: &Arc<CronSchedulerOptions>,
        _initial: bool,
        _next_fire_at: &Arc<Mutex<HashMap<String, u64>>>,
        _in_flight: &Arc<Mutex<HashSet<String>>>,
    ) {
        // In a real implementation, would read from cronTasks file
        // For now, tasks are empty until populated externally
        let mut t = tasks.lock().await;
        *t = Vec::new();
    }

    async fn check(
        is_owner: &Arc<AtomicBool>,
        tasks: &Arc<Mutex<Vec<CronTask>>>,
        next_fire_at: &Arc<Mutex<HashMap<String, u64>>>,
        in_flight: &Arc<Mutex<HashSet<String>>>,
        options: &Arc<CronSchedulerOptions>,
    ) {
        if let Some(ref is_killed) = options.is_killed {
            if is_killed() {
                return;
            }
        }

        if (options.is_loading)() && !options.assistant_mode {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let jitter_cfg = options
            .get_jitter_config
            .as_ref()
            .map(|f| f())
            .unwrap_or(DEFAULT_CRON_JITTER_CONFIG);

        let tasks_snapshot = tasks.lock().await.clone();
        let mut seen = HashSet::new();
        let mut fired_file_recurring = Vec::new();

        if is_owner.load(Ordering::SeqCst) {
            for task in &tasks_snapshot {
                if let Some(ref filter) = options.filter {
                    if !filter(task) {
                        continue;
                    }
                }
                seen.insert(task.id.clone());

                {
                    let in_flight_guard = in_flight.lock().await;
                    if in_flight_guard.contains(&task.id) {
                        continue;
                    }
                }

                let mut nfa = next_fire_at.lock().await;
                let next = nfa.entry(task.id.clone()).or_insert_with(|| {
                    // Compute initial next-fire time
                    // Simplified: use created_at + interval
                    task.created_at + 60_000 // Default 1 minute after creation
                });

                if now < *next {
                    continue;
                }

                // Fire the task
                if let Some(ref on_fire_task) = options.on_fire_task {
                    on_fire_task(task.clone());
                } else {
                    (options.on_fire)(task.prompt.clone());
                }

                let aged = is_recurring_task_aged(task, now, jitter_cfg.recurring_max_age_ms);

                if task.recurring.unwrap_or(false) && !aged {
                    // Reschedule from now
                    let new_next = now + 60_000; // Simplified
                    *next = new_next;
                    fired_file_recurring.push(task.id.clone());
                } else {
                    // One-shot or aged-out: remove
                    let mut ifl = in_flight.lock().await;
                    ifl.insert(task.id.clone());
                    nfa.remove(&task.id);
                }
            }
        }

        // Evict schedule entries for tasks no longer present
        if seen.is_empty() {
            let mut nfa = next_fire_at.lock().await;
            nfa.clear();
        } else {
            let mut nfa = next_fire_at.lock().await;
            nfa.retain(|id, _| seen.contains(id));
        }
    }
}

/// 对应 TS `createCronScheduler(options)`：构造调度器实例。
///
/// Rust 端的 [`CronScheduler`] 直接通过 `CronScheduler::new(options)` 构造，
/// 该函数提供与 TS 同名的工厂入口。
pub fn create_cron_scheduler(options: CronSchedulerOptions) -> CronScheduler {
    CronScheduler::new(options)
}

/// Build the missed-task notification text.
pub fn build_missed_task_notification(missed: &[CronTask]) -> String {
    let plural = missed.len() > 1;
    let header = format!(
        "The following one-shot scheduled task{} missed while Mossen was not running. \
         {} already been removed from .mossen/scheduled_tasks.json.\n\n\
         Do NOT execute {} yet. \
         First use the AskUserQuestion tool to ask whether to run {} now. \
         Only execute if the user confirms.",
        if plural { "s were" } else { " was" },
        if plural { "They have" } else { "It has" },
        if plural {
            "these prompts"
        } else {
            "this prompt"
        },
        if plural { "each one" } else { "it" },
    );

    let blocks: Vec<String> = missed
        .iter()
        .map(|t| {
            let meta = format!(
                "[cron: {}, created {}]",
                t.cron,
                format_timestamp(t.created_at)
            );
            // Use a fence one longer than any backtick run in the prompt
            let _longest_run = t.prompt.matches('`').fold(0usize, |max, _| max.max(1));
            let mut run_length = 0usize;
            let mut current_run = 0usize;
            for ch in t.prompt.chars() {
                if ch == '`' {
                    current_run += 1;
                    if current_run > run_length {
                        run_length = current_run;
                    }
                } else {
                    current_run = 0;
                }
            }
            let fence_len = std::cmp::max(3, run_length + 1);
            let fence: String = std::iter::repeat('`').take(fence_len).collect();
            format!("{}\n{}\n{}\n{}", meta, fence, t.prompt, fence)
        })
        .collect();

    format!("{}\n\n{}", header, blocks.join("\n\n"))
}

fn format_timestamp(ms: u64) -> String {
    // Simple timestamp formatting
    let secs = ms / 1000;
    let datetime = chrono::DateTime::from_timestamp(secs as i64, 0);
    match datetime {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("{}ms", ms),
    }
}
