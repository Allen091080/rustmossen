use std::collections::HashSet;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mossen_agent::types::{ContentDelta, SdkMessage, StreamEventData};
use mossen_tools::todo::TodoItem;
use mossen_tui::app::HelpDialogState;
use mossen_tui::app_services::SearchPanelState;
use mossen_tui::approval_state::{PermissionKind, PermissionPromptState};
use mossen_tui::message_model::{MessageData, MessageType};
use mossen_tui::render_cache::RenderHeightCache;
use mossen_tui::render_glyphs::RenderGlyphs;
use mossen_tui::render_model::{
    approval_decision_message_content, final_summary_message_content, ApprovalDecisionKind,
    ApprovalDecisionModel, CommandSummaryModel, FileChangeSummaryModel, FinalSummaryModel,
    FooterItem, RenderNode, RenderTranscript, ToolPhase, VerificationSummaryModel,
};
use mossen_tui::state::{
    McpConnectionState, McpServerStatus, RenderActivity, SlashCommandInfo, SlashCommandKind,
    TeammateState, TurnState, UiStage,
};
use mossen_tui::theme::Theme;
use mossen_tui::widgets::messages::MessagesWidget;
use mossen_tui::widgets::prompt_input::{Suggestion, SuggestionKind};
use mossen_tui::{ActiveModal, App};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const PRODUCT_SIZES: &[(u16, u16)] = &[(32, 8), (40, 10), (48, 14), (72, 18), (100, 24), (132, 36)];

const PRODUCT_FORBIDDEN: &[&str] = &[
    "Render error",
    "panicked",
    "thread 'main'",
    "index outside of buffer",
    "RUST_BACKTRACE",
    "terminal=Completed",
    "(stop: tool_use)",
    "stop: tool_use",
    "raw_json",
    "\"stdout\"",
    "\"stderr\"",
    "old_todos",
    "new_todos",
    "mossen-render:approval-decision",
    "mossen-render:final-summary",
    "\u{1b}",
    "\u{7}",
    "\u{8}",
    "\u{0c}",
    "[31m",
    "[0m",
    "Dialog.tsx",
    "App.tsx",
    "REPL.tsx",
];

struct Scenario {
    name: &'static str,
    build: fn() -> App,
    semantic_needles: &'static [&'static str],
}

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

fn render_app(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| app.render_for_test(frame))
        .expect("app frame should draw");
    buffer_to_string(terminal.backend().buffer(), width, height)
}

fn mouse_at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn scrollbar_rail_rows(rendered: &str, width: usize) -> Vec<usize> {
    let rows = rendered
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            matches!(
                line.chars().nth(width.saturating_sub(1)),
                Some('#') | Some('|') | Some('┃') | Some('│')
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();

    let mut best: &[usize] = &[];
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        while end < rows.len() && rows[end] == rows[end - 1] + 1 {
            end += 1;
        }
        if end - start > best.len() {
            best = &rows[start..end];
        }
        start = end;
    }
    best.to_vec()
}

fn stream_event(event: StreamEventData) -> SdkMessage {
    SdkMessage::StreamEvent {
        event,
        task_id: None,
    }
}

fn buffer_to_string(buf: &ratatui::buffer::Buffer, width: u16, height: u16) -> String {
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

fn assert_product_contract(name: &str, rendered: &str) {
    assert!(
        rendered.lines().any(|line| !line.trim().is_empty()),
        "{name} rendered a blank frame\n--- frame ---\n{rendered}"
    );
    for ch in rendered.chars().filter(|ch| *ch != '\n') {
        assert!(
            !ch.is_control(),
            "{name} leaked terminal control character {ch:?}\n--- frame ---\n{rendered}"
        );
    }
    let normalized = normalize_cjk_cell_spacing(rendered);
    for needle in PRODUCT_FORBIDDEN {
        assert!(
            !rendered.contains(needle) && !normalized.contains(needle),
            "{name} leaked forbidden product render text {needle:?}\n--- frame ---\n{rendered}"
        );
    }
}

fn assert_semantic_contract(name: &str, rendered: &str) {
    for needle in [
        "terminal=Completed",
        "(stop: tool_use)",
        "stop: tool_use",
        "raw_json",
        "old_todos",
        "new_todos",
        "\u{1b}",
        "[31m",
        "[0m",
    ] {
        assert!(
            !rendered.contains(needle),
            "{name} leaked forbidden semantic text {needle:?}\n--- model ---\n{rendered}"
        );
    }
}

fn assert_contains_all(name: &str, rendered: &str, needles: &[&str]) {
    let normalized = normalize_cjk_cell_spacing(rendered);
    for needle in needles {
        assert!(
            rendered.contains(needle) || normalized.contains(needle),
            "{name} missed semantic anchor {needle:?}\n--- frame ---\n{rendered}"
        );
    }
}

fn assert_contains_any(name: &str, rendered: &str, needles: &[&str]) {
    let normalized = normalize_cjk_cell_spacing(rendered);
    assert!(
        needles
            .iter()
            .any(|needle| rendered.contains(needle) || normalized.contains(needle)),
        "{name} missed all semantic anchors {needles:?}\n--- frame ---\n{rendered}"
    );
}

fn first_line_index(rendered: &str, needle: &str) -> Option<usize> {
    let normalized_needle = normalize_cjk_cell_spacing(needle);
    rendered.lines().position(|line| {
        line.contains(needle) || normalize_cjk_cell_spacing(line).contains(&normalized_needle)
    })
}

fn last_line_index(rendered: &str, needles: &[&str]) -> Option<usize> {
    let normalized_needles: Vec<String> = needles
        .iter()
        .map(|needle| normalize_cjk_cell_spacing(needle))
        .collect();
    let lines: Vec<&str> = rendered.lines().collect();
    lines.iter().enumerate().rev().find_map(|(index, line)| {
        let normalized = normalize_cjk_cell_spacing(line);
        if needles
            .iter()
            .zip(normalized_needles.iter())
            .any(|(needle, normalized_needle)| {
                (*line).contains(needle) || normalized.contains(normalized_needle)
            })
        {
            Some(index)
        } else {
            None
        }
    })
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

fn product_messages() -> Vec<MessageData> {
    let bash_result = serde_json::json!({
        "stdout": "\u{1b}[31mANSI 必须清理\u{1b}[0m\nCargo.toml\ncrates\nphases\n逐行阅读输出\n",
        "stderr": "",
        "exit_code": 0
    })
    .to_string();
    let todo_result = serde_json::json!({
        "old_todos": [],
        "new_todos": [
            {
                "id": "render-contract",
                "content": "产品级渲染合同必须压住 raw 协议输出",
                "status": "in_progress"
            }
        ]
    })
    .to_string();

    vec![
        msg(MessageType::User, "请逐行阅读当前项目，不要只扫目录结构。"),
        msg(
            MessageType::Assistant,
            "## 真实分析\n\n- 从入口开始读\n- 再看渲染链路\n\n```rust\nfn main() {\n    println!(\"mossen\");\n}\n```\n\n| 层 | 职责 |\n| --- | --- |\n| L1 | 语义清洗 |\n| L3 | 终端布局 |",
        ),
        msg(
            MessageType::Assistant,
            "(no content - terminal=Completed)\n\n(stop: tool_use)",
        ),
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            "command  ls -la\ncwd      /Users/allen/Documents/rustmossen",
        ),
        tool_msg(MessageType::ToolResult, "Bash", bash_result),
        tool_msg(MessageType::ToolUse, "Glob", "null"),
        tool_msg(MessageType::ToolResult, "Glob", "crates/mossen-tui/src/app.rs"),
        tool_msg(
            MessageType::ToolUse,
            "TodoWrite",
            serde_json::json!({
                "todos": [
                    {
                        "id": "render-contract",
                        "content": "产品级渲染合同必须压住 raw 协议输出",
                        "status": "in_progress"
                    }
                ]
            })
            .to_string(),
        ),
        tool_msg(MessageType::ToolResult, "TodoWrite", todo_result),
    ]
}

fn pathological_messages() -> Vec<MessageData> {
    let mut messages = product_messages();
    let long_unbroken = "render_contract_long_segment_".repeat(64);
    messages.push(msg(
        MessageType::Assistant,
        format!(
            "\u{1b}[31m### 渲染压力\u{1b}[0m\t中文 mixed e\u{301}\n\n- ANSI 不应进入语义层\n- 长行必须可折叠或裁剪\n\n```rust\nlet path = \"{long_unbroken}\";\n```\n\n| case | expectation |\n| --- | --- |\n| resize | stable |\n| cjk | no byte panic |"
        ),
    ));
    messages.push(tool_msg(
        MessageType::ToolUse,
        "Read",
        "{\"file_path\":\"/tmp/读.rs\"",
    ));
    messages.push(tool_msg(
        MessageType::ToolUse,
        "Bash",
        serde_json::json!({
            "command": "printf pathological-render-output",
            "cwd": "/Users/allen/Documents/rustmossen"
        })
        .to_string(),
    ));
    let mut bash_error = tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": format!("\u{1b}[32mgreen\u{1b}[0m\taligned\n{long_unbroken}\n最后一行仍然要可见\n"),
            "stderr": "\u{1b}[31mwarning\u{1b}[0m\tstderr\n",
            "exit_code": 1
        })
        .to_string(),
    );
    bash_error.is_error = true;
    messages.push(bash_error);
    messages.push(msg(
        MessageType::Assistant,
        "final-render-anchor：缩放风暴后仍应看到正常语义内容。",
    ));
    messages
}

fn scroll_contract_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.state.is_streaming = false;
    app.state.is_waiting_for_response = false;
    app.state.turn_state = TurnState::Idle;
    app.prompt.input.clear();

    let mut messages = Vec::new();
    messages.push(msg(
        MessageType::Assistant,
        "head-scroll-anchor：这是长 transcript 的第一屏内容。",
    ));
    for index in 0..80 {
        messages.push(msg(
            MessageType::Assistant,
            format!(
                "滚动合同第 {index:02} 行：中文内容、wrapped text、以及足够长的句子用于真实高度测量。"
            ),
        ));
    }
    messages.push(msg(
        MessageType::Assistant,
        "tail-scroll-anchor：这是长 transcript 的最终结论。",
    ));
    app.messages = messages;
    app
}

fn tall_single_message_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();
    app.state.is_streaming = false;
    app.state.is_waiting_for_response = false;
    app.state.turn_state = TurnState::Idle;

    let mut body = String::from("## tall-single-message-head-anchor\n\n");
    for index in 0..1_300 {
        body.push_str(&format!(
            "- tall-single-row-{index:04}：单条模型回复也必须支持真实虚拟滚动，不能只裁剪 scratch buffer。\n"
        ));
    }
    body.push_str("\ntall-single-message-tail-anchor：单条超长回复真实尾部。");
    app.messages = vec![msg(MessageType::Assistant, body)];
    app
}

