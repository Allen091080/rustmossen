#!/usr/bin/env python3
"""W245 - noncritical top line budget is cumulative across a draw plan."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_TOTAL_LINES",
        "StreamJsonTerminalDrawLineBudget",
        "available_noncritical_top_lines",
        "consume_noncritical_top_lines",
        '"cumulative_noncritical_top_widgets"',
        '"drawLineBudgetMaxNoncriticalTopTotalLines"',
        "draw_scheduler_caps_cumulative_noncritical_top_lines_before_terminal_ops",
    ]:
        require(terminal, token, "cumulative top line budget", failures)

    for token in [
        "terminal_cumulative_noncritical_top_line_budget",
        "terminal_draw_plan_noncritical_top_total_budget",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w245_terminal_cumulative_top_line_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render cumulative noncritical top line budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W245 terminal cumulative top line budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w245_terminal_cumulative_top_line_budget_smoke: ok")


if __name__ == "__main__":
    main()
