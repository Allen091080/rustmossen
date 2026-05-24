#!/usr/bin/env python3
"""W236 - slash result event patches carry direct draw top-stack layout hints."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "STREAM_JSON_RENDER_SLASH_RESULT_EVENT_TOP_START_ROW",
        "terminal_slash_result_patch_top_stack_layout_value",
        '"eventPatchDrawPlanCompatible"',
        '"topStackLayout"',
        '"topStartRow"',
        '"layoutMode"',
        '"preventsStatusOverwrite"',
        '"prefer_frame_patch_layout"',
        "slash_result_event_patch_can_render_draw_plan_without_frame_patch",
    ]:
        require(bridge, token, "slash result patch top-stack layout", failures)

    for token in [
        "terminal_patch_sequence_value",
        '"sourceEventSequence"',
        "draw_scheduler_uses_source_event_sequence_for_event_patch",
    ]:
        require(terminal, token, "terminal draw scheduler event patch sequence", failures)

    for token in [
        "slash_result_event_patch_top_stack_layout",
        "terminal_slash_result_event_patch_top_stack_layout",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w236_stream_json_slash_result_patch_top_stack_layout_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result patch top-stack layout",
        "phase note",
        failures,
    )

    if failures:
        print("=== W236 stream-json slash result patch top-stack layout smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w236_stream_json_slash_result_patch_top_stack_layout_smoke: ok")


if __name__ == "__main__":
    main()