fn enormous_single_message_app() -> App {
    let mut app = tall_single_message_app();
    let mut body = String::from("## enormous-single-message-head-anchor\n\n");
    for index in 0..66_000 {
        body.push_str(&format!(
            "- enormous-single-row-{index:05}: virtual scroll must not cap a single assistant block at u16::MAX.\n"
        ));
    }
    body.push_str("\nenormous-single-message-tail-anchor: real tail beyond 65535 rows.");
    app.messages = vec![msg(MessageType::Assistant, body)];
    app
}

fn tall_rich_markdown_app() -> App {
    let mut app = tall_single_message_app();
    let mut body = String::from("## rich-virtual-head-anchor\n\n");
    for index in 0..1_100 {
        body.push_str(&format!(
            "- rich-virtual-row-{index:04}：Markdown 列表、中文宽字符、inline `code_{index}` 和足够长的文字必须在 deep scroll fallback 中保持高度一致。\n"
        ));
        if index % 220 == 0 {
            body.push_str(&format!(
                "\n```rust\nfn rich_virtual_code_{index}() {{\n    println!(\"semantic markdown virtualization\");\n}}\n```\n\n"
            ));
        }
    }
    body.push_str(
        "\n```rust\nfn rich_virtual_tail_code() {\n    println!(\"rich-virtual-code-anchor\");\n}\n```\n\n",
    );
    body.push_str(
        "| Layer | Contract |\n| --- | --- |\n| L2 | semantic markdown |\n| L3 | virtual rows |\n\n",
    );
    body.push_str("rich-virtual-tail-anchor：超长富文本回复真实尾部。");
    app.messages = vec![msg(MessageType::Assistant, body)];
    app
}

fn fuzz_contract_app(seed: usize) -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();

    let mut messages = Vec::new();
    for index in 0..24 {
        let token = format!("seed-{seed}-case-{index}");
        match index % 6 {
            0 => messages.push(msg(
                MessageType::Assistant,
                format!(
                    "\u{1b}[31m### fuzz {token}\u{1b}[0m\t中文 e\u{301}\n\n```text\n{}\n```",
                    "long_unbroken_segment_".repeat(12)
                ),
            )),
            1 => messages.push(tool_msg(
                MessageType::ToolUse,
                "Bash",
                serde_json::json!({
                    "command": format!("printf {token}"),
                    "cwd": "/Users/allen/Documents/rustmossen"
                })
                .to_string(),
            )),
            2 => messages.push(tool_msg(
                MessageType::ToolResult,
                "Bash",
                serde_json::json!({
                    "stdout": format!("\u{1b}[32m{token}\u{1b}[0m\tok\n"),
                    "stderr": "",
                    "exit_code": 0
                })
                .to_string(),
            )),
            3 => messages.push(tool_msg(
                MessageType::ToolUse,
                "Read",
                format!("{{\"file_path\":\"/tmp/{token}.rs\""),
            )),
            4 => messages.push(msg(
                MessageType::User,
                format!("用户输入 {token}：需要验证宽字符、标点，不能 panic。"),
            )),
            _ => messages.push(tool_msg(
                MessageType::ToolResult,
                "Glob",
                serde_json::json!({
                    "files": [
                        format!("crates/mossen-tui/src/{token}.rs"),
                        "crates/mossen-tui/src/app.rs"
                    ]
                })
                .to_string(),
            )),
        }
    }
    messages.push(msg(
        MessageType::Assistant,
        format!("fuzz-tail-anchor-{seed}：语义 fuzz 合同结束。"),
    ));
    app.messages = messages;
    app
}

#[derive(Debug, Clone, Copy)]
struct RenderFuzzRng {
    state: u64,
}

impl RenderFuzzRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 7;
        x ^= x >> 9;
        x ^= x << 8;
        self.state = x;
        x
    }

    fn range(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        (self.next_u64() as usize) % upper
    }

    fn coin(&mut self) -> bool {
        self.range(2) == 0
    }
}

fn generated_fuzz_tool_name(rng: &mut RenderFuzzRng) -> &'static str {
    const TOOLS: &[&str] = &[
        "Bash",
        "Read",
        "Grep",
        "Glob",
        "Edit",
        "TodoWrite",
        "Task",
        "mcp__filesystem__read_file",
        "ThirdPartyProvider",
    ];
    TOOLS[rng.range(TOOLS.len())]
}

fn generated_fuzz_visible_text(seed: usize, index: usize, rng: &mut RenderFuzzRng) -> String {
    let mut text = format!("property-fuzz-visible-{seed}-{index} 中文 e\u{301}");
    if rng.coin() {
        text.push_str("\tindent");
    }
    if rng.coin() {
        text.push_str("\u{1b}[35m ansi-color \u{1b}[0m");
    }
    if rng.coin() {
        text.push_str("\u{7}\u{8}\u{0c}");
    }
    let repeat = 1 + rng.range(8);
    text.push(' ');
    text.push_str(&"long_unbroken_property_segment_".repeat(repeat));
    text
}

fn generated_fuzz_payload(seed: usize, index: usize, rng: &mut RenderFuzzRng) -> String {
    let visible = generated_fuzz_visible_text(seed, index, rng);
    match rng.range(7) {
        0 => serde_json::json!({
            "command": format!("printf property-fuzz-visible-{seed}-{index}"),
            "cwd": "/Users/allen/Documents/rustmossen",
            "authorization": "Bearer property-fuzz-secret",
            "token_count": 32 + index
        })
        .to_string(),
        1 => serde_json::json!({
            "pattern": format!("property-fuzz-visible-{seed}-{index}"),
            "path": "crates/mossen-tui/src",
            "session_token": "property-fuzz-secret",
            "total_tokens": 64 + index
        })
        .to_string(),
        2 => serde_json::json!({
            "stdout": visible,
            "stderr": "\u{1b}[31mproperty-fuzz-visible-stderr\u{1b}[0m",
            "exit_code": if rng.coin() { 0 } else { 1 },
            "private_key": "property-fuzz-secret"
        })
        .to_string(),
        3 => serde_json::json!({
            "todos": [
                { "content": format!("property-fuzz-visible-{seed}-{index} todo"), "status": "in_progress" },
                { "content": "property-fuzz-visible-done", "status": "completed" }
            ],
            "credentials": "property-fuzz-secret"
        })
        .to_string(),
        4 => serde_json::json!({
            "nested": {
                "visible": visible,
                "items": [
                    { "name": format!("property-fuzz-visible-item-{seed}-{index}"), "accessToken": "property-fuzz-secret" }
                ]
            },
            "remaining_tokens": 4096
        })
        .to_string(),
        5 => format!(
            "{{\"query\":\"property-fuzz-visible-{seed}-{index}\",\"unterminated\":[1,2,"
        ),
        _ => visible,
    }
}

fn generated_property_fuzz_app(seed: usize) -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();

    let mut rng = RenderFuzzRng::new(seed as u64);
    let mut messages = Vec::new();
    messages.push(msg(
        MessageType::User,
        format!("property-fuzz-head-anchor-{seed}：生成式渲染合同开始。"),
    ));

    for index in 0..72 {
        match rng.range(8) {
            0 => messages.push(msg(
                MessageType::Assistant,
                generated_fuzz_visible_text(seed, index, &mut rng),
            )),
            1 => messages.push(msg(
                MessageType::System,
                generated_fuzz_visible_text(seed, index, &mut rng),
            )),
            2 => messages.push(msg(
                MessageType::Progress,
                generated_fuzz_visible_text(seed, index, &mut rng),
            )),
            3 => messages.push(msg(
                MessageType::CommandOutput,
                generated_fuzz_visible_text(seed, index, &mut rng),
            )),
            4 => messages.push(tool_msg(
                MessageType::ToolUse,
                generated_fuzz_tool_name(&mut rng),
                generated_fuzz_payload(seed, index, &mut rng),
            )),
            5 => messages.push(tool_msg(
                MessageType::ToolResult,
                generated_fuzz_tool_name(&mut rng),
                generated_fuzz_payload(seed, index, &mut rng),
            )),
            6 => {
                let mut data = msg(
                    MessageType::Assistant,
                    format!(
                        "<think>{}</think>\nproperty-fuzz-visible-after-thinking-{seed}-{index}",
                        generated_fuzz_visible_text(seed, index, &mut rng)
                    ),
                );
                data.is_streaming = rng.coin();
                messages.push(data);
            }
            _ => messages.push(msg(
                MessageType::Attachment,
                format!(
                    "property-fuzz-visible-attachment-{seed}-{index}: {}",
                    "wide_payload_".repeat(1 + rng.range(12))
                ),
            )),
        }
    }

    messages.push(msg(
        MessageType::Assistant,
        format!("property-fuzz-tail-anchor-{seed}：生成式渲染合同结束。"),
    ));
    app.messages = messages;
    app
}

fn assert_scroll_state_bounded(name: &str, app: &App) {
    let max_offset = app
        .scroll
        .total_items
        .saturating_sub(app.scroll.visible_count);
    assert!(
        app.scroll.offset <= max_offset,
        "{name} left transcript scroll out of bounds: offset={} max={} total={} visible={}",
        app.scroll.offset,
        max_offset,
        app.scroll.total_items,
        app.scroll.visible_count
    );
}

fn streaming_soak_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();
    app.messages = vec![msg(
        MessageType::User,
        "启动虚拟 streaming soak：大量 token、resize、手动滚动和恢复底部必须稳定。",
    )];
    app.handle_engine_message(stream_event(StreamEventData::MessageStart));
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "## streaming-soak-head-anchor\n\n".to_string(),
        },
    }));
    app
}

fn push_streaming_soak_delta(app: &mut App, index: usize) {
    let mut text = format!(
        "- streaming-soak-row-{index:04}: virtual long-running stream keeps the renderer paced and scroll-safe."
    );
    if index % 37 == 0 {
        text.push_str(" 中文 e\u{301} \t ansi \u{1b}[33mcolor\u{1b}[0m controls \u{7}\u{8}");
    }
    if index % 113 == 0 {
        text.push_str(" long_unbroken_streaming_soak_segment_");
        text.push_str(&"x".repeat(160));
    }
    text.push('\n');
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta { text },
    }));
}

