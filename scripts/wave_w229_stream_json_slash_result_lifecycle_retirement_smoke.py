#!/usr/bin/env python3
"""W229 - slash result regions retire before lifecycle status renders."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "retire_slash_result_widget_for_lifecycle",
        "terminal_slash_result_activity_active",
        '"compact_boundary"',
        '"compact_request_status"',
        '"conversation_cleared"',
        '"clear_request_status"',
        "slash_command_result_retires_before_clear_lifecycle_status",
        '"removedRegionIds"',
        '"retiredRegions"',
    ]:
        require(bridge, token, "slash result lifecycle retirement", failures)

    for token in [
        "slash_result_lifecycle_retirement",
        "terminal_slash_result_lifecycle_retirement",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w229_stream_json_slash_result_lifecycle_retirement_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result lifecycle retirement",
        "phase note",
        failures,
    )

    if failures:
        print("=== W229 stream-json slash result lifecycle retirement smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w229_stream_json_slash_result_lifecycle_retirement_smoke: ok")


if __name__ == "__main__":
    main()
