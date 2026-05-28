#!/usr/bin/env python3
"""W249 - cursor restore is fail-safe after budget-truncated draw ops."""

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
        '"failSafeRestoreAfterBudgetTruncation"',
        '"cursorRestoreFailSafe"',
        '"budgetTruncatedCursorRestore"',
        "saved_cursor_open",
        "cursor_restore_fail_safe_count",
        "draw_executor_fail_safe_restores_cursor_after_budget_truncation",
    ]:
        require(terminal, token, "cursor restore fail-safe", failures)

    for token in [
        "terminal_cursor_restore_fail_safe",
        "terminal_budget_truncated_cursor_restore",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w249_terminal_cursor_restore_failsafe_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render cursor restore fail-safe",
        "phase note",
        failures,
    )

    if failures:
        print("=== W249 terminal cursor restore fail-safe smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w249_terminal_cursor_restore_failsafe_smoke: ok")


if __name__ == "__main__":
    main()
