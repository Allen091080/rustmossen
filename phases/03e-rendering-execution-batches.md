# Phase 3.5 Round 4: Rendering Execution Batches

This document is the durable handoff for the five rendering batches. If the
agent context is compacted or another session resumes the work, start here,
then run the commands in the "Current Checkpoint" section before editing.

## Current Checkpoint

- Workspace: `/Users/allen/Documents/rustmossen`
- User goal: make the harness-agent terminal rendering feel as solid as Codex
  CLI / Mossen Code CLI before deeper chain work.
- Most recent blocker fixed before this plan: agent loop no longer exits after
  MiniMax/OpenAI-compatible tool results or thinking-only responses.
- Rendering baseline: messages, markdown, inline approval, status bar,
  tool-result branches, TaskList, teammate tree, and MCP status exist, but they
  need a stricter contract, snapshot coverage, and transcript-level polish.
- Resume command set:

```bash
cd /Users/allen/Documents/rustmossen
cargo build -p mossen-cli --bin mossen
cargo test -p mossen-tui render_snapshot -- --nocapture
git diff -- phases/03e-rendering-execution-batches.md crates/mossen-tui/RENDER_CONTRACT.md crates/mossen-tui/tests/render_snapshot.rs
```

## Non-Negotiable UX Rules

1. The transcript must explain the agent loop: user intent, assistant thought
   surface, tool attempt, approval wait, tool result, continuation, final answer.
2. Never show raw protocol noise as primary UI: `stop`, `null`, broken JSON
   prefixes, or internal finish reasons are defects unless explicitly inside a
   debug panel.
3. Approval follows the tool that needs it. A footer may announce waiting state,
   but the decision UI belongs under the relevant tool card.
4. Long content must be bounded by default and expandable by user action.
5. Tool cards must be semantically distinct: Bash is not Read, Read is not Grep,
   and diff is not plain text.
6. Snapshot fixtures must cover regressions before visual polish is layered on.

## Batch 1: Contract And Snapshot Harness

### Goal

Create the rendering contract and a ratatui buffer snapshot harness so future
rendering changes have stable expectations.

### Scope

- Add `crates/mossen-tui/RENDER_CONTRACT.md`.
- Add integration tests under `crates/mossen-tui/tests/render_snapshot.rs`.
- Cover:
  - mixed assistant markdown and tool transcript;
  - Bash / Read / Grep / Edit-style output;
  - inline permission prompt;
  - narrow viewport clipping and bounded long output.

### Done Criteria

- `cargo test -p mossen-tui render_snapshot -- --nocapture` passes.
- Snapshots assert high-signal UI markers, not color internals.
- The test names and contract make the next batch obvious without chat context.

## Batch 2: Transcript Structure And Layout Ownership

### Goal

Move from a loose message list to a transcript that preserves agent-loop
relationships.

### Tasks

- Pair `ToolUse` and `ToolResult` into a stable visual group.
- Represent `ToolUseStarted`, `Running`, `ApprovalRequired`, `Success`, `Error`,
  `Cancelled` as explicit render states.
- Ensure approval panel is allocated by the grouped tool row, not by an
  unrelated screen overlay.
- Move TaskList / teammate activity away from ad-hoc overlay positions. They
  must either be transcript items or deterministic reserved panels.
- Make sticky scroll respect user scroll: auto-scroll only when already at the
  bottom.

### Done Criteria

- A project-analysis turn visibly flows through assistant text -> tool card ->
  approval/result -> assistant continuation.
- TaskList and teammate activity no longer cover message text.
- Snapshot coverage includes grouped tool result and inline approval below the
  triggering tool.

## Batch 3: P0 Tool Cards

### Goal

Make the daily coding tools readable enough that users can judge what happened
without opening logs.

### Tasks

- Bash card:
  - command, cwd, exit code, duration when available;
  - stdout and stderr sections;
  - timeout/interrupted/error states;
  - bounded preview with expansion hint.
- Read card:
  - path, range, total lines;
  - line numbers and syntax highlighting;
  - binary/image/error states.
- Grep / Glob cards:
  - pattern, path, mode;
  - match counts and file lists;
  - highlighted content-mode matches when present.
- Edit / Write / MultiEdit cards:
  - file path;
  - unified diff coloring;
  - added/removed counts;
  - clear success/error state.
- TodoWrite card:
  - task status changes;
  - no deadlock and no hidden "tool completed but UI stopped" state.
- Task / sub-agent card:
  - child agent status, nested tool summary, final result.

### Done Criteria

- P0 tool cards are covered by snapshots at 80x24 and 120x40.
- No P0 tool result renders as opaque JSON in normal mode.

