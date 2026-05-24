#!/usr/bin/env python3
"""W254 - executor budget declarations cannot raise renderer hard caps."""

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
        "terminal_draw_usize_budget_with_hard_cap",
        "terminal_draw_executor_budget_hard_caps_value",
        '"min_declared_budget_with_renderer_hard_cap"',
        '"terminalExecutorBudgetHardCaps"',
        '"executorBudgetHardCaps"',
        "draw_executor_hard_caps_external_scrollback_budget_declarations",
        "draw_executor_hard_caps_external_text_byte_budget_declaration",
        "draw_executor_hard_caps_external_terminal_op_budget_declaration",
        "STREAM_JSON_RENDER_DRAW_MAX_TERMINAL_OPS",
        "STREAM_JSON_RENDER_DRAW_MAX_TEXT_BYTES",
        "STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_PHYSICAL_LINES",
        "STREAM_JSON_RENDER_DRAW_MAX_SCROLLBACK_CLEAR_VISIBLE_ROWS",
    ]:
        require(terminal, token, "executor budget hard caps", failures)

    for token in [
        "terminal_executor_budget_hard_caps",
        "terminal_budget_declaration_hard_cap",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w254_terminal_executor_budget_hard_caps_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render executor budget hard caps",
        "phase note",
        failures,
    )

    if failures:
        print("=== W254 terminal executor budget hard caps smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w254_terminal_executor_budget_hard_caps_smoke: ok")


if __name__ == "__main__":
    main()
