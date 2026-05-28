use tokio::sync::broadcast;

use super::hosted_limits::RateLimitState;

/// Subscriber handle for hosted limits changes.
/// This replaces the React useHostedLimits() hook with a broadcast-based pattern.
pub struct HostedLimitsSubscription {
    rx: broadcast::Receiver<RateLimitState>,
    current: RateLimitState,
}

impl HostedLimitsSubscription {
    pub fn new(tx: &broadcast::Sender<RateLimitState>, initial: RateLimitState) -> Self {
        Self {
            rx: tx.subscribe(),
            current: initial,
        }
    }

    /// Get the current limits value.
    pub fn current(&self) -> &RateLimitState {
        &self.current
    }

    /// Wait for the next limits update, returning the new value.
    pub async fn next(&mut self) -> Option<RateLimitState> {
        match self.rx.recv().await {
            Ok(limits) => {
                self.current = limits.clone();
                Some(limits)
            }
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // On lag, try again to get the latest
                match self.rx.recv().await {
                    Ok(limits) => {
                        self.current = limits.clone();
                        Some(limits)
                    }
                    Err(_) => None,
                }
            }
        }
    }
}

/// TS `useHostedLimits()` — returns a fresh subscription handle backed by the
/// supplied broadcast sender + initial state. The Rust port models the React
/// hook as a value-type that the agent runtime polls/updates.
pub fn use_hosted_limits(
    tx: &broadcast::Sender<RateLimitState>,
    initial: RateLimitState,
) -> HostedLimitsSubscription {
    HostedLimitsSubscription::new(tx, initial)
}
