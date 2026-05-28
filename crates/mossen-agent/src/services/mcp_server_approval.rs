//! MCP-server-approval flow — pending project-scope servers gating.
//!
//! Translates `services/mcpServerApproval.tsx`. JSX is intentionally not
//! ported; this Rust module exposes the orchestration logic only. The UI
//! layer (mossen-tui) renders the approval dialog when this function returns
//! a non-empty list of pending server names.

/// Find pending MCP servers in the `project` scope that need approval.
///
/// `project_server_names` is the list of server names declared in the project
/// MCP config. `is_pending(name)` returns true when the named server's
/// approval status is still pending.
pub fn find_pending_project_servers<F>(
    project_server_names: &[String],
    mut is_pending: F,
) -> Vec<String>
where
    F: FnMut(&str) -> bool,
{
    project_server_names
        .iter()
        .filter(|name| is_pending(name))
        .cloned()
        .collect()
}

/// TS `handleMcpjsonServerApprovals` — orchestration entry-point. Returns the
/// set of project-scope server names that still require user approval. The
/// caller (mossen-tui) decides whether to render the single-approval dialog
/// or the multi-select dialog based on the length of the returned vec.
///
/// Returns an empty `Vec` when there are no pending approvals.
pub fn handle_mcpjson_server_approvals<F>(
    project_server_names: &[String],
    is_pending: F,
) -> Vec<String>
where
    F: FnMut(&str) -> bool,
{
    find_pending_project_servers(project_server_names, is_pending)
}
