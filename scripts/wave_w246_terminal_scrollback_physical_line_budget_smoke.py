#!/usr/bin/env python3
"""W246 - scrollback appends cap viewport-dependent physical line writes."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES",
        "terminal_draw_scrollback_physical_line_budget",
        "terminal_draw_scrollback_physical_line_budget_value",
        '"cap_scrollback_physical_lines_before_terminal_writes"',
        '"scrollbackPhysicalLineBudgeted"',
        "scrollback_physical_line_budget_exceeded",
        "draw_executor_caps_scrollback_physical_lines_before_terminal_writes",
    ]:
        require(terminal, token, "scrollback physical line budget", failures)

    for token in [
        "terminal_scrollback_physical_line_budget",
        "terminal_scrollback_write_budget_executor_enforced",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w246_terminal_scrollback_physical_line_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render scrollback physical line budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W246 terminal scrollback physical line budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w246_terminal_scrollback_physical_line_budget_smoke: ok")


if __name__ == "__main__":
    main()
