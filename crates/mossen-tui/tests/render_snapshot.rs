use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mossen_agent::types::SdkMessage;
use mossen_tools::todo::TodoItem;
use mossen_tui::app::{HelpDialogState, PickerKind};
use mossen_tui::app_services::{open_message_selector, SearchPanelState};
use mossen_tui::approval_state::{PermissionAction, PermissionKind, PermissionPromptState};
use mossen_tui::layout::VirtualScroll;
use mossen_tui::message_model::{MessageData, MessageType};
use mossen_tui::render_model::{
    approval_decision_message_content, final_summary_message_content,
    ApprovalAction as RenderApprovalAction, ApprovalDecisionKind, ApprovalDecisionModel,
    ApprovalRenderModel, ApprovalRiskLevel, CommandSummaryModel, FileChangeSummaryModel,
    FinalSummaryModel, FooterItem, VerificationSummaryModel,
};
use mossen_tui::state::{
    McpConnectionState, McpServerStatus, RenderActivity, SlashCommandInfo, SlashCommandKind,
    TeammateState, TurnState, UiStage,
};
use mossen_tui::theme::Theme;
use mossen_tui::widgets::approval::ApprovalBlockWidget;
use mossen_tui::widgets::messages::MessagesWidget;
use mossen_tui::widgets::panels::{
    MemoryEntry, MemoryPanelState, ModelInfo, ModelPickerState, SkillInfo, SkillsPanelState,
};
use mossen_tui::widgets::prompt_input::{Suggestion, SuggestionKind};
use mossen_tui::{ActiveModal, App};
use mossen_types::{AssistantMessage, ContentBlock, Role, ToolUseBlock};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::Terminal;

fn msg(message_type: MessageType, content: impl Into<String>) -> MessageData {
    MessageData {
        message_type,
        content: content.into(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    }
}

fn tool_msg(message_type: MessageType, tool_name: &str, content: impl Into<String>) -> MessageData {
    let mut data = msg(message_type, content);
    data.tool_name = Some(tool_name.to_string());
    data
}

fn render_messages(messages: &[MessageData], width: u16, height: u16) -> String {
    render_messages_with_focus(messages, width, height, None)
}

fn render_messages_with_focus(
    messages: &[MessageData],
    width: u16,
    height: u16,
    focused_idx: Option<usize>,
) -> String {
    let theme = Theme::default();
    let mut scroll = VirtualScroll::new(height);
    // Contract snapshots render from the transcript top so high-signal
    // structure remains visible. Sticky-bottom behavior gets separate Batch 2
    // coverage because it depends on user scroll state.
    scroll.sticky = false;
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    let widget = MessagesWidget::new(messages, &theme, &scroll).focused_idx(focused_idx);
    widget.render(Rect::new(0, 0, width, height), &mut buf);
    buffer_to_string(&buf, width, height)
}

fn render_permission(state: &PermissionPromptState, width: u16, height: u16) -> String {
    let model = ApprovalRenderModel {
        id: "snapshot-permission".to_string(),
        tool_name: state.tool_name.clone(),
        title: state.kind.label().to_string(),
        detail_label: state.kind.detail_label().to_string(),
        detail: state.kind.detail(),
        risk: ApprovalRiskLevel::Medium,
        body: state.explanation.clone().unwrap_or_default(),
        actions: vec![
            RenderApprovalAction::Allow,
            RenderApprovalAction::AlwaysAllow,
            RenderApprovalAction::EditCommand,
            RenderApprovalAction::Deny,
        ],
        selected_action: match state.selected_action {
            PermissionAction::Allow => RenderApprovalAction::Allow,
            PermissionAction::AllowAlways => RenderApprovalAction::AlwaysAllow,
            PermissionAction::EditCommand => RenderApprovalAction::EditCommand,
            PermissionAction::Deny => RenderApprovalAction::Deny,
        },
        anchor_block_id: None,
        expanded: state.show_details,
    };
    render_approval(&model, width, height)
}

fn render_approval(model: &ApprovalRenderModel, width: u16, height: u16) -> String {
    let theme = Theme::default();
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    ApprovalBlockWidget::new(model, &theme).render(Rect::new(0, 0, width, height), &mut buf);
    buffer_to_string(&buf, width, height)
}

fn render_app_frame(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| app.render_for_test(frame))
        .expect("app frame should render");
    buffer_to_string(terminal.backend().buffer(), width, height)
}

fn buffer_to_string(buf: &Buffer, width: u16, height: u16) -> String {
    let mut out = String::new();
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        if y + 1 < height {
            out.push('\n');
        }
    }
    out
}

