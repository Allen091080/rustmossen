#!/usr/bin/env python3
"""W248 - terminal ops are budgeted before executor dispatch."""

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
        "STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS",
        "terminal_draw_terminal_op_budget",
        "terminal_draw_terminal_op_budget_value",
        '"cap_terminal_ops_before_execution"',
        '"terminalOpBudgeted"',
        "terminal_op_budget_exceeded",
        ".take(terminal_op_budget)",
        "draw_executor_caps_terminal_ops_before_execution",
    ]:
        require(terminal, token, "terminal op execution budget", failures)

    for token in [
        "terminal_op_execution_budget",
        "terminal_op_budget_executor_enforced",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w248_terminal_op_execution_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render terminal op execution budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W248 terminal op execution budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w248_terminal_op_execution_budget_smoke: ok")


if __name__ == "__main__":
    main()
