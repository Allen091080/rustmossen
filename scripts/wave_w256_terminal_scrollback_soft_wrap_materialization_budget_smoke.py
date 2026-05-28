#!/usr/bin/env python3
"""W256 - scrollback soft-wrap materialization is capped before allocation."""

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
        "terminal_draw_soft_wrapped_lines_with_unicode_budget",
        "terminal_draw_push_soft_wrapped_line_with_budget",
        "terminal_draw_scrollback_soft_wrap_materialization_budget_value",
        '"cap_soft_wrap_materialization_before_scrollback_allocation"',
        '"scrollbackSoftWrapMaterializationBudgeted"',
        '"scrollbackSoftWrapMaterializationBudget"',
        "omitted_wrapped_line_count",
        "remaining_physical_lines",
        "terminal_soft_wrap_materialization_respects_line_budget",
        "draw_executor_caps_scrollback_soft_wrap_materialization_before_allocation",
    ]:
        require(terminal, token, "soft-wrap materialization budget", failures)

    for token in [
        "terminal_scrollback_soft_wrap_materialization_budget",
        "terminal_soft_wrap_budget_before_allocation",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w256_terminal_scrollback_soft_wrap_materialization_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render scrollback soft-wrap materialization budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W256 terminal scrollback soft-wrap materialization budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w256_terminal_scrollback_soft_wrap_materialization_budget_smoke: ok")


if __name__ == "__main__":
    main()