fn arbitrary_tool_payload_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;
    app.messages = vec![
        msg(MessageType::User, "任意工具 payload 渲染合同"),
        tool_msg(
            MessageType::ToolUse,
            "ThirdPartyTool",
            serde_json::json!({
                "query": "arbitrary-tool-visible-query\u{7}",
                "authorization_header": "Bearer raw-use-auth-secret",
                "client_secret": "raw-use-client-secret",
                "token_count": 64
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "CustomProvider",
            serde_json::json!({
                "status": "ok\u{8}",
                "session_token": "raw-result-session-token",
                "private_key": "raw-result-private-key",
                "nested": {
                    "visible": "\u{1b}[31marbitrary-tool-visible-nested\u{1b}[0m",
                    "accessToken": "raw-result-access-token"
                },
                "items": [
                    {
                        "name": "arbitrary-tool-visible-item",
                        "password": "raw-result-password"
                    }
                ],
                "token_count": 128,
                "total_tokens": 256
            })
            .to_string(),
        ),
        msg(
            MessageType::Assistant,
            "arbitrary-tool-anchor：任意工具语义渲染结束。",
        ),
    ];
    app
}

fn large_session_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();
    app.state.is_streaming = false;
    app.state.is_waiting_for_response = false;
    app.state.turn_state = TurnState::Idle;

    let mut messages = Vec::new();
    for index in 0..900 {
        match index % 9 {
            0 => messages.push(tool_msg(
                MessageType::ToolUse,
                "Bash",
                serde_json::json!({
                    "command": format!("echo large-session-{index}"),
                    "cwd": "/Users/allen/Documents/rustmossen"
                })
                .to_string(),
            )),
            1 => messages.push(tool_msg(
                MessageType::ToolResult,
                "Bash",
                serde_json::json!({
                    "stdout": format!("large-session-{index}\n{}\n", "bounded output ".repeat(18)),
                    "stderr": "",
                    "exit_code": 0
                })
                .to_string(),
            )),
            _ => messages.push(msg(
                MessageType::Assistant,
                format!(
                    "large-session-row-{index:04}：长会话渲染预算合同，包含中文和 wrapped text。"
                ),
            )),
        }
    }
    messages.push(msg(
        MessageType::Assistant,
        "large-session-tail-anchor：长会话最终内容。",
    ));
    app.messages = messages;
    app
}

fn rich_content_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();
    app.state.is_streaming = false;
    app.state.is_waiting_for_response = false;
    app.state.turn_state = TurnState::Idle;

    let diff_output = "\
diff --git a/crates/mossen-tui/src/render_model.rs b/crates/mossen-tui/src/render_model.rs\n\
index 1111111..2222222 100644\n\
--- a/crates/mossen-tui/src/render_model.rs\n\
+++ b/crates/mossen-tui/src/render_model.rs\n\
@@ -1,3 +1,4 @@\n\
-old render path\n\
+semantic render path\n\
+rich-content-diff-anchor\n";

    app.messages = vec![
        msg(MessageType::User, "验证富文本渲染：Markdown、代码块、表格、diff 必须走 active App 路径。"),
        msg(
            MessageType::Assistant,
            "## 富文本渲染合同\n\n- Markdown 列表必须保留层级\n- 代码块必须作为代码块呈现\n\n```rust\nfn rich_contract() {\n    println!(\"semantic render\");\n}\n```\n\n| Layer | Responsibility |\n| --- | --- |\n| L1 | events |\n| L2 | semantics |\n| L3 | terminal |\n\nrich-markdown-tail-anchor",
        ),
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            serde_json::json!({
                "command": "git diff -- crates/mossen-tui/src/render_model.rs",
                "cwd": "/Users/allen/Documents/rustmossen"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": diff_output,
                "stderr": "",
                "exit_code": 0
            })
            .to_string(),
        ),
        msg(
            MessageType::Assistant,
            "rich-content-tail-anchor：富文本和工具 diff 之后，最终内容仍然可见。",
        ),
    ];
    app
}

fn apply_common_product_state(app: &mut App) {
    app.fullscreen = true;
    app.engine_config.model = "MiniMax-M2.7".to_string();
    app.total_cost_usd = 0.15;
    app.state.turn_state = TurnState::Streaming;
    app.state.is_streaming = true;
    app.state.is_waiting_for_response = true;
    app.prompt.input.clear();
    app.prompt
        .input
        .insert_str("继续验证复杂渲染矩阵，不能把协议噪声、ANSI 或 panic 打到用户主 transcript 里");
    app.prompt.input.move_end();
    app.prompt.show_suggestions = true;
    app.prompt.suggestions = vec![
        Suggestion {
            label: "/plan".to_string(),
            description: Some("Create an execution plan".to_string()),
            kind: SuggestionKind::Command,
        },
        Suggestion {
            label: "/mcp".to_string(),
            description: Some("Inspect MCP server status".to_string()),
            kind: SuggestionKind::Command,
        },
    ];
    app.prompt.selected_suggestion = Some(0);
    app.state.task_list.tasks = vec![TodoItem {
        id: "render-contract".to_string(),
        content: "完整渲染合同：真实 App 路径 resize/approval/footer/transcript 全部要过"
            .to_string(),
        status: "in_progress".to_string(),
    }];
    app.state
        .teammate_states
        .insert("逐行阅读子 agent".to_string(), TeammateState::Running);
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
}

fn complex_streaming_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();

    let mut approval = PermissionPromptState::new(
        PermissionKind::Shell {
            command: "cargo test -p mossen-tui render_contract".to_string(),
        },
        "Bash",
    );
    approval.explanation =
        Some("审批应跟随在当前回复下方，不能盖住 transcript 或底部输入。".to_string());
    approval.show_details = true;
    app.active_modal = ActiveModal::PermissionRequest(approval);

    app
}

fn help_dialog_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();
    app.active_modal = ActiveModal::HelpDialog(HelpDialogState::default());
    app.state.all_slash_commands = vec![
        SlashCommandInfo {
            name: "plan".to_string(),
            description: "Create a structured execution plan".to_string(),
            category: "System".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Command,
        },
        SlashCommandInfo {
            name: "项目分析技能".to_string(),
            description: "逐行阅读当前项目并输出真实缺口".to_string(),
            category: "Skills".to_string(),
            aliases: Vec::new(),
            argument_hint: String::new(),
            kind: SlashCommandKind::Skill,
        },
    ];
    app
}

fn mcp_dialog_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();
    app.active_modal = ActiveModal::McpServersDialog;
    app
}

fn tasks_dialog_app() -> App {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();
    app.active_modal = ActiveModal::TasksDialog;
    app
}

fn product_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "complex streaming approval frame",
            build: complex_streaming_app,
            semantic_needles: &["Bash", "Shell Command", "Waiting for approval"],
        },
        Scenario {
            name: "help dialog frame",
            build: help_dialog_app,
            semantic_needles: &["Mossen Help", "/plan", "/项目分析技能"],
        },
        Scenario {
            name: "mcp dialog frame",
            build: mcp_dialog_app,
            semantic_needles: &["MCP Servers", "connected", "本地文件系统服务"],
        },
        Scenario {
            name: "tasks dialog frame",
            build: tasks_dialog_app,
            semantic_needles: &["TodoWrite tasks", "Background agents", "running"],
        },
    ]
}

#[test]
fn app_render_contract_survives_product_state_matrix() {
    for scenario in product_scenarios() {
        for &(width, height) in PRODUCT_SIZES {
            let mut app = (scenario.build)();
            let rendered = render_app(&mut app, width, height);
            let name = format!("{} at {}x{}", scenario.name, width, height);

            assert_product_contract(&name, &rendered);
            if width >= 72 && height >= 18 {
                assert_contains_all(&name, &rendered, scenario.semantic_needles);
            } else if width >= 40 && height >= 10 {
                let mut fallback_needles = scenario.semantic_needles.to_vec();
                fallback_needles.extend(["mossen", "MiniMax-M2.7", "msgs", "Ask anything"]);
                assert_contains_any(&name, &rendered, &fallback_needles);
            }
        }
    }
}

#[test]
fn app_render_contract_search_modal_uses_semantic_previews_not_raw_payloads() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![
        tool_msg(
            MessageType::ToolUse,
            "Bash",
            serde_json::json!({
                "command": "printf semantic-search-anchor",
                "cwd": "/Users/allen/Documents/rustmossen"
            })
            .to_string(),
        ),
        tool_msg(
            MessageType::ToolResult,
            "Bash",
            serde_json::json!({
                "stdout": "semantic-search-anchor\nok\n",
                "stderr": "",
                "exit_code": 0,
                "file_path": "raw-search-preview-should-not-leak"
            })
            .to_string(),
        ),
        msg(
            MessageType::Assistant,
            "(no content - terminal=Completed)\n\n(stop: tool_use)",
        ),
    ];
    let mut panel = SearchPanelState::new();
    panel.input.set_query("semantic".to_string());
    panel.matches = vec![1, 2];
    app.services.search_panel_state = Some(panel);
    app.active_modal = ActiveModal::Search("semantic".to_string());

    let rendered = render_app(&mut app, 100, 24);

    assert_product_contract("search semantic previews", &rendered);
    assert_contains_any(
        "search semantic previews",
        &rendered,
        &["Bash", "semantic-search-anchor"],
    );
    for forbidden in [
        "\"stdout\"",
        "\"stderr\"",
        "file_path",
        "raw-search-preview-should-not-leak",
        "terminal=Completed",
        "stop: tool_use",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "search modal leaked raw fallback text {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_generic_json_tool_payload_redacts_secrets() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![tool_msg(
        MessageType::ToolResult,
        "ThirdPartyTool",
        serde_json::json!({
            "status": "ok",
            "message": "generic-json-redaction-anchor",
            "api_key": "raw-api-secret",
            "nested": {
                "secret_token": "raw-nested-secret",
                "visible": "kept"
            },
            "items": [
                {
                    "password": "raw-array-secret",
                    "name": "visible-item"
                }
            ],
            "token_count": 1234
        })
        .to_string(),
    )];

    let rendered = render_app(&mut app, 112, 28);

    assert_product_contract("generic JSON tool secret redaction", &rendered);
    assert_contains_all(
        "generic JSON tool secret redaction",
        &rendered,
        &[
            "ThirdPartyTool",
            "generic-json-redaction-anchor",
            "redacted",
            "token_count",
        ],
    );
    for forbidden in ["raw-api-secret", "raw-nested-secret", "raw-array-secret"] {
        assert!(
            !rendered.contains(forbidden),
            "generic JSON tool render leaked secret value {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_raw_payloads_require_explicit_raw_view() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![
        msg(MessageType::User, "show semantic view first"),
        tool_msg(
            MessageType::ToolUse,
            "Grep",
            r#"{"raw_json":"debug-only","pattern":"render_bash_result","path":"crates/mossen-tui"}"#,
        ),
    ];

    let normal = render_app(&mut app, 100, 24);
    assert_product_contract("normal frame before raw view", &normal);
    assert_contains_all(
        "normal frame before raw view",
        &normal,
        &["Grep", "render_bash_result", "crates/mossen-tui"],
    );
    assert!(
        !normal.contains("\"pattern\""),
        "normal frame leaked JSON keys before /raw\n--- frame ---\n{normal}"
    );

    app.prompt.input.clear();
    app.prompt.input.insert_str("/raw");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));

    let raw = render_app(&mut app, 100, 24);
    assert!(
        raw.lines().any(|line| !line.trim().is_empty()),
        "raw view rendered blank\n--- frame ---\n{raw}"
    );
    for forbidden in [
        "Render error",
        "panicked",
        "thread 'main'",
        "index outside of buffer",
    ] {
        assert!(
            !raw.contains(forbidden),
            "raw view leaked render failure marker {forbidden:?}\n--- frame ---\n{raw}"
        );
    }
    assert_contains_all(
        "explicit raw view",
        &raw,
        &[
            "Raw Transcript",
            "explicit /raw debug view",
            "\"raw_json\":\"debug-only\"",
            "\"pattern\":\"render_bash_result\"",
        ],
    );
}

