#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
RENDERER = ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    bridge = BRIDGE.read_text()
    renderer = RENDERER.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "StreamJsonTerminalDrawScheduler",
        "terminal_draw_scheduler",
        "render_patch_value(&terminal_patch)",
        "STREAM_JSON_RENDER_DRAW_PLAN_TYPE",
        "STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION",
    ):
        require(bridge, token, f"bridge draw plan token {token}")

    for token in (
        "pub struct StreamJsonTerminalDrawScheduler",
        "pub fn render_patch_value",
        '"render_draw_plan"',
        '"anchored_region_patch"',
        '"save_cursor"',
        '"restore_cursor"',
        '"clearStaleRegionLines"',
        '"dropWhenSuperseded"',
        "draw_scheduler_clears_stale_lines_when_region_shrinks",
        "draw_scheduler_skips_duplicate_frame_patch_without_terminal_ops",
    ):
        require(renderer, token, f"draw scheduler token {token}")

    for token in (
        '"draw_plan_stream": true',
        '"draw_plan_type": STREAM_JSON_RENDER_DRAW_PLAN_TYPE',
        '"draw_plan_schema_version": STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION',
        '"anchored_draw_plan": true',
        '"cursor_save_restore": true',
        '"clear_stale_region_lines": true',
        '"drop_superseded_frames": true',
    ):
        require(structured_io, token, f"status draw plan metadata {token}")

    require(
        run_all,
        "wave_w140_stream_json_terminal_draw_plan_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal draw plan",
        "phase note",
    )

    print("wave_w140_stream_json_terminal_draw_plan_smoke: ok")


if __name__ == "__main__":
    main()
