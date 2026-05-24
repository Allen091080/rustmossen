#!/usr/bin/env python3
"""W197 - terminal renderer strips bidi format controls."""

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
        "format_control_stripped_count",
        "terminal_unsafe_format_control",
        "bidiControlsStripped",
        "unsafeFormatControlsStripped",
        "unicodeBidiSpoofGuard",
        "terminal_bounded_line_strips_bidi_format_controls",
        "draw_executor_strips_bidi_format_controls_before_printing",
    ]:
        require(renderer, token, "bidi format renderer", failures)

    for token in [
        "terminal_bidi_control_strip",
        "terminal_unicode_format_control_guard",
        "terminal_directional_spoof_guard",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w197_terminal_bidi_format_guard_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render bidi format control guard",
        "phase note",
        failures,
    )

    if failures:
        print("=== W197 terminal bidi format guard smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w197_terminal_bidi_format_guard_smoke: ok")


if __name__ == "__main__":
    main()