## Batch 4: Approval And Footer

### Goal

Make blocking state unmistakable and operable without stealing context.

### Tasks

- Replace default centered permission overlay with inline approval under the
  relevant tool card.
- Show risk-bearing fields:
  - shell command;
  - file path;
  - write/edit diff summary;
  - URL;
  - MCP server/tool source.
- Keyboard rules:
  - Tab / arrows move selection;
  - Enter confirms;
  - Esc denies or backs out consistently.
- Preserve transcript history after decision: approved / always / denied.
- Footer layout:
  - left: project/cwd and branch if known;
  - middle: model, access mode, turn state;
  - right: cost/context/MCP/background tasks.
- Footer state labels:
  - `idle`, `waiting`, `streaming`, `running tool`, `waiting approval`,
    `cancelling`, `cancelled`, `error`.

### Done Criteria

- The user can decide a permission request by reading only the triggering tool
  card plus its inline approval panel.
- Footer tells where the agent is blocked without duplicating the approval body.

## Batch 5: Real TTY Bake And Polish

### Goal

Use real terminal sessions to turn the renderer from "passes tests" into
"feels stable while coding."

### Manual Bake Scenarios

Run in a real TTY:

```bash
cd /Users/allen/Documents/rustmossen
export MOSSEN_CODE_USE_CUSTOM_BACKEND=1
export MOSSEN_CODE_CUSTOM_BASE_URL=https://api.minimaxi.com/v1
export MOSSEN_CODE_CUSTOM_MODEL=MiniMax-M2.7
export MOSSEN_CODE_CUSTOM_AUTH_TOKEN='<your MiniMax key>'
target/debug/mossen
```

Then test:

1. `你好，给一个带列表和代码块的回复`
2. `读一下 Cargo.toml`
3. `执行 ls -la`
4. `搜索 ToolResult 在哪里渲染`
5. `修改一个很小的文件并展示 diff`
6. `用 TodoWrite 建三个任务`
7. `派一个子 agent 分析 mossen-tui 的消息渲染`
8. Start a long task, press Ctrl+C, verify the TUI does not exit.

### Done Criteria

- No panic.
- No unexpected process exit.
- No naked `stop`, `null`, or raw protocol fragments in normal transcript.
- Long content is bounded and expandable.
- Markdown code blocks and tables are readable.
- Chinese wide characters do not break layout.
- Approval, footer, and scroll behavior remain stable in narrow and wide views.

## Progress Log

- 2026-05-21: Plan created. Batch 1 starts with render contract and snapshot
  harness.
- 2026-05-21: Batch 1 initial implementation landed:
  - added `crates/mossen-tui/RENDER_CONTRACT.md`;
  - added `crates/mossen-tui/tests/render_snapshot.rs`;
  - covered mixed assistant markdown + Bash/Read/Grep/Edit cards, permission
    panel, and bounded long Bash output;
  - fixed Grep / Glob tool-result height estimation so match rows are not
    clipped out of boxed cards;
  - verified with `cargo test -p mossen-tui render_snapshot -- --nocapture`.
- 2026-05-21: Batch 2 layout ownership landed:
  - added `split_auxiliary_panels` so TodoWrite task state and sub-agent
    activity reserve deterministic space instead of painting over transcript
    rows;
  - wired fullscreen and inline rendering through the auxiliary layout split;
  - kept tiny terminal behavior conservative by skipping live panels when there
    is not enough room to preserve readable message content;
  - added layout tests for wide right-rail, narrow stacked panels, and tiny
    content fallback.
- 2026-05-21: Batch 3 P0 tool-card pass landed:
  - Bash result cards now surface command, cwd, duration, stdout, and stderr
    metadata when available, including error snapshots;
  - TodoWrite results render as task counts and status lines instead of
    leaking `old_todos` / `new_todos` protocol JSON;
  - Write-style results summarize file path, line count, and byte count instead
    of dumping the written content into the normal transcript;
  - Task / Agent results reuse markdown rendering for final output;
  - render snapshots cover Bash error metadata, TodoWrite, Write summaries, and
    nested agent markdown, while asserting that protocol noise such as `null`
    and stop markers stays out of the primary UI.
- 2026-05-21: Batch 4 approval/footer pass landed:
  - inline approval panel now labels the risk-bearing field (`Command`, `URL`,
    `Write Path`, etc.) and shows concise keyboard hints beside the waiting
    state;
  - fullscreen and inline render paths allocate approval and auxiliary regions
    before rendering transcript rows, so blocking UI follows the turn instead
    of floating over previous content;
  - footer turn-state labels now distinguish `running tool` and
    `waiting approval`;
  - coverage added for approval card detail rendering and footer state
    transitions.
