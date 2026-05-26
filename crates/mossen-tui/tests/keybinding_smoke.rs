//! Keyboard/render smoke for the TUI — drives the same `App::handle_key`
//! dispatcher the event loop uses and asserts the visible state after
//! each interaction. Catches regressions like:
//!   * Ctrl+E toggling `show_all_thinking`
//!   * Up/Down moving keyboard focus across messages
//!   * Space collapsing/expanding a focused ToolUse group
//!   * → / ← swapping a ToolResult between preview and full_content
//!   * Esc clearing focus on first press, opening MessageSelector on second
//!
//! These all run without raw mode / a real terminal — `App::new()` builds
//! a default app and `dispatch_key_for_test` is the test seam.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mossen_agent::types::{PermissionMode, SdkMessage};
use mossen_tools::todo::TodoItem;
use mossen_tui::app::{ActiveModal, PickerKind};
use mossen_tui::message_model::{display_tool_name, MessageData, MessageType};
use mossen_tui::render_glyphs::RenderGlyphs;
use mossen_tui::render_model::{
    approval_decision_message_content, final_summary_message_content, ApprovalDecisionKind,
    ApprovalDecisionModel, CommandSummaryModel, FileChangeSummaryModel, FinalSummaryModel,
    FooterItem, ProcessRowKind, ProcessStatus, VerificationSummaryModel,
};
use mossen_tui::state::{
    McpConnectionState, McpServerStatus, RenderActivity, SlashCommandInfo, SlashCommandKind,
    TeammateState, TurnState, UiStage,
};
use mossen_tui::App;
use mossen_types::command::{CommandLoadedFrom, PromptCommandSource};
use mossen_types::{ContentBlock, Message, Role, TextBlock};
use ratatui::{backend::TestBackend, Terminal};
use std::collections::HashMap;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn mouse_scroll(kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

fn mouse_at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn history_message(role: Role, text: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text(TextBlock {
            text: text.to_string(),
        })],
        uuid: None,
        is_meta: None,
        origin: None,
        timestamp: None,
        extra: HashMap::new(),
    }
}

fn test_skill(name: &str) -> mossen_skills::CraftCommand {
    mossen_skills::create_skill_command(mossen_skills::CreateSkillCommandInput {
        skill_name: name.to_string(),
        display_name: None,
        description: "Test dynamic skill".to_string(),
        has_user_specified_description: true,
        markdown_content: "Use this test skill.".to_string(),
        allowed_tools: Vec::new(),
        argument_hint: None,
        argument_names: Vec::new(),
        when_to_use: None,
        version: None,
        model: None,
        disable_model_invocation: false,
        user_invocable: true,
        source: PromptCommandSource::ProjectSettings,
        base_dir: None,
        loaded_from: CommandLoadedFrom::Skills,
        hooks: None,
        execution_context: None,
        agent: None,
        paths: None,
        effort: None,
        shell: None,
    })
}

fn render_app_text(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    terminal
        .draw(|frame| app.render_for_test(frame))
        .expect("draw app frame");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer.content[buffer.index_of(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn line_index_containing(rendered: &str, needle: &str) -> Option<usize> {
    rendered.lines().position(|line| line.contains(needle))
}

fn scrollbar_thumb_rows(rendered: &str, width: usize) -> Vec<usize> {
    rendered
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.chars().nth(width.saturating_sub(1)) == Some('#')).then_some(index)
        })
        .collect()
}

fn scrollbar_rail_rows(rendered: &str, width: usize) -> Vec<usize> {
    rendered
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            matches!(
                line.chars().nth(width.saturating_sub(1)),
                Some('#') | Some('|')
            )
            .then_some(index)
        })
        .collect()
}

fn scrollbar_rail_column_rows(rendered: &str) -> Option<(usize, Vec<usize>)> {
    let mut columns: HashMap<usize, Vec<usize>> = HashMap::new();
    for (row, line) in rendered.lines().enumerate() {
        for (column, ch) in line.chars().enumerate() {
            if matches!(ch, '#' | '|') {
                columns.entry(column).or_default().push(row);
            }
        }
    }
    columns
        .into_iter()
        .filter(|(_, rows)| rows.len() >= 3)
        .max_by_key(|(_, rows)| rows.len())
}