fn assert_snapshot_has(name: &str, snapshot: &str, needles: &[&str]) {
    let normalized = normalize_cjk_cell_spacing(snapshot);
    for needle in needles {
        assert!(
            snapshot.contains(needle) || normalized.contains(needle),
            "snapshot {name:?} did not contain {needle:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

fn assert_no_protocol_noise(name: &str, snapshot: &str) {
    let normalized = normalize_cjk_cell_spacing(snapshot);
    for needle in [
        "terminal=Completed",
        "(stop: tool_use)",
        "raw_json",
        "\"stdout\"",
        "old_todos",
        "new_todos",
        "mossen-render:final-summary",
        "null",
    ] {
        assert!(
            !snapshot.contains(needle) && !normalized.contains(needle),
            "snapshot {name:?} leaked protocol/noise marker {needle:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

fn normalize_cjk_cell_spacing(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut prev: Option<char> = None;

    while let Some(ch) = chars.next() {
        if ch == ' ' {
            if let (Some(before), Some(after)) = (prev, chars.peek().copied()) {
                if is_wide_text_cell(before) && is_wide_text_cell(after) {
                    continue;
                }
            }
        }
        out.push(ch);
        prev = Some(ch);
    }

    out
}

fn is_wide_text_cell(ch: char) -> bool {
    is_cjk(ch)
        || matches!(
            ch,
            '：' | '。'
                | '，'
                | '、'
                | '；'
                | '！'
                | '？'
                | '（'
                | '）'
                | '《'
                | '》'
                | '“'
                | '”'
                | '‘'
                | '’'
        )
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0xF900..=0xFAFF
    )
}

#[test]
fn render_snapshot_transcript_shows_markdown_and_p0_tool_cards() {
    let bash_result = serde_json::json!({
        "stdout": "Cargo.toml\ncrates\nphases\n",
        "stderr": "",
        "exit_code": 0
    })
    .to_string();
    let read_result = serde_json::json!({
        "type": "text",
        "file_path": "/Users/allen/Documents/rustmossen/Cargo.toml",
        "total_lines": 4,
        "content": "1│[workspace]\n2│resolver = \"2\"\n3│members = [\n4│    \"crates/mossen-tui\""
    })
    .to_string();
    let edit_result = serde_json::json!({
        "file_path": "/Users/allen/Documents/rustmossen/src/demo.rs",
        "old_string": "fn main() {\n    println!(\"old\");\n}\n",
        "new_string": "fn main() {\n    println!(\"new\");\n}\n"
    })
    .to_string();
    let messages = vec![
        msg(MessageType::User, "分析当前项目"),
        msg(
            MessageType::Assistant,
            "## 计划\n\n- 先读取文件\n- 再运行命令\n\n```rust\nfn main() {}\n```",
        ),
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            "command  ls -la\ncwd      /Users/allen/Documents/rustmossen",
        ),
        tool_msg(MessageType::ToolResult, "Bash", bash_result),
        tool_msg(
            MessageType::ToolUse,
            "Read",
            "file_path /Users/allen/Documents/rustmossen/Cargo.toml",
        ),
        tool_msg(MessageType::ToolResult, "Read", read_result),
        tool_msg(
            MessageType::ToolResult,
            "Grep",
            "crates/mossen-tui/src/widgets/message.rs:374:Some(\"Read\") =>",
        ),
        tool_msg(MessageType::ToolResult, "Edit", edit_result),
    ];

    let snapshot = render_messages(&messages, 120, 60);

    assert_snapshot_has(
        "transcript p0 tools",
        &snapshot,
        &[
            "分析当前项目",
            "# 计划",
            "fn main()",
            "▼ Bash",
            "stdout",
            "Cargo.toml",
            "exit 0",
            "▼ Read",
            "/Users/allen/Documents/rustmossen/Cargo.toml",
            "[workspace]",
            "▼ Grep",
            "matches",
            "message.rs:374",
            "Changed 1 file",
            "+1 -1",
            "▼ Edit",
            "/Users/allen/Documents/rustmossen/src/demo.rs",
            "old",
            "new",
        ],
    );
    assert_no_protocol_noise("transcript p0 tools", &snapshot);
}

#[test]
fn render_snapshot_focused_message_keeps_wrapped_tail_visible() {
    let messages = vec![msg(MessageType::Assistant, "abcdefg")];
    let snapshot = render_messages_with_focus(&messages, 10, 3, Some(0));

    assert_snapshot_has("focused wrapped message", &snapshot, &["abcdef"]);
    assert!(
        snapshot
            .lines()
            .any(|line| line.trim_start().starts_with('g')),
        "focused message should reserve enough height for the wrapped tail\n--- snapshot ---\n{snapshot}"
    );
    assert_no_protocol_noise("focused wrapped message", &snapshot);
}

#[test]
fn render_snapshot_p0_cards_cover_errors_todos_writes_and_agents() {
    let bash_result = serde_json::json!({
        "command": "cargo test -p mossen-tui render_snapshot",
        "cwd": "/Users/allen/Documents/rustmossen",
        "stdout": "",
        "stderr": "error: snapshot mismatch\n",
        "exit_code": 101,
        "duration_ms": 42
    })
    .to_string();
    let todo_result = serde_json::json!({
        "old_todos": [],
        "new_todos": [
            {"id": "1", "content": "补齐 P0 工具卡快照", "status": "completed"},
            {"id": "2", "content": "优化 TodoWrite 展示", "status": "in_progress"},
            {"id": "3", "content": "做真实终端回归", "status": "pending"}
        ]
    })
    .to_string();
    let write_result = serde_json::json!({
        "file_path": "/Users/allen/Documents/rustmossen/tmp/render-demo.md",
        "content": "# Demo\n\nhello\n"
    })
    .to_string();
    let agent_result = serde_json::json!({
        "agent_type": "render-review",
        "task_id": "agent-render-1",
        "stopped_reason": "EndTurn",
        "total_tool_use_count": 3,
        "total_token_count": 8120,
        "total_duration_ms": 2330,
        "last_tool_use_name": "Grep",
        "result_text": "## Findings\n\n- Tool cards stay grouped\n- Markdown survives child output",
        "messages": [
            {
                "type": "assistant",
                "content": [
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "src/render_model.rs"}},
                    {"type": "tool_use", "name": "Grep", "input": {"pattern": "ToolCard"}}
                ]
            }
        ]
    })
    .to_string();

    let messages = vec![
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            "command  cargo test -p mossen-tui render_snapshot\ncwd      /Users/allen/Documents/rustmossen",
        ),
        tool_msg(MessageType::ToolResult, "Bash", bash_result),
        tool_msg(
            MessageType::ToolUse,
            "TodoWrite",
            "completed 补齐 P0 工具卡快照\nin_progress 优化 TodoWrite 展示\npending 做真实终端回归",
        ),
        tool_msg(MessageType::ToolResult, "TodoWrite", todo_result),
        tool_msg(MessageType::ToolResult, "Write", write_result),
        tool_msg(MessageType::ToolResult, "Agent", agent_result),
    ];

    let snapshot = render_messages(&messages, 120, 44);

    assert_snapshot_has(
        "p0 errors todos writes agents",
        &snapshot,
        &[
            "▼ Bash",
            "command",
            "cargo test -p mossen-tui render_snapshot",
            "cwd",
            "stderr",
            "snapshot mismatch",
            "exit 101",
            "duration 42ms",
            "▼ TodoWrite",
            "Plan: 3 steps",
            "Active:",
            "completed",
            "in_progress",
            "pending",
            "P0",
            "▼ Write",
            "/Users/allen/Documents/rustmossen/tmp/render-demo.md",
            "Demo",
            "▼ Agent",
            "agent-render-1",
            "render-review",
            "3 nested tool calls",
            "last tool",
            "Grep",
            "tokens",
            "# Findings",
            "Tool cards stay grouped",
        ],
    );
    assert_no_protocol_noise("p0 errors todos writes agents", &snapshot);
}

#[test]
fn render_snapshot_task_workitem_tools_are_semantic_not_raw_json() {
    let messages = vec![
        tool_msg(
            MessageType::ToolUse,
            "TaskCreate",
            serde_json::json!({
                "subject": "补齐终端渲染机制",
                "description": "覆盖工作项工具的稳定卡片",
                "activeForm": "Rendering task state"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskCreate",
            serde_json::json!({
                "task": {
                    "id": "task-render-1",
                    "subject": "补齐终端渲染机制"
                }
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskList",
            serde_json::json!({
                "tasks": [
                    {"id": "task-render-1", "subject": "补齐终端渲染机制", "status": "in_progress", "blockedBy": []},
                    {"id": "task-render-2", "subject": "跑渲染回归", "status": "pending", "owner": "allen"}
                ]
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskGet",
            serde_json::json!({
                "task": {
                    "id": "task-render-1",
                    "subject": "补齐终端渲染机制",
                    "description": "让 Task 系列工具不再显示原始 JSON。",
                    "status": "in_progress",
                    "blocks": [],
                    "blockedBy": []
                }
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskUpdate",
            serde_json::json!({
                "success": true,
                "taskId": "task-render-1",
                "updatedFields": ["status", "owner"]
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskOutput",
            serde_json::json!({
                "retrieval_status": "ready",
                "task": {
                    "task_id": "task-render-1",
                    "task_type": "agent",
                    "status": "completed",
                    "description": "后台子任务完成",
                    "output": "完成检查\n没有新的阻塞",
                    "exit_code": 0
                }
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "TaskStop",
            serde_json::json!({
                "message": "Successfully stopped task: task-render-1",
                "task_id": "task-render-1",
                "task_type": "agent"
            })
            .to_string(),
        ),
    ];

    let snapshot = render_messages(&messages, 128, 72);

    assert_snapshot_has(
        "task workitem tools",
        &snapshot,
        &[
            "▼ TaskCreate",
            "subject",
            "补齐终端渲染机制",
            "task-render-1",
            "▼ TaskList",
            "2 tasks",
            "in_progress: task-render-1",
            "owner allen",
            "▼ TaskGet",
            "description",
            "JSON",
            "▼ TaskUpdate",
            "2 updated fields",
            "status, owner",
            "▼ TaskOutput",
            "retrieval ready",
            "完成检查",
            "▼ TaskStop",
            "Successfully stopped task",
        ],
    );
    assert_no_protocol_noise("task workitem tools", &snapshot);
    for raw_marker in [
        "\"task\"",
        "\"tasks\"",
        "\"retrieval_status\"",
        "\"updatedFields\"",
        "\"task_id\"",
    ] {
        assert!(
            !snapshot.contains(raw_marker),
            "task tool snapshot leaked raw JSON marker {raw_marker:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

#[test]
fn render_snapshot_permission_panel_exposes_risk_detail_and_actions() {
    let mut state = PermissionPromptState::new(
        PermissionKind::Shell {
            command: "cargo test -p mossen-tui render_snapshot".to_string(),
        },
        "Bash",
    );
    state.selected_action = PermissionAction::AllowAlways;
    state.explanation = Some("Command requires shell execution in the project workspace.".into());
    state.show_details = true;

    let snapshot = render_permission(&state, 88, 10);

    assert_snapshot_has(
        "permission shell",
        &snapshot,
        &[
            "Shell Command",
            "Waiting for approval",
            "Enter confirm",
            "Tool:",
            "Bash",
            "Command:",
            "cargo test -p mossen-tui render_snapshot",
            "Risk:",
            "Medium",
            "Allow",
            "Always",
            "Deny",
            "Command requires shell execution",
        ],
    );
    assert_no_protocol_noise("permission shell", &snapshot);
}

#[test]
fn render_snapshot_semantic_approval_model_matches_inline_surface() {
    let model = ApprovalRenderModel {
        id: "approval-1".to_string(),
        tool_name: "Bash".to_string(),
        title: "Shell Command".to_string(),
        detail_label: "Command".to_string(),
        detail: "cargo test -p mossen-tui render_snapshot".to_string(),
        risk: ApprovalRiskLevel::Medium,
        body: "Command requires shell execution in the project workspace.".to_string(),
        actions: vec![
            RenderApprovalAction::Allow,
            RenderApprovalAction::AlwaysAllow,
            RenderApprovalAction::Deny,
        ],
        selected_action: RenderApprovalAction::AlwaysAllow,
        anchor_block_id: Some("tool-0-1".to_string()),
        expanded: true,
    };

    let snapshot = render_approval(&model, 88, 9);

    assert_snapshot_has(
        "semantic approval model",
        &snapshot,
        &[
            "Shell Command",
            "Waiting for approval",
            "Tool:",
            "Bash",
            "Command:",
            "cargo test -p mossen-tui render_snapshot",
            "Risk:",
            "Medium",
            "Allow",
            "Always",
            "Deny",
            "Command requires shell execution",
        ],
    );
    assert_no_protocol_noise("semantic approval model", &snapshot);
}

#[test]
fn render_snapshot_final_summary_separates_verification_and_risks() {
    let summary = FinalSummaryModel {
        id: "summary-render".to_string(),
        success: false,
        terminal: "Failed".to_string(),
        changed_files: vec![FileChangeSummaryModel {
            path: "src/lib.rs".to_string(),
            status: "M".to_string(),
            additions: 3,
            deletions: 1,
        }],
        commands: vec![CommandSummaryModel {
            command: "cargo test".to_string(),
            cwd: Some("/repo".to_string()),
            exit_code: Some(1),
            duration_ms: Some(42),
            status: "failed".to_string(),
        }],
        verification_results: vec![VerificationSummaryModel {
            command: "cargo test".to_string(),
            status: "failed".to_string(),
            passed: false,
            exit_code: Some(1),
            duration_ms: Some(42),
        }],
        residual_risks: vec!["At least one validation command failed.".to_string()],
        notes: vec!["Re-run after fixing the failing assertion.".to_string()],
    };
    let snapshot = render_messages(
        &[msg(
            MessageType::System,
            final_summary_message_content(&summary),
        )],
        92,
        18,
    );

    assert_snapshot_has(
        "final summary verification risks",
        &snapshot,
        &[
            "Final Summary",
            "Needs attention",
            "Files:",
            "src/lib.rs",
            "Commands:",
            "cargo test",
            "Verification:",
            "1 checks",
            "Risks:",
            "At least one validation command failed",
            "Note:",
            "Re-run after fixing",
        ],
    );
    assert_no_protocol_noise("final summary verification risks", &snapshot);
}

#[test]
fn render_snapshot_app_frame_shows_inline_approval_and_footer_state() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages.push(msg(MessageType::User, "请分析当前项目"));
    app.messages
        .push(msg(MessageType::Assistant, "我会先运行一个命令确认结构。"));
    let mut approval = PermissionPromptState::new(
        PermissionKind::Shell {
            command: "ls -la".to_string(),
        },
        "Bash",
    );
    approval.explanation =
        Some("审批说明：这个命令会读取当前目录，必须跟随在回复内容下方展示。".to_string());
    approval.show_details = true;
    app.active_modal = ActiveModal::PermissionRequest(approval);

    let snapshot = render_app_frame(&mut app, 96, 30);

    assert_snapshot_has(
        "app frame inline approval",
        &snapshot,
        &[
            "请分析当前项目",
            "我会先运行一个命令确认结构",
            "Shell Command",
            "Command",
            "ls -la",
            "Risk",
            "Medium",
            "审批说明",
            "Allow",
            "Always",
            "Deny",
            "approval required",
        ],
    );
    assert_no_protocol_noise("app frame inline approval", &snapshot);
}

#[test]
fn render_snapshot_app_frame_shows_status_chrome() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.model = "example-fast".to_string();
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.state.ui_stage = UiStage::RunningCommand;
    app.messages
        .push(msg(MessageType::User, "继续验证顶部状态行"));
    app.messages.push(msg(
        MessageType::Assistant,
        "我会保持当前阶段、模型和 reasoning 在顶部可见。",
    ));

    let snapshot = render_app_frame(&mut app, 110, 28);

    assert_snapshot_has(
        "app frame status chrome",
        &snapshot,
        &[
            "running command",
            "example-fast",
            "Supervised",
            "reasoning:high",
        ],
    );
    assert_no_protocol_noise("app frame status chrome", &snapshot);
}

#[test]
fn render_snapshot_app_frame_shows_active_activity_panel() {
    let mut app = App::new();
    app.fullscreen = true;
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandOutput {
            stream: "stdout".to_string(),
            bytes: 4096,
            preview_lines: 8,
            hidden_lines: 112,
            total_lines: Some(120),
            full_log_available: true,
        });
    app.messages
        .push(msg(MessageType::User, "请继续验证命令输出活动面板"));
    app.messages.push(msg(
        MessageType::Assistant,
        "命令输出保持在活动面板中，历史区继续展示稳定 transcript。",
    ));

    let snapshot = render_app_frame(&mut app, 120, 30);

    assert_snapshot_has(
        "app frame active activity panel",
        &snapshot,
        &[
            "Command output",
            "running command",
            "stdout: 8 shown",
            "112 hidden",
            "full log",
            "transcript",
        ],
    );
    assert_no_protocol_noise("app frame active activity panel", &snapshot);
}

#[test]
fn render_snapshot_app_frame_sticky_scroll_follows_long_transcript_tail() {
    let mut app = App::new();
    app.fullscreen = true;
    let body = (0..90)
        .map(|n| format!("分析行 {n:02}: 逐行阅读代码并记录真实发现"))
        .collect::<Vec<_>>()
        .join("\n");
    app.messages.push(msg(
        MessageType::Assistant,
        format!("## 项目分析\n\n{body}\n\n最终结论：渲染必须跟到底部。"),
    ));

    let snapshot = render_app_frame(&mut app, 88, 26);

    assert_snapshot_has(
        "app sticky scroll tail",
        &snapshot,
        &["最终结论：渲染必须跟到底部。"],
    );
    assert!(
        !normalize_cjk_cell_spacing(&snapshot).contains("分析行 00"),
        "sticky frame should show the tail, not the transcript top\n--- snapshot ---\n{snapshot}"
    );
}

#[test]
fn render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.cwd = "/Users/allen/Documents/rustmossen/逐行阅读项目".to_string();
    app.engine_config.model = "example-fast".to_string();
    app.total_cost_usd = 0.15;
    app.state.is_streaming = true;
    app.messages
        .push(msg(MessageType::User, "请逐行阅读当前项目"));
    app.messages.push(msg(
        MessageType::Assistant,
        "我会沿着主渲染链路检查输入、spinner 和底部状态栏。",
    ));
    app.prompt.input.clear();
    app.prompt
        .input
        .insert_str("请继续逐行阅读 mossen-tui 的 active 渲染链路，先不要相信结构概述，继续穿过底部输入、spinner 和状态栏，直到记录真实缺口");
    app.prompt.input.move_end();

    let snapshot = render_app_frame(&mut app, 76, 20);

    assert_snapshot_has(
        "app bottom chrome multibyte",
        &snapshot,
        &[
            "真实缺口",
            "Thinking",
            "example-fast",
            "$0.15",
            "2 msgs",
            "Enter to send",
        ],
    );
    assert!(
        !normalize_cjk_cell_spacing(&snapshot).contains("请继续逐行"),
        "long prompt should scroll horizontally to the tail\n--- snapshot ---\n{snapshot}"
    );
    assert_no_protocol_noise("app bottom chrome multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_inline_prompt_shows_command_suggestions() {
    let mut app = App::new();
    app.fullscreen = false;
    app.messages.push(msg(
        MessageType::Assistant,
        "输入 / 时，命令建议必须作为底部输入区的一部分出现。",
    ));
    app.prompt.input.clear();
    app.prompt.input.insert_str("/");
    app.prompt.show_suggestions = true;
    app.prompt.selected_suggestion = Some(0);
    app.prompt.suggestions = vec![
        Suggestion {
            label: "plan".to_string(),
            description: Some("制定执行计划，记录真实缺口".to_string()),
            kind: SuggestionKind::Command,
        },
        Suggestion {
            label: "逐行阅读".to_string(),
            description: Some("从入口开始穿过活跃链路".to_string()),
            kind: SuggestionKind::Skill,
        },
    ];

    let snapshot = render_app_frame(&mut app, 76, 14);

    assert_snapshot_has(
        "app inline prompt suggestions",
        &snapshot,
        &["/plan", "真实缺口", "/逐行阅读", "Enter to send"],
    );
    assert_no_protocol_noise("app inline prompt suggestions", &snapshot);
}

#[test]
fn render_snapshot_app_frame_complex_turn_survives_resize_matrix_without_noise() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.model = "example-fast".to_string();
    app.total_cost_usd = 0.03;
    app.state.is_streaming = true;
    app.messages.push(msg(
        MessageType::User,
        "启动子 agent 逐行读代码，必须给真实结论。",
    ));
    app.messages.push(msg(
        MessageType::Assistant,
        concat!(
            "## 渲染巡检\n\n",
            "| 项 | 状态 |\n| --- | --- |\n| Markdown | 通过 |\n| 代码块 | 待验证 |\n\n",
            "```rust\nfn render_pipeline() { println!(\"真实路径\"); }\n```\n\n",
            "diff --git a/src/app.rs b/src/app.rs\n",
            "@@ -1 +1 @@\n-旧渲染\n+三层渲染\n\n",
            "最终结论：resize 后也不能泄露协议噪声。"
        ),
    ));
    app.messages.push(msg(
        MessageType::Assistant,
        "  (no content - terminal=Completed)\n\n... (stop: tool_use)  ",
    ));
    app.messages
        .push(tool_msg(MessageType::ToolUse, "Glob", "null"));
    app.messages.push(tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": "\u{1b}[31m红色 ANSI 必须清理\u{1b}[0m\n逐行阅读输出\n",
            "stderr": "",
            "exit_code": 0
        })
        .to_string(),
    ));
    app.messages.push(tool_msg(
        MessageType::ToolUse,
        "Task",
        "{\"description\":\"逐行阅读\",\"prompt\":\"读代码",
    ));
    app.messages.push(tool_msg(
        MessageType::ToolResult,
        "Edit",
        serde_json::json!({
            "file_path": "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/app.rs",
            "old_string": "旧渲染",
            "new_string": "三层渲染"
        })
        .to_string(),
    ));
    app.prompt.input.clear();
    app.prompt
        .input
        .insert_str("继续验证复杂渲染矩阵，不能把协议噪声、ANSI 或 panic 打到用户主 transcript 里");
    app.prompt.input.move_end();
    app.state.task_list.tasks = vec![TodoItem {
        id: "render-redline".to_string(),
        content: "完整渲染红线：App 活跃路径 resize/approval/footer/transcript 全部要过"
            .to_string(),
        status: "in_progress".to_string(),
    }];
    app.state
        .teammate_states
        .insert("逐行阅读子 agent".to_string(), TeammateState::Running);
    let mut approval = PermissionPromptState::new(
        PermissionKind::Shell {
            command: "cargo test -p mossen-tui render_snapshot".to_string(),
        },
        "Bash",
    );
    approval.explanation = Some("审批说明：跟随在回复下方，不能盖住 transcript。".to_string());
    approval.show_details = true;
    app.active_modal = ActiveModal::PermissionRequest(approval);

    for (width, height) in [(48, 16), (72, 18), (100, 22), (132, 30)] {
        let snapshot = render_app_frame(&mut app, width, height);
        let name = format!("complex app frame {width}x{height}");

        assert_snapshot_has(&name, &snapshot, &["Shell Command", "Bash"]);
        assert_no_protocol_noise(&name, &snapshot);
        for forbidden in [
            "Render error",
            "panicked",
            "thread 'main'",
            "index outside of buffer",
            "\u{1b}[31m",
        ] {
            assert!(
                !snapshot.contains(forbidden),
                "snapshot {name:?} leaked forbidden render text {forbidden:?}\n--- snapshot ---\n{snapshot}"
            );
        }
    }
}

#[test]
fn render_snapshot_app_frame_teammate_panel_handles_multibyte_cells() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages.push(msg(
        MessageType::Assistant,
        "主线程继续渲染时，右侧子任务面板不能覆盖 transcript。",
    ));
    app.state
        .teammate_states
        .insert("逐行阅读子任务很长名字".to_string(), TeammateState::Running);

    let snapshot = render_app_frame(&mut app, 112, 24);

    assert_snapshot_has(
        "app teammate panel multibyte",
        &snapshot,
        &["主线程继续渲染", "逐行阅读子任务"],
    );
    assert_no_protocol_noise("app teammate panel multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_task_side_panel_handles_multibyte_cells() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages.push(msg(
        MessageType::Assistant,
        "主线程渲染时，右侧任务面板不能挤压 transcript。",
    ));
    app.state.task_list.tasks = vec![TodoItem {
        id: "todo-1".to_string(),
        content: "逐行阅读活跃渲染链路并记录真实缺口，这条任务标题故意很长".to_string(),
        status: "in_progress".to_string(),
    }];

    let snapshot = render_app_frame(&mut app, 112, 24);

    assert_snapshot_has(
        "app task side panel multibyte",
        &snapshot,
        &["主线程渲染", "Tasks (1 total)", "逐行阅读"],
    );
    assert_no_protocol_noise("app task side panel multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_help_modal_handles_multibyte_columns() {
    let mut app = App::new();
    app.fullscreen = true;
    app.active_modal = ActiveModal::HelpDialog(HelpDialogState::default());
    app.state.all_slash_commands = vec![
        SlashCommandInfo {
            name: "项目分析技能".to_string(),
            description: "逐行阅读当前项目并输出真实缺口".to_string(),
            category: "Skills".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Skill,
        },
        SlashCommandInfo {
            name: "status".to_string(),
            description: "Show current runtime status".to_string(),
            category: "System".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        },
    ];

    let snapshot = render_app_frame(&mut app, 88, 24);

    assert_snapshot_has(
        "app help modal multibyte",
        &snapshot,
        &[
            "Mossen Help",
            "/项目分析技能",
            "逐行阅读当前项目",
            "/status",
        ],
    );
    assert_no_protocol_noise("app help modal multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_mcp_modal_handles_multibyte_columns() {
    let mut app = App::new();
    app.fullscreen = true;
    app.active_modal = ActiveModal::McpServersDialog;
    app.state.mcp_servers = vec![McpServerStatus {
        name: "本地文件系统服务".to_string(),
        state: McpConnectionState::Connected,
        transport: "stdio".to_string(),
        tools_count: 12,
        prompts_count: 3,
        resources_count: 4,
        scope: "project".to_string(),
        last_error: None,
    }];

    let snapshot = render_app_frame(&mut app, 88, 22);

    assert_snapshot_has(
        "app mcp modal multibyte",
        &snapshot,
        &["MCP Servers", "connected", "本地文件系统服务", "12 tools"],
    );
    assert_no_protocol_noise("app mcp modal multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_message_selector_handles_multibyte_rows() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages.push(msg(
        MessageType::User,
        "先确认 selector 是否走真实 App modal 渲染路径。",
    ));
    app.messages.push(msg(
        MessageType::Assistant,
        "逐行阅读真实代码后再给结论，当前这一行故意很长，用来确认消息选择器不会因为中文宽字符把边框挤坏或把后续内容写到错误单元格。",
    ));

    let mut services = std::mem::take(&mut app.services);
    open_message_selector(&mut app, &mut services, false);
    app.services = services;

    let snapshot = render_app_frame(&mut app, 72, 20);

    assert_snapshot_has(
        "app message selector multibyte",
        &snapshot,
        &["Select Message", "逐行阅读真实代码", "…"],
    );
    assert_no_protocol_noise("app message selector multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_search_modal_handles_multibyte_rows() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages.push(msg(
        MessageType::Assistant,
        "逐行阅读代码时，搜索结果预览这一行也要按终端列宽裁剪，不能把中文宽字符当成单字节普通字符。",
    ));
    let mut panel = SearchPanelState::new();
    panel.input.set_query("逐行阅读".to_string());
    panel.matches = vec![0];
    app.services.search_panel_state = Some(panel);
    app.active_modal = ActiveModal::Search("逐行阅读代码并验证搜索浮层的中文列宽裁剪".to_string());

    let snapshot = render_app_frame(&mut app, 70, 20);

    assert_snapshot_has(
        "app search modal multibyte",
        &snapshot,
        &["Search", "> 逐行阅读代码", "搜索结果预览"],
    );
    assert_no_protocol_noise("app search modal multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_tasks_dialog_handles_multibyte_rows() {
    let mut app = App::new();
    app.fullscreen = true;
    app.active_modal = ActiveModal::TasksDialog;
    app.state.task_list.tasks = vec![TodoItem {
        id: "todo-1".to_string(),
        content: "逐行阅读 mossen-tui 活跃渲染链路并记录真实缺口，内容故意拉长以验证状态列不会被中文宽字符挤出边界".to_string(),
        status: "in_progress".to_string(),
    }];
    app.state.teammate_states.insert(
        "逐行阅读子任务名称也很长需要按列宽裁剪".to_string(),
        TeammateState::Running,
    );

    let snapshot = render_app_frame(&mut app, 84, 22);

    assert_snapshot_has(
        "app tasks modal multibyte",
        &snapshot,
        &[
            "TodoWrite tasks",
            "mossen-tui",
            "in_progress",
            "Background agents",
            "running",
        ],
    );
    assert_no_protocol_noise("app tasks modal multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_picker_handles_multibyte_items() {
    let mut app = App::new();
    app.fullscreen = true;
    app.active_modal = ActiveModal::Picker {
        kind: PickerKind::BackgroundTasks,
        title: "Background Tasks".to_string(),
        items: vec![
            "逐行阅读代码的后台子任务名称非常非常长，需要被裁剪而不是越过弹窗边框".to_string(),
        ],
        selected: 0,
    };

    let snapshot = render_app_frame(&mut app, 70, 18);

    assert_snapshot_has(
        "app picker multibyte",
        &snapshot,
        &["Background Tasks", "逐行阅读代码"],
    );
    assert_no_protocol_noise("app picker multibyte", &snapshot);
}

#[test]
fn render_snapshot_app_frame_model_skills_memory_panels_handle_multibyte_rows() {
    let mut model_app = App::new();
    model_app.fullscreen = true;
    model_app.active_modal = ActiveModal::ModelPicker(ModelPickerState::new(vec![ModelInfo {
        id: "example-fast-m2.7".to_string(),
        name: "example-fast 项目逐行阅读长上下文模型名称".to_string(),
        provider: "自定义后端".to_string(),
        supports_thinking: true,
        supports_streaming: true,
        is_current: true,
    }]));
    let model_snapshot = render_app_frame(&mut model_app, 78, 22);
    assert_snapshot_has(
        "app model picker multibyte",
        &model_snapshot,
        &["Select Model", "example-fast", "自定义后端"],
    );
    let first_line = model_snapshot.lines().next().unwrap_or_default();
    assert!(
        !first_line.contains("Select Model"),
        "model picker should render as a centered modal, not at the top\n--- snapshot ---\n{model_snapshot}"
    );
    assert_no_protocol_noise("app model picker multibyte", &model_snapshot);

    let mut skills_app = App::new();
    skills_app.fullscreen = true;
    skills_app.active_modal = ActiveModal::SkillsPanel(SkillsPanelState::new(vec![SkillInfo {
        name: "项目逐行阅读技能长名称".to_string(),
        description: "读取真实代码路径并输出可验证结论".to_string(),
        enabled: true,
    }]));
    let skills_snapshot = render_app_frame(&mut skills_app, 78, 22);
    assert_snapshot_has(
        "app skills panel multibyte",
        &skills_snapshot,
        &["Crafts", "项目逐行阅读技能", "读取真实代码"],
    );
    assert_no_protocol_noise("app skills panel multibyte", &skills_snapshot);

    let mut memory_app = App::new();
    memory_app.fullscreen = true;
    memory_app.active_modal = ActiveModal::MemoryPanel(MemoryPanelState::new(vec![MemoryEntry {
        title: "渲染红线记录：必须穿过 App 活跃路径".to_string(),
        category: "项目记忆".to_string(),
        preview: "避免安慰剂陷阱".to_string(),
    }]));
    let memory_snapshot = render_app_frame(&mut memory_app, 78, 22);
    assert_snapshot_has(
        "app memory panel multibyte",
        &memory_snapshot,
        &["Recall", "渲染红线记录", "项目记忆"],
    );
    assert_no_protocol_noise("app memory panel multibyte", &memory_snapshot);
}

#[test]
fn render_snapshot_approval_decision_stays_in_transcript_without_raw_marker() {
    let decision = ApprovalDecisionModel {
        id: "approval-decision-1".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::Allowed,
        detail: "ls -la".to_string(),
        anchor_block_id: Some("tool-0".to_string()),
    };
    let messages = vec![
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            serde_json::json!({ "command": "ls -la" }).to_string(),
        ),
        msg(
            MessageType::System,
            approval_decision_message_content(&decision),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "Cargo.toml\ncrates\n",
                "stderr": "",
                "exit_code": 0
            })
            .to_string(),
        ),
    ];

    let snapshot = render_messages(&messages, 88, 18);

    assert_snapshot_has(
        "approval decision transcript",
        &snapshot,
        &[
            "▼ Bash",
            "command",
            "ls -la",
            "Allowed Bash",
            "stdout",
            "Cargo.toml",
        ],
    );
    assert!(!snapshot.contains("mossen-render:approval-decision"));
    assert_no_protocol_noise("approval decision transcript", &snapshot);
}

#[test]
fn render_snapshot_mcp_channel_approval_decision_stays_near_tool_context() {
    let mut app = App::new();
    app.fullscreen = true;
    app.handle_engine_message(SdkMessage::Assistant {
        message: AssistantMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse(ToolUseBlock {
                id: "toolu-mcp-filesystem-snapshot".to_string(),
                name: "mcp__filesystem__read_file".to_string(),
                input: serde_json::json!({ "path": "Cargo.toml" }),
            })],
            uuid: Some("assistant-mcp-snapshot".to_string()),
            model: None,
            stop_reason: Some("tool_use".to_string()),
            extra: std::collections::HashMap::new(),
        },
        usage: None,
        task_id: None,
    });
    app.active_modal = ActiveModal::McpChannelApproval(
        mossen_agent::mcp::channel_approval::ChannelApprovalRequest {
            id: "mcp-approval-snapshot".to_string(),
            server_name: "filesystem".to_string(),
            plugin: Some("local-plugin".to_string()),
            marketplace: Some("dev".to_string()),
            reason: "server wants a local channel".to_string(),
        },
    );
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let snapshot = render_app_frame(&mut app, 92, 20);

    assert_snapshot_has(
        "mcp approval decision anchored",
        &snapshot,
        &[
            "[filesystem] read_file",
            "input:",
            "Cargo.toml",
            "Allowed MCP Channel",
            "filesystem",
        ],
    );
    assert_no_protocol_noise("mcp approval decision anchored", &snapshot);
}

#[test]
fn render_snapshot_approval_decision_survives_narrow_and_wide_profiles() {
    let decision = ApprovalDecisionModel {
        id: "approval-decision-1".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::AlwaysAllowed,
        detail: "cargo test -p mossen-tui render_snapshot".to_string(),
        anchor_block_id: Some("tool-0".to_string()),
    };
    let messages = vec![
        msg(MessageType::User, "继续优化渲染"),
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            serde_json::json!({
                "command": "cargo test -p mossen-tui render_snapshot",
                "cwd": "/Users/allen/Documents/rustmossen"
            })
            .to_string(),
        ),
        msg(
            MessageType::System,
            approval_decision_message_content(&decision),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "running 9 tests\nall green\n",
                "stderr": "",
                "exit_code": 0
            })
            .to_string(),
        ),
    ];

    for width in [60, 120] {
        let snapshot = render_messages(&messages, width, 24);
        assert_snapshot_has(
            &format!("approval decision width {width}"),
            &snapshot,
            &[
                "继续优化渲染",
                "Bash",
                "Always allowed Bash",
                "stdout",
                "all green",
                "exit 0",
            ],
        );
        assert!(!snapshot.contains("mossen-render:approval-decision"));
        assert_no_protocol_noise(&format!("approval decision width {width}"), &snapshot);
    }
}

#[test]
fn render_snapshot_renderer_profile_matrix_keeps_core_semantics() {
    let decision = ApprovalDecisionModel {
        id: "approval-decision-1".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::Allowed,
        detail: "ls -la".to_string(),
        anchor_block_id: Some("tool-2".to_string()),
    };
    let messages = vec![
        msg(MessageType::User, "分析当前项目"),
        msg(
            MessageType::Assistant,
            "## Findings\n\n- 入口在 `crates/mossen-cli`\n- TUI 走三层渲染\n\n```rust\nfn main() {}\n```",
        ),
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            serde_json::json!({ "command": "ls -la" }).to_string(),
        ),
        msg(
            MessageType::System,
            approval_decision_message_content(&decision),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "Cargo.toml\ncrates\nphases\n",
                "stderr": "",
                "exit_code": 0
            })
            .to_string(),
        ),
    ];

    for width in [60, 80, 120, 160] {
        let snapshot = render_messages(&messages, width, 32);
        assert_snapshot_has(
            &format!("renderer profile width {width}"),
            &snapshot,
            &[
                "分析当前项目",
                "# Findings",
                "fn main()",
                "Bash",
                "Allowed Bash",
                "stdout",
                "Cargo.toml",
                "exit 0",
            ],
        );
        assert_no_protocol_noise(&format!("renderer profile width {width}"), &snapshot);
    }
}