#[test]
fn app_render_contract_raw_modal_includes_raw_engine_event_journal() {
    let mut app = App::new();
    app.fullscreen = true;

    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "raw-contract-session".to_string(),
        model: "MiniMax-M2.7".to_string(),
        tools: vec!["Bash".to_string()],
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ApiRetry {
        error: "rate limited".to_string(),
        attempt: 1,
        max_retries: 2,
        retry_in_ms: 500,
        task_id: None,
    });

    app.prompt.input.clear();
    app.prompt.input.insert_str("/raw");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RawTranscript(_)));

    let raw = render_app(&mut app, 130, 30);
    assert_product_contract("raw event journal modal", &raw);
    assert_contains_all(
        "raw event journal modal",
        &raw,
        &[
            "Raw Transcript",
            "raw_events=2",
            "snapshot version=1",
            "session=raw-contract-session",
            "relations roots=1 parented=0 parents=0 orphans=0",
            "engine events",
            "event 1 turn=turn-0001 scope=main kind=system_init",
            "\"type\":\"system_init\"",
            "event 2 turn=turn-0001 scope=main kind=api_retry",
            "summary=attempt=1/2",
            "\"type\":\"api_retry\"",
        ],
    );
}

#[test]
fn app_render_contract_timeline_modal_uses_structured_render_events() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(MessageType::User, "检查渲染事件时间线")];
    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "timeline-contract-session".to_string(),
        model: "MiniMax-M2.7".to_string(),
        tools: Vec::new(),
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "Bash".to_string(),
        tool_use_id: Some("toolu-timeline-contract".to_string()),
        summary: serde_json::json!({
            "stdout": "timeline contract\nok\n",
            "exit_code": 0,
            "duration_ms": 250
        })
        .to_string(),
        full_content: None,
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::Result {
        terminal: "Completed".to_string(),
        cost_usd: None,
        duration_ms: Some(250),
        usage: None,
        task_id: None,
    });

    app.prompt.input.clear();
    app.prompt.input.insert_str("/events");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RenderTimeline(_)));

    let rendered = render_app(&mut app, 112, 28);
    assert_product_contract("structured render timeline modal", &rendered);
    assert_contains_all(
        "structured render timeline modal",
        &rendered,
        &[
            "Render Timeline",
            "events:",
            "turns:",
            "turn-0001",
            "command_output",
            "command_finish",
            "final_summary",
            "turn:",
            "stage:",
            "scope: main",
            "refresh:",
            "history:",
            "exit 0",
        ],
    );
    for forbidden in ["\"stdout\"", "\"exit_code\"", "\"duration_ms\""] {
        assert!(
            !rendered.contains(forbidden),
            "timeline modal leaked raw command payload key {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_timeline_modal_shows_plan_progress_counts() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(MessageType::User, "检查计划事件时间线")];
    app.handle_engine_message(SdkMessage::SystemInit {
        session_id: "timeline-plan-contract-session".to_string(),
        model: "MiniMax-M2.7".to_string(),
        tools: Vec::new(),
        task_id: None,
    });
    app.handle_engine_message(SdkMessage::ToolUseSummary {
        tool_name: "TodoWrite".to_string(),
        tool_use_id: Some("toolu-plan-contract".to_string()),
        summary: serde_json::json!({
            "old_todos": [],
            "new_todos": [
                {"id": "1", "content": "Read render timeline", "status": "completed"},
                {"id": "2", "content": "Plan progress", "status": "in_progress"},
                {"id": "3", "content": "Check narrow terminal fallback", "status": "pending"},
                {"id": "4", "content": "Resolve blocked render contract", "status": "blocked"}
            ]
        })
        .to_string(),
        full_content: None,
        task_id: None,
    });
    app.messages = vec![msg(MessageType::User, "检查计划事件时间线")];

    app.prompt.input.clear();
    app.prompt.input.insert_str("/timeline");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::RenderTimeline(_)));

    let rendered = render_app(&mut app, 150, 30);
    assert_product_contract("structured plan timeline modal", &rendered);
    assert_contains_all(
        "structured plan timeline modal",
        &rendered,
        &[
            "Render Timeline",
            "plan_updated",
            "plan updated: 4 step(s)",
            "1 done",
            "1 active",
            "1 pending",
            "1 blocked",
            "active: Plan progress",
            "detail: plan updated",
        ],
    );
    for forbidden in ["old_todos", "new_todos", "\"status\""] {
        assert!(
            !rendered.contains(forbidden),
            "timeline modal leaked raw TodoWrite payload key {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_ps_modal_uses_semantic_process_state() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(
        MessageType::Assistant,
        "Process inspector contract fixture",
    )];
    app.state.turn_state = TurnState::Streaming;
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandStarted {
            command: Some("cargo test -p mossen-tui render_contract".to_string()),
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

    app.prompt.input.clear();
    app.prompt.input.insert_str("/ps");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ProcessList(_)));

    let rendered = render_app(&mut app, 112, 28);
    assert_product_contract("semantic process status modal", &rendered);
    assert_contains_all(
        "semantic process status modal",
        &rendered,
        &[
            "Process Status",
            "running command",
            "turn: running command",
            "Command activity",
            "cargo test -p mossen-tui render_contract",
            "/ps",
            "终端渲染检查面板",
            "agent-render-review",
        ],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let moved = render_app(&mut app, 112, 28);
    assert_product_contract("semantic process status modal navigated", &moved);
    assert_contains_all(
        "semantic process status modal navigated",
        &moved,
        &["Command activity", "stage: running command"],
    );
}

#[test]
fn app_render_contract_process_and_status_show_plan_progress_counts() {
    fn plan_progress_app() -> App {
        let mut app = App::new();
        apply_common_product_state(&mut app);
        app.messages = vec![msg(MessageType::User, "检查计划进度状态")];
        app.state.turn_state = TurnState::Streaming;
        app.state.ui_stage = UiStage::Planning;
        app.state.render_activity.set(RenderActivity::Plan {
            step_count: 4,
            completed_count: 1,
            active_count: 1,
            pending_count: 1,
            blocked_count: 1,
            active_step: Some("Plan progress".to_string()),
        });
        app
    }

    let mut process_app = plan_progress_app();
    process_app.prompt.input.clear();
    process_app.prompt.input.insert_str("/ps");
    process_app.prompt.show_suggestions = false;
    process_app.prompt.selected_suggestion = None;
    process_app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        process_app.active_modal,
        ActiveModal::ProcessList(_)
    ));

    let process_rendered = render_app(&mut process_app, 132, 30);
    assert_product_contract("plan progress process status modal", &process_rendered);
    assert_contains_all(
        "plan progress process status modal",
        &process_rendered,
        &[
            "Process Status",
            "planning",
            "Plan activity",
            "plan: 4 steps",
            "1 done",
            "1 active",
            "1 pending",
            "1 blocked",
            "Plan progress",
        ],
    );

    let mut status_app = plan_progress_app();
    status_app.prompt.input.clear();
    status_app.prompt.input.insert_str("/status");
    status_app.prompt.show_suggestions = false;
    status_app.prompt.selected_suggestion = None;
    status_app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(status_app.active_modal, ActiveModal::StatusDialog));

    let status_rendered = render_app(&mut status_app, 132, 34);
    assert_product_contract("plan progress status overview modal", &status_rendered);
    assert_contains_all(
        "plan progress status overview modal",
        &status_rendered,
        &[
            "Status",
            "Turn",
            "Activity",
            "plan: 4 steps",
            "1 done",
            "1 active",
            "1 pending",
            "1 blocked",
            "Plan progress",
        ],
    );
}

#[test]
fn app_render_contract_status_modal_uses_semantic_session_state() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![
        msg(MessageType::User, "检查状态概览"),
        msg(MessageType::Assistant, "Status modal contract fixture"),
    ];
    app.engine_config.api_base_url = Some("http://localhost:8000/v1".to_string());
    app.engine_config.api_key = Some("redacted-test-key".to_string());
    app.engine_config.output_style = Some("Concise".to_string());
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.engine_session_id = Some("session-render-status".to_string());
    app.state.ui_stage = UiStage::RunningCommand;
    app.state
        .render_activity
        .set(RenderActivity::CommandStarted {
            command: Some("cargo test -p mossen-tui render_contract".to_string()),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
        });

    app.prompt.input.clear();
    app.prompt.input.insert_str("/status");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::StatusDialog));

    let rendered = render_app(&mut app, 112, 34);
    assert_product_contract("semantic status overview modal", &rendered);
    assert_contains_all(
        "semantic status overview modal",
        &rendered,
        &[
            "Status",
            "Session",
            "Turn",
            "Policy",
            "Workspace",
            "MiniMax-M2.7",
            "running command",
            "reasoning:high",
            "API Key",
            "configured",
            "Todos",
            "Agents",
            "MCP",
        ],
    );
    assert!(
        !rendered.contains("redacted-test-key"),
        "status modal must summarize API key state without exposing the key\n--- frame ---\n{rendered}"
    );
}