fn seed_tool_use_pair(app: &mut App) -> (usize, usize) {
    let user_idx = app.messages.len();
    app.messages.push(MessageData {
        message_type: MessageType::User,
        content: "run ls".to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
    let tu_idx = app.messages.len();
    app.messages.push(MessageData {
        message_type: MessageType::ToolUse,
        content: "ls -la".to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
    let tr_idx = app.messages.len();
    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: "preview".to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: Some("full output that is much longer than the preview".to_string()),
        expanded: false,
    });
    let _ = user_idx;
    (tu_idx, tr_idx)
}

fn seed_diff_result(app: &mut App) {
    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: serde_json::json!({
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
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
}

fn seed_long_diff_result(app: &mut App) {
    let mut stdout = concat!(
        "diff --git a/src/long.rs b/src/long.rs\n",
        "--- a/src/long.rs\n",
        "+++ b/src/long.rs\n",
        "@@ -1,36 +1,36 @@\n",
    )
    .to_string();
    for index in 0..36 {
        stdout.push_str(&format!("-old diff review row {index:02}\n"));
        stdout.push_str(&format!("+new diff review row {index:02}\n"));
    }

    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: serde_json::json!({
            "stdout": stdout,
            "exit_code": 0
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
}

fn seed_file_change_result(app: &mut App) {
    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: serde_json::json!({
            "file_path": "src/lib.rs",
            "old_string": "fn main() {\n    println!(\"old\");\n}\n",
            "new_string": "fn main() {\n    println!(\"new\");\n    println!(\"extra\");\n}\n"
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Write".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
}

fn seed_many_file_change_results(app: &mut App, count: usize) {
    for index in 0..count {
        app.messages.push(MessageData {
            message_type: MessageType::ToolResult,
            content: serde_json::json!({
                "file_path": format!("src/generated_{index:02}.rs"),
                "old_string": format!("pub fn value_{index}() -> usize {{ 1 }}\n"),
                "new_string": format!(
                    "pub fn value_{index}() -> usize {{\n    {index}\n}}\n"
                )
            })
            .to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Write".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }
}

fn seed_render_timeline(app: &mut App) {
    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "timeline-session".to_string(),
        model: "MiniMax-M2.7".to_string(),
        tools: Vec::new(),
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-timeline-bash".to_string()),
        summary: serde_json::json!({
            "stdout": "timeline smoke\nok\n",
            "exit_code": 0,
            "duration_ms": 120
        })
        .to_string(),
        full_content: None,
        task_id: None,
    });
}

fn seed_command_history(app: &mut App) {
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
    app.messages.push(MessageData {
        message_type: MessageType::ToolUse,
        content: serde_json::json!({
            "command": "cargo test -p mossen-tui command_history",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: serde_json::json!({
            "stdout": "running command history tests\nok\n",
            "stderr": "warning: existing warning noise\n",
            "exit_code": 0,
            "duration_ms": 88
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: Some(
            serde_json::json!({
                "stdout": "running command history tests\nok\nfull log tail\n",
                "stderr": "warning: existing warning noise\n",
                "exit_code": 0,
                "duration_ms": 88
            })
            .to_string(),
        ),
        expanded: false,
    });
}

fn seed_many_command_history(app: &mut App, count: usize) {
    for index in 0..count {
        app.messages.push(MessageData {
            message_type: MessageType::ToolUse,
            content: serde_json::json!({
                "command": format!("cargo test -p mossen-tui command_history_{index:02}"),
                "cwd": "/Users/allen/Documents/rustmossen"
            })
            .to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
        app.messages.push(MessageData {
            message_type: MessageType::ToolResult,
            content: serde_json::json!({
                "stdout": format!("running command history test {index:02}\nok\n"),
                "stderr": "",
                "exit_code": 0,
                "duration_ms": 80 + index
            })
            .to_string(),
            timestamp: None,
            is_streaming: false,
            tool_name: Some("Bash".to_string()),
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }
}

fn seed_error_history(app: &mut App) {
    app.state.ui_stage = UiStage::Failed;
    app.state.render_activity.set(RenderActivity::Error {
        source: "engine".to_string(),
        summary: "model stream interrupted".to_string(),
    });
    app.messages.push(MessageData {
        message_type: MessageType::System,
        content: "Build failed\nerror[E0425]: cannot find value `missing`".to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: true,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
    app.messages.push(MessageData {
        message_type: MessageType::ToolUse,
        content: serde_json::json!({
            "command": "cargo test -p mossen-tui error_history",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
    app.messages.push(MessageData {
        message_type: MessageType::ToolResult,
        content: serde_json::json!({
            "stdout": "running error history tests\n",
            "stderr": "thread 'render' panicked\nassertion failed\n",
            "stderr_hidden_lines": 6,
            "exit_code": 1,
            "duration_ms": 88,
            "error": "tests failed"
        })
        .to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: Some("Bash".to_string()),
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
}

fn seed_final_summary_history(app: &mut App) {
    let summary = FinalSummaryModel {
        id: "summary-keybinding".to_string(),
        success: true,
        terminal: "Completed".to_string(),
        changed_files: vec![FileChangeSummaryModel {
            path: "crates/mossen-tui/src/widgets/summary_history.rs".to_string(),
            status: "A".to_string(),
            additions: 180,
            deletions: 0,
        }],
        commands: vec![CommandSummaryModel {
            command: "cargo test -p mossen-tui --test keybinding_smoke".to_string(),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
            exit_code: Some(0),
            duration_ms: Some(512),
            status: "passed".to_string(),
        }],
        verification_results: vec![VerificationSummaryModel {
            command: "cargo check -p mossen-tui".to_string(),
            status: "passed".to_string(),
            passed: true,
            exit_code: Some(0),
            duration_ms: Some(300),
        }],
        residual_risks: Vec::new(),
        notes: vec!["Task execution code was untouched".to_string()],
    };
    app.messages.push(MessageData {
        message_type: MessageType::System,
        content: final_summary_message_content(&summary),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
}

fn seed_approval_history(app: &mut App) {
    let allowed = ApprovalDecisionModel {
        id: "approval-keybinding-allowed".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::Allowed,
        detail: "cargo test -p mossen-tui --test keybinding_smoke".to_string(),
        anchor_block_id: Some("tool-keybinding-1".to_string()),
    };
    let denied = ApprovalDecisionModel {
        id: "approval-keybinding-denied".to_string(),
        tool_name: "Write".to_string(),
        decision: ApprovalDecisionKind::Denied,
        detail: "crates/mossen-tui/src/task_execution.rs".to_string(),
        anchor_block_id: None,
    };
    for decision in [allowed, denied] {
        app.messages.push(MessageData {
            message_type: MessageType::System,
            content: approval_decision_message_content(&decision),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }
}

fn seed_process_status(app: &mut App) {
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
}

fn seed_status_overview(app: &mut App) {
    app.engine_config.model = "MiniMax-M2.7".to_string();
    app.engine_config.api_base_url = Some("http://localhost:8000/v1".to_string());
    app.engine_config.api_key = Some("redacted-test-key".to_string());
    app.engine_config.output_style = Some("Concise".to_string());
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.engine_session_id = Some("session-render-status".to_string());
    app.total_cost_usd = 0.125;
    app.messages.push(MessageData {
        message_type: MessageType::User,
        content: "检查 /status 语义概览".to_string(),
        timestamp: None,
        is_streaming: false,
        tool_name: None,
        is_error: false,
        thinking: None,
        thinking_completed_at: None,
        full_content: None,
        expanded: false,
    });
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
}

#[test]
fn ctrl_e_toggles_show_all_thinking() {
    let mut app = App::new();
    assert!(!app.show_all_thinking);
    app.dispatch_key_for_test(ctrl('e'));
    assert!(
        app.show_all_thinking,
        "Ctrl+E should flip show_all_thinking on"
    );
    app.dispatch_key_for_test(ctrl('e'));
    assert!(!app.show_all_thinking, "Ctrl+E should flip back off");
}

#[test]
fn ctrl_c_cancels_waiting_turn_instead_of_quitting() {
    let mut app = App::new();
    app.state.is_waiting_for_response = true;

    app.dispatch_key_for_test(ctrl('c'));

    assert!(!app.should_quit, "Ctrl+C should cancel an in-flight turn");
    assert!(!app.state.is_waiting_for_response);
    assert!(
        app.messages
            .iter()
            .any(|message| message.content == "↯ Cancelled"),
        "cancel should leave a visible transcript marker"
    );
}

#[test]
fn ctrl_c_cancels_waiting_turn_even_when_modal_is_open() {
    let mut app = App::new();
    app.state.is_waiting_for_response = true;
    app.active_modal = ActiveModal::Picker {
        kind: PickerKind::BackgroundTasks,
        title: "Background Tasks".to_string(),
        items: vec!["agent-1".to_string()],
        selected: 0,
    };

    app.dispatch_key_for_test(ctrl('c'));

    assert!(!app.should_quit, "Ctrl+C should cancel, not quit");
    assert!(!app.state.is_waiting_for_response);
    assert!(matches!(app.active_modal, ActiveModal::None));
    assert!(
        app.messages
            .iter()
            .any(|message| message.content == "↯ Cancelled"),
        "cancel should leave a visible transcript marker"
    );
}

#[test]
fn arrows_move_focus_between_messages_when_prompt_empty() {
    let mut app = App::new();
    let (tu, tr) = seed_tool_use_pair(&mut app);
    // Prompt is empty + not streaming: plain arrows own transcript row scroll,
    // while Alt+arrows move message focus.
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
    assert!(app.focused_message_idx.is_some());
    // After auto-collapsing ToolUse, ToolResult is hidden — focus should
    // skip it on subsequent navigation. Force the collapse first so the
    // skip path is exercised.
    app.collapsed_tool_groups.insert(tu);
    app.focused_message_idx = Some(tu);
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    // ToolResult at `tr` is hidden, so focus should NOT land there.
    assert_ne!(app.focused_message_idx, Some(tr));
}

#[test]
fn space_toggles_tool_use_collapse() {
    let mut app = App::new();
    let (tu, _tr) = seed_tool_use_pair(&mut app);
    app.focused_message_idx = Some(tu);
    assert!(!app.collapsed_tool_groups.contains(&tu));
    app.dispatch_key_for_test(key(KeyCode::Char(' ')));
    assert!(
        app.collapsed_tool_groups.contains(&tu),
        "Space should collapse focused ToolUse"
    );
    app.dispatch_key_for_test(key(KeyCode::Char(' ')));
    assert!(
        !app.collapsed_tool_groups.contains(&tu),
        "Space again should expand"
    );
}

#[test]
fn right_left_expand_collapse_tool_result_full_content() {
    let mut app = App::new();
    let (_tu, tr) = seed_tool_use_pair(&mut app);
    app.focused_message_idx = Some(tr);
    assert!(!app.messages[tr].expanded);
    app.dispatch_key_for_test(key(KeyCode::Right));
    assert!(
        app.messages[tr].expanded,
        "→ should expand focused ToolResult"
    );
    app.dispatch_key_for_test(key(KeyCode::Left));
    assert!(
        !app.messages[tr].expanded,
        "← should collapse focused ToolResult"
    );
}

#[test]
fn esc_clears_focus_before_opening_selector() {
    let mut app = App::new();
    let (tu, _tr) = seed_tool_use_pair(&mut app);
    app.focused_message_idx = Some(tu);
    app.dispatch_key_for_test(key(KeyCode::Esc));
    assert!(app.focused_message_idx.is_none(), "first Esc clears focus");
    // Second Esc would open the selector — we don't validate the modal
    // state here because it depends on services setup, but the dispatch
    // must not panic.
    app.dispatch_key_for_test(key(KeyCode::Esc));
}

#[test]
fn ctrl_s_stashes_prompt_and_clears_input() {
    let mut app = App::new();
    app.prompt.input.insert_str("draft message");
    assert_eq!(app.prompt.input.value, "draft message");
    app.dispatch_key_for_test(ctrl('s'));
    assert!(
        app.prompt.input.value.is_empty(),
        "Ctrl+S should clear the prompt after stashing"
    );
}

#[test]
fn pending_image_inserts_marker_into_prompt() {
    let mut app = App::new();
    // Simulate a paste having already happened (no clipboard access in
    // CI). Directly seeding pending_images + inserting the marker the
    // way `try_paste_image` would.
    app.pending_images
        .push(("image/png".to_string(), "AAAA".to_string()));
    app.prompt.input.insert_str("[Image #1]");
    assert_eq!(app.pending_images.len(), 1);
    assert!(app.prompt.input.value.contains("[Image #1]"));
}

#[test]
fn theme_slash_opens_picker_and_enter_applies() {
    let mut app = App::new();
    // `/theme` runs through handle_submit which routes slash commands.
    app.prompt.input.insert_str("/theme");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(
        matches!(
            app.active_modal,
            mossen_tui::app::ActiveModal::Picker { .. }
        ),
        "/theme should open a Picker modal"
    );
    // ↓ moves selection, Enter applies.
    app.dispatch_key_for_test(key(KeyCode::Down));
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(
        matches!(app.active_modal, mossen_tui::app::ActiveModal::None),
        "Enter should close the picker"
    );
}

#[test]
fn slash_typeahead_lists_commands_and_accepts_with_prefix() {
    let mut app = App::new();

    app.dispatch_key_for_test(key(KeyCode::Char('/')));
    app.dispatch_key_for_test(key(KeyCode::Char('h')));
    app.dispatch_key_for_test(key(KeyCode::Char('e')));

    assert!(
        app.prompt.show_suggestions,
        "typing a slash command prefix should open typeahead"
    );
    assert!(
        app.prompt
            .suggestions
            .iter()
            .any(|suggestion| suggestion.label == "help"),
        "/he should include /help"
    );

    app.dispatch_key_for_test(key(KeyCode::Tab));

    assert_eq!(
        app.prompt.input.value, "/help ",
        "accepted slash suggestions must keep the slash prefix"
    );
    assert!(!app.prompt.show_suggestions);

    let mut permissions_app = App::new();
    permissions_app.dispatch_key_for_test(key(KeyCode::Char('/')));
    permissions_app.dispatch_key_for_test(key(KeyCode::Char('p')));
    permissions_app.dispatch_key_for_test(key(KeyCode::Char('e')));
    permissions_app.dispatch_key_for_test(key(KeyCode::Char('r')));
    assert!(
        permissions_app
            .prompt
            .suggestions
            .iter()
            .any(|suggestion| suggestion.label == "permissions"),
        "/per should include /permissions"
    );
}

#[test]
fn slash_catalog_matches_aliases_and_shows_argument_hints() {
    let mut app = App::new();
    let config_entry = app
        .state
        .all_slash_commands
        .iter()
        .find(|entry| entry.name == "config")
        .expect("/config should be present in the slash catalog");
    assert!(config_entry.aliases.iter().any(|alias| alias == "settings"));
    assert_eq!(config_entry.argument_hint, "[key=value]");

    for ch in "/settings".chars() {
        app.dispatch_key_for_test(key(KeyCode::Char(ch)));
    }

    assert!(
        app.prompt.show_suggestions,
        "typing a slash alias should open typeahead"
    );
    let config_suggestion = app
        .prompt
        .suggestions
        .iter()
        .find(|suggestion| suggestion.label == "config")
        .expect("/settings should match the canonical /config suggestion");
    let description = config_suggestion.description.as_deref().unwrap_or("");
    assert!(description.contains("args: [key=value]"), "{description}");
    assert!(description.contains("aliases: /settings"), "{description}");

    let mut help_app = App::new();
    help_app.prompt.input.insert_str("/help settings");
    help_app.dispatch_key_for_test(key(KeyCode::Enter));
    let rendered = render_app_text(&mut help_app, 110, 24);
    assert!(rendered.contains("/config [key=value]"), "{rendered}");
    assert!(rendered.contains("aliases: /settings"), "{rendered}");
}

#[test]
fn slash_typeahead_page_down_scrolls_visible_suggestions() {
    fn catalog_entry(index: usize) -> SlashCommandInfo {
        SlashCommandInfo {
            name: format!("cmd{index:02}"),
            description: format!("Command {index:02}"),
            category: "Smoke".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        }
    }

    let mut app = App::new();
    app.state.all_slash_commands = (0..12).map(catalog_entry).collect();

    app.dispatch_key_for_test(key(KeyCode::Char('/')));
    assert!(app.prompt.show_suggestions);
    assert_eq!(app.prompt.selected_suggestion, Some(0));

    app.dispatch_key_for_test(key(KeyCode::PageDown));
    assert_eq!(
        app.prompt.selected_suggestion,
        Some(5),
        "PageDown should page within slash suggestions instead of the transcript"
    );
    app.dispatch_key_for_test(key(KeyCode::Tab));
    assert_eq!(app.prompt.input.value, "/cmd05 ");
}

#[test]
fn help_dialog_scrolls_and_filters_slash_catalog() {
    fn catalog_entry(index: usize) -> SlashCommandInfo {
        SlashCommandInfo {
            name: format!("command-{index:02}"),
            description: format!("Catalog smoke command {index:02}"),
            category: "Smoke".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        }
    }

    let mut app = App::new();
    app.state.all_slash_commands = (0..35).map(catalog_entry).collect();

    app.prompt.input.insert_str("/help");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let first = render_app_text(&mut app, 90, 18);
    assert!(first.contains("/command-00"), "{first}");
    assert!(
        !first.contains("/command-20"),
        "initial help viewport should not show far rows\n{first}"
    );

    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::HelpDialog(state) = &app.active_modal else {
        panic!("/help should keep the scrollable help modal open");
    };
    assert_eq!(
        state.scroll, 11,
        "PageDown should use the last rendered help body height"
    );
    let scrolled = render_app_text(&mut app, 90, 18);
    assert!(scrolled.contains("12-22/36"), "{scrolled}");
    assert!(scrolled.contains("/command-15"), "{scrolled}");
    assert!(
        !scrolled.contains("/command-00"),
        "scrolled help viewport should move past the first command\n{scrolled}"
    );

    let mut filtered = App::new();
    filtered.state.all_slash_commands = (0..35).map(catalog_entry).collect();
    filtered.prompt.input.insert_str("/help command-29");
    filtered.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::HelpDialog(state) = &filtered.active_modal else {
        panic!("/help <query> should open the help modal");
    };
    assert_eq!(state.query, "command-29");
    let filtered_help = render_app_text(&mut filtered, 90, 18);
    assert!(filtered_help.contains("Filter:"), "{filtered_help}");
    assert!(filtered_help.contains("command-29"), "{filtered_help}");
    assert!(!filtered_help.contains("command-00"), "{filtered_help}");
}

#[test]
fn command_output_modal_scrolls_long_body() {
    let mut app = App::new();
    let body = (0..40)
        .map(|index| format!("row-{index:02} command output line"))
        .collect::<Vec<_>>()
        .join("\n");
    app.active_modal = ActiveModal::CommandOutput {
        title: "Long Command".to_string(),
        body,
        is_error: false,
    };

    let first = render_app_text(&mut app, 92, 18);
    assert!(first.contains("1-11/40"), "{first}");
    assert!(first.contains("row-00"), "{first}");
    assert!(!first.contains("row-30"), "{first}");

    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let paged = render_app_text(&mut app, 92, 18);
    assert!(paged.contains("12-22/40"), "{paged}");
    assert!(paged.contains("row-11"), "{paged}");
    assert!(
        !paged.contains("row-00"),
        "PageDown should move the command-output viewport\n{paged}"
    );
    assert!(
        !paged.contains("row-08"),
        "PageDown should use the last rendered command-output body height\n{paged}"
    );

    app.dispatch_key_for_test(key(KeyCode::End));
    let bottom = render_app_text(&mut app, 92, 18);
    assert!(bottom.contains("row-39"), "{bottom}");
    assert!(!bottom.contains("row-00"), "{bottom}");

    app.dispatch_key_for_test(key(KeyCode::Home));
    let top = render_app_text(&mut app, 92, 18);
    assert!(top.contains("row-00"), "{top}");

    app.dispatch_key_for_test(key(KeyCode::Esc));
    assert!(matches!(app.active_modal, ActiveModal::None));
}

#[test]
fn command_output_modal_scrollbar_tracks_mouse_click_and_drag() {
    let mut app = App::new();
    app.glyphs = RenderGlyphs::ascii();
    let body = (0..40)
        .map(|index| format!("modal rail row-{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.active_modal = ActiveModal::CommandOutput {
        title: "Long Command".to_string(),
        body,
        is_error: false,
    };

    let first = render_app_text(&mut app, 92, 18);
    assert!(first.contains("1-11/40"), "{first}");
    let (rail_column, rail_rows) = scrollbar_rail_column_rows(&first)
        .unwrap_or_else(|| panic!("command output should render a modal scrollbar\n{first}"));
    let top_row = *rail_rows.first().expect("modal rail top row");
    let bottom_row = *rail_rows.last().expect("modal rail bottom row");

    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Down(MouseButton::Left),
        rail_column as u16,
        bottom_row as u16,
    ));
    let bottom = render_app_text(&mut app, 92, 18);
    assert!(bottom.contains("modal rail row-39"), "{bottom}");
    assert!(!bottom.contains("modal rail row-00"), "{bottom}");

    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Drag(MouseButton::Left),
        rail_column as u16,
        top_row as u16,
    ));
    let top = render_app_text(&mut app, 92, 18);
    assert!(top.contains("modal rail row-00"), "{top}");
    assert!(!top.contains("modal rail row-39"), "{top}");
}

#[test]
fn mouse_wheel_scrolls_active_modal_before_transcript() {
    fn catalog_entry(index: usize) -> SlashCommandInfo {
        SlashCommandInfo {
            name: format!("command-{index:02}"),
            description: format!("Catalog smoke command {index:02}"),
            category: "Smoke".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        }
    }

    let mut help_app = App::new();
    help_app.state.all_slash_commands = (0..35).map(catalog_entry).collect();
    help_app.scroll.set_total_items(200);
    help_app.scroll.sticky = false;
    help_app.scroll.offset = 50;
    help_app.prompt.input.insert_str("/help");
    help_app.dispatch_key_for_test(key(KeyCode::Enter));

    help_app.dispatch_mouse_for_test(mouse_scroll(MouseEventKind::ScrollDown));
    let ActiveModal::HelpDialog(state) = &help_app.active_modal else {
        panic!("mouse wheel should leave the help modal open");
    };
    assert_eq!(state.scroll, 3);
    assert_eq!(
        help_app.scroll.offset, 50,
        "mouse wheel must not scroll the transcript behind an active modal"
    );

    help_app.dispatch_mouse_for_test(mouse_scroll(MouseEventKind::ScrollUp));
    let ActiveModal::HelpDialog(state) = &help_app.active_modal else {
        panic!("mouse wheel should leave the help modal open");
    };
    assert_eq!(state.scroll, 0);

    let mut output_app = App::new();
    output_app.scroll.set_total_items(200);
    output_app.scroll.sticky = false;
    output_app.scroll.offset = 40;
    let body = (0..40)
        .map(|index| format!("row-{index:02} command output line"))
        .collect::<Vec<_>>()
        .join("\n");
    output_app.active_modal = ActiveModal::CommandOutput {
        title: "Long Command".to_string(),
        body,
        is_error: false,
    };

    output_app.dispatch_mouse_for_test(mouse_scroll(MouseEventKind::ScrollDown));
    assert_eq!(
        output_app.scroll.offset, 40,
        "command-output wheel input should stay inside the modal"
    );
    let scrolled = render_app_text(&mut output_app, 92, 18);
    assert!(scrolled.contains("row-03"), "{scrolled}");
    assert!(!scrolled.contains("row-00"), "{scrolled}");

    output_app.dispatch_mouse_for_test(mouse_scroll(MouseEventKind::ScrollUp));
    let top = render_app_text(&mut output_app, 92, 18);
    assert!(top.contains("row-00"), "{top}");
}

#[test]
fn diff_review_uses_rendered_viewport_for_scroll_range() {
    let mut app = App::new();
    seed_long_diff_result(&mut app);
    app.prompt.input.insert_str("/diff");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::DiffReview(_)));

    let first = render_app_text(&mut app, 100, 18);
    assert!(
        first.contains("1-11/76"),
        "diff footer should expose the rendered viewport range\n{first}"
    );

    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::DiffReview(state) = &app.active_modal else {
        panic!("diff modal should stay open after PageDown");
    };
    assert_eq!(
        state.scroll, 11,
        "PageDown should use the last rendered diff body height, not a fixed 20 rows"
    );
    let paged = render_app_text(&mut app, 100, 18);
    assert!(paged.contains("12-22/76"), "{paged}");
    assert!(paged.contains("old diff review row 04"), "{paged}");
}

#[test]
fn raw_transcript_uses_rendered_viewport_for_page_navigation() {
    let mut app = App::new();
    for index in 0..20 {
        app.messages.push(MessageData {
            message_type: MessageType::System,
            content: format!("raw transcript row {index:02}"),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }

    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));
    let first = render_app_text(&mut app, 100, 18);
    assert!(first.contains("Raw Transcript"), "{first}");

    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::RawTranscript(state) = &app.active_modal else {
        panic!("raw transcript modal should stay open after PageDown");
    };
    assert_eq!(
        state.scroll, 11,
        "PageDown should use the rendered raw transcript body height"
    );
}

#[test]
fn semantic_list_modals_use_rendered_viewport_for_page_navigation() {
    let mut files_app = App::new();
    seed_many_file_change_results(&mut files_app, 30);
    files_app.prompt.input.insert_str("/files");
    files_app.dispatch_key_for_test(key(KeyCode::Enter));
    let first = render_app_text(&mut files_app, 100, 18);
    assert!(first.contains("File Changes"), "{first}");

    files_app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::FileChanges(state) = &files_app.active_modal else {
        panic!("file changes modal should stay open after PageDown");
    };
    assert_eq!(
        state.selected, 9,
        "PageDown should use the rendered file-change row viewport"
    );

    let mut commands_app = App::new();
    seed_many_command_history(&mut commands_app, 30);
    commands_app.prompt.input.insert_str("/commands");
    commands_app.dispatch_key_for_test(key(KeyCode::Enter));
    let first = render_app_text(&mut commands_app, 104, 18);
    assert!(first.contains("Command History"), "{first}");

    commands_app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::CommandHistory(state) = &commands_app.active_modal else {
        panic!("command history modal should stay open after PageDown");
    };
    assert_eq!(
        state.selected, 9,
        "PageDown should use the rendered command-history row viewport"
    );
}

#[test]
fn transcript_scrollbar_tracks_sticky_and_manual_scroll() {
    let mut app = App::new();
    app.glyphs = RenderGlyphs::ascii();
    for index in 0..80 {
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: format!("scrollbar transcript row {index:02}"),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }

    let bottom = render_app_text(&mut app, 80, 20);
    assert!(bottom.contains("scrollbar transcript row 79"), "{bottom}");
    let bottom_thumb = scrollbar_thumb_rows(&bottom, 80);
    assert!(
        !bottom_thumb.is_empty(),
        "long transcript should render a scrollbar thumb\n{bottom}"
    );
    assert!(
        bottom.lines().any(|line| line.ends_with('|')),
        "scrollbar should include a visible track, not only a thumb\n{bottom}"
    );

    app.scroll.scroll_up(20);
    let scrolled = render_app_text(&mut app, 80, 20);
    let scrolled_thumb = scrollbar_thumb_rows(&scrolled, 80);
    assert!(
        !scrolled_thumb.is_empty(),
        "manual scroll should keep the scrollbar visible\n{scrolled}"
    );
    assert!(
        scrolled_thumb[0] < bottom_thumb[0],
        "scrollbar thumb should move upward after manual scroll\nbottom={bottom_thumb:?}\nscrolled={scrolled_thumb:?}\n{scrolled}"
    );
    assert!(
        !scrolled.contains("scrollbar transcript row 79"),
        "manual scroll should leave sticky bottom before rendering\n{scrolled}"
    );
}

#[test]
fn transcript_scrollbar_tracks_mouse_click_and_drag() {
    let mut app = App::new();
    app.glyphs = RenderGlyphs::ascii();
    for index in 0..80 {
        app.messages.push(MessageData {
            message_type: MessageType::Assistant,
            content: format!("scrollbar pointer row {index:02}"),
            timestamp: None,
            is_streaming: false,
            tool_name: None,
            is_error: false,
            thinking: None,
            thinking_completed_at: None,
            full_content: None,
            expanded: false,
        });
    }

    let bottom = render_app_text(&mut app, 80, 20);
    assert!(bottom.contains("scrollbar pointer row 79"), "{bottom}");
    let rail_rows = scrollbar_rail_rows(&bottom, 80);
    assert!(
        !rail_rows.is_empty(),
        "long transcript should render a clickable scrollbar rail\n{bottom}"
    );
    let top_row = *rail_rows.first().expect("rail top row");
    let bottom_row = *rail_rows.last().expect("rail bottom row");
    let bottom_offset = app.scroll.offset;

    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Down(MouseButton::Left),
        79,
        top_row as u16,
    ));
    assert!(
        app.scroll.offset < bottom_offset,
        "clicking the top of the rail should move away from sticky bottom"
    );
    assert!(!app.scroll.sticky);
    let top_click = render_app_text(&mut app, 80, 20);
    assert!(
        !top_click.contains("scrollbar pointer row 79"),
        "top rail click should leave the transcript tail\n{top_click}"
    );

    let top_click_rail_rows = scrollbar_rail_rows(&top_click, 80);
    let prompt_row = top_click
        .lines()
        .position(|line| line.contains("Ask anything"))
        .unwrap_or(usize::MAX);
    let rail_bottom_before_prompt = top_click_rail_rows
        .into_iter()
        .rfind(|row| *row < prompt_row)
        .unwrap_or(bottom_row);

    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Drag(MouseButton::Left),
        79,
        rail_bottom_before_prompt as u16,
    ));
    assert!(
        app.scroll.sticky,
        "dragging to the rail bottom should restore sticky bottom"
    );
    let dragged = render_app_text(&mut app, 80, 20);
    assert!(dragged.contains("scrollbar pointer row 79"), "{dragged}");
}

#[test]
fn builtin_slash_commands_open_expected_ui() {
    let mut app = App::new();
    let dir = tempfile::tempdir().expect("tempdir should be created");
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();

    app.prompt.input.insert_str("/help");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::HelpDialog(_)));

    app.active_modal = ActiveModal::None;
    seed_status_overview(&mut app);
    app.prompt.input.insert_str("/status");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::StatusDialog));
    let status = render_app_text(&mut app, 110, 32);
    assert!(status.contains("Status"), "{status}");
    assert!(status.contains("Session"), "{status}");
    assert!(status.contains("Turn"), "{status}");
    assert!(status.contains("Policy"), "{status}");
    assert!(status.contains("Workspace"), "{status}");
    assert!(status.contains("MiniMax-M2.7"), "{status}");
    assert!(status.contains("running command"), "{status}");
    assert!(status.contains("API Key"), "{status}");
    assert!(status.contains("configured"), "{status}");
    assert!(status.contains("MCP"), "{status}");

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/debug-config");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::DebugConfig(state) = &app.active_modal else {
        panic!("/debug-config should open the redacted semantic debug config modal");
    };
    assert!(state.model.summary.contains("secrets redacted"));
    assert!(state.model.row_count() > 12);
    let debug_config = render_app_text(&mut app, 112, 30);
    assert!(debug_config.contains("Debug Config"), "{debug_config}");
    assert!(debug_config.contains("Engine"), "{debug_config}");
    assert!(debug_config.contains("API Key"), "{debug_config}");
    assert!(debug_config.contains("configured"), "{debug_config}");
    assert!(debug_config.contains("effort"), "{debug_config}");
    assert!(
        !debug_config.contains("redacted-test-key"),
        "debug config must not print API key values\n{debug_config}"
    );
    app.dispatch_key_for_test(key(KeyCode::Down));
    let ActiveModal::DebugConfig(state) = &app.active_modal else {
        panic!("debug config modal should stay open after scrolling");
    };
    assert_eq!(state.scroll, 1);
    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::DebugConfig(state) = &app.active_modal else {
        panic!("debug config modal should stay open after PageDown");
    };
    assert_eq!(
        state.scroll, 22,
        "PageDown should use the rendered debug-config body height"
    );
    let scrolled_debug_config = render_app_text(&mut app, 112, 30);
    assert!(
        scrolled_debug_config.contains("Fullscreen"),
        "{scrolled_debug_config}"
    );
    assert!(
        scrolled_debug_config.contains("Runtime"),
        "{scrolled_debug_config}"
    );
    app.dispatch_key_for_test(key(KeyCode::End));
    let ActiveModal::DebugConfig(state) = &app.active_modal else {
        panic!("debug config modal should stay open after End");
    };
    assert_eq!(
        state.scroll,
        state.model.row_count().saturating_sub(21),
        "End should clamp to the last rendered debug-config viewport, not an empty tail"
    );
    app.dispatch_key_for_test(key(KeyCode::PageDown));
    let ActiveModal::DebugConfig(state) = &app.active_modal else {
        panic!("debug config modal should stay open after extra PageDown");
    };
    assert_eq!(
        state.scroll,
        state.model.row_count().saturating_sub(21),
        "PageDown at the bottom should not overscroll debug-config"
    );

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/tasks");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::TasksDialog));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/mcp");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::McpServersDialog));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/model");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::ModelPicker(_)));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/skills");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::SkillsPanel(_)));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/memory");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::MemoryPanel(_)));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/raw");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));

    app.active_modal = ActiveModal::None;
    seed_process_status(&mut app);
    app.prompt.input.insert_str("/ps");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::ProcessList(state) = &app.active_modal else {
        panic!("/ps should open the semantic process/status modal");
    };
    assert_eq!(state.selected, 0);
    assert_eq!(state.model.summary.stage, "running command");
    assert!(state.model.summary.active_count >= 3);
    assert!(state
        .model
        .rows
        .iter()
        .any(|row| row.kind == ProcessRowKind::Activity
            && row.status == ProcessStatus::Running
            && row.title == "Command activity"));
    assert!(state.model.rows.iter().any(|row| {
        row.kind == ProcessRowKind::Todo && row.title.contains("终端渲染检查面板")
    }));
    assert!(state
        .model
        .rows
        .iter()
        .any(|row| { row.kind == ProcessRowKind::Agent && row.title == "agent-render-review" }));
    app.dispatch_key_for_test(key(KeyCode::Down));
    let ActiveModal::ProcessList(state) = &app.active_modal else {
        panic!("process modal should stay open after row navigation");
    };
    assert_eq!(state.selected, 1);

    app.active_modal = ActiveModal::None;
    seed_command_history(&mut app);
    app.prompt.input.insert_str("/commands");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::CommandHistory(state) = &app.active_modal else {
        panic!("/commands should open the semantic command history modal");
    };
    assert_eq!(state.selected, 0);
    assert!(state.model.summary.total_count >= 2);
    assert!(state.model.summary.running_count >= 1);
    assert!(state.model.rows.iter().any(|row| row
        .title
        .contains("cargo test -p mossen-tui command_history")));
    let rendered = render_app_text(&mut app, 104, 24);
    assert!(rendered.contains("Command History"));
    assert!(rendered.contains("cargo test -p mossen-tui command_history"));
    assert!(rendered.contains("stdout"));
    app.dispatch_key_for_test(key(KeyCode::Down));
    let ActiveModal::CommandHistory(state) = &app.active_modal else {
        panic!("command history modal should stay open after row navigation");
    };
    assert_eq!(state.selected, 1);
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::CommandHistory(state) = &app.active_modal else {
        panic!("command history modal should stay open after expanding full log");
    };
    assert!(state
        .expanded_rows
        .contains(&state.model.rows[state.selected].id));
    let rendered = render_app_text(&mut app, 104, 24);
    assert!(rendered.contains("full log: expanded"));
    assert!(rendered.contains("full log tail"));

    app.active_modal = ActiveModal::None;
    seed_error_history(&mut app);
    app.prompt.input.insert_str("/errors");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::ErrorHistory(state) = &app.active_modal else {
        panic!("/errors should open the semantic error history modal");
    };
    assert_eq!(state.selected, 0);
    assert!(state.model.summary.total_count >= 3);
    assert!(state.model.summary.command_failure_count >= 1);
    assert!(state
        .model
        .rows
        .iter()
        .any(|row| row.summary.contains("tests failed")));
    let rendered = render_app_text(&mut app, 104, 24);
    assert!(rendered.contains("Error History"));
    assert!(rendered.contains("model stream interrupted"));
    app.dispatch_key_for_test(key(KeyCode::Down));
    let ActiveModal::ErrorHistory(state) = &app.active_modal else {
        panic!("error history modal should stay open after row navigation");
    };
    assert_eq!(state.selected, 1);
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::ErrorHistory(state) = &app.active_modal else {
        panic!("error history modal should stay open after expanding details");
    };
    assert!(state
        .expanded_rows
        .contains(&state.model.rows[state.selected].id));
    let rendered = render_app_text(&mut app, 104, 24);
    assert!(rendered.contains("details: expanded"));
    assert!(rendered.contains("cannot find value"));

    app.active_modal = ActiveModal::None;
    seed_final_summary_history(&mut app);
    app.prompt.input.insert_str("/results");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::FinalSummaryHistory(state) = &app.active_modal else {
        panic!("/results should open the semantic final summary modal");
    };
    assert_eq!(state.selected, 0);
    assert_eq!(state.model.summary.total_count, 1);
    assert_eq!(state.model.summary.completed_count, 1);
    assert!(state
        .model
        .rows
        .iter()
        .any(|row| row.commands.iter().any(|command| {
            command
                .command
                .contains("cargo test -p mossen-tui --test keybinding_smoke")
        })));
    let rendered = render_app_text(&mut app, 108, 24);
    assert!(rendered.contains("Final Summaries"));
    assert!(rendered.contains("summary_history.rs"));
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::FinalSummaryHistory(state) = &app.active_modal else {
        panic!("final summary modal should stay open after expanding details");
    };
    assert!(state
        .expanded_rows
        .contains(&state.model.rows[state.selected].id));
    let rendered = render_app_text(&mut app, 108, 24);
    assert!(rendered.contains("details: expanded"));
    assert!(rendered.contains("cargo check -p mossen-tui"));

    app.active_modal = ActiveModal::None;
    seed_approval_history(&mut app);
    app.prompt.input.insert_str("/approvals");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::ApprovalHistory(state) = &app.active_modal else {
        panic!("/approvals should open the semantic approval history modal");
    };
    assert_eq!(state.selected, 0);
    assert_eq!(state.model.summary.total_count, 2);
    assert_eq!(state.model.summary.allowed_count, 1);
    assert_eq!(state.model.summary.denied_count, 1);
    assert!(state.model.rows.iter().any(|row| row
        .detail
        .contains("cargo test -p mossen-tui --test keybinding_smoke")));
    let rendered = render_app_text(&mut app, 108, 24);
    assert!(rendered.contains("Approval History"));
    assert!(rendered.contains("[Allowed]"));
    assert!(rendered.contains("cargo test -p mossen-tui"));
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::ApprovalHistory(state) = &app.active_modal else {
        panic!("approval history modal should stay open after expanding details");
    };
    assert!(state
        .expanded_rows
        .contains(&state.model.rows[state.selected].id));
    let rendered = render_app_text(&mut app, 108, 24);
    assert!(rendered.contains("details: expanded"));
    assert!(rendered.contains("source block: approval-keybinding-allowed"));

    app.active_modal = ActiveModal::None;
    seed_diff_result(&mut app);
    app.prompt.input.insert_str("/diff");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::DiffReview(state) = &app.active_modal else {
        panic!("/diff should open the semantic diff review modal");
    };
    assert_eq!(state.files.len(), 2);
    assert_eq!(state.files[0].path, "src/new.rs");

    app.dispatch_key_for_test(key(KeyCode::Right));
    let ActiveModal::DiffReview(state) = &app.active_modal else {
        panic!("diff modal should stay open after file navigation");
    };
    assert_eq!(state.selected_file, 1);

    app.dispatch_key_for_test(key(KeyCode::Left));
    app.dispatch_key_for_test(key(KeyCode::Char(' ')));
    let ActiveModal::DiffReview(state) = &app.active_modal else {
        panic!("diff modal should stay open after folding");
    };
    assert!(state.collapsed_files.contains(&0));
    let rendered = render_app_text(&mut app, 100, 24);
    assert!(rendered.contains("Diff Review"));
    assert!(rendered.contains("File collapsed"));

    app.active_modal = ActiveModal::None;
    seed_file_change_result(&mut app);
    app.prompt.input.insert_str("/files");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::FileChanges(state) = &app.active_modal else {
        panic!("/files should open the semantic file changes modal");
    };
    assert_eq!(state.model.rows.len(), 1);
    assert_eq!(state.model.rows[0].path, "src/lib.rs");
    assert_eq!(state.model.summary.modified_count, 1);
    let rendered = render_app_text(&mut app, 96, 20);
    assert!(rendered.contains("File Changes"), "{rendered}");
    assert!(rendered.contains("src/lib.rs"), "{rendered}");
    assert!(rendered.contains("[M]"), "{rendered}");

    app.active_modal = ActiveModal::None;
    seed_render_timeline(&mut app);
    app.prompt.input.insert_str("/timeline");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::RenderTimeline(state) = &app.active_modal else {
        panic!("/timeline should open the structured render-event timeline modal");
    };
    assert!(state.model.summary.total_count >= 2);
    assert!(state
        .model
        .rows
        .iter()
        .any(|row| row.event == "command_finish"));
    let rendered = render_app_text(&mut app, 104, 22);
    assert!(rendered.contains("Render Timeline"), "{rendered}");
    assert!(rendered.contains("command_finish"), "{rendered}");
    assert!(rendered.contains("exit 0"), "{rendered}");

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/statusline");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::StatusLineConfig(_)));
    app.dispatch_key_for_test(key(KeyCode::Char(' ')));
    assert!(!app.state.footer_config.is_enabled(FooterItem::Project));

    app.active_modal = ActiveModal::None;
    app.prompt
        .input
        .insert_str("/statusline command printf keybinding-status");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(
        app.active_modal,
        ActiveModal::CommandOutput { .. }
    ));
    assert_eq!(
        app.state
            .footer_config
            .external_command
            .as_ref()
            .map(|config| config.command.as_str()),
        Some("printf keybinding-status")
    );
    assert!(app
        .state
        .footer_config
        .is_enabled(FooterItem::ExternalStatus));

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/title 渲染会话");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::TitleConfig(_)));
    assert_eq!(app.services.manual_title.as_deref(), Some("渲染会话"));
    assert!(app.services.title.get_title().contains("渲染会话"));
    let rendered = render_app_text(&mut app, 92, 18);
    assert!(rendered.contains("Session Title"), "{rendered}");
    assert!(rendered.contains("Current"), "{rendered}");
    assert!(rendered.contains("Draft"), "{rendered}");
    assert!(!rendered.contains("\u{1b}"), "{rendered}");
}

#[test]
fn clear_slash_requires_confirmation_and_clears_messages() {
    let mut app = App::new();
    seed_tool_use_pair(&mut app);
    assert!(!app.messages.is_empty());

    app.prompt.input.insert_str("/clear");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(matches!(app.active_modal, ActiveModal::ConfirmClear));

    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(matches!(app.active_modal, ActiveModal::None));
    assert!(
        app.messages.is_empty(),
        "/clear confirmation should clear chat"
    );
}

#[test]
fn compact_slash_surfaces_progress_state() {
    let mut app = App::new();
    app.engine_history = vec![
        history_message(Role::User, "one"),
        history_message(Role::Assistant, "two"),
        history_message(Role::User, "three"),
        history_message(Role::Assistant, "four"),
    ];

    app.prompt.input.insert_str("/compact");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(app.state.compact_in_progress);
    assert_eq!(app.engine_history.len(), 4);

    app.dispatch_tick_for_test();

    assert!(!app.state.compact_in_progress);
    assert_eq!(app.engine_history.len(), 4);
    assert_eq!(app.engine_history[0].is_meta, Some(true));
    let boundary_text = match &app.engine_history[0].content[0] {
        ContentBlock::Text(block) => block.text.as_str(),
        _ => panic!("compact boundary should be text"),
    };
    assert!(boundary_text.contains("[manual compact boundary"));
    let metadata = app.engine_history[0]
        .extra
        .get("compact_metadata")
        .expect("manual compact boundary should carry metadata");
    assert_eq!(metadata["trigger"], "manual");
    assert_eq!(metadata["compacted_message_count"], 2);
    let summary_text = match &app.engine_history[1].content[0] {
        ContentBlock::Text(block) => block.text.as_str(),
        _ => panic!("compact summary should be text"),
    };
    assert!(summary_text.contains("Earlier conversation summary"));
    assert!(summary_text.contains("user: one"));
    assert!(summary_text.contains("assistant: two"));
    assert!(
        app.messages
            .iter()
            .any(|message| message.content.contains("(compact) messages 4 -> 3")),
        "/compact should leave visible feedback in the transcript"
    );
}

#[test]
fn compact_slash_forwards_custom_instructions_to_compactor() {
    let mut app = App::new();
    app.engine_history = vec![
        history_message(Role::User, "one"),
        history_message(Role::Assistant, "two"),
        history_message(Role::User, "three"),
        history_message(Role::Assistant, "four"),
    ];

    app.prompt
        .input
        .insert_str("/compact preserve permission decisions and MCP context");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(app.state.compact_in_progress);
    assert_eq!(
        app.state.compact_progress.as_deref(),
        Some("Compacting conversation history with custom instructions...")
    );

    app.dispatch_tick_for_test();

    assert!(!app.state.compact_in_progress);
    assert_eq!(app.engine_history.len(), 4);
    assert_eq!(
        app.engine_history[0].extra["compact_metadata"]["trigger"],
        "manual"
    );
    let summary_text = match &app.engine_history[1].content[0] {
        ContentBlock::Text(block) => block.text.as_str(),
        _ => panic!("compact summary should be text"),
    };
    assert!(summary_text.contains(
        "Compaction instructions applied: preserve permission decisions and MCP context"
    ));
}

#[test]
fn compact_plan_slash_previews_without_mutating_history() {
    let mut app = App::new();
    app.engine_history = vec![
        history_message(Role::User, "one"),
        history_message(Role::Assistant, "two"),
        history_message(Role::User, "three"),
        history_message(Role::Assistant, "four"),
    ];

    app.prompt.input.insert_str("/compact plan");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(app.engine_history.len(), 4);
    let ActiveModal::CommandOutput {
        title,
        body,
        is_error,
    } = &app.active_modal
    else {
        panic!("/compact plan should open a preview modal");
    };
    assert_eq!(title, "Compact Plan");
    assert!(!is_error);
    assert!(body.contains("state: idle"), "{body}");
    assert!(body.contains("messages: 4 -> 3"), "{body}");
    assert!(body.contains("compacted messages: 2"), "{body}");
    assert!(body.contains("recent messages kept: 2"), "{body}");
    assert!(body.contains("estimated savings:"), "{body}");
    assert!(body.contains("hooks: not configured"), "{body}");
    assert!(body.contains("custom instructions: none"), "{body}");
    assert!(body.contains("Preview only"), "{body}");

    app.active_modal = ActiveModal::None;
    app.prompt
        .input
        .insert_str("/compact plan preserve permission decisions");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    let ActiveModal::CommandOutput { body, .. } = &app.active_modal else {
        panic!("/compact plan with custom instructions should open a preview modal");
    };
    assert!(
        body.contains("custom instructions: preserve permission decisions"),
        "{body}"
    );
}

#[test]
fn compact_status_and_cancel_keep_history_unmutated() {
    let mut app = App::new();
    app.engine_history = vec![
        history_message(Role::User, "one"),
        history_message(Role::Assistant, "two"),
        history_message(Role::User, "three"),
        history_message(Role::Assistant, "four"),
    ];

    app.prompt.input.insert_str("/compact");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    assert!(app.state.compact_in_progress);

    app.prompt.input.insert_str("/compact status");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    let ActiveModal::CommandOutput {
        title,
        body,
        is_error,
    } = &app.active_modal
    else {
        panic!("/compact status should open a status modal");
    };
    assert_eq!(title, "Compact Status");
    assert!(!is_error);
    assert!(body.contains("state: running"), "{body}");
    assert!(body.contains("cancellable: yes"), "{body}");
    assert!(body.contains("hint: /compact cancel"), "{body}");

    app.active_modal = ActiveModal::None;
    app.prompt.input.insert_str("/compact cancel");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(!app.state.compact_in_progress);
    assert_eq!(app.engine_history.len(), 4);
    assert!(app
        .messages
        .iter()
        .any(|message| message.content.contains("(compact) cancelled")));

    app.dispatch_tick_for_test();
    assert_eq!(app.engine_history.len(), 4);
}

#[test]
fn permissions_slash_picker_updates_visible_session_mode() {
    let mut app = App::new();

    app.prompt.input.insert_str("/permissions");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    let ActiveModal::Picker {
        kind: PickerKind::PermissionMode,
        title,
        items,
        selected,
    } = &app.active_modal
    else {
        panic!("/permissions should open the permission mode picker");
    };
    assert_eq!(title, "Select permission mode");
    assert_eq!(*selected, 0);
    assert!(items.iter().any(|item| item == "Full Auto"));

    app.dispatch_key_for_test(key(KeyCode::Down));
    app.dispatch_key_for_test(key(KeyCode::Down));
    app.dispatch_key_for_test(key(KeyCode::Down));
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(matches!(app.active_modal, ActiveModal::None));
    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .map(String::as_str),
        Some("bypassPermissions")
    );
    assert!(
        app.messages.iter().any(|message| message
            .content
            .contains("Permission mode set to: Full Auto")),
        "permission selection should leave visible feedback in the transcript"
    );
    let rendered = render_app_text(&mut app, 120, 18);
    assert!(rendered.contains("Full Auto"), "{rendered}");
}

#[test]
fn permissions_slash_rule_subcommands_reach_registry() {
    let mut app = App::new();
    app.directives = Some(std::sync::Arc::new(mossen_commands::all_directives()));

    app.prompt.input.insert_str("/permissions allow Bash");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(matches!(app.active_modal, ActiveModal::None));
    assert!(
        app.messages.iter().any(|message| {
            message.content.contains("/permissions")
                && message.content.contains("Added allow rule for: Bash")
        }),
        "/permissions allow should execute the registry directive instead of opening the mode picker"
    );
    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_ALLOW_RULES")
            .map(String::as_str),
        Some("Bash")
    );

    app.prompt.input.insert_str("/permissions list");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(
        app.messages.iter().any(|message| {
            message.content.contains("/permissions")
                && message.content.contains("Permission rules:")
                && message.content.contains("Allow:")
                && message.content.contains("Bash")
        }),
        "/permissions list should execute the registry directive"
    );

    app.prompt.input.insert_str("/permissions deny Write");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_DENY_RULES")
            .map(String::as_str),
        Some("Write")
    );

    app.prompt.input.insert_str("/permissions reset");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(!app
        .command_context
        .env_vars
        .contains_key("MOSSEN_PERMISSION_ALLOW_RULES"));
    assert!(!app
        .command_context
        .env_vars
        .contains_key("MOSSEN_PERMISSION_DENY_RULES"));
}

#[test]
fn permissions_slash_accepts_direct_mode_arguments() {
    let mut app = App::new();

    app.prompt.input.insert_str("/permissions full-auto");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .map(String::as_str),
        Some("bypassPermissions")
    );

    app.prompt.input.insert_str("/permissions mode dont-ask");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .map(String::as_str),
        Some("dontAsk")
    );

    app.prompt.input.insert_str("/permission-mode accept-edits");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .map(String::as_str),
        Some("acceptEdits")
    );
}