#[test]
fn render_snapshot_long_bash_result_is_bounded_in_narrow_viewport() {
    let preview = (1..=8)
        .map(|idx| format!("line {idx:03} output from a long command"))
        .collect::<Vec<_>>()
        .join("\n");
    let full = (1..=120)
        .map(|idx| format!("line {idx:03} output from a long command"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut result = tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": preview,
            "stdout_hidden_lines": 112,
            "stderr": "",
            "exit_code": 0
        })
        .to_string(),
    );
    result.full_content = Some(
        serde_json::json!({
            "stdout": full,
            "stderr": "",
            "exit_code": 0
        })
        .to_string(),
    );

    let snapshot = render_messages(&[result], 80, 12);

    assert_snapshot_has(
        "long bash bounded",
        &snapshot,
        &[
            "▼ Bash",
            "Output:",
            "full log",
            "available",
            "stdout",
            "line 001 output from a long command",
            "112 lines hidden",
            "exit 0",
        ],
    );
    assert_no_protocol_noise("long bash bounded", &snapshot);
    assert!(
        !snapshot.contains("line 120 output"),
        "long Bash snapshot should stay bounded\n--- snapshot ---\n{snapshot}"
    );
}

#[test]
fn render_snapshot_bash_output_strips_ansi_and_clips_wide_lines() {
    let result = tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": format!(
                "\u{1b}[31m红色失败\u{1b}[0m\t下一步继续读取代码\n{}",
                "逐行阅读".repeat(20)
            ),
            "stderr": "\u{1b}[33m警告：继续检查渲染边界\u{1b}[0m",
            "exit_code": 1
        })
        .to_string(),
    );

    let snapshot = render_messages(&[result], 56, 16);

    assert_snapshot_has(
        "bash ansi stripped",
        &snapshot,
        &["▼ Bash", "stdout", "红色失败", "stderr", "警告", "exit 1"],
    );
    assert_no_protocol_noise("bash ansi stripped", &snapshot);
    assert!(
        !snapshot.contains('\u{1b}')
            && !snapshot.contains("[31m")
            && !snapshot.contains("[33m")
            && !snapshot.contains("[0m"),
        "ANSI escape sequences should not leak into visible transcript\n--- snapshot ---\n{snapshot}"
    );
}

