/// Token budget tracking for query continuation decisions.

const COMPLETION_THRESHOLD: f64 = 0.9;
const DIMINISHING_THRESHOLD: u64 = 500;

#[derive(Debug, Clone)]
pub struct BudgetTracker {
    pub continuation_count: u32,
    pub last_delta_tokens: u64,
    pub last_global_turn_tokens: u64,
    pub started_at: u64,
}

impl BudgetTracker {
    pub fn new() -> Self {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            continuation_count: 0,
            last_delta_tokens: 0,
            last_global_turn_tokens: 0,
            started_at,
        }
    }
}

impl Default for BudgetTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ContinueDecision {
    pub nudge_message: String,
    pub continuation_count: u32,
    pub pct: u32,
    pub turn_tokens: u64,
    pub budget: u64,
}

#[derive(Debug, Clone)]
pub struct CompletionEvent {
    pub continuation_count: u32,
    pub pct: u32,
    pub turn_tokens: u64,
    pub budget: u64,
    pub diminishing_returns: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum TokenBudgetDecision {
    Continue(ContinueDecision),
    Stop(Option<CompletionEvent>),
}

/// Trait for getting budget continuation message
pub trait TokenBudgetContext: Send + Sync {
    fn get_budget_continuation_message(&self, pct: u32, turn_tokens: u64, budget: u64) -> String;
}

pub fn check_token_budget(
    ctx: &dyn TokenBudgetContext,
    tracker: &mut BudgetTracker,
    agent_id: Option<&str>,
    budget: Option<u64>,
    global_turn_tokens: u64,
) -> TokenBudgetDecision {
    // Agents and null/zero budgets always stop
    if agent_id.is_some() || budget.is_none() || budget == Some(0) {
        return TokenBudgetDecision::Stop(None);
    }

    let budget = budget.unwrap();
    let turn_tokens = global_turn_tokens;
    let pct = ((turn_tokens as f64 / budget as f64) * 100.0).round() as u32;
    let delta_since_last = global_turn_tokens.saturating_sub(tracker.last_global_turn_tokens);

    let is_diminishing = tracker.continuation_count >= 3
        && delta_since_last < DIMINISHING_THRESHOLD
        && tracker.last_delta_tokens < DIMINISHING_THRESHOLD;

    if !is_diminishing && (turn_tokens as f64) < (budget as f64 * COMPLETION_THRESHOLD) {
        tracker.continuation_count += 1;
        tracker.last_delta_tokens = delta_since_last;
        tracker.last_global_turn_tokens = global_turn_tokens;
        let nudge_message = ctx.get_budget_continuation_message(pct, turn_tokens, budget);
        return TokenBudgetDecision::Continue(ContinueDecision {
            nudge_message,
            continuation_count: tracker.continuation_count,
            pct,
            turn_tokens,
            budget,
        });
    }

    if is_diminishing || tracker.continuation_count > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        return TokenBudgetDecision::Stop(Some(CompletionEvent {
            continuation_count: tracker.continuation_count,
            pct,
            turn_tokens,
            budget,
            diminishing_returns: is_diminishing,
            duration_ms: now.saturating_sub(tracker.started_at),
        }));
    }

    TokenBudgetDecision::Stop(None)
}

/// TS `createBudgetTracker` — constructor wrapper. Returns a default tracker.
pub fn create_budget_tracker() -> BudgetTracker {
    BudgetTracker::new()
}
