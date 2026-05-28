#!/usr/bin/env python3
"""W196 - terminal renderer normalizes non-ESC control characters."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "control_char_normalized_count",
        "terminal_control_char_becomes_space",
        "c0ControlCharsNormalized",
        "tabsNormalizedToSpaces",
        "newlineWritesSuppressed",
        "terminal_bounded_line_normalizes_control_chars_without_terminal_effects",
        "draw_executor_normalizes_control_chars_before_printing",
    ]:
        require(renderer, token, "control-char renderer", failures)

    for token in [
        "terminal_control_char_normalization",
        "terminal_tab_width_normalization",
        "terminal_newline_write_guard",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w196_terminal_control_char_normalization_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render control character normalization",
        "phase note",
        failures,
    )

    if failures:
        print("=== W196 terminal control character normalization smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w196_terminal_control_char_normalization_smoke: ok")


if __name__ == "__main__":
    main()