#[test]
fn render_snapshot_bash_timeout_and_interruption_are_visible() {
    let mut result = tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "command": "cargo test -- --nocapture",
            "stderr": "test timed out\n",
            "exit_code": 124,
            "timed_out": true,
            "interrupted": true,
            "signal": "SIGTERM",
            "duration_ms": 30000,
            "error": "command exceeded timeout"
        })
        .to_string(),
    );
    result.is_error = true;

    let snapshot = render_messages(&[result], 74, 16);

    assert_snapshot_has(
        "bash timeout interrupted",
        &snapshot,
        &[
            "▼ Bash",
            "Failed",
            "exit 124",
            "timeout",
            "interrupted",
            "SIGTERM",
            "duration 30000ms",
            "command exceeded timeout",
            "stderr",
            "test timed out",
        ],
    );
    assert_no_protocol_noise("bash timeout interrupted", &snapshot);
}

#[test]
fn render_snapshot_tool_inputs_are_semantic_not_raw_json() {
    let messages = vec![
        tool_msg(MessageType::ToolUse, "Glob", "null"),
        tool_msg(
            MessageType::ToolUse,
            "Grep",
            serde_json::json!({
                "pattern": "render_bash_result",
                "path": "crates/mossen-tui",
                "glob": "*.rs",
                "output_mode": "content"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolUse,
            "Read",
            serde_json::json!({
                "file_path": "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/widgets/message.rs",
                "offset": 80,
                "limit": 24
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolUse,
            "Write",
            serde_json::json!({
                "file_path": "/Users/allen/Documents/rustmossen/tmp/render-note.md",
                "content": "# Notes\n\n- polish tool cards\n"
            })
            .to_string(),
        ),
    ];

    let snapshot = render_messages(&messages, 112, 26);

    assert_snapshot_has(
        "semantic tool inputs",
        &snapshot,
        &[
            "▼ Glob",
            "(no input)",
            "▼ Grep",
            "pattern",
            "render_bash_result",
            "path",
            "crates/mossen-tui",
            "mode",
            "content",
            "▼ Read",
            "file",
            "message.rs",
            "range",
            "lines 81-104",
            "▼ Write",
            "render-note.md",
            "polish tool cards",
        ],
    );
    assert_no_protocol_noise("semantic tool inputs", &snapshot);
    assert!(
        !snapshot.contains("\"pattern\"") && !snapshot.contains("\"file_path\""),
        "tool inputs should be labels, not raw JSON\n--- snapshot ---\n{snapshot}"
    );
}

#[test]
fn render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript() {
    let mut app = App::new();
    app.messages
        .push(msg(MessageType::User, "inspect raw debug separation"));
    app.messages.push(tool_msg(
        MessageType::ToolUse,
        "Grep",
        r#"{"raw_json":"debug-only","pattern":"render_bash_result","path":"crates/mossen-tui"}"#,
    ));

    let normal = render_app_frame(&mut app, 110, 22);
    assert_snapshot_has(
        "normal semantic transcript before raw view",
        &normal,
        &["Grep", "render_bash_result", "crates/mossen-tui"],
    );
    assert_no_protocol_noise("normal semantic transcript before raw view", &normal);
    assert!(
        !normal.contains("\"pattern\""),
        "normal transcript leaked raw JSON keys before explicit /raw view\n--- snapshot ---\n{normal}"
    );

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));

    let raw = render_app_frame(&mut app, 110, 24);
    assert_snapshot_has(
        "explicit raw transcript debug view",
        &raw,
        &[
            "Raw Transcript",
            "explicit /raw debug view",
            "messages=2",
            "message 1 turn=- kind=ToolUse",
            "record tool-1 source=1 turn=- kind=ToolUse",
            "\"raw_json\":\"debug-only\"",
            "\"pattern\":\"render_bash_result\"",
        ],
    );
}

#[test]
fn render_snapshot_raw_debug_view_includes_layer1_engine_events() {
    let mut app = App::new();
    app.fullscreen = true;

    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "raw-event-session".to_string(),
        model: "example-fast".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-raw-snapshot".to_string()),
        summary: "tests passed".to_string(),
        full_content: Some("cargo test\nok".to_string()),
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::Result {
        terminal: "Completed".to_string(),
        cost_usd: Some(0.001),
        duration_ms: Some(420),
        usage: None,
        task_id: None,
    });

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));

    let raw = render_app_frame(&mut app, 140, 36);
    assert_snapshot_has(
        "explicit raw transcript engine event journal",
        &raw,
        &[
            "Raw Transcript",
            "raw_events=3",
            "snapshot version=1",
            "session=raw-event-session",
            "relations roots=0 parented=1 parents=1 orphans=1",
            "engine events",
            "event 1 turn=turn-0001 scope=main kind=system_init",
            "\"type\":\"system_init\"",
            "event 2 turn=turn-0001 scope=main kind=tool_use_summary",
            "summary=tool=Bash",
            "\"tool_use_id\":\"toolu-raw-snapshot\"",
            "record relations",
            "parent toolu-raw-snapshot children=toolu-raw-snapshot:result",
            "missing parents toolu-raw-snapshot",
        ],
    );
}