#[test]
fn app_render_contract_debug_config_modal_is_redacted_and_semantic() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![
        msg(MessageType::User, "检查 debug config"),
        msg(MessageType::Assistant, "Debug config fixture"),
    ];
    app.engine_config.model = "MiniMax-M2.7".to_string();
    app.engine_config.api_base_url = Some("http://localhost:8000/v1".to_string());
    app.engine_config.api_key = Some("redacted-debug-config-key".to_string());
    app.engine_config.max_turns = Some(8);
    app.engine_config.output_style = Some("Concise".to_string());
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.engine_config.extra_body.insert(
        "secret_token".to_string(),
        serde_json::json!("must-not-render"),
    );
    app.engine_session_id = Some("session-render-debug-config-123456789".to_string());
    app.state.footer_config.set_enabled(FooterItem::Cost, false);
    app.prompt.input.clear();
    app.prompt.input.insert_str("/debug-config");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::DebugConfig(_)));

    let rendered = render_app(&mut app, 118, 34);
    assert_product_contract("semantic debug config modal", &rendered);
    assert_contains_all(
        "semantic debug config modal",
        &rendered,
        &[
            "Debug Config",
            "secrets redacted",
            "Session",
            "Engine",
            "Policy",
            "Renderer",
            "MiniMax-M2.7",
            "API Key",
            "configured",
            "Extra Body",
            "secret_token",
            "redacted",
        ],
    );
    for forbidden in [
        "redacted-debug-config-key",
        "must-not-render",
        "\"secret_token\"",
        "\"api_key\"",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "debug config leaked raw or secret config value {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }

    for _ in 0..8 {
        app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    let renderer_scrolled = render_app(&mut app, 118, 34);
    assert_product_contract(
        "semantic debug config renderer diagnostics",
        &renderer_scrolled,
    );
    assert_contains_all(
        "semantic debug config renderer diagnostics",
        &renderer_scrolled,
        &["Transcript Cache", "Frame Scheduler"],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    let scrolled = render_app(&mut app, 92, 18);
    assert_product_contract("semantic debug config modal scrolled", &scrolled);
    assert_contains_all(
        "semantic debug config modal scrolled",
        &scrolled,
        &["Height Cache"],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let bottom = render_app(&mut app, 92, 18);
    assert_product_contract("semantic debug config modal bottom", &bottom);
    assert_contains_all(
        "semantic debug config modal bottom",
        &bottom,
        &["Runtime", "Transcript", "Slash Catalog"],
    );
}

#[test]
fn app_render_contract_commands_modal_uses_semantic_command_history() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(MessageType::User, "检查命令历史语义面板")];
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

    app.prompt.input.clear();
    app.prompt.input.insert_str("/commands");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::CommandHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let rendered = render_app(&mut app, 116, 30);
    assert_product_contract("semantic command history modal", &rendered);
    assert_contains_all(
        "semantic command history modal",
        &rendered,
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
    for forbidden in ["\"stdout\"", "\"stderr\"", "exit_code\":"] {
        assert!(
            !rendered.contains(forbidden),
            "command history modal leaked raw command JSON {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_errors_modal_uses_semantic_error_history() {
    let mut app = rich_content_app();
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

    app.prompt.input.clear();
    app.prompt.input.insert_str("/errors");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ErrorHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let rendered = render_app(&mut app, 116, 30);
    assert_product_contract("semantic error history modal", &rendered);
    assert_contains_all(
        "semantic error history modal",
        &rendered,
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
    for forbidden in ["\"stdout\"", "\"stderr\"", "exit_code\":"] {
        assert!(
            !rendered.contains(forbidden),
            "error history modal leaked raw command JSON {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_results_modal_uses_semantic_final_summary_history() {
    let mut app = rich_content_app();
    let summary = FinalSummaryModel {
        id: "summary-contract-modal".to_string(),
        success: true,
        terminal: "Completed".to_string(),
        changed_files: vec![FileChangeSummaryModel {
            path: "crates/mossen-tui/src/widgets/summary_history.rs".to_string(),
            status: "A".to_string(),
            additions: 220,
            deletions: 0,
        }],
        commands: vec![CommandSummaryModel {
            command: "cargo test -p mossen-tui --test render_contract".to_string(),
            cwd: Some("/Users/allen/Documents/rustmossen".to_string()),
            exit_code: Some(0),
            duration_ms: Some(900),
            status: "passed".to_string(),
        }],
        verification_results: vec![VerificationSummaryModel {
            command: "cargo check -p mossen-tui".to_string(),
            status: "passed".to_string(),
            passed: true,
            exit_code: Some(0),
            duration_ms: Some(700),
        }],
        residual_risks: vec!["Live terminal soak remains outside this unit test".to_string()],
        notes: vec!["No task execution code changed".to_string()],
    };
    app.messages.push(msg(
        MessageType::System,
        final_summary_message_content(&summary),
    ));

    app.prompt.input.clear();
    app.prompt.input.insert_str("/results");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.active_modal,
        ActiveModal::FinalSummaryHistory(_)
    ));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let rendered = render_app(&mut app, 118, 30);
    assert_product_contract("semantic final summary history modal", &rendered);
    assert_contains_all(
        "semantic final summary history modal",
        &rendered,
        &[
            "Final Summaries",
            "summaries:",
            "completed:",
            "[Completed]",
            "details: expanded",
            "summary_history.rs",
            "cargo test -p mossen-tui --test render_contract",
            "check: cargo check -p mossen-tui",
            "risk: Live terminal soak remains outside this unit test",
            "Space collapses details",
        ],
    );
    for forbidden in [
        "mossen-render:final-summary",
        "\"changed_files\"",
        "\"commands\"",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "results modal leaked raw final-summary payload {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_approvals_modal_uses_semantic_decision_history() {
    let mut app = rich_content_app();
    let allowed = ApprovalDecisionModel {
        id: "approval-contract-allowed".to_string(),
        tool_name: "Bash".to_string(),
        decision: ApprovalDecisionKind::Allowed,
        detail: "cargo test -p mossen-tui --test render_contract".to_string(),
        anchor_block_id: Some("tool-0".to_string()),
    };
    let denied = ApprovalDecisionModel {
        id: "approval-contract-denied".to_string(),
        tool_name: "Write".to_string(),
        decision: ApprovalDecisionKind::Denied,
        detail: "crates/mossen-tui/src/task_execution.rs".to_string(),
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

    app.prompt.input.clear();
    app.prompt.input.insert_str("/approvals");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::ApprovalHistory(_)));
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let rendered = render_app(&mut app, 116, 30);
    assert_product_contract("semantic approval history modal", &rendered);
    assert_contains_all(
        "semantic approval history modal",
        &rendered,
        &[
            "Approval History",
            "approvals:",
            "allowed:",
            "denied:",
            "[Allowed]",
            "details: expanded",
            "cargo test -p mossen-tui --test render_contract",
            "source block: approval-contract-allowed",
            "Space collapses details",
        ],
    );
    for forbidden in [
        "mossen-render:approval-decision",
        "\"tool_name\"",
        "\"decision\"",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "approvals modal leaked raw approval payload {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_diff_modal_uses_semantic_diff_viewer() {
    let mut app = rich_content_app();

    app.prompt.input.clear();
    app.prompt.input.insert_str("/diff");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::DiffReview(_)));

    let diff = render_app(&mut app, 110, 26);
    assert_product_contract("semantic diff review modal", &diff);
    assert_contains_all(
        "semantic diff review modal",
        &diff,
        &[
            "Diff Review",
            "render_model.rs",
            "rich-content-diff-anchor",
            "@@ -1,3 +1,4 @@",
            "Left/Right files",
        ],
    );
    assert!(
        !diff.contains("\"stdout\"") && !diff.contains("\"exit_code\""),
        "diff modal must render semantic diff, not raw command JSON\n--- frame ---\n{diff}"
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let folded = render_app(&mut app, 90, 18);
    assert_product_contract("semantic diff review folded modal", &folded);
    assert_contains_all(
        "semantic diff review folded modal",
        &folded,
        &["Diff Review", "File collapsed", "Press Space to expand"],
    );
    assert!(
        !folded.contains("rich-content-diff-anchor"),
        "folding the selected file should hide its hunk body\n--- frame ---\n{folded}"
    );
}

#[test]
fn app_render_contract_statusline_config_keeps_core_status_visible() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.engine_config.cwd = "/Users/allen/Documents/hidden-project".to_string();
    app.engine_config.model = "hidden-model".to_string();
    app.messages = product_messages();
    app.state.ui_stage = UiStage::RunningCommand;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;
    app.state
        .footer_config
        .set_enabled(FooterItem::Project, false);
    app.state
        .footer_config
        .set_enabled(FooterItem::Model, false);
    app.state.footer_config.set_enabled(FooterItem::Cost, false);
    app.state
        .footer_config
        .set_enabled(FooterItem::MessageCount, false);

    let rendered = render_app(&mut app, 120, 24);

    assert_product_contract("statusline configured footer", &rendered);
    assert!(
        rendered.contains("running command"),
        "core status should remain visible when configurable items are hidden\n--- frame ---\n{rendered}"
    );
    for hidden in ["hidden-project", "hidden-model", "$0.15"] {
        assert!(
            !rendered.contains(hidden),
            "statusline config failed to hide {hidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_statusline_presets_are_visible_and_codex_focused() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.engine_config.cwd = dir.path().to_string_lossy().to_string();
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.messages = product_messages();
    app.state.ui_stage = UiStage::RunningCommand;
    app.prompt.input.clear();
    app.prompt.input.insert_str("/statusline");
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::StatusLineConfig(_)));

    let modal = render_app(&mut app, 112, 28);
    assert_product_contract("statusline preset modal", &modal);
    assert_contains_all(
        "statusline preset modal",
        &modal,
        &[
            "Status Line",
            "Preset",
            "Standard",
            "M minimal",
            "C focused",
            "D standard",
            "F full",
            "Space toggles",
        ],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(app.state.footer_config.preset_label(), "Focused");
    app.active_modal = ActiveModal::None;
    let focused = render_app(&mut app, 120, 24);
    assert_product_contract("statusline focused preset", &focused);
    assert_contains_all(
        "statusline focused preset",
        &focused,
        &[
            "MiniMax-M2.7",
            "Supervised",
            "reasoning:high",
            "running command",
            "ctx 0/200k",
        ],
    );
    let footer_line = focused
        .lines()
        .rev()
        .find(|line| {
            line.contains("MiniMax-M2.7")
                && line.contains("ctx 0/200k")
                && !line.contains("status:")
        })
        .unwrap_or_else(|| panic!("focused footer line not found\n--- frame ---\n{focused}"));
    let project_tail = dir
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    for hidden in [project_tail, "$0.15", "msgs"] {
        assert!(
            hidden.is_empty() || !footer_line.contains(hidden),
            "focused statusline footer should hide {hidden:?}\n--- footer ---\n{footer_line}\n--- frame ---\n{focused}"
        );
    }
}

#[test]
fn app_render_contract_title_modal_sets_sanitized_terminal_title() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(MessageType::User, "设置终端标题")];
    app.prompt.input.clear();
    app.prompt
        .input
        .insert_str("/title 渲染会话\u{1b}]2;raw\u{7}");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::TitleConfig(_)));

    let rendered = render_app(&mut app, 100, 22);
    assert_product_contract("session title modal", &rendered);
    assert_contains_all(
        "session title modal",
        &rendered,
        &[
            "Session Title",
            "Current",
            "Custom",
            "Draft",
            "渲染会话",
            "]2;raw",
            "Enter saves",
        ],
    );
    assert_eq!(app.services.manual_title.as_deref(), Some("渲染会话]2;raw"));
    assert!(app.services.title.get_title().contains("渲染会话]2;raw"));
    assert!(
        !rendered.contains('\u{1b}') && !rendered.contains('\u{7}'),
        "title modal must not render terminal control characters\n--- frame ---\n{rendered}"
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.services.manual_title, None);
    let reset = render_app(&mut app, 100, 22);
    assert_contains_all("session title modal reset", &reset, &["default", "reset"]);
}

#[test]
fn app_render_contract_files_modal_uses_semantic_file_change_summary() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![
        msg(MessageType::User, "查看本轮文件变更"),
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
    ];
    app.prompt.input.clear();
    app.prompt.input.insert_str("/files");
    app.prompt.show_suggestions = false;
    app.prompt.selected_suggestion = None;
    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.active_modal, ActiveModal::FileChanges(_)));

    let rendered = render_app(&mut app, 100, 22);
    assert_product_contract("semantic file changes modal", &rendered);
    assert_contains_all(
        "semantic file changes modal",
        &rendered,
        &[
            "File Changes",
            "files: 1",
            "modified: 1",
            "src/lib.rs",
            "[M]",
            "Modified",
            "additions: 2",
            "deletions: 1",
            "Esc closes",
        ],
    );
    for forbidden in ["file_path", "old_string", "new_string"] {
        assert!(
            !rendered.contains(forbidden),
            "file changes modal leaked raw file-change payload key {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_keeps_status_footer_visible_with_transcript() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.engine_config
        .extra_body
        .insert("effort".to_string(), serde_json::json!("high"));
    app.messages = product_messages();
    app.state.ui_stage = UiStage::RunningCommand;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;

    let rendered = render_app(&mut app, 100, 24);

    assert_product_contract("status footer", &rendered);
    assert_contains_all(
        "status footer",
        &rendered,
        &[
            "running command",
            "MiniMax-M2.7",
            "Supervised",
            "reasoning:high",
        ],
    );
    let status_y = first_line_index(&rendered, "MiniMax-M2.7").expect("status should render");
    let transcript_y =
        first_line_index(&rendered, "完整渲染合同").expect("transcript should render");
    let prompt_y = first_line_index(&rendered, "Ask anything").expect("prompt should render");
    assert!(
        status_y > transcript_y && status_y > prompt_y,
        "status footer should stay below transcript and prompt content\n--- frame ---\n{rendered}"
    );
}

#[test]
fn app_render_contract_keeps_active_panel_above_transcript_history() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();
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
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;

    let rendered = render_app(&mut app, 120, 28);

    assert_product_contract("active activity panel", &rendered);
    assert_contains_all(
        "active activity panel",
        &rendered,
        &[
            "Command output",
            "running command",
            "stdout: 8 shown",
            "112 hidden",
            "full log",
        ],
    );
    let panel_y = first_line_index(&rendered, "Command output")
        .expect("active command panel should render above history");
    let transcript_y =
        first_line_index(&rendered, "完整渲染合同").expect("transcript should remain visible");
    assert!(
        panel_y < transcript_y,
        "active panel should occupy chrome above transcript, not overwrite history\n--- frame ---\n{rendered}"
    );
}

