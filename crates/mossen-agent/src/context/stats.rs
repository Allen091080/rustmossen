//! Stats store — metrics collection with histograms and reservoir sampling.
//!
//! Translates: context/stats.tsx
//! React context/provider → struct-based store.

use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

const RESERVOIR_SIZE: usize = 1024;

/// Histogram data for a single metric.
#[derive(Debug, Clone)]
struct Histogram {
    reservoir: Vec<f64>,
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
}

/// Compute a percentile from a sorted array.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = p / 100.0 * (sorted.len() as f64 - 1.0);
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    if lower == upper || upper >= sorted.len() {
        return sorted[lower.min(sorted.len() - 1)];
    }
    sorted[lower] + (sorted[upper] - sorted[lower]) * (index - lower as f64)
}

/// Inner mutable state of the stats store.
#[derive(Debug, Default)]
struct StatsInner {
    metrics: HashMap<String, f64>,
    histograms: HashMap<String, Histogram>,
    sets: HashMap<String, HashSet<String>>,
}

/// Stats store — thread-safe metrics collection.
///
/// Provides increment, set, observe (histogram), and add (set) operations.
/// `get_all()` flushes all metrics including histogram percentiles.
#[derive(Debug, Clone)]
pub struct StatsStore {
    inner: Arc<RwLock<StatsInner>>,
}

impl StatsStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StatsInner::default())),
        }
    }

    /// Increment a counter metric by the given value (default 1).
    pub fn increment(&self, name: &str, value: f64) {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.metrics.entry(name.to_string()).or_insert(0.0);
        *entry += value;
    }

    /// Set a gauge metric to an exact value.
    pub fn set(&self, name: &str, value: f64) {
        let mut inner = self.inner.write().unwrap();
        inner.metrics.insert(name.to_string(), value);
    }

    /// Observe a value for a histogram metric (reservoir sampling).
    pub fn observe(&self, name: &str, value: f64) {
        let mut inner = self.inner.write().unwrap();
        let h = inner
            .histograms
            .entry(name.to_string())
            .or_insert(Histogram {
                reservoir: Vec::new(),
                count: 0,
                sum: 0.0,
                min: value,
                max: value,
            });
        h.count += 1;
        h.sum += value;
        if value < h.min {
            h.min = value;
        }
        if value > h.max {
            h.max = value;
        }
        // Reservoir sampling (Algorithm R)
        if h.reservoir.len() < RESERVOIR_SIZE {
            h.reservoir.push(value);
        } else {
            let j = rand::Rng::gen_range(&mut rand::thread_rng(), 0..h.count as usize);
            if j < RESERVOIR_SIZE {
                h.reservoir[j] = value;
            }
        }
    }

    /// Add a string value to a set metric (counts unique values).
    pub fn add(&self, name: &str, value: &str) {
        let mut inner = self.inner.write().unwrap();
        inner
            .sets
            .entry(name.to_string())
            .or_default()
            .insert(value.to_string());
    }

    /// Get all metrics as a flat key-value map.
    ///
    /// Histograms are expanded to: `{name}_count`, `{name}_min`, `{name}_max`,
    /// `{name}_avg`, `{name}_p50`, `{name}_p95`, `{name}_p99`.
    /// Sets are reported as their cardinality.
    pub fn get_all(&self) -> HashMap<String, f64> {
        let inner = self.inner.read().unwrap();
        let mut result: HashMap<String, f64> = inner.metrics.clone();

        for (name, h) in &inner.histograms {
            if h.count == 0 {
                continue;
            }
            result.insert(format!("{name}_count"), h.count as f64);
            result.insert(format!("{name}_min"), h.min);
            result.insert(format!("{name}_max"), h.max);
            result.insert(format!("{name}_avg"), h.sum / h.count as f64);
            let mut sorted = h.reservoir.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            result.insert(format!("{name}_p50"), percentile(&sorted, 50.0));
            result.insert(format!("{name}_p95"), percentile(&sorted, 95.0));
            result.insert(format!("{name}_p99"), percentile(&sorted, 99.0));
        }

        for (name, s) in &inner.sets {
            result.insert(name.clone(), s.len() as f64);
        }

        result
    }
}

impl Default for StatsStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `context/stats.tsx` exports.
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;
use std::sync::Mutex;

/// `stats.tsx` `createStatsStore`.
pub fn create_stats_store() -> StatsStore {
    StatsStore::new()
}

static GLOBAL_STATS: Lazy<Mutex<StatsStore>> = Lazy::new(|| Mutex::new(StatsStore::new()));

/// Lightweight handle used by `useStats` / `StatsProvider` mirrors.
#[derive(Debug, Default, Clone, Copy)]
pub struct StatsHandle;

impl StatsHandle {
    /// Snapshot the global counters/gauges.
    pub fn snapshot(&self) -> std::collections::HashMap<String, f64> {
        GLOBAL_STATS.lock().unwrap().get_all()
    }
}

/// `stats.tsx` `StatsProvider` — installs the global stats store and returns
/// a handle.
pub fn stats_provider() -> StatsHandle {
    StatsHandle
}

/// `stats.tsx` `useStats`.
pub fn use_stats() -> StatsHandle {
    StatsHandle
}

/// `stats.tsx` `useCounter` — increment a named counter and return the new
/// total. Mirrors the React hook that bumps a counter on render.
pub fn use_counter(name: &str) -> f64 {
    let store = GLOBAL_STATS.lock().unwrap();
    store.increment(name, 1.0);
    store.get_all().get(name).copied().unwrap_or(0.0)
}

/// `stats.tsx` `useGauge` — set a gauge value.
pub fn use_gauge(name: &str, value: f64) {
    GLOBAL_STATS.lock().unwrap().set(name, value);
}

/// `stats.tsx` `useTimer` — record an observation for a histogram metric.
pub fn use_timer(name: &str, value_ms: f64) {
    GLOBAL_STATS.lock().unwrap().observe(name, value_ms);
}

/// `stats.tsx` `useSet` — add a value to a set metric.
pub fn use_set(name: &str, value: &str) {
    GLOBAL_STATS.lock().unwrap().add(name, value);
}

/// Module-level stats context handle. Mirrors `export const StatsContext`.
pub static StatsContext: once_cell::sync::Lazy<std::sync::Mutex<StatsStore>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(StatsStore::default()));