#[test]
fn render_snapshot_command_exports_render_session_snapshot_modal() {
    let mut app = App::new();
    let dir = tempfile::tempdir().expect("tempdir should be created");
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();
    app.prompt
        .input
        .insert_str("export render session snapshot");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "snapshot-command-session".to_string(),
        model: "test-model".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-snapshot-command-1".to_string()),
        summary: "ok".to_string(),
        full_content: Some("full command log".to_string()),
        task_id: None,
    });

    app.prompt
        .input
        .insert_str("/render-snapshot save snapshots/current.json");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let path = dir.path().join("snapshots").join("current.json");
    assert!(
        path.exists(),
        "snapshot file should exist: {}",
        path.display()
    );
    assert!(matches!(
        app.active_modal,
        ActiveModal::CommandOutput { .. }
    ));
    let snapshot = render_app_frame(&mut app, 140, 24);
    assert_snapshot_has(
        "render snapshot export modal",
        &snapshot,
        &[
            "Render Snapshot",
            "Saved render session snapshot",
            "session: snapshot-command-session",
            "current turn: turn-0001",
            "records: 2",
            "raw events: 2",
            "snapshots/current.json",
        ],
    );
    assert_no_protocol_noise("render snapshot export modal", &snapshot);
}

#[test]
fn render_snapshot_restore_hydrates_visible_transcript() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let path = dir.path().join("snapshots").join("restore-visible.json");
    let mut source = App::new();
    source.engine_config.cwd = dir.path().to_string_lossy().to_string();
    source
        .prompt
        .input
        .insert_str("restore visible transcript from snapshot");
    source.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    source.handle_engine_message(SdkMessage::SystemInit {
        session_id: "snapshot-restore-session".to_string(),
        model: "test-model".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    source.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-restore-visible-1".to_string()),
        summary: "ok".to_string(),
        full_content: Some("cargo test\nok".to_string()),
        task_id: None,
    });
    source.handle_engine_message(SdkMessage::Result {
        terminal: "Completed".to_string(),
        cost_usd: None,
        duration_ms: Some(20),
        usage: None,
        task_id: None,
    });
    source
        .prompt
        .input
        .insert_str(&format!("/render-snapshot save {}", path.display()));
    source.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let mut app = App::new();
    app.messages.push(msg(
        MessageType::Assistant,
        "old transcript should be replaced",
    ));
    app.prompt
        .input
        .insert_str(&format!("/render-snapshot restore {}", path.display()));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let modal = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "render snapshot restore modal",
        &modal,
        &[
            "Render Snapshot",
            "Restored render session snapshot",
            "engine execution not resumed",
            "session: snapshot-restore-session",
            "records: 2",
            "raw events: 3",
        ],
    );
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let restored = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "restored render snapshot transcript",
        &restored,
        &["restore visible transcript from snapshot", "Bash", "ok"],
    );
    assert!(
        !restored.contains("old transcript should be replaced"),
        "restore should replace the previous live transcript\n--- snapshot ---\n{restored}"
    );
    assert_no_protocol_noise("restored render snapshot transcript", &restored);
}

