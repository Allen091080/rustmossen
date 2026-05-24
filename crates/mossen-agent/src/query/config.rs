/// Query configuration — immutable values snapshotted once at query() entry.

/// Trait for external dependencies
pub trait QueryConfigContext: Send + Sync {
    fn get_session_id(&self) -> String;
    fn check_statsig_feature_gate(&self, gate: &str) -> bool;
    fn is_env_truthy(&self, key: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct QueryGates {
    pub streaming_tool_execution: bool,
    pub emit_tool_use_summaries: bool,
    pub fast_mode_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct QueryConfig {
    pub session_id: String,
    pub gates: QueryGates,
}

pub fn build_query_config(ctx: &dyn QueryConfigContext) -> QueryConfig {
    QueryConfig {
        session_id: ctx.get_session_id(),
        gates: QueryGates {
            streaming_tool_execution: ctx
                .check_statsig_feature_gate("mossen_streaming_tool_execution2"),
            emit_tool_use_summaries: ctx.is_env_truthy("MOSSEN_CODE_EMIT_TOOL_USE_SUMMARIES"),
            fast_mode_enabled: !ctx.is_env_truthy("MOSSEN_CODE_DISABLE_FAST_MODE"),
        },
    }
}
