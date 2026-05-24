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
        "current_error_widget",
        "current_final_summary_widget",
        '"errorRegionId"',
        '"finalSummaryRegionId"',
        '"replace_layered"',
        '"replace_final_summary"',
        "terminal_error_lines",
        "terminal_final_summary_lines",
        "terminal_frame_renders_error_as_layered_widget",
        "terminal_frame_renders_final_summary_as_independent_region",
    ):
        require(render_events, token, f"error/final terminal frame token {token}")

    for token in (
        "top_region_row_base_offset",
        '"error" => 11',
        '"final_summary" => 16',
    ):
        require(renderer, token, f"error/final draw placement token {token}")

    for token in (
        '"terminal_error_widget": true',
        '"terminal_final_summary_widget": true',
        '"independent_error_region": true',
        '"layered_error_region": true',
        '"independent_final_summary_region": true',
        '"final_summary_terminal_region": true',
    ):
        require(structured_io, token, f"status error/final metadata {token}")

    require(
        run_all,
        "wave_w149_stream_json_terminal_error_final_widgets_smoke",
        "run_all registration",
    )
    print("wave_w149_stream_json_terminal_error_final_widgets_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
