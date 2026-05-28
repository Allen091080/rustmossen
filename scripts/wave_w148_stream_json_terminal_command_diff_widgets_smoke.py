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
        '"commandRegionId"',
        '"diffRegionId"',
        '"replace_summary"',
        '"replace_collapsed"',
        "terminal_command_lines",
        "terminal_diff_lines",
        "terminal_frame_renders_command_as_summary_widget_without_log_wall",
        "terminal_frame_renders_diff_as_collapsed_widget",
    ):
        require(render_events, token, f"command/diff terminal frame token {token}")

    for token in (
        "top_region_row_base_offset",
        '"command" => 1',
        '"diff" => 7',
    ):
        require(renderer, token, f"command/diff draw placement token {token}")

    for token in (
        '"terminal_command_widget": true',
        '"terminal_diff_widget": true',
        '"independent_command_region": true',
        '"command_output_summary_only": true',
        '"independent_diff_region": true',
        '"diff_collapsed_by_default": true',
    ):
        require(structured_io, token, f"status command/diff metadata {token}")

    require(
        run_all,
        "wave_w148_stream_json_terminal_command_diff_widgets_smoke",
        "run_all registration",
    )
    print("wave_w148_stream_json_terminal_command_diff_widgets_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