#[test]
fn app_render_contract_plan_activity_panel_shows_progress_counts() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = product_messages();
    app.state.ui_stage = UiStage::Planning;
    app.state.render_activity.set(RenderActivity::Plan {
        step_count: 4,
        completed_count: 1,
        active_count: 1,
        pending_count: 1,
        blocked_count: 1,
        active_step: Some("验证计划活动面板".to_string()),
    });
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.prompt.selected_suggestion = None;

    let rendered = render_app(&mut app, 120, 28);

    assert_product_contract("plan activity panel", &rendered);
    assert_contains_all(
        "plan activity panel",
        &rendered,
        &[
            "Plan",
            "planning",
            "4 steps",
            "1 done",
            "1 active",
            "1 pending",
            "1 blocked",
            "active: 验证计划活动面板",
            "Plan: 1 step",
        ],
    );
    let panel_y = first_line_index(&rendered, "4 steps")
        .expect("plan activity panel should render progress counts");
    let transcript_y =
        first_line_index(&rendered, "完整渲染合同").expect("transcript should remain visible");
    assert!(
        panel_y < transcript_y,
        "plan activity panel should stay above transcript history\n--- frame ---\n{rendered}"
    );
    assert!(
        !rendered.contains("new_todos") && !rendered.contains("\"status\""),
        "plan rendering should not leak raw TodoWrite JSON\n--- frame ---\n{rendered}"
    );
}

#[test]
fn app_render_contract_keeps_approval_inline_and_footer_alive() {
    let mut app = complex_streaming_app();
    let rendered = render_app(&mut app, 100, 24);

    assert_product_contract("inline approval", &rendered);
    assert_contains_all(
        "inline approval",
        &rendered,
        &[
            "Shell Command",
            "cargo test -p mossen-tui render_contract",
            "Allow",
            "Always",
            "Deny",
            "approval required",
            "MiniMax-M2.7",
            "msgs",
        ],
    );

    let approval_y =
        first_line_index(&rendered, "Shell Command").expect("approval title should render");
    let footer_y = last_line_index(&rendered, &["MiniMax-M2.7", "msgs"])
        .expect("footer should remain visible");
    assert!(
        approval_y < footer_y,
        "inline approval must stay above the footer\n--- frame ---\n{rendered}"
    );

    let prompt_y = first_line_index(&rendered, "继续验证复杂渲染矩阵")
        .expect("prompt content should remain visible");
    assert!(
        approval_y < prompt_y && prompt_y <= footer_y,
        "approval, prompt, and footer should keep their vertical order\n--- frame ---\n{rendered}"
    );
}

#[test]
fn app_render_contract_survives_resize_storm_and_pathological_content() {
    let mut app = complex_streaming_app();
    app.messages = pathological_messages();
    let storm_sizes = [
        (132, 36),
        (44, 10),
        (100, 24),
        (24, 8),
        (160, 40),
        (32, 10),
        (72, 18),
        (40, 12),
    ];

    for (index, &(width, height)) in storm_sizes.iter().enumerate() {
        let rendered = render_app(&mut app, width, height);
        let name = format!("resize storm frame {index} at {width}x{height}");

        assert_product_contract(&name, &rendered);
        assert!(
            !rendered.contains("file_path"),
            "{name} leaked malformed tool payload details\n--- frame ---\n{rendered}"
        );

        if width >= 72 && height >= 18 {
            assert_contains_any(
                &name,
                &rendered,
                &[
                    "final-render-anchor",
                    "Shell Command",
                    "malformed input",
                    "最后一行仍然要可见",
                    "MiniMax-M2.7",
                ],
            );
        }

        if index == 0 {
            app.scroll.scroll_up(48);
        }
    }

    assert!(
        app.scroll.offset
            <= app
                .scroll
                .total_items
                .saturating_sub(app.scroll.visible_count),
        "resize storm left scroll offset out of bounds: offset={} total={} visible={}",
        app.scroll.offset,
        app.scroll.total_items,
        app.scroll.visible_count
    );
}

#[test]
fn app_render_contract_keeps_long_transcript_scroll_usable() {
    let mut app = scroll_contract_app();

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("long transcript sticky bottom", &bottom);
    assert_contains_all(
        "long transcript sticky bottom",
        &bottom,
        &["tail-scroll-anchor"],
    );

    app.scroll.scroll_up(10_000);
    let top = render_app(&mut app, 80, 18);
    assert_product_contract("long transcript manual top scroll", &top);
    assert_contains_all(
        "long transcript manual top scroll",
        &top,
        &["head-scroll-anchor"],
    );

    app.scroll.scroll_to_bottom();
    let bottom_again = render_app(&mut app, 80, 18);
    assert_product_contract("long transcript return to bottom", &bottom_again);
    assert_contains_all(
        "long transcript return to bottom",
        &bottom_again,
        &["tail-scroll-anchor"],
    );
}

#[test]
fn app_render_contract_reaches_tail_of_single_tall_message() {
    let mut app = tall_single_message_app();

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("single tall message sticky bottom", &bottom);
    assert_contains_all(
        "single tall message sticky bottom",
        &bottom,
        &["tall-single-message-tail-anchor"],
    );
    assert!(
        !bottom.contains("tall-single-row-0800"),
        "single tall message should render the real tail, not the deepest scratch-buffer slice\n--- frame ---\n{bottom}"
    );

    app.scroll.scroll_up(10_000);
    let top = render_app(&mut app, 80, 18);
    assert_product_contract("single tall message manual top scroll", &top);
    assert_contains_all(
        "single tall message manual top scroll",
        &top,
        &["tall-single-message-head-anchor"],
    );

    app.scroll.scroll_to_bottom();
    let bottom_again = render_app(&mut app, 80, 18);
    assert_product_contract("single tall message return to bottom", &bottom_again);
    assert_contains_all(
        "single tall message return to bottom",
        &bottom_again,
        &["tall-single-message-tail-anchor"],
    );
}

#[test]
fn app_render_contract_tall_message_scroll_crosses_scratch_boundary() {
    let mut app = tall_single_message_app();

    let _ = render_app(&mut app, 80, 18);
    app.scroll.sticky = false;
    app.scroll.offset = 812;

    let boundary = render_app(&mut app, 80, 18);
    assert_product_contract("single tall message scratch boundary", &boundary);
    assert_contains_all(
        "single tall message scratch boundary",
        &boundary,
        &["tall-single-row-0410"],
    );
}

#[test]
fn app_render_contract_reaches_tail_beyond_u16_height_cap() {
    let mut app = enormous_single_message_app();

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("single enormous message sticky bottom", &bottom);
    assert_contains_all(
        "single enormous message sticky bottom",
        &bottom,
        &["enormous-single-message-tail-anchor"],
    );
}

#[test]
fn app_render_contract_tall_rich_markdown_virtual_scroll_keeps_shapes() {
    let mut app = tall_rich_markdown_app();

    let bottom = render_app(&mut app, 100, 28);
    assert_product_contract("tall rich markdown sticky bottom", &bottom);
    assert_contains_all(
        "tall rich markdown sticky bottom",
        &bottom,
        &[
            "rich-virtual-code-anchor",
            "Layer",
            "Contract",
            "rich-virtual-tail-anchor",
        ],
    );
    assert_contains_any(
        "tall rich markdown sticky bottom",
        &bottom,
        &["╭─ rust", "rust"],
    );
    assert!(
        !bottom.contains("```rust") && !bottom.contains("| --- |"),
        "tall rich markdown fallback leaked raw markdown syntax\n--- frame ---\n{bottom}"
    );

    app.scroll.scroll_up(900);
    let middle = render_app(&mut app, 100, 28);
    assert_product_contract("tall rich markdown manual middle", &middle);
    assert_contains_any(
        "tall rich markdown manual middle",
        &middle,
        &["rich-virtual-row-0", "rich_virtual_code_"],
    );
}

#[test]
fn app_render_contract_streaming_tall_message_keeps_scroll_policy() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();

    app.messages = vec![msg(
        MessageType::User,
        "启动超长 streaming 回复，验证 sticky-bottom 和手动上滚策略。",
    )];
    app.handle_engine_message(stream_event(StreamEventData::MessageStart));
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "## streaming-tall-head-anchor\n\n".to_string(),
        },
    }));
    for index in 0..900 {
        app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: format!(
                    "- streaming-tall-row-{index:04}：streaming 增长时 sticky-bottom 必须跟住真实尾部。\n"
                ),
            },
        }));
    }
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "\nstreaming-tall-tail-anchor：当前 streaming 尾部。".to_string(),
        },
    }));

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("streaming tall sticky bottom", &bottom);
    assert_contains_all(
        "streaming tall sticky bottom",
        &bottom,
        &["streaming-tall-tail-anchor"],
    );

    app.scroll.scroll_up(10_000);
    let top = render_app(&mut app, 80, 18);
    assert_product_contract("streaming tall manual top scroll", &top);
    assert_contains_all(
        "streaming tall manual top scroll",
        &top,
        &["streaming-tall-head-anchor"],
    );

    for index in 900..940 {
        app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: format!(
                    "\nstreaming-tall-row-{index:04}：用户手动上滚时新增内容不能强行抢回底部。"
                ),
            },
        }));
    }
    let still_top = render_app(&mut app, 80, 18);
    assert_product_contract("streaming tall preserves manual scroll", &still_top);
    assert_contains_all(
        "streaming tall preserves manual scroll",
        &still_top,
        &["streaming-tall-head-anchor"],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    let returned_bottom = render_app(&mut app, 80, 18);
    assert_product_contract("streaming tall ctrl-l returns bottom", &returned_bottom);
    assert_contains_all(
        "streaming tall ctrl-l returns bottom",
        &returned_bottom,
        &["streaming-tall-row-0939"],
    );
}

