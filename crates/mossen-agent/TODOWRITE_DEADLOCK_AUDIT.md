# TodoWrite / TaskList Deadlock Audit

Date: 2026-05-20

## Scope

This audit covers the "TodoWrite / TaskList appears stuck" report from the
MiniMax-M2.7 smoke runs recorded in:

- `/Users/allen/Library/Caches/mossen/logs/mossen-88551.log`
- `/Users/allen/Library/Caches/mossen/logs/mossen-92795.log`

## Observed Behavior

- `mossen-88551.log` shows `TodoWrite` emitted at `2026-05-20T12:26:59Z`,
  followed by another model call at `12:27:09Z`. The tool did return and the
  dialogue loop continued.
- The same run shows `TaskList` emitted at `12:27:17Z`, then a long visible gap
  before the next model call at `12:29:06Z`.
- `mossen-92795.log` shows a `Task` call at `12:30:25Z`, then a nested `Bash`
  call at `12:30:43Z`. The actual shell execution starts at `12:31:24Z`, which
  matches a user-facing permission wait rather than a tool execution hang.

## Root Cause

There were two overlapping issues that made a live run look deadlocked:

1. Tool results were not rendered as clear transcript rows. When a tool
   completed, the user often saw the assistant pause with no visible evidence
   that the tool had finished.
2. The permission gate previously treated every tool as requiring approval.
   Read-only or internal bookkeeping tools such as `TaskList` could therefore
   block behind a permission prompt, even though they should execute directly.

The `TodoWrite` tool itself does not need user approval and is implemented with
`needs_permission() == false`. `TaskList` is read-only and therefore resolves to
`needs_permission() == false` through the tool registry.

## Fixes Applied

- The dialogue permission gate now asks the registry whether a tool actually
  needs permission before calling the interactive gate. Read-only tools and
  `TodoWrite` skip the modal and execute immediately.
- The registry resolves `Task` as an alias for `Agent`, so MiniMax-compatible
  task calls do not fail or stall on a missing tool name.
- The TUI now emits visible `ToolUseSummary` rows for tool results and renders
  specialized cards for `Bash`, `Read`, `Grep`, `Glob`, `Agent`, `Edit`,
  `Write`, `NotebookEdit`, and `MultiEdit`.
- Permission prompts now show structured command/path/diff previews instead of
  only a generic tool name.
- Ctrl+C now cancels an in-flight turn before modal routing, closes the active
  modal, denies any outstanding permission oneshot, and leaves a visible
  `↯ Cancelled` transcript marker.

## Residual Risk

Dropping the TUI receiver cancels the visible turn state, but a background model
request that was already in flight may still finish on the spawned task. Its
messages are no longer rendered once the receiver is dropped. A future deeper
fix should keep a cancellation token or join handle in the TUI so Ctrl+C can
abort the background task directly.

## Verification

The expected verification set after these fixes is:

- `cargo test -p mossen-tui ctrl_c`
- `cargo check -p mossen-tui -p mossen-agent`
- A MiniMax-M2.7 TTY smoke that exercises `TodoWrite`, `TaskList`, `Task`/`Agent`,
  and a nested `Bash` permission request.
