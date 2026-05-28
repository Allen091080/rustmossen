#!/usr/bin/env python3
"""W195 - terminal renderer normalizes CR/backspace progress controls."""

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
        "terminal_sanitize_terminal_control_text",
        "terminal_normalize_inline_terminal_controls",
        "inline_control_normalized_count",
        "inlineControlsNormalized",
        "carriageReturnProgressNormalized",
        "backspaceProgressNormalized",
        "terminal_bounded_line_normalizes_carriage_return_progress",
        "terminal_bounded_line_normalizes_backspace_progress",
        "draw_executor_normalizes_inline_progress_controls_before_printing",
    ]:
        require(renderer, token, "inline control renderer", failures)

    for token in [
        "terminal_inline_control_normalization",
        "terminal_carriage_return_progress_normalization",
        "terminal_backspace_progress_normalization",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w195_terminal_inline_control_normalization_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render inline progress control normalization",
        "phase note",
        failures,
    )

    if failures:
        print("=== W195 terminal inline control normalization smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w195_terminal_inline_control_normalization_smoke: ok")


if __name__ == "__main__":
    main()
