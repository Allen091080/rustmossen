/// Context for subagents (Agent tool agents).
#[derive(Debug, Clone)]
pub struct SubagentContext {
    pub agent_id: String,
    pub parent_session_id: Option<String>,
    pub agent_type: AgentType,
    pub subagent_name: Option<String>,
    pub is_built_in: Option<bool>,
    pub invoking_request_id: Option<String>,
    pub invocation_kind: Option<InvocationKind>,
    pub invocation_emitted: bool,
}

/// Context for in-process teammates.
#[derive(Debug, Clone)]
pub struct TeammateAgentContext {
    pub agent_id: String,
    pub agent_name: String,
    pub team_name: String,
    pub agent_color: Option<String>,
    pub plan_mode_required: bool,
    pub parent_session_id: String,
    pub is_team_lead: bool,
    pub agent_type: AgentType,
    pub invoking_request_id: Option<String>,
    pub invocation_kind: Option<InvocationKind>,
    pub invocation_emitted: bool,
}

/// Agent type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Subagent,
    Teammate,
}

/// Invocation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationKind {
    Spawn,
    Resume,
}

/// Discriminated union for agent context.
#[derive(Debug, Clone)]
pub enum AgentContext {
    Subagent(SubagentContext),
    Teammate(TeammateAgentContext),
}

impl AgentContext {
    pub fn agent_type(&self) -> AgentType {
        match self {
            Self::Subagent(_) => AgentType::Subagent,
            Self::Teammate(_) => AgentType::Teammate,
        }
    }

    pub fn agent_id(&self) -> &str {
        match self {
            Self::Subagent(ctx) => &ctx.agent_id,
            Self::Teammate(ctx) => &ctx.agent_id,
        }
    }

    pub fn invoking_request_id(&self) -> Option<&str> {
        match self {
            Self::Subagent(ctx) => ctx.invoking_request_id.as_deref(),
            Self::Teammate(ctx) => ctx.invoking_request_id.as_deref(),
        }
    }

    pub fn invocation_kind(&self) -> Option<InvocationKind> {
        match self {
            Self::Subagent(ctx) => ctx.invocation_kind,
            Self::Teammate(ctx) => ctx.invocation_kind,
        }
    }

    pub fn invocation_emitted(&self) -> bool {
        match self {
            Self::Subagent(ctx) => ctx.invocation_emitted,
            Self::Teammate(ctx) => ctx.invocation_emitted,
        }
    }

    pub fn set_invocation_emitted(&mut self, value: bool) {
        match self {
            Self::Subagent(ctx) => ctx.invocation_emitted = value,
            Self::Teammate(ctx) => ctx.invocation_emitted = value,
        }
    }
}

// Thread-local storage for agent context
tokio::task_local! {
    static AGENT_CONTEXT: AgentContext;
}

/// Get the current agent context, if any.
/// Note: In Rust, we use task_local; this returns None outside a `run_with_agent_context` scope.
pub fn get_agent_context() -> Option<AgentContext> {
    AGENT_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

/// Run an async function with the given agent context.
pub async fn run_with_agent_context<F, T>(context: AgentContext, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    AGENT_CONTEXT.scope(context, f).await
}

/// Type guard to check if context is a SubagentContext.
pub fn is_subagent_context(context: Option<&AgentContext>) -> bool {
    matches!(context, Some(AgentContext::Subagent(_)))
}

/// Type guard to check if context is a TeammateAgentContext.
pub fn is_teammate_agent_context(
    context: Option<&AgentContext>,
    agent_swarms_enabled: bool,
) -> bool {
    if agent_swarms_enabled {
        matches!(context, Some(AgentContext::Teammate(_)))
    } else {
        false
    }
}

/// Get the subagent name suitable for analytics logging.
pub fn get_subagent_log_name() -> Option<String> {
    let context = get_agent_context()?;
    match &context {
        AgentContext::Subagent(ctx) => {
            let name = ctx.subagent_name.as_ref()?;
            if ctx.is_built_in.unwrap_or(false) {
                Some(name.clone())
            } else {
                Some("user-defined".to_string())
            }
        }
        _ => None,
    }
}

/// Consume the invoking request_id for the current agent context.
/// Returns the id on the first call after a spawn/resume, then None.
pub fn consume_invoking_request_id(
    context: &mut AgentContext,
) -> Option<(String, Option<InvocationKind>)> {
    let (request_id, kind, emitted) = match context {
        AgentContext::Subagent(ctx) => (
            ctx.invoking_request_id.as_ref(),
            ctx.invocation_kind,
            ctx.invocation_emitted,
        ),
        AgentContext::Teammate(ctx) => (
            ctx.invoking_request_id.as_ref(),
            ctx.invocation_kind,
            ctx.invocation_emitted,
        ),
    };

    if request_id.is_none() || emitted {
        return None;
    }

    let result = request_id.cloned();
    context.set_invocation_emitted(true);

    result.map(|id| (id, kind))
}