#[test]
fn app_render_contract_streaming_resize_mouse_scroll_stays_usable() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.glyphs = RenderGlyphs::ascii();
    app.active_modal = ActiveModal::None;
    app.prompt.input.clear();
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();
    app.state.task_list.tasks.clear();
    app.state.teammate_states.clear();

    app.messages = vec![msg(
        MessageType::User,
        "启动组合渲染合同：streaming 长输出、鼠标滚动、resize、继续追加 token。",
    )];
    app.handle_engine_message(stream_event(StreamEventData::MessageStart));
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "## streaming-resize-scroll-head-anchor\n\n".to_string(),
        },
    }));
    for index in 0..720 {
        app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: format!(
                    "- streaming-resize-scroll-row-{index:04}: long streaming output must keep viewport ownership while it grows.\n"
                ),
            },
        }));
    }
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "\nstreaming-resize-scroll-tail-before-resize".to_string(),
        },
    }));

    let bottom = render_app(&mut app, 92, 20);
    assert_product_contract("stream resize mouse initial bottom", &bottom);
    assert_contains_all(
        "stream resize mouse initial bottom",
        &bottom,
        &["streaming-resize-scroll-tail-before-resize"],
    );
    let initial_rail_rows = scrollbar_rail_rows(&bottom, 92);
    assert!(
        !initial_rail_rows.is_empty(),
        "stream resize mouse contract should expose transcript scrollbar\n{bottom}"
    );
    let rail_top = *initial_rail_rows
        .first()
        .expect("initial scrollbar top row");

    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Down(MouseButton::Left),
        91,
        rail_top as u16,
    ));
    let manual_top = render_app(&mut app, 92, 20);
    assert_product_contract("stream resize mouse manual top", &manual_top);
    assert_contains_all(
        "stream resize mouse manual top",
        &manual_top,
        &["streaming-resize-scroll-head-anchor"],
    );
    assert!(
        !manual_top.contains("streaming-resize-scroll-tail-before-resize"),
        "mouse scrollbar top click should leave the streaming tail\n{manual_top}"
    );
    assert!(
        !app.scroll.sticky,
        "mouse scrollbar top click should disable sticky-bottom"
    );

    for index in 720..760 {
        app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: format!(
                    "\nstreaming-resize-scroll-row-{index:04}: appended while reader is manually scrolled."
                ),
            },
        }));
    }
    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "\n\nW82-tail-after-drag".to_string(),
        },
    }));

    let resized_manual = render_app(&mut app, 72, 16);
    assert_product_contract(
        "stream resize mouse preserves manual scroll",
        &resized_manual,
    );
    assert_contains_all(
        "stream resize mouse preserves manual scroll",
        &resized_manual,
        &["streaming-resize-scroll-head-anchor"],
    );
    assert!(
        !resized_manual.contains("W82-tail-after-drag"),
        "resize plus appended streaming text must not steal manual scroll\n{resized_manual}"
    );
    assert!(
        !app.scroll.sticky,
        "resized manual transcript should remain non-sticky"
    );

    let resized_rail_rows = scrollbar_rail_rows(&resized_manual, 72);
    assert!(
        !resized_rail_rows.is_empty(),
        "resized manual transcript should keep a clickable scrollbar\n{resized_manual}"
    );
    let rail_bottom = *resized_rail_rows
        .last()
        .expect("resized scrollbar bottom row");
    app.dispatch_mouse_for_test(mouse_at(
        MouseEventKind::Drag(MouseButton::Left),
        71,
        rail_bottom as u16,
    ));

    let restored_bottom = render_app(&mut app, 72, 16);
    assert_product_contract("stream resize mouse drag returns bottom", &restored_bottom);
    assert_contains_all(
        "stream resize mouse drag returns bottom",
        &restored_bottom,
        &["W82-tail-after-drag"],
    );
    assert!(
        app.scroll.sticky,
        "dragging the resized rail bottom should restore sticky-bottom"
    );
}

#[test]
fn app_render_contract_keyboard_focus_scroll_owns_viewport() {
    let mut app = scroll_contract_app();
    app.active_modal = ActiveModal::None;
    app.prompt.show_suggestions = false;
    app.prompt.suggestions.clear();

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("keyboard focus scroll setup", &bottom);
    assert_contains_all(
        "keyboard focus scroll setup",
        &bottom,
        &["tail-scroll-anchor"],
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    assert!(
        !app.scroll.sticky,
        "keyboard focus navigation should take ownership of transcript scroll"
    );

    let focused_top = render_app(&mut app, 80, 18);
    assert_product_contract("keyboard focus scroll top", &focused_top);
    assert_contains_all(
        "keyboard focus scroll top",
        &focused_top,
        &["head-scroll-anchor"],
    );
    assert!(
        !focused_top.contains("tail-scroll-anchor"),
        "keyboard focus should move the viewport away from the sticky tail\n{focused_top}"
    );

    app.handle_engine_message(SdkMessage::ApiRetry {
        error: "retry while keyboard focus is reading history".to_string(),
        attempt: 1,
        max_retries: 3,
        retry_in_ms: 250,
        task_id: None,
    });
    let after_append = render_app(&mut app, 80, 18);
    assert_product_contract("keyboard focus scroll after append", &after_append);
    assert_contains_all(
        "keyboard focus scroll after append",
        &after_append,
        &["head-scroll-anchor"],
    );
    assert!(
        !after_append.contains("tail-scroll-anchor"),
        "async append must not steal a keyboard-focused history viewport\n{after_append}"
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    let restored_bottom = render_app(&mut app, 80, 18);
    assert_product_contract("keyboard focus ctrl-l bottom", &restored_bottom);
    assert_contains_all(
        "keyboard focus ctrl-l bottom",
        &restored_bottom,
        &["tail-scroll-anchor"],
    );
}

#[test]
fn app_render_contract_async_append_preserves_manual_row_scroll() {
    let mut app = tall_single_message_app();

    let bottom = render_app(&mut app, 80, 18);
    assert_product_contract("manual row scroll setup", &bottom);
    assert_contains_all(
        "manual row scroll setup",
        &bottom,
        &["tall-single-message-tail-anchor"],
    );

    app.scroll.scroll_up(200);
    let preserved_offset = app.scroll.offset;
    assert!(
        preserved_offset > app.messages.len(),
        "fixture must be row-scrolled, not message-index-scrolled: offset={}, messages={}",
        preserved_offset,
        app.messages.len()
    );

    app.handle_engine_message(SdkMessage::ApiRetry {
        error: "retry while user is reading prior output".to_string(),
        attempt: 1,
        max_retries: 3,
        retry_in_ms: 250,
        task_id: None,
    });

    assert_eq!(
        app.scroll.offset, preserved_offset,
        "async transcript append must not clamp row-based manual scroll to message count"
    );

    let after_append = render_app(&mut app, 80, 18);
    assert_product_contract("manual row scroll after async append", &after_append);
    assert!(
        !after_append.contains("tall-single-message-head-anchor"),
        "manual row scroll should not jump back to transcript head\n--- frame ---\n{after_append}"
    );
}

#[test]
fn app_render_contract_survives_streaming_resize_interleave() {
    let mut app = App::new();
    apply_common_product_state(&mut app);
    app.messages = vec![msg(MessageType::User, "启动 streaming resize 合同。")];
    app.handle_engine_message(stream_event(StreamEventData::MessageStart));

    let chunks = [
        "<think>先分析入口",
        "，再分析渲染层</think>\nstream-anchor：开始输出。\n",
        "```rust\nfn render_contract() {}\n```\n",
        "stream-tail-anchor：最终 streaming 内容。\n",
    ];
    let sizes = [(132, 36), (40, 10), (100, 24), (32, 8), (72, 18)];

    for (index, chunk) in chunks.iter().enumerate() {
        app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: (*chunk).to_string(),
            },
        }));
        let (width, height) = sizes[index % sizes.len()];
        let rendered = render_app(&mut app, width, height);
        let name = format!("streaming resize interleave {index} at {width}x{height}");
        assert_product_contract(&name, &rendered);
        if index > 0 && width >= 72 && height >= 18 {
            assert_contains_any(&name, &rendered, &["stream-anchor", "stream-tail-anchor"]);
        }
    }

    app.handle_engine_message(stream_event(StreamEventData::MessageDelta {
        usage: None,
        stop_reason: Some("tool_use".to_string()),
    }));
    let after_tool_stop = render_app(&mut app, 100, 24);
    assert_product_contract("streaming tool_use stop hidden", &after_tool_stop);
    assert!(
        !after_tool_stop.contains("tool_use"),
        "tool_use stop reason should not render as assistant output\n--- frame ---\n{after_tool_stop}"
    );

    app.handle_engine_message(SdkMessage::Result {
        terminal: "Completed".to_string(),
        cost_usd: Some(0.01),
        duration_ms: Some(42),
        usage: None,
        task_id: None,
    });
    let completed = render_app(&mut app, 100, 24);
    assert_product_contract("streaming completed frame", &completed);
    assert_contains_all(
        "streaming completed frame",
        &completed,
        &["stream-tail-anchor"],
    );
}

#[test]
fn app_render_contract_survives_deterministic_semantic_fuzz_corpus() {
    for seed in 0..6 {
        let mut app = fuzz_contract_app(seed);
        for &(width, height) in &[(28, 8), (44, 12), (80, 18), (120, 28)] {
            let rendered = render_app(&mut app, width, height);
            let name = format!("semantic fuzz seed {seed} at {width}x{height}");
            assert_product_contract(&name, &rendered);
            assert!(
                !rendered.contains("file_path"),
                "{name} leaked malformed JSON-looking tool payload\n--- frame ---\n{rendered}"
            );
            if width >= 80 && height >= 18 {
                assert_contains_any(
                    &name,
                    &rendered,
                    &[&format!("fuzz-tail-anchor-{seed}"), "Bash", "Glob"],
                );
            }
        }
    }
}

