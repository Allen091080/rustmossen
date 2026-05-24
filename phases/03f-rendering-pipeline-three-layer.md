# TUI Rendering Pipeline Three-Layer Plan

Status: in progress - Batch 3 semantic approval sidecar plus Layer 1 engine-id/result-id bridge complete, Batch 4 RendererProfile and transcript-only renderer entry complete for the active transcript, Batch 5 height-cache/panic-boundary first slices complete, active product render contract gate expanded, tall-block virtualization, scratch-boundary slicing, usize virtual-height, async-append manual-scroll, virtual-row height consistency, and UTF-8 input/search boundary fixes complete, old translated renderer trees removed
Owner: mossen-tui
Created: 2026-05-21

## Goal

把当前 TUI 从“各组件直接理解 agent/message/tool 原始数据并各自渲染”收敛为三层：

1. AgentEvent / Message layer
2. Semantic RenderModel layer
3. Responsive Terminal Renderer layer

目标不是重写 UI，而是建立一条稳定、可测试、可迁移的渲染管线。旧
`root_*`/翻译兼容渲染树已退出；后续只能在三层管线和活动 widgets 上继续做。

## Current Diagnosis

现在 TUI 渲染处于 beta 中段，主要问题是：

- `App` 同时负责 agent loop 状态、消息转换、审批状态、footer 状态、滚动、渲染触发。
- `MessageData` 既像 transcript 数据，又像 widget props，还承载 tool result 展开状态。
- `widgets/message.rs` 已收缩为数据模块，旧 message-row 渲染岛已退出编译；当前 active transcript path 是 `MessagesWidget -> RenderTranscript -> RenderBlockWidget`（`widgets/render_block.rs`）。旧翻译组件树已删除，Message selector、idle return、approval、footer、task panel、teammate spinner 等活动入口都已迁到 `widgets/` 或 `approval_state.rs`。
- 三种 root viewport 各自截断、布局、预览文本，导致修一个 bug 容易在另一个 viewport 复发。
- 状态优先级没有统一模型，之前出现过 idle dialog 盖住 approval、streaming 空转 5000s、中文 prompt byte slicing panic。
- 快照测试已经开始覆盖 P0；审批决策已从 App sidecar 进入
  `render_lifecycle::TranscriptRecords`，再以 record-native 方式进入
  RenderModel。完整的 turn/tool/approval lifecycle reducer 还没收束。
- 工具块的兼容锚点已稳定到 `tool-{ToolUse source index}`，旧
  `tool-x-y` 锚点仍作为历史别名被接受。
- `RenderTranscript::from_records` 已不再整批回退到 `MessageData` 列表，
  block id、source index、tool pairing、approval anchor 都以
  `TranscriptRecord` 为准。
- engine `ToolUseBlock.id` 已在 App 边界保留下来，并作为 Layer 1
  record id override 输入 RenderModel；工具卡和审批锚点不再只能依赖
  message 下标。
- engine `ToolResultBlock.tool_use_id` 现在会进入 TUI 的
  `SdkMessage::ToolUseSummary`，App 会为 ToolResult 记录稳定 result id
  和 parent tool-use id；`RenderTranscript::from_records` 可以用 parent id
  配对 tool use/result，而不只靠相邻消息猜测。
- `MessagesWidget` 已能在没有 backing `MessageData` slice 的情况下渲染
  supplied `RenderTranscript`；高度计算、unseen divider、折叠 tool result
  隐藏逻辑都改为读 semantic block/source index，而不是读 raw message
  type。
- 新增 active product render contract：`crates/mossen-tui/tests/render_contract.rs`
  通过 `App::render_for_test` 覆盖真实 frame，而不是只测快照或孤立
  widget。它已经覆盖产品状态矩阵、inline approval/footer 顺序、协议噪声
  清理、高度缓存一致性、病态内容、resize storm、long-transcript scroll、
  rich Markdown/code/table/diff、streaming/resize interleave、
  deterministic fuzz corpus、single-tall-message virtualization、scratch
  boundary slicing、u16 cap overflow regression、streaming tall-message scroll
  policy、async append manual row-scroll preservation、large-session budget。
- Layer 2 现在会在语义模型边界清理所有可见文本里的 ANSI 控制序列并
  展开 tab，覆盖 assistant/user/thinking/tool sections，避免把终端控制符
  留给 Layer 3 临场补救。
- Layer 3 现在有 oversized text-block 虚拟行兜底：当单个 assistant
  回复高到安全 scratch buffer 无法覆盖当前 scroll offset 时，
  `MessagesWidget` 会按 semantic rows 直接绘制可见切片，避免粘底时停在
  中间行。
- Layer 3 的 oversized text-block 兜底现在覆盖跨边界窗口：当
  `skip + visible_height` 超过 scratch buffer 高度时，整段可见窗口直接
  使用 semantic virtual-row slice，避免前半屏有内容、后半屏空白。
- Layer 3 高度模型现在保持 `usize` 虚拟高度：单个 assistant block
  超过 `u16::MAX` 行时，scroll model 不再被 ratatui buffer 高度上限截断。
  `u16` 上限只允许出现在临时 scratch buffer 绘制阶段。
- 流式超长单条回复也进入 active contract：sticky-bottom 必须跟住增长尾部，
  用户手动上滚时新增 chunk 不能抢回底部，`Ctrl+L` 才恢复到底部。
- App 事件层不再用消息条数覆盖行滚动总量。异步追加系统消息、工具结果、
  重试提示等 transcript mutation 只更新 transcript 辅助状态；实际
  `VirtualScroll.total_items` 只能由 `sync_message_scroll()` 根据
  `RenderSurface` 测量出的语义行高写入。