#[test]
fn render_snapshot_resume_restores_latest_autosaved_render_session() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let mut source = App::new();
    source.engine_config.cwd = dir.path().to_string_lossy().to_string();
    source
        .prompt
        .input
        .insert_str("resume latest autosaved render transcript");
    source.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    source.handle_engine_message(SdkMessage::SystemInit {
        session_id: "snapshot-resume-session".to_string(),
        model: "test-model".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    source.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-resume-visible-1".to_string()),
        summary: "ok".to_string(),
        full_content: Some("cargo test\nok".to_string()),
        task_id: None,
    });
    source
        .autosave_render_session_snapshot()
        .expect("source autosave should succeed")
        .expect("source snapshot should be written");

    let mut app = App::new();
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();
    app.messages
        .push(msg(MessageType::Assistant, "old session row"));
    app.prompt.input.insert_str("/resume");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let modal = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "render snapshot resume modal",
        &modal,
        &[
            "Render Snapshot",
            "Restored render session snapshot",
            "session: snapshot-resume-session",
            "records: 2",
            "raw events: 2",
        ],
    );
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let restored = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "resume restored render snapshot transcript",
        &restored,
        &["resume latest autosaved render transcript", "Bash", "ok"],
    );
    assert!(
        !restored.contains("old session row"),
        "resume should replace stale live transcript\n--- snapshot ---\n{restored}"
    );
    assert_no_protocol_noise("resume restored render snapshot transcript", &restored);
}

#[test]
fn render_snapshot_startup_restore_hydrates_visible_transcript() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let mut source = App::new();
    source.engine_config.cwd = dir.path().to_string_lossy().to_string();
    source
        .prompt
        .input
        .insert_str("startup restored autosaved render transcript");
    source.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    source.handle_engine_message(SdkMessage::SystemInit {
        session_id: "snapshot-startup-session".to_string(),
        model: "test-model".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    source.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-startup-visible-1".to_string()),
        summary: "ok".to_string(),
        full_content: Some("cargo check\nok".to_string()),
        task_id: None,
    });
    source
        .autosave_render_session_snapshot()
        .expect("source autosave should succeed")
        .expect("source snapshot should be written");

    let mut app = App::new();
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();
    app.restore_latest_render_session_snapshot_on_startup()
        .expect("startup restore should succeed")
        .expect("startup restore should find latest snapshot");

    let restored = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "startup restored render snapshot transcript",
        &restored,
        &["startup restored autosaved render transcript", "Bash", "ok"],
    );
    assert_no_protocol_noise("startup restored render snapshot transcript", &restored);

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let raw = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "startup restore raw status",
        &raw,
        &[
            "startup restore status=restored",
            "snapshot-startup-session",
            "raw_events=2",
        ],
    );
}

#[test]
fn render_snapshot_timeline_modal_is_structured_event_history() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages
        .push(msg(MessageType::User, "查看结构化渲染事件"));
    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "timeline-snapshot-session".to_string(),
        model: "example-fast".to_string(),
        tools: Vec::new(),
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-timeline-snapshot".to_string()),
        summary: serde_json::json!({
            "stdout": "timeline snapshot\nok\n",
            "exit_code": 0,
            "duration_ms": 180
        })
        .to_string(),
        full_content: None,
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::Result {
        terminal: "Completed".to_string(),
        cost_usd: None,
        duration_ms: Some(180),
        usage: None,
        task_id: None,
    });

    app.prompt.input.insert_str("/timeline");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RenderTimeline(_)));

    let snapshot = render_app_frame(&mut app, 104, 24);
    assert_snapshot_has(
        "render timeline modal",
        &snapshot,
        &[
            "Render Timeline",
            "events:",
            "turns:",
            "turn-0001",
            "command_output",
            "command_finish",
            "final_summary",
            "turn:",
            "scope: main",
            "history:",
            "exit 0",
            "Up/Down selects",
        ],
    );
    assert_no_protocol_noise("render timeline modal", &snapshot);
    for forbidden in ["\"stdout\"", "\"exit_code\"", "\"duration_ms\""] {
        assert!(
            !snapshot.contains(forbidden),
            "timeline modal leaked raw command payload key {forbidden:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

#[test]
fn render_snapshot_statusline_config_is_explicit_session_ui() {
    let mut app = App::new();
    app.engine_config.cwd = "/Users/allen/Documents/rustmossen".to_string();
    app.engine_config.model = "example-fast".to_string();
    app.messages
        .push(msg(MessageType::Assistant, "Status line config snapshot"));

    app.prompt.input.insert_str("/statusline");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::StatusLineConfig(_)));

    let snapshot = render_app_frame(&mut app, 100, 24);
    assert_snapshot_has(
        "statusline config modal",
        &snapshot,
        &[
            "Status Line",
            "Core status",
            "locked",
            "[x] Project",
            "[x] Model",
            "[x] Context",
        ],
    );
    assert_no_protocol_noise("statusline config modal", &snapshot);
}

#[test]
fn render_snapshot_statusline_config_persists_across_startup() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let mut source = App::new();
    source.engine_config.cwd = dir.path().to_string_lossy().to_string();
    source.engine_config.model = "example-fast".to_string();

    source.prompt.input.insert_str("/statusline minimal");
    source.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!source.state.footer_config.is_enabled(FooterItem::Project));
    assert!(source.state.footer_config.is_enabled(FooterItem::Context));

    let mut app = App::new();
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();
    app.engine_config.model = "example-fast".to_string();
    app.load_footer_render_config_on_startup()
        .expect("statusline startup load should succeed")
        .expect("statusline config should exist");

    assert!(!app.state.footer_config.is_enabled(FooterItem::Project));
    assert!(!app.state.footer_config.is_enabled(FooterItem::MessageCount));
    assert!(app.state.footer_config.is_enabled(FooterItem::Model));
    assert!(app.state.footer_config.is_enabled(FooterItem::Context));

    let footer = render_app_frame(&mut app, 96, 18);
    assert_snapshot_has("persisted statusline footer", &footer, &["example-fast"]);
    assert!(
        !footer.contains("Messages"),
        "minimal footer should not show the message-count label\n--- snapshot ---\n{footer}"
    );

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let raw = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "persisted statusline raw status",
        &raw,
        &["statusline config status=loaded path="],
    );
    assert_no_protocol_noise("persisted statusline raw status", &raw);
}

#[test]
fn render_snapshot_external_statusline_output_is_stable_footer_chrome() {
    let mut app = App::new();
    app.engine_config.cwd = "/Users/allen/Documents/rustmossen".to_string();
    app.engine_config.model = "example-fast".to_string();
    app.state
        .footer_config
        .set_enabled(FooterItem::ExternalStatus, true);
    app.state.footer_config.external_command =
        Some(mossen_tui::render_model::ExternalStatusLineCommandConfig::new("printf branch-main"));
    app.external_statusline_output = Some("branch main".to_string());
    app.external_statusline_error = Some("previous timeout".to_string());

    let frame = render_app_frame(&mut app, 112, 18);
    assert_snapshot_has("external statusline footer", &frame, &["branch main"]);
    assert_no_protocol_noise("external statusline footer", &frame);

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let raw = render_app_frame(&mut app, 112, 24);
    assert_snapshot_has(
        "external statusline raw status",
        &raw,
        &[
            "external statusline configured=true",
            "output=branch main",
            "error=previous timeout",
        ],
    );
    assert_no_protocol_noise("external statusline raw status", &raw);
}

#[test]
fn render_snapshot_session_title_modal_is_semantic_and_sanitized() {
    let mut app = App::new();
    app.fullscreen = true;
    app.prompt
        .input
        .insert_str("/title 渲染会话\u{1b}]2;raw\u{7}");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::TitleConfig(_)));

    let snapshot = render_app_frame(&mut app, 92, 18);
    assert_snapshot_has(
        "session title modal",
        &snapshot,
        &[
            "Session Title",
            "Current",
            "Custom",
            "Draft",
            "渲染会话",
            "]2;raw",
            "Enter saves",
            "Ctrl+U reset",
        ],
    );
    assert_no_protocol_noise("session title modal", &snapshot);
    assert!(
        !snapshot.contains('\u{1b}') && !snapshot.contains('\u{7}'),
        "session title modal must not render terminal control characters\n--- snapshot ---\n{snapshot}"
    );
}

#[test]
fn render_snapshot_file_changes_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages = vec![
        msg(MessageType::User, "查看文件变更摘要"),
        tool_msg(
            MessageType::ToolResult,
            "Write",
            serde_json::json!({
                "file_path": "src/lib.rs",
                "old_string": "fn main() {\n    println!(\"old\");\n}\n",
                "new_string": "fn main() {\n    println!(\"new\");\n    println!(\"extra\");\n}\n"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Write",
            serde_json::json!({
                "file_path": "src/new.rs",
                "content": "pub fn added() {}\n"
            })
            .to_string(),
        ),
    ];
    app.prompt.input.insert_str("/changes");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::FileChanges(_)));

    let snapshot = render_app_frame(&mut app, 96, 20);
    assert_snapshot_has(
        "file changes modal",
        &snapshot,
        &[
            "File Changes",
            "files: 2",
            "modified: 1",
            "added: 1",
            "src/lib.rs",
            "src/new.rs",
            "Modified",
            "Up/Down selects",
        ],
    );
    assert_no_protocol_noise("file changes modal", &snapshot);
    for forbidden in ["file_path", "old_string", "new_string", "\"content\""] {
        assert!(
            !snapshot.contains(forbidden),
            "file changes modal leaked raw file-change payload key {forbidden:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

#[test]
fn render_snapshot_status_overview_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.cwd = "/Users/allen/Documents/rustmossen".to_string();
    app.engine_config.model = "example-fast".to_string();
    app.engine_config.api_base_url = Some("http://localhost:8000/v1".to_string());
    app.engine_config.api_key = Some("redacted-test-key".to_string());
    app.engine_config.output_style = Some("Concise".to_string());
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.engine_session_id = Some("session-render-status".to_string());
    app.total_cost_usd = 0.375;
    app.messages
        .push(msg(MessageType::User, "请展示 /status 里的语义会话状态"));
    app.state.turn_state = TurnState::Streaming;
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandStarted {
            command: Some("cargo test -p mossen-tui status_overview".to_string()),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
        });
    app.state.task_list.tasks = vec![TodoItem {
        id: "render-status".to_string(),
        content: "实现 /status 语义会话概览".to_string(),
        status: "in_progress".to_string(),
    }];
    app.state.mcp_servers = vec![McpServerStatus {
        name: "filesystem".to_string(),
        state: McpConnectionState::Connected,
        transport: "stdio".to_string(),
        tools_count: 5,
        prompts_count: 0,
        resources_count: 2,
        scope: "project".to_string(),
        last_error: None,
    }];

    app.prompt.input.insert_str("/status");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::StatusDialog));

    let snapshot = render_app_frame(&mut app, 112, 34);
    assert_snapshot_has(
        "status overview modal",
        &snapshot,
        &[
            "Status",
            "Session",
            "Turn",
            "Policy",
            "Workspace",
            "example-fast",
            "running command",
            "reasoning:high",
            "ctx 0/200k",
            "API Key",
            "configured",
            "MCP",
            "Esc closes",
        ],
    );
    assert_no_protocol_noise("status overview modal", &snapshot);
}

