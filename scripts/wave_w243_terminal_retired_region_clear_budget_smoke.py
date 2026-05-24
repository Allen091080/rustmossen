#!/usr/bin/env python3
"""W243 - retired region clears are budgeted before terminal ops are built."""

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
        "render_patch_retired_region_source_line_count",
        "render_draw_previous_region_line_count",
        "render_draw_line_budget_max_lines",
        '"cap_retired_region_clear_lines_before_terminal_ops"',
        "patch_renderer_caps_retired_region_clear_lines_before_terminal_ops",
        '"regionLineBudgetOmittedLineCount"',
    ]:
        require(terminal, token, "retired clear budget", failures)

    for token in [
        "terminal_retired_region_clear_line_budget",
        "terminal_clear_ops_prebudgeted_lines",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w243_terminal_retired_region_clear_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render retired region clear budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W243 terminal retired region clear budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w243_terminal_retired_region_clear_budget_smoke: ok")


if __name__ == "__main__":
    main()