- Layer 3 的 semantic virtual-row fallback 现在和正常高度测量共享同一
  高度合同。新增契约先暴露出 focused 窄宽 rich Markdown 块测量为 41
  行、虚拟行源只有 39 行；修复后虚拟行生成会按
  `wrapped_line_count_for_text` 补齐每个 segment，避免 deep-scroll 切片
  与 scroll total 漂移。
- 搜索和旧文本输入 hook 已补 UTF-8 边界合同。之前这些路径按 byte
  offset 左右移动、删除、kill 文本，中文 query/prompt 会把 cursor 留在
  多字节字符中间，下一次 `remove`/`drain`/slice 可能 panic。现在
  `SearchInputState` 和 `hooks/text_input::TextCursor` 在移动、删除、
  kill-to-start/end、word deletion 前都会 clamp 到合法 char boundary；
  已用 hook 单测、搜索浮层 App snapshot、审批中文截断回归验证。

## Target Architecture

```text
AgentEvent / Message
  - raw stream events
  - message append/update events
  - tool lifecycle events
  - approval events
  - footer/runtime state
          |
          v
Semantic RenderModel
  - transcript blocks
  - tool cards
  - approval anchors
  - footer model
  - overlay/blocking state
  - viewport-independent semantics
          |
          v
Responsive Terminal Renderer
  - large/medium/small viewport layout
  - wrapping, clipping, folding
  - markdown/code/diff rendering
  - scroll/virtualization
  - visual style/theme
```

## Layer 1: AgentEvent / Message

职责：

- 只表达事实，不关心视觉。
- 保留 raw data，但不把 raw JSON 直接交给 UI 展示。
- 对每个 turn、message、tool call、approval 建立稳定 id。
- 区分 lifecycle：queued, streaming, waiting_approval, running_tool, completed, failed, cancelled, timed_out。
- 对 custom backend 的 stream 事件做语义归一：文本 delta、thinking delta、tool call start/delta/end、usage、finish reason。

禁止：

- 在这里做 terminal-width clipping。
- 在这里生成 box/border/footer 文案。
- 在这里把 protocol stop/null 当作可见文本。

产物：

- `AgentEvent` / `UiEvent` 输入定义。
- `MessageRecord` / `ToolRecord` / `ApprovalRecord` 状态表。
- turn-level lifecycle reducer。

## Layer 2: Semantic RenderModel

职责：

- 把 Layer 1 的事实转换为 viewport-independent 的语义 UI 模型。
- 统一决定“用户应该看到什么”，但不决定“占几列、画什么边框”。
- 把 tool input/output 转成语义 card，而不是 raw JSON。
- 把 approval 作为 transcript 下方的 anchored block，而不是全局 modal 默认抢占。
- 把 footer 作为状态摘要模型，避免各 viewport 各自拼字符串。
- 清理 protocol noise：`null`、`stop`、`terminal=Completed`、空 tool result、内部 tag。
- 清理可见文本控制序列：ANSI escape、tab 对齐等必须在语义层归一，不能
  只依赖某个 widget 的裁剪逻辑。
- 所有用户/模型/工具文本的截断统一走 `mossen_utils::string_utils`。
- 搜索/输入状态即使仍保存 byte offset，也必须在所有移动、删除、截断、
  kill/yank 边界处先归一到合法 UTF-8 char boundary。

核心类型：

- `RenderTranscript`
- `RenderBlock`
- `RenderNode`
- `ToolCardModel`
- `ApprovalRenderModel`
- `FooterRenderModel`
- `RenderState`
- `ViewportHints`

禁止：

- 使用 raw byte slicing 截断用户/模型/工具文本。
- 在旧 root/组件模块里重新解析 tool JSON。
- 在 widget 内部判断 tool lifecycle。

产物：

- `crates/mossen-tui/src/render_model.rs`
- MessageData 兼容 adapter：`RenderTranscript::from_messages`
- 后续 reducer adapter：`RenderTranscript::from_app_state`
- semantic snapshot tests。

## Layer 3: Responsive Terminal Renderer

职责：

- 只消费 RenderModel。
- 根据 viewport 选择布局策略：large, medium, small。
- 处理 terminal width、CJK、emoji、ANSI、markdown、code block、diff、folding、scroll。
- 统一虚拟滚动和 sticky-bottom 策略。
- 工具卡、审批卡、footer 使用同一套 semantic props。

布局策略：

- Large: transcript + side/status affordances; richer tool sections.
- Medium: transcript-first; tool cards bounded; approval inline; compact footer.
- Small: minimal but complete; no protocol leakage; all blocking state visible.

禁止：

- Renderer 直接读取 agent raw event。
- Renderer 直接解析 tool JSON。
- Renderer 直接处理 backend finish reason。

产物：

- `RendererProfile`
- `TranscriptRenderer`
- `ToolCardRenderer`
- `ApprovalRenderer`
- `FooterRenderer`
- viewport matrix snapshot。

## Execution Batches

### Batch 1: Contract And RenderModel Skeleton

Status: completed

Tasks:

- Add this plan file.
- Add `render_model` module with the stable semantic types.
- Add `RenderTranscript::from_messages` compatibility adapter.
- Add tests for:
  - user/assistant/tool messages become semantic blocks;
  - protocol-only tool noise is hidden;
  - tool use/result lifecycle maps into `ToolPhase`;
  - multibyte content survives model conversion.
- Export the module from `mossen-tui`.

Acceptance:

- `cargo fmt -p mossen-tui`
- `cargo test -p mossen-tui render_model -- --nocapture`
- Existing render snapshots still pass.

### Batch 2: Tool Card Semantic Normalization

Status: completed

Tasks:

