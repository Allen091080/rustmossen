#!/usr/bin/env python3
"""W261 - terminal resize forces an immediate current-frame redraw."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "render_frame_value_forced",
        "forcedRedraw",
        "forceRedrawReason",
        "supersededSequenceBypass",
        "scrollbackAppendOnceSuppressed",
        "renderer_forces_redraw_for_resize_even_when_frame_hash_is_unchanged",
        "force_redraw_current_frame_with_latest_viewport",
        "render_patch_suppress_forced_scrollback_reappend",
    ]:
        require(renderer, token, "forced resize redraw renderer contract", failures)

    for token in [
        "emit_terminal_resize_draw_plan_items",
        "emit_current_terminal_forced_draw_plan_items",
        "terminal_frontend_resize_emit_forces_current_draw_plan_only",
        "terminal_frontend_resize_does_not_reappend_committed_transcript",
    ]:
        require(events, token, "resize draw-plan emitter", failures)

    require(
        repl,
        "emit_terminal_resize_draw_plan_items()",
        "resize frontend dispatch",
        failures,
    )
    for token in [
        "terminal_resize_immediate_redraw",
        "terminal_resize_bypasses_superseded_sequence",
        "terminal_resize_suppresses_duplicate_scrollback_append",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w261_terminal_resize_forced_redraw_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render resize forced redraw",
        "phase note",
        failures,
    )

    if failures:
        print("=== W261 terminal resize forced redraw smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w261_terminal_resize_forced_redraw_smoke: ok")


if __name__ == "__main__":
    main()
