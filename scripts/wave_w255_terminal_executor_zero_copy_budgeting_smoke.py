#!/usr/bin/env python3
"""W255 - draw executor budgets borrowed arrays without pre-budget cloning."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def forbid(text: str, token: str, label: str, failures: list[str]) -> None:
    if token in text:
        failures.append(f"{label}: forbidden {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_draw_executor_zero_copy_budgeting_value",
        '"terminalExecutorZeroCopyBudgeting"',
        '"executorZeroCopyBudgeting"',
        '"borrow_terminal_ops_and_scrollback_lines_before_budget"',
        'let terminal_ops = draw_plan.get("terminalOps").and_then(Value::as_array);',
        "let terminal_op_count = terminal_ops.map_or(0, Vec::len);",
        "for operation in terminal_ops.iter().take(terminal_op_budget)",
        "let line_count = lines.map_or(0, Vec::len);",
        "for (line_index, line) in lines.into_iter().flatten().enumerate()",
    ]:
        require(terminal, token, "zero-copy executor budgeting", failures)

    for token in [
        'draw_plan\n            .get("terminalOps")\n            .and_then(Value::as_array)\n            .cloned()',
        '.get("lines")\n                        .and_then(Value::as_array)\n                        .cloned()',
    ]:
        forbid(terminal, token, "pre-budget clone guard", failures)

    for token in [
        "terminal_executor_zero_copy_budgeting",
        "terminal_scrollback_lines_zero_copy_budgeting",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w255_terminal_executor_zero_copy_budgeting_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render executor zero-copy budgeting",
        "phase note",
        failures,
    )

    if failures:
        print("=== W255 terminal executor zero-copy budgeting smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w255_terminal_executor_zero_copy_budgeting_smoke: ok")


if __name__ == "__main__":
    main()
