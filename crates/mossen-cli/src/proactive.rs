// proactive.rs — Translation of proactive/ directory:
// proactive/index.ts, proactive/useProactive.ts

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;

// ============================================================================
// index.ts — Proactive State Machine
// ============================================================================

const DEFAULT_TICK_MS: i64 = 60_000;

pub const DEFAULT_TICK_PROMPT: &str =
    "<tick>Continue working proactively. Make the most useful next move, or sleep if there is nothing to do.</tick>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProactiveSource {
    Command,
    Env,
    System,
    Custom,
}

impl ProactiveSource {
    pub fn from_str(s: &str) -> Self {
        match s {
            "command" => Self::Command,
            "env" => Self::Env,
            "system" => Self::System,
            _ => Self::Custom,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Env => "env",
            Self::System => "system",
            Self::Custom => "custom",
        }
    }
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static CONTEXT_BLOCKED: AtomicBool = AtomicBool::new(false);
static NEXT_TICK_AT: AtomicI64 = AtomicI64::new(0);

// 0 = null, non-zero is stored as the ProactiveSource discriminant + 1
static SOURCE: AtomicI64 = AtomicI64::new(0);

static LISTENERS: Mutex<Option<Vec<Box<dyn Fn() + Send + Sync>>>> = Mutex::new(None);

fn compute_next_tick_at() -> i64 {
    let active = ACTIVE.load(Ordering::SeqCst);
    let paused = PAUSED.load(Ordering::SeqCst);
    let blocked = CONTEXT_BLOCKED.load(Ordering::SeqCst);
    if !active || paused || blocked {
        return 0;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    now_ms + DEFAULT_TICK_MS
}

fn emit() {
    if let Ok(guard) = LISTENERS.lock() {
        if let Some(ref listeners) = *guard {
            for listener in listeners {
                listener();
            }
        }
    }
}

fn update_state() {
    let tick = compute_next_tick_at();
    NEXT_TICK_AT.store(tick, Ordering::SeqCst);
    emit();
}

pub fn subscribe_to_proactive_changes(listener: Box<dyn Fn() + Send + Sync>) -> usize {
    let mut guard = LISTENERS.lock().unwrap();
    let listeners = guard.get_or_insert_with(Vec::new);
    listeners.push(listener);
    listeners.len() - 1
}

pub fn is_proactive_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

pub fn is_proactive_paused() -> bool {
    PAUSED.load(Ordering::SeqCst) || CONTEXT_BLOCKED.load(Ordering::SeqCst)
}

pub fn get_next_tick_at() -> Option<i64> {
    let v = NEXT_TICK_AT.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        Some(v)
    }
}

pub fn activate_proactive(source: ProactiveSource) {
    ACTIVE.store(true, Ordering::SeqCst);
    PAUSED.store(false, Ordering::SeqCst);
    SOURCE.store(source as i64 + 1, Ordering::SeqCst);
    update_state();
}

pub fn deactivate_proactive() {
    ACTIVE.store(false, Ordering::SeqCst);
    PAUSED.store(false, Ordering::SeqCst);
    SOURCE.store(0, Ordering::SeqCst);
    update_state();
}

pub fn pause_proactive() {
    PAUSED.store(true, Ordering::SeqCst);
    update_state();
}

pub fn resume_proactive() {
    PAUSED.store(false, Ordering::SeqCst);
    update_state();
}

pub fn set_context_blocked(blocked: bool) {
    CONTEXT_BLOCKED.store(blocked, Ordering::SeqCst);
    update_state();
}

pub fn get_proactive_source() -> Option<ProactiveSource> {
    let v = SOURCE.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        match (v - 1) as u8 {
            0 => Some(ProactiveSource::Command),
            1 => Some(ProactiveSource::Env),
            2 => Some(ProactiveSource::System),
            _ => Some(ProactiveSource::Custom),
        }
    }
}

// ============================================================================
// useProactive.ts — Proactive Tick Scheduling (Rust equivalent)
// ============================================================================

pub struct ProactiveSchedulerConfig {
    pub is_loading: bool,
    pub queued_commands_length: usize,
    pub has_active_local_jsx_ui: bool,
    pub is_in_plan_mode: bool,
}

pub struct ProactiveScheduler {
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ProactiveScheduler {
    pub fn new() -> Self {
        Self { cancel: None }
    }

    pub fn start<F, G>(&mut self, config: ProactiveSchedulerConfig, on_submit: F, on_queue: G)
    where
        F: Fn(&str) + Send + Sync + 'static,
        G: Fn(&str) + Send + Sync + 'static,
    {
        self.stop();

        if !is_proactive_active() || is_proactive_paused() {
            return;
        }
        let next_tick = match get_next_tick_at() {
            Some(t) => t,
            None => return,
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let delay = ((next_tick - now_ms).max(0)) as u64;

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cancel = Some(tx);

        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay)) => {
                    if config.is_loading || config.queued_commands_length > 0
                        || config.has_active_local_jsx_ui || config.is_in_plan_mode
                    {
                        on_queue(DEFAULT_TICK_PROMPT);
                    } else {
                        on_submit(DEFAULT_TICK_PROMPT);
                    }
                }
                _ = rx => {
                    // Cancelled
                }
            }
        });
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
    }
}

impl Default for ProactiveScheduler {
    fn default() -> Self {
        Self::new()
    }
}