#[test]
fn selected_permission_mode_flows_into_next_engine_request() {
    let mut app = App::new();

    app.prompt.input.insert_str("/permissions");
    app.dispatch_key_for_test(key(KeyCode::Enter));
    for _ in 0..4 {
        app.dispatch_key_for_test(key(KeyCode::Down));
    }
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert_eq!(
        app.command_context
            .env_vars
            .get("MOSSEN_PERMISSION_MODE")
            .map(String::as_str),
        Some("dontAsk")
    );

    app.prompt.input.insert_str("try a gated tool");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    let params = app
        .pending_submit
        .as_ref()
        .expect("user prompt should create engine params");
    assert_eq!(params.permission_mode, PermissionMode::DontAsk);
}

#[test]
fn mcp_channel_approval_enter_allows_session() {
    let mut app = App::new();
    let id = "test-channel-approval-enter".to_string();
    app.active_modal = ActiveModal::McpChannelApproval(
        mossen_agent::mcp::channel_approval::ChannelApprovalRequest {
            id: id.clone(),
            server_name: "phone".to_string(),
            plugin: Some("phone".to_string()),
            marketplace: Some("local".to_string()),
            reason: "not allowlisted".to_string(),
        },
    );

    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(matches!(app.active_modal, ActiveModal::None));
    assert!(mossen_agent::mcp::channel_approval::is_allowed(&id));
}

