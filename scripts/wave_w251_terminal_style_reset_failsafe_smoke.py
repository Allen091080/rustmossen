#!/usr/bin/env python3
"""W251 - semantic style reset is fail-safe after styled write errors."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        '"failSafeResetOnWriteError"',
        '"styleResetFailSafe"',
        '"styleWriteErrorReset"',
        "terminal_draw_fail_reset_style",
        "FailAfterStyleColorWriter",
        "draw_executor_fail_resets_style_on_styled_write_error",
    ]:
        require(terminal, token, "style reset fail-safe", failures)

    for token in [
        "terminal_style_reset_fail_safe",
        "terminal_style_write_error_reset",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w251_terminal_style_reset_failsafe_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render style reset fail-safe",
        "phase note",
        failures,
    )

    if failures:
        print("=== W251 terminal style reset fail-safe smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w251_terminal_style_reset_failsafe_smoke: ok")


if __name__ == "__main__":
    main()