#[test]
fn render_snapshot_debug_config_modal_is_redacted_and_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.cwd = "/Users/allen/Documents/rustmossen".to_string();
    app.engine_config.model = "example-fast".to_string();
    app.engine_config.api_base_url = Some("http://localhost:8000/v1".to_string());
    app.engine_config.api_key = Some("redacted-debug-config-key".to_string());
    app.engine_config.max_turns = Some(6);
    app.engine_config.output_style = Some("Concise".to_string());
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.engine_config.extra_body.insert(
        "secret_token".to_string(),
        serde_json::json!("must-not-render"),
    );
    app.engine_session_id = Some("session-render-debug-config-123456789".to_string());
    app.messages
        .push(msg(MessageType::User, "请展示 /debug-config 的脱敏配置"));

    app.prompt.input.insert_str("/debug-config");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::DebugConfig(_)));

    let snapshot = render_app_frame(&mut app, 118, 34);
    assert_snapshot_has(
        "debug config modal",
        &snapshot,
        &[
            "Debug Config",
            "secrets redacted",
            "Session",
            "Engine",
            "Policy",
            "Renderer",
            "example-fast",
            "API Key",
            "configured",
            "Extra Body",
            "secret_token",
            "redacted",
            "Esc closes",
        ],
    );
    assert_no_protocol_noise("debug config modal", &snapshot);
    for forbidden in [
        "redacted-debug-config-key",
        "must-not-render",
        "\"secret_token\"",
        "\"api_key\"",
    ] {
        assert!(
            !snapshot.contains(forbidden),
            "debug config leaked raw or secret config value {forbidden:?}\n--- snapshot ---\n{snapshot}"
        );
    }

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    let scrolled = render_app_frame(&mut app, 92, 18);
    assert_snapshot_has("debug config modal scrolled", &scrolled, &["Height Cache"]);
    assert_no_protocol_noise("debug config modal scrolled", &scrolled);

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let bottom = render_app_frame(&mut app, 92, 18);
    assert_snapshot_has(
        "debug config modal bottom",
        &bottom,
        &["Runtime", "Transcript", "Slash Catalog"],
    );
    assert_no_protocol_noise("debug config modal bottom", &bottom);
}

#[test]
fn render_snapshot_command_history_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandOutput {
            stream: "stdout".to_string(),
            bytes: 2048,
            preview_lines: 4,
            hidden_lines: 20,
            total_lines: Some(24),
            full_log_available: true,
        });
    app.messages
        .push(msg(MessageType::User, "请展示命令执行历史"));
    app.messages.push(tool_msg(
        MessageType::ToolUse,
        "Bash",
        serde_json::json!({
            "command": "cargo test -p mossen-tui command_history",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string(),
    ));
    let mut result = tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": "running command history tests\nok\n",
            "stderr": "warning: existing warning noise\n",
            "exit_code": 0,
            "duration_ms": 88
        })
        .to_string(),
    );
    result.full_content = Some(
        serde_json::json!({
            "stdout": "running command history tests\nok\nfull log tail\n",
            "stderr": "warning: existing warning noise\n",
            "exit_code": 0,
            "duration_ms": 88
        })
        .to_string(),
    );
    app.messages.push(result);

    app.prompt.input.insert_str("/commands");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::CommandHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let snapshot = render_app_frame(&mut app, 116, 30);
    assert_snapshot_has(
        "command history modal",
        &snapshot,
        &[
            "Command History",
            "commands:",
            "running:",
            "full logs:",
            "Active command output",
            "cargo test -p mossen-tui command_history",
            "cwd: /Users/allen/Documents/rustmossen",
            "status: Succeeded",
            "full log: expanded",
            "stdout: running command history tests",
            "full log tail",
            "stderr: warning: existing warning noise",
            "Esc closes",
            "Space collapses log",
        ],
    );
    assert_no_protocol_noise("command history modal", &snapshot);
}

#[test]
fn render_snapshot_error_history_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    app.state.ui_stage = UiStage::Failed;
    app.state.render_activity.set(RenderActivity::Error {
        source: "engine".to_string(),
        summary: "model stream interrupted".to_string(),
    });
    let mut error = msg(
        MessageType::System,
        "Build failed\nTypeScript could not resolve import './auth'",
    );
    error.is_error = true;
    app.messages.push(error);
    app.messages.push(tool_msg(
        MessageType::ToolUse,
        "Bash",
        serde_json::json!({
            "command": "pnpm test",
            "cwd": "/repo/web"
        })
        .to_string(),
    ));
    app.messages.push(tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": "running tests\n",
            "stderr": "auth.spec.ts assertion failed\n",
            "stderr_hidden_lines": 3,
            "exit_code": 1,
            "duration_ms": 1200,
            "error": "2 tests failed in auth.spec.ts"
        })
        .to_string(),
    ));

    app.prompt.input.insert_str("/errors");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ErrorHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let snapshot = render_app_frame(&mut app, 116, 30);
    assert_snapshot_has(
        "error history modal",
        &snapshot,
        &[
            "Error History",
            "errors:",
            "command failures:",
            "model stream interrupted",
            "Build failed",
            "details: expanded",
            "TypeScript could not resolve import",
            "Esc closes",
            "Space collapses details",
        ],
    );
    assert_no_protocol_noise("error history modal", &snapshot);
}

#[test]
fn render_snapshot_final_summary_history_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    let summary = FinalSummaryModel {
        id: "summary-snapshot-modal".to_string(),
        success: false,
        terminal: "Completed with follow-up risk".to_string(),
        changed_files: vec![mossen_tui::render_model::FileChangeSummaryModel {
            path: "crates/mossen-tui/src/widgets/summary_history.rs".to_string(),
            status: "A".to_string(),
            additions: 220,
            deletions: 0,
        }],
        commands: vec![CommandSummaryModel {
            command: "cargo test -p mossen-tui --test render_snapshot".to_string(),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
            exit_code: Some(0),
            duration_ms: Some(810),
            status: "passed".to_string(),
        }],
        verification_results: vec![VerificationSummaryModel {
            command: "cargo check -p mossen-tui".to_string(),
            status: "passed".to_string(),
            passed: true,
            exit_code: Some(0),
            duration_ms: Some(640),
        }],
        residual_risks: vec!["Large session soak still needs a live terminal pass".to_string()],
        notes: vec!["Task execution code was untouched".to_string()],
    };
    app.messages.push(msg(
        MessageType::System,
        final_summary_message_content(&summary),
    ));

    app.prompt.input.insert_str("/results");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.active_modal,
        ActiveModal::FinalSummaryHistory(_)
    ));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let snapshot = render_app_frame(&mut app, 118, 30);
    assert_snapshot_has(
        "final summary history modal",
        &snapshot,
        &[
            "Final Summaries",
            "summaries:",
            "attention:",
            "[Needs attention]",
            "Completed with follow-up risk",
            "details: expanded",
            "summary_history.rs",
            "cargo test -p mossen-tui --test render_snapshot",
            "check: cargo check -p mossen-tui",
            "risk: Large session soak still needs a live terminal pass",
            "Esc closes",
            "Space collapses details",
        ],
    );
    assert_no_protocol_noise("final summary history modal", &snapshot);
}

#[test]
fn render_snapshot_approval_history_modal_is_semantic() {
    let mut app = App::new();
    app.fullscreen = true;
    let allowed = ApprovalDecisionModel {
        id: "approval-snapshot-allowed".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::AlwaysAllowed,
        detail: "cargo test -p mossen-tui --test render_snapshot".to_string(),
        anchor_block_id: Some("tool-snapshot-1".to_string()),
    };
    let denied = ApprovalDecisionModel {
        id: "approval-snapshot-denied".to_string(),
        tool_name: "Write".to_string(),
        decision: ApprovalDecisionKind::Denied,
        detail: "crates/mossen-tui/src/task.rs".to_string(),
        anchor_block_id: None,
    };
    app.messages.push(msg(
        MessageType::System,
        approval_decision_message_content(&allowed),
    ));
    app.messages.push(msg(
        MessageType::System,
        approval_decision_message_content(&denied),
    ));

    app.prompt.input.insert_str("/approvals");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ApprovalHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let snapshot = render_app_frame(&mut app, 116, 30);
    assert_snapshot_has(
        "approval history modal",
        &snapshot,
        &[
            "Approval History",
            "approvals:",
            "allowed:",
            "denied:",
            "[Always allowed]",
            "details: expanded",
            "cargo test -p mossen-tui --test render_snapshot",
            "anchor: tool-snapshot-1",
            "source block: approval-snapshot-allowed",
        ],
    );
    assert!(!snapshot.contains("mossen-render:approval-decision"));
    assert_no_protocol_noise("approval history modal", &snapshot);
}

#[test]
fn render_snapshot_process_status_modal_summarizes_active_state() {
    let mut app = App::new();
    app.fullscreen = true;
    app.engine_config.model = "example-fast".to_string();
    app.state.turn_state = TurnState::Streaming;
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandStarted {
            command: Some("cargo test -p mossen-tui".to_string()),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
        });
    app.state.task_list.tasks = vec![TodoItem {
        id: "render-ps".to_string(),
        content: "实现 /ps 终端渲染检查面板".to_string(),
        status: "in_progress".to_string(),
    }];
    app.state
        .teammate_states
        .insert("agent-render-review".to_string(), TeammateState::Running);

    app.prompt.input.insert_str("/ps");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ProcessList(_)));

    let snapshot = render_app_frame(&mut app, 112, 28);
    assert_snapshot_has(
        "process status modal",
        &snapshot,
        &[
            "Process Status",
            "stage: running command",
            "turn: running command",
            "[running]",
            "Current turn",
            "cmd: cargo test -p mossen-tui",
            "Command activity",
            "/ps",
            "终端渲染检查面板",
            "agent-render-review",
            "Esc closes",
        ],
    );
    assert_no_protocol_noise("process status modal", &snapshot);
}

