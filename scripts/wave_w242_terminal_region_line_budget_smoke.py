#!/usr/bin/env python3
"""W242 - terminal region lines are budgeted before terminal ops are built."""

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
        "STREAM_JSON_RENDER_PATCH_MAX_REGION_LINES",
        "RenderPatchLineSet",
        "render_patch_region_budgeted_line_count",
        '"lineBudget"',
        '"cap_region_lines_before_terminal_ops"',
        '"regionLineBudgeted"',
        '"terminalOpsPrebudgetedLines"',
        "patch_renderer_caps_pathological_region_lines_before_terminal_ops",
    ]:
        require(terminal, token, "terminal region line budget", failures)

    for token in [
        "terminal_anchored_region_line_budget",
        "terminal_ops_prebudgeted_region_lines",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w242_terminal_region_line_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render region line budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W242 terminal region line budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w242_terminal_region_line_budget_smoke: ok")


if __name__ == "__main__":
    main()