- Move tool input/output interpretation out of widget rendering into RenderModel builders.
- Normalize P0 tools:
  - Bash
  - Read
  - Grep
  - Glob
  - Write
  - Edit
  - MultiEdit
  - TodoWrite
  - Task / Agent
  - MCP tools
- Represent cards with sections:
  - summary rows;
  - command/input rows;
  - stdout/stderr/result sections;
  - risk/approval section;
  - truncation/expand affordance.
- Keep semantic parsing in `RenderModel`; do not reintroduce tool JSON parsing
  into root or message widgets.

Acceptance:

- Snapshot tests verify no raw JSON keys for normal tool cards.
- Tool cards in large/medium/small derive from the same `ToolCardModel`.
- `rg` confirms P0 tool JSON parsing is not duplicated in root renderers.

### Batch 3: Approval And Blocking State Model

Status: in progress - RenderSurface Batch A complete, approval semantic sidecar and Layer 1 record skeleton complete

Tasks:

- Introduce `ApprovalRenderModel` anchored to the preceding tool block when possible.
- Model blocking state priority:
  1. active approval;
  2. active error requiring user acknowledgement;
  3. cost/rate-limit threshold;
  4. idle return;
  5. informational overlays.
- Ensure footer summarizes the same blocking state without hiding it.
- Migrate MCP approval and Bash approval to the same model path.

Acceptance:

- Approval can never be hidden behind idle/welcome/cost dialogs.
- Snapshot covers approval under tool card in narrow and wide viewport.
- Engine tests cover approval priority and dismissal behavior.

### Batch 4: Responsive Renderer Migration

Status: in progress - RendererProfile first slice complete, old message-row renderer retired

Tasks:

- Add renderer profile abstraction:
  - large;
  - medium;
  - small.
- Move root viewport differences into profile/layout code.
- Make transcript renderer consume `RenderTranscript` only.
- Keep old widgets behind compatibility wrappers until snapshots match.
- Add bounded layout utilities for:
  - markdown;
  - code blocks;
  - diff;
  - long shell output;
  - CJK/emoji text.

Acceptance:

- old root/component modules no longer parse semantic message content.
- Render snapshots pass for 60, 80, 120, 160 columns.
- Long transcript scroll remains stable.

### Batch 5: Performance, Virtualization, And Production Hardening

Status: started - height cache, top-level render panic boundary, and active product render contract first slices complete

Tasks:

- Cache render-model block height by:
  - block id;
  - width;
  - expansion state;
  - theme/render profile.
- Add large transcript fixture:
  - 1000+ messages;
  - long Bash output;
  - markdown/code/diff;
  - nested agent/tool blocks.
- Measure render time and memory.
- Avoid full transcript re-layout when only footer/spinner changes.
- Add lint/static scan for dangerous user-text slicing.
- Add property/fuzz tests for:
  - CJK;
  - emoji;
  - ANSI;
  - invalid/partial JSON;
  - huge tool output;
  - terminal resize.
- Add panic boundary around top-level render with structured error block.
- Define release gate checklist.

Acceptance:

- Large transcript render does not noticeably stall.
- Height cache invalidates correctly on width/expand/content changes.
- Snapshot and performance tests run in CI-friendly time.
- No panic on malformed tool payloads.
- No panic on multibyte truncation.
- No hidden approval.
- No visible protocol noise in normal transcript.
- No unbounded output card in default view.
- `cargo test -p mossen-tui render_snapshot -- --nocapture` is required before release.
- `cargo test -p mossen-tui --test render_contract -- --nocapture` is required
  before any claim that App/frame rendering is product-ready.

## Latest Evidence

- 2026-05-22: Active product render contract expanded slice:
  - added `crates/mossen-tui/tests/render_contract.rs` as a non-snapshot gate
    that renders through `App::render_for_test` and `ratatui::TestBackend`;
  - product-state matrix covers streaming transcript, inline shell approval,
    help/MCP/tasks dialogs, footer model, command suggestions, task/subagent
    panels, and P0 tool cards;
  - resize-storm/pathological fixture reuses one `App` across 132/44/100/24/
    160/32/72/40-column frames with ANSI, tabs, CJK, combining characters,
    long code, malformed Read input, and failing Bash output;
  - long transcript scroll fixture proves sticky-bottom reaches the final
    answer, manual scroll-up can reach the head, and returning to bottom shows
    the tail again through the same active App frame;
  - streaming/resize interleave fixture feeds `SdkMessage::StreamEvent`
    deltas through `App::handle_engine_message`, renders between chunks at
    changing sizes, and requires visible assistant content once a
    content-bearing delta has arrived;
  - deterministic fuzz corpus renders active App frames with ANSI, tabs, CJK,
    combining marks, long unbroken text, malformed known-tool payloads, and
    mixed tool cards;
  - rich-content active App fixture renders Markdown headings/lists, fenced
    Rust code, Markdown tables, and Bash stdout diff output; the contract fails
    if raw Markdown fence/table delimiter syntax leaks into the normal
    transcript;
  - large-session budget fixture renders a 900-message transcript over several
    viewport sizes and fails on obvious active-path stalls;
  - single-tall-message fixture first failed by showing a mid-message
    scratch-buffer slice instead of the real tail; fixed in
    `widgets/messages.rs` by direct semantic virtual-row slicing for oversized
    text blocks;
  - scratch-boundary fixture first failed by showing
    `tall-single-row-0405` through `tall-single-row-0408` followed by blank
    rows when the visible window crossed the safe scratch buffer; fixed by
    using semantic virtual-row slicing when `skip + visible_height` exceeds
    scratch height;
  - enormous single-message fixture first failed by stopping around
    `enormous-single-row-32766` and missing the real tail beyond `u16::MAX`;
    fixed by changing `RenderBlockWidget::required_height` and
    `RenderHeightCache` to keep `usize` virtual heights;
  - streaming tall-message fixture feeds 900+ text deltas through
    `App::handle_engine_message` and proves sticky-bottom/manual-scroll/Ctrl+L
    scroll policy on the active App path;
  - async-append manual-scroll fixture first failed by clamping
    `VirtualScroll.offset` from 2390 to 0 after `SdkMessage::ApiRetry`
    appended a system message while the user was reading a long assistant
    block; fixed by replacing App transcript-mutation
    `set_total_items(messages.len())` calls with `note_transcript_changed()`,
    leaving row totals to `sync_message_scroll()` and measured semantic
    content height;
  - virtual-row height consistency fixture first failed for focused,
    12-column rich Markdown because `RenderBlockWidget::required_height()`
    measured 41 rows while the semantic virtual-row fallback produced 39;
    fixed by padding fallback rows to the same wrapped height estimator;
  - tall rich Markdown active App fixture covers deep-scroll rendering for
    CJK text, fenced Rust code, Markdown tables, and tail anchors without
    leaking raw Markdown fences/table delimiter syntax;
  - the contract asserts product invariants: no panic/banner text, no raw
    protocol text, no visible ANSI escapes, approval above prompt/footer,
    malformed payload details hidden, rich content rendered structurally,
    streamed assistant content visible, height cache deterministic, scroll
    offset bounded after resize, and large sessions within the local smoke
    threshold;
  - the active contract exposed an ANSI cleanup gap in Layer 2. Fixed by
    sanitizing all visible semantic text in `render_model.rs`, before Markdown
    or tool layout sees it;
  - verified `cargo test -p mossen-tui --test render_contract -- --nocapture`,
    `cargo test -p mossen-tui --lib strips_ansi -- --nocapture`, and
    `cargo test -p mossen-tui render_snapshot -- --nocapture`.
