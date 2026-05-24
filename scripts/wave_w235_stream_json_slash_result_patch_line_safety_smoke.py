#!/usr/bin/env python3
"""W235 - slash result event patches carry patch-safe line metadata."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    bridge = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "stream_json_terminal_patch_safe_line",
        "STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS",
    ]:
        require(terminal, token, "terminal patch-safe helper", failures)

    for token in [
        "terminal_slash_result_patch_safe_lines",
        '"patchSafeLines"',
        '"maxLineCells"',
        '"lineWidthCells"',
        '"maxLineWidthCells"',
        '"sourceLineCount"',
        '"safeLineCount"',
        '"controlCharsStripped"',
        '"boundedLineCells"',
    ]:
        require(bridge, token, "slash result patch line safety", failures)

    for token in [
        "slash_result_event_patch_line_safety",
        "terminal_slash_result_event_patch_line_safety",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w235_stream_json_slash_result_patch_line_safety_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Stream-json slash result patch line safety",
        "phase note",
        failures,
    )

    if failures:
        print("=== W235 stream-json slash result patch line safety smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w235_stream_json_slash_result_patch_line_safety_smoke: ok")


if __name__ == "__main__":
    main()
