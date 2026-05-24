# Mossen TUI Render Contract

This contract defines the normal terminal rendering surface for harness-agent
style coding sessions. It is intentionally user-facing: protocol details are
allowed in debug output, but the default transcript should render semantic
agent events.

## Transcript Items

### UserMessage

- Prefix: user marker.
- Body: plain user text with attachment chips for pasted images.
- Must preserve Chinese and other wide characters without overlap.

### AssistantMarkdown

- Prefix: assistant marker.
- Body: Markdown, including headings, lists, links, tables, inline code, and
  fenced code blocks.
- Paragraphs, lists, quotes, code blocks, rules, and tables should have
  readable block spacing without leaving trailing blank transcript rows.
- Must not show internal stop reasons as answer text.

### Thinking

- Rendered as a dim reasoning preview attached to the assistant message.
- Streaming state may animate; completed state may fade unless pinned.
- Pure thinking without visible answer must not be treated as a final answer by
  the agent loop.

### ToolUse

- Rendered as a tool card with a stable title.
- Required fields:
  - tool display name;
  - concise input summary;
  - running / waiting approval / completed / failed state when known.
- MCP-origin tools must show the server source, for example `[server] tool`.
- Empty, `null`, or absent input must render as `(no input)`, not raw protocol
  JSON.
- JSON input should be translated into semantic fields for known tools:
  commands, paths, patterns, ranges, descriptions, task counts, and write/edit
  summaries.

### ToolApproval

- Rendered inline under the tool that needs approval.
- Required fields:
  - action being approved;
  - risk-bearing detail such as command, path, diff summary, URL, or MCP server;
  - Allow / Always / Deny actions;
  - selected action.
- Default rendering must not be a centered modal unless the terminal is too
  small to fit an inline panel.

### ToolResultSuccess

- Rendered grouped under its ToolUse.
- Must show a semantic result, not raw JSON, for P0 tools.
- Long content is bounded and includes an expansion hint.

### ToolResultError

- Rendered grouped under its ToolUse.
- Must show the error message and relevant stderr / exit code / failed path.
- Error styling should be visible without relying solely on color.

### Diff

- Used by Edit / Write / MultiEdit / NotebookEdit results.
- Required fields:
  - path when available;
  - added and removed lines;
  - hunk headers;
  - bounded preview with expansion hint.

### TodoList

- Shows tasks with status, short content, and recent update.
- Must not block later tool rendering.
- Should occupy deterministic layout space or appear as a transcript item.

### SubAgent

- Shows child agent id/name, status, nested tool summary, and final summary.
- Nested approval must be surfaced to the parent transcript when it blocks.

### SystemStatus

- Footer-level information:
  - cwd/project;
  - model;
  - access mode;
  - turn state;
  - cost/context when known;
  - MCP/task summary when relevant.
- Footer should summarize blocking state, not replace transcript details.

## P0 Tool Card Requirements

### Bash

- Show command and cwd when available.
- Show stdout and stderr as separate sections.
- Show exit code, timeout, interrupted, and error state.
- Bound output by default.

### Read

- Show file path and line range / total lines.
- Show line numbers.
- Syntax highlight when extension is known.
- Handle text, binary, image, and error payloads without dumping the raw
  protocol payload.

### Grep / Glob

- Show pattern/path/mode when available.
- Show match count or file count.
- Show matched files or content-mode lines.
- Bound previews and show an explicit truncation hint when the upstream result
  was already shortened.

### Edit / Write / MultiEdit

- Show file path when available.
- Show diff with added/removed coloring.
- Show summary counts when available.
- Show added/removed totals before a diff preview when they can be derived.

### TodoWrite

- Show task statuses.
- Tool completion must not leave the turn visually stuck.

### Task / Agent

- Show child status and final result.
- Nested tool and approval activity must be visible enough that users do not
  think the parent agent froze.

## Snapshot Coverage Rules

- Snapshot tests assert visible text and layout markers, not exact colors.
- Every P0 tool gets at least one normal snapshot and one long/narrow snapshot.
- Approval gets a snapshot showing it below or directly adjacent to tool
  context.
- Snapshots should fail if the normal transcript exposes `null`, bare `stop`,
  broken JSON prefixes, or missing high-signal labels.

## Text Boundary Rules

- User text, model text, tool input, tool output, logs, and transcript previews
  must never be truncated with raw byte slicing such as `&text[..n]`.
- Character-count truncation must use
  `mossen_utils::string_utils::truncate_chars`,
  `truncate_chars_with_suffix`, or `prefix_chars`.
- Byte-budget truncation is allowed only for memory or IO limits, and must use
  `safe_prefix_by_bytes`, `safe_suffix_by_bytes`, or
  `truncate_bytes_with_suffix` so UTF-8 boundaries are preserved.
- Terminal-width clipping may be conservative, but it must be UTF-8 safe before
  visual width polish. A clipped Chinese prompt must degrade to a shorter
  string, not panic.
- New rendering code should treat direct string slicing as parsing-only. If the
  source is user/model/tool text rather than a known ASCII token, add a helper
  call and a regression test.