#[test]
fn app_render_contract_arbitrary_tool_payloads_are_scrubbed_and_redacted() {
    let mut app = arbitrary_tool_payload_app();
    let rendered = render_app(&mut app, 120, 28);
    assert_product_contract("arbitrary tool payload render", &rendered);
    assert_contains_all(
        "arbitrary tool payload render",
        &rendered,
        &[
            "CustomProvider",
            "authorization_header",
            "arbitrary-tool-visible-query",
            "arbitrary-tool-visible-nested",
            "arbitrary-tool-anchor",
            "redacted",
            "token_count: 128",
            "total_tokens: 256",
        ],
    );
    for forbidden in [
        "raw-use-auth-secret",
        "raw-use-client-secret",
        "raw-result-session-token",
        "raw-result-private-key",
        "raw-result-access-token",
        "raw-result-password",
        "Bearer",
        "\u{1b}",
        "\u{7}",
        "\u{8}",
        "[31m",
        "[0m",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "arbitrary tool payload render leaked {forbidden:?}\n--- frame ---\n{rendered}"
        );
    }
}

#[test]
fn app_render_contract_survives_generated_property_fuzz_matrix() {
    for seed in 0..8 {
        let mut app = generated_property_fuzz_app(seed);
        for &(width, height) in &[(24, 6), (32, 8), (48, 12), (80, 20), (132, 34)] {
            let bottom = render_app(&mut app, width, height);
            let name = format!("property fuzz seed {seed} bottom at {width}x{height}");
            assert_product_contract(&name, &bottom);
            assert_scroll_state_bounded(&name, &app);
            assert!(
                !bottom.contains("property-fuzz-secret") && !bottom.contains("Bearer property"),
                "{name} leaked generated fuzz secret\n--- frame ---\n{bottom}"
            );
            if width >= 80 && height >= 20 {
                assert_contains_any(
                    &name,
                    &bottom,
                    &[
                        &format!("property-fuzz-tail-anchor-{seed}"),
                        "property-fuzz-visible",
                    ],
                );
            }

            let scroll_rows = 5 + seed * 3 + width as usize % 11;
            app.scroll.scroll_up(scroll_rows);
            let manual = render_app(&mut app, width, height);
            let manual_name = format!("property fuzz seed {seed} manual at {width}x{height}");
            assert_product_contract(&manual_name, &manual);
            assert_scroll_state_bounded(&manual_name, &app);
            assert!(
                !manual.contains("property-fuzz-secret") && !manual.contains("Bearer property"),
                "{manual_name} leaked generated fuzz secret\n--- frame ---\n{manual}"
            );
        }
    }
}

#[test]
fn app_render_contract_simulated_streaming_soak_keeps_scroll_and_budget() {
    let mut app = streaming_soak_app();
    let sizes = [(96, 22), (72, 18), (120, 30), (48, 12), (84, 20)];
    let started = Instant::now();
    let mut frames = 0usize;
    let mut manual_scroll_started = false;

    for index in 0..1_800 {
        push_streaming_soak_delta(&mut app, index);

        if index == 420 {
            let before_manual = render_app(&mut app, 96, 22);
            assert_product_contract("streaming soak before manual scroll", &before_manual);
            assert_contains_all(
                "streaming soak before manual scroll",
                &before_manual,
                &["streaming-soak-row-0419"],
            );
            app.scroll.scroll_up(10_000);
            manual_scroll_started = true;
            assert!(
                !app.scroll.sticky,
                "streaming soak manual scroll should disable sticky-bottom"
            );
        }

        if index % 90 == 0 {
            let (width, height) = sizes[(index / 90) % sizes.len()];
            let name = format!("streaming soak frame {index} at {width}x{height}");
            let rendered = render_app(&mut app, width, height);
            assert_product_contract(&name, &rendered);
            assert_scroll_state_bounded(&name, &app);
            frames += 1;

            if manual_scroll_started && height >= 12 {
                assert_contains_all(&name, &rendered, &["streaming-soak-head-anchor"]);
                assert!(
                    !rendered.contains("streaming-soak-row-1799"),
                    "{name} should not jump to the live tail while manual scroll owns history\n--- frame ---\n{rendered}"
                );
                assert!(
                    !app.scroll.sticky,
                    "{name} should preserve non-sticky manual scroll during streaming append"
                );
            }
        }
    }

    app.handle_engine_message(stream_event(StreamEventData::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: "\nstreaming-soak-tail-anchor".to_string(),
        },
    }));

    let manual = render_app(&mut app, 96, 22);
    assert_product_contract("streaming soak manual final frame", &manual);
    assert_scroll_state_bounded("streaming soak manual final frame", &app);
    assert_contains_all(
        "streaming soak manual final frame",
        &manual,
        &["streaming-soak-head-anchor"],
    );
    assert!(
        !manual.contains("streaming-soak-tail-anchor"),
        "manual streaming soak viewport should not be stolen by the final tail\n--- frame ---\n{manual}"
    );

    app.dispatch_key_for_test(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    let restored = render_app(&mut app, 96, 22);
    assert_product_contract("streaming soak restored bottom", &restored);
    assert_scroll_state_bounded("streaming soak restored bottom", &app);
    assert_contains_all(
        "streaming soak restored bottom",
        &restored,
        &["streaming-soak-tail-anchor"],
    );
    assert!(
        app.scroll.sticky,
        "Ctrl-L should restore streaming soak sticky-bottom"
    );

    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(1_500),
        "streaming soak render budget exceeded: {elapsed:?} for {frames} sampled frames"
    );
    assert!(
        frames >= 20,
        "streaming soak should sample enough frames to exercise resize pacing, got {frames}"
    );
}

#[test]
fn app_render_contract_large_session_stays_within_budget() {
    let mut app = large_session_app();
    let sizes = [(120, 32), (80, 24), (132, 36), (48, 14)];
    let started = Instant::now();

    for &(width, height) in &sizes {
        let rendered = render_app(&mut app, width, height);
        let name = format!("large session budget at {width}x{height}");
        assert_product_contract(&name, &rendered);
        if width >= 80 && height >= 24 {
            assert_contains_any(&name, &rendered, &["large-session-tail-anchor"]);
        }
    }

    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(800),
        "large session render budget exceeded: {elapsed:?} for {} frames",
        sizes.len()
    );
}

#[test]
fn app_render_contract_renders_rich_markdown_code_table_and_diff() {
    let mut app = rich_content_app();

    for &(width, height) in &[(160, 48), (120, 36), (80, 24), (48, 14)] {
        let rendered = render_app(&mut app, width, height);
        let name = format!("rich content active frame at {width}x{height}");

        assert_product_contract(&name, &rendered);
        assert!(
            !rendered.contains("```rust") && !rendered.contains("| --- |"),
            "{name} leaked raw Markdown syntax instead of rendered structure\n--- frame ---\n{rendered}"
        );

        if width >= 140 && height >= 40 {
            assert_contains_all(
                &name,
                &rendered,
                &[
                    "富文本渲染合同",
                    "rich_contract",
                    "Layer",
                    "Responsibility",
                    "diff --git",
                    "rich-content-diff-anchor",
                    "rich-content-tail-anchor",
                ],
            );
            assert_contains_any(&name, &rendered, &["╭─ rust", "rust"]);
        } else if width >= 100 && height >= 30 {
            assert_contains_all(
                &name,
                &rendered,
                &[
                    "Layer",
                    "Responsibility",
                    "diff --git",
                    "rich-content-diff-anchor",
                    "rich-content-tail-anchor",
                ],
            );
            assert_contains_any(&name, &rendered, &["╰─", "rust", "rich_contract"]);
        } else if width >= 80 && height >= 24 {
            assert_contains_all(
                &name,
                &rendered,
                &["rich-content-diff-anchor", "rich-content-tail-anchor"],
            );
            assert_contains_any(
                &name,
                &rendered,
                &["diff --git", "@@ -1,3 +1,4 @@", "index"],
            );
        } else {
            assert_contains_any(
                &name,
                &rendered,
                &[
                    "rich-content-tail-anchor",
                    "diff --git",
                    "rich_contract",
                    "MiniMax-M2.7",
                ],
            );
        }
    }
}

#[test]
fn semantic_render_contract_strips_protocol_before_layout() {
    let transcript = RenderTranscript::from_messages(&product_messages());
    let debug = format!("{transcript:#?}");
    assert_semantic_contract("semantic transcript", &debug);

    let bash = transcript
        .blocks
        .iter()
        .find_map(|block| block.tool.as_ref().filter(|tool| tool.name == "Bash"))
        .expect("Bash tool should become a semantic tool card");
    assert_eq!(bash.phase, ToolPhase::Succeeded);
    assert!(
        bash.sections
            .iter()
            .any(|section| section.body.contains("ls -la")),
        "Bash command input should survive semantic cleanup: {bash:#?}"
    );
    assert!(
        bash.sections
            .iter()
            .any(|section| section.body.contains("Cargo.toml")),
        "Bash stdout should survive semantic cleanup: {bash:#?}"
    );

    assert!(
        transcript
            .blocks
            .iter()
            .any(|block| block.nodes.iter().any(
                |node| matches!(node, RenderNode::Markdown(text) if text.contains("```rust"))
            )),
        "assistant markdown must remain markdown in the semantic model: {transcript:#?}"
    );
}

#[test]
fn layout_height_contract_is_deterministic_cached_and_bounded() {
    let mut messages = product_messages();
    let huge_stdout = (0..800)
        .map(|index| format!("render-contract-line-{index:04}"))
        .collect::<Vec<_>>()
        .join("\n");
    messages.push(tool_msg(
        MessageType::ToolUse,
        "Bash",
        "command  printf many-lines",
    ));
    messages.push(tool_msg(
        MessageType::ToolResult,
        "Bash",
        serde_json::json!({
            "stdout": huge_stdout,
            "stderr": "",
            "exit_code": 0
        })
        .to_string(),
    ));

    let transcript = RenderTranscript::from_messages(&messages);
    let theme = Theme::default();
    let collapsed = HashSet::new();
    let cache = RenderHeightCache::default();

    for width in [18u16, 24, 32, 48, 80, 120] {
        let no_cache = MessagesWidget::required_content_height_from_transcript_with_cache(
            messages.len(),
            &transcript,
            &theme,
            width,
            true,
            &collapsed,
            None,
        );
        let cached_first = MessagesWidget::required_content_height_from_transcript_with_cache(
            messages.len(),
            &transcript,
            &theme,
            width,
            true,
            &collapsed,
            Some(&cache),
        );
        let cached_second = MessagesWidget::required_content_height_from_transcript_with_cache(
            messages.len(),
            &transcript,
            &theme,
            width,
            true,
            &collapsed,
            Some(&cache),
        );

        assert_eq!(
            no_cache, cached_first,
            "height cache changed layout at width {width}"
        );
        assert_eq!(
            cached_first, cached_second,
            "height cache is not deterministic at width {width}"
        );
        assert!(
            (1..260).contains(&no_cache),
            "long tool output should stay preview-bounded at width {width}, got height {no_cache}"
        );
    }

    let stats = cache.stats();
    assert!(
        stats.hits > 0 && stats.misses > 0,
        "height cache should be exercised by the contract test, got {stats:?}"
    );
}