- 2026-05-21: Added Layer 3 `RenderHeightCache`:
  - keys include block id, block signature, width, `RendererProfile`, theme,
    and margin/thinking/focus/collapsed flags;
  - `App` passes the same cache into scroll-height accounting and transcript
    rendering;
  - `large_transcript_height_cache_reuses_layouts` exercises 1100 render
    blocks and 6749 rows; first pass records 1100 misses, second pass records
    1100 hits, with local elapsed time of 26ms.
- 2026-05-21: Hardened malformed known-tool payload handling:
  - malformed JSON-looking Bash/Read/Grep/Glob/Write/Edit/MultiEdit/TodoWrite/
    Task/Agent payloads now render as semantic `malformed input/output`
    sections instead of leaking raw JSON keys into normal transcript;
  - snapshot coverage spans 36/80/132 columns with CJK, emoji, markdown code,
    malformed Bash output, and malformed Task input.
- 2026-05-21: Top-level render panic boundary now suppresses the panic hook
  while drawing and restores it after, so caught render panics do not print a
  Rust panic banner into the active TUI.
- 2026-05-21: Retired the old compiled `MessageRenderer` / `MessageRowWidget`
  translated terminal translation path:
  - `widgets/message.rs` now only defines `MessageData`, `MessageType`, and
    `display_tool_name`;
  - the live transcript renderer remains
    `MessagesWidget -> RenderTranscript -> RenderBlockWidget`.
- 2026-05-21: Root UTF-8 hardening first slice:
  - replaced byte slicing in root snippets, truncation, token/API-key display,
    secret redaction, global search file suffixes, markdown table alignment,
    and file-path links with UTF-8 boundary-safe helpers;
  - added tests for multibyte root helper paths and small-root path display.
- 2026-05-21: Continued legacy component retirement:
  - deleted the unused translated message compatibility module;
  - removed `root_large`'s unused `VirtualMessageList`, translated
    scroll-handler, `MessagesState/MessagesWidget`, message-actions nav, and
    `MessageRowState/MessageRowWidget` sections;
  - kept `MessageSelectorState`, `RenderableMessage`, and
    `MessageSelectorWidget` because the live app still uses them;
  - routed `open_message_selector` through `RenderTranscript` and
    `RenderBlock::selector_summary()` so selector rows use semantic summaries
    instead of raw message/tool JSON;
  - added `message_selector_uses_semantic_render_summaries` to prove Bash tool
    JSON does not leak through the selector modal.
- 2026-05-21: Retired small/medium root compatibility islands:
  - removed unused `retired_compact_root.rs` from the compiled component tree;
  - reduced `root_medium.rs` from a broad translated terminal-translated component grab bag
    to the only live surface: `IdleReturnDialogState` and
    `IdleReturnDialogWidget`;
  - verified that remaining `root_medium` call sites are only app service
    state creation, app modal rendering, and tests.