#[test]
fn permission_prompt_renders_inline_and_marks_footer() {
    let mut app = App::new();
    mossen_tui::app::__debug_open_permission_modal(&mut app);

    let rendered = render_app_text(&mut app, 100, 30);

    assert!(
        rendered.contains("Tool: DebugTool"),
        "permission prompt should be visible in the rendered frame"
    );
    assert!(
        rendered.contains("approval required"),
        "footer should make a pending approval visible"
    );
    let tool_line = line_index_containing(&rendered, "Tool: DebugTool")
        .expect("permission prompt tool line should render");
    assert!(
        tool_line >= 16,
        "permission prompt should sit below the transcript, not as a centered modal; got line {tool_line}\n{rendered}"
    );
}

#[test]
fn dynamic_skill_discovery_notifies_and_updates_slash_catalog() {
    let registry = mossen_skills::new_shared_registry();
    let mut app = App::new().with_skill_registry(registry.clone());

    assert!(!app
        .state
        .all_slash_commands
        .iter()
        .any(|entry| entry.name == "dynamic-smoke"));

    registry
        .write()
        .unwrap()
        .add_dynamic_crafts(vec![test_skill("dynamic-smoke")]);
    app.dispatch_tick_for_test();

    assert!(
        app.state
            .all_slash_commands
            .iter()
            .any(|entry| entry.name == "dynamic-smoke"),
        "newly discovered skills should enter slash typeahead"
    );
    assert!(
        app.messages
            .iter()
            .any(|message| message.content.contains("Skill discovered: /dynamic-smoke")),
        "newly discovered skills should leave visible feedback"
    );
}

