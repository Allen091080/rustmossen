//! Teammate utilities for agent swarm coordination.
//!
//! Identifies whether this instance runs as a spawned teammate in a swarm,
//! manages dynamic team context, and provides team coordination helpers.

use once_cell::sync::Lazy;
use parking_lot::RwLock;

/// Teammate context for in-process teammates.
#[derive(Debug, Clone)]
pub struct TeammateContext {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: Option<String>,
}

/// Dynamic team context set at runtime.
#[derive(Debug, Clone)]
pub struct DynamicTeamContext {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: Option<String>,
}

/// Global dynamic team context.
static DYNAMIC_TEAM_CONTEXT: Lazy<RwLock<Option<DynamicTeamContext>>> =
    Lazy::new(|| RwLock::new(None));

/// Thread-local teammate context (simulates AsyncLocalStorage).
thread_local! {
    static TEAMMATE_CONTEXT: std::cell::RefCell<Option<TeammateContext>> =
        std::cell::RefCell::new(None);
}

/// Get the current in-process teammate context.
pub fn get_teammate_context() -> Option<TeammateContext> {
    TEAMMATE_CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// Check if running as an in-process teammate.
pub fn is_in_process_teammate() -> bool {
    TEAMMATE_CONTEXT.with(|ctx| ctx.borrow().is_some())
}

/// Create a teammate context.
pub fn create_teammate_context(
    agent_id: String,
    agent_name: String,
    team_name: String,
    color: Option<String>,
    plan_mode_required: bool,
    parent_session_id: Option<String>,
) -> TeammateContext {
    TeammateContext {
        agent_id,
        agent_name,
        team_name,
        color,
        plan_mode_required,
        parent_session_id,
    }
}

/// Run a closure with a teammate context.
pub fn run_with_teammate_context<F, R>(ctx: TeammateContext, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEAMMATE_CONTEXT.with(|cell| {
        let prev = cell.borrow().clone();
        *cell.borrow_mut() = Some(ctx);
        let result = f();
        *cell.borrow_mut() = prev;
        result
    })
}

/// Get the parent session ID.
pub fn get_parent_session_id() -> Option<String> {
    if let Some(ctx) = get_teammate_context() {
        return Some(ctx.parent_session_id?);
    }
    DYNAMIC_TEAM_CONTEXT
        .read()
        .as_ref()
        .and_then(|c| c.parent_session_id.clone())
}

/// Set the dynamic team context.
pub fn set_dynamic_team_context(context: Option<DynamicTeamContext>) {
    *DYNAMIC_TEAM_CONTEXT.write() = context;
}

/// Clear the dynamic team context.
pub fn clear_dynamic_team_context() {
    *DYNAMIC_TEAM_CONTEXT.write() = None;
}

/// Get the current dynamic team context.
pub fn get_dynamic_team_context() -> Option<DynamicTeamContext> {
    DYNAMIC_TEAM_CONTEXT.read().clone()
}

/// Get the agent ID.
pub fn get_agent_id() -> Option<String> {
    if let Some(ctx) = get_teammate_context() {
        return Some(ctx.agent_id);
    }
    DYNAMIC_TEAM_CONTEXT
        .read()
        .as_ref()
        .map(|c| c.agent_id.clone())
}

/// Get the agent name.
pub fn get_agent_name() -> Option<String> {
    if let Some(ctx) = get_teammate_context() {
        return Some(ctx.agent_name);
    }
    DYNAMIC_TEAM_CONTEXT
        .read()
        .as_ref()
        .map(|c| c.agent_name.clone())
}

/// Get the team name.
pub fn get_team_name(team_context: Option<&str>) -> Option<String> {
    if let Some(ctx) = get_teammate_context() {
        return Some(ctx.team_name);
    }
    if let Some(name) = DYNAMIC_TEAM_CONTEXT
        .read()
        .as_ref()
        .map(|c| c.team_name.clone())
    {
        if !name.is_empty() {
            return Some(name);
        }
    }
    team_context.map(|s| s.to_string())
}

/// Check if this session is running as a teammate.
pub fn is_teammate() -> bool {
    if is_in_process_teammate() {
        return true;
    }
    let ctx = DYNAMIC_TEAM_CONTEXT.read();
    ctx.as_ref()
        .map(|c| !c.agent_id.is_empty() && !c.team_name.is_empty())
        .unwrap_or(false)
}

/// Get the teammate's assigned color.
pub fn get_teammate_color() -> Option<String> {
    if let Some(ctx) = get_teammate_context() {
        return ctx.color;
    }
    DYNAMIC_TEAM_CONTEXT
        .read()
        .as_ref()
        .and_then(|c| c.color.clone())
}

/// Check if plan mode is required.
pub fn is_plan_mode_required() -> bool {
    if let Some(ctx) = get_teammate_context() {
        return ctx.plan_mode_required;
    }
    if let Some(ctx) = DYNAMIC_TEAM_CONTEXT.read().as_ref() {
        return ctx.plan_mode_required;
    }
    std::env::var("MOSSEN_CODE_PLAN_MODE_REQUIRED")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Check if this session is a team lead.
pub fn is_team_lead(lead_agent_id: Option<&str>) -> bool {
    let lead_id = match lead_agent_id {
        Some(id) if !id.is_empty() => id,
        _ => return false,
    };

    let my_agent_id = get_agent_id();

    if let Some(ref my_id) = my_agent_id {
        if my_id == lead_id {
            return true;
        }
    }

    // Backwards compat: no agent ID set means original session (the lead)
    if my_agent_id.is_none() {
        return true;
    }

    false
}

/// Task type for teammate tracking.
#[derive(Debug, Clone)]
pub struct TeammateTask {
    pub task_type: String,
    pub status: String,
    pub is_idle: bool,
}

/// Check if there are active in-process teammates.
pub fn has_active_in_process_teammates(tasks: &[TeammateTask]) -> bool {
    tasks
        .iter()
        .any(|t| t.task_type == "in_process_teammate" && t.status == "running")
}

/// Check if there are working (non-idle) in-process teammates.
pub fn has_working_in_process_teammates(tasks: &[TeammateTask]) -> bool {
    tasks
        .iter()
        .any(|t| t.task_type == "in_process_teammate" && t.status == "running" && !t.is_idle)
}

/// Wait for all working teammates to become idle.
pub async fn wait_for_teammates_to_become_idle(
    tasks: &[TeammateTask],
    on_idle: impl Fn() + Send + 'static,
) {
    let working_count = tasks
        .iter()
        .filter(|t| t.task_type == "in_process_teammate" && t.status == "running" && !t.is_idle)
        .count();

    if working_count == 0 {
        return;
    }

    // In a real implementation, this would set up callbacks.
    // For now, immediately resolve since we don't have the reactive state system.
    on_idle();
}
