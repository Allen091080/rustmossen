# Phase 3.7: Product-Grade TUI Rendering Plan

Status: active - Batch A complete, Batch B semantic sidecar plus Layer 1 engine-id/result-id bridge complete, Batch C legacy-widget retirement complete for the TUI `components` tree, Batch D hardening started, Batch E height-cache/panic-boundary first slices complete, Batch F active product render contract expanded, tall-block virtualization, scratch-boundary slicing, usize virtual-height, async-append manual-scroll, virtual-row height consistency, UTF-8 input/search boundary hardening fixes, session-local status-line item configuration, persistent status-line render config, external status-line command compatibility, render transcript hot-path cache, dirty-frame render scheduler, adaptive frame pacing, render event backlog coalescing, resize/focus storm coalescing, streaming dirty throttling, keyboard focus scroll anchoring, non-scrollable scroll sticky guard, viewport-sized transcript page navigation, self-induced scrollbar overflow guard, no-op transcript page-key dirty guard, no-op transcript focus-key dirty guard, external statusline in-flight frame guard, focus-change no-op dirty guard, same-size resize no-op dirty guard, plan activity progress counts, plan timeline progress counts, plan process/status progress counts, render hot-path cache diagnostics, generic JSON payload secret redaction, richer status-line presets, arbitrary tool payload fuzz/control scrub, generated property fuzz matrix, simulated streaming soak budget, no-visible streaming delta dirty guard, debug-config viewport scroll clamp, render-loop draw-error snapshot recovery, PTY live streaming terminal soak, PTY resize/manual-scroll streaming soak, PTY mouse wheel/scrollbar streaming soak, PTY long-duration matrix soak, `/compact plan` token dry-run preview, `/permissions` mode picker plus engine request wiring, team-memory sync runtime config wiring, team-memory watcher debounce push wiring, team-memory watcher CLI lifecycle wiring, team-memory write-tool notification wiring, team-memory CLI/sync path alignment, team-memory secret guard write blocking, team-memory utility detector alignment, team-memory prompt/runtime wiring, team-memory extraction gate wiring, SDK permission prompt fail-closed bridge, auto-compact query replay wiring, top status header, active activity panel, semantic `/diff` review surface, semantic `/files` file-change summary, semantic `/timeline` render-event lifecycle history, Layer 1 stable render turn ids, Layer 1 raw engine event journal, Layer 1 render session snapshot serialization, Layer 1 render session snapshot persistence/load helper, TUI render snapshot export/load validation command, TUI render snapshot restore/hydration command, render snapshot autosave on session end, TUI resume latest render snapshot flow, TUI startup render snapshot restore flow, Layer 1 generalized relation index, semantic `/ps` process inspector, semantic `/status` session overview, semantic `/commands` command history plus full-log expansion, semantic `/errors` error history, semantic `/results` final-summary history, semantic `/approvals` approval history, semantic `/debug-config` redacted configuration inspection, semantic `/title` terminal title inspection/configuration, compact cancellation/status controls, scrollable/filterable slash help catalog, scrollable command-output modal, modal-first mouse-wheel routing, dirty-on-change mouse render scheduling, diff review intraline change highlighting, rendered-viewport diff review scrolling, rendered-viewport help/command-output modal scrolling, and rendered-viewport semantic inspection modal scrolling complete
Owner: mossen-tui
Created: 2026-05-21

This file is the durable record for the next rendering push. The goal is to
make the TUI reliable enough that we can return to the harness/agent loop
without carrying rendering ambiguity forward.

2026-05-23 streaming dirty throttling:

- Red-line framing: the product render-event layer already marked text,
  thinking, tool-input, and command-output stream updates as throttled, but
  `App::handle_engine_message()` and streaming transcript mutation still
  marked the frame dirty unconditionally. That meant high-frequency token
  streams could bypass the refresh policy and force a redraw per delta.
- `STREAM_THROTTLE_MS` is now exported from `render_events`, and App has a
  policy-aware dirty marker for `Immediate`, `Throttled`, and `Passive` refresh
  policies.
- Streaming text/thinking transcript updates now use the throttled refresh
  policy, while structural events such as message start, tool start/finish,
  approval, retry, and final summary still force immediate render visibility.
- The throttled dirty marker also accounts for recent slow frames, so the TUI
  backs off when rendering itself is the bottleneck instead of piling redraws
  behind a token stream.
- Suppressed throttled redraws now record a `render_throttled_dirty_at`
  deadline. Tick wakeups can flush the pending transcript update at the stream
  throttle deadline instead of waiting for the slower active-animation frame
  interval, keeping streaming responsive without per-token redraw.
- The main event loop now computes a unified next-frame deadline and includes a
  `sleep_until` wakeup in the event/engine select path, so render pacing is not
  only an incidental side effect of the fixed tick timer.
- Removed both the handler tail dirty mark and the `run()` engine-message
  select-branch dirty mark that invalidated throttling; sub-agent stream
  activity now uses the same throttled policy.
- Added W80 static smoke coverage and a regression test proving a fast
  streaming delta updates the transcript without immediately scheduling a
  frame, then schedules once the throttle interval elapses.
- Added deadline-selection coverage proving throttled streaming transcript
  updates pick the stream throttle deadline ahead of the slower active-animation
  interval.
- Added a paced follow-up regression test proving a throttled transcript update
  still gets flushed by the active render scheduler when no new engine message
  arrives immediately.
- Verified:
  - `python3 scripts/wave_w80_render_stream_throttle_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::next_render_frame_deadline_prefers_throttled_streaming_updates`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::streaming_text_delta_dirty_mark_is_throttled`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::throttled_streaming_delta_gets_paced_followup_frame`
  - `cargo test -q -p mossen-tui render_events::tests::throttles_streaming_text_updates`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_survives_streaming_resize_interleave`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_large_session_stays_within_budget`
  - `cargo check -q -p mossen-tui`
  - `rustfmt crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_events.rs --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_events.rs scripts/wave_w80_render_stream_throttle_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched render files

2026-05-23 resize scroll clamp:

- Red-line framing: resize/focus storms are already coalesced and long
  transcripts have virtual scrolling, but a terminal resize could update the
  viewport height while a user was manually scrolled up without immediately
  clamping the row offset to the new viewport. The next render pass corrected
  the state, but the gap left scrollbar hit-testing and wheel input dependent on
  a temporarily invalid scroll range.
- `VirtualScroll::set_viewport_height()` now clamps manual-scroll offsets the
  same way `set_total_items()` does, while preserving `sticky = false` so a
  resize cannot silently re-enable sticky-bottom and steal the reader back to
  the live tail.
- Added W81 static smoke coverage plus layout-level and App-event-level
  regressions for resize clamping.
- Verified:
  - `python3 scripts/wave_w81_render_scroll_resize_smoke.py`
  - `cargo test -q -p mossen-tui layout::tests::viewport_resize_clamps_manual_scroll_without_restoring_sticky`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::resize_event_clamps_manual_scroll_without_restoring_sticky`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_streaming_tall_message_keeps_scroll_policy`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_survives_resize_storm_and_pathological_content`
  - `cargo check -q -p mossen-tui`

2026-05-23 streaming resize mouse scroll contract:

- Red-line framing: W80 and W81 covered streaming redraw pacing and resize-time
  manual-scroll clamping separately. The missing product contract was the
  real-session interleave: long streaming output, user-controlled transcript
  scrollbar movement, terminal resize, more streaming deltas, and then using
  the resized scrollbar to return to the live tail.
- Added a render contract that grows a streaming assistant block past the
  viewport, verifies sticky-bottom reaches the current stream tail, clicks the
  transcript rail to the top, appends more stream text while manual scroll is
  active, renders through a smaller terminal size, and proves the viewport is
  not stolen back to the new tail.
- The same contract then drags the resized transcript rail to the bottom and
  verifies sticky-bottom plus the post-resize stream tail anchor, covering the
  scrollbar hit target after scroll state and viewport dimensions changed.
- Added W82 static smoke coverage for the combination contract, mouse click and
  drag inputs, resize render pass, appended stream tail anchor, manual-scroll
  preservation assertion, and `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w82_render_stream_resize_scroll_smoke.py`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_streaming_resize_mouse_scroll_stays_usable`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_streaming_tall_message_keeps_scroll_policy`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/tests/render_contract.rs scripts/wave_w82_render_stream_resize_scroll_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched W82 files

2026-05-23 keyboard focus scroll anchoring:

- Red-line framing: transcript mouse scrolling and row-based manual scroll were
  protected, but keyboard focus navigation could still select a semantic
  message whose rendered rows were outside the visible viewport. That creates a
  product-grade failure mode where keyboard actions target hidden history and
  subsequent async output can appear to yank the viewport.
- App now stores the last rendered transcript content area separately from the
  scrollbar rail. The focus path uses that real viewport width/height instead
  of message counts or a fixed terminal-size guess.
- `MessagesWidget` now exposes a semantic source-index to rendered row-range
  lookup using the same `RenderTranscript`, height cache, glyph profile,
  collapsed groups, and focused-height flags as the actual renderer.
- `move_focus()` now anchors the virtual scroll around the focused semantic
  block and disables sticky-bottom when keyboard navigation takes ownership of
  history. Focusing the latest message at the live tail can still restore sticky
  bottom.
- Added a product render contract proving repeated keyboard focus movement
  reaches the transcript head, async append does not steal the keyboard-owned
  history viewport, and Ctrl-L still returns to the live tail.
- Added W83 static smoke coverage for the stored viewport area, focus visibility
  guard, semantic row-range helper, render contract, async-append assertion, and
  `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w83_render_keyboard_focus_scroll_smoke.py`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_keyboard_focus_scroll_owns_viewport`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_streaming_resize_mouse_scroll_stays_usable`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/messages.rs crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/messages.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w83_render_keyboard_focus_scroll_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/messages.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w83_render_keyboard_focus_scroll_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 non-scrollable scroll sticky guard:

- Red-line framing: mouse wheel or PageUp/PageDown should only take ownership
  of transcript history when there is an actual rendered scroll range. On a
  short transcript, a no-op upward scroll could still flip sticky-bottom off,
  so the next async output might fail to follow the live tail even though the
  user never moved to readable history.
- `VirtualScroll` now centralizes max-offset math and treats scroll-up/down
  with no scroll range as a no-op that clamps to offset zero and preserves
  sticky-bottom. Sticky scroll movement also computes from the live bottom
  offset, so a stale internal offset cannot make the first manual scroll jump
  to the wrong rendered rows.
- Added App-level coverage proving wheel input on a short transcript does not
  schedule a dirty frame and does not break future live-tail anchoring.
- Added W84 static smoke coverage for the scroll range helper, no-op guard,
  layout regression, App regression, sticky assertion, and `run_all_smoke.sh`
  registration.
- Verified:
  - `python3 scripts/wave_w84_render_scroll_noop_sticky_smoke.py`
  - `cargo test -q -p mossen-tui layout::tests::non_scrollable_scroll_up_preserves_sticky_bottom`
  - `cargo test -q -p mossen-tui layout::tests::sticky_scroll_up_uses_live_bottom_offset`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::non_scrollable_transcript_wheel_preserves_sticky_without_dirty_frame`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/layout.rs crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/layout.rs crates/mossen-tui/src/app.rs scripts/wave_w84_render_scroll_noop_sticky_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/layout.rs crates/mossen-tui/src/app.rs scripts/wave_w84_render_scroll_noop_sticky_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 viewport-sized transcript page navigation:

- Red-line framing: scrollable modals already use rendered viewport height for
  PageUp/PageDown, but the main transcript still used a fixed 10-row jump. On
  large terminals that makes history navigation feel stuck; on small terminals
  it makes keyboard paging inconsistent with the visible message area.
- Transcript PageUp/PageDown now uses the last rendered message content area
  height, falling back to the current virtual-scroll viewport when no frame has
  been drawn yet.
- Added App-level coverage proving PageUp and PageDown use the rendered
  transcript viewport height instead of the legacy fixed 10 rows.
- Added W85 static smoke coverage for the helper, PageUp/PageDown call sites,
  fixed-row regression assertion, App regression, and `run_all_smoke.sh`
  registration.
- Verified:
  - `python3 scripts/wave_w85_render_transcript_page_viewport_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::transcript_page_keys_use_rendered_message_viewport_height`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w85_render_transcript_page_viewport_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w85_render_transcript_page_viewport_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 self-induced scrollbar overflow guard:

- Red-line framing: transcript scrollbars should signal real history overflow,
  not create overflow by reserving their own column. At boundary widths, the
  prior decision path could measure only the reduced rail width and show a
  rail for content that fit at full width, causing avoidable layout jitter and
  fake scroll affordances.
- Transcript overflow detection now first measures the full message content
  width. Only if that full-width render overflows does the TUI reserve the
  one-column rail and recompute row accounting for the reduced rendered width.
- Narrow terminals that cannot host a rail still keep full-width row accounting
  and scroll state, so keyboard and wheel scroll remain usable without a fake
  scrollbar.
- Added App-level coverage proving a text block that wraps only after rail
  reservation keeps the full content width, does not show a scrollbar, and
  preserves sticky-bottom state.
- Added W86 static smoke coverage for the full-width guard, regression test,
  self-induced overflow assertion, and `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w86_render_scrollbar_overflow_guard_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::transcript_scrollbar_does_not_create_its_own_overflow`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::transcript_page_keys_use_rendered_message_viewport_height`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w86_render_scrollbar_overflow_guard_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w86_render_scrollbar_overflow_guard_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 no-op transcript page-key dirty guard:

- Red-line framing: W84 made no-op mouse wheel scrolling preserve sticky-bottom
  without scheduling a redraw on short transcripts. Keyboard PageUp/PageDown
  still marked the frame dirty even when no visible transcript scroll state
  changed, creating avoidable idle redraws in the same short-history case.
- Key event handling now treats main-transcript PageUp/PageDown as a visible
  scroll-state mutation: it records the transcript offset/sticky pair before
  routing the key and only marks the frame dirty when that pair changes.
- Modal PageUp/PageDown and all other keyboard paths retain their existing
  redraw behavior, so text input, modal navigation, shortcuts, and explicit
  refresh commands keep immediate visual feedback.
- Added App-level coverage proving PageUp and PageDown on a non-scrollable
  transcript preserve sticky-bottom and do not schedule a redraw.
- Added W87 static smoke coverage for the page-key guard, dirty comparison,
  regression test, no-op redraw assertions, and `run_all_smoke.sh`
  registration.
- Verified:
  - `python3 scripts/wave_w87_render_page_key_noop_dirty_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::non_scrollable_transcript_page_key_preserves_sticky_without_dirty_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::transcript_page_keys_use_rendered_message_viewport_height`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w87_render_page_key_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w87_render_page_key_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 no-op transcript focus-key dirty guard:

- Red-line framing: W87 removed redraws for PageUp/PageDown that do not move
  the transcript. The adjacent keyboard focus path still marked the frame dirty
  for Up/Down even when an empty transcript or a single already-focused message
  produced no visible focus or scroll-state change.
- Key event handling now treats main-transcript focus Up/Down as a visible
  transcript-state mutation. It records focused message, offset, and
  sticky-bottom before routing the key and only marks dirty when that visible
  state changes.
- Prompt history, suggestions, modals, streaming, and all non-focus keyboard
  paths retain their existing redraw behavior, so visible UI changes still
  repaint immediately.
- Added App-level coverage proving focus Up on an empty transcript and focus
  Down on a single already-focused message do not schedule redraws.
- Added W88 static smoke coverage for the focus-key guard, visible-state
  fingerprint, regression test, no-op redraw assertions, and
  `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w88_render_focus_key_noop_dirty_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::no_op_transcript_focus_keys_do_not_dirty_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::non_scrollable_transcript_page_key_preserves_sticky_without_dirty_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::transcript_page_keys_use_rendered_message_viewport_height`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_keyboard_focus_scroll_owns_viewport`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w88_render_focus_key_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w88_render_focus_key_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 external statusline in-flight frame guard:

- Red-line framing: the external statusline command path is useful for Codex
  CLI-style status chrome, but its in-flight bit is not rendered in the main
  footer. Treating that hidden background process as active animation could
  keep scheduling frames while the command runs, increasing idle CPU and
  flicker risk.
- The render scheduler no longer treats `external_statusline_in_flight` as an
  active animation source. Background statusline work now repaints the main
  surface when visible output or error text changes, not while the invisible
  process is merely running.
- The visible-state fingerprint no longer includes statusline sequence churn.
  It still includes output/error, and keeps the in-flight bit only for the
  explicit `/debug-config` surface where that field is actually displayed.
- Added App-level coverage proving in-flight statusline work does not dirty the
  main surface, does not drive active render frames, still repaints when
  visible output changes, and remains visible in the debug modal.
- Added W89 static smoke coverage for the active-animation guard, debug-only
  in-flight fingerprint, regression test, assertions, and
  `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w89_render_external_statusline_inflight_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::external_statusline_inflight_does_not_drive_invisible_frames`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::external_statusline_command_tick_is_nonblocking_and_stable`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::render_frame_scheduler_skips_idle_and_paces_active_animation`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w89_render_external_statusline_inflight_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w89_render_external_statusline_inflight_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 focus-change no-op dirty guard:

- Red-line framing: terminal focus gained/lost events update the notification
  latch and chrome focus state, but those fields are not visible in the main
  TUI surface. Marking every focus event dirty can create avoidable redraws
  when users alt-tab or when terminal emulators emit focus storms.
- The FocusChange event path now records the same visible-state fingerprint
  used by tick processing and only schedules a redraw if the rendered surface
  actually changes.
- The notification-fired latch is no longer part of the visible-state
  fingerprint, because it only gates future OS notifications and is not
  rendered in the transcript, footer, or modal surfaces.
- Added App-level coverage proving focus gained still resets the notification
  latch, but focus gained/lost without visible TUI changes does not dirty the
  frame.
- Added W90 static smoke coverage for the focus-change fingerprint guard,
  removal of the notification latch from the render fingerprint, regression
  test, no-op redraw assertions, and `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w90_render_focus_change_noop_dirty_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::focus_change_updates_notification_latch_without_dirty_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::external_statusline_inflight_does_not_drive_invisible_frames`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::render_frame_scheduler_skips_idle_and_paces_active_animation`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w90_render_focus_change_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w90_render_focus_change_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 same-size resize no-op dirty guard:

- Red-line framing: resize/focus storms are already coalesced, but a terminal
  can still deliver a resize event whose dimensions and scroll viewport state
  are already current. Marking that event dirty creates an avoidable redraw and
  flicker point.
- The Resize event path now compares the rendered resize state before and
  after applying terminal dimensions plus transcript viewport synchronization.
  It only marks the frame dirty when width, height, viewport rows, offset, or
  sticky-bottom actually changes.
- The guard still redraws when a same-dimension resize synchronizes a stale
  viewport height, preserving the first-resize correction path.
- Added App-level coverage for both cases: a fully unchanged same-size resize
  does not redraw, while same terminal dimensions with stale viewport state
  still update viewport rows and schedule a frame.
- Added W91 static smoke coverage for the resize before/after fingerprint,
  dirty guard, regression test, no-op redraw assertion, viewport-sync
  assertion, and `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w91_render_resize_noop_dirty_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::same_size_resize_without_visible_state_change_does_not_dirty_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::resize_event_clamps_manual_scroll_without_restoring_sticky`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_survives_resize_storm_and_pathological_content`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs scripts/wave_w91_render_resize_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs scripts/wave_w91_render_resize_noop_dirty_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 plan activity progress counts:

- Red-line framing: the PRD treats plans as first-class rendering objects, not
  plain chat text. TodoWrite cards already render structured plan steps, but
  the active activity/footer path only carried total step count and active
  step, so the live chrome lost progress shape.
- `RenderEventKind::PlanUpdated` now carries completed, active, pending, and
  blocked counts derived from TodoWrite/TaskNotePad-style payloads. Plain-text
  plan summaries degrade to pending counts.
- `RenderActivity::Plan`, the footer status line, timeline detail, process
  row, and active activity panel now preserve those progress counts, while
  still showing the current active step when available.
- Added App-level coverage proving the activity panel and footer show plan
  progress counts plus active step.
- Added an App render contract proving the plan activity panel stays above
  transcript history, shows all progress counts, keeps the structured
  TodoWrite plan card visible, and does not leak raw todo JSON.
- Added W92 static smoke coverage for the event fields, status counter,
  timeline summary helper, activity panel helper, App regression, render
  contract, and `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w92_render_plan_activity_progress_smoke.py`
  - `cargo test -q -p mossen-tui render_events::tests::maps_todowrite_summary_to_plan_event`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::render_surface_carries_plan_progress_counts_in_active_panel`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::structured_render_events_drive_footer_activity`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_plan_activity_panel_shows_progress_counts`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_keeps_active_panel_above_transcript_history`
  - `cargo test -q -p mossen-tui render_model::tests::normalizes_todo_result_json`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
 - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w92_render_plan_activity_progress_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w92_render_plan_activity_progress_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 plan timeline progress counts:

- Red-line framing: this remains render-layer history/readability work only.
  It does not change task execution, agent semantics, permissions, or the
  harness.
- `/timeline` already received structured `PlanUpdated` events from the W92
  plan progress model, but selected-row detail still only exposed the tool id.
  The selected plan row now repeats the normalized progress summary in detail
  text, so the counts remain discoverable even when the row header is clipped.
- Added model coverage proving `plan_updated` rows preserve total, done,
  active, pending, blocked, active-step, and tool-id facts.
- Added an App render contract proving `/timeline` shows plan progress counts
  from TodoWrite-derived structured events and does not leak raw TodoWrite JSON.
- Added W93 static smoke coverage for the model regression, selected detail,
  render contract, raw-key leak guard, and `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w93_render_plan_timeline_progress_smoke.py`
  - `cargo test -q -p mossen-tui render_model::tests::render_timeline_preserves_plan_progress_counts`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_timeline_modal_shows_plan_progress_counts`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_timeline_modal_uses_structured_render_events`
  - `cargo test -q -p mossen-tui widgets::render_timeline::tests::timeline_renders_counts_and_selected_detail`
  - `cargo test -q -p mossen-tui render_events::tests::maps_todowrite_summary_to_plan_event`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w93_render_plan_timeline_progress_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w93_render_plan_timeline_progress_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 plan process/status progress counts:

- Red-line framing: this remains TUI render-surface consistency work only. It
  does not change task execution, agent semantics, permissions, or the
  harness.
- `/ps` and `/status` already receive activity text from the shared
  `RenderActivity::status_line()` path. Added a product contract proving plan
  progress counts survive through both process inspection and status overview
  surfaces.
- The contract covers total, done, active, pending, blocked, and active-step
  facts for a live planning turn, keeping the same normalized vocabulary as
  the activity panel, footer, and `/timeline`.
- Added W94 static smoke coverage for the shared status-line source, process
  row bridge, status overview source, render contract, and `run_all_smoke.sh`
  registration.
- Verified:
  - `rustfmt crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w94_render_plan_process_status_smoke.py`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_process_and_status_show_plan_progress_counts`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_ps_modal_uses_semantic_process_state`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_status_modal_uses_semantic_session_state`
 - `cargo check -q -p mossen-tui`
 - `rustfmt --check crates/mossen-tui/tests/render_contract.rs`
 - `git diff --check -- crates/mossen-tui/tests/render_contract.rs scripts/wave_w94_render_plan_process_status_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
 - `rg -n "[[:blank:]]$" crates/mossen-tui/tests/render_contract.rs scripts/wave_w94_render_plan_process_status_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 render hot-path cache diagnostics:

- Red-line framing: this remains terminal rendering mechanics only. It does
  not change task execution, agent behavior, permissions, harness logic, or
  model request construction.
- `/debug-config` now exposes renderer hot-path health through redacted,
  semantic rows for transcript cache state and frame scheduler state. This
  makes long-session lag/flicker diagnosis visible without leaking raw engine
  payloads or secrets.
- Added a long-session repeated-frame regression proving visible state-only
  changes, such as stage or prompt repaint, reuse the cached transcript model,
  while a real transcript append invalidates the cache exactly once.
- Added W95 static smoke coverage for the debug rows, diagnostic sources,
  long-session regression, product contract anchors, and `run_all_smoke.sh`
  registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w95_render_hot_path_cache_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::repeated_long_session_frames_reuse_transcript_cache_for_visible_state_changes`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_debug_config_modal_is_redacted_and_semantic`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_large_session_stays_within_budget`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_contract.rs`
 - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w95_render_hot_path_cache_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
 - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w95_render_hot_path_cache_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 generic JSON payload secret redaction:

- Red-line framing: this remains render-model normalization only. It does not
  alter tool execution, model calls, permissions, harness behavior, or raw
  debug surfaces.
- Generic JSON tool payload rendering now redacts sensitive key families such
  as API keys, authorization values, passwords, secrets, and token-bearing
  secret fields before they reach normal transcript sections or inline object
  summaries.
- The redaction is key-scoped so normal observability fields such as
  `token_count` continue to render.
- Added W96 static smoke coverage for the redaction helper, key detector,
  non-secret token-count assertion, product contract, and `run_all_smoke.sh`
  registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w96_render_generic_payload_redaction_smoke.py`
  - `cargo test -q -p mossen-tui render_model::tests::generic_json_tool_payload_redacts_sensitive_fields`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_generic_json_tool_payload_redacts_secrets`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w95_render_hot_path_cache_smoke.py scripts/wave_w96_render_generic_payload_redaction_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w95_render_hot_path_cache_smoke.py scripts/wave_w96_render_generic_payload_redaction_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 richer status-line presets:

- Red-line framing: this is a terminal render configuration slice only. It
  does not alter task execution, tool permissions, harness behavior, or the
  agent loop.
- `FooterRenderConfig` now has distinct `minimal`, Codex-like `focused`,
  `standard`, and `full` preset shapes. `full` includes the external-status
  slot, while `focused` keeps model, permission mode, reasoning, live activity,
  and context visible without project/cost/message-count noise.
- `/statusline focused`, `/statusline focus`, and `/statusline codex` apply
  the focused preset. The status-line modal now shows the current preset and
  visible `M/C/D/F` preset shortcuts instead of hiding those keys as implicit
  behavior.
- Added W97 static smoke coverage for preset shape detection, Codex aliasing,
  modal hints, the product contract, and `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w97_render_statusline_presets_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::footer_statusline_presets_have_distinct_render_shapes`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::footer_statusline_codex_alias_applies_focused_preset`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_statusline_presets_are_visible_and_codex_focused`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w97_render_statusline_presets_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w97_render_statusline_presets_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 arbitrary tool payload fuzz/control scrub:

- Red-line framing: this is terminal render-model hardening only. It does not
  alter task execution, model calls, permissions, harness behavior, or raw
  debug surfaces.
- Semantic text cleanup now removes terminal control characters after ANSI
  stripping while preserving line breaks. Arbitrary tool payloads can no
  longer carry BEL/backspace/form-feed style controls into normal transcript
  cells.
- Generic JSON redaction now covers authorization headers, private keys,
  credentials, and token-shaped secret keys while preserving observability
  counters such as `token_count` and `total_tokens`.
- Added W98 static smoke coverage plus a render-model regression and product
  render contract for arbitrary third-party tool payloads with nested JSON,
  control characters, ANSI sequences, long-ish fields, and sensitive tokens.
- Verified:
  - `rustfmt crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w98_render_arbitrary_payload_fuzz_smoke.py`
  - `cargo test -q -p mossen-tui render_model::tests::arbitrary_json_tool_payloads_scrub_controls_and_redact_token_secrets`
  - `cargo test -q -p mossen-tui render_model::tests::generic_json_tool_payload_redacts_sensitive_fields`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_arbitrary_tool_payloads_are_scrubbed_and_redacted`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w98_render_arbitrary_payload_fuzz_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - `rg -n "[[:blank:]]$" crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/render_contract.rs scripts/wave_w98_render_arbitrary_payload_fuzz_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md` returned no matches

2026-05-23 generated property fuzz matrix:

- Red-line framing: this remains TUI render-contract hardening only. It does
  not change task execution, model calls, permissions, harness behavior, or
  agent-loop semantics.
- Added a deterministic in-test render fuzzer that generates mixed user,
  assistant, system, progress, attachment, command-output, tool-use, and
  tool-result transcript rows with valid JSON, malformed JSON, nested arbitrary
  payloads, long unbroken text, CJK/combining text, ANSI sequences, terminal
  controls, and secret-shaped fields.
- The product render contract now rejects any visible terminal control
  character outside newlines, not just a fixed BEL/backspace/form-feed sample.
- Added a W99 matrix contract that renders each generated session at narrow,
  small, normal, and wide sizes, then manual-scrolls and re-renders while
  asserting product cleanliness, secret redaction, and bounded transcript
  scroll state after every frame.
- Added W99 static smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w99_render_property_fuzz_matrix_smoke.py`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_survives_generated_property_fuzz_matrix`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_arbitrary_tool_payloads_are_scrubbed_and_redacted`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/tests/render_contract.rs`

2026-05-23 simulated streaming soak budget:

- Red-line framing: this remains TUI render-contract hardening only. It does
  not change task execution, model calls, permissions, harness behavior, or
  agent-loop semantics.
- Added a virtual long-running streaming soak fixture that pushes 1,800 text
  deltas through the real `App::handle_engine_message()` streaming path,
  including wide text, combining marks, ANSI sequences, terminal controls, and
  long unbroken segments.
- The soak samples rendered frames across resize-sized viewports, triggers
  manual transcript scroll mid-stream, keeps appending deltas, verifies the
  manual history viewport is not stolen by the live tail, and then uses Ctrl-L
  to restore sticky-bottom at the final tail.
- The contract asserts product cleanliness, bounded scroll state, enough frame
  samples to exercise resize pacing, and a wall-clock budget for the sampled
  render loop. This is a fast deterministic proxy for the remaining real
  external-terminal soak, not a claim that real multi-minute terminal soak is
  complete.
- Added W100 static smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/tests/render_contract.rs`
  - `python3 scripts/wave_w100_render_streaming_soak_budget_smoke.py`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_simulated_streaming_soak_keeps_scroll_and_budget`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_survives_generated_property_fuzz_matrix`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/tests/render_contract.rs`

2026-05-23 no-visible streaming delta dirty guard:

- Red-line framing: this remains TUI render-loop responsiveness work only. It
  does not change task execution, model calls, permissions, harness behavior,
  or agent-loop semantics.
- Empty streaming text/thinking deltas now return before creating transcript
  mutations or scheduling a redraw. This avoids keepalive-style stream packets
  turning into pointless frame deadlines.
- Text streaming now compares the derived visible `(thinking, content)` pair
  with the existing pending assistant row before marking the transcript dirty.
  Parser-boundary chunks such as a lone `<think>` update the internal stream
  buffer but no longer schedule a visible frame until actual thinking or answer
  text appears.
- Added an App-level regression proving empty text, empty thinking, and
  no-visible parser-boundary deltas do not set `dirty` or a throttled redraw
  deadline, while the first visible reasoning text still schedules a paced
  redraw.
- Added W101 static smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w101_render_stream_no_visible_dirty_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::no_visible_streaming_delta_does_not_schedule_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::streaming_text_delta_dirty_mark_is_throttled`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::throttled_streaming_delta_gets_paced_followup_frame`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::next_render_frame_deadline_prefers_throttled_streaming_updates`
  - `cargo check -q -p mossen-tui`

2026-05-23 debug-config viewport scroll clamp:

- Red-line framing: this remains terminal-rendering interaction work only. It
  does not change task execution, model calls, permissions, harness behavior,
  or agent-loop semantics.
- `/debug-config` already exposed redacted renderer diagnostics, but its
  internal scroll max was `row_count - 1` instead of `row_count -
  viewport_rows`. That could let End/PageDown leave a mostly blank tail in a
  long diagnostics panel and made the scrollbar imply unreachable content.
- `DebugConfigState` now owns a viewport-aware scroll max and render-time
  visible-scroll clamp. Keyboard, wheel, pointer-scrollbar, and resize redraws
  share the same last-full-viewport boundary.
- Added unit and keybinding regressions proving End/PageDown stop at the last
  rendered viewport and stale oversized offsets clamp before drawing.
- Added W102 static smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs`
  - `python3 scripts/wave_w102_render_debug_config_scroll_clamp_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::debug_config_scroll_clamps_to_rendered_viewport`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs`

2026-05-23 render-loop draw-error snapshot recovery:

- Red-line framing: this is TUI render-loop recovery work. It does not change
  task execution, model calls, tool permissions, or agent-loop semantics.
- The remaining product risk list calls out harness loop state recovery. Before
  this slice, `App::run()` only autosaved render-session snapshots after a
  clean loop exit; a terminal backend draw/flush error returned through `?`
  before the current transcript, raw-event journal, and interrupted-turn render
  state were persisted.
- The event-loop body is now separated behind `run_event_loop_with_bus()` so
  recovery paths can be tested without starting real crossterm readers. The
  public `run()` path still owns startup restore, real event-source spawning,
  and normal loop lifecycle.
- Terminal draw failure now performs a best-effort render-session autosave
  before returning an error with explicit recovery context. Normal early quit
  and clean loop exits use the same best-effort helper.
- Added a synthetic failing backend regression proving a live run-loop draw
  error writes the current render snapshot under the session cwd, preserves the
  session id and visible command marker, and leaves no pending engine submit.
- Added W103 static smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-tui/src/app.rs`
  - `python3 scripts/wave_w103_render_loop_draw_error_autosave_smoke.py`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::run_autosaves_render_snapshot_when_terminal_draw_fails`
  - `cargo test -q -p mossen-tui app::engine_stream_tests::run_restores_latest_render_snapshot_before_first_loop`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs`

2026-05-23 PTY live streaming terminal soak:

- Red-line framing: this is terminal-rendering mechanism work only. It does
  not change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- Added a real PTY smoke that runs interactive `mossen --bare` in an isolated
  harness project with a fixed 24x96 terminal size and a local
  OpenAI-compatible SSE mock backend.
- The mock emits a paced multi-chunk assistant stream with stable head/tail
  markers. The harness sends a prompt through the PTY, waits for the tail
  marker to render, exits with `/quit`, and stores raw/text PTY captures plus
  mock request evidence under the fixture artifacts directory.
- The assertions cover process exit, prompt delivery, chat-completions hit,
  multi-chunk SSE completion, head/tail marker rendering, alternate-screen
  enter/leave pairing, bounded fullscreen clears, and bounded output size.
- This closes the first real external-terminal live streaming foundation. It
  is still shorter than a multi-minute soak; follow-up should expand the same
  harness across resize, manual-scroll, and longer-duration variants.
- Added W104 runtime smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `cargo build -q -p mossen-cli --bin mossen`
  - `python3 scripts/wave_w104_render_pty_live_streaming_soak.py`

2026-05-23 PTY resize/manual-scroll streaming soak:

- Red-line framing: this remains terminal-rendering mechanism work only. It
  does not change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- Added a real PTY smoke that extends the W104 path with terminal resize and
  user-owned transcript scrolling while a local OpenAI-compatible SSE stream is
  still active.
- The harness starts interactive `mossen --bare`, streams 220 rows, sends a
  narrow resize, injects repeated PageUp key events to move away from the live
  tail, sends a second resize while manually scrolled, then waits for the mock
  stream to complete before using Ctrl+L to restore sticky-bottom.
- Assertions prove the PTY process exits cleanly, the stream completes, both
  resizes are delivered, manual history rows render before restore, the live
  tail remains hidden while manual scroll owns the viewport, Ctrl+L renders the
  final tail, alternate-screen enter/leave is paired, repeated clears are
  bounded, and output size stays bounded.
- This moves the external-terminal evidence from "live streaming works" to
  "live streaming remains usable under resize and manual scroll". Remaining
  work is a longer-duration soak matrix and real mouse-scroll/scrollbar input
  injection in PTY.
- Added W105 runtime smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w105_render_pty_resize_manual_scroll_soak.py`

2026-05-23 PTY mouse wheel/scrollbar streaming soak:

- Red-line framing: this is terminal input/rendering mechanism work only. It
  does not change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- The real TUI startup path now enables crossterm mouse capture when entering
  alternate screen and disables it during normal cleanup. Panic cleanup also
  disables mouse capture, and the external-editor path disables it before
  leaving the TUI and re-enables it when returning.
- Added a real PTY smoke that injects xterm SGR mouse events into interactive
  `mossen --bare`: wheel-up during streaming, wheel-down restore, scrollbar
  top click after restore, and final wheel-down restore before quitting.
- Assertions prove mouse capture enable/disable sequences are emitted, wheel-up
  renders history and hides the live tail, wheel-down restores the tail,
  scrollbar top click renders history and hides the tail, final wheel-down
  restores the tail again, alternate-screen enter/leave is paired, repeated
  clears are bounded, and output size stays bounded.
- This closes the real terminal mouse input gap called out by the PRD's
  "scrollbar must be scrollable" and "keyboard interaction first, but terminal
  scroll must remain usable" expectations.
- Added W106 runtime smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `rustfmt crates/mossen-cli/src/repl.rs crates/mossen-cli/src/entrypoints.rs crates/mossen-tui/src/app.rs`
  - `cargo build -q -p mossen-cli --bin mossen`
  - `python3 scripts/wave_w106_render_pty_mouse_scroll_soak.py`

2026-05-23 PTY long-duration matrix soak:

- Red-line framing: this remains terminal-rendering mechanism work only. It
  does not change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- Added a real external PTY matrix soak that keeps an OpenAI-compatible SSE
  stream active for a minimum duration, emits 500+ chunks, and exercises four
  terminal-size changes across wide, narrow, tall, compact, and medium
  layouts.
- While the stream is still active, the harness uses real mouse-wheel input to
  take ownership of transcript history. It then continues receiving streaming
  deltas and resize events, verifies the final tail does not steal the manual
  viewport, restores to the tail, clicks the scrollbar rail back to history,
  and restores to the tail again before quitting.
- Assertions cover stream duration, chunk completion, all matrix resizes,
  manual history preservation, tail hiding before restore, tail restoration,
  scrollbar history preservation, mouse-capture enable/disable, alternate
  screen pairing, bounded clears, and bounded output size.
- This closes the longer-duration external-terminal soak gap from the product
  render risk list. Remaining render cleanup is focused on retiring root
  compatibility parsing, not on missing live terminal interaction coverage.
- Added W107 runtime smoke coverage plus `run_all_smoke.sh` registration.
- Verified:
  - `python3 -m py_compile scripts/wave_w107_render_pty_long_matrix_soak.py`
  - `python3 scripts/wave_w107_render_pty_long_matrix_soak.py`

2026-05-23 root command-summary boundary cleanup:

- Red-line framing: this is render-pipeline boundary cleanup only. It does not
  change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- Final-summary command facts now come from Layer 2 semantic transcript runs via
  `command_summaries_from_messages()` instead of a root/app-local Bash and
  PowerShell JSON parser.
- Removed the root-level command payload parser helpers from `app.rs`, including
  command/cwd/exit/duration extraction from raw tool JSON. The root tool-preview
  fallback also no longer parses tool JSON; semantic block selector summaries
  own rich tool previews.
- Added a render-model regression proving completed and pending command
  summaries are derived from semantic transcript command runs, and added W108
  static smoke coverage that rejects reintroducing the old root command-summary
  parser path.
- Verified:
  - `python3 scripts/wave_w108_render_root_command_summary_boundary_smoke.py`
  - `cargo test -q -p mossen-tui command_summaries_are_derived_from_semantic_transcript_runs`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs`
  - `python3 -m py_compile scripts/wave_w108_render_root_command_summary_boundary_smoke.py`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs scripts/wave_w108_render_root_command_summary_boundary_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 root tool-preview boundary cleanup:

- Red-line framing: this is render-pipeline boundary cleanup only. It does not
  change task execution, model-call construction, tool permissions, or
  agent-loop semantics.
- Tool-use transcript previews and approval-dialog detail text now use
  `tool_call_preview_from_input()` from Layer 2 semantic render model instead
  of app/root-local render formatting helpers.
- Removed root-level tool preview formatting helpers from `app.rs`, including
  known-tool preview branching, edit diff rendering, preview-block clipping, and
  short input summary formatting. Permission classification still stays in the
  app boundary because it controls approval semantics rather than terminal
  rendering.
- Added render-model regressions for structured input summaries and known-tool
  multi-line previews, plus W109 static smoke coverage that rejects
  reintroducing the old root preview formatter path.
- Verified:
  - `python3 scripts/wave_w109_render_tool_preview_boundary_smoke.py`
  - `cargo test -q -p mossen-tui tool_`
  - `cargo check -q -p mossen-tui`
  - `rustfmt --check crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs`
  - `python3 -m py_compile scripts/wave_w108_render_root_command_summary_boundary_smoke.py scripts/wave_w109_render_tool_preview_boundary_smoke.py`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs scripts/wave_w108_render_root_command_summary_boundary_smoke.py scripts/wave_w109_render_tool_preview_boundary_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 permission summary render boundary cleanup:

- Red-line framing: this is approval UI rendering cleanup only. It does not
  change task execution, model-call construction, permission decisions, or
  agent-loop semantics.
- `ToolUseConfirm.input_summary` now uses the semantic
  `tool_input_summary_from_value()` helper instead of serializing the raw tool
  input JSON in `app.rs`.
- Added an app regression that a Bash permission request surfaces `ls -la` as
  the modal summary rather than raw JSON, and added W110 static smoke coverage
  to reject reintroducing raw JSON permission summaries.
- Verified:
  - `python3 scripts/wave_w110_render_permission_summary_boundary_smoke.py`
  - `cargo test -q -p mossen-tui permission_`
  - `cargo test -q -p mossen-tui pending_tool_approval_preempts_idle_return_dialog`
  - `cargo check -q -p mossen-tui`

2026-05-23 compact-plan preview render boundary cleanup:

- Red-line framing: this is `/compact plan` preview rendering cleanup only. It
  does not change compaction execution, model-call construction, tool
  permissions, or agent-loop semantics.
- The compact-plan dry-run token estimate now gets its synthetic summary text
  from `compact_plan_summary_preview_from_messages()` in Layer 2 semantic
  render model instead of app/root-local message/content block formatting
  helpers.
- Tool-use blocks in that preview now reuse `tool_input_summary_from_value()`
  so terminal-facing compact previews show concise semantic input summaries
  rather than raw tool-input JSON.
- Added a render-model regression proving compact-plan previews format text,
  tool-use, and tool-result blocks without raw tool JSON, plus W111 static
  smoke coverage that rejects reintroducing compact-plan preview formatters in
  `app.rs`.
- Verified:
  - `python3 scripts/wave_w111_render_compact_plan_preview_boundary_smoke.py`
  - `cargo test -q -p mossen-tui compact_plan_`
  - `cargo check -q -p mossen-tui`

2026-05-23 assistant content render boundary cleanup:

- Red-line framing: this is SDK assistant-message to transcript-fact boundary
  cleanup only. It does not change task execution, model-call construction,
  tool permission decisions, or agent-loop semantics.
- Main-agent and sub-agent `Assistant` message handling now reuse
  `assistant_content_facts()` from Layer 1 render lifecycle instead of each
  root/app branch directly scanning `ContentBlock::Text` and
  `ContentBlock::ToolUse`.
- The helper produces stable text and tool-use facts once, keeping raw
  `AssistantMessage.content` interpretation out of the root app while leaving
  existing transcript rendering and task/sub-agent state updates intact.
- Added Layer 1 regression coverage for extracting concatenated assistant text
  and tool-use facts, plus W112 static smoke coverage that rejects
  reintroducing root assistant content-block extraction.
- Verified:
  - `python3 scripts/wave_w112_render_assistant_content_boundary_smoke.py`
  - `cargo test -q -p mossen-tui assistant_content_facts_extract_text_and_tool_uses_once`
  - `cargo test -q -p mossen-tui engine_tool_use_id_flows_into_render_record_and_approval_anchor`
  - `cargo test -q -p mossen-tui app_layers_subagent_records_under_task_parent`
  - `cargo check -q -p mossen-tui`

2026-05-23 permission-mode choice render boundary cleanup:

- Red-line framing: this is `/permissions` picker and status/access-mode
  render-model cleanup only. It does not change permission decisions, tool
  execution, model-call construction, or agent-loop semantics.
- Permission-mode labels, canonical engine codes, picker ordering, selected
  index normalization, and status display labels now live in Layer 2
  `PermissionModeChoiceRenderModel` helpers instead of app/root-local constants
  and functions.
- `/permissions`, `/permission-mode`, status surfaces, and `PromptParams`
  permission-mode wiring now all reuse the same semantic choice model, reducing
  drift between terminal labels such as `Full Auto` and engine codes such as
  `bypassPermissions`.
- Added render-model regression coverage for label/code normalization and W113
  static smoke coverage that rejects reintroducing root-local permission mode
  choice helpers.
- Verified:
  - `python3 scripts/wave_w113_render_permission_mode_boundary_smoke.py`
  - `cargo test -q -p mossen-tui permission_mode_choices_normalize_labels_and_codes`
  - `cargo check -q -p mossen-tui`

2026-05-23 tool-summary transcript render boundary cleanup:

- Red-line framing: this is `ToolUseSummary` to transcript-record rendering
  cleanup only. It does not change tool execution, permission decisions,
  model-call construction, TodoWrite state semantics, or agent-loop behavior.
- Main-agent and sub-agent tool result rows now come from Layer 1
  `tool_summary_transcript_facts()` instead of each app/root branch rebuilding
  parent ids, result record ids, and `MessageData` tool-result fields locally.
- Stable render id helpers for task roots and scoped tool records now live with
  the render lifecycle boundary, keeping task/sub-agent result parentage
  consistent with render snapshots and relation indexes.
- Added Layer 1 regression coverage for scoped parent ids, latest-tool fallback,
  task fallback, and produced tool-result message fields, plus W114 static smoke
  coverage that rejects reintroducing root-local tool-summary record-id helpers.
- Verified:
  - `python3 scripts/wave_w114_render_tool_summary_boundary_smoke.py`
  - `cargo test -q -p mossen-tui tool_summary_transcript_facts_scope_parent_ids_and_message`
  - `cargo test -q -p mossen-tui engine_tool_result_keeps_parent_tool_use_id`
  - `cargo check -q -p mossen-tui`

2026-05-23 engine-notice transcript render boundary cleanup:

- Red-line framing: this is compact-boundary/API-retry transcript row rendering
  cleanup only. It does not change retry timing, compact execution, model-call
  construction, permission decisions, tool execution, or agent-loop behavior.
- Compact boundary transcript rows and compact progress text now come from Layer
  1 `compact_boundary_transcript_facts()` instead of root-local formatting in
  `app.rs`.
- API retry transcript rows now come from Layer 1 `api_retry_transcript_message()`
  while `app.rs` still owns the retry UI stage transition.
- Added Layer 1 regression coverage for compact progress/message fields and
  retry error message fields, plus W115 static smoke coverage that rejects
  reintroducing root-local engine-notice transcript formatting.
- Verified:
  - `python3 scripts/wave_w115_render_engine_notice_boundary_smoke.py`
  - `cargo test -q -p mossen-tui engine_notice_transcript_facts_format_compact_and_retry_rows`
  - `cargo test -q -p mossen-tui main_engine_messages_are_ingested_as_raw_layer1_events`
  - `cargo check -q -p mossen-tui`

2026-05-23 progress transcript render boundary cleanup:

- Red-line framing: this is sub-agent progress and exceptional stop-reason row
  rendering cleanup only. It does not change sub-agent execution, teammate
  state transitions, tool execution, permission decisions, model-call
  construction, or agent-loop behavior.
- Sub-agent start/completed transcript rows now come from Layer 1
  `task_started_transcript_facts()` and `task_completed_transcript_facts()`
  instead of root-local `MessageData` construction in `app.rs`.
- Non-normal stream stop reasons now use Layer 1
  `exceptional_stop_reason_transcript_message()`, keeping the normal
  `end_turn`/`tool_use` suppression rule with the transcript rendering facts.
- Added Layer 1 regression coverage for task progress ids/parents/messages and
  exceptional stop-reason filtering, plus W116 static smoke coverage that
  rejects reintroducing root-local progress transcript formatting.
- Verified:
  - `python3 scripts/wave_w116_render_progress_boundary_smoke.py`
  - `cargo test -q -p mossen-tui progress_transcript_facts_format_task_and_stop_rows`
 - `cargo test -q -p mossen-tui app_layers_subagent_records_under_task_parent`
 - `cargo test -q -p mossen-tui tool_use_stop_and_empty_completed_do_not_render_as_answer`
 - `cargo check -q -p mossen-tui`

2026-05-23 assistant/tool transcript render boundary cleanup:

- Red-line framing: this is Assistant/ToolUse transcript row rendering cleanup
  only. It does not change assistant streaming semantics, tool execution,
  permission decisions, model-call construction, TodoWrite parsing, or
  agent-loop behavior.
- Main assistant rows, pending streaming assistant placeholders, sub-agent
  assistant rows, and Assistant-emitted tool-use rows now come from Layer 1
  transcript helpers instead of root-local `MessageData` construction.
- Pending assistant finalization now lives with the transcript facts, including
  empty-placeholder removal and non-`Completed` terminal fallback text for
  thinking-only messages.
- Added Layer 1 regression coverage for main/task assistant rows, main/task
  tool-use rows, pending placeholders, and finalization rules, plus W117 static
  smoke coverage that rejects reintroducing root-local assistant/tool transcript
  formatting.
- Verified:
  - `python3 scripts/wave_w117_render_assistant_transcript_boundary_smoke.py`
  - `cargo test -q -p mossen-tui assistant_transcript_facts_format_main_task_tool_and_pending_rows`
  - `cargo test -q -p mossen-tui pending_assistant_finalization_owns_empty_and_terminal_rules`
  - `cargo test -q -p mossen-tui app_layers_subagent_records_under_task_parent`
  - `cargo test -q -p mossen-tui tool_use_stop_and_empty_completed_do_not_render_as_answer`
  - `cargo check -q -p mossen-tui`

2026-05-23 basic transcript row render boundary cleanup:

- Red-line framing: this is basic transcript-row construction cleanup only. It
  does not change command execution, slash-command routing, skill resolution,
  permission decisions, model-call construction, final-summary extraction, or
  agent-loop behavior.
- Root/app runtime code now uses Layer 1 helpers for System, User,
  CommandOutput, SkillInvocation, cancelled, and final-summary transcript rows
  instead of directly constructing `MessageData` variants.
- The same helpers cover user prompt echo, slash command output, unknown
  command errors, theme/output-style notices, Ctrl+T/Ctrl+S notices, skill
  invocation previews, cancellation rows, and final-summary marker rows.
- Added Layer 1 regression coverage for the basic row fields and W118 static
  smoke coverage that rejects root-runtime `MessageData` construction for
  those row types.
- Verified:
  - `python3 scripts/wave_w118_render_basic_transcript_boundary_smoke.py`
  - `cargo test -q -p mossen-tui basic_transcript_messages_format_user_system_command_skill_and_summary`
  - `cargo test -q -p mossen-tui app_layers_subagent_records_under_task_parent`
  - `cargo check -q -p mossen-tui`

2026-05-23 compact modal render boundary cleanup:

- Red-line framing: this is `/compact plan` and `/compact status` modal
  rendering cleanup only. It does not change compaction execution, engine
  history mutation, compact cancellation, hooks, permission decisions, model-call
  construction, or agent-loop behavior.
- Layer 2 now owns the compact plan/status viewport-independent render models,
  token dry-run estimate formatting, hook/status labels, and command-output body
  strings.
- Root/app runtime code now only collects compact state into render models and
  opens the modal, instead of formatting compact plan/status bodies inline.
- Added Layer 2 regression coverage for compact plan/status bodies and W119
  static smoke coverage that rejects reintroducing root-local compact modal
  formatters.
- Verified:
  - `python3 scripts/wave_w119_render_compact_modal_boundary_smoke.py`
  - `cargo test -q -p mossen-tui compact_plan_model_formats_dry_run_body`
  - `cargo test -q -p mossen-tui compact_status_model_formats_lifecycle_body`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_plan_slash_previews_without_mutating_history`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_status_and_cancel_keep_history_unmutated`
  - `cargo check -q -p mossen-tui`

2026-05-23 permissions slash routing:

- Red-line framing: this is slash-command routing and permission-mode selection
  wiring only. It does not change tool permission evaluation, approval prompt
  decisions, engine history, model-call construction beyond the already-wired
  permission mode field, or agent-loop behavior.
- `/permissions` with no arguments still opens the session permission-mode
  picker, preserving the visible Codex-like approval mode workflow.
- `/permissions allow|deny|list|reset` now reaches the `mossen-commands`
  registry instead of being swallowed by the TUI picker fast path, so permission
  rule commands are reachable from the same slash namespace.
- `/permissions full-auto`, `/permissions mode dont-ask`, and
  `/permission-mode accept-edits` now switch the current session mode directly;
  mode matching accepts common hyphen/space/apostrophe variants.
- Added keybinding coverage for registry routing and direct mode selection plus
  W120 static smoke coverage that rejects reintroducing unconditional
  `/permissions` picker interception.
- Verified:
  - `python3 scripts/wave_w120_permissions_slash_routing_smoke.py`
  - `cargo test -q -p mossen-tui --test keybinding_smoke permissions_slash_`
 - `cargo test -q -p mossen-tui permission_mode_choices_normalize_labels_and_codes`
 - `cargo check -q -p mossen-tui`

2026-05-23 slash catalog alias and argument hints:

- Red-line framing: this is slash-command discoverability and rendering metadata
  only. It does not change directive execution, permission evaluation, compact
  lifecycle behavior, or agent-loop semantics.
- The slash catalog now carries command aliases and argument hints from the
  directive registry, plus TUI-only aliases/hints for built-in panels. This
  makes `/help` and `/` typeahead reflect how the command router already
  accepts aliases such as `/settings`, `/cmds`, `/debug-raw`, and
  `/status-line`.
- Typeahead matching now scores aliases and argument hints, while accepting a
  suggestion still inserts the canonical slash command. Help output shows a
  compact usage label such as `/config [key=value]` and includes alias metadata
  in the description line.
- Added keybinding coverage for alias search and usage display plus W121 static
  smoke coverage that rejects dropping alias/hint metadata from the catalog.
- Verified:
  - `python3 scripts/wave_w121_slash_catalog_alias_hint_smoke.py`
  - `cargo test -q -p mossen-tui --test keybinding_smoke slash_catalog_matches_aliases_and_shows_argument_hints`

2026-05-23 auto-compact query replay wiring:

- Red-line framing: automatic compaction could report a compact boundary to
  the TUI without carrying the compacted messages back into the next model
  request, which leaves long sessions vulnerable to context growth even though
  the UI says compaction happened.
- `context::AutoCompactResult::Compacted` now includes the compacted message
  list from `compact_conversation()`, and the tracking state records reset
  failure count, last compact token count, and compact time after success.
- `dialogue::execute_turn_cycle()` now replaces `state.messages` and the
  immediate `messages_for_query` with the compacted messages before repairing
  orphan tool results and building the streaming request.
- The older `services::compact::auto_compact` path no longer returns a
  placeholder non-compacted result after threshold; it invokes
  `compact_conversation()`, adapts the result into `CompactionResult`, and
  preserves circuit-breaker behavior.
- Added W79 smoke and focused unit tests for context replay, service
  compaction, and circuit-breaker skip behavior.
- Verified:
  - `python3 scripts/wave_w79_auto_compact_query_replay_smoke.py`
  - `cargo test -q -p mossen-agent context::tests::auto_compact_returns_compacted_messages_and_updates_tracking`
  - `cargo test -q -p mossen-agent services::compact::auto_compact::tests`
  - `cargo check -q -p mossen-agent`
  - `rustfmt crates/mossen-agent/src/context/mod.rs crates/mossen-agent/src/dialogue.rs crates/mossen-agent/src/services/compact/auto_compact.rs --check`
  - `git diff --check -- crates/mossen-agent/src/context/mod.rs crates/mossen-agent/src/dialogue.rs crates/mossen-agent/src/services/compact/auto_compact.rs scripts/wave_w79_auto_compact_query_replay_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 SDK permission prompt fail-closed bridge:

- Red-line framing: the headless/SDK permission prompt path still had a
  placeholder allow branch, so a missing prompt bridge could silently approve
  tool execution instead of preserving the user-facing permission contract.
- `create_can_use_tool_with_permission_prompt()` now delegates to a
  callback-aware bridge and fails closed when no SDK MCP server advertises the
  prompt tool, no callable bridge exists, or the callback returns an error.
- SDK MCP server metadata is inspected for common tool declaration shapes,
  including strings, tool arrays, `availableTools`, `toolNames`, map keys, and
  `capabilities.tools`.
- Callback allow/deny decisions now map directly to `CanUseToolResult`,
  including updated tool input when the permission prompt returns one.
- Added W78 static smoke coverage and focused async tests for unavailable
  prompt tools, missing bridges, callback allow with updated input, and
  callback errors failing closed.
- Verified:
  - `python3 scripts/wave_w78_permission_prompt_fail_closed_smoke.py`
  - `cargo test -q -p mossen-cli print_handlers::tests`
  - `cargo check -q -p mossen-cli`
  - `cargo fmt -p mossen-cli`
  - `cargo fmt -p mossen-cli --check`
  - `git diff --check -- crates/mossen-cli/src/print_handlers.rs scripts/wave_w78_permission_prompt_fail_closed_smoke.py scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory extraction gate wiring:

- Red-line framing: W77 made the main session prompt/runtime expose team
  memory, but the background memory extraction path still always built the
  auto-only extraction prompt even when team-memory was enabled.
- `ExtractMemoriesConfig` now carries `team_memory_enabled`, defaulting false
  to preserve auto-only behavior unless the caller opts into the team-memory
  scope.
- `run_extraction()` now builds the combined auto/team extraction prompt when
  `team_memory_enabled` is true, while retaining the auto-only prompt for the
  default path.
- W50 smoke was upgraded from stale TS file paths to the current Rust gate
  implementation. It now verifies independent KAIROS/team-memory config keys,
  memdir rollout sources, runtime snapshot wiring, extraction prompt selection,
  and run-all registration.
- Verified:
  - `python3 scripts/wave_w50_team_memory_gate_smoke.py`
  - `cargo test -q -p mossen-agent services::extract_memories::tests`
  - `cargo check -q -p mossen-agent`
  - `cargo fmt -p mossen-agent`
  - `cargo fmt -p mossen-agent --check`
  - `git diff --check -- crates/mossen-agent/src/services/extract_memories/mod.rs scripts/wave_w50_team_memory_gate_smoke.py phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory prompt/runtime wiring:

- Red-line framing: after sync, watcher, write notification, path alignment,
  secret guard, and detector work, the CLI still had a hardcoded team-memory
  prompt/runtime gap: the model could be kept unaware of the same memory path
  that production tools and sync now use.
- Team-memory enablement now derives from auto-memory enablement plus explicit
  rollout flags or real sync availability, with disable flags taking priority.
  The old hardcoded false path is covered by unit tests.
- CLI memory prompts now include the auto/team memory instructions from
  `memdir::load_memory_prompt()` in `gather_memory_text()`, so sessions receive
  the same team-memory path contract that write/sync tooling enforces.
- Interactive runtime snapshots now report real memory and team-memory
  enabled/path/entrypoint/sync facts instead of placeholder false values, which
  makes debug/status surfaces reflect the active runtime contract.
- Team-memory path checks now use component-boundary matching for prompt/write
  validation so sibling directories do not inherit team-memory privileges.
- Added W77 static smoke coverage for rollout flags, sync availability, prompt
  injection, runtime snapshot facts, path-boundary tests, and
  `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w77_team_memory_prompt_runtime_smoke.py`
  - `cargo test -q -p mossen-cli memdir::tests`
  - `cargo test -q -p mossen-cli system_prompt::tests`
  - `cargo check -q -p mossen-cli`
  - `cargo fmt -p mossen-cli`
  - `cargo fmt -p mossen-cli --check`
  - `git diff --check -- crates/mossen-cli/src/memdir.rs crates/mossen-cli/src/system_prompt.rs crates/mossen-cli/src/interactive.rs scripts/run_all_smoke.sh`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory utility detector alignment:

- Red-line framing: after sync/watch and production writes moved to the current
  `memory/team` path, utility detectors still needed to stop making string-prefix
  or old TS-name assumptions that would skew analytics, collapse summaries, and
  helper classification.
- `memory_file_detection` now uses component-boundary path checks for config,
  auto-memory, team-memory, and memory-base paths instead of raw
  `starts_with`, so sibling directories such as `memory-other` or `team-other`
  are not misclassified.
- `team_memory_ops` now recognizes the current `memory/team` component shape
  while retaining the legacy `.mossen/team-memory` shape, and its write/edit
  classifier uses production tool names `Write` and `Edit`.
- Added W76 static smoke coverage and focused unit tests for current path shape,
  legacy path shape, sibling-boundary rejection, session config boundaries, and
  production tool-name matching.
- Verified:
  - `python3 scripts/wave_w76_team_memory_detector_alignment_smoke.py`
  - `cargo test -q -p mossen-utils memory_file_detection::tests`
  - `cargo test -q -p mossen-utils team_memory_ops::tests`
  - `cargo check -q -p mossen-utils`
  - `cargo fmt -p mossen-utils`
  - `cargo fmt -p mossen-utils --check`
  - `git diff --check -- crates/mossen-agent/src/services/team_memory_sync/secret_guard.rs crates/mossen-agent/src/services/team_memory_sync/service.rs crates/mossen-agent/src/services/team_memory_sync/mod.rs crates/mossen-tools/src/file_write.rs crates/mossen-tools/src/file_edit.rs crates/mossen-utils/src/memory_file_detection.rs crates/mossen-utils/src/team_memory_ops.rs scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory secret guard write blocking:

- Red-line framing: this closes the next memory execution-chain gap. The
  secret scanner existed and upload-time scanning existed, but real production
  `Write`/`Edit` could still put a secret on disk under team memory before sync
  logic saw it.
- The team-memory secret guard now uses the same resolved path detector as
  sync/watch instead of the old `.mossen` or `team-memory` marker heuristic, so
  it follows `projects/<project>/memory/team` and all explicit overrides.
- Production `Write` rejects detected secrets before directory creation and
  atomic persist. Production `Edit` rejects detected secrets before new-file,
  empty-file, and normal replacement writes.
- Added W75 static smoke coverage for resolved-path detection, no old marker
  heuristic, write-before-persist ordering, edit branch coverage, and
  `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w75_team_memory_secret_guard_smoke.py`
  - `cargo test -q -p mossen-agent services::team_memory_sync::secret_guard::tests`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-tools`
  - `cargo fmt -p mossen-agent -p mossen-tools`
  - `cargo fmt -p mossen-agent -p mossen-tools --check`
  - `git diff --check -- crates/mossen-agent/src/services/team_memory_sync/secret_guard.rs crates/mossen-agent/src/services/team_memory_sync/service.rs crates/mossen-agent/src/services/team_memory_sync/mod.rs crates/mossen-tools/src/file_write.rs crates/mossen-tools/src/file_edit.rs scripts/run_all_smoke.sh phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory CLI/sync path alignment:

- Red-line framing: this removes a production mismatch in the memory
  execution chain. The CLI memory prompt told the model to write team memories
  under the project auto-memory directory, while sync/watch defaults still
  targeted a separate `.mossen/team-memory` tree.
- Team-memory sync now derives its default local directory from the same path
  shape used by the CLI prompt: config/remote memory base,
  `projects/<sanitized-project>/memory/team`. `TEAM_MEMORY_DIR` and
  `MOSSEN_TEAM_MEMORY_DIR` remain explicit highest-priority overrides.
- `MOSSEN_COWORK_MEMORY_PATH_OVERRIDE` is handled as an auto-memory root and
  maps to `<override>/team`, matching `get_team_mem_path()`.
- Added W74 static smoke coverage for CLI prompt path shape, sync default
  path shape, memory override handling, remote memory base handling, and
  `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w74_team_memory_path_alignment_smoke.py`
  - `cargo test -q -p mossen-agent services::team_memory_sync::service::tests`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-tools`
  - `cargo fmt -p mossen-agent -p mossen-tools --check`
  - `git diff --check -- crates/mossen-agent/src/services/team_memory_sync/service.rs crates/mossen-agent/src/services/team_memory_sync/mod.rs crates/mossen-tools/src/file_write.rs crates/mossen-tools/src/file_edit.rs scripts/run_all_smoke.sh`
  - trailing-whitespace scan over the touched files

2026-05-23 team-memory write-tool notification wiring:

- Red-line framing: this closes the immediate memory execution-chain gap after
  watcher lifecycle wiring. Successful production tool writes can now wake the
  watcher instead of relying only on raw filesystem events.
- `team_memory_sync` now exposes a path-gated
  `notify_team_memory_file_write()` helper backed by the resolved
  team-memory directory, including env override support through the same
  runtime config used by sync.
- The real production `Write` and `Edit` tools now call the helper after their
  atomic write paths succeed. The helper ignores non-team-memory paths, so
  ordinary project file writes do not pay a sync cost.
- Added W73 static smoke coverage for service path detection, path-gated
  watcher notification, Write/Edit integration, and `run_all_smoke.sh`
  registration.
- Verified:
  - `python3 scripts/wave_w73_team_memory_write_notify_smoke.py`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-tools`
  - `cargo fmt -p mossen-agent -p mossen-tools --check`

2026-05-23 team-memory watcher CLI lifecycle wiring:

- Red-line framing: this continues the post-TUI memory execution-chain work.
  The watcher can now follow actual CLI session lifetime instead of existing
  only as an exported service API.
- CLI session routes now start background services after the subcommand
  short-circuit and before directive/tool registry setup, so interactive,
  oneshot, stdin, and input-file runs get the watcher while subcommands bypass
  it.
- `run()` now stops session background services after route completion,
  covering normal and error returns and giving the watcher a final pending
  flush opportunity.
- Added W72 static smoke coverage for subcommand bypass, session-route start,
  run-level cleanup stop, helper calls into the mossen-agent watcher API, and
  `run_all_smoke.sh` registration.
- Verified:
  - `python3 scripts/wave_w72_team_memory_watcher_lifecycle_smoke.py`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-cli`

2026-05-23 team-memory watcher debounce push wiring:

- Red-line framing: this continues the post-TUI memory execution-chain work.
  Runtime auth/repo/dir config now exists, so the watcher can move from a
  placeholder container to an actual file-change sync trigger.
- `start_team_memory_watcher()` now checks team-memory sync availability,
  starts the real filesystem watcher on the resolved team-memory directory,
  and avoids replacing an already-running watcher.
- The watcher now runs a background debounce worker: file events or explicit
  `notify_team_memory_write()` mark pending changes, wait for the debounce
  window, then call `push_team_memory()` with persistent sync state. Stop
  requests wake all worker loops and flush pending changes once.
- Permanent push failures (`NoOauth`, `NoRepo`, and non-retryable 4xx) now
  suppress further pushes until a remove event or session restart, matching the
  previous log message instead of only recording the state.
- Added watcher tests for retry/permanent-failure classification, write
  notification state, and suppression behavior.
- Verified:
  - `cargo fmt -p mossen-agent`
  - `cargo fmt -p mossen-agent --check`
  - `cargo test -q -p mossen-agent services::team_memory_sync::watcher::tests`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-agent`

2026-05-23 team-memory sync runtime config wiring:

- Red-line framing: this is the first small post-TUI execution-chain slice.
  The network sync, conflict handling, batching, local file IO, and secret scan
  were already present; this change removes the placeholder entry conditions
  that made the production path unreachable.
- `is_team_memory_sync_available()` now reflects real availability from an
  explicit team-memory token env var or existing hosted OAuth tokens. Sync
  endpoint config now accepts `TEAM_MEMORY_SYNC_URL`,
  `MOSSEN_TEAM_MEMORY_SYNC_URL`, or `MOSSEN_API_BASE_URL`, with the previous
  `https://api.mossen.ai` default preserved.
- Pull/push now derive the repository slug from
  `TEAM_MEMORY_REPO_SLUG`/`MOSSEN_TEAM_MEMORY_REPO_SLUG` or from git remotes
  (`origin` first, then the first configured remote). The local team-memory
  directory can be overridden with `TEAM_MEMORY_DIR` or
  `MOSSEN_TEAM_MEMORY_DIR`; otherwise it remains `.mossen/team-memory`.
- Added pure unit coverage for token selection, base URL cleanup, explicit and
  remote repo slug parsing, repo slug precedence, and local directory
  resolution without mutating process env or touching the network.
- Verified:
  - `cargo fmt -p mossen-agent`
  - `cargo fmt -p mossen-agent --check`
  - `cargo test -q -p mossen-agent services::team_memory_sync::service::tests`
  - `cargo test -q -p mossen-agent services::team_memory_sync`
  - `cargo check -q -p mossen-agent`

2026-05-23 `/compact plan` token dry-run preview:

- Red-line framing: this is still slash-command/TUI control-plane work. It
  improves the manual compaction surface and its user-visible telemetry without
  changing the model turn loop or task semantics.
- `/compact plan` no longer shows the old placeholder `messages * 256` token
  math. The preview now dry-runs the same half-history compaction shape used by
  `/compact run`, estimates tokens from real message content, shows compacted
  and recent-message counts, and reports when short histories may gain no token
  savings because summary overhead dominates.
- Actual compact completion feedback now computes the pre-compact token count
  from real `engine_history` content, while the compact service reports
  `remaining_token_count` from the resulting message list instead of a fixed
  per-message constant.
- Added smoke coverage for the richer `/compact plan` modal fields while
  preserving the non-mutating preview contract.
- Verified:
  - `cargo fmt -p mossen-tui -p mossen-agent`
  - `cargo fmt -p mossen-tui -p mossen-agent --check`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_plan_slash_previews_without_mutating_history`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_slash_surfaces_progress_state`
  - `cargo test -q -p mossen-agent services::compact::compact::tests::`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`

2026-05-23 `/permissions` request wiring smoke:

- Red-line framing: no new permission semantics were invented here. The agent
  side already has typed `PermissionMode` decisions for default, plan,
  acceptEdits, bypassPermissions, and dontAsk; this slice locks the TUI bridge
  into that existing execution path.
- Added a keybinding smoke that opens `/permissions`, selects `dontAsk`, submits
  the next prompt, and asserts the generated `PromptParams.permission_mode` is
  `PermissionMode::DontAsk`. This catches regressions where the picker updates
  visible state but fails to affect the engine request.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke selected_permission_mode_flows_into_next_engine_request`

## Reference Behaviors

Primary sources checked on 2026-05-21:

- Mossen Code status line docs:
  https://code.mossen.com/docs/en/statusline
- Mossen Code permissions docs:
  https://code.mossen.com/docs/en/permissions
- OpenAI Codex CLI features:
  https://developers.openai.com/codex/cli/features
- OpenAI Codex CLI slash commands:
  https://developers.openai.com/codex/cli/slash-commands
- OpenAI Codex permissions:
  https://developers.openai.com/codex/permissions
- OpenAI Codex open-source repo:
  https://github.com/openai/codex

Observed product principles:

1. The transcript is the source of user trust. Codex explicitly says actions
   stay surfaced in a transcript for review and rollback. Mossen must never
   replace a failed or blocked turn with raw protocol strings like `stop`,
   `null`, or an empty completion marker.
2. Blocking state is primary UI, not an overlay afterthought. Mossen Code
   treats shell/file permission as a structured permission system; Codex lets
   users switch approval modes inside the TUI with `/permissions`. Mossen must
   model approval as semantic state before rendering it.
3. The footer/status line is a contract. Mossen Code feeds structured session
   data to status-line commands; Codex exposes `/statusline` and `/status` for
   model, context, limits, roots, policy, and session facts. Mossen footer data
   must be assembled once as `FooterRenderModel`.
4. The TUI has inspection tools. Codex exposes `/raw`, `/diff`, `/ps`,
   `/status`, `/debug-config`, `/statusline`, and `/title`. Mossen needs normal
   semantic rendering plus an intentional debug/raw path, not accidental raw
   leakage in the main transcript.
5. Good terminal UI protects the loop from layout bugs. Long output,
   multibyte text, narrow width, and background work must not panic or make the
   agent look frozen.

## Current State

Done:

- Old terminal framework renderer tree and compatibility islands are removed.
- `RenderTranscript`, `RenderBlock`, and `ToolCardModel` exist.
- Messages now render through `RenderTranscript` and `RenderBlockWidget`.
- P0 tool inputs/results mostly normalize into semantic tool cards.
- Markdown, long Bash, protocol-noise, and several tool-card snapshots pass.
- Tool-section rendering now recognizes unified diff output and styles diff
  headers, hunks, additions, and removals semantically instead of treating
  them as undifferentiated stdout text.
- `crates/mossen-tui/tests/render_contract.rs` now exercises the true
  `App::render_for_test` path, not retired components or isolated snapshots,
  across product-state frames, inline approval/footer ordering, semantic
  protocol cleanup, height-cache determinism, and resize-storm/pathological
  content.
- The active render contract now includes a single-message tall-output
  regression. It first failed by showing a mid-message scratch-buffer slice
  instead of the real tail, then Layer 3 was fixed to render oversized text
  blocks by semantic virtual rows when a scratch buffer cannot cover the
  requested scroll offset.
- The active render contract now also covers the transition zone where the
  visible window starts inside the safe scratch buffer but extends past its
  bottom. It first failed by rendering rows 405-408 and then blank space;
  Layer 3 now switches the whole visible slice to semantic virtual rows when
  the requested range crosses the scratch boundary.
- The active render contract now also covers a single assistant block beyond
  `u16::MAX` virtual rows. It first failed by sticking around row 32766, then
  Layer 3 height measurement/cache was changed to preserve `usize` virtual
  heights and only cap actual scratch buffers at terminal-render time.
- The active render contract now covers async transcript appends while the
  user is manually row-scrolled inside a long message. It first failed by
  clamping `VirtualScroll.offset` from 2390 to 0 when `ApiRetry` appended a
  system message, then App transcript mutation handlers were changed to stop
  writing `messages.len()` into the row-based scroll model. Row totals are
  now updated only by `sync_message_scroll()` after Layer 3 measures actual
  content height from the semantic render surface.
- Layer 3 now has a text-block height consistency contract between
  `RenderBlockWidget::required_height()` and `MessagesWidget`'s semantic
  virtual-row fallback. It first failed for focused, narrow rich Markdown:
  the measured block height was 41 rows but the virtual slice source produced
  39 rows. The fallback now pads generated virtual rows to the same
  Markdown/wrap height estimator used by normal layout, preventing deep-scroll
  drift when a single rich assistant block exceeds ratatui's safe scratch
  buffer.
- The active render contract now includes a tall rich Markdown virtual-scroll
  fixture with CJK text, fenced Rust code, tables, and a tail anchor. It fails
  if the deep-scroll path leaks raw Markdown fences/table separators or loses
  the rendered code/table/tail anchors.
- The TUI input/search path now has a UTF-8 boundary hardening contract.
  `SearchInputState` and the old `hooks/text_input::TextCursor` previously
  moved and deleted by byte offset, so a CJK query/prompt could leave the
  cursor inside a multibyte character and panic before the next frame could
  render. Cursor movement, delete/backspace, kill-to-start/end, and old
  text-hook word deletion now clamp to valid char boundaries before slicing.
  This is tracked here because search/filter/prompt editing are visible TUI
  surfaces and must not unwind the render loop.
- Layer 2 now strips ANSI control sequences and expands tabs for all visible
  semantic text before it reaches Markdown/tool/footer rendering. This covers
  assistant/user/thinking text as well as tool-card sections.
- As of this file, `RenderSurface`, `BlockingRenderModel`,
  `FooterRenderModel.blocking`, and `ApprovalBlockWidget` exist.
- App inline approvals now render from `ApprovalRenderModel` instead of
  directly from `PermissionPromptState`.
- App status bar now builds `FooterRenderModel` first and renders it directly
  through `widgets::footer::FooterWidget`.
- Top-level app frames now build one `RenderSurface` per frame and pass that
  surface into transcript, inline approval, scroll height, and footer
  rendering.
- Blocking-state coverage exists for Bash approval, file approval, MCP channel
  approval, cost threshold, idle return, and error command output.
- New approval decisions now persist as Layer 1 sidecar records on `App` and
  render as semantic transcript blocks for allow/always/deny/cancel decisions;
  the old internal marker path remains only for historical compatibility.
- `render_lifecycle` now exists as the first Layer 1 compatibility boundary:
  it turns current `MessageData` plus approval sidecar facts into
  viewport-independent transcript records before Layer 2 builds
  `RenderTranscript`.
- `RenderTranscript::from_records` now consumes `TranscriptRecord` directly
  for block ids, source indices, lifecycle state, tool pairing, and approval
  anchors. It no longer converts the full record list back to `MessageData`
  before building Layer 2.
- Engine `ToolUseBlock.id` values are now preserved as Layer 1 record id
  overrides on `App`, so visible tool-card ids and active/persisted approval
  anchors can use stable engine tool ids instead of list-position guesses.
- Engine tool results now carry `tool_use_id` through `SdkMessage::ToolUseSummary`;
  `App` records stable result ids plus parent tool-use ids, and
  `RenderTranscript::from_records` can pair ToolUse/ToolResult records by that
  parent relationship instead of adjacency alone.
- Layer 1 now assigns session-local render turn ids such as `turn-0001` at
  the TUI App boundary. Main user/assistant/tool/result/final-summary records
  and structured render events share the same turn id, and `/raw`,
  `/timeline`, `/status`, and `/ps` expose it without touching task execution
  code.
- Layer 1 now ingests raw `SdkMessage` events at the TUI App boundary into a
  capped `RawEngineEventRecord` journal with sequence, scope, kind, turn id,
  summary, and serialized payload preview. `/raw` shows this explicit engine
  event journal separately from normal semantic transcript rendering.
- Layer 1 now has a versioned `RenderSessionSnapshot` JSON roundtrip for
  `TranscriptRecords`, approval/final-summary sidecars, raw engine events,
  session id, current/latest turn ids, and next sequence counters. `/raw`
  surfaces snapshot version/count/serialized-size metadata without touching task
  execution code.
- Layer 1 can now persist and restore `RenderSessionSnapshot` JSON files through
  parent-directory creation, pretty JSON, temp-file write, file sync, atomic
  rename, JSON load validation, and App wrappers for the current TUI snapshot.
- `/render-snapshot` and `/snapshot` are now TUI-only commands for explicitly
  exporting the current render session snapshot or validating/loading snapshot
  metadata. The default export path is
  `<cwd>/.mossen/render-sessions/<session>.json`, explicit paths are supported,
  and load/validate mode reports metadata without mutating the live session.
- `/render-snapshot restore <path>` now hydrates the live TUI render session from
  a snapshot: visible transcript rows, record id/parent/turn overrides,
  approval sidecars, final-summary sidecars, raw engine event journal,
  session/turn sequence counters, and interrupted-turn render state are restored
  while engine execution remains explicitly not resumed.
- Non-empty TUI render sessions now autosave a `RenderSessionSnapshot` at the end
  of `App::run` to the sanitized default snapshot path. Empty sessions are
  skipped, autosave status/error is tracked on `App`, and `/raw` reports the
  current autosave path/status.
- `/resume` and `/continue` now restore the latest valid render snapshot from
  `<cwd>/.mossen/render-sessions` or from an explicit snapshot path argument.
  `/render-snapshot restore` and `/render-snapshot hydrate` without a path use
  the same latest-snapshot discovery, while `latest`/`load latest` remain
  metadata-only inspection paths.
- `App::run` now performs a startup-only latest render snapshot restore before
  entering the event loop when the live App has no existing render content.
  Startup restore hydrates transcript/record/raw-event state only, records
  status in `/raw`, skips initialized App state, and does not resume
  engine/tool execution.
- Layer 1 now exposes a generalized `TranscriptRelationIndex` over arbitrary
  `parent_id` relationships. It reports roots, child records, parent groups,
  and missing parent ids without terminal layout. Subagent/task records created
  by the TUI are now parented under stable `task:{id}` roots, and `/raw` shows
  relation counts plus parent/child debug rows.
- Approval decision facts now carry stable session-local ids from `App`,
  rather than deriving visible decision block ids from list position.
- Tool transcript blocks now use stable `tool-{source_index}` ids derived
  from the ToolUse record, so the block id no longer changes when a
  ToolResult row arrives.
- `RendererProfile` exists for responsive terminal rendering and is consumed by
  the semantic block renderer's tool-output budgets.
- `widgets/message.rs` has been reduced to message data and tool-name display
  helpers only; the old translated terminal-translated `MessageRenderer` renderer is gone.
- The old translated message-row renderer is gone; the active transcript
  renderer is now the semantic
  `MessagesWidget -> RenderTranscript -> RenderBlockWidget` path in
  `widgets/render_block.rs`.
- `components/messages.rs` has been deleted after confirming it had no live
  workspace call sites.
- The old `root_large` transcript-list, scroll-handler, message-list, and
  message-row compatibility renderers have been removed; `root_large` keeps
  selector/restore and other non-transcript dialog components.
- `retired_compact_root` has been removed from the compiled component tree after
  call-site checks showed no workspace usage.
- `root_medium` has been reduced to the live `IdleReturnDialogState` and
  `IdleReturnDialogWidget` surface; the unused compatibility grab bag is gone.
- `MessageSelector` now builds its rows from `RenderTranscript` semantic block
  summaries instead of raw `MessageData.content`, so tool JSON/protocol text
  does not leak through that modal path.
- `MessagesWidget` can render a supplied `RenderTranscript` without a
  backing `MessageData` slice, and its height, divider, and collapsed-result
  logic now consume semantic block/source-index data instead of inspecting raw
  message types.
- First UTF-8 boundary hardening pass completed for root text helpers that
  previously sliced user/model/tool-facing strings by byte index.
- Second UTF-8 boundary hardening pass completed for search input and the old
  text hook used by compatibility input modes. Active evidence:
  `hooks::search_input::tests::search_input_edits_multibyte_text_on_char_boundaries`,
  `hooks::text_input::tests::text_cursor_edits_multibyte_text_on_char_boundaries`,
  `hooks::text_input::tests::text_input_ctrl_commands_keep_unicode_offsets_valid`,
  `render_snapshot_app_frame_search_modal_handles_multibyte_rows`, and
  `permission_summary_truncates_multibyte_input_without_panic`.
- MCP channel approval now resolves a stable transcript anchor when there is a
  matching MCP tool block for the same server. The active approval panel and
  its persisted allow/deny decision use the same anchor, so the decision stays
  next to the triggering tool context in the normal App frame.
- `/raw` now opens an explicit raw transcript debug view. Normal transcript
  rendering continues to hide raw tool JSON/protocol payloads, while the raw
  modal intentionally exposes raw `MessageData` content, Layer 1 record
  metadata, sidecar approvals/final summaries, and visible semantic block ids.
- `/statusline` now opens a TUI-only session-local footer item configuration
  panel. `FooterRenderConfig` travels through `AppState -> FooterRenderModel ->
  FooterWidget`, so configurable project/model/activity/context/cost/message
  items share the same semantic footer path while core blocking/turn status
  remains visible.
- Footer/status-line render configuration now persists as project-local JSON at
  `<cwd>/.mossen/render-ui/statusline.json`. Presets and modal toggles save
  automatically, `App::run` loads the config on startup when no runtime
  override already exists, and `/raw` reports load/save status without touching
  engine/tool execution.
- The persisted footer config now supports external status-line command
  compatibility. It can load `statusLine: { type: "command", command: ... }`
  shaped JSON, `/statusline command <shell command>` configures the hook, and
  the TUI executes it on a background tick with clamped interval/timeout so
  slow commands cannot block scrolling, input, or frame rendering. The footer
  renders the last stable stdout value and `/raw` reports command state/errors.
- The App render hot path now caches `RenderTranscript` by a render revision
  plus a lightweight transcript-shape key. Normal App mutation paths invalidate
  through `note_transcript_changed()`, streaming text/thinking deltas invalidate
  as content changes, and direct public `messages` fixture appends still miss
  the cache via the shape key. This avoids rebuilding Layer 2 transcript
  semantics on unchanged frames without freezing streaming output or manual
  scroll positions.
- The App frame now has a semantic active activity panel above transcript
  history. It is derived from `RenderActivityState` or the current blocking
  model, uses display-width clipping plus ASCII fallback, and gives streaming
  command/output/plan/error/blocking activity an in-place render target without
  rereading task execution state.
- `/diff` now opens an intentional semantic diff review surface. It scans
  semantic tool sections for unified diff bodies, parses them into grouped file
  hunks with line numbers and add/remove counts, and renders a navigable
  file-detail modal with per-file fold/unfold instead of exposing raw stdout
  JSON or task execution state.
- `/ps` now opens an intentional semantic process/status inspection surface. It
  builds a read-only `ProcessListRenderModel` from existing App render state:
  current turn, blocking, active activity, compact progress, TodoWrite tasks,
  task-store snapshots, foreground/background counts, and teammate agent state.
  It does not reach into or mutate task execution code.
- `/status` now opens an intentional semantic session overview surface. It
  builds a `StatusOverviewRenderModel` from the same footer/process/App facts
  used by top status, footer, and `/ps`, then renders session, turn, policy,
  workspace, API-key state, context, cost, MCP, TodoWrite, and agent summaries
  through a dedicated widget. It does not expose secret values or touch task
  execution code.
- `/commands` now opens an intentional semantic command execution history
  surface. It extracts Bash/PowerShell command runs from semantic tool cards,
  combines current render activity when a command is active, renders
  command/cwd/status/exit/duration/stdout/stderr/full-log availability, and
  supports read-only Space/Enter full-log expansion plus PageUp/PageDown detail
  scrolling through a dedicated widget. It does not reach into or mutate task
  execution code.
- `/errors` now opens an intentional semantic error history surface. It
  extracts structured error blocks, failed command cards, and current error or
  retry activity from the existing render surface, then renders source,
  summary, key detail, hidden-detail counts, retry hints, and read-only
  expandable details through a dedicated widget. It does not reach into or
  mutate task execution code.
- `/results`, `/summaries`, and `/final-summary` now open an intentional
  semantic final-summary history surface. It extracts structured final-summary
  blocks from the render transcript, renders task status, changed files,
  commands, verification results, residual risks, and notes through a
  dedicated widget, and supports read-only Space/Enter detail expansion plus
  PageUp/PageDown detail scrolling. It deliberately does not shadow the
  existing `/summary` directive and does not touch task execution code.
- `/approvals`, `/approval-history`, and `/approval-log` now open an
  intentional semantic approval history surface. It extracts approval decision
  rows from semantic transcript sidecar blocks, can model a pending
  `ApprovalRenderModel` row, renders pending/allowed/denied/cancelled counts,
  risk/detail/action/body/anchor facts through a dedicated widget, and
  supports read-only Space/Enter detail expansion plus PageUp/PageDown detail
  scrolling. It deliberately does not shadow or mutate the existing
  `/permissions` directive and does not touch task execution code.
- `/debug-config` now opens an intentional redacted semantic configuration
  inspection surface. It renders session, engine, policy, renderer, footer,
  and runtime facts through `DebugConfigRenderModel` and `DebugConfigWidget`,
  exposes only configured/missing API-key state and extra-body key names, and
  supports read-only scrolling without printing raw request bodies,
  credentials, or touching task execution code.
- `/title` and `/session-title` now open an intentional semantic terminal title
  inspection/configuration surface. Manual titles are sanitized before reaching
  terminal OSC title output, persist across streaming/ready chrome, can be
  reset with `Ctrl+U`, and render through `SessionTitleRenderModel` plus
  `SessionTitleWidget` without touching task execution code.
- `/files`, `/changes`, and `/changed-files` now open an intentional semantic
  file-change summary surface. It consumes existing Layer 2 file-change
  summaries from tool results/final summaries, renders grouped status/count
  facts through `FileChangeListRenderModel` and `FileChangesWidget`, and does
  not query git, expose raw file-edit JSON keys, or touch task execution code.
- `/timeline`, `/events`, and `/render-events` now open an intentional
  semantic render-event lifecycle surface. It records the structured
  `RenderEvent` rows applied to the main TUI, caps the session-local history,
  renders refresh/history/stage/scope facts through `RenderTimelineRenderModel`
  and `RenderTimelineWidget`, and does not expose raw command JSON or touch
  task execution code.

Not done:

- `root_large` still contains non-transcript compatibility UI components.
  Dialogs, settings, tasks, status, and misc component modules still need the
  same call-site proof before deletion/migration.
- Footer has a semantic model, a session-local item configuration path,
  project-local JSON persistence, external command compatibility, and richer
  status-line presets.
- Layer 1 has a compatibility `TranscriptRecords` boundary, stable
  approval-decision ids, stable engine tool-call ids for the main tool-use
  path, stable tool-result ids from the execution path, first parent/child
  ToolUse/ToolResult relationships, stable compatibility tool anchors, and a
  record-native Layer 2 bridge. The full reducer still needs broader relation
  hydration for sidecar and turn-level records.
- Renderer profile migration has started, but root/profile cleanup and full
  compatibility-island retirement are still pending. Dialogs, settings, task,
  and status components still need the same call-site proof before
  deletion/migration.
- A top-level render panic boundary exists, with first coverage for synthetic
  render panics, malformed known-tool JSON, multibyte text, viewport resize,
  search/prompt UTF-8 editing boundaries, and a 1000+ block transcript
  fixture. Remaining gap: continued root compatibility parsing cleanup outside
  the final-summary command, tool-preview, approval-summary, and compact-plan
  preview paths, with assistant content extraction now moved to Layer 1 and
  permission-mode choice rendering moved to Layer 2.
- The active product render contract is now a required floor, but it is not a
  production-ready declaration by itself. It covers deterministic fuzz,
  generated property fuzz, simulated streaming soak budget,
  streaming/resize interleaving, long-transcript scroll, and large-session
  budget smoke tests. Remaining product gaps are now focused on the remaining
  root compatibility parsing cleanup outside the command-summary and
  tool-preview/approval-summary/compact-plan-preview/assistant-content/
  permission-mode paths.

## Three-Layer Pipeline

Layer 1: Event and lifecycle reducer

- Input: engine stream events, tool lifecycle, permission requests, MCP
  approval, task/subagent events, cost/rate-limit/idling notices.
- Output: stable records with ids, parent/child relationships, lifecycle, raw
  payload, and normalized finish reasons.
- No terminal widths, box drawing, footer strings, or protocol-debug text.

Layer 2: Semantic RenderModel

- Input: Layer 1 records or the current compatibility adapter from
  `MessageData`.
- Output: `RenderSurface`, containing transcript blocks, approval anchors,
  footer model, and blocking state.
- Must answer: what should the user see and what is blocking the turn?
- No viewport decisions, raw byte slicing, duplicate JSON parsing, or widget
  heuristics.

Layer 3: Responsive Terminal Renderer

- Input: one `RenderSurface` plus viewport/theme/profile.
- Output: ratatui cells.
- Owns markdown/code/diff layout, CJK/emoji width, folding, clipping,
  virtualization, sticky scroll, and terminal-title/footer variants.
- May show debug/raw data only when a deliberate debug/raw mode is active.

## Execution Batches From Here

Batch A: RenderSurface wiring

Status: completed on 2026-05-21

- Make top-level `render_fullscreen` and `render_inline` build one
  `RenderSurface` per frame.
- Make transcript, inline approval, footer, spinner/blocking labels consume
  the same surface.
- Add tests proving approval/footer/blocking agree for Bash, file, MCP,
  cost-threshold, idle-return, and error states.

Gate:

- Semantic test plus frame snapshot for every blocking state.
- No direct footer turn-state string assembly outside the RenderSurface path.

Batch B: Approval history and anchors

Status: in progress - semantic sidecar plus Layer 1 record skeleton completed on 2026-05-21

- After allow/always/deny, append a compact visible decision block under the
  relevant tool card.
- Store anchor ids from the tool lifecycle, not by searching the visible list
  when possible.
- Make MCP approvals use the same decision block path.

Gate:

- Snapshot: tool request -> inline approval -> decision -> tool result.
- Narrow and wide snapshots both show the decision without modal overlap.

Batch C: Renderer profile migration

Status: in progress on 2026-05-21 - RendererProfile first slice and old message-row renderer retirement complete

- Introduce `RendererProfile`: large, medium, small.
- Move viewport-specific behavior out of `root_*` parsing paths into profile
  layout.
- Continue retiring root compatibility parsing now that `render_block.rs` is no
  longer compiled.

Gate:

- Snapshot matrix at 60, 80, 120, and 160 columns.
- `rg` check confirms root renderers do not parse tool JSON.

Batch D: Rich content hardening

Status: started on 2026-05-21 - malformed known-tool payload, UTF-8 root-helper, and explicit `/raw` debug-view first slices complete

- Markdown tables/code blocks/diffs get bounded, readable renderers.
- Read uses syntax highlighting through semantic code sections.
- Diff cards expose file, added/removed counts, hunks, and clipped preview.
- `/raw`-style debug rendering is intentionally separate from normal rendering.

Gate:

- Markdown/code/diff snapshots for CJK, emoji, long lines, ANSI, and malformed
  payloads.
- Normal transcript cannot leak raw JSON keys for P0 tools.

Batch E: Production performance and panic boundary

Status: started on 2026-05-21 - height-cache and top-level panic-boundary first slices complete

- Add height cache keyed by block id, width, profile, expansion, and theme.
- Add large transcript fixture with 1000+ blocks.
- Add top-level render panic boundary that shows a structured render-error
  block instead of exiting the process.
- Measure render time and memory in CI-friendly tests.

Gate:

- Large transcript render stays within the agreed local threshold.
- No panic on malformed tool payload, multibyte clipping, resize, or huge
  output.

Batch F: Active product render contract

Status: expanded on 2026-05-22 - product-state matrix, inline approval/footer,
semantic cleanup, height-cache, scroll, rich Markdown/code/table/diff,
streaming/resize interleave, deterministic fuzz, single-tall-message
virtualization, enormous-message virtual-height cap, streaming tall-message
scroll policy, scratch-boundary slicing, async-append manual-scroll
preservation, and large-session budget slices complete

- Add a non-snapshot product gate that renders full `App` frames through
  `App::render_for_test` and `ratatui::TestBackend`.
- Cover realistic product states:
  - streaming assistant text with Markdown/code/table content;
  - Bash/Glob/TodoWrite/Read semantic tool cards;
  - inline shell approval;
  - help, MCP, and tasks dialogs;
  - footer model/cost/message count/model label;
  - session-local footer item configuration;
  - long output, malformed known-tool payloads, ANSI, tabs, CJK, combining
    marks, and resize-storm width/height changes on the same `App`.
- Assert product invariants instead of pixel-only snapshots:
  - frame is nonblank;
  - normal transcript contains no raw protocol noise;
  - no Rust panic/banner text leaks into the terminal;
  - approval remains above prompt/footer;
  - malformed payload details stay hidden;
  - height cache does not change measured layout;
  - scroll offset remains bounded after resize storms.
  - long transcripts remain usable: sticky bottom shows the tail, manual
    scroll-up reaches the head, and returning to bottom shows the tail again.
  - single extremely tall assistant messages also remain usable: sticky bottom
    reaches the real tail instead of the deepest safe scratch-buffer slice.
  - tall-message scrolling remains continuous when the viewport crosses the
    scratch-buffer boundary instead of leaving blank rows.
  - enormous single assistant messages beyond `u16::MAX` virtual rows reach
    their true tail instead of stopping at the old capped virtual height.
  - streaming tall messages preserve scroll policy: sticky bottom follows the
    growing tail, manual scroll-up stays pinned while new chunks arrive, and
    `Ctrl+L` returns to the current bottom.
  - async transcript appends preserve a user's manual row-scroll offset inside
    long messages instead of clamping to message count.
  - streamed assistant deltas remain visible while viewport sizes change.
  - rich Markdown, code blocks, tables, and diff output render as structured
    terminal content instead of raw source syntax.
  - deterministic fuzz corpus cannot leak malformed payload keys or control
    sequences.
  - large sessions stay under the CI smoke budget for active App rendering.

Gate:

- `cargo test -p mossen-tui --test render_contract -- --nocapture` passes.
- The gate must fail on raw protocol leakage, visible ANSI escapes, malformed
  tool JSON leakage, approval/footer ordering regression, scroll offset
  overflow, invisible streamed assistant text, raw Markdown/table syntax
  leakage, large-session stalls, or render panic text.
- Snapshot tests remain useful, but they are supporting evidence only.

## Anti-Placebo Rules

No claim counts unless it has evidence:

- A semantic change needs a unit test at Layer 2.
- A visible change needs a ratatui buffer snapshot at Layer 3.
- An App/frame rendering claim needs
  `cargo test -p mossen-tui --test render_contract -- --nocapture`; snapshots
  alone do not close product rendering.
- A bug fix needs a negative assertion for the exact failure class.
- A performance claim needs a fixture and timing/memory output.
- A product-comparison claim needs a source link or code reference.
- "Looks better" is not accepted. The artifact must name the before/after
  behavior and the command that verifies it.

Required regression classes:

- No hidden approval behind idle/cost/help overlays.
- No naked `stop`, `null`, `terminal=Completed`, or raw protocol JSON in normal
  transcript.
- No raw byte slicing of user/model/tool text.
- No visible ANSI/control escape text in normal transcript, assistant,
  thinking, or tool sections.
- No unbounded default tool output.
- No full-process panic from rendering.
- No footer state that contradicts the visible transcript.
- No background/subagent activity that covers message text.

## Evidence Log

2026-05-22 slash compact plan and permission picker slice:

- Added `/compact plan`/`/compact preview`/`/compact status` as UI-only
  previews. They open a `CommandOutput` modal with message/token estimates and
  do not mutate `engine_history`; `/compact`, `/compact run`, and
  `/compact apply` keep the existing compaction behavior.
- Added `/permissions`, `/permission-mode`, and `/approval-mode` as TUI picker
  commands for the session-visible permission mode. The picker updates the
  same permission-mode label used by footer/top-status/status/debug surfaces
  and leaves transcript feedback.
- Added slash suggestions for `permissions` and `permission-mode` so typeahead
  exposes the new mode chooser.
- Added regression coverage:
  - `compact_plan_slash_previews_without_mutating_history`
  - `permissions_slash_picker_updates_visible_session_mode`
  - `slash_typeahead_lists_commands_and_accepts_with_prefix`
- Verification:
  - `cargo fmt -p mossen-tui --check` passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke slash_typeahead_lists_commands_and_accepts_with_prefix -- --nocapture` passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_plan_slash_previews_without_mutating_history -- --nocapture` passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke permissions_slash_picker_updates_visible_session_mode -- --nocapture` passed
  - `cargo test -q -p mossen-tui --lib` -> 290 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 20 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 54 passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 adaptive frame pacing and frame-duration telemetry slice:

- The main TUI run loop now measures real `terminal.draw(...)` duration for
  drawn frames and stores last/max/average frame-duration counters in `App`.
- Active streaming/status animation redraw cadence is now adaptive: the base
  interval remains 66 ms, slow frames stretch the next active-animation interval
  to twice the previous frame duration, capped at 250 ms.
- Idle dirty-frame behavior remains unchanged: clean idle frames are skipped,
  and dirty input/engine/render events still draw immediately.
- `/raw` now exposes `last_frame_duration_ms`, `max_frame_duration_ms`,
  `avg_frame_duration_ms`, and `active_frame_interval_ms` alongside
  dirty/drawn/skipped scheduler counters.
- Added regression coverage:
  - `render_frame_scheduler_adapts_after_slow_frame`
  - targeted raw/debug separation:
    `render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript`
  - targeted large-session frame budget:
    `app_render_contract_large_session_stays_within_budget`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 290 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 54 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 dirty-frame render scheduler slice:

- Added an App-level dirty-frame scheduler so unchanged idle turns skip
  `terminal.draw(...)` instead of repainting the whole ratatui surface on every
  tick.
- Input, mouse, resize, engine messages, transcript changes, and render events
  mark the frame dirty; active streaming and external status-line animation can
  still redraw without new transcript events.
- Active animation redraw is paced by `ACTIVE_RENDER_FRAME_INTERVAL` at 66 ms,
  which keeps spinner/stream status live while avoiding high-frequency idle
  redraw churn.
- Tick handling now compares a render-state fingerprint before and after the
  tick path, so background state changes can mark the frame dirty without making
  every idle tick dirty.
- `/raw` reports `render frame scheduler` dirty/active/drawn/skipped/age stats
  below the existing transcript cache stats for debugging redraw cadence.
- Added regression coverage:
  - `render_frame_scheduler_skips_idle_and_paces_active_animation`
  - targeted manual-scroll stability:
    `app_render_contract_async_append_preserves_manual_row_scroll`
  - targeted streaming scroll policy:
    `app_render_contract_streaming_tall_message_keeps_scroll_policy`
  - targeted raw/debug separation:
    `render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 289 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 54 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 render transcript hot-path cache slice:

- Added a `RenderTranscript` cache in `App` keyed by render revision plus
  transcript shape so unchanged frames reuse the Layer 2 semantic transcript
  model instead of rebuilding it for every panel/render pass.
- `note_transcript_changed()` now invalidates the cache, and streaming
  text/thinking deltas call it after mutating the pending assistant message so
  live output does not render stale content.
- The cache key also observes public transcript shape changes, covering
  integration fixtures and external callers that append to `app.messages`
  directly before opening semantic panels.
- `/raw` reports render transcript cache revision/cached/hit/miss state at the
  bottom of the debug view, keeping first-screen raw records stable.
- Added regression coverage:
  - `render_transcript_cache_reuses_model_until_transcript_changes`
  - targeted scroll stability checks:
    `app_render_contract_async_append_preserves_manual_row_scroll`
  - targeted streaming freshness checks:
    `app_render_contract_streaming_tall_message_keeps_scroll_policy`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 288 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 54 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 external status-line command compatibility slice:

- Extended `FooterRenderConfig` with optional external status-line command
  config and `FooterItem::ExternalStatus`, keeping it out of the default
  footer unless a command is configured.
- The project-local status-line config loader now accepts both Mossen render
  config JSON and compatibility-shaped `statusLine: { type: "command",
  command: ... }` JSON.
- `/statusline command <shell command>` configures and persists an external
  status-line command; `/statusline clear-command` disables it.
- Ticks execute the configured command in the background with stdin JSON,
  clamped interval/timeout, stale-result filtering, and last-good-output
  rendering so slow commands do not block input, scrolling, or frames.
- `/raw` now reports external status-line configured/in-flight/output/error
  state for inspection.
- Added regression coverage:
  - `footer_statusline_loads_external_command_compat_shape`
  - `external_statusline_command_tick_is_nonblocking_and_stable`
  - `render_snapshot_external_statusline_output_is_stable_footer_chrome`
  - targeted `builtin_slash_commands_open_expected_ui` coverage for
    `/statusline command`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 287 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 54 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 persistent status-line render config slice:

- Added project-local footer/status-line config persistence at
  `<cwd>/.mossen/render-ui/statusline.json` using pretty JSON plus temp-file
  write, file sync, and atomic rename.
- `FooterRenderConfig` and `FooterItem` are now JSON serializable. `/statusline`
  presets and modal toggles save automatically; `/statusline save|load|reload`
  and `/statusline path` expose explicit control.
- `App::run` loads the persisted config at startup when the App still has the
  default runtime footer config, and skips loading when a caller has already
  installed a non-default runtime config.
- `/raw` now reports status-line config persistence status, path, and error so
  render chrome state is inspectable from the terminal.
- Added regression coverage:
  - `footer_statusline_config_persists_to_project_file`
  - `footer_statusline_startup_load_skips_non_default_runtime_config`
  - `render_snapshot_statusline_config_persists_across_startup`
  - targeted `builtin_slash_commands_open_expected_ui` coverage for modal
    toggle persistence
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 284 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 53 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 TUI startup render snapshot restore flow slice:

- Added startup latest render snapshot restore at the beginning of `App::run`.
  It restores only when the App has no existing render content, so a launched
  session cannot overwrite already-initialized transcript or pending engine UI.
- Startup restore reuses the same latest valid `.json` snapshot discovery under
  `<cwd>/.mossen/render-sessions` and hydrates the existing render snapshot
  state without starting engine/tool execution.
- `/raw` now reports startup restore status, path, and error text alongside the
  autosave state so the render lifecycle is inspectable from the terminal.
- Added regression coverage:
  - `startup_restore_hydrates_latest_render_snapshot_when_empty`
  - `startup_restore_skips_when_app_already_has_render_content`
  - `run_restores_latest_render_snapshot_before_first_loop`
  - `render_snapshot_startup_restore_hydrates_visible_transcript`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 282 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 52 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 TUI resume latest render snapshot flow slice:

- Added latest valid render snapshot discovery under
  `<cwd>/.mossen/render-sessions`, filtering to valid `.json`
  `RenderSessionSnapshot` files and choosing the newest modified snapshot.
- `/render-snapshot restore` and `/render-snapshot hydrate` without a path now
  restore the latest snapshot. `/render-snapshot latest` and `load latest`
  inspect latest metadata without mutating the live session.
- `/resume` and `/continue` now hydrate the latest render snapshot, or an
  explicit snapshot path when provided, replacing stale live transcript rows
  while keeping engine execution explicitly not resumed.
- Added regression coverage:
  - `resume_command_restores_latest_render_snapshot`
  - `resume_command_reports_when_no_render_snapshot_exists`
  - `render_snapshot_resume_restores_latest_autosaved_render_session`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 279 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 51 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 render snapshot autosave on session end slice:

- Added `App::autosave_render_session_snapshot`. It writes non-empty TUI render
  sessions to the default
  `<cwd>/.mossen/render-sessions/<session-or-turn>.json` path and skips empty
  sessions so a blank launch does not create snapshot directories.
- `App::run` now calls the autosave helper after the main loop exits, providing
  a render-layer shutdown persistence boundary without touching agent/tool/task
  execution paths.
- `/raw` now reports autosave status, path, and error text so the explicit debug
  surface can prove whether the render session has a durable snapshot boundary.
- Added regression coverage:
  - `render_snapshot_autosave_writes_default_snapshot_when_session_has_content`
  - `render_snapshot_autosave_skips_empty_session`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 277 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 50 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 TUI render snapshot restore/hydration command slice:

- Added `/render-snapshot restore <path>` and `/render-snapshot hydrate <path>`.
  Restore rebuilds the current TUI render session from
  `RenderSessionSnapshot`: transcript messages, record id/parent/turn overrides,
  approval sidecars, final-summary sidecars, raw engine event history,
  session/current-turn ids, and next sequence counters.
- Restore resets stale live transcript/pending engine UI state, clears layout
  cache/scroll state, preserves interrupted-turn state as `Streaming`, and makes
  the execution boundary explicit: restoring a render snapshot does not resume
  the engine/tool process.
- Added regression coverage:
  - `render_snapshot_restore_command_hydrates_current_tui_state`
  - `render_snapshot_restore_preserves_interrupted_turn_state`
  - `render_snapshot_restore_hydrates_visible_transcript`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 275 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 50 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 TUI render snapshot export/load validation command slice:

- Added TUI-only `/render-snapshot`, `/snapshot`, and `/render-session` command
  handling. Save/export writes the current `RenderSessionSnapshot` to either an
  explicit path or the sanitized default
  `<cwd>/.mossen/render-sessions/<session>.json`.
- Added load/inspect/validate mode that reads a snapshot file and reports
  version, session, turn, record, raw-event, and sequence metadata without
  mutating the live TUI session.
- Added slash-command suggestions/categories and a visible `CommandOutput`
  result modal for success and error states.
- Added regression coverage:
  - `render_snapshot_command_exports_current_session_to_explicit_path`
  - `render_snapshot_command_uses_sanitized_default_session_path`
  - `render_snapshot_command_exports_render_session_snapshot_modal`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 273 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 49 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 Layer 1 render session snapshot persistence/load helper slice:

- Added `RenderSessionSnapshot::to_json_pretty`, `save_json_file`, and
  `load_json_file`. Snapshot saves now create parent directories, write pretty
  JSON to a sibling temp file, sync it, rename it into place, and map malformed
  JSON loads to `InvalidData`.
- Added `App` save/load wrappers for the current TUI render snapshot so the
  render layer has a durable file boundary without automatic restore, slash
  command wiring, or task execution changes.
- Added regression coverage:
  - `render_session_snapshot_saves_and_loads_json_file`
  - `app_saves_and_loads_current_render_session_snapshot_file`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 271 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 48 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 Layer 1 generalized relation index slice:

- Added `TranscriptRelationIndex` to Layer 1. It derives roots,
  `parents_by_child`, `children_by_parent`, and missing parent ids from
  arbitrary `TranscriptRecord.parent_id` values, plus helper accessors for
  record lookup and child records.
- Subagent/task records created at the TUI boundary now use a stable
  `task:{id}` root record. Subagent assistant/tool/result/completion records
  attach to that task root or to the matching tool-use parent when available.
- `/raw` now surfaces relation counts and explicit parent/child debug rows, so
  the debug path can inspect Layer 1 hierarchy separately from normal semantic
  transcript rendering.
- Added regression coverage:
  - `relation_index_groups_arbitrary_parent_child_records`
  - `app_layers_subagent_records_under_task_parent`
  - `render_snapshot_raw_debug_view_includes_layer1_engine_events`
  - `app_render_contract_raw_modal_includes_raw_engine_event_journal`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 269 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 48 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 Layer 1 render session snapshot serialization slice:

- Added `RenderSessionSnapshot` and `RENDER_SESSION_SNAPSHOT_VERSION` to Layer
  1. The snapshot serializes current transcript records, sidecar approval/final
  summary facts, raw engine events, session id, current/latest render turn ids,
  and next record/turn/raw-event sequence counters.
- Added serde coverage for Layer 1 record/event enums and record containers so
  the snapshot can roundtrip through JSON without parsing terminal text or
  reaching into task execution state.
- `App` now builds the snapshot from current Layer 1 state, and `/raw` shows
  snapshot version, session id, record count, raw-event count, latest turn id,
  and serialized JSON byte size as explicit debug metadata.
- Added regression coverage:
  - `render_session_snapshot_roundtrips_layer1_records_and_events`
  - `app_render_session_snapshot_roundtrips_current_layer1_state`
  - `render_snapshot_raw_debug_view_includes_layer1_engine_events`
  - `app_render_contract_raw_modal_includes_raw_engine_event_journal`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 267 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 48 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 Layer 1 raw engine event journal slice:

- Added `RawEngineEventRecord` and `RawEngineEventKind` to Layer 1. Each
  record captures the source `SdkMessage` sequence, main/task scope, event
  kind, summary, optional render turn id, and capped serialized payload
  preview.
- `App::handle_engine_message` now records raw engine events at the TUI
  boundary before semantic rendering. Main events inherit the active
  `turn-000N` id; task/subagent events keep task scope without being promoted
  to the main turn id.
- `/raw` now includes an explicit `engine events` section and `raw_events=N`
  count, while the normal transcript still keeps raw payload keys out of the
  user-facing semantic view.
- Added regression coverage:
  - `raw_engine_event_record_preserves_sdk_message_identity`
  - `main_engine_messages_are_ingested_as_raw_layer1_events`
  - `render_snapshot_raw_debug_view_includes_layer1_engine_events`
  - `app_render_contract_raw_modal_includes_raw_engine_event_journal`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 265 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 33 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 48 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 Layer 1 stable render turn id slice:

- Added `turn_id` to `RenderEvent` and `TranscriptRecord`, plus the
  App-side record override map needed to attach one session-local
  `turn-000N` id across the main user turn.
- Main user/assistant/tool/result/final-summary records and main structured
  render events now share the same turn id. The id is cleared after
  finalization, and the latest completed id remains visible as the last turn
  label.
- `/raw`, `/timeline`, `/status`, and `/ps` now surface the turn id as a
  semantic render fact. Manual/debug-only records still show `turn=-` instead
  of inventing identity.
- Added regression coverage:
  - `applies_turn_id_overrides_at_layer1_boundary`
  - `render_timeline_collects_structured_event_rows`
  - `main_engine_turn_records_and_events_share_stable_turn_id`
  - `timeline_renders_counts_and_selected_detail`
  - `render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript`
  - `render_snapshot_timeline_modal_is_structured_event_history`
  - `app_render_contract_timeline_modal_uses_structured_render_events`
- Verification:
  - `cargo test -q -p mossen-tui --lib` -> 263 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 32 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 47 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `cargo fmt -p mossen-tui --check` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/render_timeline.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'if(/[ \t]$/){print "$ARGV:$.: trailing whitespace\n"; $bad=1} END{exit($bad ? 1 : 0)}' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_events.rs crates/mossen-tui/src/render_lifecycle.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/render_timeline.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/timeline` render-event lifecycle slice:

- Added `RenderTimelineRenderModel`, summary counts, and event rows derived
  from structured `RenderEvent` values already driving the TUI footer/activity
  state.
- Added `RenderTimelineWidget` with event totals, refresh/history/stage/scope
  facts, selected-row details, display-width clipping, multibyte-safe rows, and
  ASCII fallback.
- Added session-local `App::render_event_history` with a capped history of the
  main render events applied to the TUI. This turns the existing event stream
  into a reviewable lifecycle timeline without changing agent/tool execution.
- Added `/timeline`, `/events`, and `/render-events` as TUI-only lifecycle
  inspection commands. The modal renders semantic event facts and does not
  expose raw command JSON keys.
- Added regression coverage:
  - `render_timeline_collects_structured_event_rows`
  - `timeline_renders_counts_and_selected_detail`
  - `timeline_clips_multibyte_rows_with_ascii_separator`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_timeline_modal_is_structured_event_history`
  - `app_render_contract_timeline_modal_uses_structured_render_events`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -q -p mossen-tui --lib` -> 261 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 32 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 47 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/render_timeline.rs crates/mossen-tui/src/widgets/file_changes.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'print "$ARGV:$.: trailing whitespace\n" if /[ \t]+$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/render_timeline.rs crates/mossen-tui/src/widgets/file_changes.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/files` file-change summary slice:

- Added `FileChangeListRenderModel`, summary counts, and row models derived
  from existing semantic `FileChangeSummaryModel` facts.
- Added `FileChangesWidget` with changed-file totals, modified/added/deleted
  counts, selected-row details, display-width clipping, multibyte-safe rows,
  and ASCII fallback.
- Added `/files`, `/changes`, and `/changed-files` as TUI-only file-change
  summary commands. The modal consumes existing semantic file-change facts
  from the transcript/final-summary path and does not query git or mutate task
  execution code.
- Added regression coverage:
  - `file_changes_renders_counts_and_selected_detail`
  - `file_changes_clips_multibyte_paths_with_ascii_separator`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_file_changes_modal_is_semantic`
  - `app_render_contract_files_modal_uses_semantic_file_change_summary`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -q -p mossen-tui --lib` -> 258 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 31 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 46 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/app_services.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/file_changes.rs crates/mossen-tui/src/widgets/session_title.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'print "$ARGV:$.: trailing whitespace\n" if /[ \t]+$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/app_services.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/file_changes.rs crates/mossen-tui/src/widgets/session_title.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/title` terminal title slice:

- Added `SessionTitleRenderModel` and `SessionTitleWidget` for terminal title
  inspection/configuration with display-width clipping, multibyte-safe rows,
  ASCII fallback, and explicit save/reset/close affordances.
- Added `/title` and `/session-title` as TUI-only slash commands. They can open
  the modal, set a sanitized manual title from arguments, or reset to the
  default title base without touching task execution code.
- Added `TerminalServices.manual_title` plus title sanitization. ESC/BEL are
  removed, other control characters become whitespace, and manual title bases
  persist across streaming/ready terminal chrome before reset.
- Added regression coverage:
  - `manual_title_is_sanitized_and_persists_across_streaming_edges`
  - `session_title_renders_current_custom_and_draft`
  - `session_title_clips_long_multibyte_title_with_ascii_separator`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_session_title_modal_is_semantic_and_sanitized`
  - `app_render_contract_title_modal_sets_sanitized_terminal_title`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -q -p mossen-tui --lib` -> 256 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 30 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 45 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/app_services.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/session_title.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'print "$ARGV:$.: trailing whitespace\n" if /[ \t]+$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/app_services.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/session_title.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/debug-config` redacted configuration slice:

- Added `DebugConfigRenderModel` for redacted session, engine, policy,
  renderer, footer, and runtime inspection facts.
- Added `DebugConfigWidget` with sectioned rows, display-width-safe clipping,
  ASCII separator fallback, footer affordances, and read-only scrolling.
- Added `/debug-config` and `/debugconfig` as TUI-only inspection commands.
  The App builds the modal from existing App/footer/render state and exposes
  only configured/missing API-key state plus extra-body key names.
- Fixed the debug-config modal scroll state so small-height dialogs can reach
  tail sections; `End` jumps to the last section anchor instead of being
  capped by an unrelated fixed viewport size.
- Added regression coverage:
  - `debug_config_renders_redacted_config_rows`
  - `debug_config_scrolls_and_clips_multibyte_rows_with_ascii_separator`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_debug_config_modal_is_redacted_and_semantic`
  - `app_render_contract_debug_config_modal_is_redacted_and_semantic`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -q -p mossen-tui --lib` -> 253 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 29 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 44 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/debug_config.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - `perl -ne 'print "$ARGV:$.: trailing whitespace\n" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/debug_config.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/approvals` approval history slice:

- Added `ApprovalHistoryRenderModel`, summary counts, and approval rows
  extracted from semantic `RenderNode::ApprovalDecision` transcript blocks,
  plus a pending-row adapter from existing `ApprovalRenderModel`.
- Added `ApprovalHistoryWidget` with pending/allowed/denied/cancelled counts,
  risk/action/body/anchor/source details, read-only expansion, detail
  scrolling, display-width clipping, and ASCII fallback.
- Added `/approvals`, `/approval-history`, and `/approval-log` as TUI-only
  inspection commands while leaving the existing `mossen-commands`
  `/permissions` directive untouched.
- Added regression coverage:
  - `approval_history_collects_decision_rows`
  - `approval_history_renders_pending_detail`
  - `approval_history_expands_semantic_details`
  - `approval_history_scrolls_expanded_details`
  - `approval_history_clips_multibyte_with_ascii_footer`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_approval_history_modal_is_semantic`
  - `app_render_contract_approvals_modal_uses_semantic_decision_history`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib` -> 251 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 28 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 43 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/results` final-summary history slice:

- Added `FinalSummaryHistoryRenderModel`, summary counts, and final-summary
  rows extracted from semantic `RenderNode::FinalSummary` transcript blocks.
- Added `SummaryHistoryWidget` with final-summary counts, status rows,
  basename row previews, selected-summary details, read-only expansion,
  detail scrolling, display-width clipping, and ASCII fallback.
- Added `/results`, `/summaries`, and `/final-summary` as TUI-only inspection
  commands while leaving the existing `mossen-commands` `/summary` directive
  untouched.
- Added regression coverage:
  - `final_summary_history_collects_structured_final_summaries`
  - `summary_history_renders_selected_summary_detail`
  - `summary_history_expands_semantic_final_summary_details`
  - `summary_history_clips_multibyte_rows_without_panic`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_final_summary_history_modal_is_semantic`
  - `app_render_contract_results_modal_uses_semantic_final_summary_history`
- Verification:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib` -> 246 passed
  - `cargo test -q -p mossen-tui --test render_snapshot` -> 42 passed
  - `cargo test -q -p mossen-tui --test render_contract` -> 27 passed
  - `cargo test -q -p mossen-tui --test keybinding_smoke` -> 18 passed
  - `cargo check -q -p mossen-tui` passed
  - Cargo still emits pre-existing workspace warning noise outside this slice.

2026-05-22 semantic `/errors` error history slice:

- Added `ErrorHistoryRenderModel`, summary rows, error rows, and failed-command
  extraction as viewport-independent TUI render data.
- Added `ErrorHistoryWidget` with error summaries, selected-error details,
  command-failure rows, retry visibility, detail expansion, detail scrolling,
  display-width clipping, and ASCII fallback.
- Added `/errors`, `/errs`, and `/failures` as intentional TUI-only inspection
  commands. The App builds the modal from existing semantic transcript blocks
  plus current render activity, then handles Space/Enter detail expansion and
  PageUp/PageDown detail scrolling without reading from or mutating task
  execution code.
- Added regression coverage:
  - `error_history_collects_error_blocks_and_failed_commands`
  - `error_history_renders_selected_error_detail`
  - `error_history_expands_semantic_details`
  - `error_history_scrolls_expanded_details`
  - `error_history_clips_multibyte_with_ascii_footer`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_error_history_modal_is_semantic`
  - `app_render_contract_errors_modal_uses_semantic_error_history`
- Verified targeted commands:
  - `cargo test -q -p mossen-tui --lib error_history -- --nocapture`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_snapshot render_snapshot_error_history_modal_is_semantic -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_errors_modal_uses_semantic_error_history -- --nocapture`

2026-05-22 semantic `/commands` command history and full-log expansion slice:

- Added `CommandHistoryRenderModel`, summary rows, command row models, and
  command run status helpers as viewport-independent TUI render data.
- Added sanitized `CommandStreamRenderModel.full_text` storage for embedded
  stdout/stderr command logs already present in semantic tool-result content.
- Added `CommandHistoryWidget` with command summaries, selected-command
  details, stdout/stderr previews, full-log availability, embedded full-log
  expansion, detail scrolling, display-width clipping, and ASCII fallback.
- Added `/commands`, `/cmds`, and `/logs` as intentional TUI-only inspection
  commands. The App builds the modal from existing semantic transcript tool
  cards and current render activity, then handles Space/Enter expansion and
  PageUp/PageDown detail scrolling without reading from or mutating task
  execution code.
- Added regression coverage:
  - `command_history_collects_semantic_command_runs`
  - `command_history_renders_selected_command_detail`
  - `command_history_expands_embedded_full_log`
  - `command_history_scrolls_expanded_full_log`
  - `command_history_clips_multibyte_with_ascii_footer`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_command_history_modal_is_semantic`
  - `app_render_contract_commands_modal_uses_semantic_command_history`
- Verified targeted commands:
  - `cargo test -q -p mossen-tui --lib command_history -- --nocapture`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_snapshot render_snapshot_command_history_modal_is_semantic -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_commands_modal_uses_semantic_command_history -- --nocapture`
- Verified full TUI gates:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib`
  - `cargo test -q -p mossen-tui --test render_snapshot`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo check -q -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/command_history.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`
  - `perl -ne 'print "$ARGV:$.:$_" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/command_history.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 semantic `/status` session overview slice:

- Added `StatusOverviewRenderModel`, section/row models, and row severity
  levels as viewport-independent TUI render data.
- Added `StatusOverviewWidget` with sectioned session/turn/policy/workspace
  rendering, display-width clipping, ASCII fallback, and explicit `/status`
  footer affordance.
- Replaced the old `/status` ad-hoc `Paragraph` with an intentional semantic
  App modal built from existing footer, process summary, App config/state,
  context, cost, MCP, TodoWrite, and teammate facts.
- Kept API keys secret by rendering only configured/missing state.
- Added regression coverage:
  - `status_overview_renders_sections_and_rows`
  - `status_overview_clips_multibyte_and_uses_ascii_separator`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_status_overview_modal_is_semantic`
  - `app_render_contract_status_modal_uses_semantic_session_state`
- Verified targeted commands:
  - `cargo test -q -p mossen-tui --lib status_overview -- --nocapture`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_snapshot render_snapshot_status_overview_modal_is_semantic -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_status_modal_uses_semantic_session_state -- --nocapture`
- Verified full TUI gates after the slice:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib`
  - `cargo test -q -p mossen-tui --test render_snapshot`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo check -q -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/status_overview.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`
  - `perl -ne 'print "$ARGV:$.:$_" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/status_overview.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 semantic `/ps` process inspector slice:

- Added `ProcessListRenderModel`, summary/row/fact types, row kinds, and
  process statuses as viewport-independent TUI render data.
- Added `ProcessListWidget` with summary, selectable rows, selected-row facts,
  display-width clipping, ASCII fallback, and status styling.
- Added `/ps` and `/processes` as intentional TUI-only inspection commands.
  The App builds the modal from existing semantic state for turn, blocking,
  render activity, compact progress, TodoWrite tasks, task-store snapshots,
  foreground/background counts, and teammate agents.
- Wired modal key handling for close, up/down, page up/down, home, and end
  without touching task execution code.
- Added regression coverage:
  - `process_list_renders_summary_and_selected_detail`
  - `process_list_uses_ascii_separator_and_clips_detail`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_process_status_modal_summarizes_active_state`
  - `app_render_contract_ps_modal_uses_semantic_process_state`
- Verified targeted commands:
  - `cargo test -q -p mossen-tui --lib process_list -- --nocapture`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_snapshot render_snapshot_process_status_modal_summarizes_active_state -- --nocapture`
  - `cargo test -q -p mossen-tui --test render_contract app_render_contract_ps_modal_uses_semantic_process_state -- --nocapture`
- Verified full TUI gates after the slice:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib`
  - `cargo test -q -p mossen-tui --test render_snapshot`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo check -q -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/process_list.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`
  - `perl -ne 'print "$ARGV:$.:$_" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/widgets/process_list.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 semantic `/diff` review slice:

- Added a unified-diff parser for the TUI diff widgets that preserves old/new
  file paths, hunk headers, line numbers, binary/new/deleted flags, and
  additions/deletions.
- Expanded `DiffDialogWidget` into a grouped review surface with file
  selection, detail scrolling, and per-file collapse state.
- Added `/diff` as an intentional TUI-only inspection command. It builds its
  state from semantic transcript tool sections and falls back to a visible
  command-output notice when no semantic diff is available.
- Wired modal key handling for close, previous/next file, scroll, top/bottom,
  and fold/unfold without touching task execution code.
- Added regression coverage:
  - `parse_unified_diff_groups_files_and_counts_lines`
  - `diff_dialog_renders_file_list_and_selected_detail`
  - `diff_dialog_can_collapse_selected_file`
  - `builtin_slash_commands_open_expected_ui`
  - `render_snapshot_diff_review_modal_groups_files_and_folds`
  - `app_render_contract_diff_modal_uses_semantic_diff_viewer`
- Verified targeted commands:
  - `cargo test -p mossen-tui --lib diff -- --nocapture`
  - `cargo test -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_diff_review_modal_groups_files_and_folds -- --nocapture`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_diff_modal_uses_semantic_diff_viewer -- --nocapture`
- Verified full TUI gates after the slice:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib`
  - `cargo test -q -p mossen-tui --test render_snapshot`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo check -q -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/diff.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`
  - `perl -ne 'print "$ARGV:$.:$_" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/app.rs crates/mossen-tui/src/widgets/diff.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 active activity panel slice:

- Added `ActivityPanelRenderModel` to `RenderSurface` as the semantic live
  panel for current turn activity, separate from transcript history and
  derived from `RenderActivityState` or blocking state.
- Added `ActivityPanelWidget` with severity styling, compact narrow-width
  fallback, ASCII border support, and display-width clipping for long command
  and output facts.
- Wired fullscreen and inline App layouts to reserve the active panel above
  transcript history when the terminal has room, so streaming activity can
  refresh in chrome instead of becoming another transcript log block.
- Kept approval and final summary out of the active activity panel. Approval
  stays in its dedicated approval surface, while final summary remains
  transcript history, preserving narrow resize and completed-turn visibility.
- Added regression coverage:
  - `activity_panel_renders_command_activity_details`
  - `activity_panel_clips_compact_width`
  - `activity_panel_uses_ascii_border_profile`
  - `render_surface_carries_active_panel_from_render_activity`
  - `render_snapshot_app_frame_shows_active_activity_panel`
  - `app_render_contract_keeps_active_panel_above_transcript_history`
- Verified targeted commands:
  - `cargo test -p mossen-tui --lib activity_panel`
  - `cargo test -p mossen-tui --lib render_surface_carries_active_panel_from_render_activity`
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_app_frame_shows_active_activity_panel`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_keeps_active_panel_above_transcript_history`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_keeps_`
- Verified full TUI gates after the slice:
  - `cargo fmt -p mossen-tui`
  - `cargo test -p mossen-tui --lib`
  - `cargo test -p mossen-tui --test render_snapshot`
  - `cargo test -p mossen-tui --test render_contract`
  - `cargo test -p mossen-tui --test keybinding_smoke`
  - `cargo check -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/activity_panel.rs crates/mossen-tui/src/widgets/status_header.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`
  - `perl -ne 'print "$ARGV:$.:$_" if /[ \t]$/; close ARGV if eof' crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/activity_panel.rs crates/mossen-tui/src/widgets/status_header.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 top status header slice:

- Added `TopStatusRenderModel` to `RenderSurface`, derived from the same
  footer facts so top status, footer, and blocking state stay consistent
  without rereading task state.
- Added `StatusHeaderWidget` as a one-row top status renderer with
  blocking-first labels, model/mode/reasoning/context facts,
  display-width clipping, and ASCII separator fallback.
- Wired fullscreen and inline App layouts to reserve and render the top status
  row when the terminal is large enough, leaving task execution untouched.
- Added regression coverage:
  - `status_header_renders_core_session_facts`
  - `status_header_prioritizes_blocking_and_clips_narrow_width`
  - `status_header_uses_ascii_separator_when_requested`
  - `render_surface_carries_top_status_from_footer_facts`
  - `render_snapshot_app_frame_shows_top_status_header`
  - `app_render_contract_keeps_top_status_visible_above_transcript`
- Verified targeted commands:
  - `cargo test -p mossen-tui --lib status_header`
  - `cargo test -p mossen-tui --lib render_surface_carries_top_status_from_footer_facts`
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_app_frame_shows_top_status_header`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_keeps_top_status_visible_above_transcript`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_statusline_config_keeps_core_status_visible`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_keeps_approval_inline_and_footer_alive`
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_app_frame_shows_inline_approval_and_footer_state`
- Verified full TUI gates after the slice:
  - `cargo fmt -p mossen-tui`
  - `cargo test -p mossen-tui --lib`
  - `cargo test -p mossen-tui --test render_snapshot`
  - `cargo test -p mossen-tui --test render_contract`
  - `cargo test -p mossen-tui --test keybinding_smoke`
  - `cargo check -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/widgets/status_header.rs crates/mossen-tui/src/widgets/mod.rs crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 session-local status-line configuration slice:

- Added `FooterItem`, `FooterPreset`, and `FooterRenderConfig` as Layer 2
  footer semantics, plus `AppState.footer_config` as session-local TUI state.
- `FooterWidget` now honors configured left/right footer items while keeping
  core blocking/turn status visible regardless of item toggles.
- Added `/statusline` and `/status-line` UI-only slash handling. `/statusline`
  opens an item-toggle modal; `/statusline minimal|default|full` applies a
  session preset without touching task execution.
- Added regression coverage:
  - `footer_render_model_carries_session_statusline_config`
  - `footer_widget_honors_item_config_but_keeps_core_status`
  - `render_snapshot_statusline_config_is_explicit_session_ui`
  - `app_render_contract_statusline_config_keeps_core_status_visible`
  - `builtin_slash_commands_open_expected_ui`
- Verified targeted commands:
  - `cargo test -p mossen-tui --lib footer`
  - `cargo test -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui`
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_statusline_config_is_explicit_session_ui`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_statusline_config_keeps_core_status_visible`
- Verified full TUI gates after the slice:
  - `cargo test -p mossen-tui --lib`
  - `cargo test -p mossen-tui --test render_snapshot`
  - `cargo test -p mossen-tui --test render_contract`
  - `cargo test -p mossen-tui --test keybinding_smoke`
  - `cargo check -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/render_model.rs crates/mossen-tui/src/state.rs crates/mossen-tui/src/widgets/footer.rs crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 explicit raw debug view slice:

- Added a TUI-only `/raw` modal that builds an intentional raw transcript
  debug view from current messages, Layer 1 records, approval/final-summary
  sidecars, and visible semantic block ids.
- Normal transcript rendering remains semantic: the product contract first
  verifies the same raw tool payload does not expose JSON keys or `raw_json`
  in the App frame, then opens `/raw` and verifies the raw payload is visible
  only inside the explicit debug modal.
- Added regression coverage:
  - `render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript`
  - `app_render_contract_raw_payloads_require_explicit_raw_view`
  - `builtin_slash_commands_open_expected_ui`
- Verified targeted commands:
  - `cargo test -p mossen-tui --test render_snapshot render_snapshot_raw_debug_view_is_explicitly_separate_from_normal_transcript -- --nocapture`
  - `cargo test -p mossen-tui --test render_contract app_render_contract_raw_payloads_require_explicit_raw_view -- --nocapture`
  - `cargo test -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui -- --nocapture`
- Verified full TUI gates after the slice:
  - `cargo test -p mossen-tui --lib`
  - `cargo test -p mossen-tui --test render_snapshot`
  - `cargo test -p mossen-tui --test render_contract`
  - `cargo test -p mossen-tui --test keybinding_smoke`
  - `cargo check -p mossen-tui`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/render_snapshot.rs crates/mossen-tui/tests/render_contract.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`

2026-05-22 MCP approval anchor slice:

- MCP channel approvals now scan the semantic transcript for the latest MCP
  tool whose server name matches the approval request.
- `ApprovalRenderModel.anchor_block_id` and the persisted MCP allow/deny
  decision both reuse that tool id when available.
- Added regression coverage:
  - `app::engine_stream_tests::mcp_channel_approval_anchors_to_matching_mcp_tool`
  - `render_snapshot_mcp_channel_approval_decision_stays_near_tool_context`
- Verified:
  - `cargo test -p mossen-tui --lib`
  - `cargo test -p mossen-tui --test render_snapshot`
  - `cargo test -p mossen-tui --test render_contract`
  - `cargo test -p mossen-tui --test keybinding_smoke`
  - `cargo check -p mossen-tui`

2026-05-22 Batch F active product render contract slice:

- Added `crates/mossen-tui/tests/render_contract.rs` as the non-snapshot
  product gate for active TUI rendering.
- The contract renders the true `App::render_for_test` path over multiple
  viewport sizes and product states: streaming transcript, inline approval,
  help/MCP/tasks dialogs, footer, command suggestions, task/subagent panels,
  and realistic tool cards.
- Added a resize-storm/pathological-content fixture that reuses the same `App`
  while switching 132/44/100/24/160/32/72/40-column frames and includes ANSI,
  tabs, CJK, combining characters, long unbroken code, malformed Read input,
  and failing Bash output.
- Added a long-transcript scroll contract for the earlier "content beyond the
  current screen cannot be reached" failure class: the active App frame must
  show the tail in sticky-bottom mode, the head after manual scroll-up, and the
  tail again after `scroll_to_bottom()`.
- Added a streaming/resize interleave contract that feeds `SdkMessage`
  `StreamEvent` deltas through `App::handle_engine_message`, renders between
  chunks at changing sizes, and verifies visible assistant anchors after
  content-bearing deltas.
- Added a deterministic semantic fuzz corpus for active App frames, covering
  ANSI, tabs, CJK, combining marks, long unbroken text, malformed known-tool
  payloads, and mixed Bash/Glob/Read-style tool cards.
- Added a rich-content active App frame contract for Markdown headings/lists,
  fenced Rust code, Markdown tables, and Bash stdout diff rendering. It fails
  if raw Markdown fence/table delimiter syntax leaks into the normal
  transcript.
- Added a 900-message large-session smoke budget over several viewport sizes
  to catch obvious re-layout stalls on the active `App::render_for_test` path.
- Added a single-tall-message regression for the failure class where one
  model reply is taller than ratatui's safe scratch buffer. The test failed
  before the fix by rendering around `tall-single-row-0408` instead of
  `tall-single-message-tail-anchor`.
- Fixed Layer 3 oversized text-block virtualization in
  `widgets/messages.rs`: when the visible scroll offset is deeper than the
  safe scratch buffer, the renderer now builds semantic virtual text rows for
  that block and paints the requested visible slice directly.
- Added `app_render_contract_tall_message_scroll_crosses_scratch_boundary`.
  Before the fix, a manual row offset near the scratch-buffer boundary rendered
  `tall-single-row-0405` through `tall-single-row-0408` and then blank rows.
  The renderer now switches to the same semantic virtual-row slice whenever
  `skip + visible_height` crosses the scratch-buffer height, not only when
  `skip >= scratch_height`.
- Added `app_render_contract_reaches_tail_beyond_u16_height_cap` for a single
  assistant block with 66,000 semantic rows. Before the fix, sticky-bottom
  rendered around `enormous-single-row-32766` and missed
  `enormous-single-message-tail-anchor`.
- Fixed that failure by changing `RenderBlockWidget::required_height` and
  `RenderHeightCache` from capped `u16` heights to `usize` virtual heights.
  Layer 3 still caps only the temporary ratatui scratch buffer used for
  painting, not the transcript scroll model.
- Added a streaming tall-message scroll policy contract. It feeds 900+ text
  deltas through `App::handle_engine_message`, verifies sticky-bottom reaches
  the streaming tail, verifies manual scroll-up is preserved while new chunks
  arrive, and verifies `Ctrl+L` re-anchors to the latest bottom.
- Added `app_render_contract_async_append_preserves_manual_row_scroll`.
  Before the fix, `SdkMessage::ApiRetry` appended a system message while the
  user was reading a long assistant block and clamped `VirtualScroll.offset`
  from 2390 to 0 because App event handlers wrote `messages.len()` into the
  row-based scroll model.
- Fixed that failure by replacing transcript-mutation
  `self.scroll.set_total_items(self.messages.len())` calls with
  `note_transcript_changed()`. `VirtualScroll.total_items` is now updated by
  `sync_message_scroll()` from measured semantic content height, not by
  message count side effects.
- Added `widgets::messages::tests::virtual_text_rows_match_measured_text_block_height`.
  The first run failed for a focused, 12-column rich Markdown block because
  virtual rows undercounted the measured height by two rows. The virtual
  text-block fallback now pads each generated segment to
  `wrapped_line_count_for_text`, keeping scroll totals and fallback slices on
  the same height contract.
- Added `app_render_contract_tall_rich_markdown_virtual_scroll_keeps_shapes`
  so the same fix is covered through `App::render_for_test`, including rich
  Markdown/code/table shapes and manual middle scroll.
- The active contract first exposed that ANSI cleanup only existed in the
  widget/tool-output path. Layer 2 now sanitizes all visible semantic text in
  `render_model.rs`, before Markdown/tool layout sees it.
- Added regression coverage:
  - `app_render_contract_survives_product_state_matrix`
  - `app_render_contract_keeps_approval_inline_and_footer_alive`
  - `app_render_contract_keeps_long_transcript_scroll_usable`
  - `app_render_contract_survives_resize_storm_and_pathological_content`
  - `app_render_contract_survives_streaming_resize_interleave`
  - `app_render_contract_survives_deterministic_semantic_fuzz_corpus`
  - `app_render_contract_renders_rich_markdown_code_table_and_diff`
  - `app_render_contract_large_session_stays_within_budget`
  - `app_render_contract_reaches_tail_of_single_tall_message`
  - `app_render_contract_tall_message_scroll_crosses_scratch_boundary`
  - `app_render_contract_reaches_tail_beyond_u16_height_cap`
  - `app_render_contract_tall_rich_markdown_virtual_scroll_keeps_shapes`
  - `app_render_contract_streaming_tall_message_keeps_scroll_policy`
  - `app_render_contract_async_append_preserves_manual_row_scroll`
  - `widgets::messages::tests::virtual_text_rows_match_measured_text_block_height`
  - `semantic_render_contract_strips_protocol_before_layout`
  - `layout_height_contract_is_deterministic_cached_and_bounded`
  - `strips_ansi_from_all_visible_semantic_text`
- Verified:
  - `cargo test -p mossen-tui --test render_contract -- --nocapture`
  - `cargo test -p mossen-tui --lib strips_ansi -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`

2026-05-21:

- Added `RenderSurface`, `BlockingRenderModel`, and footer blocking state in
  `crates/mossen-tui/src/render_model.rs`.
- Added `ApprovalBlockWidget`, a renderer that consumes
  `ApprovalRenderModel`.
- App inline approval now adapts Bash/file/MCP approval requests into
  `ApprovalRenderModel`.
- App footer now builds `FooterRenderModel` before adapting to
  `StatusBarWidget`.
- Added regression coverage:
  - `render_surface_promotes_approval_to_blocking_footer_state`
  - `footer_render_model_uses_same_blocking_state_as_approval_surface`
  - `renders_semantic_approval_without_permission_state`
  - `render_snapshot_semantic_approval_model_matches_inline_surface`
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -p mossen-tui render_model -- --nocapture`
  - `cargo test -p mossen-tui widgets::approval -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui footer_render_model -- --nocapture`

2026-05-21 Batch A closeout:

- `App::render_frame` now builds one `RenderSurface` and passes it through
  `render_fullscreen` / `render_inline`.
- `MessagesWidget` can consume a caller-provided `RenderTranscript`, so frame
  rendering and scroll-height accounting no longer rebuild divergent transcript
  models.
- Inline approval rendering now consumes `surface.approvals` instead of asking
  `App` for active modal state again.
- Footer rendering now consumes `surface.footer`.
- Streaming spinner labels now derive their blocking wording from
  `RenderSurface` instead of directly inferring it from modal state.
- Tool approval anchors now resolve against `RenderTranscript` block ids when
  compatibility message data is the only lifecycle source available.
- Added regression coverage:
  - `render_surface_unifies_transcript_approval_and_footer`
  - `render_surface_models_file_permission_as_approval_blocking`
  - `render_surface_models_mcp_channel_approval_as_approval_blocking`
  - `render_surface_keeps_cost_blocking_without_approval`
  - `render_surface_models_idle_return_as_blocking_without_approval`
  - `render_surface_models_error_command_output_as_blocking`
  - `spinner_status_text_uses_surface_blocking_state`
  - `render_snapshot_app_frame_shows_inline_approval_and_footer_state`
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -p mossen-tui render_surface -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -q -p mossen-tui spinner_status_text_uses_surface_blocking_state -- --nocapture`
  - `cargo build -p mossen-cli --bin mossen`
  - `git diff --check`

2026-05-21 Batch B first slice:

- Added `ApprovalDecisionModel` and `ApprovalDecisionKind` to the semantic
  render model.
- Added `RenderNode::ApprovalDecision` and
  `RenderBlockKind::ApprovalDecision`.
- Approval submit/cancel paths now append semantic decision records for:
  - Bash/file permission requests;
  - tool-use confirmation allow/always/deny/cancel;
  - MCP channel allow/deny.
- The compatibility storage format uses an internal
  `mossen-render:approval-decision:` marker, and `RenderTranscript` converts
  it into a semantic decision block before widgets see it. Normal transcript
  snapshots assert the marker is not visible.
- Added regression coverage:
  - `approval_decision_marker_becomes_semantic_block`
  - `accepted_tool_permission_persists_as_semantic_decision_block`
  - `render_snapshot_approval_decision_stays_in_transcript_without_raw_marker`
  - `render_snapshot_approval_decision_survives_narrow_and_wide_profiles`
- Verified:
  - `cargo test -p mossen-tui approval_decision -- --nocapture`
  - `cargo test -p mossen-tui accepted_tool_permission_persists_as_semantic_decision_block -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_approval_decision_survives_narrow_and_wide_profiles -- --nocapture`

2026-05-21 Batch C first slice:

- Added `crates/mossen-tui/src/render_profile.rs` with explicit
  `RendererProfile::{Small, Medium, Large}` selection from terminal width.
- `MessagesWidget` now derives the profile from the viewport width and passes
  it into `RenderBlockWidget`.
- `RenderBlockWidget` now uses the profile for tool preview line limits,
  expanded line limits, and per-line clipping budgets.
- Added regression coverage:
  - `renderer_profile_is_selected_from_terminal_width`
  - `renderer_profile_controls_tool_preview_budget`
  - `render_snapshot_renderer_profile_matrix_keeps_core_semantics`
- Verified:
  - `cargo test -p mossen-tui renderer_profile -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_renderer_profile_matrix_keeps_core_semantics -- --nocapture`

2026-05-21 Batch E panic-boundary first slice:

- Added a top-level render panic boundary around the app frame render path.
- Production `terminal.draw` and `render_for_test` now call
  `render_frame_safely`, which catches render panics and replaces the frame
  with a visible `Render error` panel instead of unwinding out of the process.
- The panic boundary suppresses and then restores the default panic hook while
  drawing, so a caught render panic does not print `thread ... panicked` into
  the active terminal UI.
- Added regression coverage:
  - `render_panic_boundary_shows_error_frame_instead_of_unwinding`
- Verified:
  - `cargo test -p mossen-tui render_panic_boundary -- --nocapture`

2026-05-21 Batch E height-cache first slice:

- Added `crates/mossen-tui/src/render_cache.rs` with `RenderHeightCache`.
- Cache keys include block id, content signature, terminal width,
  `RendererProfile`, theme name, margin/thinking/focus/collapsed flags.
- `App` owns one cache and passes it into both scroll-height accounting and
  message rendering, so the frame path no longer has to re-measure every block
  from scratch on every draw.
- Added regression coverage:
  - `height_cache_hits_for_same_block_profile_width_and_flags`
  - `height_cache_invalidates_on_width_content_expand_and_theme`
  - `large_transcript_height_cache_reuses_layouts`
- Evidence from the large fixture:
  - 1100 render blocks;
  - 6749 rendered rows;
  - first pass: 1100 misses;
  - second pass: 1100 hits;
  - local elapsed time: 26ms.
- Verified:
  - `cargo test -p mossen-tui render_cache -- --nocapture`
  - `cargo test -p mossen-tui large_transcript_height_cache_reuses_layouts -- --nocapture`

2026-05-21 Batch D malformed-payload first slice:

- Known semantic tools with JSON-looking but malformed payloads no longer leak
  raw JSON keys into the normal transcript.
- Layer 2 now renders a semantic `malformed input/output` section and states
  that the raw payload is hidden from normal transcript rendering.
- Added regression coverage:
  - `malformed_known_tool_json_is_hidden_from_normal_transcript`
  - `render_snapshot_malformed_payloads_and_multibyte_resize_are_safe`
- The snapshot fixture covers CJK, emoji, markdown code blocks, malformed Bash
  output, malformed Task input, and 36/80/132 column widths.
- Verified:
  - `cargo test -p mossen-tui malformed_known_tool_json_is_hidden_from_normal_transcript -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_malformed_payloads_and_multibyte_resize_are_safe -- --nocapture`

2026-05-21 Batch C/D legacy-widget and UTF-8 hardening slice:

- Replaced `crates/mossen-tui/src/widgets/message.rs` with a data-only module
  containing `MessageData`, `MessageType`, and `display_tool_name`.
- Confirmed `widgets/render_block.rs` is the active semantic
  `RenderBlockWidget` path. The retired code was the old translated
  message-row renderer, not the current semantic block renderer.
- Hardened root helper rendering paths that could slice multibyte strings:
  - root search snippet extraction;
  - root normalization/truncation;
  - OAuth token display;
  - feedback secret redaction;
  - global-search file suffix display;
  - markdown-table alignment;
  - medium root API-key masking.
- Added regression coverage:
  - `formats_mcp_tool_names_for_display`
  - `root_text_helpers_are_utf8_boundary_safe`
- Verified:
  - `cargo check -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui root_text_helpers_are_utf8_boundary_safe -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_malformed_payloads_and_multibyte_resize_are_safe -- --nocapture`

2026-05-21 Batch D/E sticky-scroll and active UTF-8 boundary slice:

- Added a failing-then-passing app-frame regression for long Chinese Markdown
  transcripts in sticky-bottom mode. The before state showed the transcript
  stopping around `分析行 86` instead of the final conclusion; the fixed path
  now keeps the final tail visible.
- Replaced Layer 3 rendered-height estimation for Markdown/plain tool text
  with a grapheme-aware word-wrap measurement that mirrors ratatui's
  trim-false wrapping behavior more closely than total-width division.
- Routed `RenderBlockWidget` plain/tool text height through the same helper as
  Markdown height, so scroll accounting and actual rendering no longer drift
  for CJK + whitespace-heavy model output.
- Hardened active user/model/tool-facing truncation paths in utils, session
  memory, task output, session title, and session storage so byte slicing does
  not split UTF-8.
- Deleted unwired `semantic_adapters/*` tests that were not compiled into the
  active model-runtime module; this prevents false confidence from dead-code
  test coverage.
- Added/verified regression coverage:
  - `render_snapshot_app_frame_sticky_scroll_follows_long_transcript_tail`
  - `chinese_reasoning_and_markdown_remain_utf8_safe`
  - `content_text_preserves_multibyte_markdown_chunks`
  - `compact_memory_truncates_multibyte_on_char_boundary`
  - `format_task_output_tail_truncates_multibyte_without_panic`
  - `extract_conversation_text_tail_truncates_multibyte_without_panic`
  - `sanitize_path_truncates_unicode_names_without_panic`
- Verified:
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui render_model -- --nocapture`
  - `cargo test -p mossen-agent multibyte -- --nocapture`
  - `cargo test -p mossen-utils multibyte -- --nocapture`
  - `cargo test -p mossen-utils utf8 -- --nocapture`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`
  - `cargo check -p mossen-cli`

2026-05-21 Batch C semantic-selector and root island retirement slice:

- Deleted `crates/mossen-tui/src/components/messages.rs` (2488 lines), an
  unused translated terminal-translated message component island with no workspace call
  sites.
- Removed the unused `root_large` transcript rendering islands:
  `VirtualMessageList`, translated scroll keybinding handler,
  `MessagesState/MessagesWidget`, message actions nav, and
  `MessageRowState/MessageRowWidget`.
- Kept the `root_large` selector/restore model because `App` and
  `app_services` still use `MessageSelectorState`, `RenderableMessage`, and
  `MessageSelectorWidget`.
- Added `RenderBlock::selector_summary()` so modal/list consumers can derive
  compact rows from Layer 2 semantics instead of reaching back into raw
  message payloads.
- `open_message_selector` now maps `RenderTranscript` blocks into selector
  rows, preserving semantic tool summaries and hiding raw JSON.
- Added regression coverage:
  - `message_selector_uses_semantic_render_summaries`
- Verified:
  - `cargo check -p mossen-tui`
  - `cargo test -p mossen-tui message_selector_uses_semantic_render_summaries -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`

2026-05-21 Batch C root-small/root-medium retirement slice:

- Removed `crates/mossen-tui/src/components/retired_compact_root.rs` and its module
  export after confirming there were no workspace call sites.
- Replaced the old 2140-line `root_medium` compatibility module with a
  69-line live module containing only `IdleReturnDialogState` and
  `IdleReturnDialogWidget`.
- Confirmed the only remaining `root_medium` workspace references are:
  `app_services` state construction, `App::render_modal`, and app tests.
- Verified:
  - `cargo check -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`

2026-05-21 Batch B semantic sidecar slice:

- Added `App::approval_decisions`, so newly produced approval decisions are
  Layer 1 sidecar records instead of hidden `MessageData::System` marker
  messages.
- Added `RenderTranscript::from_messages_and_decisions`, which merges
  sidecar decisions into the semantic transcript and inserts anchored
  decisions immediately after their tool card when the anchor is available.
- Kept `mossen-render:approval-decision:` parsing only as a backward
  compatibility adapter for existing transcript data.
- Routed `render_surface_model` and `open_message_selector` through
  `RenderTranscript::from_messages_and_decisions`, so normal frame rendering
  and selector rows see the same semantic decision blocks.
- Modal handlers now emit approval-decision facts after modal event handling,
  which keeps event production separate from widget rendering.
- Added regression coverage:
  - `sidecar_approval_decision_is_inserted_after_anchor_block`
  - existing `approval_decision_marker_becomes_semantic_block` remains as a
    compatibility test;
  - `accepted_tool_permission_persists_as_semantic_decision_block`
  - `message_selector_uses_semantic_render_summaries`
- Verified:
  - `cargo check -p mossen-tui`
  - `cargo test -p mossen-tui approval_decision -- --nocapture`
  - `cargo test -p mossen-tui accepted_tool_permission_persists_as_semantic_decision_block -- --nocapture`
  - `cargo test -p mossen-tui message_selector_uses_semantic_render_summaries -- --nocapture`

2026-05-21 Layer 1 record-boundary slice:

- Added `crates/mossen-tui/src/render_lifecycle.rs` as the Layer 1
  compatibility boundary for transcript facts and approval decisions.
- Moved approval-decision persistence types into Layer 1 and kept
  `render_model` re-exports only for compatibility with existing callers.
- `ApprovalDecisionModel` now carries a stable session-local `id`; `App`
  allocates ids through `next_render_record_seq`.
- `RenderTranscript::from_messages_and_decisions` now builds
  `TranscriptRecords` first, then constructs Layer 2 blocks through
  `RenderTranscript::from_records`.
- Hidden `mossen-render:approval-decision:` messages are extracted into Layer
  1 approval facts before normal transcript records are built, so the marker
  path stays historical and cannot leak as normal text.
- Tool blocks now use stable `tool-{ToolUse source index}` ids, and
  `tool-x-y` anchors remain accepted as legacy aliases for historical
  approval data.
- Added regression coverage:
  - `render_lifecycle::tests::extracts_legacy_approval_decisions_out_of_message_records`
  - `render_lifecycle::tests::assigns_lifecycle_without_terminal_layout`
  - `render_model::tests::tool_anchor_id_stays_stable_when_result_arrives`
- Verified:
  - `cargo test -p mossen-tui render_lifecycle -- --nocapture`
  - `cargo test -p mossen-tui tool_anchor_id_stays_stable_when_result_arrives -- --nocapture`
  - `cargo test -p mossen-tui render_model -- --nocapture`
  - `cargo test -p mossen-tui accepted_tool_permission_persists_as_semantic_decision_block -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`

2026-05-21 Layer 1 record-native bridge slice:

- `RenderTranscript::from_records` now iterates `TranscriptRecord` entries
  directly instead of rebuilding a temporary message list.
- Tool pairing now uses adjacent `TranscriptRecordKind::ToolUse` and
  `TranscriptRecordKind::ToolResult` records, keeping the tool block id from
  the ToolUse record.
- Render blocks now preserve Layer 1 `source_index` values instead of list
  positions after compatibility filtering.
- Approval decisions can anchor to true Layer 1 record ids such as
  `tool-call-shell-42`; historical `tool-x-y` aliases remain accepted.
- Added regression coverage:
  - `render_model::tests::from_records_uses_layer1_ids_source_indices_and_anchors`
- Verified:
  - `cargo test -p mossen-tui from_records_uses_layer1_ids_source_indices_and_anchors -- --nocapture`
  - `cargo test -p mossen-tui tool_anchor_id_stays_stable_when_result_arrives -- --nocapture`

2026-05-21 Layer 3 diff-rendering slice:

- Tool output sections now detect unified diff bodies before Markdown/plain
  fallback rendering.
- Diff headers and hunk headers use info styling; additions use success
  styling; removals use error styling.
- This stays in Layer 3: Layer 2 still provides semantic tool sections, while
  viewport rendering decides the terminal styling for diff-shaped text.
- Added regression coverage:
  - `widgets::render_block::tests::renders_unified_diff_sections_with_diff_semantics`
  - `render_snapshot_search_read_and_diff_polish` now includes a Bash stdout
    unified diff and asserts visible hunk/add/remove lines.
- Verified:
  - `cargo test -p mossen-tui renders_unified_diff_sections_with_diff_semantics -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_search_read_and_diff_polish -- --nocapture`

2026-05-21 Layer 1 engine-id and Layer 3 transcript-only slice:

- Added Layer 1 record-id overrides at the App boundary so engine
  `ToolUseBlock.id` values survive into `TranscriptRecord.id`.
- `render_surface_model`, active approval anchoring, persisted approval
  decisions, and approval pruning now build from the same record-aware
  transcript path.
- `MessagesWidget` no longer needs raw `MessageData` type inspection for
  supplied transcript rendering, content height, unseen dividers, or collapsed
  tool-result hiding.
- Added regression coverage:
  - `render_lifecycle::tests::applies_record_id_overrides_at_layer1_boundary`
  - `app::engine_stream_tests::engine_tool_use_id_flows_into_render_record_and_approval_anchor`
  - `widgets::messages::tests::supplied_transcript_renders_without_message_slice`
- Verified:
  - `cargo test -p mossen-tui applies_record_id_overrides_at_layer1_boundary -- --nocapture`
  - `cargo test -p mossen-tui engine_tool_use_id_flows_into_render_record_and_approval_anchor -- --nocapture`
  - `cargo test -p mossen-tui supplied_transcript_renders_without_message_slice -- --nocapture`
  - `cargo test -p mossen-tui render_model -- --nocapture`

2026-05-21 Layer 1 tool-result parent-id slice:

- Extended `SdkMessage::ToolUseSummary` with optional `tool_use_id` and set it
  from the actual `ToolResultBlock.tool_use_id` in the agent dialogue loop.
- Added App-side result record id overrides plus parent id overrides, shifted
  and cleared together with the existing record-id map.
- `TranscriptRecord` now carries `parent_id`, and
  `RenderTranscript::from_records` can pair ToolUse/ToolResult records by
  stable parent id rather than adjacency alone.
- Added regression coverage:
  - `render_lifecycle::tests::applies_parent_id_overrides_at_layer1_boundary`
  - `render_model::tests::from_records_pairs_tool_result_by_parent_id`
  - `app::engine_stream_tests::engine_tool_result_keeps_parent_tool_use_id`
- Verified:
  - `cargo test -p mossen-tui applies_parent_id_overrides_at_layer1_boundary -- --nocapture`
  - `cargo test -p mossen-tui from_records_pairs_tool_result_by_parent_id -- --nocapture`
  - `cargo test -p mossen-tui engine_tool_result_keeps_parent_tool_use_id -- --nocapture`
  - `cargo test -p mossen-tui -- --nocapture`
  - `cargo build -p mossen-cli --bin mossen`
  - `cargo fmt -p mossen-agent -p mossen-tui --check`
  - `cargo check -p mossen-tui`
  - `git diff --check`

2026-05-21 Layer 3 terminal-cell correctness slice:

- Red-line framing: these changes are on the active App rendering path, not
  the retired translated component islands.
- `widgets/text_input.rs` now keeps the placeholder legible instead of drawing
  the cursor over the first placeholder character, and horizontally scrolls
  long one-line input so the cursor/tail remains visible.
- `widgets/prompt_input.rs` now lays out prompt indicators, prefixes, and
  suggestion descriptions using terminal display width instead of byte/string
  length.
- `widgets::footer::FooterWidget` now reserves right-side cost/message metrics
  with Unicode display width, so wide project/model text cannot eat the footer
  metrics.
- `widgets/spinner.rs` now advances shimmer/status text by terminal cell width,
  preventing CJK status text from writing into wide-character continuation
  cells.
- `app.rs` now treats `state.is_streaming` as authoritative for the footer and
  spinner label unless the latest visible event is a running tool. This removes
  the active chrome bug where a streaming frame could still say `idle`.
- Added App-level integration coverage for the same active path:
  `render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells` renders a
  full `App::render_for_test` frame with CJK prompt text, spinner, status bar,
  model, cost, and message count.
- Important anti-placebo note: the retired `src/components` tree has now been
  removed. Completion evidence must come from `App::render_for_test`,
  `MessagesWidget -> RenderTranscript -> RenderBlockWidget`,
  `App::render_auxiliary_panels`, `ApprovalBlockWidget`, and active bottom
  chrome widgets.
- Added focused regression coverage:
  - `widgets::text_input::tests::focused_placeholder_keeps_first_character_visible`
  - `widgets::text_input::tests::long_multibyte_input_keeps_tail_and_cursor_visible`
  - `widgets::prompt_input::tests::active_prompt_renders_placeholder_without_hiding_first_character`
  - `widgets::prompt_input::tests::active_prompt_suggestions_do_not_overlap_multibyte_labels`
  - `widgets::footer::tests::footer_widget_keeps_right_metrics_visible_when_tiny`
  - `widgets::spinner::tests::spinner_row_clips_multibyte_message_by_display_width`
  - `app::engine_stream_tests::streaming_flag_drives_footer_and_spinner_state`
  - `render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells`
- Verified:
  - `cargo test -p mossen-tui --lib keeps -- --nocapture`
  - `cargo test -p mossen-tui --lib active_prompt -- --nocapture`
  - `cargo test -p mossen-tui --lib spinner_row_clips_multibyte_message_by_display_width -- --nocapture`
  - `cargo test -p mossen-tui --lib streaming_flag_drives_footer_and_spinner_state -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `cargo check -p mossen-cli`
  - `git diff --check`

2026-05-21 Layer 3 auxiliary-panel terminal-cell slice:

- Red-line framing: this slice targets active auxiliary rendering used by
  `App::render_auxiliary_panels`, not an isolated translated component island.
- `widgets/spinner.rs` now computes glimmer/teammate message width from
  grapheme clusters and clips rendered spinner text by terminal display width.
- `TeammateSpinnerLineWidget` now clips teammate names and status messages by
  terminal cells, and advances `x` by display width instead of byte length.
  This prevents CJK sub-agent names from overlapping right-rail content.
- Added App-level evidence:
  `render_snapshot_app_frame_teammate_panel_handles_multibyte_cells` renders a
  full `App::render_for_test` frame where `split_auxiliary_panels` exposes the
  teammate panel with a multibyte sub-agent name.
- Added focused regression coverage:
  - `widgets::spinner::tests::glimmer_message_clips_multibyte_by_display_width`
  - `widgets::spinner::tests::teammate_line_truncates_multibyte_name_by_display_width`
  - `render_snapshot_app_frame_teammate_panel_handles_multibyte_cells`
- Verified:
  - `cargo test -p mossen-tui --lib spinner_anim -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_app_frame_teammate_panel_handles_multibyte_cells -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
 - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 tool-output ANSI and height-consistency slice:

- Red-line framing: this slice targets the active transcript path
  `MessagesWidget -> RenderTranscript -> RenderBlockWidget`. It does not
  count the unused translated diff/custom-select widgets as evidence.
- Tool section bodies now normalize terminal output before viewport clipping:
  ANSI escape sequences are stripped, tabs become visible spacing, and long
  lines are clipped by terminal display width instead of raw character count.
  This keeps colored Bash stdout/stderr from leaking `\x1b[31m`-style control
  text into the transcript.
- `RenderBlockWidget::section_height` now measures clipped-output hint rows
  with the same wrapping logic used by `Paragraph`, instead of assuming the
  hint always fits on one row. This fixes the class of bugs where stderr/body
  content appears logically present but gets eaten by the tool-card bottom
  border after CJK wrapping.
- Added active-path evidence:
  - `widgets::render_block::tests::bounded_section_body_strips_ansi_and_clips_by_display_width`
  - `render_snapshot_bash_output_strips_ansi_and_clips_wide_lines`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 auxiliary task-panel row-width slice:

- Red-line framing: this slice covers the active task side panel reachable
  through `App::render_auxiliary_panels`, not the retired root compatibility
  task/list widgets.
- `TaskListV2Widget` now budgets each todo row by terminal display width:
  the status icon and spacer are measured first, then the todo content is
  clipped to the remaining terminal cells. CJK task titles can no longer push
  past the side-panel border or create a later buffer write outside the row.
- Added App-level evidence:
  - `render_snapshot_app_frame_task_side_panel_handles_multibyte_cells`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 inline approval height/body slice:

- Red-line framing: this slice covers active inline approvals rendered below
  transcript content through `App::active_inline_approval_height` and
  `ApprovalBlockWidget`. It does not count the old permission modal overlay,
  which active inline approvals intentionally bypass.
- `ApprovalBlockWidget::required_height` now matches the rows actually used by
  the widget: border rows, title, status/detail, body, actions, and help. This
  fixes the undercount where approval body text could be logically present but
  clipped away by the panel bottom border.
- Active App height reservation now measures the same capped inline panel width
  used by render, instead of measuring against the full terminal width. Long
  approval explanations therefore reserve the rows they will occupy on screen.
- Collapsed approval bodies now remain visible as one terminal-display-width
  clipped row with an ellipsis; expanded bodies continue to use wrapped body
  measurement.
- Added active-path evidence:
  - `widgets::approval::tests::required_height_keeps_collapsed_body_visible`
  - `render_snapshot_app_frame_shows_inline_approval_and_footer_state`
  - `render_snapshot_semantic_approval_model_matches_inline_surface`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 focused transcript height-budget slice:

- Red-line framing: this slice targets active transcript rows rendered by
  `MessagesWidget -> RenderBlockWidget`, including the keyboard-focused
  message path. It does not count inactive prompt widgets or retired
  transcript renderers.
- `RenderBlockWidget::required_height` now reserves the same focus-band cell
  that `RenderBlockWidget::render` consumes before calculating body wrapping.
  Without this, a focused message could measure with one extra cell, render
  with one fewer cell, and lose the wrapped tail row in virtual scrollback.
- Added active-path evidence:
  - `widgets::render_block::tests::focused_rows_reserve_the_focus_bar_width`
  - `render_snapshot_focused_message_keeps_wrapped_tail_visible`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 markdown wrap-height calibration slice:

- Red-line framing: this slice targets active Markdown/plain transcript height
  accounting used by `MessagesWidget -> RenderBlockWidget -> MarkdownWidget`.
  It does not count inactive Markdown/table helpers outside the active
  transcript path.
- The local wrapped-line estimator now handles long unspaced tokens by
  carrying only the current terminal line, instead of repeatedly counting the
  already-consumed part of the same token. This removes inflated scrollback
  height for long file paths, hashes, URLs, and single-token code output.
- The estimator keeps the existing CJK/soft-break behavior that protects
  sticky scrolling from undercounting ratatui's actual rendered rows. A failed
  attempt to swap in `textwrap` exposed that it undercounted the active CJK
  sticky-scroll fixture, so the accepted fix is narrower and evidence-based.
- Added active-path evidence:
  - `widgets::markdown::tests::wrapped_height_counts_long_words_by_terminal_width`
  - `widgets::markdown::tests::wrapped_height_combines_styled_spans_before_counting`
  - `render_snapshot_app_frame_sticky_scroll_follows_long_transcript_tail`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 modal/dialog terminal-cell slice:

- Red-line framing: this slice targets active modal renderers called by
  `App::render_modal`, not retired translated dialog code.
- `render_help_dialog` now pads the command column and truncates descriptions
  by terminal display width, so CJK slash-command names/descriptions do not
  drift into adjacent columns.
- `render_mcp_servers_dialog` now pads and truncates state, server-name, and
  detail columns by terminal display width, so wide server names and CJK
  status text cannot push rows past the modal border.
- Added App-level evidence:
  - `render_snapshot_app_frame_help_modal_handles_multibyte_columns`
  - `render_snapshot_app_frame_mcp_modal_handles_multibyte_columns`
- Verified:
  - `cargo test -p mossen-tui render_snapshot_app_frame_ -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 active modal/panel row-width slice:

- Red-line framing: this slice covers active modal/panel renderers reachable
  through `App::render_modal`, `open_message_selector`, and service-owned
  modal state. It does not count any unwired translated widget as evidence.
- `MessageSelectorWidget` now truncates selector rows by terminal display
  width after they have been derived from semantic `RenderTranscript`
  summaries.
- The in-session `Search` modal now truncates both the query row and match
  preview rows by terminal display width.
- `TasksDialog` now keeps task content, task status, background-agent ids, and
  agent status labels inside one terminal row using display-width budgets.
- Generic `Picker` rows now truncate multibyte labels by terminal display
  width before rendering.
- Active `ModelPickerWidget`, `SkillsPanelWidget`, and `MemoryPanelWidget`
  now budget their row fields by terminal display width, keeping provider,
  description, and category columns from drifting past the panel edge.
- `ShellOutputWidget` and `CondenseNoticeWidget` also use the same display
  width helper for their single-line clipped rows, reducing future divergence
  if those widgets are reconnected.
- Added App-level evidence:
  - `render_snapshot_app_frame_message_selector_handles_multibyte_rows`
  - `render_snapshot_app_frame_search_modal_handles_multibyte_rows`
  - `render_snapshot_app_frame_tasks_dialog_handles_multibyte_rows`
  - `render_snapshot_app_frame_picker_handles_multibyte_items`
  - `render_snapshot_app_frame_model_skills_memory_panels_handle_multibyte_rows`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-21 Layer 3 inline prompt suggestion height slice:

- Red-line framing: this slice targets the active bottom chrome path
  `App::render_inline -> PromptInputWidget`, not inactive prompt components
  or command execution behavior.
- Found by first adding an App-level failing fixture:
  `render_snapshot_app_frame_inline_prompt_shows_command_suggestions`.
  Before the fix, inline mode hard-coded the prompt area to 2 rows, so `/`
  command suggestions had no allocated rows and `/plan` disappeared from the
  real frame.
- Fixed `App::render_inline` to reserve
  `PromptInputWidget::required_height()` just like fullscreen mode already
  did. This keeps command/skill suggestions attached to the bottom prompt
  surface while preserving the separate status-row footer.
- Active evidence:
  - failing-before/passing-after
    `render_snapshot_app_frame_inline_prompt_shows_command_suggestions`
  - full active snapshot suite `cargo test -p mossen-tui render_snapshot -- --nocapture`
- Verification so far:
  - `cargo test -p mossen-tui render_snapshot_app_frame_inline_prompt_shows_command_suggestions -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`

2026-05-21 Layer 3 semantic footer renderer slice:

- Red-line framing: this slice removes a live adapter in the active footer
  path. The retired status widget is not present in the active TUI tree and is
  not counted as evidence for the App frame.
- Added `widgets::footer::FooterWidget`, a Layer 3 renderer that consumes
  `FooterRenderModel` directly. `App::render_status_bar` no longer converts
  the semantic footer through the retired status widget.
- The widget preserves project/model/access mode, blocking badge, thinking
  indicator, MCP summary, cost, and message-count cells from the same
  `RenderSurface.footer` used by transcript/approval/spinner.
- Tiny-width right metrics now keep the tail visible with display-width-safe
  start truncation, so message-count/status cells are not lost behind a long
  left side.
- Active evidence:
  - `widgets::footer::tests::footer_widget_renders_semantic_footer_model_directly`
  - `widgets::footer::tests::footer_widget_keeps_right_metrics_visible_when_tiny`
  - `render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells`
  - `render_snapshot_app_frame_shows_inline_approval_and_footer_state`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui footer_widget -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_app_frame_bottom_chrome_handles_multibyte_cells -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot_app_frame_shows_inline_approval_and_footer_state -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`

2026-05-21 Batch C renderer-tree retirement closure:

- Red-line framing: this is cleanup of inactive rendering roots, not a claim
  that command execution or the harness loop is complete.
- Removed the public `components` module from `mossen-tui` and deleted the
  retired `crates/mossen-tui/src/components` source tree. The active App path
  now imports render state from `approval_state.rs` and widgets from
  `crates/mossen-tui/src/widgets`.
- Active replacements are now:
  - `approval_state.rs` for permission interaction state;
  - `widgets/approval.rs` for inline approval cards;
  - `widgets/footer.rs` for status/footer chrome;
  - `widgets/message_selector.rs`, `widgets/idle_return.rs`,
    `widgets/cost_threshold.rs`, `widgets/panels.rs`,
    `widgets/task_list.rs`, and `widgets/spinner.rs` for formerly split
    modal/auxiliary renderer responsibilities.
- Anti-placebo checks:
  - `rg` over active TUI source/tests finds no imports from the retired
    component module;
  - `rg --files crates/mossen-tui/src/components` returns no files;
  - source/docs search excluding dependency lockfiles finds no retired
    provider/model branding.
- Verified so far:
  - `cargo fmt -p mossen-tui --check`
  - `cargo check -p mossen-tui`
  - `cargo test -p mossen-tui render_snapshot_app_frame_complex_turn_survives_resize_matrix_without_noise -- --nocapture`

2026-05-21 Layer 3 spinner/stall semantics closure:

- Red-line framing: this fixes a live bottom-chrome rendering semantic bug,
  not the agent/harness execution loop.
- Before this slice, `SpinnerRowWidget` considered any turn older than three
  seconds stalled. That made normal long streaming answers look stuck and
  turned the spinner red even while engine events were still arriving.
- `SpinnerState` now tracks two clocks:
  - `elapsed()` for total turn time shown in the status text;
  - `idle_for()` for time since the last engine/render activity.
- `App::handle_engine_message` marks spinner activity for every in-flight
  engine message, so fresh deltas/tool events keep the visual state active
  without resetting the user-visible elapsed seconds.
- Added regression coverage:
  - `widgets::spinner::tests::spinner_activity_keeps_long_active_turn_from_becoming_stalled`
  - `app::engine_stream_tests::engine_activity_clears_spinner_stalled_without_resetting_elapsed_time`
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo test -p mossen-tui activity -- --nocapture`
  - `cargo test -p mossen-tui render_snapshot -- --nocapture`
  - `cargo test -p mossen-tui --lib -- --nocapture`
  - `cargo check -p mossen-cli`
  - `python3 scripts/layer_boundary_audit.py`
  - `python3 scripts/smoke_check.py`
  - `git diff --check`

2026-05-22 Permission-mode execution wiring:

- Red-line framing: this does not complete the whole harness/memory/skill/MCP
  program; it closes the gap where `/permissions` only changed TUI state while
  the engine still executed every approval tool through the same default path.
- Added an engine-side `PermissionMode` and threaded it through
  `PromptParams -> OrchestratorConfig -> DialogueSpec`.
- TUI dispatch now converts the selected `/permissions` mode into the agent
  request, so future tool-use decisions observe the visible session mode.
- Tool execution now applies mode overrides before the interactive gate:
  `default` preserves the current modal gate, `plan` blocks mutating tools
  while allowing `ExitPlanMode`, `acceptEdits` auto-allows file edit tools,
  `bypassPermissions`/`auto`/`yolo` auto-allow, and `dontAsk` blocks approval
  tools instead of prompting.
- Added targeted agent tests for mode parsing and mode decisions.
- Verified:
  - `cargo check -q -p mossen-agent`
  - `cargo check -q -p mossen-tui`
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-agent dialogue::tests`
  - `cargo test -q -p mossen-tui --test keybinding_smoke permissions_slash_picker_updates_visible_session_mode`

2026-05-22 Skill tool execution wiring:

- Red-line framing: this does not finish the whole skills/plugin product
  surface. It closes the agent-loop hole where the `Skill` tool existed in
  the tool list but always returned a stub error when the model invoked it.
- Added `mossen-skills` as a `mossen-tools` dependency and wired
  `mossen_tools::skill::CraftInvoker` to the loaded dynamic and bundled craft
  registries.
- The `Skill` tool now resolves aliases, executes the craft template with the
  current cwd/session context, returns structured JSON with `allowedTools` and
  prompt text, and reports missing skills as structured tool errors.
- Added tool-level tests for executing a disk-loaded dynamic skill and for the
  missing-skill error path.
- Verified:
 - `cargo check -q -p mossen-tools`
 - `cargo check -q -p mossen-cli`
 - `cargo test -q -p mossen-tools skill::tests`

2026-05-22 MCP tool-definition injection:

- Red-line framing: this does not complete MCP resources/prompts, channel
  approval, or plugin-provided MCP install flows. It closes the immediate
  agent-loop gap where MCP call routing existed but connected MCP tools were
  not advertised in the model-visible `tools` list.
- Added MCP protocol-tool conversion into the Mossen `ToolDefinition` shape,
  preserving JSON Schema `properties`, `required`, and extra schema fields.
- CLI REPL startup now collects connected MCP server tools after
  `connect_all()` and attaches them to the TUI as extra model-visible tool
  definitions.
- TUI prompt dispatch now merges built-in registry definitions with those
  extra definitions, while execution still bypasses the built-in registry for
  `mcp__...` calls and routes through the live MCP manager.
- Agent MCP execution now resolves normalized `mcp__server__tool` names back
  to the original connected server and original MCP tool name before invoking
  `tools/call`, so names containing spaces or punctuation do not become
  advertised-but-unusable.
- Added focused tests for MCP schema conversion and for forwarding extra tool
  definitions into `PromptParams`.
- Verified:
  - `cargo check -q -p mossen-mcp`
  - `cargo test -q -p mossen-mcp tools::tests`
  - `cargo check -q -p mossen-agent`
 - `cargo check -q -p mossen-tui`
 - `cargo check -q -p mossen-cli`
 - `cargo test -q -p mossen-tui engine_stream_tests::extra_tool_definitions_are_forwarded_to_engine_params`

2026-05-22 Skill inventory system-prompt wiring:

- Red-line framing: this does not complete plugin skill install flows or
  dynamic mid-session prompt refresh. It closes the gap where the `Skill` tool
  could execute loaded skills but the system prompt still treated the session
  as having zero user-invocable skills.
- Bundled skill registration now upserts by skill name, so repeated startup
  initialization does not duplicate bundled entries.
- REPL and oneshot startup now initialize bundled skills, refresh cwd skill
  discovery, format the loaded user-invocable skill inventory, and pass both
  skill count and skill list into system prompt assembly.
- System prompt assembly now includes a `User-invocable skills` block only when
  the `Skill` tool is enabled and skills are present, and session-specific
  guidance now uses the real loaded skill count.
- Added focused tests for loaded dynamic skill listing and system prompt skill
  inventory injection.
- Verified:
  - `cargo test -q -p mossen-tools skill_tool::prompt::tests`
 - `cargo test -q -p mossen-cli system_prompt::tests`
 - `cargo check -q -p mossen-cli`
 - `cargo check -q -p mossen-tools`
 - `cargo check -q -p mossen-skills`

2026-05-22 SessionStart hook execution wiring:

- Red-line framing: this does not complete plugin hook hot-load, setup hooks,
  pre/post compact hooks, or visible TUI hook message rendering. It closes the
  startup gap where SessionStart was only logged as a fake event instead of
  executing the real hook pipeline.
- Setup now captures the hooks config snapshot before session routing, matching
  the existing startup contract for settings-backed hooks.
- REPL startup and oneshot startup now call `process_session_start_hooks` with
  a CLI-built `HooksContext`, then execute through
  `hooks_utils::execute_session_start_hooks`.
- The hook context now includes session id, cwd/project root, model,
  interaction mode, trust state, registered hook config, settings hook snapshot,
  transcript path resolvers, current subprocess environment, and settings
  accessors.
- Interactive startup maps restore mode to `Resume`; fresh REPL and oneshot
  startup use `Startup`. Oneshot forces synchronous hook execution because
  there is no interactive UI loop to absorb delayed hook output.
- Hook results preserve message attachments, additional contexts,
  initial-user-message injection, and watch-path requests from the shared hook
  pipeline.
- Current limitation: marketplace/cache-backed plugin discovery is still not a
  full `PluginLoaderEnv` implementation. A follow-up slice should wire the full
  plugin loader environment rather than only built-in and inline plugin hook
  sources.
- Verified:
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-cli session_hooks::tests`

2026-05-22 Setup hook execution wiring:

- Red-line framing: this still does not complete setup hook UI surfacing,
  marketplace/cache plugin discovery, hook hot-reload, or maintenance-triggered
  setup hooks. It closes the startup gap where setup captured hook settings but
  never executed the `Setup` hook event.
- CLI setup now runs `process_setup_hooks` after capturing the hook config
  snapshot and before deferred background prefetch starts.
- Setup hook execution reuses the same CLI hook context adapter and plugin hook
  loader as SessionStart, so settings hooks, SDK hooks, and plugin hooks share
  one registration and execution path.
- Setup hooks run with the `init` trigger and forced synchronous execution,
  because this phase happens before the interactive TUI event loop can safely
  receive delayed hook output.
- Added the setup executor adapter beside SessionStart so both event families
  build `HooksContext` at execution time after plugin hook registration.
- Verified:
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-cli session_hooks::tests`

2026-05-22 SessionStart plugin hook loader wiring:

- Red-line framing: this still does not complete marketplace plugin install,
  plugin refresh hot-reload, setup hooks, or hook output rendering in the TUI.
  It closes the immediate no-op loader gap for SessionStart by registering
  plugin hook matchers before the hook executor builds its runtime context.
- Bootstrap now stores SDK-registered hooks and plugin-registered hooks
  separately, so a plugin hook reload no longer clears SDK hook callbacks.
  `get_registered_hooks` merges both views for the shared hook executor.
- Plugin hook payloads now remain as JSON hook objects instead of lossy strings,
  so command/prompt/agent/http hook definitions can be parsed by the existing
  hook executor.
- SessionStart now uses a CLI plugin hook loader that collects enabled built-in
  plugin hook config, enabled built-in skill plugin hook config, and inline
  plugin `hooks/hooks.json` entries from bootstrap or
  `MOSSEN_CODE_INLINE_PLUGIN_DIRS`.
- The SessionStart executor now builds `HooksContext` at execution time, after
  plugin hooks are loaded, so newly registered plugin matchers are visible to
  `execute_session_start_hooks`.
 - Added parser/registration regression coverage for wrapped plugin
  `hooks/hooks.json` files and preservation of plugin metadata plus command
  payloads.
- Verified:
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-cli session_hooks::tests`

2026-05-22 Startup hook context consumption:

- Red-line framing: this still does not complete visible hook cards,
  persisted hook transcripts, or hook hot-reload. It closes the gap where
  SessionStart hook output was produced but not consumed by the first model
  request.
- TUI startup now stores `hook_additional_context` messages as pending
  `ContentBlock::Text` values, shows a lightweight system row in the
  transcript, and drains the context into the next normal prompt submission.
- REPL startup now consumes hook-provided initial user messages; when no
  explicit initial prompt is present, it queues that message through the same
  submit path used by typed input.
- Oneshot startup now injects hook additional context into
  `PromptParams.additional_blocks`; a hook initial user message becomes the
  prompt when the CLI prompt is empty, or is attached as startup context when a
  prompt already exists.
- Added focused TUI coverage proving startup hook context is included in the
  first prompt and then cleared.
- Verified:
  - `cargo check -q -p mossen-tui`
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-tui engine_stream_tests::startup_hook_context_drains_into_first_prompt`
  - `cargo test -q -p mossen-cli session_hooks::tests`

2026-05-22 Non-blocking compact task path:

- Red-line framing: this still does not replace the placeholder compaction
  summarizer with the final model-backed implementation. It closes the UI
  responsiveness gap where `/compact` ran compaction directly inside key
  handling and would freeze rendering once real compaction work became slower.
- `/compact` now enqueues a compact task, marks compact progress immediately,
  and lets the TUI main loop/tick path launch and poll the task result.
- Production runs use the active tokio runtime to spawn compaction; tests
  without a runtime keep a synchronous tick fallback so the state machine is
  deterministic.
- Compact cancellation now clears pending/active compact tasks and stale
  compact results are ignored by task id.
- Active frame scheduling now treats compact-in-progress as active work, so
  the progress banner and process list keep repainting while compaction runs.
- Verified:
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact`

2026-05-22 Compact summary preservation fallback:

- Red-line framing: this still does not complete the final model-backed
  compaction summarizer or pre/post compact hook execution. It closes the
  correctness gap where `/compact` simply dropped the older half of model
  history without preserving any context.
- `compact_conversation` now emits a model-visible meta user summary for the
  compacted-away messages, then appends the recent messages that remain in
  the active context.
- Partial compaction uses the same summary-plus-recent shape, so both full and
  partial compact paths preserve old-message intent instead of silently losing
  it.
- The local fallback summarizes text, tool-use, tool-result, and image blocks
  with bounded previews so compact output cannot balloon the next prompt.
- Updated `/compact` preview and smoke coverage for the new
  `messages 4 -> 3` shape: summary row plus two recent messages.
- Verified:
  - `cargo fmt -p mossen-agent -p mossen-tui`
  - `cargo check -q -p mossen-agent`
  - `cargo check -q -p mossen-tui`
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-agent services::compact::compact::tests::compact_conversation_keeps_summary_plus_recent_messages`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact`
  - `cargo test -q -p mossen-tui engine_stream_tests::startup_hook_context_drains_into_first_prompt`

2026-05-22 Compact hook execution bridge:

- Red-line framing: this still does not complete the final model-backed
  compaction summarizer. It does connect manual `/compact` to the real
  PreCompact/PostCompact hook runtime instead of the previous log-only stub.
- Agent compact now has `compact_conversation_with_options`, which accepts a
  `HooksContext`, trigger, custom instructions, cancellation token, and timeout.
- PreCompact hook stdout is merged with custom compact instructions and is
  visible in the local fallback summary as applied compaction instructions.
- PostCompact hooks receive the generated compact summary, and both pre/post
  hook display messages are returned with the compact result for transcript
  rendering.
- The REPL launcher builds a session hook context from bootstrap state and
  passes it into the TUI engine config, so `/compact` uses the same hook and
  plugin registration surface as SessionStart/Setup.
- `/compact plan` now exposes whether a compact hook runtime is configured.
- Verified:
  - `cargo fmt -p mossen-agent -p mossen-tui -p mossen-cli`
  - `cargo check -q -p mossen-agent`
  - `cargo check -q -p mossen-tui`
  - `cargo check -q -p mossen-cli`
  - `cargo test -q -p mossen-agent services::compact::compact::tests::compact_conversation_executes_pre_and_post_compact_hooks`
 - `cargo test -q -p mossen-agent services::compact::compact::tests::compact_conversation_keeps_summary_plus_recent_messages`
 - `cargo test -q -p mossen-tui --test keybinding_smoke compact`
 - `cargo test -q -p mossen-cli session_hooks::tests`
 - `cargo test -q -p mossen-tui engine_stream_tests::startup_hook_context_drains_into_first_prompt`

2026-05-23 Compact cancellation/status controls:

- Red-line framing: this still does not complete the final model-backed
  compaction summarizer or full Codex CLI parity. It closes the terminal
  control gap where manual `/compact` could launch real hook work without a
  task-local cancellation token or a direct status/cancel command.
- `/compact` now owns a `CancellationToken` for each queued/running compact
  task and passes it into PreCompact/PostCompact hook execution.
- Ctrl+C/turn cancellation, render-snapshot hydration cleanup, and
  `/compact cancel` clear pending/active compact state and cancel the active
  token so slow hooks can stop through the same mechanism.
- `/compact status` opens a TUI status modal with running/idle state, task id,
  pending-launch state, cancellability, hook runtime availability, current
  progress text, and the `/compact cancel` hint.
- Focused TUI smoke coverage verifies that status/cancel do not mutate
  `engine_history`, stale compact ticks do not apply after cancel, and the
  transcript records `(compact) cancelled`.
- Focused agent coverage verifies a pre-cancelled compact token prevents hook
  stdout/instructions from entering the fallback summary.
- Verified:
  - `cargo fmt -p mossen-agent -p mossen-tui -p mossen-cli`
  - `cargo check -q -p mossen-agent`
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-agent services::compact::compact::tests::compact_conversation_respects_cancelled_hook_token`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact_status_and_cancel_keep_history_unmutated`
  - `cargo test -q -p mossen-tui --test keybinding_smoke compact`

2026-05-23 Scrollable/filterable slash help catalog:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a slash-command usability gap where `/help`
  rendered the whole command/skill catalog into a fixed modal with no scroll
  state, so a large dynamic catalog could become impossible to inspect.
- `ActiveModal::HelpDialog` now carries `HelpDialogState` with scroll offset
  and an optional query string.
- `/help` and `/` open the same catalog; `/help <query>` filters by command
  name, description, or category while keeping command and skill styling.
- The help modal renders a bounded viewport with a stable footer range and
  handles Up/Down/PageUp/PageDown/Home/End/Esc without mutating transcript or
  task execution state.
- Existing multibyte help rendering tests now construct the explicit help
  state, preserving CJK width coverage.
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke help_dialog_scrolls_and_filters_slash_catalog`
  - `cargo test -q -p mossen-tui --test keybinding_smoke slash_typeahead_lists_commands_and_accepts_with_prefix`
  - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui`
  - `cargo test -q -p mossen-tui --test render_snapshot render_snapshot_app_frame_help_modal_handles_multibyte_columns`
  - `cargo test -q -p mossen-tui --test render_contract help` compiled the render-contract target; the filter matched no test names in that crate.

2026-05-23 Scrollable command-output modal:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a generic terminal-rendering gap where long
  slash-command and diagnostic output could be opened in a fixed modal with no
  navigation path.
- `ActiveModal::CommandOutput` now renders through a bounded viewport with a
  footer range indicator instead of relying on wrapped full-body rendering.
- The modal handles Up/Down/PageUp/PageDown/Home/End/Esc, and render-time
  clamping keeps stale scroll offsets within the current terminal-sized body
  viewport.
- Slash command dispatch resets command-output scroll so separate command
  results do not inherit the previous modal position.
- Focused smoke coverage verifies first page, PageDown, End, Home, and Esc
  behavior on a long command-output body.
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke command_output_modal_scrolls_long_body`

2026-05-23 Modal-first mouse-wheel routing:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a terminal interaction gap where mouse-wheel
  input always scrolled the transcript even while a scrollable modal was the
  visible active surface.
- Mouse-wheel events now route to the active modal first; the main transcript
  scrolls only when no modal is open.
- The route covers help, raw transcript, diff review, list/detail history
  modals, debug config, model/skill/memory pickers, generic pickers, and the
  bounded command-output modal.
- Non-scrollable modals consume wheel input instead of moving hidden
  transcript state behind the overlay.
- Added a mouse-event test seam and focused smoke coverage for `/help` plus a
  long command-output modal, including proof that background transcript offset
  is not mutated by modal wheel input.
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke mouse_wheel_scrolls_active_modal_before_transcript`

2026-05-23 Dirty-on-change mouse render scheduling:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a redraw-efficiency gap where mouse events could
  schedule frames even when no visible render state changed.
- Mouse events now mark the render frame dirty only when visible
  scroll/selection state changes.
- Wheel input over a non-scrollable modal is consumed without repainting hidden
  transcript state behind the overlay.
- Mouse move/drag remains reserved for future widget routing and no longer
  schedules idle frames by itself.
- Added focused dirty-frame coverage for mouse move, static-modal wheel input,
  transcript wheel input, and scrollable help-modal wheel input.
- Verified:
  - `cargo fmt -p mossen-tui --check`
  - `cargo check -q -p mossen-tui`
  - `cargo test -q -p mossen-tui --lib mouse_events_dirty_only_when_visible_scroll_state_changes`
  - `cargo test -q -p mossen-tui --test keybinding_smoke mouse_wheel_scrolls_active_modal_before_transcript`

2026-05-23 Diff review intraline change highlighting:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes one diff-review readability gap called out by
  the Codex CLI evidence set: changed lines should help reviewers see the
  edited fragment, not only the whole added/removed line.
- `DiffDetailWidget` now detects adjacent removed/added lines and splits each
  line into unchanged prefix, changed middle, and unchanged suffix spans.
- The changed fragment keeps the add/remove semantic color and gains
  bold/underline emphasis, with stronger background color when the active theme
  supports color.
- The implementation stays at Layer 3: unified-diff parsing, file folding,
  scroll state, and `/diff` modal state are unchanged.
- Added buffer-cell style coverage proving that changed removed/added fragments
  are emphasized while the unchanged prefix keeps the base diff style.
- Verified:
  - `cargo test -q -p mossen-tui widgets::diff::tests`
  - `cargo check -q -p mossen-tui`

2026-05-23 Rendered-viewport diff review scrolling:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a visible terminal interaction gap where
  Diff Review navigation used a fixed 20-row page size even when the rendered
  modal body was shorter or taller.
- `App::render_frame` now records the actual terminal width/height seen by the
  last frame, so modal key/mouse handlers can make decisions from the real
  viewport instead of stale defaults.
- Diff Review now derives its PageUp/PageDown, End, and mouse-wheel scroll
  bounds from the rendered modal content height.
- Diff Review clamps over-large scroll offsets at render time after resize, so
  a smaller terminal does not create an empty or unreachable tail.
- The footer now exposes a `start-end/total` range, matching the help and
  command-output modals, so users can see that the diff is scrollable.
- Added a small-terminal smoke test proving that PageDown advances by the
  rendered 11-row body height rather than the old fixed 20 rows.
- Verified:
  - `cargo test -q -p mossen-tui --test keybinding_smoke diff_review_uses_rendered_viewport_for_scroll_range`

2026-05-23 Rendered-viewport help and command-output modal scrolling:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes the same terminal interaction class for Help
  and CommandOutput modals: navigation must use the visible modal body height,
  not fixed historical constants.
- Help and CommandOutput now share their modal-height and content-viewport
  calculations between rendering, keyboard PageUp/PageDown/End handling, and
  mouse-wheel scroll routing.
- Tiny terminal heights are clamped to the actual frame height so modal
  viewport math does not invent rows outside the visible terminal.
- Existing smoke tests now assert rendered `start-end/total` ranges and prove
  PageDown advances by the 11-row body rendered in an 18-row terminal.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke help_dialog_scrolls_and_filters_slash_catalog`
  - `cargo test -q -p mossen-tui --test keybinding_smoke command_output_modal_scrolls_long_body`

2026-05-23 Rendered-viewport semantic inspection modal scrolling:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes the same terminal interaction class for the
  semantic inspection modals that were still paginating with fixed 18/20-row
  constants.
- `/raw`, `/files`, `/timeline`, `/ps`, `/commands`, `/errors`, `/results`,
  `/approvals`, and `/debug-config` now derive keyboard and mouse scroll
  bounds from the rendered modal height.
- Shared modal-height helpers now model each widget's real reserved chrome:
  raw transcript reserves one footer row; semantic list/detail widgets reserve
  summary, spacer, and footer rows.
- Render paths clamp visible list scroll through a cloned state before drawing,
  so resize changes do not leave a selected row outside the visible viewport.
- Added small-terminal smoke coverage proving `/raw` PageDown advances by the
  rendered 11-row body and `/files` plus `/commands` PageDown advance by the
  rendered 9-row semantic list viewport.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke raw_transcript_uses_rendered_viewport_for_page_navigation`
 - `cargo test -q -p mossen-tui --test keybinding_smoke semantic_list_modals_use_rendered_viewport_for_page_navigation`
 - `cargo test -q -p mossen-tui --test keybinding_smoke builtin_slash_commands_open_expected_ui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
 - `cargo check -q -p mossen-tui`
 - `cargo fmt -p mossen-tui --check`
 - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`

2026-05-23 Resize/focus storm coalescing:

- Red-line framing: this remains TUI render-loop responsiveness work only. It
  does not change task execution, agent semantics, permissions, or harness
  behavior.
- Terminal dragging and focus churn can enqueue many resize/focus events before
  the next frame. Rendering every intermediate size is wasted work and can show
  up as lag or flicker during long sessions.
- The coalesced event batch now treats consecutive `Resize` and consecutive
  `FocusChange` events as lossy runs and keeps only the latest event in each
  run. Keyboard, mouse, quit, and other non-lossy events still flush pending
  runs so input boundaries keep their order.
- Added a regression test proving resize/focus storms collapse at an input
  boundary while the key event and later resize run are preserved.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui event::tests::recv_coalesced`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/event.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 Render event backlog coalescing:

- Red-line framing: this is still TUI render-loop responsiveness work only. It
  does not change task execution, agent semantics, permissions, or harness
  behavior.
- The PRD calls out no lag, no flicker, and usable scrolling during long
  tasks. The run loop previously consumed one event per iteration from an
  unbounded event queue while a 30fps tick source kept producing animation
  wakeups. A slow frame could therefore leave stale ticks ahead of keyboard,
  mouse, resize, or engine-visible work.
- `EventBus` now supports coalesced batch receive: after one wakeup it drains a
  bounded number of ready events and collapses redundant `Tick` events to one.
  User input, resize, focus, quit, and other non-tick events remain in order.
- `App::run` now handles these event batches in both streaming and idle paths,
  stopping the batch immediately once quit is requested. This reduces stale
  wakeup backlog without forcing extra frames or touching the renderer surface.
- Added a regression test proving stale ticks are dropped while resize, focus,
  and quit events behind them are preserved in the same batch.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui event::tests::recv_coalesced_drops_stale_ticks_without_dropping_input`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/src/event.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 Transcript scrollbar pointer routing:

- Red-line framing: this is still TUI-rendering work only. It does not change
  task execution, agent loop semantics, or the broader harness systems.
- The external rendering references call out long-session control, non-stuck
  scrollback, and avoiding fake UI affordances as product requirements. The
  previous slice rendered a position rail; this slice makes the rail an actual
  control so users can click or drag the transcript scrollbar instead of being
  limited to wheel/key scrolling.
- `App` now stores the last rendered transcript scrollbar rail area, clears it
  whenever the rail is not present, and routes left-click/left-drag mouse
  events to the transcript only when no modal owns input.
- Pointer position maps directly into the same row-based `VirtualScroll`
  offset used by keyboard/wheel scrolling. Dragging to the bottom restores
  sticky-bottom behavior; clicking above the bottom exits sticky mode.
- Added smoke coverage proving a long transcript rail accepts top click and
  bottom drag, hides the tail after leaving sticky bottom, then restores the
  tail after dragging back down.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_mouse_click_and_drag`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 Modal scrollbar pointer routing:

- Red-line framing: this is still TUI-rendering work only. It does not change
  task execution, agent loop semantics, or the broader harness systems.
- The rendering references require long panels to stay readable and controllable
  instead of exposing fake scroll affordances. This closes that interaction gap
  for modal and inspection surfaces, matching the transcript rail behavior.
- Added one shared modal scrollbar hit target plus a generic scrollbar widget
  path. Help, command output, raw transcript, diff review, file changes,
  timeline, process list, command/error/final/approval histories, and debug
  config now render overflow rails with the current glyph profile.
- Left click and drag on a modal rail maps pointer row into the same existing
  scroll state used by keyboard and wheel input. Selectable list surfaces keep
  the selected row visible; expanded detail-history modals skip the list rail so
  detail scrolling owns input.
- Added smoke coverage proving a long command-output modal rail can jump to the
  bottom and drag back to the top.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke command_output_modal_scrolls_long_body`
  - `cargo test -q -p mossen-tui --test keybinding_smoke command_output_modal_scrollbar_tracks_mouse_click_and_drag`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`
  - trailing-whitespace scan over the touched files

2026-05-23 Transcript scrollbar position feedback:

- Red-line framing: this still does not complete full Codex CLI parity or the
  execution harness. It closes a product-level terminal feedback gap from the
  PRD's "可回看" and long-task requirements: long transcripts could be scrolled,
  but users had no visible position indicator and could mistake manual scroll
  for a stuck bottom.
- The main transcript now reserves a one-column right rail only when rendered
  content overflows the message viewport. The rail uses ratatui's stateful
  scrollbar with Unicode and ASCII symbols, so it degrades cleanly with the
  existing glyph profile.
- Scroll accounting measures the same reduced content width that is rendered
  when the rail is present, keeping sticky-bottom and manual-scroll offsets in
  sync with wrapped rows.
- Added smoke coverage proving the rail appears for a long transcript, sticky
  bottom shows the tail, and the scrollbar thumb moves upward after manual
  scroll before the next render.
- Verified:
  - `cargo fmt -p mossen-tui`
  - `cargo test -q -p mossen-tui --test keybinding_smoke transcript_scrollbar_tracks_sticky_and_manual_scroll`
  - `cargo test -q -p mossen-tui --test keybinding_smoke`
  - `cargo test -q -p mossen-tui --test render_contract`
  - `cargo check -q -p mossen-tui`
  - `cargo fmt -p mossen-tui --check`
  - `git diff --check -- crates/mossen-tui/src/app.rs crates/mossen-tui/tests/keybinding_smoke.rs phases/03g-rendering-product-grade-plan.md`

2026-05-23 Session permission rule gate:

- Red-line framing: this is the first narrow return from slash-command UI into
  execution behavior. It does not finish Codex CLI parity or the full harness,
  but it makes `/permissions allow|deny|reset` session state real instead of
  being display-only.
- `/permissions` now mirrors allow/deny rules through command-context env so
  the registry output and the TUI session state report the same source of
  truth.
- `App` hydrates session permission rules from env, applies rule subcommand
  side effects before registry execution, and wraps the interactive permission
  prompt gate with a rule gate for each prompt dispatch.
- Deny rules take precedence over allow rules. Matching covers tool names,
  common tool input fields, tool-plus-input candidates, command prefixes, path
  prefixes, and `*` wildcard patterns.
- Added smoke and focused test coverage for command display, slash routing
  side effects, and rule-before-fallback permission decisions.

2026-05-23 Compact custom instruction bridge:

- Red-line framing: this does not complete the full harness/context system, but
  it removes a practical `/compact` gap on the path to production context
  management.
- `/compact <instructions>` and `/compact run <instructions>` now preserve the
  user-provided summarization focus and pass it into
  `CompactConversationOptions.custom_instructions` instead of silently dropping
  it.
- `/compact plan <instructions>` keeps history unchanged while showing the same
  custom instruction text in the preview body, so the preview reflects the
  eventual compaction request.
- The compact progress state now distinguishes custom-instruction compactions,
  and keybinding coverage verifies the generated summary records the applied
  instructions.

2026-05-23 Auto-compact boundary metadata:

- Red-line framing: this is agent/context production work, not another TUI
  rendering slice. It does not complete the full harness, but it closes a
  concrete context-management gap in the main dialogue loop.
- Successful auto-compact in `context::auto_compact_if_needed` now prepends a
  model-context boundary message with `compact_metadata`, matching the
  existing compact boundary consumers for recent-compact/session-memory
  bookkeeping.
- The boundary records trigger, pre/post token counts, and compacted message
  count. The summary and recent messages remain after the boundary.
- A zero-message compaction result is now treated as a failed compact attempt
  instead of resetting the circuit breaker while leaving the over-limit context
  unchanged.

2026-05-23 Session memory compact bridge:

- Red-line framing: this is agent/context/memory work. It still does not finish
  the whole memory system, but it removes the no-op bridge where session memory
  compaction could never be used.
- `get_session_memory_content` now reads a non-empty session memory snapshot
  from an explicit env path or the default `.mossen` session-memory locations,
  including the JSON-backed session memory store.
- `try_session_memory_compaction` now builds a real `CompactionResult` from
  session memory, preserves the recent message segment using the existing API
  invariant logic, and marks the boundary with `trigger=session_memory`
  metadata.
- The main `context::auto_compact_if_needed` path now tries session-memory
  compaction first when enabled, then falls back to normal conversation
  compaction when no usable memory snapshot/result exists.

2026-05-23 Compact lifecycle boundary cleanup:

- Red-line framing: this is compact/context lifecycle work. It does not complete
  Codex CLI parity, but it closes a production gap where successful compaction
  could leave stale compact-warning/microcompact state and manual `/compact`
  could continue without a model-context boundary marker.
- Added a shared compact-boundary helper that prepends a metadata boundary and
  recomputes the true post-compact token count including that boundary.
- Successful auto-compact now clears warning suppression at attempt start and
  runs post-compact cleanup after both session-memory and normal compact
  success paths.
- Manual `/compact` now writes a `trigger=manual` compact boundary into
  `engine_history`, keeps the user-visible semantic compact count, and runs the
  same post-compact cleanup path as automatic compaction.
- Added unit/keybinding/static smoke coverage for boundary metadata, cleanup
  lifecycle effects, and run-all registration.

2026-05-23 Stream-json compact bridge:

- Red-line framing: this is a protocol-to-agent bridge for context management.
  It still does not complete the full Codex CLI harness or all stream-json
  control subtypes, but it turns the existing compact request buffer from a
  dead stub into an executable path.
- `StructuredIO` now recognizes inbound `compact_conversation` control requests,
  validates the currently supported `manual` mode, enqueues the request into the
  single-slot pending compact buffer, and emits a structured `queued` or
  `blocked` control response.
- The main dialogue loop now checks that pending buffer at the start of each
  turn, supports dry-run inspection without mutating context, executes real
  compaction for queued manual requests, prepends the shared compact boundary,
  emits `SdkMessage::CompactBoundary`, and runs post-compact cleanup.
- Added focused tests for handler enqueue/block behavior and dialogue safe-point
  execution/dry-run behavior, plus static smoke coverage and run-all
  registration.

2026-05-23 Stream-json runtime bridge:

- Red-line framing: this is harness/rendering mechanism work. It still does
  not complete the full Codex CLI parity target, but it moves stream-json from
  a final-text compatibility shim toward a real event stream.
- `--emit stream-json` oneshot/stdin/input-file routes now use a dedicated
  `run_oneshot_stream_json` path that emits each `SdkMessage` as NDJSON instead
  of collecting only assistant text and printing one terminal object.
- The stream-json runtime creates `StructuredIO`, drains its outbound queue
  through a single stdout writer, and reads stdin lines through
  `StructuredIO::process_line` while the agent turn is running. That makes
  control requests such as `compact_conversation` reachable in the real CLI
  path, not only in unit tests.
- The normal text/json oneshot path and stream-json path now share prompt,
  hook, skill, memory, system-prompt, and tool-registry setup through
  `build_oneshot_prompt_params`, reducing divergence between runtime modes.
- Added W128 static smoke coverage and run-all registration for the runtime
  route, `StructuredIO` stdin/outbound bridge, and SDK-message emission path.

2026-05-23 Remote IO structured bridge:

- Red-line framing: this is remote/SDK harness plumbing. It does not complete
  the whole production harness, but it closes a concrete gap where remote
  transports could connect while silently dropping protocol input/output.
- `StructuredIO` is now cloneable over its shared channels/state, so transport
  callbacks and drain tasks can share the same pending request map, input-close
  state, and outbound queue without copying protocol state.
- `RemoteIO` now feeds transport inbound data through a single ordered channel
  into `StructuredIO::process_line`, preserving control-response handling,
  environment-variable updates, and compact-control side effects.
- `RemoteIO` now drains `StructuredIO` outbound messages back to CCR when
  enabled, or to the underlying transport otherwise, so generated
  control_request/control_response/cancel messages are no longer stranded in
  the local queue.
- Transport close now marks `StructuredIO` input closed, failing pending
  permission/control waits safely instead of leaving them hanging.
- Added W129 static smoke coverage and a unit regression for NDJSON/single-JSON
  transport line splitting.

2026-05-23 Stream-json slash runtime bridge:

- Red-line framing: this is stream-json protocol plumbing, not the full
  slash-command product surface. It makes the runtime consume slash command
  control requests through the same structured IO path used by local and remote
  transports.
- `StructuredIO` now intercepts inbound `slash_command` control requests before
  generic control-request deserialization, normalizes `/command` names and args,
  and emits `slash_command_result` or explicit error control responses.
- The wired stream-json subset is intentionally narrow and real: `/help`,
  `/capabilities`, `/status`, read-only `/permissions`, and `/compact`
  `plan|preview|status|run|--confirm`. Known commands without attached runtime
  state fail with `unwired_slash_command` instead of fabricated data.
- `/compact plan` and `/compact preview` enqueue the existing compact bridge in
  dry-run mode; `/compact run` and `/compact --confirm` enqueue a real manual
  compact request through the same pending compact buffer. Custom instruction
  args are preserved.
- The slash capability manifest now marks `/compact` as an explicit
  subcommand-backed stream-json command with accepted args.
- Added W130 static smoke coverage plus focused StructuredIO unit coverage for
  help, permissions, compact-plan enqueue, and unsupported command errors.

2026-05-23 Stream-json permission mode bridge:

- Red-line framing: this is a concrete execution bridge for permission-mode
  slash commands. It does not finish the entire permission/rule product, but it
  moves `/permissions` from read-only stream-json metadata into the actual
  agent tool-permission decision path.
- `/permissions mode <mode>`, `/permissions <mode>`, and the
  `/permission-mode` or `/approval-mode` aliases now normalize common UI/SDK
  spellings such as `plan`, `accept-edits`, `full auto`, `dontAsk`, `auto`, and
  `yolo`, then update the session permission mode env.
- The non-interactive oneshot and stream-json prompt builders now read that
  session permission mode instead of hard-coding `default`, and the dialogue
  tool-permission check reads the current session override before deciding
  whether mutating tools should run, prompt, or fail closed.
- The stream-json `/permissions` response still redacts rule patterns and only
  reports rule counts, but it now marks mode mutation as supported while keeping
  rule mutation unsupported in this protocol bridge.
- Added W131 static smoke coverage and focused unit coverage for permission
  mode env updates and agent-side permission mode override behavior.

2026-05-23 Stream-json runtime status snapshot:

- Red-line framing: this is observability for the execution bridge, not a
  replacement for the full harness dashboard. It gives stream-json clients a
  stable `/status` payload that can prove whether the agent loop is idle,
  running, completed, or failed without scraping terminal text.
- Added a process-local agent runtime status module that records dialogue
  start/finish counters, active session/model, last terminal reason, and last
  error. The data is read-only and never drives execution decisions.
- `initiate_dialogue` now records runtime status around the real agent loop,
  while preserving existing `SdkMessage::SystemInit` and `SdkMessage::Result`
  behavior.
- Stream-json `/status` now keeps its legacy queue fields and adds nested
  `queues`, `compact`, `permissions`, `slash`, and `agent` sections. Pending
  compact status now includes request id, dry-run state, age, timeout, and
  custom-instruction presence.
- Added W132 static smoke coverage plus focused unit coverage for runtime
  status counters and slash `/status` payload shape.

2026-05-23 Runtime tool and permission status:

- Red-line framing: this adds execution observability, not a policy engine
  rewrite. The goal is to let stream-json harnesses prove tool and permission
  behavior without parsing terminal prose.
- Runtime status now counts tool calls started/completed/failed/denied and
  records the last tool name/status/timestamps.
- Runtime status now counts permission decisions by source:
  `permission_mode`, `permission_gate`, and `not_required`, plus allow,
  allow-always, and deny outcomes.
- The agent tool loop records these counters at the existing permission gate
  and execution-result points, preserving the current allow/deny/error control
  flow.
- Stream-json `/status` exposes the new fields through the existing nested
  `runtime.agent` snapshot.
- Added W133 static smoke coverage plus focused runtime-status and slash
  status assertions.

2026-05-23 Stream-json render event bridge:

- Red-line framing: this is a protocol/rendering bridge, not the final terminal
  renderer. It gives Codex-CLI-like clients stable render semantics while the
  existing raw SDK event stream remains available for compatibility.
- `--emit stream-json` now emits a `render_event` NDJSON object for each
  semantic render event derived from the corresponding `SdkMessage`.
- The bridge reuses the TUI `render_events_for_sdk_message` classifier and
  serializes kind, scope, stage, refresh policy, history policy, and payload in
  a viewport-independent schema.
- `/status` now advertises the render event stream metadata, including schema
  version, event type, raw-SDK-message compatibility, and throttle interval.
- Added W134 static smoke coverage plus focused serialization tests for
  throttled streaming text and final-summary result events.

2026-05-23 Stream-json render event ordering:

- Red-line framing: this is a render protocol hardening step, not visual TUI
  polish. It makes the event stream stable enough for replay, de-duplication,
  and flicker-resistant render scheduling.
- Added a stateful stream-json render-event emitter that assigns monotonic
  event sequences across the whole stream-json turn instead of restarting
  sequence metadata for each SDK message.
- Each `render_event` now includes source-message ordering metadata:
  `sourceMessageSequence`, `sourceMessageType`, and `eventIndexInSource`, plus
  `emittedAtMs`.
- `/status` advertises the ordering guarantees under `runtime.render.ordering`,
  and the render event schema version is bumped to 2.
- Added W135 static smoke coverage plus focused tests for monotonic sequencing
  across a streaming delta followed by a final result.

2026-05-23 Stream-json render snapshot reducer:

- Red-line framing: this is the first render consumer contract for stream-json,
  not the final full-screen renderer. It gives clients a state snapshot they
  can draw from without reimplementing the event reducer.
- Added `StreamJsonRenderStreamState`, which applies ordered `render_event`
  values, ignores stale/duplicate sequences, and tracks current stage, scope,
  activity, refresh pressure, history policy counts, and terminal status.
- The stream-json runtime now emits a `render_snapshot` item after the render
  events derived from each SDK message, preserving raw SDK messages and
  per-event replay while also exposing direct current render state.
- Snapshot history metadata includes scroll-stability hints such as
  `preserveScrollOnUpdateActive`, matching the requirement to avoid scroll
  jumps while streaming active content.
- `/status` advertises snapshot support under `runtime.render`, and W136 adds
  static smoke plus focused reducer/snapshot tests.

2026-05-23 Stream-json terminal frame contract:

- Red-line framing: this still keeps raw SDK messages, render events, and
  render snapshots intact. It adds the first line-oriented draw contract for
  terminal clients that want Codex-CLI-like rendering without rebuilding their
  own reducer and screen scheduler.
- `--emit stream-json` now emits a `render_frame` item after each
  `render_snapshot`. The frame carries status, active, and footer regions,
  plus a stable frame hash, refresh policy, and scroll hints.
- The draw contract explicitly prefers region patching over whole-screen
  replacement, which is the important mechanism for avoiding flicker and scroll
  jumps during streaming output.
- `/status` advertises frame stream support and the region-patching contract
  under `runtime.render.draw_contract`.
- Added W137 static smoke coverage plus focused tests that verify the frame is
  line-oriented, carries patch-region metadata, and follows the emitted
  snapshot.

2026-05-23 Stream-json render frame delta:

- Red-line framing: this is still protocol/runtime work, not final terminal
  polish. It makes the frame stream cheaper and less flickery by telling
  clients exactly which stable regions changed.
- The stream-json render emitter now retains the previous frame fingerprint
  inside a turn and emits per-region `regionHash` values plus
  `changedRegionIds` and `unchangedRegionIds`.
- `render_frame.draw` now exposes `skipIfFrameHashUnchanged`, and
  `render_frame.changes` exposes `skipDrawWhenUnchanged`, so clients can avoid
  repainting when a semantic event does not change visible terminal output.
- Frame hashes are based on region hashes rather than the event sequence, so
  repeated visible frames remain comparable across stream updates.
- `/status` advertises the frame delta contract under
  `runtime.render.draw_contract`, and W138 adds smoke plus a focused test for
  unchanged-region skip-draw behavior.

2026-05-23 Stream-json terminal patch renderer:

- Red-line framing: this is the first terminal-side consumer of the frame
  stream, not a replacement for the interactive ratatui UI. It translates
  stable `render_frame` objects into direct region patch operations that a
  Codex-CLI-like terminal scheduler can apply.
- Added `StreamJsonTerminalPatchRenderer`, which tracks the last applied frame
  hash, skips duplicate frames, and emits `replace_region` operations only for
  changed regions instead of asking clients to repaint the whole screen.
- Patch payloads carry flush, cursor, scroll, and safety metadata:
  `preservePrompt`, `restoreAfterDraw`, stable scroll hints, ANSI-safe lines,
  control-character stripping, and a bounded max line width.
- `--emit stream-json` now emits `render_patch` after `render_frame`, while
  preserving raw SDK messages, render events, snapshots, and frames.
- `/status` advertises patch stream support and draw-contract capabilities
  such as patch operations, duplicate-frame skipping, ANSI-safe lines, and
  prompt cursor preservation. W139 adds static smoke plus focused renderer and
  bridge tests.

2026-05-23 Stream-json terminal draw plan:

- Red-line framing: this is terminal scheduler preparation, not a rewrite of
  the interactive ratatui loop. It turns `render_patch` into an anchored draw
  plan that a terminal backend can execute without clearing the whole screen.
- Added `StreamJsonTerminalDrawScheduler`, which tracks previous region line
  counts and emits terminal operations such as `save_cursor`, anchored
  `move_to_row`, `clear_line`, `write_line`, and `restore_cursor`.
- Draw plans include region start rows, per-line width cells, stale trailing
  line clearing when a region shrinks, flush policy, throttle interval,
  superseded-frame dropping, and cursor/scroll preservation metadata.
- `--emit stream-json` now emits `render_draw_plan` after `render_patch`, while
  duplicate frame patches produce skipped draw plans with no terminal ops.
- `/status` advertises draw-plan stream support and draw-contract capabilities
  for anchored plans, cursor save/restore, stale-line clearing, and dropping
  superseded frames. W140 adds smoke plus focused scheduler and bridge tests.

2026-05-23 Stream-json terminal draw executor:

- Red-line framing: this is the real TTY backend for the stream-json draw plan,
  still separate from agent execution semantics and from NDJSON transport.
- Added `StreamJsonTerminalDrawExecutor`, `StreamJsonTerminalViewport`, and an
  execution report so a client can apply `render_draw_plan.terminalOps` to a
  crossterm writer with bounded, testable side effects.
- The executor uses synchronized terminal updates, absolute row moves,
  current-line clearing, line-width bounding, no newline writes, cursor
  save/restore, and stale/superseded sequence skipping; it never issues a full
  screen clear.
- `/status` now advertises the crossterm draw executor and its guardrails:
  synchronized update, absolute row moves, line-wrap guard, and no-newline
  writes. W141 adds smoke plus focused byte-level executor tests.

2026-05-23 Stream-json terminal draw runtime queue:

- Red-line framing: this is the scheduling layer between `render_draw_plan`
  events and the crossterm executor, not a change to agent semantics or the
  NDJSON transport.
- Added `StreamJsonTerminalDrawRuntime`, which accepts draw plans with an
  explicit millisecond clock, coalesces throttled active updates, keeps only
  the latest pending draw, and flushes on the throttle deadline.
- The runtime applies pending draws with the latest viewport, so terminal
  resize before a delayed flush uses the current column width and avoids
  wrap-induced scroll pollution.
- Manual-scroll hold support lets a frontend preserve the user's scroll
  position by queuing active-region updates while manual scrolling is active,
  then flushing the latest pending draw after the hold is released.
- `/status` advertises the runtime queue, coalescing, resize awareness, and
  manual-scroll hold contracts. W142 adds smoke plus focused runtime tests.

2026-05-23 Stream-json terminal frontend emit mode:

- Red-line framing: this connects the stream-json render pipeline to a real
  in-process terminal frontend without changing the NDJSON stream transport.
- Added `--emit terminal` for oneshot/stdin/input-file paths. It builds the
  same render event stream as `--emit stream-json`, consumes only
  `render_draw_plan` items in process, and applies them through
  `StreamJsonTerminalDrawRuntime`.
- The terminal frontend keeps a throttle deadline and flushes pending draw
  plans via `tokio::select!`, so delayed active-region updates do not wait
  until task completion when the agent is still streaming.
- The existing `--emit stream-json` path remains transport-pure: raw SDK
  messages and render objects continue as NDJSON, while ANSI drawing is only
  enabled through `--emit terminal`.
- `/status` advertises the terminal frontend emit mode and NDJSON/ANSI
  isolation contract. W143 adds smoke coverage for CLI routing, frontend
  wiring, and status metadata.

2026-05-23 Stream-json terminal frontend PTY contract:

- Red-line framing: this is executable PTY validation for the terminal frontend,
  not a change to task execution semantics or the NDJSON stream transport.
- Added a real PTY smoke for `--oneshot ... --emit terminal` against an isolated
  OpenAI-compatible SSE mock backend, reusing the same fixture isolation pattern
  as the interactive PTY render soaks.
- The smoke proves the terminal frontend reaches the mock chat completion,
  renders streaming head/tail markers through local terminal drawing, exits
  cleanly, and keeps output bounded.
- The render bridge now carries a bounded visible assistant-text tail into the
  active terminal region, so the terminal frontend renders real model content
  instead of only byte-count activity summaries.
- The PTY contract also asserts no alternate screen entry, no full-screen clear,
  no raw stream-json payload leakage, and presence of synchronized-update plus
  cursor save/restore control bytes. W144 is registered in `run_all_smoke.sh`.

2026-05-23 Stream-json terminal log isolation:

- Red-line framing: this keeps terminal-rendered UI bytes separate from
  diagnostics; it does not change agent execution semantics or the NDJSON
  transport.
- `--emit terminal` now uses the same per-pid file logging sink as the
  interactive TUI path, but without printing the log-path announcement into the
  terminal frontend. Text/json/stream-json oneshot paths keep their existing
  stderr log behavior.
- Added a W145 PTY smoke that reuses the W144 mock streaming backend and proves
  terminal emit still renders content while `INFO/WARN/ERROR mossen`,
  `init_sequence`, `setup`, `cli_ok`, and cleanup tracing tokens are absent
  from the PTY output.

2026-05-23 Stream-json terminal scrollback transcript commit:

- Red-line framing: this adds a real history commit path for completed terminal
  turns; it does not change agent execution semantics or the NDJSON stream
  transport.
- The render state now keeps a bounded assistant transcript separately from the
  active visible tail. On terminal completion, the frame includes a
  `transcript` region with `updateMode=append_scrollback`, so the final answer
  can be committed to normal terminal scrollback instead of remaining only in
  the patched activity area.
- The draw scheduler translates transcript regions into an
  `append_scrollback_block` terminal op, separate from anchored status/activity
  patching. The executor clears the stale bottom activity rows, writes bounded
  normal lines with CRLF, avoids full-screen clears and alt-screen mode, and
  appends the block once per frame hash.
- `/status` advertises the scrollback transcript contract. W146 adds a PTY
  smoke proving the committed transcript block contains the streamed head/tail
  markers while preserving the existing terminal safety invariants.

2026-05-23 Stream-json terminal approval widget:

- Red-line framing: this is a terminal rendering contract for approval prompts;
  it does not change permission execution semantics or the approval decision
  transport.
- `approval_requested` render events now become an independent `approval`
  frame region with `updateMode=replace_blocking`, instead of being rendered
  only as ordinary activity/log text.
- While approval is active, the regular active region is cleared so the
  decision point cannot be lost in streaming output. The frame marks
  `status.blocking=true`, declares `approvalRegionId`, and records the
  blocking region IDs.
- The draw scheduler carries blocking region metadata into the draw plan,
  giving terminal frontends an explicit approval widget boundary to style or
  route input against. `/status` advertises the approval widget contract, and
  W147 guards the source-level integration.

2026-05-23 Stream-json terminal command and diff widgets:

- Red-line framing: this adds independent terminal widget boundaries for
  command output and file diff summaries; it does not execute commands,
  expand logs, or implement the full diff viewer yet.
- Command start/output/finish events now update a `command` frame region that
  preserves command/cwd context, reports preview/hidden line counts, records
  full-log availability, and stays summary-only so long commands do not become
  a terminal log wall.
- File change and diff events now update a separate `diff` frame region that
  shows file count plus additions/deletions and marks the diff collapsed by
  default. This creates the frame boundary needed for later unified-diff
  expansion and word-level diff rendering.
- The draw scheduler gives command and diff widgets stable top-band placement
  below the status line, keeping them distinct from the bottom activity,
 approval, footer, and scrollback transcript regions. `/status` advertises the
  command/diff widget contracts, and W148 guards the source-level integration.

2026-05-23 Stream-json terminal error and final summary widgets:

- Red-line framing: this adds terminal rendering contracts for layered errors
  and end-of-turn summaries; it does not change agent retry behavior,
  permission decisions, command execution, or result generation.
- Error and API retry events now update an independent `error` frame region
  with high-level summary, key detail, retry state, and an explicit
  details-available signal. This keeps failures out of the generic activity
  stream and matches the PRD requirement to avoid raw exception walls.
- Final summary events now update an independent `final_summary` frame region
  that captures result, terminal reason, and any current command/diff/error
  summaries as a concise end-of-turn handoff. The region complements the
  scrollback transcript commit instead of replacing it.
- The draw scheduler reserves stable top-band placement for `error` and
  `final_summary` below status, command, and diff widgets. `/status`
  advertises the error/final-summary widget contracts, and W149 guards the
  source-level integration.

2026-05-23 Stream-json terminal viewport collision guard:

- Red-line framing: this is a terminal draw-safety guard. It does not change
  agent execution semantics, stream-json event extraction, or task history.
- Independent error and final-summary widgets now suppress their duplicate
  generic active-region rendering, so the same failure or summary is not drawn
  both in a top widget and in the bottom activity band.
- The terminal draw executor now computes reserved bottom rows from the current
  draw plan's bottom-anchored regions and clips top-anchored widget rows before
  they can collide with active, approval, or footer rows on short viewports.
  This keeps the anchored patch strategy safe under normal 24-row terminals
  and narrower windows.
- `/status` advertises the viewport collision guard and duplicate-active
  suppression contracts, and W150 guards the source-level integration.

2026-05-23 Stream-json terminal retired region clear:

- Red-line framing: this is a terminal draw cleanup mechanism. It does not
  change agent execution semantics, approval decisions, command execution, or
  slash-command routing.
- Render frame fingerprints now preserve each region's anchor, role,
  placement, hash, and line count. When a widget region disappears between
  frames, the next frame records `removedRegionIds` plus `retiredRegions`
  metadata instead of silently dropping the region from the delta.
- The terminal patch renderer converts retired regions into `clear_region`
  operations with `updateMode=clear_retired`. The draw scheduler reuses the
  existing stale-line clearing path to clear every previously occupied row,
  preventing approval/error/final-summary/widget remnants from staying visible
  after the independent widget resolves.
- `/status` advertises the retired-region clear contract, and W151 guards the
  frame, patch, draw-plan, and status metadata integration.

2026-05-23 Stream-json terminal manual-scroll critical bypass:

- Red-line framing: this is terminal render scheduling behavior only. It does
  not change manual-scroll state, permission decisions, agent execution, or
  command semantics.
- The draw runtime still preserves ordinary active-region updates while the
  user is manually scrolled away from the live tail, but it now bypasses that
  hold for critical draw plans: blocking approval regions, error widgets,
  final-summary widgets, scrollback commits, and retired-region cleanup.
- This prevents a Codex-style terminal from hiding a permission prompt, failure
 state, final answer commit, or stale-widget cleanup merely because the user
  is reading earlier output. Ordinary streaming activity remains protected
  from disturbing manual scroll.
- `/status` advertises the manual-scroll critical-bypass contract, and W152
  guards the source-level runtime behavior plus status metadata.

2026-05-23 Stream-json terminal scrollback soft wrap:

- Red-line framing: this improves terminal transcript rendering only. It does
  not change agent execution semantics, stream-json transport, or model
  results.
- Completed assistant transcript commits now soft-wrap long lines to the
  current terminal width before writing normal scrollback lines. This keeps
  history readable after the task ends without relying on terminal implicit
  wrapping or truncating the content with `...`.
- The executor still bounds every physical line to the viewport width, writes
  explicit CRLF line breaks, avoids alternate screen/full-screen clear, and
  reports wrapped scrollback line counts for diagnostics.
- `/status` advertises the terminal scrollback soft-wrap contract, and W153
  guards the executor/source-level behavior plus status metadata.

2026-05-23 Stream-json terminal dynamic top stack:

- Red-line framing: this improves terminal widget layout only. It does not
  change agent execution semantics, command execution, approval decisions, or
  stream-json transport.
- Top-anchored terminal widgets now use a compact dynamic stack generated from
  the current frame's top-region order and line counts, instead of fixed gaps
  such as command at `top+1`, diff at `top+7`, error at `top+11`, and final
  summary at `top+16`.
- The patch renderer tracks previous top-region layout, so an unchanged widget
  is redrawn when its row offset changes because a widget above it appeared,
  disappeared, or resized. Retired top widgets clear from their previous
  dynamic rows.
- The draw plan advertises `topLayoutMode=dynamic_stack` and
 `topLayoutCompactsGaps=true`, keeping short viewports useful while preserving
  the bottom-region collision guard. `/status` advertises the dynamic top-stack
  contract, and W154 guards the source-level integration.

2026-05-23 Stream-json terminal frontend event pump:

- Red-line framing: this wires terminal frontend input and resize scheduling
  only. It does not change agent execution semantics, NDJSON transport,
  permission decisions, or task history.
- The `--emit terminal` path now starts a lightweight crossterm event pump only
  when stdin and stdout are real TTYs. Resize events refresh the draw runtime's
  viewport and flush pending draws through the existing scheduler.
- PageUp, Home, Up, and mouse wheel up put the draw runtime into manual-scroll
  hold. PageDown, End, Down, Ctrl-L, and mouse wheel down restore tail-follow
  behavior and flush any preserved pending draw.
- The terminal emit path still does not enter alternate screen, enable mouse
  capture, or do full-screen clears. It remains a normal scrollback-friendly
  terminal stream while giving the draw runtime enough frontend events to avoid
  stale viewport state and stuck manual-scroll behavior. `/status` advertises
  the frontend event-pump contracts, and W155 guards the source-level
  integration.

2026-05-23 Stream-json terminal semantic colors:

- Red-line framing: this improves terminal readability only. It does not
  change region layout, refresh cadence, agent execution semantics, command
  execution, approval decisions, or stream-json transport.
- Patch/draw planning now attaches a `semanticStyle` to each drawn terminal
  line based on the region role and line meaning: status/accent, activity/info,
  approval/warning, command success/failure, diff additions/deletions,
  layered errors, final summaries, transcript headers, and muted footers.
- The crossterm executor applies foreground color only for known semantic
  styles and resets color immediately after each styled line. Unknown styles
  fall back to plain text, preserving ANSI-safe bounded-line behavior and
  avoiding style bleed into prompts or shell output.
- Draw plans advertise `semanticColors`, `plainTextFallback`, and
  `resetAfterLine`; execution reports count styled lines and resets. `/status`
  advertises the semantic-color contracts, and W156 guards the source-level
  integration.

2026-05-23 Stream-json terminal command and diff previews:

- Red-line framing: this enriches terminal widget rendering only. It does not
  execute commands, expand a full-screen log/diff viewer, alter agent/task
  semantics, or change permission behavior.
- Command widgets now carry sanitized bounded preview lines alongside the
  existing shown/hidden/full-log metadata. The terminal frame can show useful
  stdout/stderr tail context without turning command output into a log wall.
- Diff widgets now carry bounded per-file summary lines, omitted-file counts,
  and optional unified-diff hunk previews while still remaining collapsed by
  default. This gives the terminal enough engineering context before a later
  expandable viewer exists.
- `/status` advertises the command-preview and collapsed-diff-preview
  contracts, and W157 guards the source-level widget, status, smoke, and phase
  integration.

2026-05-23 Stream-json terminal widget expansion controls:

- Red-line framing: this adds terminal-render expansion controls only. It does
  not execute commands, fetch full logs, open a full-screen modal, alter
  approval decisions, or change agent/task semantics.
- Command and diff widgets now preserve an `expanded` state and carry separate
  bounded expanded-preview budgets. Collapsed rendering stays compact, while an
  expanded widget can show more stdout/stderr lines, file rows, and diff hunk
  lines without becoming an unbounded log wall.
- The local terminal frontend maps `o` to command-output expansion and `d` to
  diff expansion. The control path reuses the existing snapshot/frame/patch/draw
  pipeline and forces an immediate anchored-region redraw instead of repainting
  the whole screen.
- `/status` advertises widget expansion controls, command/diff expand-collapse,
 expanded preview budgets, and immediate-redraw contracts. W158 guards the
  state, frontend event mapping, status metadata, smoke, and phase integration.

2026-05-23 Stream-json terminal interaction footer hints:

- Red-line framing: this is terminal-render affordance work only. It does not
  execute commands, approve or reject tool calls, mutate slash-command routing,
  or change agent/task semantics.
- Terminal snapshots and frames now expose contextual interaction metadata for
  scroll controls, command expansion, diff expansion, and approval-decision
  hints. The metadata is derived from the current render state rather than a
  separate UI-side guess.
- The footer line now surfaces bounded key hints such as PgUp hold, End live,
  Ctrl-L live, and contextual `o`/`d` expand-collapse controls. Finished turns
  keep the completion footer instead of advertising stale controls.
- Approval widgets now show product-shaped decision labels: Approve once,
  Reject, and Always allow. `/status` advertises footer keymap and contextual
  interaction contracts, and W159 guards the source-level integration.

2026-05-23 Stream-json terminal approval action focus:

- Red-line framing: this is approval-render state-machine work only. It does
  not submit approval decisions, mutate permission rules, edit commands, or
  change agent/task semantics.
- Approval widgets now expose an action model with Approve once, Reject, Edit
  command, and Always allow actions. Each action carries a stable id, label,
  key hint, focus state, and semantic flags such as destructive,
  requires-edit, and session-scoped.
- The local terminal frontend maps Tab/Right and Shift-Tab/Left into approval
  focus movement. These controls reuse the same snapshot/frame/patch/draw
  pipeline and force an immediate anchored redraw while preserving the
  blocking approval region.
- `/status` advertises approval action-model, focus-navigation, edit-command
  action, and session-action contracts. W160 guards the render state,
  frontend key mapping, status metadata, smoke, and phase integration.

2026-05-23 Stream-json terminal approval action activation intent:

- Red-line framing: this is approval-render interaction intent work only. It
  does not approve or reject tool calls, mutate permission rules, edit command
  text, or change agent/task semantics.
- Approval action models now advertise Enter selection plus y/n/e/a shortcut
  activation keys. Activating an action records a bounded pending intent with
  action id, label, key, source, sequence, and explicit `submitted=false`
  / `renderOnly=true` metadata for the future decision bridge.
- Approval widgets show the select affordance and, after activation, a concise
  selected-action line while keeping the approval region blocking until the
  real permission decision path resolves it.
- The local terminal frontend maps Enter and y/n/e/a into the same snapshot,
  frame, patch, and draw-plan pipeline used by focus changes, preserving
  anchored redraws instead of repainting the whole terminal.
- `/status` advertises approval activation, Enter-select, shortcut-action, and
  pending-intent contracts. W161 guards the render state, frontend key mapping,
  status metadata, smoke registration, and phase integration.

2026-05-23 Stream-json terminal approval decision bridge:

- Red-line framing: this connects terminal approval actions to the existing
  SDK `can_use_tool` response path only for one-shot allow and explicit reject.
  It does not edit command text, install session permission rules, or alter
  task execution semantics.
- StructuredIO now exposes a fail-closed bridge from the single pending
  permission request to `approve_once` / `reject` control responses. Unsupported
  `edit_command` and `approve_for_session` actions remain unresolved until
  their dedicated edit/rule bridges exist.
- Injected local approval responses now use the same resolved-permission
  callback path as external `control_response` input, preserving lifecycle
  parity and still emitting a cancel message to the SDK consumer.
- `/status` advertises the decision bridge, approve-once bridge, reject bridge,
  and fail-closed ambiguity guard. W162 guards the source-level bridge,
  callback parity, status metadata, smoke registration, and phase integration.

2026-05-23 Stream-json terminal approval interactive gate bridge:

- Red-line framing: this connects local terminal approval key activation to the
  agent permission gate only. It does not implement command editing, durable
  permission-rule persistence, or remote SDK permission prompting.
- Terminal-render oneshot now installs an `InteractiveGate` when attached to a
  real TTY. Permission requests from the agent produce the approval region in
  the same render snapshot/frame/patch/draw pipeline as other terminal widgets.
- Activating Approve once, Reject, or Always allow submits `Allow`, `Deny`, or
  `AllowAlways` back through the pending gate responder so tool execution can
  continue or fail closed. Edit command stays pending and surfaces an
  unsupported bridge status rather than approving anything.
- Approval intents now record bridge status after submission attempts, changing
  from render-only pending metadata to submitted / unsupported / no-pending
  state in the terminal action model.
- `/status` advertises the interactive gate bridge, local decision submit,
  allow-always session bridge, and edit-command fail-closed contracts. W163
 guards the source-level gate wiring, render bridge status metadata, status
 metadata, smoke registration, and phase integration.

2026-05-23 Stream-json terminal approval input preview:

- Red-line framing: this improves terminal approval rendering only. It does
  not implement command editing, mutate tool inputs, persist permission rules,
  or alter agent execution semantics.
- Terminal-render permission requests now carry the pending tool input into the
  render bridge, allowing the approval widget to show bounded contextual
  preview lines before the user approves or rejects a tool call.
- The preview is tool-aware: Bash/Execute show command, cwd, description, and
  timeout; file tools show paths and bounded edit/write summaries; generic and
  MCP-like tools fall back to sorted top-level input summaries without dumping
  large JSON bodies into the blocking region.
- Snapshots and frames expose `inputPreview` metadata with explicit bounded /
  redacted flags, and the approval region renders the preview above the action
  selector so the user can make the permission decision without scrollback
  hunting.
- `/status` advertises approval input-preview and bounded-preview contracts.
  W164 guards the source-level bridge, render metadata, status metadata, smoke
  registration, and phase integration.

2026-05-23 Stream-json terminal approval session rule bridge:

- Red-line framing: this closes the stream-json terminal approval action
  bridge for session-scoped allow rules. It still does not implement command
  editing, persistent permission writes, or task execution changes.
- `approve_for_session` now resolves the single pending SDK `can_use_tool`
  request with an `allow` decision carrying `updatedPermissions` add-rule
  updates scoped to `session`. Existing permission suggestions are normalized
  to session scope when available; otherwise shell commands fall back to an
  exact command rule and other tools fall back to a session-scoped tool rule.
- The bridge remains fail-closed for ambiguous pending requests and unsupported
  edit-command activation, preserving the approval region's safety model while
  making the visible Always allow action real for stream-json clients.
- `/status` advertises session-rule bridge and session-rule update contracts.
  W165 guards the source-level helper, response metadata, status metadata,
  smoke registration, and phase integration.

2026-05-23 Terminal interactive gate scoped session rule:

- Red-line framing: this hardens the local terminal approval gate after the
  stream-json session-rule bridge. It does not add command editing, persistent
  permission writes, or new tool execution semantics.
- The local `InteractiveGate` no longer treats Always allow as a tool-wide
  blanket for shell tools. Bash, PowerShell, and Execute now pre-approve by
  exact command rule within the current session, so approving `cargo test -q`
  does not silently approve a later `cargo check -q` or unrelated shell call.
- Tools without a command field still fall back to a session-scoped tool rule,
  preserving the existing non-shell behavior while avoiding the highest-risk
  broad Bash approval path.
- `/status` advertises scoped allow-always and exact-command-rule contracts.
  W166 guards the gate helper, async gate behavior, status metadata, smoke
  registration, and phase integration.

2026-05-23 Stream-json terminal approval submitted region retirement:

- Red-line framing: this improves terminal-render state consistency after a
  local approval action is submitted. It does not change permission decisions,
  command editing, persistent permission writes, or tool execution semantics.
- Successful terminal approval submissions now retire the blocking approval
  region immediately and replace it with a non-blocking active status line such
  as `approval submitted: Approve once`. Unsupported edit-command or failed
  submits still keep the approval region blocking and fail closed.
- The frame delta now exposes the approval region as retired after a successful
  submit, so the draw scheduler clears the old blocking region instead of
  leaving stale approval controls on screen until the next agent event.
- `/status` advertises submitted-approval non-blocking behavior and blocking
  region retirement. W167 guards the render state transition, frame retirement,
  status metadata, smoke registration, and phase integration.

2026-05-23 Stream-json terminal edit-command approval bridge:

- Red-line framing: this connects the stream-json approval-action protocol to
  SDK updated-input decisions. It does not add a local terminal line editor,
  persist permission rules, or change tool execution semantics.
- `control_request` messages with subtype `terminal_approval_action` can now
  submit `edit_command` with either `updatedInput` or a shell `command`. The
  bridge resolves the single pending `can_use_tool` request with
  `behavior=allow`, camelCase `updatedInput`, and `userModified=true`.
- Missing edited input, empty commands, non-shell string edits, unsupported
  actions, no pending requests, and ambiguous pending requests remain
  fail-closed.
- `/status` advertises the terminal approval-action control request, the
  edit-command bridge, and updated-input support. W168 guards the source-level
  bridge, control-request response, status metadata, smoke registration, and
  phase integration.

2026-05-23 Local terminal edit-command input bridge:

- Red-line framing: this upgrades the local terminal-render approval path from
  render-only `Edit command` intent to an executable edit-command input flow.
  It is still intentionally scoped to shell-command tools and does not persist
  permission rules for edited commands.
- The terminal event pump now switches into command-input capture while an edit
  is active. Printable characters update the command buffer, Backspace edits
  it, Enter submits it, and Esc cancels. The approval region stays blocking
  while editing, so accidental partial input cannot approve the command.
- `PermissionDecision` can now carry `AllowWithUpdatedInput`, and the dialogue
  execution loop replaces the original tool input with that updated input
  before dispatching the tool. This makes the local TTY edit path affect the
  command that actually runs, matching the stream-json updated-input bridge.
- Empty edited commands stay pending and fail closed. Non-shell tools still
  cannot use the string command editor. `/status` advertises local edit-command
  input, local edit-command submit, and updated-input execution support. W169
  guards the local input bridge, render inline editor, execution handoff,
  status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render input capture lifecycle:

- Red-line framing: this hardens the local terminal-render frontend input
  mechanism. It does not change agent planning, task execution semantics,
  permission policy, or slash-command business logic.
- The terminal-render oneshot path now stops the pre-main early-input reader
  before starting its own crossterm event pump, preventing two independent
  stdin readers from splitting approval, scroll, and edit-command keystrokes.
- Local terminal render now enables raw mode and mouse capture with an RAII
  guard, keeps the renderer scrollback-friendly by avoiding alternate-screen
  mode, and restores terminal state before writing the final newline.
- Permission prompts are only connected to the interactive gate when the local
  terminal event pump is actually available. If input capture cannot start, the
  path fails closed instead of showing a blocking approval widget that cannot
  receive user input.
- `/status` advertises raw-mode capture, mouse capture, and early-input
  isolation. W170 guards input-capture setup, event-pump gating, terminal
  restore ordering, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render Ctrl-C interrupt bridge:

- Red-line framing: this adds a local terminal-render interrupt path. It does
  not claim full process supervision parity, shell child kill parity, or
  multi-turn TUI cancellation completion.
- `PromptParams` and `SubmitOptions` can now carry a caller-owned
  `CancellationToken`, and `submit_prompt` forwards it into the
  `SessionOrchestrator` turn. This lets the local terminal frontend cancel the
  same token watched by API retry, streaming, hook, and tool-loop checks.
- The terminal event pump maps Ctrl-C to a dedicated interrupt event even while
  edit-command capture is active. The handler cancels the turn token, exits
  manual-scroll hold, clears edit capture, and emits a terminal `cancelled`
  result so the anchored regions do not stay stale.
- If Ctrl-C happens while an approval request is blocking inside
  `InteractiveGate`, the approval bridge resolves the pending request with
  `Deny` to unblock the dialogue loop; the cancellation token then prevents
  continued work on the next cancellation check.
- `/status` advertises Ctrl-C interrupt, turn cancellation, and approval-unblock
  support. W171 guards cancellation-token plumbing, key mapping, approval
  unblock behavior, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render key-release filter:

- Red-line framing: this hardens local terminal-render frontend input
  normalization. It does not change approval policy, command execution,
  renderer layout, or slash-command business logic.
- Crossterm can surface `KeyEventKind::Release` when keyboard-enhancement mode
  is active. The terminal-render event mapper now ignores release events before
  approval shortcuts, edit-command input, widget toggles, or manual-scroll
  controls can see them. This prevents one physical keypress from causing a
  second action on key-up.
- `KeyEventKind::Press` remains the normal path, and `Repeat` stays available
  for held scroll/edit keys. `/status` advertises the key-release filter, and
  W172 guards the event normalization, repeat behavior, status metadata, smoke
  registration, and phase integration.

2026-05-23 Terminal-render edit-command bracketed paste:

- Red-line framing: this hardens the local edit-command input path. It does
  not add general prompt paste handling, image paste handling, or persistent
  permission-rule changes for edited commands.
- Local terminal-render input capture now enables bracketed paste with the
  same RAII lifecycle as raw mode and mouse capture. Terminal state is restored
  with `DisableBracketedPaste` before the final newline.
- `Event::Paste(String)` is ignored in normal approval focus mode so pasted
  text cannot trigger approval shortcuts. While edit-command capture is active,
  paste events append normalized text to the command buffer in one render
  update, preserving newline/tab command structure while dropping terminal
  control bytes.
- `/status` advertises bracketed-paste capture and edit-command paste support.
  W173 guards the capture lifecycle, event mapping, normalized buffer append,
  status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render interrupt tool cancellation:

- Red-line framing: this hardens the local terminal-render interrupt path from
  the UI cancellation token into the active tool await boundary. It does not
  add process-group supervision, background task recall, or new permission
  policy.
- `ToolRegistry` now exposes a cancellation-aware execution boundary that
  races tool execution against the current turn token. When Ctrl-C cancels the
  token, the in-flight tool future is dropped immediately instead of waiting
  for the tool timeout. Foreground Bash commands already own their child
  process inside that future with `kill_on_drop(true)`, so this gives the local
  terminal-render path a real execution-side stop for the active command.
- The dialogue loop now uses that cancellation-aware boundary for registered
  tools, and also races MCP tool dispatch against the same token. Cancellation
  records the active tool as `cancelled` and terminates the turn with
  `AbortedTools` instead of appending a synthetic tool result and continuing
  the loop.
- `/status` advertises interrupt-driven tool execution cancellation. W174
  guards the registry cancellation boundary, future-drop behavior, dialogue
  handoff, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render shell process-group cancellation:

- Red-line framing: this hardens the active Bash foreground command lifecycle
  used by the terminal-render interrupt path. It does not change permission
  policy, slash-command routing, background task recall, or inactive Bash
  mirror modules.
- The registered Bash tool now starts foreground commands in a dedicated Unix
  process group. A small RAII guard terminates that group when the foreground
  command future is dropped before normal completion, which is the path reached
  by terminal-render Ctrl-C through the W174 cancellation-aware tool boundary.
- Timeout now also terminates the foreground command process group instead of
  relying on `kill_on_drop(true)` for only the shell child. A focused runtime
  test launches `sleep` as a shell child, forces a timeout, and verifies the
  child pid does not survive.
- `/status` advertises shell process-group termination. W175 guards the active
  registered Bash path, process-group setup, group signal termination, runtime
  child cleanup behavior, smoke registration, and phase integration.

2026-05-23 Terminal-render background Bash task lifecycle:

- Red-line framing: this hardens explicit `run_in_background=true` Bash usage
  so terminal rendering can keep long-running commands out of the live log wall
  while preserving a real task handle for later output/stop operations. It does
  not add streaming background output, persistence across process restarts, or
  new permission policy.
- The registered Bash tool now creates a task-store record for background
  commands, returns `backgroundTaskId`, keeps the child future alive in a Tokio
  task, captures stdout/stderr into the task record, and writes terminal status,
  exit code, timeout metadata, and truncated output after completion.
- Background Bash processes are registered by task id and OS process group, so
  `TaskStop` can terminate the running shell group instead of only returning a
  synthetic success response. `TaskOutput` now honors its blocking poll contract
  and treats completed, failed, cancelled, and deleted tasks as ready.
- `/status` advertises background Bash task lifecycle support. W176 guards the
  Bash return id, task-store process registration, output/stop bridges, runtime
  output capture, child cleanup behavior, smoke registration, and phase
  integration.

2026-05-23 Terminal-render background task render summary:

- Red-line framing: this connects background Bash task lifecycle data into the
  stream-json terminal renderer. It does not add cross-process task persistence,
  OS notifications, or a live background-output tailer.
- The stream-json render bridge now recognizes `backgroundTaskId` returned by
  Bash and preserves it in the command widget with bounded preview lines. The
  quick foreground Bash call is rendered as a background-task start summary
  instead of an opaque `exit unknown` command result.
- `TaskOutput` summaries for `background_shell` tasks now produce supplemental
  task-scoped command output/finish render events. The frame carries task id,
  task status, exit code, and a bounded output preview, so long task output does
  not become a terminal log wall.
- `/status` advertises background task render summaries. W177 guards structured
  task-id enrichment, bounded preview extraction, TaskOutput-to-command-summary
  bridging, smoke registration, and phase integration.

2026-05-23 Terminal-render background task status panel:

- Red-line framing: this adds an independent, bounded background-task status
  region to the terminal frame. It does not add proactive OS notifications,
  persistent task storage, or live background-output streaming.
- Render-event extraction now emits `BackgroundTaskUpdated` for Bash
  `backgroundTaskId`, `TaskOutput` background shell records, and `TaskStop`
  results. This gives the stream-json renderer a task update signal separate
  from the active foreground command widget.
- The stream-json terminal state keeps a small background-task registry and
  renders a `background_tasks` region with task id, status, command when known,
  output line counts, and exit code. The region stays visible after a foreground
  command replaces the command widget, and it remains line-bounded to avoid log
  walls and viewport churn.
- `/status` advertises the background task status panel. W178 guards render
  event extraction, task registry updates, independent frame region metadata,
  no-log-wall task summaries, smoke registration, and phase integration.

2026-05-23 Terminal-render critical region top priority:

- Red-line framing: this tightens stream-json terminal region ordering only.
  It does not change agent execution, approval decisions, task lifecycle, or
  command/diff rendering contents.
- Critical top widgets now occupy the dynamic top stack before ordinary
  command, background-task, and diff widgets: error first, then final summary,
  then the lower-priority operational summaries. This keeps failures and final
  outcomes visible on short terminals even when command, diff, and background
  task panels are also present.
- `/status` advertises critical top-region priority. W179 guards the region
  ordering, final-summary row placement after an error, status metadata, smoke
  registration, and phase integration.

2026-05-23 Terminal-render independent plan status panel:

- Red-line framing: this promotes `plan_updated` from a transient active-line
  summary into a first-class terminal region. It does not change plan tools,
  slash-command semantics, agent scheduling, or task execution.
- The stream-json terminal state now keeps a bounded plan widget with total,
  completed, active, pending, and blocked counts plus the active step. The
  frame exposes a `plan` top region with `replace_plan` update mode, positioned
  ahead of command/background/diff summaries but behind critical error/final
  result regions.
- `/status` advertises the independent plan status panel. W180 guards the
  state model, frame metadata, bounded plan lines, dynamic top-stack ordering,
  semantic styling, smoke registration, and phase integration.

2026-05-23 Terminal-render file-change and diff split:

- Red-line framing: this separates the file-change summary object from the diff
  review object in the stream-json terminal renderer. It does not change edit
  execution, diff generation, or approval policy.
- `file_change_summary` now feeds a persistent `file_changes` top region with
  bounded file previews and line counts. `diff_available` continues to feed the
  `diff` region, so unified diff preview can appear without overwriting the
  file-change summary that explains what files changed.
- `/status` advertises the independent file-change summary region and the
  file-change/diff separation. W181 guards state separation, frame metadata,
  dynamic top-stack ordering, semantic styling, smoke registration, and phase
  integration.

2026-05-23 Terminal-render file-change expansion controls:

- Red-line framing: this makes the file-change summary region locally
  expandable as a renderer concern only. It does not change edit execution,
  diff generation, approval decisions, or task semantics.
- The stream-json terminal state now treats `file_changes` as a real widget
  with collapsed and expanded bounded file previews. Pressing `f` in the local
  terminal toggles only that region and forces an anchored patch, so the user
  can inspect more changed files without a full-screen redraw or log wall.
- `/status` advertises file-change expansion controls. W182 guards the state
  toggle, footer hint, local key bridge, bounded expanded preview, smoke
  registration, and phase integration.

2026-05-23 Terminal-render final summary change context:

- Red-line framing: this adjusts terminal final-summary rendering only. It does
  not change edit execution, diff generation, approval decisions, or task
  semantics.
- The final summary widget now carries `fileChangeSummary` and `diffSummary`
  separately instead of treating diff as a fallback for file changes. The
  visible summary renders both bounded lines when available, so the end-of-turn
  region can preserve “files changed” and “diff review” context at the same
  time.
- `/status` advertises final-summary file-change context. W183 guards the
  widget payload, visible final-summary lines, status metadata, smoke
  registration, and phase integration.

2026-05-23 Terminal-render error detail expansion:

- Red-line framing: this adds local error-detail expansion to the terminal
  renderer only. It does not change retry policy, command execution, approval
  decisions, or task semantics.
- Error widgets now preserve bounded detail previews and an expanded detail
  view. Pressing `x` toggles only the `error` region between layered summary
  and detail preview, keeping the draw path anchored-region based so failures
  remain inspectable without full-screen repainting or log-wall output.
- `/status` advertises error expansion controls and bounded error detail
  preview. W184 guards the widget payload, footer hint, local key bridge,
  bounded detail rendering, smoke registration, and phase integration.

2026-05-23 Terminal-render background task expansion:

- Red-line framing: this extends the background-task status panel renderer only.
  It does not change Bash task lifecycle, task execution, cancellation, or
  background output collection.
- The stream-json terminal state now treats `background_tasks` as an expandable
  panel. The collapsed view remains a small summary of recent tasks; pressing
  `b` switches the same anchored region into a bounded detail view with more
  task rows, commands, output counts, and exit codes.
- `/status` advertises background-task expansion controls. W185 guards the
  state toggle, footer hint, local key bridge, expanded bounded task list,
  smoke registration, and phase integration.

2026-05-23 Terminal-render top-stack clip diagnostics:

- Red-line framing: this improves terminal draw diagnostics for crowded
  expanded widgets only. It does not change region ordering, approval blocking,
  command execution, task execution, or slash-command routing.
- Draw plans now report top-region count, total top-region line count, and an
  explicit `clip_before_bottom_regions` overflow policy. The draw executor also
  records reserved bottom rows, visible top-row budget, and top-clipped row
 count when short terminals clip expanded top widgets before they collide with
 approval, active, or footer rows.
- `/status` advertises top-stack clip diagnostics and visible top-budget
  reporting. W186 guards the draw-plan metadata, executor report fields,
  short-viewport collision regression, smoke registration, and phase
  integration.

2026-05-23 Terminal-render footer hint budget:

- Red-line framing: this changes only terminal footer rendering and interaction
  metadata. It does not change key handling, approval decisions, command
  execution, task execution, or slash-command routing.
- The interaction snapshot keeps the full contextual hint list, while the
  footer line now uses a bounded visible hint budget and replaces hidden entries
  with a `+N more` marker. This prevents crowded command/background/file/diff/
  error states from relying on terminal-width hard clipping to hide controls.
- `/status` advertises the footer hint budget, overflow count, and full-hint
  snapshot preservation. W187 guards the visible/footer split, overflow
  metadata, footer line output, smoke registration, and phase integration.

2026-05-23 Terminal-render native mouse scroll default:

- Red-line framing: this changes only the terminal-render input capture policy.
  It does not change approval decisions, command execution, task execution,
  slash-command routing, or keyboard controls.
- Stream-json terminal rendering now keeps mouse capture off by default so the
  host terminal's native scrollback, scrollbar, and trackpad scrolling remain
  usable during long output. Mouse capture remains available behind
  `MOSSEN_TERMINAL_RENDER_CAPTURE_MOUSE=1` for environments that explicitly
  want wheel events routed through the renderer.
- `/status` advertises native mouse scroll, opt-in mouse capture, and default
  mouse-capture-off behavior. W188 guards the default-off policy, env opt-in,
  status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render manual-scroll stale deadline suppression:

- Red-line framing: this changes only the terminal draw runtime's scheduling
  behavior while manual scroll hold is active. It does not change region
  contents, approval decisions, command execution, task execution, or
  slash-command routing.
- When a throttled pending draw reaches its deadline while manual scroll hold is
  active, the runtime now keeps the pending draw but clears the stale flush
  deadline. This prevents the event loop from repeatedly retrying an update
  that must stay held until the user returns to live rendering with End,
  PageDown, Down, or Ctrl+L.
- `/status` advertises manual-scroll deadline suppression and no-busy-retry
  behavior. W189 guards the stale-deadline branch, release-after-hold flush,
  status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render synchronized update fail-closed cleanup:

- Red-line framing: this changes only the terminal draw executor's write-error
  cleanup path. It does not change region contents, approval decisions, command
  execution, task execution, or slash-command routing.
- When an anchored draw has already entered terminal synchronized-update mode
  and a later terminal write fails, the executor now makes a best-effort write
  of `EndSynchronizedUpdate` before returning the original error. This keeps
  the terminal from being left in a frozen synchronized-update state after a
  mid-draw I/O failure.
- `/status` advertises synchronized-update fail-closed cleanup. W190 guards the
  write-error branch with a forced-failure writer, status metadata, smoke
  registration, and phase integration.

2026-05-23 Terminal-render semantic color plain fallback:

- Red-line framing: this changes only terminal draw styling. It does not change
  region contents, refresh scheduling, approval decisions, command execution,
  task execution, or slash-command routing.
- The draw executor now keeps semantic style metadata but can omit foreground
  color escape sequences in plain-text environments. It respects
  `MOSSEN_TERMINAL_RENDER_COLOR=never`, `NO_COLOR`, `TERM=dumb`, and
  `CLICOLOR=0`; `MOSSEN_TERMINAL_RENDER_COLOR=always` forces color back on.
- `/status` advertises the real plain-text color fallback path. W191 guards the
  no-color writer behavior, env-policy decisions, status metadata, smoke
  registration, and phase integration.

2026-05-23 Terminal-render Unicode grapheme width guard:

- Red-line framing: this changes only terminal line bounding and wrapping. It
  does not change region selection, refresh scheduling, approval decisions,
  command execution, task execution, or slash-command routing.
- Terminal patch safety, draw-line clipping, and scrollback soft wrapping now
  iterate by Unicode grapheme cluster instead of scalar character. Complex
  emoji/ZWJ sequences and combining-mark text are either kept intact or omitted
  as a whole when the terminal cell budget is exhausted, avoiding dangling
  partial glyphs that can misalign subsequent anchored rows.
- `/status` advertises the grapheme-cluster and complex-Unicode width guards.
  W192 guards the dependency, bounded-line behavior, soft-wrap behavior, status
  metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render ASCII glyph fallback:

- Red-line framing: this changes only terminal glyph emission at draw time. It
  does not change event payloads, region selection, refresh scheduling,
  approval decisions, command execution, task execution, or slash-command
  routing.
- The draw executor now has a terminal Unicode capability policy. It respects
  `MOSSEN_TERMINAL_RENDER_UNICODE=never`/`ascii`, `TERM=dumb`, and ASCII/C/POSIX
  locales; `MOSSEN_TERMINAL_RENDER_UNICODE=always` forces Unicode back on. When
 Unicode is disabled, complex glyphs are replaced with readable ASCII
 fallbacks such as `+`, `-`, `>`, `*`, `x`, `|`, `...`, or `?` before width
 bounding and terminal writes.
- `/status` advertises ASCII glyph fallback and Unicode/ASCII mode policy.
  W193 guards env-policy decisions, bounded-line fallback, draw-executor output,
  status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render control sequence stripping:

- Red-line framing: this changes only terminal text sanitization before draw
  writes. It does not change event payload selection, refresh scheduling,
  approval decisions, command execution, task execution, or slash-command
  routing.
- The terminal patch and draw paths now remove ANSI CSI, OSC/title, DCS/PM/APC,
  C1 CSI/OSC, and bare ESC control sequences before grapheme width accounting,
  ASCII fallback, wrapping, or terminal writes. This prevents command/model
  output from moving the cursor, clearing the screen, changing terminal title,
  or leaving `[31m`/`[2J` fragments in visible text.
- `/status` advertises ANSI and OSC control-sequence stripping. W194 guards
  bounded-line sanitization, scrollback wrapping sanitization, draw-executor
  output/reporting, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render inline progress control normalization:

- Red-line framing: this changes only terminal text normalization before draw
  writes. It does not change event payload selection, refresh scheduling,
  approval decisions, command execution, task execution, or slash-command
  routing.
- After ANSI/OSC stripping, the terminal patch and draw paths now normalize
  inline progress controls before grapheme width accounting: carriage return
  keeps the latest visible progress segment, and backspace/delete remove the
  previous grapheme. This prevents command progress output such as
  `10%\r100%` or backspace-based status rewrites from becoming concatenated
  garbage in active widgets or scrollback transcript commits.
- `/status` advertises inline-control, carriage-return progress, and backspace
  progress normalization. W195 guards bounded-line normalization, draw-executor
  output/reporting, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render control character normalization:

- Red-line framing: this changes only terminal text normalization before draw
  writes. It does not change event payload selection, refresh scheduling,
  approval decisions, command execution, task execution, or slash-command
  routing.
- The terminal patch and draw paths now make non-ESC C0/C1 controls an explicit
  contract instead of an implicit side effect of line bounding. Tabs and
  embedded newlines are normalized to spaces for readable one-line widgets,
  while BEL, NUL, and other terminal-effect controls are removed before width
  accounting or crossterm writes.
- Draw execution reports now count normalized control characters, and draw-plan
  safety metadata advertises C0 control normalization, tab normalization, and
  newline write suppression. W196 guards bounded-line behavior, draw-executor
  output/reporting, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render bidi format control guard:

- Red-line framing: this changes only terminal text normalization before draw
  writes. It does not change event payload selection, refresh scheduling,
  approval decisions, command execution, task execution, slash-command routing,
  or Unicode grapheme preservation.
- The terminal patch and draw paths now strip Unicode bidi/directional format
  controls such as RLO/LRO/PDF and isolate marks before width accounting or
  crossterm writes. This prevents command output, file names, and diff previews
  from using invisible direction overrides to spoof the visual order shown in
  the terminal while preserving ZWJ/variation-selector graphemes for emoji.
- Draw execution reports now count stripped format controls, and draw-plan
  safety metadata advertises bidi control stripping, unsafe format-control
 stripping, and the Unicode bidi spoof guard. W197 guards bounded-line
 behavior, draw-executor output/reporting, status metadata, smoke registration,
 and phase integration.

2026-05-23 Terminal-render unified diff file sections:

- Red-line framing: this improves terminal diff widget rendering only. It does
  not execute commands, change file edits, alter approval decisions, or mutate
  slash-command routing.
- Diff widgets now parse unified diff text into bounded per-file sections. The
  expanded diff view can show `file: path +N -N` followed by that file's hunk
  preview instead of flattening all hunk lines into one undifferentiated block.
- The parser keeps file and hunk budgets explicit so large diffs remain
  bounded, scroll-safe, and suitable for the existing patch-region renderer.
  `/status` advertises unified diff file sections, grouped diff previews, and
 per-file hunk previews. W198 guards source-level parsing, grouped rendering,
 status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render command stream tail buffer:

- Red-line framing: this changes only terminal command-output widget rendering
  and metadata. It does not execute commands, change subprocess lifecycle,
  alter approval decisions, or mutate slash-command routing.
- Command output widgets now keep a bounded tail buffer across multiple output
  chunks instead of treating every chunk as an isolated preview. Collapsed
  command cards retain the latest small tail, while expanded command cards keep
  a larger bounded tail, so long commands remain readable without becoming a
  log wall.
- The widget records chunk count, observed output lines, retained tail lines,
  and hidden tail lines. `/status` advertises command stream tail buffering,
  chunk accounting, and bounded tail previews. W199 guards source-level tail
  accumulation, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render rich status bar metadata:

- Red-line framing: this changes only terminal status-line rendering and
  status metadata. It does not change model selection, permission decisions,
  reasoning behavior, token accounting, command execution, or slash-command
  routing.
- The stream-json terminal status region now carries Codex-CLI-style session
  facts: stage, elapsed time, scope, model, permission mode, reasoning state,
  and context token usage. The renderer also emits full, compact, and minimal
  status-line variants so narrow terminals can clip or choose a shorter status
  string without losing the core state.
- `/status` advertises rich status-bar metadata, model/mode/reasoning display,
  context usage, and width variants. W200 guards status-line enrichment, frame
  and snapshot metadata, status metadata, smoke registration, and phase
  integration.

2026-05-23 Terminal-render final summary verification context:

- Red-line framing: this changes only terminal final-summary rendering and
  metadata. It does not execute commands, alter subprocess lifecycle, change
  approval decisions, mutate model behavior, or change slash-command routing.
- Final summary widgets now retain a bounded command history instead of only
  showing the last command widget. The summary derives a verification status
  from recorded command exits, preserving passed/failed/unknown counts so the
  end-of-turn region can explain what was actually validated.
- Residual risk metadata is now explicit: raised errors remain first-class risk
  context, failed command verification is surfaced when no error summary exists,
  and successful turns without command verification are called out as
  unverified. `/status` advertises command history, verification results,
  residual risks, and bounded history retention. W201 guards source-level
  aggregation, status metadata, smoke registration, and phase integration.

2026-05-23 Stream-json /plan slash command bridge:

- Red-line framing: this changes only stream-json slash-command routing and the
  session permission-mode environment used by the existing permission bridge.
  It does not execute tools, edit files, generate plans, alter compact
  execution, or change terminal draw scheduling.
- `/plan status` now reports whether plan mode is active, `/plan enter` switches
  the current stream-json session into plan mode, and `/plan exit` returns the
  session to the default supervised mode. The existing `plan-mode` alias routes
  to the same handler, keeping the slash catalog and runtime behavior aligned.
- The capability manifest now advertises `slash.plan` as an available
  permission-mode switching command with explicit accepted args. W202 guards the
  handler, alias, manifest entry, smoke registration, and phase integration.

2026-05-23 Stream-json read-only slash inventory commands:

- Red-line framing: this changes only stream-json slash-command routing and
  read-only process-local snapshots. It does not install plugins, read skill
  content, expose hook command bodies, mutate memory files, start MCP servers,
  switch models, execute tools, or alter terminal draw scheduling.
- `/cost`, `/hooks`, `/memory`, `/skills`, `/plugin`, and `/agents` now return
  bounded safe summaries from existing runtime/bootstrap state instead of
  falling through to `unwired_slash_command` while being advertised as
  available. Sensitive content is redacted: hook command bodies, skill content,
  raw config, paths, and memory content are not emitted.
- The slash capability sources now point at the stream-json handler for these
  commands, and the wired command list includes the read-only inventory set.
 W203 guards the handlers, safe payload surfaces, smoke registration, and phase
 integration.

2026-05-23 Stream-json /model slash command bridge:

- Red-line framing: this changes only stream-json slash-command routing and the
  process-local main-loop model override. It does not call a model, start a
  request, edit task files, mutate config files, or alter terminal draw
  scheduling.
- `/model` now reports the current model source, effective model, known alias
  inventory, and model-string availability. `/model <name>` or `/model set
  <name>` updates the session override for the next turn, while `/model reset`
 clears that override back to the initial/env/default source chain.
- The slash capability manifest now points `slash.model` at the stream-json
  handler and publishes explicit accepted args. W204 guards the handler,
  model-override mutation, wired-command list, smoke registration, and phase
  integration.

2026-05-23 Stream-json /mcp slash command bridge:

- Red-line framing: this changes only stream-json slash-command routing and a
  read-only MCP runtime snapshot. It does not start or stop MCP servers, install
  tools, mutate MCP config, perform OAuth, open network connections, or alter
  terminal draw scheduling.
- `/mcp` now reports whether an MCP manager is installed, per-server connection
  state, transport label, scope, tool/prompt/resource counts, and aggregate
  counts. Raw server config, tool schemas, server instructions, local paths, and
  error details stay redacted.
- The slash capability manifest now points `slash.mcp` at the stream-json
  handler and the wired command list includes `/mcp`. W205 guards the handler,
  redacted payload surface, smoke registration, and phase integration.

2026-05-23 Stream-json /clear slash command bridge:

- Red-line framing: this changes only stream-json slash-command routing, a
  single-slot pending clear request, and safe-point dialogue state clearing. It
  does not execute tools, mutate files, alter model calls, or change terminal
  draw scheduling.
- `/clear` now returns a preview by default and requires `/clear --confirm` (or
  `/clear run --confirm`) before queueing a clear request. The request is
  consumed at the dialogue loop safe point, clears the in-memory conversation,
  emits `conversation_cleared`, and resets render-visible token counters.
- `/status` reports pending clear state beside compact state, and the slash
  capability manifest now points `slash.clear` at the stream-json handler with
  explicit accepted args. W206 guards the handler, queue, safe-point execution,
  render event mapping, smoke registration, and phase integration.

2026-05-23 Stream-json permission mode choices:

- Red-line framing: this changes only `/permissions` slash-command parsing and
  response metadata. It does not execute tools, persist permission rules, alter
  file access, or change terminal draw scheduling.
- `/permissions`, `/permission-mode`, and `/approval-mode` now expose a stable
  `mode_options` array for terminal pickers, including Codex-style `Suggest`,
  `Auto Edit`, and `Full Auto` labels plus per-mode edit/shell approval
  semantics. Common UI aliases such as `suggest`, `auto-edit`, `full-auto`, and
  `choose suggest` map to the existing session permission env values.
- The slash capability manifest now advertises those accepted args. W207 guards
  the choice payload, alias parsing, manifest args, smoke registration, and
  phase integration.

2026-05-23 Stream-json /diff slash command bridge:

- Red-line framing: this changes only `/diff` slash-command routing and a
  bounded read-only git diff summary payload. It does not write files, stage
  files, execute task commands, emit full patch content, or alter terminal draw
  scheduling.
- `/diff`, plus the `/changes` alias, now returns structured change counts and
  per-file added/removed/binary/untracked metadata for terminal diff panels.
  Raw diff text, hunks, file contents, and cwd are not included in the default
  payload.
- The slash capability manifest now advertises `slash.diff` as available with
  explicit read-only args. W208 guards the handler, bounded payload surface,
  alias routing, smoke registration, and phase integration.

2026-05-23 Stream-json /approvals slash command bridge:

- Red-line framing: this changes only `/approvals` slash-command routing and a
  redacted read-only approval status payload. It does not approve, reject, edit,
  or otherwise resolve pending tool permission requests, and it does not alter
  terminal draw scheduling.
- `/approvals`, plus `/approval-history` and `/approval-log` aliases, now
  returns pending approval count, bounded shell-command preview metadata,
  redaction flags, available terminal approval actions, and aggregate approval
  decision counters from the runtime status snapshot.
- Actual approval submission remains on the existing
  `terminal_approval_action` control-request bridge. W209 guards the handler,
  redacted pending payload, alias routing, capability manifest entry, smoke
  registration, and phase integration.

2026-05-23 Stream-json /context slash command bridge:

- Red-line framing: this changes only `/context` slash-command routing and a
  read-only token/context-window snapshot. It does not run message-level context
  analysis, mutate compact state, trigger compaction, alter model calls, or
  change terminal draw scheduling.
- `/context`, plus the `/ctx` alias, now returns aggregate input/cache/output
  token counts, effective context-window metadata, warning/error/auto-compact
  thresholds, current pending compact status, and redaction flags suitable for a
  terminal context panel.
- The slash capability manifest now advertises `slash.context` as available
  with explicit read-only args. W210 guards the handler, token/window payload,
  alias routing, capability manifest entry, smoke registration, and phase
  integration.

2026-05-23 Stream-json /config slash command bridge:

- Red-line framing: this changes only `/config` slash-command routing and a
  redacted read-only runtime/config source snapshot. It does not read raw
  settings files, expose local paths or secret values, mutate config, install
  plugins, change permission rules, or alter terminal draw scheduling.
- `/config`, plus the `/settings` alias, now returns process-local client,
  permission-mode, source, flag-settings, plugin-count, and redaction metadata
  suitable for terminal settings panels. Raw config content, setting values,
  env values, inline plugin names, and file paths are not emitted.
- The slash capability manifest now advertises `slash.config` as available
  with explicit read-only args. W211 guards the handler, redacted payload
  surface, alias routing, capability manifest entry, smoke registration, and
  phase integration.

2026-05-23 Stream-json /doctor slash command bridge:

- Red-line framing: this changes only `/doctor` slash-command routing and a
  redacted read-only in-process health snapshot. It does not run external
  binaries, perform network checks, scan installation paths, mutate auth/config,
  start MCP servers, or alter terminal draw scheduling.
- `/doctor` now reports stream-json runtime, render-pipeline readiness, slash
  wiring, agent counters, and aggregate MCP counts for terminal health panels.
  Raw errors, server details, config, secrets, env values, and install paths are
  not emitted.
- The slash capability manifest now advertises `slash.doctor` as available
  with explicit read-only args. W212 guards the handler, redacted/fast payload,
  capability manifest entry, smoke registration, and phase integration.

2026-05-23 Stream-json /ide slash command bridge:

- Red-line framing: this changes only `/ide` slash-command routing and a
  redacted read-only MCP IDE/LSP status snapshot. It does not scan processes,
  open editors, mutate MCP config, install extensions, start servers, or alter
  terminal draw scheduling.
- `/ide` now reports IDE MCP transports, aggregate connection counts, pending
  LSP diagnostic count, supported transports, and explicit no-scan/no-open
  flags for terminal IDE panels. Raw config, error details, diagnostic content,
  and local paths are not emitted.
- The slash capability manifest now advertises `slash.ide` as available with
  explicit read-only args and the `/editor` alias. W213 guards the handler,
  bounded payload, capability manifest entry, smoke registration, and phase
  integration.

2026-05-23 Stream-json /profile slash command bridge:

- Red-line framing: this changes only `/profile` slash-command routing and the
  session-scoped model profile state bridge. It does not write settings files,
  expose API keys or base URLs, run profile network tests, mutate auth state,
  install providers, or alter terminal draw scheduling.
- `/profile` and `/profiles` now report redacted profile inventory, current and
  default profile names, source/model metadata, and explicit redaction flags.
  `/profile use <name>` switches only the process-local session override for
  the next turn.
- The slash capability manifest now advertises `slash.profile` as available
  with explicit inventory and session-switch args. W214 guards handler routing,
  no-secret/no-url payload shape, alias normalization, capability entry, smoke
  registration, and phase integration.

2026-05-23 Stream-json /init slash command bridge:

- Red-line framing: this changes only `/init` slash-command routing and a
  prompt-handoff payload for project memory initialization. It does not directly
  write MOSSEN.md, create `.mossen/skills`, mutate config, bypass file-edit
  permissions, start a background model turn, or alter terminal draw scheduling.
- `/init` now reuses the existing `InitDirective` prompt generator and returns
  prompt handoff metadata for terminal clients: target file classes, existing
  file booleans, prompt preview/length, and explicit normal-tool-permission
  flags. `/init run` includes the generated agent prompt; `/init preview`
  keeps the full prompt out of the payload.
- The slash capability manifest now advertises `slash.init` as available with
  prompt/run/preview args and `writes_files` side-effect metadata. W215 guards
  handler routing, no-direct-write behavior, capability entry, smoke
  registration, and phase integration.

2026-05-23 Stream-json /login and /logout auth handoff:

- Red-line framing: this changes only stream-json slash-command protocol output
  for auth commands. It does not write credentials, delete keychain entries,
  clear token files, mutate env vars, start browser OAuth, or expose raw auth
  paths/secrets in JSON payloads.
- `/login` and `/logout` now return a redacted auth-status snapshot plus an
  external CLI handoff (`mossen auth` or `mossen deauth`) so terminal clients can
  render Codex-like auth command results without blocking the draw loop on an
  interactive credential flow.
- The slash capability manifest now marks `slash.login` and `slash.logout` as
  available auth-state commands. W216 guards routing, capability availability,
  redaction flags, no-direct-mutation flags, smoke registration, and phase
  integration.

2026-05-23 Tool-path skill discovery hook:

- Red-line framing: this changes only path-triggered skill discovery and
  activation for file tools. It does not change terminal draw scheduling,
  slash-command routing, permission policy, MCP execution, plugin installation,
  or task-code behavior.
- Dynamic skill discovery now checks `cwd/.mossen/skills` when the observed path
  is the cwd itself or a child path, closing the startup/tool-path gap without
  adding an agent-to-skills dependency cycle.
- Read/Write/Edit tool success paths now observe touched project paths,
  discover/add project skill directories, and run conditional activation with a
  bounded path set. Tool metadata reports only redacted counts and activated
  skill names; raw file paths are not included. W217 guards the cwd-level
  discovery fix, tool hooks, redaction flags, smoke registration, and phase
  integration.

2026-05-23 Skill invocation command-tag handoff:

- Red-line framing: this changes only the skill invocation prompt handoff and
  Skill tool display normalization. It does not change task-code behavior,
  terminal draw scheduling, file-tool permissions, plugin installation, MCP
  execution, or slash-command routing outside skill invocation.
- Skill execution now has one shared formatter that wraps the rendered skill
  body with Codex-compatible `<command-name>` and `<command-args>` tags before
  the payload is submitted back to the model. This closes the loop where the
  model could receive the skill body without the command envelope that tells it
  the skill has already been invoked.
- Direct slash skill invocation uses the wrapped model prompt while keeping the
  visible transcript preview tag-free. The `Skill` tool returns the same wrapped
  result plus redacted `skill_invocation` metadata, including explicit
  `resultIncludesCommandTags`, `rawSkillRootIncluded`, and
  `metadataContentRedacted` flags.
- Skill semantic rendering strips display-only command tags from visible result
  cards, so terminal output preserves the Codex-style model protocol without
  leaking protocol tags into the user-facing transcript. W218 guards the shared
  formatter, Tool/TUI handoff, display stripping, regression tests, smoke
  registration, and phase integration.

2026-05-23 Stream-json compact confirmation guard:

- Red-line framing: this changes only the stream-json `/compact` slash-command
  control bridge and capability metadata. It does not change terminal draw
  scheduling, TUI local compact execution, automatic compaction, model calls,
  file-tool permissions, plugin installation, MCP execution, or task-code
  behavior.
- `/compact` with no args now follows the safe preview path instead of failing,
  and `/compact plan` / `/compact preview` continue to enqueue dry-run compact
  requests at the dialogue safe point.
- Mutating compaction now requires an explicit confirmation token: bare
  `/compact run` returns a completed control response with `requires_confirm`
  and does not enqueue work, while `/compact run --confirm` and
  `/compact --confirm` enqueue a real non-dry-run request. Custom instructions
  still pass through after removing the confirmation token.
- The compact capability manifest now advertises the `confirm` alias alongside
  `--confirm`. W219 guards the confirm gate, no-arg preview default, redacted
  compact status metadata, regression tests, smoke registration, and phase
  integration.

2026-05-23 Stream-json compact cancellation control:

- Red-line framing: this changes only the stream-json `/compact` slash-command
  control bridge and compact capability metadata. It does not change terminal
  draw scheduling, TUI local compact cancellation, automatic compaction, model
  calls, file-tool permissions, plugin installation, MCP execution, or
  task-code behavior.
- `/compact cancel` and `/compact stop` now clear a queued pending compact
  request before the dialogue safe point executes it. The response reports
  whether anything was cancelled, the previous request id, dry-run state, and
  only a boolean for custom-instruction presence; raw custom instructions are
  not echoed in the cancel status.
- The compact capability manifest now advertises `cancel` and `stop`, matching
  the existing local TUI compact controls. W220 guards stream-json cancellation,
  redacted cancel metadata, capability registration, regression tests, smoke
  registration, and phase integration.

2026-05-23 Stream-json permission semantic payload:

- Red-line framing: this changes only stream-json `/permissions` response
  metadata and slash capability args. It does not change terminal draw
  scheduling, TUI local permission picker behavior, agent permission decisions,
  file-tool permissions, plugin installation, MCP execution, or task-code
  behavior.
- `/permissions` now returns a `codex_mode` object that maps the internal
  session permission mode to terminal-facing approval semantics: Codex-style
  value/label, edit approval, shell approval, risk, read-only plan flags, and
  legacy-mode detection. This removes the need for terminal renderers to infer
  mode behavior from internal enum names.
- The same response now includes `terminal_control` metadata with the status
  line label, picker options, selected index, alias support, and explicit rule
  redaction. Capability args now advertise common user-facing aliases such as
  `ask`, `supervised`, `read-only`, `readonly`, and `never-ask`.
- W221 guards the semantic permission payload, Codex-mode aliases, regression
  assertions, smoke registration, and phase integration.

2026-05-23 Agent permission alias parity:

- Red-line framing: this changes only agent-side permission-mode string parsing
  and evidence coverage. It does not change terminal draw scheduling, slash
  command routing, permission decisions for canonical modes, file-tool
  permissions, plugin installation, MCP execution, or task-code behavior.
- The agent loop now parses the same user-facing aliases advertised by
  stream-json `/permissions`: `suggest` and `ask` resolve to `default`,
  `read-only`, `readonly`, and `read` resolve to `plan`, and `never-ask`
  resolves to `dontAsk`. This keeps direct env/control injection consistent
  with the slash payload and TUI permission picker.
- W222 guards alias parity across stream-json payloads, agent parsing, and
  existing permission-mode execution tests so terminal status and actual tool
  permission decisions do not drift.

2026-05-23 Stream-json permission rule commands:

- Red-line framing: this changes only stream-json `/permissions`
  slash-command session-rule state and redacted response metadata. It does not
  change terminal draw scheduling, TUI local rule application, agent permission
  decisions, file-tool permissions, plugin installation, MCP execution, or
  task-code behavior.
- `/permissions allow <rule>` and `/permissions deny <rule>` now update the
  session-scoped allow/deny env rule sets used by terminal-facing permission
  state. Adding a rule removes the same rule from the opposite behavior bucket
  and keeps duplicates out.
- `/permissions list` / `show` / `rules` now report rule counts through the
  same semantic payload, and `/permissions reset` / `clear` clears the
  stream-json session rule env. Rule patterns remain redacted in all responses;
  clients get counts, behavior, and booleans instead of raw patterns.
- W223 guards allow/deny/list/reset stream-json routing, session env mutation,
  redacted payload fields, capability metadata, smoke registration, and phase
  integration.

2026-05-23 Agent session permission rules:

- Red-line framing: this changes only agent-loop permission preflight for
  session allow/deny rules already surfaced by TUI and stream-json
  `/permissions`. It does not change terminal draw scheduling, slash command
  parsing, file-tool internals, plugin installation, MCP execution semantics,
  or task-code behavior.
- The agent loop now checks `MOSSEN_PERMISSION_DENY_RULES` and
  `MOSSEN_PERMISSION_ALLOW_RULES` before approval-mode shortcuts and before
  the interactive permission gate. Deny rules win over allow rules, and matched
  deny responses use a generic message instead of echoing raw rule patterns.
- Rule matching follows the existing TUI session-rule semantics: tool name,
  command/file/path/url/description/prompt candidates, `Tool value` and
  `Tool:value` candidates, wildcard patterns, and path-prefix matching. This
  makes stream-json `/permissions allow|deny` mutate real session permission
  behavior instead of only returning UI metadata.
- W224 guards the agent preflight source label, deny-before-allow precedence,
  command candidate matching, path-prefix matching, stream-json bridge
  alignment, smoke registration, and phase integration.

2026-05-23 Compact safe-point status events:

- Red-line framing: this changes only compact control-request lifecycle
  visibility at the agent/TUI/stream-json boundary. It does not change terminal
  draw scheduling, compaction selection, compact prompt content, model calls,
  permission behavior, MCP/plugin execution, or task-code behavior.
- Pending `/compact` safe-point execution now emits a structured
  `compact_request_status` SDK message for dry-run, completed, skipped, failed,
  and timed-out outcomes. The status includes request id, redacted/bounded
  reason text, dry-run state, token counts when available, message counts when
  available, and compacted-message count when available.
- TUI render events and stream-json render events now carry
  `compact_request_status` as an immediate freeze-history event, so terminal
  clients can close the loop for `/compact plan/run` even when no compact
  boundary is created.
- W225 guards the SDK variant, safe-point emission, TUI event mapping, app
  activity/progress handling, stream-json event serialization, regression
  tests, smoke registration, and phase integration.

2026-05-23 Clear safe-point status events:

- Red-line framing: this changes only clear control-request lifecycle
  visibility at the agent/TUI/stream-json boundary. It does not change terminal
  draw scheduling, conversation clear semantics, model calls, permission
  behavior, MCP/plugin execution, or task-code behavior.
- Pending `/clear` safe-point execution now emits a structured
  `clear_request_status` SDK message for dry-run, completed, and timed-out
  outcomes. The status includes request id, dry-run state, message counts when
  available, and bounded reason text for non-mutating outcomes.
- TUI render events and stream-json render events now carry
  `clear_request_status` as an immediate freeze-history event, so terminal
  clients can close the loop even when dry-run or timeout does not produce a
  `conversation_cleared` boundary.
- W226 guards the SDK variant, safe-point emission, TUI event mapping, app
  activity/progress handling, raw lifecycle classification, stream-json event
  serialization, regression tests, smoke registration, and phase integration.

2026-05-23 Stream-json slash result render bridge:
- Red-line framing: this changes only terminal rendering for stream-json slash
  command results. It does not change slash-command semantics, permission
  decisions, compaction/clear execution, model calls, MCP/plugin execution, or
  task-code behavior.
- Slash command `control_response` payloads now produce an immediate
  `slash_command_result` render event plus snapshot/frame/patch/draw-plan items
  when a stream-json renderer is attached. This closes the visibility gap where
  `/help`, `/permissions`, `/compact`, `/clear`, and other structured slash
  results could be consumed as protocol responses without updating the
  Codex-like terminal render surface.
- `run_oneshot_stream_json` now shares one `StreamJsonRenderEventEmitter` with
  `StructuredIO`, so SDK-message render events and slash-result render events
  use the same monotonically increasing event sequence instead of resetting
  reducer state.
- TUI render semantics now include `SlashCommandResult` as an immediate
  freeze-history event, with activity/process/timeline mappings for normal
  terminal inspection surfaces.
- W227 guards the semantic event, TUI activity wiring, stream-json emitter,
  StructuredIO render response bridge, shared-emitter repl wiring, regression
  tests, smoke registration, and phase integration.

2026-05-23 Stream-json slash result terminal region:

- Red-line framing: this changes only stream-json terminal rendering for slash
  command results. It does not change slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, MCP/plugin
  execution, or task-code behavior.
- Slash command results now render into a durable top-level `slash_result`
  terminal region instead of only appearing as a transient bottom activity row.
  This makes `/help`, `/permissions`, `/compact plan`, `/clear preview`, and
  similar structured command results visible on the Codex-like terminal surface
  without being overwritten by the next activity update.
- The slash result region is bounded and redacted: it renders command/status,
  summary, and a small command-specific preview, reports omitted lines, and
  does not include the raw response object. This keeps large help catalogs and
  rich permission payloads from causing top-stack collisions or leaking raw
  control metadata into the visible terminal frame.
- Stream-json `/status` render metadata now advertises the independent slash
  result region and bounded preview contract. W228 guards the region wiring,
  bounded help preview, snapshot/frame metadata, smoke registration, and phase
  integration.

2026-05-23 Stream-json slash result lifecycle retirement:

- Red-line framing: this changes only stream-json terminal rendering lifecycle
  for slash command result regions. It does not change slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls,
  MCP/plugin execution, or task-code behavior.
- Durable slash result regions now retire on new turn and compact/clear
  lifecycle boundaries. This prevents stale `/compact run` or `/clear run`
  command-response rows from masking the later safe-point status that actually
  proves the operation completed, skipped, or timed out.
- Slash result active-row suppression is now scoped to the
  `slash_command_result` activity itself. Later lifecycle/status activity can
  render in the normal active region, while the removed `slash_result` region is
  cleared through the existing retired-region delta path instead of forcing a
  full-screen redraw.
- Stream-json `/status` render metadata now advertises the slash-result
  lifecycle-retirement contract. W229 guards the lifecycle clearing hook,
  active-row ownership rule, retired-region delta, smoke registration, and phase
  integration.

2026-05-23 Stream-json slash result event preview payload:

- Red-line framing: this changes only stream-json render-event payloads for
  slash command result rendering. It does not change slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls,
  MCP/plugin execution, or task-code behavior.
- `slash_command_result` render events now carry the same bounded, redacted
  preview metadata used by the snapshot/frame path. Event-only terminal
  reducers can render `/help`, `/permissions`, `/compact plan`, and
  `/clear preview` result details without waiting for or scraping a later frame
  item.
- The event payload still does not include the raw response object. It carries
  only preview lines, counts, preview limits, and explicit
  `rawResponseIncluded: false`/`redacted: true` metadata, so command catalogs
  and permission payloads stay inspectable without leaking raw control data.
- Stream-json `/status` render metadata now advertises the slash-result event
  preview contract. W230 guards event enrichment, event-only reducer rendering,
  redaction, smoke registration, and phase integration.

2026-05-23 Stream-json slash result event region contract:

- Red-line framing: this changes only stream-json render-event contract
  metadata for slash result terminal regions. It does not change slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, MCP/plugin execution, or task-code behavior.
- `slash_command_result` render events now carry an explicit `terminalRegion`
  contract describing the durable top `slash_result` region, replace update
  mode, bounded/redacted preview guarantees, active-row duplicate suppression,
  and the lifecycle event kinds that retire the region. Event-driven terminal
  clients no longer need to infer this from a later frame shape.
- Turn/compact/clear lifecycle render events now carry `retireRegionIds` and
  `terminalRetireRegions` metadata for `slash_result`. The reducer still keeps
  its fail-safe lifecycle fallback, but it can now retire the region from the
  event payload itself, which makes event-only clients less likely to leave a
  stale slash result widget after `/compact run` or `/clear run`.
- Stream-json `/status` render metadata now advertises the slash-result event
  region and lifecycle-retire contract. W231 guards region contract payloads,
  payload-driven retirement, status metadata, smoke registration, and phase
  integration.

2026-05-23 Stream-json slash result event region render payload:

- Red-line framing: this changes only stream-json render-event payloads for
  slash result terminal rendering. It does not change slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls,
  MCP/plugin execution, or task-code behavior.
- `slash_command_result` render events now carry `terminalRegionRender`, a
  directly drawable bounded/redacted `slash_result` region payload with the
  same lines the reducer later exposes in the frame. Event-driven terminal
  clients can render the slash result top region without duplicating command,
  status, summary, preview, and omitted-line formatting rules.
- The region render payload keeps `rawResponseIncluded: false`, line counts,
  max-line bounds, top-region placement, and the draw-region field name. This
  preserves the Codex-like anchored patch model while making event-only clients
  less likely to diverge visually from the snapshot/frame path.
- Stream-json `/status` render metadata now advertises the slash-result event
  region render payload. W232 guards direct region lines, event/frame line
  parity, redaction, smoke registration, and phase integration.

2026-05-23 Stream-json slash result event region patch payload:

- Red-line framing: this changes only stream-json render-event payloads for
  slash result terminal rendering. It does not change slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls,
  MCP/plugin execution, or task-code behavior.
- `slash_command_result` render events now carry `terminalRegionPatch`, an
  anchored-region patch hint derived from the same bounded/redacted
  `terminalRegionRender` lines. Event-driven terminal clients can apply a
  `replace_region` operation for the durable top `slash_result` region without
  falling back to whole-screen repaint or reformatting the slash response.
- Lifecycle events that retire slash results now also carry a clear-region
  patch hint. This makes event-only clients less likely to leave stale
  `/compact run` or `/clear run` result rows after the safe-point status arrives,
  while still preserving dynamic top-stack layout and prompt/cursor safety.
- Stream-json `/status` render metadata now advertises the slash-result event
  region patch payload. W233 guards replace/clear patch hints, no whole-screen
  repaint metadata, cursor/scroll preservation metadata, redaction, smoke
  registration, and phase integration.

2026-05-23 Stream-json slash result patch idempotency guards:

- Red-line framing: this changes only stream-json render-event payload metadata
  for slash result terminal rendering. It does not change slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, MCP/plugin execution, or task-code behavior.
- Slash result event region render and patch payloads now carry a `regionHash`
  that matches the later frame region hash, plus an `idempotencyKey`,
  `skipIfRegionHashUnchanged`, and explicit `eventSequenceGuard` metadata.
  Event-driven terminal clients can drop stale or duplicate patches before
  touching the screen, reducing flicker and redundant redraws.
- Retire/clear patch hints now carry source event sequence, supersession,
  skip-if-absent, and previous-region-state metadata. This lets event-only
  clients clear stale slash result rows idempotently without inventing a
  full-screen refresh when the region has already gone away.
- Stream-json `/status` render metadata now advertises slash-result patch
  idempotency guards. W234 guards event/frame hash parity, replace/clear patch
  dedupe metadata, event sequence propagation, smoke registration, and phase
  integration.

2026-05-23 Stream-json slash result patch line safety:

- Red-line framing: this changes only stream-json render-event patch metadata
  for slash result terminal rendering. It does not change slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, MCP/plugin execution, or task-code behavior.
- Slash result event `replace_region` patches now reuse the terminal patch-safe
  line helper, so event-only clients receive bounded, control-normalized lines
  with per-line cell widths, max-width metadata, truncation state, and
  control-strip state. The event patch path now has the same direct-draw safety
  vocabulary as the frame patch path.
- Retire/clear patch hints now declare the same empty-line safety contract:
  `patchSafeLines`, max line cells, empty width arrays, and zero source/safe
  line counts. That keeps replace and clear operations mechanically uniform for
  terminal clients.
- Stream-json `/status` render metadata now advertises slash-result event patch
  line safety. W235 guards helper exposure, replace/clear patch safety fields,
  status metadata, smoke registration, and phase integration.

2026-05-23 Stream-json slash result patch top-stack layout:

- Red-line framing: this changes only stream-json render-event patch metadata
  and terminal draw-plan scheduling for slash result terminal rendering. It
  does not change slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, MCP/plugin execution, or task-code
  behavior.
- Slash result event replace patches now carry an explicit `sequence` and
  top-stack layout hints: `topStartRow`, `topLineCount`, `layoutMode`, and a
  `topStackLayout` object that records the status-row baseline and conflict
  policy. Event-only clients can feed the patch to the draw scheduler without
  writing slash-result rows over the status line.
- The terminal draw scheduler now falls back from `sequence` to
  `sourceEventSequence`, so event-derived region patches still participate in
  supersession/drop guards when they are rendered without a full frame patch.
- Retire/clear event patches declare that direct draw-plan rendering requires
  previous client layout. W236 guards replace-patch top-stack row placement,
  source-event sequence scheduling, status metadata, smoke registration, and
  phase integration.

2026-05-23 Stream-json slash result patch manual-scroll hold:

- Red-line framing: this changes only stream-json render-event patch scroll
  metadata and terminal draw-runtime scheduling for slash result terminal
  rendering. It does not change slash-command semantics, permission decisions,
  compact/clear safe-point execution, model calls, MCP/plugin execution, or
  task-code behavior.
- Slash result event replace patches now declare that they are noncritical top
  region updates that should preserve manual scroll. When a user is reviewing
  scrollback, the draw runtime queues the slash-result top patch instead of
  writing into the viewport and causing a scroll jump or visible flicker.
- Lifecycle retire/clear patches keep an explicit manual-scroll bypass policy,
  so stale slash-result rows can still be cleared when compact/clear/new-turn
  boundaries require immediate visibility. W237 guards the event scroll
 contract, runtime hold/bypass behavior, status metadata, smoke registration,
 and phase integration.

2026-05-23 Terminal-render widget patch manual-scroll policy:

- Red-line framing: this changes only frame-derived terminal patch scroll
  metadata and draw-runtime scheduling for terminal rendering. It does not
  change slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, MCP/plugin execution, or task-code behavior.
- Frame-derived command/diff/plan/background/file-change/slash-result top
  widget patches now receive an explicit noncritical manual-scroll hold policy.
  When a user is reading scrollback, these widget refreshes queue until manual
  scrolling ends instead of touching the visible viewport.
- Critical frame-derived regions keep explicit bypass metadata. Approval,
  error, final-summary, retired-region clears, and scrollback commits remain
  immediately drawable so blocking or lifecycle information is not hidden behind
  a scroll hold. W238 guards the patch scroll contract, runtime hold/bypass
  behavior, status metadata, smoke registration, and phase integration.

2026-05-23 Terminal-render manual-scroll pending supersession:

- Red-line framing: this changes only terminal patch metadata and draw-runtime
  pending scheduling for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Noncritical widget patches that are held during manual scroll now advertise a
  `replace_pending_with_latest` pending policy. The draw runtime already
 replaces pending plans; W239 locks that behavior to the widget scroll contract
 so scrolling users see only the newest command/diff/plan-style region after
 releasing scroll.
- Critical region updates advertise `bypass_pending_hold`, matching their
  immediate draw behavior. W239 guards stale pending replacement, status
  metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render viewport width adaptation contract:

- Red-line framing: this changes only terminal draw-plan metadata and draw
  executor diagnostics for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- Draw plans now advertise a viewport adaptation contract with `full`,
  `compact`, and `minimal` width profiles at the Codex-like 120/80 column
  breakpoints. The contract states that status content should use the shortest
  fitting variant, secondary fields should drop before truncation, and top
  stack overflow should be clipped before bottom regions.
- The terminal draw executor now reports the concrete viewport rows, columns,
  width profile, status-line policy, and secondary-field policy it used for the
  draw. W240 guards draw-plan metadata, executor diagnostics, status metadata,
  smoke registration, and phase integration.

2026-05-24 Terminal-render viewport line variant selection:

- Red-line framing: this changes only terminal render-frame metadata, patch
  payloads, draw-plan terminal ops, and draw executor text selection for
  terminal rendering. It does not change task execution, slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, or MCP/plugin behavior.
- Status and footer regions now carry `full`, `compact`, and `minimal`
  line variants through the frame and patch pipeline. The terminal draw plan
  preserves those variants on `write_line` operations so the runtime does not
  have to hard-truncate a wide status/footer line when a narrower explicit
  variant exists.
- The draw executor now selects the first viewport-fitting variant based on
  the active width profile before applying the final grapheme/ANSI safety
  bound. Execution diagnostics count variant-enabled lines, selected variants,
  and fallback selections. W241 guards frame metadata, patch/draw propagation,
  narrow-viewport selection, status metadata, smoke registration, and phase
  integration.

2026-05-24 Terminal-render region line budget:

- Red-line framing: this changes only terminal patch generation, draw-plan
  metadata, and status metadata for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- Patch generation now caps each region before terminal ops are built. This
  prevents an unexpectedly large active/top widget region from expanding into
  thousands of `move_to_row` / `clear_line` / `write_line` ops and causing a
  long draw stall or visible terminal lag.
- Draw plans now report the region-line budget policy, max rendered lines,
  omitted-line totals, and whether terminal ops were prebudgeted. W242 guards
  the patch cap, draw-plan diagnostics, status metadata, smoke registration,
  and phase integration.

2026-05-24 Terminal-render retired region clear budget:

- Red-line framing: this changes only terminal patch generation, draw-plan
  metadata, and status metadata for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- Retired-region cleanup now caps the previous-line count before terminal ops
  are built. This prevents a stale or oversized `previousLineCount` from
  expanding into thousands of `move_to_row` / `clear_line` ops during approval,
  footer, command, diff, or slash-result region retirement.
- The draw scheduler reuses the line-budget metadata when deciding stale-line
  clears, so direct patch payloads and frame-derived retired-region patches both
  stay within the same terminal-op budget. `/status` advertises the retired
  clear budget and clear-op prebudget contracts, and W243 guards source-level
  helpers, draw-plan diagnostics, smoke registration, and phase integration.

2026-05-24 Terminal-render noncritical top line draw budget:

- Red-line framing: this changes only terminal draw-plan generation,
  terminal-op budgeting metadata, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission decisions,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Draw scheduling now applies a second budget to noncritical top widgets before
  terminal ops are emitted. Command, diff, plan, file-change, background-task,
  and slash-result widgets can no longer combine into an oversized anchored
  patch even when each individual region is under the per-region cap.
- Critical/status surfaces remain outside this noncritical top-line budget, so
  approval, error, final-summary, footer, and scrollback commits keep their
  visibility guarantees. W244 guards the source-level draw-line budget,
  terminal-op omission metadata, status metadata, smoke registration, and phase
  integration.

2026-05-24 Terminal-render cumulative noncritical top line budget:

- Red-line framing: this changes only terminal draw-plan generation,
  terminal-op budgeting metadata, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission decisions,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Noncritical top widgets now share one cumulative draw-plan line budget before
  terminal ops are emitted. This closes the remaining lag path where command,
  diff, plan, file-change, background-task, and slash-result widgets could each
  stay under their per-region cap while still combining into an oversized
  anchored patch.
- Draw plans report both per-region and cumulative noncritical top budgets, and
  omitted planned lines carry explicit terminal-op omission metadata. Critical
  approval/error/final-summary/footer/scrollback surfaces remain outside the
  budget. W245 guards cumulative budget state, region-plan metadata, status
  metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render scrollback physical line budget:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor write budgeting, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Scrollback transcript commits now carry a viewport-dependent physical-line
  budget, and the draw executor enforces it immediately before terminal writes.
  This closes the narrow-terminal lag path where bounded source lines could
  still soft-wrap into a large number of physical scrollback writes.
- Execution reports now expose whether the scrollback physical-line budget was
  applied, exceeded, and how many source/wrapped lines were omitted by the
 executor. W246 guards draw-plan metadata, executor enforcement, status
  metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render text byte write budget:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor write budgeting, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Terminal text writes now share a draw-level byte budget that is enforced
  immediately before `Print` operations. This closes the remaining lag path
  where bounded line counts could still produce an oversized terminal write
  burst through high-byte Unicode text or dense log content.
- The byte cap truncates on grapheme boundaries, omits later writes after the
  budget is exhausted, and reports max bytes, written bytes, truncation count,
  omission count, and exceeded state. W247 guards draw-plan metadata, executor
  enforcement, status metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render terminal op execution budget:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor dispatch budgeting, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Terminal draw execution now has a draw-level `terminalOps` budget enforced
  before executor dispatch. This closes the control-sequence burst path where
  an oversized or malformed draw plan could ask the renderer to process too
  many cursor moves, clears, writes, or invalid ops in one refresh.
- Execution reports expose max ops, total ops, omitted op count, and exceeded
  state. W248 guards draw-plan metadata, executor enforcement, status metadata,
  smoke registration, and phase integration.

2026-05-24 Terminal-render cursor restore fail-safe:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor cleanup behavior, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- When the terminal-op budget truncates a draw after `save_cursor` but before
  the planned `restore_cursor`, the executor now restores the cursor as a
  fail-safe cleanup action after closing any synchronized update. This prevents
  prompt/cursor drift after budget-protected refreshes.
- Execution reports expose whether a cursor was saved and how many fail-safe
  cursor restores were emitted. W249 guards draw-plan metadata, executor
  cleanup, status metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render synchronized update budget fail-safe:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor cleanup reporting, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- When the terminal-op budget truncates a draw after `begin_batch` but before
  the planned `end_batch`, the executor now reports the fail-safe
  `EndSynchronizedUpdate` cleanup as an executed terminal op. This makes the
  anti-freeze behavior observable and keeps budget-protected refreshes from
  leaving the terminal in synchronized-update mode.
- Draw plans and `/status` advertise the synchronized-update fail-safe and
  budget-truncated close contract. W250 guards draw-plan metadata, executor
  cleanup reporting, status metadata, smoke registration, and phase
  integration.

2026-05-24 Terminal-render style reset fail-safe:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor write-error cleanup, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- If a semantic color has been emitted for a line and the following terminal
  text write fails, the executor now attempts `ResetColor` before closing any
 synchronized update and returning the original error. This prevents color
  bleed into the prompt or later terminal output on partial render failures.
- Draw plans and `/status` advertise the style-reset fail-safe and
  write-error reset contract. W251 guards draw-plan metadata, executor
  fail-safe cleanup, status metadata, smoke registration, and phase
  integration.

2026-05-24 Terminal-render cursor restore write-error fail-safe:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor write-error cleanup, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- If a terminal write error occurs after an anchored patch has saved the prompt
  cursor, the executor now attempts to close synchronized update mode and
  restore the cursor before returning the original error. This closes the
  partial-render path where the terminal could recover from a failed refresh
  but leave the user's prompt cursor at a widget row.
- Draw plans and `/status` advertise the write-error cursor-restore contract.
  W252 guards draw-plan metadata, executor fail-safe cleanup, status metadata,
  smoke registration, and phase integration.

2026-05-24 Terminal-render scrollback clear-visible rows budget:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor write budgeting, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Scrollback transcript commits already default to clearing only a small number
  of bottom viewport rows before appending to normal terminal scrollback. The
  executor now enforces that same clear-visible-row cap even for malformed or
  externally supplied draw plans, so an oversized `clearVisibleBottomRows` value
  cannot expand into a large cursor-move / clear-line burst.
- Draw plans and `/status` advertise the scrollback clear-visible-row budget.
  W253 guards draw-plan metadata, executor enforcement, status metadata, smoke
  registration, and phase integration.

2026-05-24 Terminal-render executor budget hard caps:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor budget interpretation, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Draw plans may declare tighter terminal-op, text-byte, scrollback physical
  line, and scrollback visible-row clear budgets, but executor parsing now
  hard-caps every declared budget at the renderer's built-in production limits.
  This keeps malformed or externally supplied draw plans from raising limits and
  causing oversized terminal-op loops, write bursts, clear-line bursts, or
  scrollback commits.
- Draw plans and `/status` advertise the executor budget hard-cap contract.
  W254 guards metadata, hard-cap helper usage, executor behavior for oversized
  external declarations, smoke registration, and phase integration.

2026-05-24 Terminal-render executor zero-copy budgeting:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor hot-path iteration, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- The draw executor now borrows `terminalOps` and scrollback `lines` arrays while
  applying operation, text-byte, visible-row clear, and physical-line budgets
  instead of cloning whole arrays before the budgets take effect. This prevents
  malformed or externally supplied draw plans from causing pre-budget memory
  amplification and latency spikes even when the executor later omits most
  operations or scrollback lines.
- Draw plans and `/status` advertise the zero-copy budgeting contract. W255
  guards the borrowed terminal-op and scrollback-line iteration pattern, removal
  of the pre-budget clone path, smoke registration, and phase integration.

2026-05-24 Terminal-render scrollback soft-wrap materialization budget:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor scrollback wrapping, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Scrollback commits now pass the remaining physical-line budget into the
  soft-wrap materializer, so a single extremely long output line is not expanded
  into a huge intermediate `Vec<String>` before the executor drops most wrapped
  lines. The executor records omitted wrapped-line budget pressure and stops the
  source-line loop once the bounded materialization indicates remaining wrapped
  content.
- Draw plans and `/status` advertise the soft-wrap materialization budget.
  W256 guards bounded materialization, executor-level long-line behavior,
  status metadata, smoke registration, and phase integration.

2026-05-24 Terminal-render scrollback soft-wrap streaming sanitizer:

- Red-line framing: this changes only terminal draw-plan metadata, terminal
  draw-executor scrollback wrapping/sanitization, and status metadata for
  terminal rendering. It does not change task execution, slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, or MCP/plugin behavior.
- Budgeted scrollback soft-wrap now strips terminal control sequences and
  normalizes carriage-return/backspace progress while it iterates graphemes and
  applies the materialization budget. It no longer builds a full sanitized copy
  of a long line before the physical-line budget can stop wrapping.
- Draw plans and `/status` advertise the streaming sanitizer contract. W257
  guards the streaming escape-sequence consumers, inline-control behavior,
  removal of the full-line sanitize path in budgeted wrapping, smoke
  registration, and phase integration.

2026-05-24 Terminal-render draw-plan borrowed patch inputs:

- Red-line framing: this changes only terminal draw-plan scheduler metadata,
  patch-to-draw-plan iteration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- The draw-plan scheduler now borrows patch `operations` and per-region `lines`
  while building region plans and terminal ops instead of cloning those arrays
  before iterating. Scrollback terminal ops still emit owned JSON lines in the
  final draw plan, but the intermediate patch-to-draw-plan pass no longer
  creates an extra full copy before the renderer's draw and executor budgets can
  take effect.
- Draw plans and `/status` advertise the borrowed patch-input contract. W258
  guards borrowed operation/line iteration, removal of the pre-draw-plan clone
  pattern, smoke registration, and phase integration.

2026-05-24 Terminal-render draw-runtime owned pending submit:

- Red-line framing: this changes only terminal draw-runtime submission,
  terminal frontend draw-plan dispatch, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- The terminal frontend now moves owned draw-plan values into the draw runtime
  when dispatching render items. When throttle coalescing or manual-scroll hold
  needs to queue a pending plan, the runtime stores that owned value directly
  instead of cloning a potentially large draw plan first.
- Draw plans and `/status` advertise the owned pending-submit contract. The
  borrowed submit API remains as a compatibility path and reports when it clones
  into the pending queue, while W259 guards the frontend owned-submit path,
  runtime owned queueing, smoke registration, and phase integration.

2026-05-24 Terminal-render frontend draw-plan-only dispatch:

- Red-line framing: this changes only terminal render-event emitter helpers,
  local terminal frontend dispatch, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- The local terminal frontend now asks the render-event emitter for draw-plan
  items only. Snapshot, frame, and patch values are still available for the
  NDJSON stream-json path and are still used internally to derive the draw plan,
  but they are no longer appended to the in-process terminal dispatch queue
  where they would immediately be ignored.
- `/status` advertises the draw-plan-only dispatch contract. W260 guards the
  terminal-only emitter helpers, replacement of full render-item calls in the
  local terminal frontend, smoke registration, and phase integration.

2026-05-24 Terminal-render resize forced redraw:

- Red-line framing: this changes only terminal patch/draw-plan metadata,
  terminal render-event resize helpers, local terminal resize dispatch, and
  status metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Resize events now rebuild the current terminal frame into a forced draw plan
  and submit it immediately with the latest viewport instead of only flushing an
  already pending plan. The forced patch bypasses frame-hash skip and the draw
  plan bypasses stale sequence suppression, so a same-sequence frame can be
  redrawn after width changes without waiting for the next SDK event.
- Forced resize redraw suppresses same-frame scrollback append regions, so a
  transcript that was already committed to terminal history is not appended
  again when the user resizes the terminal after completion.
- `/status` advertises the resize redraw contract. W261 guards forced patch
  generation, draw-plan-only resize emission, local resize dispatch, smoke
  registration, and phase integration.

2026-05-24 Terminal-render resize burst coalescing:

- Red-line framing: this changes only the terminal frontend event pump,
  terminal resize event handling, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Resize events now use a pending gate before entering the frontend event
  queue. While one resize redraw is already queued, subsequent resize events
  are coalesced instead of accumulating in the unbounded channel and causing a
  redraw burst.
- The resize handler releases the pending gate only after it has submitted the
  forced current-frame draw plan. Because handling still reads
  `StreamJsonTerminalViewport::current()`, the redraw uses the latest terminal
  dimensions rather than the dimensions of the first resize event in the burst.
- `/status` advertises the resize burst coalescing contract. W262 guards the
  pending gate helper, duplicate resize drop behavior before queueing, gate
  release after handling, smoke registration, and phase integration.

2026-05-24 Terminal-render manual-scroll burst coalescing:

- Red-line framing: this changes only the terminal frontend event pump,
  terminal manual-scroll event handling, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Manual-scroll start/end events now use a lightweight pending-state gate before
  entering the frontend event queue. Repeated PageUp/Up or PageDown/Down style
  bursts no longer accumulate duplicate state changes while one matching scroll
  event is already waiting to be handled.
- The handler releases the matching scroll pending state only after it has
  updated runtime manual-scroll state and flushed any pending draw on scroll
  resume. This preserves the existing manual-scroll hold contract while reducing
  queue pressure during key-repeat bursts.
- `/status` advertises the manual-scroll burst coalescing contract. W263 guards
  scroll pending state tokens, duplicate scroll-state drop behavior before
  queueing, release after handling, smoke registration, and phase integration.

2026-05-24 Terminal-render priority frontend events:

- Red-line framing: this changes only the terminal frontend event pump,
  terminal frontend event selection, and status metadata for terminal rendering.
 It does not change task execution, slash-command semantics, permission
 decisions, compact/clear safe-point execution, model calls, or MCP/plugin
 behavior.
- User-critical frontend events now use a separate high-priority queue. Ctrl-C,
  approval action keys, focus keys, widget toggles, and edit-command input no
  longer sit behind queued resize or manual-scroll render events.
- The terminal event loop checks the priority queue first. After a priority
  event is handled, queued low-priority resize/manual-scroll events are drained
  and their pending gates are released, so stale scroll or resize backlog cannot
  undo the user's latest critical interaction.
- `/status` advertises the priority frontend event contract. W264 guards the
  priority queue, priority-vs-low routing, low-priority backlog draining, smoke
  registration, and phase integration.

2026-05-24 Terminal-render teardown pending flush:

- Red-line framing: this changes only the terminal draw runtime, terminal
  render oneshot final flush, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- When the terminal render path exits while manual scrolling has preserved a
  noncritical pending draw, teardown now explicitly releases the manual-scroll
  hold before the final pending flush.
- The final flush still uses the existing pending deadline when present, but a
  manual-scroll-held draw no longer remains queued forever at process exit.
- `/status` advertises the teardown pending-flush contract. W265 guards the
  runtime release method, oneshot final-flush integration, smoke registration,
  and phase integration.

2026-05-24 Terminal-render priority fairness budget:

- Red-line framing: this changes only terminal frontend event-loop scheduling
  and status metadata for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- High-priority terminal frontend events still bypass low-priority resize and
  manual-scroll backlog, but they now use a small consecutive-event budget.
  After the budget is reached, the loop yields once so ready SDK stream
  messages, permission requests, or normal terminal events can run.
- If no non-priority work is ready, the yield resets the budget and priority
  input remains responsive. This prevents key-repeat or edit-input bursts from
  turning into stream-output starvation without removing the fast path for
  urgent controls.
- `/status` advertises the priority fairness contract. W266 guards the
  fairness budget helpers, event-loop yield integration, smoke registration,
  and phase integration.

2026-05-24 Terminal-render priority-drain resize redraw:

- Red-line framing: this changes only terminal frontend event-loop drain
  handling, resize redraw submission, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- When a high-priority frontend event drains superseded low-priority backlog,
  the drain now records whether a resize event was included. Resize backlog is
  still not allowed to block the priority event, but it is no longer silently
  lost.
- After the priority event is handled, a drained resize schedules one current
  forced resize redraw using the latest viewport and current terminal frame.
  This preserves resize adaptation without replaying every stale resize event.
- `/status` advertises the priority-drain resize redraw contract. W267 guards
  drain reporting, follow-up resize redraw submission, smoke registration, and
  phase integration.

2026-05-24 Terminal-render priority-drain manual-scroll end flush:

- Red-line framing: this changes only terminal frontend event-loop drain
  follow-up handling and status metadata for terminal rendering. It does not
  change task execution, slash-command semantics, permission decisions,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Priority input still bypasses low-priority manual-scroll backlog, but a
  drained `ManualScrollEnd` is no longer reduced to a gate release only.
  The runtime now treats the drained end state as a latest bottom-resume signal.
- When the drained end state closes an active manual-scroll hold, any pending
  preserved draw is flushed immediately. This prevents output from remaining
  queued after the user has returned to the live bottom of the terminal.
- `/status` advertises the priority-drain manual-scroll end contract. W268
  guards scroll-state drain reporting, hold release, pending-draw flush, smoke
  registration, and phase integration.

2026-05-24 Terminal-render manual-scroll latest-state coalescing:

- Red-line framing: this changes only terminal manual-scroll frontend event
  coalescing, event handling, priority-drain state interpretation, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Manual-scroll start/end events now use one queued low-priority event as a
  wakeup while an atomic pending state tracks the latest scroll intent. A
  Start/End key-repeat alternation no longer grows the low-priority queue.
- When the queued wakeup is handled or drained by a priority event, the handler
  consumes the latest pending scroll state rather than blindly replaying the
  stale queued event. This preserves the user's final bottom/scrollback intent
  while reducing backlog pressure during scroll bursts.
- `/status` advertises the manual-scroll latest-state coalescing contract.
  W269 guards opposite-state supersession, latest-state consumption,
  priority-drain compatibility, smoke registration, and phase integration.

2026-05-24 Terminal-render draw-runtime report snapshot:

- Red-line framing: this changes only terminal draw-runtime observability and
  status metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- The draw runtime now records the last accepted runtime report for every
  skipped, queued, applied, or pending-flush decision. It also maintains
  lifetime counters for report count, applied reports, queued reports, skipped
  reports, and superseded pending drops.
- The snapshot gives the next soak and interactive verification passes concrete
  evidence for whether rendering is applying, coalescing, skipping, or dropping
  pending work instead of relying on visual symptoms alone.
- `/status` advertises the draw-runtime report snapshot contract. W270 guards
  last-report capture, runtime counters, dropped-pending accounting, smoke
  registration, and phase integration.

2026-05-24 Terminal-render draw-runtime diagnostics JSON:

- Red-line framing: this changes only terminal draw-runtime diagnostics
  serialization and status metadata for terminal rendering. It does not change
  task execution, slash-command semantics, permission decisions,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- The draw runtime can now serialize its snapshot into stable JSON containing
  pending state, manual-scroll hold state, lifetime report counters, dropped
  pending count, and the last runtime report.
- The last-report JSON includes queued/applied/skipped flags, pending
  sequence, next flush deadline, drop count, and a compact execution summary
  when a draw actually ran. This gives soak scripts a direct assertion surface
  for stuck pending draws, runaway coalescing, clipped rows, or failed flushes.
- `/status` advertises the draw-runtime diagnostics JSON contract. W271 guards
  snapshot serialization, last-report serialization, execution-summary
  serialization, smoke registration, and phase integration.

2026-05-24 Terminal-render draw-runtime diagnostics soak:

- Red-line framing: this changes only terminal draw-runtime simulation tests,
  diagnostics assertions, smoke coverage, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Added a simulated long stream through the real patch renderer, draw
  scheduler, and draw runtime. The scenario holds active updates during manual
  scroll, replaces many pending frames, resizes to a narrow viewport before
  release, flushes the held latest frame, then continues a live stream across
  multiple viewport widths.
- The test asserts diagnostics JSON instead of visual symptoms: pending draw is
  present during manual-scroll hold, clears after release/final flush, drop and
  queued counters rise under coalescing pressure, and the last execution
  summary reports a flushed draw without terminal-op budget overflow.
- `/status` advertises the draw-runtime diagnostics soak contract. W272 guards
  long-stream diagnostics, resize/manual-scroll interleave, no-stuck-pending
  assertions, smoke registration, and phase integration.

2026-05-24 Terminal-render final diagnostics export:

- Red-line framing: this changes only the terminal-render oneshot frontend
  teardown path, final draw-runtime diagnostics export, smoke coverage, and
  status metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- When `MOSSEN_TERMINAL_RENDER_DIAGNOSTICS_PATH` is set, the terminal-render
  frontend writes the final draw-runtime diagnostics JSON after pending teardown
  flushes and before terminal cleanup. Normal runs do not write anything.
- The exported JSON reuses W271's stable diagnostics shape, giving future PTY
  and oneshot soak scripts an external-process assertion surface for pending
  state, manual-scroll hold state, report counters, drop counters, and last
  execution summary.
- `/status` advertises the final diagnostics export contract. W273 guards the
  opt-in env hook, JSON file writer, final diagnostics handoff, smoke
  registration, and phase integration.

2026-05-24 Terminal-render oneshot diagnostics PTY smoke:

- Red-line framing: this changes only external terminal-render smoke coverage,
  diagnostics assertions, smoke registration, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Added a PTY-backed `mossen --oneshot ... --emit terminal` smoke with a local
  OpenAI-compatible SSE mock. The child process sees a real terminal, enters
  terminal-render frontend mode, streams visible head/tail markers, exits, and
  writes the W273 final diagnostics file.
- Assertions now verify the external process diagnostics JSON: no pending draw
  remains, manual-scroll hold is false, runtime reports and applied draws were
  recorded, the last execution flushed, and terminal-op budget overflow did
  not occur. The smoke also keeps synchronized-output balance, alt-screen
  balance, and full-clear checks bounded.
- `/status` advertises the oneshot diagnostics PTY contract. W274 guards the
  external-process diagnostics handoff, no-stuck-pending assertion, marker
  rendering, smoke registration, and phase integration.

2026-05-24 Terminal-render oneshot manual-scroll diagnostics PTY smoke:

- Red-line framing: this changes only terminal draw-runtime diagnostics,
  external terminal-render smoke coverage, smoke registration, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- The draw runtime diagnostics now include
  `manualScrollPreservedReportCount`, a lifetime counter for reports held
  because manual scroll was active. This gives external PTY checks direct
  evidence that live updates were preserved rather than silently applied while
  the user was reading history.
- Added a PTY-backed `mossen --oneshot ... --emit terminal` smoke that streams
  through a local OpenAI-compatible SSE mock, sends PageUp during the stream,
  sends Ctrl-L to return to live output, and reads the W273 final diagnostics
  file after process exit.
- Assertions verify that manual-scroll preserved reports were recorded, queued
  and dropped-pending counters rose under the hold, final diagnostics have no
  pending draw or active manual-scroll hold, the last draw flushed, and
  synchronized-output/full-clear checks remain bounded.
- `/status` advertises the manual-scroll diagnostics PTY contract. W275 guards
  external-process manual-scroll hold evidence, no-stuck-pending teardown,
  marker rendering, smoke registration, and phase integration.

2026-05-24 Terminal-render oneshot resize/manual-scroll diagnostics PTY smoke:

- Red-line framing: this changes only external terminal-render smoke coverage,
  diagnostics assertions, smoke registration, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Added a PTY-backed `mossen --oneshot ... --emit terminal` smoke that streams
  through a local OpenAI-compatible SSE mock, sends PageUp to enter manual
  scroll, sends a narrow resize and then a wide resize while the hold is
  active, sends Ctrl-L to restore live output, and reads the W273 final
  diagnostics file after process exit.
- Assertions verify that manual-scroll preserved reports were recorded,
  pending draws were superseded rather than spam-applied during the hold, final
  diagnostics have no pending draw or active manual-scroll hold, and the final
  applied draw used the latest wide viewport columns after restore.
- The smoke also keeps synchronized-output balance, alt-screen balance,
  full-clear count, terminal-op budget overflow, marker rendering, and output
  size bounded so resize/manual-scroll interleaving has external-process
  coverage for flicker and stale-viewport regressions.
- `/status` advertises the resize/manual-scroll diagnostics PTY contract. W276
  guards external-process latest-viewport resize restore, no-stuck-pending
  teardown, smoke registration, and phase integration.

2026-05-24 Terminal-render interrupt diagnostics PTY smoke:

- Red-line framing: this changes only external terminal-render interrupt smoke
  coverage, diagnostics assertions, smoke registration, and status metadata for
  terminal rendering. It does not change task execution, slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, or MCP/plugin behavior.
- Added a PTY-backed `mossen --oneshot ... --emit terminal` smoke that streams
  through a local OpenAI-compatible SSE mock, sends PageUp to enter
  manual-scroll hold, then sends Ctrl-C while the stream is active.
- Assertions verify that the interrupt renders a cancelled state, the upstream
  stream does not need to complete normally, final diagnostics have no pending
  draw or active manual-scroll hold, the final draw flushed, and terminal-op
  budget overflow did not occur.
- The smoke also checks synchronized-output, alt-screen, and bracketed-paste
  enter/leave balance plus bounded full-clear and output size, giving an
  external-process guard for interrupt-time terminal cleanup regressions.
- `/status` advertises the interrupt diagnostics PTY contract. W277 guards
  external-process interrupt cleanup, no-stuck-pending teardown, smoke
  registration, and phase integration.

2026-05-24 Terminal-render PTY cleanup balance contract:

- Red-line framing: this changes only external terminal-render PTY smoke
  assertions, smoke registration, and status metadata for terminal rendering.
  It does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- W274, W275, and W276 now assert bracketed-paste enter/leave balance in
  addition to synchronized-output balance, alt-screen balance, bounded
  full-clear counts, no-stuck-pending diagnostics, and terminal-op budget
  checks. W277 already covered the interrupt path.
- Added a static contract smoke that verifies all external terminal-render PTY
  paths keep these cleanup assertions present: normal completion, manual-scroll
  restore, resize/manual-scroll restore, interrupt cancellation, and slow
  first-token heartbeat/interrupt.
- `/status` advertises the PTY cleanup balance contract. W278 guards
  completion cleanup balance, scroll/resize cleanup balance, interrupt cleanup
  balance, smoke registration, and phase integration.

2026-05-24 Terminal-render slow first-token status heartbeat:

- Red-line framing: this changes only the terminal-render frontend status
  heartbeat, external terminal-render PTY smoke coverage, smoke registration,
  and status metadata for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- The oneshot terminal frontend now emits an immediate local status frame and
  then refreshes status at a bounded one-second heartbeat while waiting for SDK
  messages. This gives slow first-token and long quiet waits a visible
  `Thinking` state with elapsed time instead of a blank or apparently stuck
  terminal.
- Heartbeat frames stop after terminal completion and skip while a pending draw
  or manual scroll is active, so they do not supersede content frames, keep
  mutating a finished turn, or fight the user while they are reading
  scrollback.
- Added a PTY-backed slow first-token smoke that delays the first SSE content,
  asserts the heartbeat/status text and a `Thinking 1s` elapsed update appear
  before the first streamed content marker, and keeps diagnostics,
  synchronized-output, alt-screen, bracketed-paste, full-clear, and output-size
  cleanup checks bounded.
- `/status` advertises the status heartbeat contract. W279 guards slow
  first-token visibility, post-finish heartbeat stop behavior, smoke
  registration, and phase integration.

2026-05-24 Terminal-render slow first-token interrupt PTY smoke:

- Red-line framing: this changes only external terminal-render PTY interrupt
  smoke coverage, smoke registration, and status metadata for terminal
  rendering. It does not change task execution, slash-command semantics,
  permission decisions, compact/clear safe-point execution, model calls, or
  MCP/plugin behavior.
- Added a PTY-backed slow first-token interrupt smoke that delays the first SSE
  content chunk long enough for the heartbeat status to appear, sends Ctrl-C
  before any content marker is rendered, and asserts the process exits before
  the mock server's first-token delay would have elapsed.
- Assertions verify that the pre-content heartbeat was visible before
  interrupt, the cancelled state rendered with elapsed time, no head/tail
  content markers reached the terminal, final diagnostics have no pending draw
  or active manual-scroll hold, and synchronized-output/bracketed-paste cleanup
  remains balanced with bounded full-clear and output size.
- `/status` advertises the slow first-token interrupt contract. W280 guards
  pre-content cancel responsiveness, no-stuck-pending teardown, cleanup
  balance, smoke registration, and phase integration.

2026-05-24 Terminal-render transcript final-message dedupe:

- Red-line framing: this changes only terminal-render visible-text transcript
  accumulation, external terminal-render PTY smoke coverage, smoke
  registration, and status metadata for terminal rendering. It does not change
  task execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- Streaming text deltas already accumulate the assistant transcript. When a
  final full assistant message repeats the same visible text, the render state
  now skips the duplicate segment instead of appending it a second time to the
  final scrollback transcript.
- Added unit coverage for delta-plus-final assistant dedupe and a PTY smoke
  that verifies the final `assistant transcript` block, head marker, tail
  marker, and first stream row each appear exactly once in scrollback.
- `/status` advertises the transcript dedupe contract. W281 guards readable
  terminal history, one-shot scrollback append semantics, smoke registration,
  and phase integration.

2026-05-24 Terminal-render heartbeat metadata continuity:

- Red-line framing: this changes only terminal-render activity preservation,
  external terminal-render PTY smoke coverage, smoke registration, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Metadata-only render events, such as model/session initialization, no longer
  clear a visible status heartbeat while the frontend is waiting for the first
  content token. The active region keeps showing `waiting for model stream`
  rather than briefly flashing `No active render activity`.
- Added unit coverage for heartbeat survival across a metadata-only
  `SystemInit` update and a PTY smoke that verifies the slow first-token output
  does not contain empty activity text before the first streamed content marker.
- `/status` advertises the heartbeat metadata-continuity contract. W282 guards
  slow first-token active-region continuity, metadata status updates, smoke
  registration, and phase integration.

2026-05-24 Terminal-render initial model seed:

- Red-line framing: this changes only terminal-render status metadata seeding,
  external terminal-render PTY smoke coverage, smoke registration, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- The oneshot terminal frontend now seeds the render emitter with the locally
  selected prompt model before the first status heartbeat. The first visible
  terminal frame therefore shows the active model slot instead of `unknown`,
  while later SDK `SystemInit` metadata remains authoritative and can replace
  the seeded value.
- Added unit coverage for the seeded pre-SDK heartbeat model and SDK metadata
  override behavior, plus a PTY smoke that verifies the slow first-token output
  does not expose an `unknown` model slot before the first streamed content
  marker.
- `/status` advertises the seeded initial-model contract. W283 guards
  first-frame model visibility, no-unknown model-slot regressions, smoke
  registration, and phase integration.

2026-05-24 Terminal-render metadata waiting redraw suppression:

- Red-line framing: this changes only terminal-render heartbeat history
  metadata, external terminal-render PTY smoke coverage, smoke registration,
  and status metadata for terminal rendering. It does not change task
  execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- The slow first-token heartbeat now uses the same `update_active` history
  policy as the following SDK `turn_started` metadata event. When the seeded
  model and heartbeat activity already match the SDK metadata frame, the
  metadata event no longer rewrites the identical `waiting for model stream`
  active line.
- Added unit coverage for heartbeat `replace_active` mode and redundant
  metadata draw-plan skipping, plus a PTY smoke that verifies the slow
  first-token output contains only one pre-content `waiting for model stream`
  write.
- `/status` advertises the heartbeat replace-active and metadata redraw
  suppression contracts. W284 guards lower-flicker pre-content rendering,
  smoke registration, and phase integration.

2026-05-24 Terminal-render assistant activity text-first preview:

- Red-line framing: this changes only terminal-render activity line ordering,
  external terminal-render PTY smoke coverage, smoke registration, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Assistant message activity now prioritizes visible text preview lines in the
  active region instead of showing byte-count summaries before the content.
  Byte counters remain available in structured state, but the ordinary terminal
  message area behaves like a user-facing message surface rather than a stream
  diagnostic panel.
- Added unit coverage for assistant activity text-first ordering and a PTY
  smoke that verifies the first streamed content marker is written before any
  assistant byte-summary label.
- `/status` advertises the assistant activity text-first and no-byte-summary
  contracts. W285 guards readable live assistant output, smoke registration,
  and phase integration.

2026-05-24 Terminal-render final assistant no byte-summary flash:

- Red-line framing: this changes only terminal-render visible assistant
  preview preservation, external terminal-render PTY smoke coverage, smoke
  registration, and status metadata for terminal rendering. It does not change
  task execution, slash-command semantics, permission decisions, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- When streaming deltas have already accumulated the assistant transcript, a
  later full assistant message can be a duplicate. The transcript dedupe path
  now still refreshes active preview lines from the existing assistant tail, so
  the active region does not briefly fall back to `assistant text: N bytes`
  before the final summary.
- Added unit coverage for duplicate final assistant preview stability and a
  PTY smoke that verifies no assistant byte-summary flash appears while the
  final scrollback transcript is still committed.
- `/status` advertises the duplicate-final-preview and final no-byte-summary
  contracts. W286 guards readable turn completion, smoke registration, and
  phase integration.

2026-05-24 Terminal-render assistant activity stable rows:

- Red-line framing: this changes only terminal-render assistant activity
  layout, external terminal-render PTY smoke coverage, smoke registration, and
  status metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Assistant message activity now reserves the fixed four-line live preview
  budget from the first visible content frame. Streaming text can replace those
  rows as the tail changes, but the active region no longer grows from one row
  to four rows and pushes visible content upward during the first few chunks.
- Added unit coverage for stable assistant activity line budgeting and a PTY
  smoke that verifies the first streamed content frame already uses rows
  20-23, with no later row-growth beyond that budget.
- `/status` advertises the assistant activity stable-line-budget and no-row-
  growth contracts. W287 guards lower-jitter live assistant output, smoke
  registration, and phase integration.

2026-05-24 Terminal-render manual-scroll completion hold:

- Red-line framing: this changes only terminal-render manual-scroll policy,
  renderer contract coverage, smoke registration, and status metadata for
  terminal rendering. It does not change task execution, slash-command
  semantics, permission decisions, compact/clear safe-point execution, model
  calls, or MCP/plugin behavior.
- Non-blocking completion writes, including transcript scrollback commits and
  final-summary region updates, now preserve manual scroll instead of forcing a
  bottom jump while the user is reading history. Approval, error, and lifecycle
  clear regions still bypass the hold because they are blocking or cleanup
  critical.
- Added unit coverage for held transcript commits and held final-summary
  completion updates, plus W288 static smoke coverage for the policy and
  `/status` contract keys.
- `/status` advertises the noncritical scrollback hold and completion hold
  contracts. W288 guards terminal scrollback usability during turn completion.

2026-05-24 Terminal-render manual-scroll tail-hold PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added a PTY regression where the mock SSE stream pauses after the final
  visible tail chunk while the user is holding PageUp/manual scroll. The smoke
  records output offsets to prove late stream updates are not written to the
  terminal until Ctrl+L releases the hold.
- `/status` advertises the manual-scroll tail-hold PTY contract. W289 guards
  the user-facing behavior behind W288's completion/manual-scroll policy.

2026-05-24 Terminal-render manual-scroll teardown release PTY:

- Red-line framing: this changes only terminal-render runtime diagnostics,
  external terminal-render PTY smoke coverage, smoke registration, and status
  metadata for terminal rendering. It does not change task execution,
  slash-command semantics, permission decisions, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- The draw runtime now reports a teardown release counter when a oneshot run
  exits while manual scroll is still holding a pending draw. This distinguishes
  user-triggered Ctrl+L release from process-exit cleanup and makes final
  diagnostics explain why completion output was flushed.
- Added W290 PTY coverage that keeps PageUp/manual scroll active through model
  completion with no Ctrl+L. The smoke verifies tail output is held until
  completion, teardown release happens exactly through runtime cleanup, and
  the process exits with no pending draw.
- `/status` advertises the teardown-release diagnostic and PTY contracts. W290
  guards completion cleanup without hidden stuck pending renders.

2026-05-24 Terminal-render manual-scroll resize teardown release PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W289 tail-hold harness with an optional resize-during-hold mode
  while keeping the default W289 behavior unchanged.
- Added W291 PTY coverage that sends PageUp, resizes narrow and then wide
  while manual scroll is active, never sends Ctrl+L, and lets process teardown
  release the pending draw. The smoke verifies no visible output growth during
  the hold and that the final flushed diagnostics use the latest wide viewport.
- `/status` advertises the resize plus teardown-release PTY contract so the
  terminal renderer's completion cleanup path is tied to latest-viewport
  behavior, not stale pending dimensions.

2026-05-24 Terminal-render manual-scroll resize interrupt PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W277 interrupt harness with an optional resize-during-hold mode
  while keeping the default W277 interrupt behavior unchanged.
- Added W292 PTY coverage that sends PageUp, resizes narrow and then wide
  while manual scroll is active, then sends Ctrl+C. The smoke verifies the
  resize events do not leak held output, interrupt renders a cancelled state,
  terminal modes remain balanced, no pending draw remains, and final
  diagnostics use the latest wide viewport.
- `/status` advertises the resize plus interrupt PTY contract so Ctrl+C
  cleanup remains tied to current terminal geometry even when the user is
  reading history.

2026-05-24 Terminal-render manual-scroll approval bypass PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W293 PTY coverage that streams long assistant text, enters PageUp/manual
  scroll, then emits an OpenAI-compatible Bash tool_call. The smoke verifies
  late noncritical assistant output stays held while the blocking approval
  region bypasses the hold and becomes visible without Ctrl+L.
- W293 cancels the pending approval with Ctrl+C after the approval appears, so
  the test proves render visibility and cleanup without executing the Bash
  command.
- `/status` advertises the manual-scroll approval-bypass PTY contract. This
  ties blocking permission visibility to the real external terminal path, not
  only renderer unit tests.

2026-05-24 Terminal-render manual-scroll approval reject PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W294 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, and submits
  the visible Reject action with `n` instead of interrupting the turn.
- The smoke verifies the noncritical tail remains held, the blocking approval
  region bypasses the hold, the reject submission retires the blocking region,
  the Bash command is not executed, and the follow-up model turn renders a
  final response with no pending draw or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval-reject PTY contract. This
  ties approval action submission to the real external terminal path while
  preserving the render-only boundary.

2026-05-24 Terminal-render manual-scroll approval approve PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W295 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, and submits
  the visible Approve once action with `y`.
- The smoke verifies the noncritical tail remains held, the blocking approval
  region bypasses the hold, the Bash command executes, command stdout is
  rendered in the terminal, and the final response exits with no pending draw
  or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval-approve PTY contract. This
  covers the real external terminal approval success path after the interrupt
  and reject paths.

2026-05-24 Terminal-render manual-scroll approval edit-command PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission
  decisions, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W296 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, opens the
  visible Edit command action with `e`, types an edited shell suffix, and
  submits it with Enter.
- The smoke verifies the noncritical tail remains held, the blocking approval
  region bypasses the hold, the inline editor renders the updated command,
 the edited command executes, command stdout is rendered, and the final
 response exits with no pending draw or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval edit-command PTY contract.
  This covers the real external terminal command-edit interaction after the
  interrupt, reject, and approve-once paths.

2026-05-24 Terminal-render manual-scroll approval always-allow PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W297 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, and submits
  the visible Always allow action with `a`.
- The smoke verifies the noncritical tail remains held, the blocking approval
  region bypasses the hold, the Always allow action is visible, the Bash
  command executes, command stdout is rendered, and the final response exits
  with no pending draw or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval always-allow PTY contract.
  This completes real external-terminal coverage for the visible approval
  action set: interrupt, reject, approve once, edit command, and always allow.

2026-05-24 Terminal-render manual-scroll approval edit-cancel PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Added W298 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, opens the
  visible Edit command action, types an edited shell suffix, cancels with Esc,
  then rejects the restored approval panel with `n`.
- The smoke verifies the noncritical tail remains held, the blocking approval
  region bypasses the hold, the edit-cancel status renders, the command is not
  executed, no command stdout is rendered, and the final response exits with no
  pending draw or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval edit-cancel PTY contract.
  This covers the cancel path for the real external-terminal command-edit
  interaction without expanding task execution behavior.

2026-05-24 Terminal-render manual-scroll approval resize-approve PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W295 approve-once PTY harness with an optional approval-resize
  mode while keeping the default W295 behavior unchanged.
- Added W299 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, renders the
  blocking approval panel, resizes narrow and then wide before approval, and
  submits the visible Approve once action with `y`.
- The smoke verifies the noncritical tail remains held, the approval panel
  redraws after resize using the latest viewport, the command executes, command
  stdout is rendered, and the final response exits with no pending draw or
  unbalanced terminal modes.
- `/status` advertises the manual-scroll approval resize-approve PTY contract.
  This covers the combined high-risk path for history reading, blocking
  approval, terminal geometry changes, and approval execution.

2026-05-24 Terminal-render manual-scroll approval active-scroll reject PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W294 reject PTY harness with an optional active-scroll mode while
  keeping the default W294 behavior unchanged.
- Added W300 PTY coverage that streams long assistant text, enters
  PageUp/manual scroll, emits an OpenAI-compatible Bash tool_call, renders the
  blocking approval panel, sends another PageUp while the approval is active,
  focuses Reject with Right, and submits with Enter.
- The smoke verifies the noncritical tail remains held, the approval panel
  remains focusable after active scrolling, Reject submission retires the
  blocking region, the command is not executed, and the final response exits
  with no pending draw or unbalanced terminal modes.
- `/status` advertises the manual-scroll approval active-scroll reject PTY
  contract. This covers the high-risk path where a user keeps reading history
  after a blocking approval appears and then acts on the approval panel.

2026-05-24 Terminal-render mouse-scroll approval reject PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W294 reject PTY harness with an optional mouse-scroll mode while
  keeping the default W294 behavior unchanged.
- Added W301 PTY coverage that enables terminal mouse capture, streams long
  assistant text, enters PageUp/manual scroll, emits an OpenAI-compatible Bash
  tool_call, renders the blocking approval panel, sends an xterm SGR wheel-up
  event while the approval is active, focuses Reject with Right, and submits
  with Enter.
- The smoke verifies mouse capture is enabled and disabled, the approval panel
  remains focusable after the wheel event, Reject submission retires the
  blocking region, the command is not executed, and the final response exits
  with no pending draw or unbalanced terminal modes.
- `/status` advertises the mouse-scroll approval reject PTY contract. This
  covers the terminal path that differs from keyboard PageUp by entering
  through mouse-reporting escape sequences.

2026-05-24 Terminal-render mouse-scroll approval approve PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W295 approve-once PTY harness with an optional mouse-scroll mode
  while keeping the default W295 behavior unchanged.
- Added W302 PTY coverage that enables terminal mouse capture, streams long
  assistant text, enters PageUp/manual scroll, emits an OpenAI-compatible Bash
  tool_call, renders the blocking approval panel, sends an xterm SGR wheel-up
  event while the approval is active, and submits the visible Approve once
  action with `y`.
- The smoke verifies mouse capture is enabled and disabled, the approval panel
  remains actionable after the wheel event, approve submission executes the
  command, command stdout is rendered, the final response renders, and cleanup
  exits with no pending draw or unbalanced terminal modes.
- `/status` advertises the mouse-scroll approval approve PTY contract. This
  covers the main approve-and-execute path after mouse-reporting scroll input.

2026-05-24 Terminal-render mouse-scroll approval edit-command PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W296 edit-command PTY harness with an optional mouse-scroll mode
  while keeping the default W296 behavior unchanged.
- Added W303 PTY coverage that enables terminal mouse capture, streams long
  assistant text, enters PageUp/manual scroll, emits an OpenAI-compatible Bash
  tool_call, renders the blocking approval panel, sends an xterm SGR wheel-up
  event while the approval is active, focuses Edit command, opens the local
  edit-command input, types an edited suffix, and submits the updated command.
- The smoke verifies mouse capture is enabled and disabled, the edit-command
  action remains focusable after the wheel event, the local editor renders, the
  updated command preview renders, the edited command executes, command stdout
  is rendered, the final response renders, and cleanup exits with no pending
  draw or unbalanced terminal modes.
- `/status` advertises the mouse-scroll approval edit-command PTY contract. This
  covers the highest-input-density approval path after mouse-reporting scroll
  input.

2026-05-24 Terminal-render mouse-scroll approval always-allow PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W297 always-allow PTY harness with an optional mouse-scroll mode
  while keeping the default W297 behavior unchanged.
- Added W304 PTY coverage that enables terminal mouse capture, streams long
  assistant text, enters PageUp/manual scroll, emits an OpenAI-compatible Bash
  tool_call, renders the blocking approval panel, sends an xterm SGR wheel-up
  event while the approval is active, and submits the visible Always allow
  action with `a`.
- The smoke verifies mouse capture is enabled and disabled, the Always allow
  action remains actionable after the wheel event, session approval executes the
  command, command stdout is rendered, the final response renders, and cleanup
  exits with no pending draw or unbalanced terminal modes.
- `/status` advertises the mouse-scroll approval always-allow PTY contract. This
  completes mouse-reporting scroll coverage across the visible approval action
  set: reject, approve once, edit command, and always allow.

2026-05-24 Terminal-render manual-scroll command-output after approval PTY:

- Red-line framing: this changes only external terminal-render PTY smoke
  coverage, smoke registration, and status metadata for terminal rendering. It
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the W295 approve-once PTY harness with optional command override and
  post-approval manual-scroll injection while keeping the default W295 behavior
  unchanged.
- Added W305 PTY coverage that approves a slow Bash command, immediately enters
  PageUp/manual scroll while the command is still running, verifies command
  output stays hidden while manual scroll is active, and then verifies teardown
  releases the held command output and final response with no pending draw.
- `/status` advertises the command-output-after-approval manual-scroll hold
  contract. This closes the gap between approval action submission and later
  command/result rendering, where a Codex-CLI-like terminal must keep history
  scrolling usable instead of forcing the viewport back to the tail.

2026-05-24 Terminal-render product acceptance gate:

- Red-line framing: this changes only terminal-render smoke coverage, smoke
  registration, status metadata, and this phase note. It does not change task
  execution, slash-command semantics, permission decisions, model calls, or
  MCP/plugin behavior.
- Added W306 as a static product acceptance gate for the current terminal
  renderer slice. It verifies the W288-W305 external-terminal matrix remains
  registered, documented, and advertised through `/status`.
- The gate checks the core rendering risks covered so far: completion/tail hold,
  teardown release, resize and interrupt interleavings, approval visibility and
  all visible approval actions, mouse-reporting scroll across approval actions,
  and post-approval command-output hold.
- This is not a substitute for the PTY smokes themselves; it is a regression
  guard that prevents future rendering/execution work from silently dropping
  the Codex-CLI-like terminal contracts already built.

2026-05-24 Terminal-render mouse-scroll command-output after approval PTY:

- Red-line framing: this changes only terminal-render frontend event routing,
  external terminal-render PTY smoke coverage, smoke registration, status
  metadata, and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Promoted manual-scroll start/end frontend events to the priority queue so
  PageUp, End, Ctrl+L, and mouse-wheel scroll intent is handled ahead of
  continuous model/tool output. This keeps the renderer in manual-scroll hold
  before post-approval command stdout can repaint the viewport.
- Extended the W295 approve-once PTY harness with an optional mouse-wheel
  scroll injection after approval while a slow command is still running. The
  default W295 behavior and the existing W302 approval mouse-scroll path remain
  unchanged.
- The W305 and W307 slow commands now touch their sentinel before a short final
  sleep so the smoke samples hidden command output while the external command is
  still running, instead of racing the final summary teardown flush.
- Added W307 PTY coverage that approves a slow Bash command, sends an xterm SGR
  mouse-wheel-up event while command output is pending, verifies command output
  stays hidden while manual scroll is active, and then verifies teardown releases
  the held command output and final response with no pending draw.
- `/status` advertises the mouse command-output-after-approval manual-scroll
  hold contract, and W306 now covers the W288-W307 external-terminal matrix.
  This closes the mouse-reporting counterpart to W305 so terminal history
  scrolling remains usable after approval regardless of keyboard or mouse input.

2026-05-24 Terminal-render manual-scroll command-output resize after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, the reusable terminal-render approval harness,
  and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Extended the W295 approve-once PTY harness with an optional resize injection
  after approval while a slow command is still running and manual scroll is
  active. The existing approval-resize path before approval, W305 keyboard
  command-output hold, and W307 mouse command-output hold behaviors remain
  unchanged.
- Added W308 PTY coverage that approves a slow Bash command, enters PageUp
  manual scroll while command output is pending, resizes narrow and then wide
  during that hold, verifies command output stays hidden while manual scroll is
  active, and then verifies teardown releases the held command output and final
  response with no pending draw.
- `/status` advertises the command-output resize-after-approval manual-scroll
  hold contract, and W306 now covers the W288-W308 external-terminal matrix.
  This closes the resize counterpart to W305/W307 so terminal history scrolling
  remains usable after approval even when the terminal geometry changes while
  command output is pending.

2026-05-24 Terminal-render mouse-scroll command-output resize after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W309 PTY coverage that approves a slow Bash command, enters manual
  scroll with an xterm SGR mouse-wheel-up event while command output is
  pending, resizes narrow and then wide during that hold, verifies command
  output stays hidden while manual scroll is active, and then verifies teardown
  releases the held command output and final response with no pending draw.
- `/status` advertises the mouse command-output resize-after-approval
  manual-scroll hold contract, and W306 now covers the W288-W309
  external-terminal matrix. This closes the mouse-reporting plus resize
  counterpart to W308 so terminal history scrolling remains usable when mouse
  input and SIGWINCH interleave while command output is pending.

2026-05-24 Terminal-render manual-scroll command interrupt after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, the reusable terminal-render approval harness,
  and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Extended the W295 approve-once PTY harness with an optional Ctrl-C injection
  after approval while a slow command is running and manual scroll is active.
  The normal approve, command-output hold, resize, and mouse-scroll paths remain
  unchanged.
- Added W310 PTY coverage that approves a slow Bash command, enters PageUp
  manual scroll while the command is running, sends Ctrl-C before command output
  can render, verifies the command started but did not complete, verifies the
  output marker did not render through the hold, and verifies the terminal exits
  with no pending draw or active manual-scroll hold.
- `/status` advertises the command interrupt-after-approval manual-scroll
  contract, and W306 now covers the W288-W310 external-terminal matrix. This
  closes the keyboard interrupt counterpart to W305 so terminal history
  scrolling and cleanup remain usable when a user cancels an approved command
  from the history viewport.

2026-05-24 Terminal-render mouse-scroll command interrupt after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W311 PTY coverage that approves a slow Bash command, enters manual
  scroll with an xterm SGR mouse-wheel-up event while the command is running,
  sends Ctrl-C before command output can render, verifies the command started
  but did not complete, verifies the output marker did not render through the
  hold, and verifies the terminal exits with no pending draw or active
  manual-scroll hold.
- `/status` advertises the mouse command interrupt-after-approval
  manual-scroll contract, and W306 now covers the W288-W311 external-terminal
  matrix. This closes the mouse-reporting interrupt counterpart to W310 so
  terminal history scrolling and cleanup remain usable when a user cancels an
  approved command from a mouse-scrolled history viewport.

2026-05-24 Terminal-render manual-scroll command resize interrupt after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, the reusable terminal-render approval harness,
  and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Adjusted the reusable W295 approve-once harness so the command-output resize
  release assertion does not require command completion on the intentional
  interrupt path; the interrupt assertions still require a started command, no
  completed sentinel, no command stdout after Ctrl-C, and a rendered cancelled
  state.
- Added W312 PTY coverage that approves a slow Bash command, enters PageUp
  manual scroll while the command is running, resizes narrow and then wide
  during the hold, sends Ctrl-C after the wide resize, and verifies cleanup
  exits with no pending draw or active manual-scroll hold at the latest
  viewport.
- `/status` advertises the command resize+interrupt-after-approval
  manual-scroll contract, and W306 now covers the W288-W312 external-terminal
  matrix. This closes the SIGWINCH plus Ctrl-C counterpart to W310 so terminal
  history scrolling remains usable when geometry changes and cancellation
  interleave during an approved command.

2026-05-24 Terminal-render mouse-scroll command resize interrupt after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W313 PTY coverage that approves a slow Bash command, enters manual
  scroll with an xterm SGR mouse-wheel-up event while the command is running,
  resizes narrow and then wide during the hold, sends Ctrl-C after the wide
  resize, and verifies cleanup exits with no pending draw or active
  manual-scroll hold at the latest viewport.
- `/status` advertises the mouse command resize+interrupt-after-approval
  manual-scroll contract, and W306 now covers the W288-W313 external-terminal
 matrix. This closes the mouse-reporting counterpart to W312 so terminal
  history scrolling remains usable when mouse input, SIGWINCH, and cancellation
  interleave during an approved command.

2026-05-24 Terminal-render manual-scroll command live-tail release after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, the reusable terminal-render approval harness,
  and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Extended the W295 approve-once PTY harness with an optional Ctrl+L live-tail
  release after approval while a command is running and manual scroll is
  active. The release path records whether command stdout leaked before the
  restore, then verifies stdout flushes only after the user returns to live
  tail.
- Added W314 PTY coverage that approves a delayed Bash command, enters PageUp
  manual scroll while the command is running, sends Ctrl+L before command
  output starts, verifies no command stdout rendered before live-tail restore,
  and verifies command output plus the final response render with no pending
  draw or active manual-scroll hold.
- `/status` advertises the command live-tail release-after-approval
  manual-scroll contract, and W306 now covers the W288-W314 external-terminal
  matrix. This closes the stuck-history-view counterpart to W305 so users can
  leave a held history viewport and resume live output during an approved
  command without waiting for teardown.

2026-05-24 Terminal-render mouse-scroll command live-tail release after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W315 PTY coverage that approves a delayed Bash command, enters manual
  scroll with an xterm SGR mouse-wheel-up event while the command is running,
  sends Ctrl+L before command output starts, verifies no command stdout
  rendered before live-tail restore, and verifies command output plus the final
  response render with mouse capture cleaned up and no pending draw or active
  manual-scroll hold.
- `/status` advertises the mouse command live-tail release-after-approval
  manual-scroll contract, and W306 now covers the W288-W315 external-terminal
  matrix. This closes the mouse-reporting counterpart to W314 so users can
  leave a mouse-scrolled history viewport and resume live output during an
  approved command without waiting for teardown.

2026-05-24 Terminal-render manual-scroll command resize live-tail release after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W316 PTY coverage that approves a delayed Bash command, enters PageUp
  manual scroll while the command is running, resizes narrow and then wide
  during the manual-scroll hold, sends Ctrl+L before command output starts,
  verifies no command stdout rendered before live-tail restore, and verifies
  command output plus the final response render at the latest viewport with no
  pending draw or active manual-scroll hold.
- `/status` advertises the command resize live-tail release-after-approval
  manual-scroll contract, and W306 now covers the W288-W316 external-terminal
  matrix. This closes the SIGWINCH counterpart to W314 so users can leave a
  held history viewport and resume live output after terminal geometry changes
  during an approved command.

2026-05-24 Terminal-render mouse-scroll command resize live-tail release after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W317 PTY coverage that approves a delayed Bash command, enters manual
  scroll with an xterm SGR mouse-wheel-up event while the command is running,
  resizes narrow and then wide during the hold, sends Ctrl+L before command
  output starts, verifies no command stdout rendered before live-tail restore,
  and verifies command output plus the final response render at the latest
  viewport with mouse capture cleaned up and no pending draw or active
  manual-scroll hold.
- `/status` advertises the mouse command resize live-tail
  release-after-approval manual-scroll contract, and W306 now covers the
  W288-W317 external-terminal matrix. This closes the mouse-reporting SIGWINCH
  counterpart to W316 so users can leave a mouse-scrolled history viewport and
  resume live output after terminal geometry changes during an approved
  command.

2026-05-24 Terminal-render manual-scroll command End live-tail release after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, the reusable terminal-render approval harness,
  and the acceptance-gate smoke. It does not change task execution,
  slash-command semantics, permission rule storage, compact/clear safe-point
  execution, model calls, or MCP/plugin behavior.
- Extended the W295 approve-once PTY harness so command-time live-tail release
  can use either Ctrl+L or the advertised End live footer path. The End branch
  sends an xterm End sequence while an approved delayed command is still
  running and manual scroll is active.
- Added W318 PTY coverage that approves a delayed Bash command, enters PageUp
  manual scroll while the command is running, sends End before command stdout
  starts, verifies no command stdout rendered before live-tail restore, and
  verifies command output plus the final response render after restore with no
  pending draw or active manual-scroll hold.
- `/status` advertises the command End live-tail release-after-approval
  manual-scroll contract, and W306 now covers the W288-W318 external-terminal
  matrix. This closes the keyboard shortcut variant visible in the footer so
  users can resume live output with End, not only Ctrl+L, during an approved
  command.

2026-05-24 Terminal-render command End live-tail matrix after approval PTY:

- Red-line framing: this changes only terminal-render PTY smoke coverage, smoke
  registration, status metadata, and the acceptance-gate smoke. It does not
  change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Added W319 PTY matrix coverage for the remaining End live-tail release
  combinations after approval: mouse-scroll, resize, and mouse-scroll plus
  resize while an approved delayed command is still running.
- The matrix reuses the W295 external-terminal harness with
  `MOSSEN_TERMINAL_APPROVAL_APPROVE_PTY_RELEASE_KEY=end`, verifies the End
  release action is recorded, verifies command stdout does not leak before
  restore, verifies command output and final response render after restore,
  verifies mouse capture balances for mouse cases, and verifies resized cases
  finish at the latest viewport.
- `/status` advertises the command End live-tail release matrix, and W306 now
  covers the W288-W319 external-terminal matrix. This turns the advertised
  `End live` footer shortcut into a real command-time contract across the same
  input and SIGWINCH interleavings already covered for Ctrl+L.

2026-05-24 Terminal-render no-fullscreen-clear external PTY contract:

- Red-line framing: this changes only terminal-render PTY smoke assertions,
  smoke registration, status metadata, and the acceptance-gate smoke. It does
  not change task execution, slash-command semantics, permission rule storage,
  compact/clear safe-point execution, model calls, or MCP/plugin behavior.
- Tightened external PTY flicker protection from "bounded fullscreen clears" to
  zero fullscreen clears for the live-streaming soak, resize/manual-scroll
  soak, mouse-scroll soak, diagnostics PTYs, tail-hold PTY, and approval PTYs.
  These paths now require `full_clears == 0` while still keeping the existing
  cleanup-balance assertion name for W278 compatibility.
- Added W320 static coverage that scans the external PTY scripts for the zero
  clear invariant and fails if the old `full_clears <= 2` allowance returns.
- `/status` advertises the external-process no-fullscreen-clear contract, and
  W306 now covers the no-clear guard alongside the W288-W319 terminal PTY
  matrix. This makes the PRD's no-flicker requirement a hard regression guard
  instead of a loose allowance.

2026-05-24 Terminal-render command PageDown live-tail matrix after approval PTY:

- Red-line framing: this wave only extends the terminal rendering frontend
  release path and PTY acceptance matrix. It deliberately does not change task
  execution, slash-command semantics, permission rule storage, compact/clear
  safe-point execution, model calls, or MCP/plugin behavior.
- The footer hint now advertises PageDown together with End as the live-tail
  release shortcut, matching the existing frontend mapping where PageDown,
  Down, End, Ctrl+L, and mouse-wheel-down leave manual-scroll hold.
- Extended the reusable W295 approve-once PTY harness with a `pagedown` release
  key that sends the xterm PageDown sequence and records
  `pagedown_live_tail_release_during_command_after_approve`.
- Added W321 PTY matrix coverage for PageDown live-tail release after approval:
  keyboard/manual scroll, mouse-scroll, and resize interleavings while an
  approved delayed command is still running. The smoke verifies no command
  stdout appears before release, pending output flushes after PageDown, mouse
  capture balances for mouse cases, and resized cases finish on the latest
  viewport.
- `/status` advertises the command PageDown live-tail release matrix, and W306
  now covers this alongside the W288-W321 external-terminal matrix so the
  advertised `PgDn/End live` shortcut is backed by a real command-time
  contract.

2026-05-24 Terminal-render command mouse-wheel-down live-tail matrix after approval PTY:

- Red-line framing: this wave only extends terminal rendering input handling,
  PTY smoke coverage, status metadata, and the acceptance gate. It deliberately
  does not change task execution, slash-command semantics, permission rule
  storage, compact/clear safe-point execution, model calls, or MCP/plugin
  behavior.
- Extended the reusable W295 approve-once PTY harness with a
  `mouse_wheel_down` release key. The harness now enables mouse capture when
  the release itself uses mouse input, sends SGR mouse-wheel-down events, and
  records `mouse_wheel_down_live_tail_release_during_command_after_approve`.
- Added W322 PTY matrix coverage for mouse-wheel-down live-tail release after
  approval: keyboard/manual scroll, mouse-scroll, and resize interleavings
  while an approved delayed command is still running. The smoke verifies no
  command stdout appears before release, pending output flushes after the wheel
  down event, mouse capture is balanced in every case, and resized cases finish
  on the latest viewport.
- `/status` advertises the command mouse-wheel-down live-tail release matrix,
  and W306 now covers it alongside the W288-W322 external-terminal matrix so
  the Codex-style wheel-down tail-follow behavior has a command-time contract.

2026-05-24 Stream-json slash compact and permission controls:

- Red-line framing: this wave only strengthens stream-json slash-command
  result payloads and their static smoke coverage. It does not change compact
  execution semantics, permission enforcement semantics, agent loop behavior,
  model calls, MCP behavior, or plugin loading.
- `/permissions` now returns a terminal-ready `permission_mode_picker` payload
  with stable option ordering, Codex-facing values, selected index/value,
  keyboard affordance metadata, and explicit session mutation support. The
  existing aliases still update the same session permission env path.
- `/compact` now returns a terminal-ready `compact_preview` payload for its
  compression preview path, with safe-point execution stage, expected
  `compact_request_status` follow-up, confirmation metadata, available actions,
  and explicit history mutation semantics. `dry-run`/`dryrun` are accepted as
  preview aliases. This stays separate from the `/plan` command.
- Added W323 static coverage and run-all registration so compact preview
  payloads and permission picker controls stay wired as structured slash
  results instead of front-end guesswork.

2026-05-24 TUI composer visibility and final-summary noise gate:

- Red-line framing: this wave applies the Codex-style bottom composer lesson to
  the existing TUI prompt and reduces completion noise. It does not change the
  agent loop, tool execution, permission enforcement, MCP behavior, or plugin
  loading.
- The prompt widget now reserves a three-row visible composer with terminal
  borders and a focused border color, while retaining the one-row fallback for
  cramped layouts. This makes the input affordance obvious instead of blending
  into the transcript/footer chrome.
- Successful text-only completions no longer create a default `Final Summary`
  transcript block or switch the activity panel to `Final summary`. Structured
  summaries still record when there is high-signal work evidence such as file
  changes, command history, verification results, residual risk, failure, or
  cancellation.
- Stream-json terminal rendering now applies the same final-summary activity
  gate: successful result-only turns emit `turn_finished` without an empty
  final-summary region, while turns with terminal work activity still emit the
  final-summary event.
- Added W324 static coverage and run-all registration so the visible composer
  and final-summary noise gate remain part of the terminal rendering contract.

2026-05-24 Rust full-chain harness validation:

- Red-line framing: this adds a current Rust harness gate for agent loop,
  context compaction, context reporting, memory extraction, permissions,
  skills, MCP, plugins, and the latest slash/render controls. It does not
  resurrect the old TS-era harness wrappers.
- Added M15.1 to run package-local Rust tests across `mossen-agent`,
  `mossen-cli`, `mossen-skills`, `mossen-tools`, `mossen-mcp`, and
  `mossen-utils`, plus the W323/W324 static smokes.
- Memory coverage includes extraction prompt behavior, session-memory compact
  preservation, team-memory path compatibility, and team-memory watcher write
  notification.
- MCP coverage includes model-visible tool conversion, config enable/disable
  persistence, and the stream-json `/mcp` redacted inventory payload.
- Plugin coverage is dynamic, not only static: M15.1 exercises agent-side
  install/enable/disable/uninstall settings writes, policy-block fail-closed
  behavior, marketplace redaction, and the `/plugin` directive's core routes.
- M15.1 writes a standard `/tmp/mossen-harness/M15.1/artifacts/assertions.json`
  and a detailed full-chain report so the existing aggregator can consume it.
- The harness records that root `run-mossen.sh` and `run-bun-featured.sh` are
  absent in this Rust checkout; current validation therefore routes through
  Cargo/package tests and `scripts/start-mossen.sh` instead of treating the old
  M1-M14 wrapper suite as authoritative.