#[test]
fn slash_skill_submission_includes_command_tags_but_transcript_stays_clean() {
    let registry = mossen_skills::new_shared_registry();
    registry
        .write()
        .unwrap()
        .add_dynamic_crafts(vec![test_skill("dynamic-smoke")]);
    let mut app = App::new().with_skill_registry(registry);

    app.prompt.input.insert_str("/dynamic-smoke with args");
    app.dispatch_key_for_test(key(KeyCode::Enter));

    assert!(app.messages.iter().any(|message| message.message_type
        == MessageType::SkillInvocation
        && message.content.contains("/dynamic-smoke")
        && !message.content.contains("<command-name>")));
    let pending = app
        .pending_submit
        .as_ref()
        .expect("skill invocation should submit prompt to engine");
    assert!(pending
        .prompt
        .contains("<command-name>/dynamic-smoke</command-name>"));
    assert!(pending
        .prompt
        .contains("<command-args>with args</command-args>"));
    assert!(pending.prompt.contains("Use this test skill."));
}

#[test]
fn mcp_tool_names_render_with_server_source() {
    assert_eq!(
        display_tool_name("mcp__filesystem__read_file"),
        "[filesystem] read_file"
    );
    assert_eq!(
        display_tool_name("github__create_issue"),
        "[github] create_issue"
    );
    assert_eq!(display_tool_name("Bash"), "Bash");
}
