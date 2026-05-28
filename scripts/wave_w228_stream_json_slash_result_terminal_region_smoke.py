#!/usr/bin/env python3
"""W228 - stream-json slash results render as a bounded terminal region."""

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
        "current_slash_result_widget",
        "terminal_slash_result_widget",
        "terminal_slash_result_lines",
        "STREAM_JSON_RENDER_SLASH_RESULT_PREVIEW_MAX_LINES",
        '"slashResultRegionId"',
        '"slash_result"',
        "slash_command_result_terminal_region_renders_bounded_help_catalog",
        '"rawResponseIncluded": false',
    ]:
        require(bridge, token, "slash result terminal region", failures)

    for token in [
        "terminal_slash_result_region",
        "terminal_slash_result_bounded_preview",
        "independent_slash_result_region",
        "slash_result_bounded_preview",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w228_stream_json_slash_result_terminal_region_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result terminal region",
        "phase note",
        failures,
    )

    if failures:
        print("=== W228 stream-json slash result terminal region smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w228_stream_json_slash_result_terminal_region_smoke: ok")


if __name__ == "__main__":
    main()