#[test]
fn render_snapshot_diff_review_modal_groups_files_and_folds() {
    let mut app = App::new();
    app.fullscreen = true;
    app.messages = vec![tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": concat!(
                "diff --git a/src/old.rs b/src/new.rs\n",
                "--- a/src/old.rs\n",
                "+++ b/src/new.rs\n",
                "@@ -1,3 +1,4 @@\n",
                " fn main() {\n",
                "-    println!(\"old\");\n",
                "+    println!(\"new\");\n",
                "+    println!(\"extra\");\n",
                " }\n",
                "diff --git a/src/added.rs b/src/added.rs\n",
                "new file mode 100644\n",
                "--- /dev/null\n",
                "+++ b/src/added.rs\n",
                "@@ -0,0 +1,1 @@\n",
                "+added\n"
            ),
            "exit_code": 0
        })
        .to_string(),
    )];
    app.prompt.input.insert_str("/diff");
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::DiffReview(_)));

    let snapshot = render_app_frame(&mut app, 110, 26);
    assert_snapshot_has(
        "diff review modal",
        &snapshot,
        &[
            "Diff Review",
            "Files",
            "new.rs",
            "+2/-1",
            "@@ -1,3 +1,4 @@",
            "println!(\"old\")",
            "println!(\"new\")",
            "Left/Right files",
        ],
    );
    assert_no_protocol_noise("diff review modal", &snapshot);

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let folded = render_app_frame(&mut app, 90, 18);
    assert_snapshot_has(
        "diff review folded modal",
        &folded,
        &["Diff Review", "File collapsed", "Press Space to expand"],
    );
    assert!(
        !folded.contains("println!(\"new\")"),
        "folded diff view should hide selected file details\n--- frame ---\n{folded}"
    );
}

#[test]
fn render_snapshot_search_read_and_diff_polish() {
    let glob_output = (1..=86)
        .map(|idx| format!("crates/mossen-tui/src/file_{idx:02}.rs"))
        .chain(std::iter::once(
            "(Results are truncated. Consider using a more specific path or pattern.)".to_string(),
        ))
        .collect::<Vec<_>>()
        .join("\n");
    let grep_json = serde_json::json!({
        "pattern": "ToolResult",
        "durationMs": 17,
        "total": 9,
        "limit": 2,
        "truncated": true,
        "message": "Results are truncated. Use a narrower pattern.",
        "matches": [
            {"path": "crates/mossen-tui/src/widgets/message.rs", "line": 279, "text": "ToolResult bodies are often raw JSON strings"},
            {"path": "crates/mossen-tui/src/widgets/render_block.rs", "line": 154, "text": "if matches!(data.message_type, ToolResult)"}
        ]
    })
    .to_string();
    let read_image = serde_json::json!({
        "type": "image",
        "file_path": "/Users/allen/Documents/rustmossen/assets/preview.png",
        "media_type": "image/png",
        "size_bytes": 4096
    })
    .to_string();
    let read_text = serde_json::json!({
        "type": "text",
        "file_path": "/Users/allen/Documents/rustmossen/src/render.rs",
        "offset": 9,
        "limit": 2,
        "total_lines": 40,
        "content": "fn render() {}\nfn verify() {}"
    })
    .to_string();
    let read_error = serde_json::json!({
        "type": "error",
        "message": "File not found: missing.rs"
    })
    .to_string();
    let bash_diff = serde_json::json!({
        "stdout": concat!(
            "diff --git a/src/demo.rs b/src/demo.rs\n",
            "@@ -1,3 +1,4 @@\n",
            " fn main() {\n",
            "-    println!(\"old\");\n",
            "+    println!(\"new\");\n",
            "+    println!(\"extra\");\n",
            " }\n"
        ),
        "exit_code": 0
    })
    .to_string();
    let edit_result = serde_json::json!({
        "file_path": "/Users/allen/Documents/rustmossen/src/demo.rs",
        "old_string": "fn main() {\n    println!(\"old\");\n}\n",
        "new_string": "fn main() {\n    println!(\"new\");\n    println!(\"extra\");\n}\n"
    })
    .to_string();

    let messages = vec![
        tool_msg(MessageType::ToolResult, "Bash", bash_diff),
        tool_msg(MessageType::ToolResult, "Glob", glob_output),
        tool_msg(MessageType::ToolResult, "Grep", grep_json),
        tool_msg(MessageType::ToolResult, "Read", read_text),
        tool_msg(MessageType::ToolResult, "Read", read_image),
        tool_msg(MessageType::ToolResult, "Read", read_error),
        tool_msg(MessageType::ToolResult, "Edit", edit_result),
    ];

    let snapshot = render_messages(&messages, 128, 90);

    assert_snapshot_has(
        "search read diff polish",
        &snapshot,
        &[
            "▼ Glob",
            "86 files",
            "upstream result truncated",
            "▼ Grep",
            "2 matches",
            "pattern",
            "ToolResult",
            "duration",
            "17ms",
            "message.rs:279",
            "shown: 2 matches of 9 matches",
            "narrower pattern",
            "▼ Read",
            "range",
            "lines 10-11",
            "10│fn render() {}",
            "29 lines not shown",
            "image",
            "image/png",
            "4096 bytes",
            "error",
            "missing.rs",
            "▼ Bash",
            "diff --git",
            "@@ -1,3 +1,4 @@",
            "-    println!(\"old\");",
            "+    println!(\"new\");",
            "▼ Edit",
            "old",
            "new",
            "extra",
        ],
    );
    assert_no_protocol_noise("search read diff polish", &snapshot);
}

#[test]
fn render_snapshot_extended_tools_use_semantic_cards_not_raw_json() {
    let messages = vec![
        tool_msg(
            MessageType::ToolUse,
            "WebFetch",
            serde_json::json!({
                "url": "https://example.com/docs",
                "prompt": "extract setup steps"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "WebFetch",
            serde_json::json!({
                "url": "https://example.com/docs",
                "code": 200,
                "codeText": "OK",
                "bytes": 2048,
                "durationMs": 31,
                "result": "Fetched summary with setup steps."
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "WebSearch",
            serde_json::json!({
                "query": "mossen tui rendering",
                "durationSeconds": 0.2,
                "results": [
                    {"title": "Rendering guide", "url": "https://example.com/render", "snippet": "terminal rendering"}
                ]
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Skill",
            serde_json::json!({
                "success": true,
                "commandName": "review",
                "allowedTools": ["Read", "Grep"],
                "result": "<command-name>/review</command-name>\n<command-args>src</command-args>\n\nSkill completed."
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "ReadMcpResource",
            serde_json::json!({
                "contents": [
                    {"uri": "file:///workspace/README.md", "mimeType": "text/markdown", "text": "# README"}
                ]
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "ListMcpResources",
            serde_json::json!([
                {"server": "filesystem", "name": "README", "uri": "file:///workspace/README.md", "mimeType": "text/markdown"}
            ])
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolUse,
            "mcp__filesystem__read_file",
            serde_json::json!({
                "path": "/Users/allen/Documents/rustmossen/README.md"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "NotebookEdit",
            serde_json::json!({
                "notebook_path": "/Users/allen/Documents/rustmossen/demo.ipynb",
                "cell_id": "cell-1",
                "cell_type": "code",
                "edit_mode": "replace",
                "new_source": "print('new')",
                "original_file": "print('old')",
                "updated_file": "print('new')"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "ExitPlanMode",
            serde_json::json!({
                "message": "Exited plan mode. The plan has been presented for approval."
            })
            .to_string(),
        ),
    ];

    let snapshot = render_messages(&messages, 132, 96);

    assert_snapshot_has(
        "extended semantic tools",
        &snapshot,
        &[
            "▼ WebFetch",
            "https://example.com/docs",
            "Fetched summary",
            "▼ WebSearch",
            "Rendering guide",
            "terminal rendering",
            "▼ Skill",
            "review",
            "Skill completed",
            "▼ ReadMcpResource",
            "file:///workspace/README.md",
            "text/markdown",
            "▼ ListMcpResources",
            "1 resources",
            "[filesystem] README",
            "▼ [filesystem] read_file",
            "/Users/allen/Documents/rustmossen/README.md",
            "▼ NotebookEdit",
            "demo.ipynb",
            "print('old')",
            "print('new')",
            "▼ ExitPlanMode",
            "Exited plan mode",
        ],
    );
    assert_no_protocol_noise("extended semantic tools", &snapshot);
    for raw_marker in [
        "\"url\"",
        "\"query\"",
        "\"commandName\"",
        "<command-name>",
        "\"contents\"",
        "\"notebook_path\"",
        "\"message\"",
    ] {
        assert!(
            !snapshot.contains(raw_marker),
            "extended tool snapshot leaked raw JSON marker {raw_marker:?}\n--- snapshot ---\n{snapshot}"
        );
    }
}

#[test]
fn render_snapshot_malformed_payloads_and_multibyte_resize_are_safe() {
    let messages = vec![
        msg(
            MessageType::Assistant,
            "### 观察\n逐行读代码，检查渲染、审批、工具卡和代码块 🚀\n```rust\nfn main() {}\n```",
        ),
        tool_msg(MessageType::ToolResult, "Bash", "{\"stdout\":\"逐行读代码"),
        tool_msg(
            MessageType::ToolUse,
            "Task",
            "{\"description\":\"逐行阅读\",\"prompt\":\"读代码",
        ),
    ];

    for (width, height) in [(36, 30), (80, 20), (132, 24)] {
        let snapshot = render_messages(&messages, width, height);
        let name = format!("malformed payload width {width}");

        assert_snapshot_has(
            &name,
            &snapshot,
            &[
                "观察",
                "Bash",
                "malformed output",
                "Task",
                "malformed input",
            ],
        );
        assert_no_protocol_noise(&name, &snapshot);
        assert!(
            !snapshot.contains("Render error") && !snapshot.contains("panicked"),
            "malformed payload/resize snapshot should not enter render error mode\n--- snapshot ---\n{snapshot}"
        );
    }
}
