//! Async utility functions.
//!
//! Provides abort-aware sleep, timeout wrappers, and sequential execution queues.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tokio::time;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Sleep
// ---------------------------------------------------------------------------

/// Abort-responsive sleep. Resolves after `duration`, or immediately when
/// `cancel` is triggered.
pub async fn sleep(duration: Duration, cancel: Option<&CancellationToken>) {
    match cancel {
        Some(token) => {
            tokio::select! {
                _ = time::sleep(duration) => {}
                _ = token.cancelled() => {}
            }
        }
        None => {
            time::sleep(duration).await;
        }
    }
}

/// Sleep with abort that returns an error if cancelled.
pub async fn sleep_or_abort(duration: Duration, cancel: &CancellationToken) -> anyhow::Result<()> {
    tokio::select! {
        _ = time::sleep(duration) => Ok(()),
        _ = cancel.cancelled() => anyhow::bail!("aborted"),
    }
}

// ---------------------------------------------------------------------------
// Timeout
// ---------------------------------------------------------------------------

/// Race a future against a timeout. Returns an error if the future doesn't
/// complete within `duration`.
pub async fn with_timeout<T>(
    future: impl Future<Output = T>,
    duration: Duration,
    message: &str,
) -> anyhow::Result<T> {
    match time::timeout(duration, future).await {
        Ok(result) => Ok(result),
        Err(_) => anyhow::bail!("{}", message),
    }
}

/// Race a future against a timeout, returning `None` instead of error on timeout.
pub async fn with_timeout_option<T>(
    future: impl Future<Output = T>,
    duration: Duration,
) -> Option<T> {
    time::timeout(duration, future).await.ok()
}

// ---------------------------------------------------------------------------
// Sequential execution queue
// ---------------------------------------------------------------------------

/// A sequential execution wrapper that ensures concurrent calls are executed
/// one at a time in FIFO order.
///
/// Equivalent to the TS `sequential()` function — prevents race conditions
/// in operations that must run serially (file writes, DB updates, etc.).
pub struct SequentialQueue<T: Send + 'static> {
    inner: Arc<Mutex<()>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SequentialQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(())),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute a future while holding the sequential lock.
    pub async fn run<F, Fut>(&self, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _guard = self.inner.lock().await;
        f().await
    }
}

impl<T: Send + 'static> Default for SequentialQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Debounce
// ---------------------------------------------------------------------------

/// A debouncer that delays execution until a quiet period has elapsed.
pub struct Debouncer {
    notify: Arc<Notify>,
    delay: Duration,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            delay,
        }
    }

    /// Signal that activity occurred (resets the debounce timer).
    pub fn trigger(&self) {
        self.notify.notify_one();
    }

    /// Wait until the debounce period has elapsed without any triggers.
    /// Returns a future that can be used in a select! loop.
    pub async fn settled(&self) {
        loop {
            tokio::select! {
                _ = time::sleep(self.delay) => return,
                _ = self.notify.notified() => continue,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Retry with backoff
// ---------------------------------------------------------------------------

/// Simple exponential backoff retry.
pub async fn retry_with_backoff<T, E, F, Fut>(
    max_attempts: usize,
    initial_delay: Duration,
    max_delay: Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut delay = initial_delay;
    let mut last_err = None;

    for attempt in 0..max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < max_attempts {
                    time::sleep(delay).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }

    Err(last_err.unwrap())
}

// ---------------------------------------------------------------------------
// Join helpers
// ---------------------------------------------------------------------------

/// Execute multiple futures concurrently and collect results.
/// Convenience wrapper around `futures::future::join_all`.
pub async fn join_all<T, I, Fut>(futures: I) -> Vec<T>
where
    I: IntoIterator<Item = Fut>,
    Fut: Future<Output = T>,
{
    futures::future::join_all(futures).await
}

// Unused import suppression
type _PinBox<T> = Pin<Box<T>>;
