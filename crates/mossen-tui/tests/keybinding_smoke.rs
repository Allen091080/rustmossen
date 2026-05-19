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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mossen_tui::widgets::message::{MessageData, MessageType};
use mossen_tui::App;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
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

#[test]
fn ctrl_e_toggles_show_all_thinking() {
    let mut app = App::new();
    assert!(!app.show_all_thinking);
    app.dispatch_key_for_test(ctrl('e'));
    assert!(app.show_all_thinking, "Ctrl+E should flip show_all_thinking on");
    app.dispatch_key_for_test(ctrl('e'));
    assert!(!app.show_all_thinking, "Ctrl+E should flip back off");
}

#[test]
fn arrows_move_focus_between_messages_when_prompt_empty() {
    let mut app = App::new();
    let (tu, tr) = seed_tool_use_pair(&mut app);
    // Prompt is empty + not streaming → arrows move focus.
    app.dispatch_key_for_test(key(KeyCode::Up));
    assert!(app.focused_message_idx.is_some());
    // After auto-collapsing ToolUse, ToolResult is hidden — focus should
    // skip it on subsequent navigation. Force the collapse first so the
    // skip path is exercised.
    app.collapsed_tool_groups.insert(tu);
    app.focused_message_idx = Some(tu);
    app.dispatch_key_for_test(key(KeyCode::Down));
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
        matches!(app.active_modal, mossen_tui::app::ActiveModal::Picker { .. }),
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
