#!/usr/bin/env python3
"""W252 - restore cursor on terminal write-error cleanup paths."""

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
        '"failSafeRestoreOnWriteError"',
        '"cursorRestoreOnWriteError"',
        '"writeErrorCursorRestore"',
        "terminal_draw_fail_cleanup",
        "saved_cursor_open",
        "draw_executor_fail_safe_restores_cursor_on_write_error",
    ]:
        require(terminal, token, "cursor restore write-error fail-safe", failures)

    require(
        structured,
        "terminal_cursor_restore_on_write_error",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w252_terminal_cursor_restore_write_error_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render cursor restore write-error fail-safe",
        "phase note",
        failures,
    )

    if failures:
        print("=== W252 terminal cursor restore write-error smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w252_terminal_cursor_restore_write_error_smoke: ok")


if __name__ == "__main__":
    main()
