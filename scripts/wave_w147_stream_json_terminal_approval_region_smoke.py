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
        '"approval"',
        '"replace_blocking"',
        '"approvalRegionId"',
        '"blockingRegionIds"',
        "terminal_approval_lines",
        "terminal_frame_renders_approval_as_independent_blocking_region",
    ):
        require(render_events, token, f"approval terminal frame token {token}")

    for token in (
        "hasBlockingRegion",
        "blockingRegionIds",
        "render_region_plan_is_blocking",
        '"role") == "approval"',
    ):
        require(renderer, token, f"approval draw plan token {token}")

    for token in (
        '"terminal_approval_widget": true',
        '"independent_approval_region": true',
        '"approval_blocks_active_log": true',
        '"approval_draw_plan_blocking_region": true',
    ):
        require(structured_io, token, f"status approval metadata {token}")

    require(
        run_all,
        "wave_w147_stream_json_terminal_approval_region_smoke",
        "run_all registration",
    )
    print("wave_w147_stream_json_terminal_approval_region_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