- 2026-05-21: Batch 5 automated bake completed:
  - `cargo fmt -p mossen-tui` passed;
  - `cargo test -p mossen-tui render_snapshot -- --nocapture` passed;
  - `cargo test -p mossen-tui layout::tests -- --nocapture` passed;
  - `cargo test -p mossen-tui widgets::messages::tests -- --nocapture`
    passed;
  - `cargo test -p mossen-tui message_model::tests -- --nocapture` passed;
  - `cargo test -p mossen-tui engine_stream_tests -- --nocapture` passed;
  - `cargo build -p mossen-cli --bin mossen` passed;
  - `target/debug/mossen --help` starts and prints the CLI help.
- 2026-05-21: Post-batch rendering polish pass landed:
  - ToolUse cards now translate known JSON inputs into semantic rows for Bash,
    Read, Grep, Glob, Write/Edit, TodoWrite, and Task/Agent instead of showing
    raw JSON keys;
  - empty and null tool inputs render as `(no input)` so normal transcripts do
    not show naked protocol `null`;
  - Read results now surface line ranges, image/binary metadata, and error
    labels without dumping raw payloads;
  - Grep/Glob previews are bounded independently from shell output, show
    counts, handle structured JSON search results, and preserve upstream
    truncation hints;
  - Write/Edit cards now include content summaries and derived diff change
    counts before previews;
  - added snapshots for semantic tool inputs, structured search/read results,
    media/error read states, and diff summaries;
  - Markdown rendering now inserts controlled spacing between headings,
    paragraphs, lists, quotes, code blocks, rules, and tables, with unit
    coverage for no trailing blank rows;
  - re-verified with render snapshots, message/layout/stream regression tests,
    and `cargo build -p mossen-cli --bin mossen`.
- 2026-05-21: Runtime hang hardening landed after a real 5000s+ streaming
  freeze:
  - pending permission requests now preempt non-critical UI such as the
    welcome-back idle dialog, so the agent loop cannot wait behind a hidden
    approval panel;
  - idle-return and cost-threshold dialogs now dismiss on any key, matching the
    on-screen prompt and avoiding a stale modal that blocks later turn state;
  - MCP channel approvals use the same yielding-modal path as tool approvals;
  - OpenAI-compatible custom-backend streams now have a semantic-progress
    watchdog, defaulting to 300 seconds and configurable with
    `MOSSEN_CODE_CUSTOM_STREAM_TIMEOUT_SECS`;
  - the watchdog resets only when text, tool calls, finish reasons, or usage
    arrive, so empty keepalive chunks cannot keep the UI stuck forever;
  - added a regression test for hidden approval behind `IdleReturn`;
  - verified with `cargo fmt -p mossen-agent -p mossen-tui`,
    `cargo test -p mossen-tui engine_stream_tests -- --nocapture`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo build -p mossen-cli --bin mossen`, and `git diff --check`.
- 2026-05-21: Text-boundary architecture pass started after a Chinese prompt
  panic exposed scattered byte slicing:
  - added shared UTF-8 safe truncation helpers in `mossen-utils` for character
    truncation and byte-budget prefix/suffix extraction;
  - moved approval summaries, tool summaries, telemetry previews, shell/MCP
    previews, terminal wrapping, hook errors, context truncation, and root
    render clipping away from direct `&text[..n]` slicing;
  - extended the same rule to persisted large tool-result previews and IDE
    diagnostic summaries, so Chinese/emoji diagnostics and saved shell output
    cannot panic while being shortened;
  - documented the rule in `RENDER_CONTRACT.md`: user, model, and tool text
    must use shared helpers; raw string slicing is reserved for parsing known
    ASCII/protocol formats;
  - added regression tests for multibyte approval input, byte-budget previews,
    diagnostic summaries, and shared string helpers so long Chinese Task
    prompts or tool output cannot panic the agent loop;
  - verified with `cargo fmt -p mossen-utils -p mossen-agent -p mossen-tools -p
    mossen-tui`, `cargo test -p mossen-utils string_utils -- --nocapture`,
    `cargo test -p mossen-utils
    tool_result_storage::tests::generate_preview_never_splits_multibyte_text --
    --nocapture`, `cargo test -p mossen-agent
    diagnostic_summary_truncates_multibyte_text_safely -- --nocapture`,
    `cargo test -p mossen-tui engine_stream_tests -- --nocapture`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo build -p mossen-cli --bin mossen`, and `git diff --check`.
