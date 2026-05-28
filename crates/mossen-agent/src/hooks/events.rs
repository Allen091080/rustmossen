//! # events — Hook 事件元数据
//!
//! 对应 TS `utils/hooks/hooksConfigManager.ts` 中的 `getHookEventMetadata()`。
//! 提供 Hook 事件的描述信息和匹配器元数据。

use mossen_types::hooks::HookEvent;

/// Matcher 元数据 — 定义事件匹配的字段和可选值。
///
/// 对应 TS `MatcherMetadata`。
#[derive(Debug, Clone)]
pub struct MatcherMetadata {
    /// 匹配的字段名。
    pub field_to_match: &'static str,
    /// 可选的匹配值列表（空列表表示动态填充）。
    pub values: Vec<String>,
}

/// Hook 事件元数据 — 描述单个事件的用途。
///
/// 对应 TS `HookEventMetadata`。
#[derive(Debug, Clone)]
pub struct HookEventMetadata {
    /// 事件摘要。
    pub summary: &'static str,
    /// 事件详细描述。
    pub description: &'static str,
    /// 匹配器元数据（可选）。
    pub matcher_metadata: Option<MatcherMetadata>,
}

/// 获取 Hook 事件的元数据描述。
///
/// 对应 TS `getHookEventMetadata()` → Rust `describe_event()`。
/// 按文档 12 命名转换。
pub fn describe_event(event: HookEvent, tool_names: &[String]) -> HookEventMetadata {
    match event {
        HookEvent::PreToolUse => HookEventMetadata {
            summary: "Before tool execution",
            description: "Input to command is JSON of tool call arguments.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and block tool call\nOther exit codes - show stderr to user only but continue with tool call",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name",
                values: tool_names.to_vec(),
            }),
        },
        HookEvent::PostToolUse => HookEventMetadata {
            summary: "After tool execution",
            description: "Input to command is JSON with fields \"inputs\" (tool call arguments) and \"response\" (tool call response).\nExit code 0 - stdout shown in transcript mode\nExit code 2 - show stderr to model immediately\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name",
                values: tool_names.to_vec(),
            }),
        },
        HookEvent::PostToolUseFailure => HookEventMetadata {
            summary: "After tool execution fails",
            description: "Input to command is JSON with tool_name, tool_input, tool_use_id, error, error_type, is_interrupt, and is_timeout.\nExit code 0 - stdout shown in transcript mode\nExit code 2 - show stderr to model immediately\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name",
                values: tool_names.to_vec(),
            }),
        },
        HookEvent::PostSampling => HookEventMetadata {
            summary: "After model response sampling completes",
            description: "Input to command is JSON with assistant_response, system_prompt, and query_source. Hook output is observability-only and does not alter the turn.",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "query_source",
                values: vec![
                    "repl".into(),
                    "sdk".into(),
                    "custom_backend".into(),
                    "agent_task".into(),
                    "background".into(),
                    "pipeline".into(),
                ],
            }),
        },
        HookEvent::PermissionDenied => HookEventMetadata {
            summary: "After auto mode classifier denies a tool call",
            description: "Input to command is JSON with tool_name, tool_input, tool_use_id, and reason.\nReturn {\"hookSpecificOutput\":{\"hookEventName\":\"PermissionDenied\",\"retry\":true}} to tell the model it may retry.\nExit code 0 - stdout shown in transcript mode\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name",
                values: tool_names.to_vec(),
            }),
        },
        HookEvent::Notification => HookEventMetadata {
            summary: "When notifications are sent",
            description: "Input to command is JSON with notification message and type.\nExit code 0 - stdout/stderr not shown\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "notification_type",
                values: vec![
                    "permission_prompt".into(),
                    "idle_prompt".into(),
                    "auth_success".into(),
                    "elicitation_dialog".into(),
                    "elicitation_complete".into(),
                    "elicitation_response".into(),
                ],
            }),
        },
        HookEvent::UserPromptSubmit => HookEventMetadata {
            summary: "When the user submits a prompt",
            description: "Input to command is JSON with original user prompt text.\nExit code 0 - stdout shown to Mossen\nExit code 2 - block processing, erase original prompt, and show stderr to user only\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::SessionStart => HookEventMetadata {
            summary: "When a new session is started",
            description: "Input to command is JSON with session start source.\nExit code 0 - stdout shown to Mossen\nBlocking errors are ignored\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "source",
                values: vec!["startup".into(), "resume".into(), "clear".into(), "compact".into()],
            }),
        },
        HookEvent::SessionEnd => HookEventMetadata {
            summary: "When a session is ending",
            description: "Input to command is JSON with session end reason.\nExit code 0 - command completes successfully\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "reason",
                values: vec!["clear".into(), "logout".into(), "prompt_input_exit".into(), "other".into()],
            }),
        },
        HookEvent::Stop => HookEventMetadata {
            summary: "Right before Mossen concludes its response",
            description: "Exit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and continue conversation\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::StopFailure => HookEventMetadata {
            summary: "When the turn ends due to an API error",
            description: "Fires instead of Stop when an API error (rate limit, auth failure, etc.) ended the turn. Fire-and-forget — hook output and exit codes are ignored.",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "error",
                values: vec![
                    "rate_limit".into(),
                    "authentication_failed".into(),
                    "billing_error".into(),
                    "invalid_request".into(),
                    "server_error".into(),
                    "max_output_tokens".into(),
                    "unknown".into(),
                ],
            }),
        },
        HookEvent::SubagentStart => HookEventMetadata {
            summary: "When a subagent (Agent tool call) is started",
            description: "Input to command is JSON with agent_id and agent_type.\nExit code 0 - stdout shown to subagent\nBlocking errors are ignored\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "agent_type",
                values: vec![],
            }),
        },
        HookEvent::SubagentStop => HookEventMetadata {
            summary: "Right before a subagent concludes its response",
            description: "Input to command is JSON with agent_id, agent_type, and agent_transcript_path.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to subagent and continue having it run\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "agent_type",
                values: vec![],
            }),
        },
        HookEvent::PreCompact => HookEventMetadata {
            summary: "Before conversation compaction",
            description: "Input to command is JSON with compaction details.\nExit code 0 - stdout appended as custom compact instructions\nExit code 2 - block compaction\nOther exit codes - show stderr to user only but continue with compaction",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger",
                values: vec!["manual".into(), "auto".into()],
            }),
        },
        HookEvent::PostCompact => HookEventMetadata {
            summary: "After conversation compaction",
            description: "Input to command is JSON with compaction details and the summary.\nExit code 0 - stdout shown to user\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger",
                values: vec!["manual".into(), "auto".into()],
            }),
        },
        HookEvent::PermissionRequest => HookEventMetadata {
            summary: "When a permission dialog is displayed",
            description: "Input to command is JSON with tool_name, tool_input, and tool_use_id.\nOutput JSON with hookSpecificOutput containing decision to allow or deny.\nExit code 0 - use hook decision if provided\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name",
                values: tool_names.to_vec(),
            }),
        },
        HookEvent::Setup => HookEventMetadata {
            summary: "Repo setup hooks for init and maintenance",
            description: "Input to command is JSON with trigger (init or maintenance).\nExit code 0 - stdout shown to Mossen\nBlocking errors are ignored\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger",
                values: vec!["init".into(), "maintenance".into()],
            }),
        },
        HookEvent::TeammateIdle => HookEventMetadata {
            summary: "When a teammate is about to go idle",
            description: "Input to command is JSON with teammate_name and team_name.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to teammate and prevent idle\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::TaskCreated => HookEventMetadata {
            summary: "When a task is being created",
            description: "Input to command is JSON with task_id, task_subject, task_description, teammate_name, and team_name.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and prevent task creation\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::TaskCompleted => HookEventMetadata {
            summary: "When a task is being marked as completed",
            description: "Input to command is JSON with task_id, task_subject, task_description, teammate_name, and team_name.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and prevent task completion\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::Elicitation => HookEventMetadata {
            summary: "When an MCP server requests user input (elicitation)",
            description: "Input to command is JSON with mcp_server_name, message, and requested_schema.\nOutput JSON with hookSpecificOutput containing action (accept/decline/cancel) and optional content.\nExit code 0 - use hook response if provided\nExit code 2 - deny the elicitation\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "mcp_server_name",
                values: vec![],
            }),
        },
        HookEvent::ElicitationResult => HookEventMetadata {
            summary: "After a user responds to an MCP elicitation",
            description: "Input to command is JSON with mcp_server_name, action, content, mode, and elicitation_id.\nOutput JSON with hookSpecificOutput containing optional action and content to override the response.\nExit code 0 - use hook response if provided\nExit code 2 - block the response (action becomes decline)\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "mcp_server_name",
                values: vec![],
            }),
        },
        HookEvent::ConfigChange => HookEventMetadata {
            summary: "When configuration files change during a session",
            description: "Input to command is JSON with source and file_path.\nExit code 0 - allow the change\nExit code 2 - block the change from being applied to the session\nOther exit codes - show stderr to user only",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "source",
                values: vec![
                    "user_settings".into(),
                    "project_settings".into(),
                    "local_settings".into(),
                    "policy_settings".into(),
                    "skills".into(),
                ],
            }),
        },
        HookEvent::InstructionsLoaded => HookEventMetadata {
            summary: "When an instruction file (MOSSEN.md or rule) is loaded",
            description: "Input to command is JSON with file_path, memory_type, load_reason, globs, trigger_file_path, and parent_file_path.\nThis hook is observability-only and does not support blocking.",
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "load_reason",
                values: vec![
                    "session_start".into(),
                    "nested_traversal".into(),
                    "path_glob_match".into(),
                    "include".into(),
                    "compact".into(),
                ],
            }),
        },
        HookEvent::WorktreeCreate => HookEventMetadata {
            summary: "Create an isolated worktree for VCS-agnostic isolation",
            description: "Input to command is JSON with name (suggested worktree slug).\nStdout should contain the absolute path to the created worktree directory.\nExit code 0 - worktree created successfully\nOther exit codes - worktree creation failed",
            matcher_metadata: None,
        },
        HookEvent::WorktreeRemove => HookEventMetadata {
            summary: "Remove a previously created worktree",
            description: "Input to command is JSON with worktree_path (absolute path to worktree).\nExit code 0 - worktree removed successfully\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::CwdChanged => HookEventMetadata {
            summary: "After the working directory changes",
            description: "Input to command is JSON with old_cwd and new_cwd.\nMOSSEN_ENV_FILE is set — write bash exports there to apply env to subsequent BashTool commands.\nHook output can include hookSpecificOutput.watchPaths.\nExit code 0 - command completes successfully\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
        HookEvent::FileChanged => HookEventMetadata {
            summary: "When a watched file changes",
            description: "Input to command is JSON with file_path and event (change, add, unlink).\nThe matcher field specifies filenames to watch in the current directory.\nHook output can include hookSpecificOutput.watchPaths to dynamically update the watch list.\nExit code 0 - command completes successfully\nOther exit codes - show stderr to user only",
            matcher_metadata: None,
        },
    }
}
