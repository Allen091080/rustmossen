#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str) -> None:
    if token not in text:
        raise AssertionError(f"missing {label}: {token}")


def main() -> int:
    render_events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured_io = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()

    for token in (
        "approval_active || error_active || final_summary_active",
        "terminal_frame_renders_error_as_layered_widget",
        "terminal_frame_renders_final_summary_as_independent_region",
    ):
        require(render_events, token, f"independent widget active suppression {token}")

    for token in (
        "terminal_draw_reserved_bottom_rows",
        "reserved_bottom_rows",
        "top_limit",
        "draw_executor_clips_top_widgets_before_bottom_regions_on_short_viewports",
    ):
        require(renderer, token, f"viewport collision guard token {token}")

    for token in (
        '"terminal_viewport_collision_guard": true',
        '"terminal_top_bottom_collision_guard"',
        '"terminal_independent_widget_suppresses_duplicate_active"',
    ):
        require(structured_io, token, f"status viewport collision metadata {token}")

    require(
        run_all,
        "wave_w150_stream_json_terminal_viewport_collision_guard_smoke",
        "run_all registration",
    )
    print("wave_w150_stream_json_terminal_viewport_collision_guard_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
