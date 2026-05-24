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
        "StreamJsonRenderRegionFingerprint",
        "removedRegionIds",
        "retiredRegions",
        "terminal_frame_clears_retired_independent_widget_regions",
    ):
        require(render_events, token, f"retired region frame token {token}")

    for token in (
        "render_patch_retired_region_operation",
        '"op": "clear_region"',
        '"updateMode": "clear_retired"',
        "draw_scheduler_clears_retired_regions_from_frame_delta",
    ):
        require(renderer, token, f"retired region draw token {token}")

    for token in (
        '"terminal_retired_region_clear"',
        "terminal_retired_region_clear",
    ):
        require(structured_io, token, f"status retired-region metadata {token}")

    require(
        run_all,
        "wave_w151_stream_json_terminal_retired_region_clear_smoke",
        "run_all registration",
    )
    print("wave_w151_stream_json_terminal_retired_region_clear_smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
