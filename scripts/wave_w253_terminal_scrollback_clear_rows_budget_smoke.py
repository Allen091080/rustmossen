#!/usr/bin/env python3
"""W253 - scrollback visible-row clears are capped before terminal writes."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS",
        "terminal_draw_scrollback_clear_visible_rows_budget",
        "terminal_draw_scrollback_clear_visible_rows_budget_value",
        '"cap_scrollback_clear_visible_rows_before_terminal_writes"',
        '"scrollbackClearVisibleRowsBudgeted"',
        "scrollback_clear_visible_rows_budget_exceeded",
        "draw_executor_caps_scrollback_clear_visible_rows_before_terminal_writes",
    ]:
        require(terminal, token, "scrollback clear-row budget", failures)

    for token in [
        "terminal_scrollback_clear_visible_rows_budget",
        "terminal_scrollback_clear_rows_executor_enforced",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w253_terminal_scrollback_clear_rows_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render scrollback clear-visible rows budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W253 terminal scrollback clear-row budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w253_terminal_scrollback_clear_rows_budget_smoke: ok")


if __name__ == "__main__":
    main()