- 2026-05-21: Sticky-scroll and UTF-8 active-path hardening completed:
  - added `render_snapshot_app_frame_sticky_scroll_follows_long_transcript_tail`
    after reproducing that a long Chinese Markdown transcript did not keep the
    final conclusion visible in sticky-bottom mode;
  - replaced Markdown/plain/tool text height estimation with a grapheme-aware
    word-wrap helper shared by `MarkdownWidget` and `RenderBlockWidget`;
  - hardened active truncation paths in utils/session/task/title/storage code
    to use UTF-8 boundary-safe helpers instead of raw byte slicing;
  - removed unwired `semantic_adapters/*` files whose tests were not part of
    the compiled model-runtime path;
  - verified `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui render_model -- --nocapture`,
    `cargo test -p mossen-agent multibyte -- --nocapture`,
    `cargo test -p mossen-utils multibyte -- --nocapture`,
    `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, `git diff --check`, and
    `cargo check -p mossen-cli`.

## Migration Rules

- Do not keep retired render paths once their behavior is represented in RenderModel tests.
- Prefer adapter-first migration: old `MessageData` can feed RenderModel while app state is gradually split.
- Keep snapshots high-signal, not pixel-perfect.
- Every moved behavior needs one semantic test and one renderer snapshot when it affects visible output.
- Do not add new parsing to legacy root/component modules.

## Production Definition

TUI rendering can be called production-ready only when:

- agent loop state and visible UI state are separated;
- all visible transcript rows come from RenderModel;
- approvals are inline and cannot be hidden;
- footer is a summary of RenderModel state, not a separate source of truth;
- text truncation is UTF-8 safe by construction;
- viewport snapshots cover normal, narrow, long-output, Chinese, and approval cases;
- long-session performance is measured;
- top-level render cannot crash the process on bad model/tool content.
- active App render contract passes across product state, pathological content,
  and resize-storm matrices.

## Progress Log

- 2026-05-21: Plan created and Batch 1 started.
- 2026-05-21: Batch 1 skeleton landed:
  - added `crates/mossen-tui/src/render_model.rs`;
  - exported `render_model` from `mossen-tui`;
  - added compatibility adapter `RenderTranscript::from_messages`;
  - added semantic tests for message conversion, protocol noise filtering, tool
    lifecycle mapping, and multibyte content preservation.
- 2026-05-21: Batch 2 started:
  - added initial P0 tool normalization in RenderModel for Bash, Read,
    Grep/Glob, Write/Edit/MultiEdit, TodoWrite, and Task/Agent;
  - known JSON tool payloads now map to semantic sections instead of being
    represented only as raw JSON inside the model.
- 2026-05-21: Batch 2 completed and Batch 4 migration started:
  - `RenderTranscript::from_messages` now merges adjacent `ToolUse` and
    `ToolResult` records into one semantic tool card with stable source
    indices;
  - `MessagesWidget` now builds visible rows from `RenderTranscript` and
    renders through `RenderBlockWidget`, so the live app path no longer
    renders transcript rows directly from raw `MessageData`;
  - assistant content uses Markdown rendering, system/progress rows preserve
    plain line scrolling, and tool cards have bounded previews;
  - Bash stdout/stderr, Grep/Glob search output, Read text/image/error, Write,
    Edit/MultiEdit, TodoWrite, Task/Agent, and null tool input now normalize
    before terminal rendering;
  - default tool section previews are capped to keep one long result from
    hiding later cards in the viewport;
  - regression coverage added for semantic model conversion, bounded tool card
    height, sticky bottom scrolling, buffer clipping, long Bash output, and
    snapshot-level protocol-noise checks.
- 2026-05-21: Superseded pending list now closed by later slices:
  - approval anchoring moved into `ApprovalRenderModel`;
  - footer state now renders from `FooterRenderModel`;
  - active viewport/profile coverage exists through App-level render snapshots;
  - old translated root and component UI islands have been removed from the TUI tree.
- 2026-05-21: TUI terminal framework cleanup completed:
  - removed the unused `crates/mossen-tui/src/terminal-framework` translated renderer tree;
  - removed the `pub mod terminal-framework` export from `mossen-tui`;
  - migrated the last small terminal chrome states in `app_services.rs`
    (`search highlight`, `terminal title`, `tab status`, `terminal focus`,
    and terminal notification escaping) into local ratatui-side state;
  - removed unused terminal-framework color compatibility helpers
    conversion leftovers after confirming they had no repo call sites;
  - removed additional unused translated terminal compatibility islands:
    `mossen-utils::render_options`, `mossen-utils::export_renderer`,
    `mossen-utils::static_render`, and the CLI-only `native_yoga` module;
  - scrubbed stale TUI comments that still described modules as terminal framework
    translations, so future rendering work reads from the current ratatui and
    RenderModel architecture instead of the old source mapping.
  - verified retired renderer/helper/file-name residual scans across `crates/`
    returns no remaining code hits.
- 2026-05-21: Cleanup self-check completed:
  - `cargo test -p mossen-utils --lib --no-run` passed, confirming deleted
    utility exports do not leave lib-test compile references;
  - `cargo test -p mossen-cli --bin mossen --no-run` passed, confirming the
    CLI binary no longer depends on `native_yoga` or `createRoot` compatibility
    shims;
  - `cargo test -p mossen-tui --lib --no-run` passed;
  - `cargo test -p mossen-tui render_snapshot -- --nocapture` passed;
  - `cargo test -p mossen-tui app_services -- --nocapture` passed;
  - `cargo test -p mossen-tui render_model -- --nocapture` passed;
  - filename checks only produced false positives such as `blink`,
    `hyperlink`, `thinking`, and `sinks`; no retired terminal framework module or retired renderer
    compatibility island remains under `crates`.
- 2026-05-21: Product-grade rendering plan started:
  - added `phases/03g-rendering-product-grade-plan.md` as the durable
    no-placebo execution record with Mossen Code / Codex CLI source references;
  - introduced `RenderSurface`, `BlockingRenderModel`, footer blocking state,
    and semantic approval action labels in `render_model.rs`;
  - added `widgets::approval::ApprovalBlockWidget`, so inline approval renders
    from `ApprovalRenderModel` instead of directly from permission prompt state;
  - routed Bash/file/MCP approval panels through the same semantic approval
    model path;
  - routed the status bar through `FooterRenderModel` first, keeping the legacy
    status widget as a renderer adapter rather than the source of truth;
  - added semantic and snapshot coverage for approval/footer/blocking agreement.
- 2026-05-21: Batch A RenderSurface wiring completed:
  - `App::render_frame` now builds one `RenderSurface` per frame and passes it
    into fullscreen/inline rendering;
  - transcript rendering, scroll-height accounting, inline approval, footer,
    and spinner blocking labels consume that same surface;
  - tool approval anchors now resolve against `RenderTranscript` block ids when
    compatibility message data is the only lifecycle source available;
  - added semantic and frame snapshot coverage for Bash/file/MCP approvals plus
    cost, idle, and error blocking states.
- 2026-05-21: Batch 3 decision history first slice completed:
  - added `ApprovalDecisionModel`, `ApprovalDecisionKind`,
    `RenderNode::ApprovalDecision`, and
    `RenderBlockKind::ApprovalDecision`;
  - approval allow/always/deny/cancel paths now leave a compact semantic
    transcript decision block instead of disappearing with only modal state;
  - MCP channel allow/deny now uses the same visible decision block path;
  - compatibility storage still uses a hidden
    `mossen-render:approval-decision:` marker because true Layer 1 approval
    lifecycle records are not in place yet;
  - added semantic and snapshot coverage proving the marker is not visible in
    normal transcript rendering.
- 2026-05-21: Batch 3 semantic approval sidecar slice completed:
  - added `App::approval_decisions`, so new approval decisions are no longer
    written as hidden system-message marker content;
  - added `RenderTranscript::from_messages_and_decisions`, which merges App
    sidecar approval facts into the semantic transcript and inserts anchored
    decisions immediately after the matching tool block;
  - kept hidden `mossen-render:approval-decision:` parsing only as a backward
    compatibility adapter for existing message data;
  - routed frame rendering and message selector rows through the
    messages-plus-decisions adapter;
  - added `sidecar_approval_decision_is_inserted_after_anchor_block` and kept
    marker compatibility coverage as an explicit regression.
- 2026-05-21: Layer 1 record-boundary slice completed:
  - added `crates/mossen-tui/src/render_lifecycle.rs` as the compatibility
    Layer 1 boundary for transcript facts and approval decisions;
  - moved approval-decision persistence types into Layer 1, with
    `render_model` keeping re-exports for existing callers;
  - `ApprovalDecisionModel` now has a stable session-local `id`, allocated by
    `App::next_render_record_seq`;
  - `RenderTranscript::from_messages_and_decisions` now builds
    `TranscriptRecords` before constructing semantic Layer 2 blocks;
  - hidden approval marker messages are extracted into Layer 1 approval facts,
    so they remain historical compatibility data instead of normal transcript
    messages;
  - tool transcript block ids now stay stable as `tool-{source index}` before
    and after the matching result arrives, while old `tool-x-y` anchors remain
    accepted for historical approval records;
  - verified `cargo test -p mossen-tui render_lifecycle -- --nocapture`,
    `cargo test -p mossen-tui tool_anchor_id_stays_stable_when_result_arrives -- --nocapture`,
    `cargo test -p mossen-tui render_model -- --nocapture`,
    `cargo test -p mossen-tui accepted_tool_permission_persists_as_semantic_decision_block -- --nocapture`,
    and `cargo test -p mossen-tui render_snapshot -- --nocapture`.
- 2026-05-21: Layer 1 record-native bridge slice completed:
  - `RenderTranscript::from_records` now iterates `TranscriptRecord` entries
    directly instead of rebuilding a temporary `MessageData` list;
  - tool pairing uses adjacent `TranscriptRecordKind::ToolUse` and
    `TranscriptRecordKind::ToolResult` records, preserving the ToolUse record
    id as the visible tool block id;
  - render blocks preserve Layer 1 `source_index` values even when protocol
    compatibility records are filtered;
  - approval decisions can anchor to Layer 1 record ids such as
    `tool-call-shell-42`, with old `tool-x-y` aliases retained for historical
    data;
  - added `render_model::tests::from_records_uses_layer1_ids_source_indices_and_anchors`;
  - verified `cargo test -p mossen-tui from_records_uses_layer1_ids_source_indices_and_anchors -- --nocapture`
    and `cargo test -p mossen-tui tool_anchor_id_stays_stable_when_result_arrives -- --nocapture`.
- 2026-05-21: Layer 3 diff-rendering slice completed:
  - tool output sections now detect unified diff bodies before Markdown/plain
    fallback rendering;
  - diff headers and hunk headers use info styling, additions use success
    styling, and removals use error styling;
  - this remains a Layer 3 responsibility: Layer 2 emits semantic tool
    sections, while terminal rendering chooses the diff-shaped text styling;
  - added `widgets::render_block::tests::renders_unified_diff_sections_with_diff_semantics`;
  - extended `render_snapshot_search_read_and_diff_polish` with a Bash stdout
    unified diff and visible hunk/add/remove assertions;
  - verified `cargo test -p mossen-tui renders_unified_diff_sections_with_diff_semantics -- --nocapture`
    and `cargo test -p mossen-tui render_snapshot_search_read_and_diff_polish -- --nocapture`.
- 2026-05-21: Layer 1 engine-id and Layer 3 transcript-only slice completed:
  - App now stores engine `ToolUseBlock.id` values as Layer 1 record-id
    overrides, so tool cards and active/persisted approval anchors can use
    stable engine tool ids;
  - `render_surface_model`, approval anchoring, persisted decisions, and
    approval pruning now build from the same record-aware transcript path;
  - `MessagesWidget` no longer relies on raw `MessageData` type inspection for
    supplied transcript rendering, height measurement, unseen dividers, or
    collapsed tool-result hiding;
  - added `render_lifecycle::tests::applies_record_id_overrides_at_layer1_boundary`;
  - added `app::engine_stream_tests::engine_tool_use_id_flows_into_render_record_and_approval_anchor`;
  - added `widgets::messages::tests::supplied_transcript_renders_without_message_slice`;
  - verified those three focused tests plus
    `cargo test -p mossen-tui render_model -- --nocapture`.
- 2026-05-21: Layer 1 tool-result parent-id slice completed:
  - `SdkMessage::ToolUseSummary` now carries optional `tool_use_id` from the
    actual `ToolResultBlock.tool_use_id` emitted by the agent dialogue loop;
  - App stores stable ToolResult record ids and parent tool-use ids in the same
    Layer 1 override path as engine ToolUse ids;
  - `TranscriptRecord` now carries `parent_id`, and
    `RenderTranscript::from_records` can pair ToolUse/ToolResult records by
    stable parent id rather than adjacency alone;
  - added `render_lifecycle::tests::applies_parent_id_overrides_at_layer1_boundary`;
  - added `render_model::tests::from_records_pairs_tool_result_by_parent_id`;
  - added `app::engine_stream_tests::engine_tool_result_keeps_parent_tool_use_id`;
  - verified those three focused tests plus `cargo test -p mossen-tui -- --nocapture`,
    `cargo build -p mossen-cli --bin mossen`, `cargo fmt -p mossen-agent -p mossen-tui --check`,
    `cargo check -p mossen-tui`, and `git diff --check`.
- 2026-05-21: Batch 4 RendererProfile first slice completed:
  - added `crates/mossen-tui/src/render_profile.rs` with explicit
    `RendererProfile::{Small, Medium, Large}` selection;
  - `MessagesWidget` passes the viewport-derived profile into
    `RenderBlockWidget`;
  - tool card preview budgets now vary by renderer profile instead of a single
    hard-coded renderer constant;
  - added 60/80/120/160-column snapshot coverage for core transcript,
    markdown, tool, and approval-decision semantics.
- 2026-05-21: Batch 5 panic-boundary first slice completed:
  - production `terminal.draw` now renders through `render_frame_safely`;
  - top-level render panics are caught and replaced with a structured
    `Render error` frame instead of aborting the process;
  - `render_for_test` uses the same boundary so future snapshot tests exercise
    the protected path;
  - added a regression test that injects a synthetic render panic and confirms
    the terminal draw returns with an error panel instead of unwinding.
- 2026-05-21: Layer 3 terminal-cell correctness slice completed:
  - active `TextInputWidget` no longer hides the first placeholder character
    under the cursor and keeps long multibyte input scrolled to the cursor/tail;
  - active `PromptInputWidget` lays out prefix, mode indicator, and suggestions
    with terminal display width instead of byte/string length;
  - active `StatusBarWidget` preserves right-side cost/message metrics when
    the left side contains wide project/model text;
  - active `SpinnerRowWidget` advances shimmer/status text by terminal cell
    width, avoiding CJK cell overlap while streaming;
  - active App chrome now reports `streaming` consistently from
    `state.is_streaming`, so a frame with the spinner visible cannot still
    label itself `idle` unless the turn has actually settled;
  - added an App-level snapshot for CJK prompt tail scrolling, spinner text,
    model, cost, message count, and footer hints through `App::render_for_test`;
  - anti-placebo note: retired compatibility modules are not counted as App
    prompt-render evidence;
  - verified focused widget tests, the App-level snapshot, full render
    snapshots, `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 auxiliary-panel terminal-cell slice completed:
  - active teammate/sub-agent right-rail rendering now uses grapheme and
    terminal display width for glimmer messages, teammate names, and teammate
    status messages;
  - added focused tests for CJK glimmer clipping and teammate-line clipping;
  - added an App-level snapshot that renders the teammate panel through
    `App::render_for_test` and `split_auxiliary_panels`;
  - verified `cargo test -p mossen-tui --lib spinner_anim -- --nocapture`,
    `cargo test -p mossen-tui render_snapshot_app_frame_teammate_panel_handles_multibyte_cells -- --nocapture`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 modal/dialog terminal-cell slice completed:
  - active `App::render_modal` help and MCP dialogs now use terminal display
    width for command/server/status columns instead of string/byte length;
  - CJK command names, command descriptions, MCP server names, and MCP details
    are padded/truncated by cell width before being turned into ratatui spans;
  - added App-level snapshots
    `render_snapshot_app_frame_help_modal_handles_multibyte_columns` and
    `render_snapshot_app_frame_mcp_modal_handles_multibyte_columns`;
  - verified focused App-frame snapshots, full render snapshots,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 active modal/panel row-width slice completed:
  - `MessageSelectorWidget`, the in-session `Search` modal, `TasksDialog`,
    generic `Picker`, `ModelPickerWidget`, `SkillsPanelWidget`, and
    `MemoryPanelWidget` now budget visible row text by terminal display width
    before ratatui paints the row;
  - this is active-path work: snapshots enter through `App::render_for_test`,
    `App::render_modal`, and `open_message_selector`, not through isolated
    translated widgets;
  - added App-level CJK snapshots for message selector, search, tasks,
    generic picker, and model/skills/memory panels;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 tool-output ANSI and height-consistency slice
  completed:
  - active `MessagesWidget -> RenderTranscript -> RenderBlockWidget` tool
    section rendering now strips ANSI control sequences, expands tabs, and
    clips long output lines by terminal display width;
  - clipped-output hint rows are now measured with the same wrapping logic
    used during render, so CJK-heavy stdout can no longer cause following
    stderr/body content to be hidden by the tool-card bottom border;
  - added active-path regression coverage
    `widgets::render_block::tests::bounded_section_body_strips_ansi_and_clips_by_display_width`
    and `render_snapshot_bash_output_strips_ansi_and_clips_wide_lines`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 auxiliary task-panel row-width slice completed:
  - active task side-panel rendering through `App::render_auxiliary_panels`
    now clips todo content by terminal display width after reserving cells for
    the status icon and spacer;
  - this is active-path evidence only: it does not count retired root task/list
    compatibility widgets as solved;
  - added App-level CJK snapshot
    `render_snapshot_app_frame_task_side_panel_handles_multibyte_cells`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 inline approval height/body slice completed:
  - active inline approval rendering now reserves height through the same
    capped panel width used by `App::render_inline_approval_model`;
  - `ApprovalBlockWidget::required_height` now accounts for every rendered row
    instead of undercounting the body/action/help region;
  - collapsed approval explanations stay visible as one terminal-display-width
    clipped row, and expanded explanations are measured with the same wrapping
    budget used by render;
  - this is active-path evidence only: old modal-overlay permission branches
    are not counted because active approval modals return before overlay
    rendering;
  - added regression coverage
    `widgets::approval::tests::required_height_keeps_collapsed_body_visible`,
    `render_snapshot_app_frame_shows_inline_approval_and_footer_state`, and
    `render_snapshot_semantic_approval_model_matches_inline_surface`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 focused transcript height-budget slice completed:
  - active `MessagesWidget -> RenderBlockWidget` rows now measure focused
    content with the same one-cell focus band that render consumes;
  - this closes a height-cache/virtual-scroll mismatch where a focused row
    could reserve one less terminal row than it actually needed and clip the
    wrapped tail;
  - added regression coverage
    `widgets::render_block::tests::focused_rows_reserve_the_focus_bar_width`
    and `render_snapshot_focused_message_keeps_wrapped_tail_visible`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 markdown wrap-height calibration slice completed:
  - active Markdown/plain transcript height accounting now handles long
    unspaced tokens without repeatedly counting already-consumed token cells;
  - the accepted fix intentionally keeps the CJK/soft-break behavior that
    protects sticky scrolling from undercounting ratatui rows; a broader
    `textwrap` replacement was rejected after the active sticky-scroll fixture
    failed;
  - added regression coverage
    `widgets::markdown::tests::wrapped_height_counts_long_words_by_terminal_width`,
    `widgets::markdown::tests::wrapped_height_combines_styled_spans_before_counting`,
    and `render_snapshot_app_frame_sticky_scroll_follows_long_transcript_tail`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`,
    `cargo test -p mossen-tui --lib -- --nocapture`,
    `cargo check -p mossen-cli`, `python3 scripts/layer_boundary_audit.py`,
    `python3 scripts/smoke_check.py`, and `git diff --check`.
- 2026-05-21: Layer 3 inline prompt suggestion height slice completed:
  - active inline bottom chrome now measures
    `PromptInputWidget::required_height()` in `App::render_inline` instead of
    hard-coding two rows for the prompt;
  - before the fix, an App-level fixture proved that `/` command suggestions
    vanished in inline mode because the prompt widget received no suggestion
    rows;
  - this intentionally does not claim slash command execution is solved; it
    only closes the active render allocation bug for visible command/skill
    suggestions;
  - added failing-before/passing-after regression coverage
    `render_snapshot_app_frame_inline_prompt_shows_command_suggestions`;
  - verified
    `cargo test -p mossen-tui render_snapshot_app_frame_inline_prompt_shows_command_suggestions -- --nocapture`
    and `cargo test -p mossen-tui render_snapshot -- --nocapture`.
- 2026-05-21: Layer 3 semantic footer renderer slice completed:
  - active footer rendering now uses `widgets::footer::FooterWidget` directly
    from `FooterRenderModel`; `App::render_status_bar` no longer adapts the
    semantic footer through any retired widget tree;
  - the footer remains tied to the same `RenderSurface.footer` and
    `RenderSurface.blocking` facts used by transcript, inline approvals, and
    spinner text;
  - tiny-width right metrics now use display-width-safe start truncation so the
    message-count tail remains visible instead of being crowded out by project
    and model text;
  - added regression coverage
    `widgets::footer::tests::footer_widget_renders_semantic_footer_model_directly`
    and `widgets::footer::tests::footer_widget_keeps_right_metrics_visible_when_tiny`;
  - verified `cargo fmt -p mossen-tui --check`,
    `cargo test -p mossen-tui footer_widget -- --nocapture`,
    `cargo test -p mossen-tui render_snapshot -- --nocapture`, and
    `cargo test -p mossen-tui --lib -- --nocapture`.
- 2026-05-21: Layer 3 spinner/stall semantics slice completed:
  - `SpinnerState` now separates total turn elapsed time from idle-since-last
    engine activity time;
  - `App::handle_engine_message` marks activity while streaming so normal long
    answers stay active, while genuinely quiet turns can still show stalled
    coloring after the idle threshold;
  - verified with focused spinner/App tests, full render snapshots, full
    `mossen-tui` lib tests, CLI check, layer-boundary audit, smoke check, and
    whitespace diff check.
- 2026-05-22: Active product render contract slice completed:
  - added a real `App::render_for_test` contract test instead of treating
    snapshots as the whole acceptance story;
  - added a resize-storm/pathological-content contract that exercises
    multi-size rendering on the same `App` with malformed payloads, ANSI,
    tabs, CJK, combining characters, long code, failing Bash output, and
    manual scroll offset;
  - added the long-transcript scroll contract for the "only current screen is
    visible" failure class;
  - moved visible ANSI cleanup into Layer 2 semantic text normalization so
    assistant/user/thinking/tool text is clean before Layer 3 layout;
  - verified the new contract, targeted ANSI semantic tests, and the existing
    render snapshot suite.
