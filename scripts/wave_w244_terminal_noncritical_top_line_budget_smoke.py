#!/usr/bin/env python3
"""W244 - noncritical top widgets are line-budgeted before terminal ops."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_NONCRITICAL_TOP_LINES",
        "render_draw_operation_line_budget",
        "render_draw_line_budget_value",
        '"cap_noncritical_top_lines_before_terminal_ops"',
        '"terminalOpsLineBudgeted"',
        '"drawLineBudgetOmittedLineCount"',
        "draw_scheduler_caps_noncritical_top_lines_before_terminal_ops",
    ]:
        require(terminal, token, "noncritical top line budget", failures)

    for token in [
        "terminal_noncritical_top_line_draw_budget",
        "terminal_ops_noncritical_top_line_budget",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w244_terminal_noncritical_top_line_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render noncritical top line draw budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W244 terminal noncritical top line budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w244_terminal_noncritical_top_line_budget_smoke: ok")


if __name__ == "__main__":
    main()
