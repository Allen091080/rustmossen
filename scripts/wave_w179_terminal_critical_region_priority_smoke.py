#!/usr/bin/env python3
"""W179 - terminal renderer prioritizes critical top regions."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "error_index < final_summary_index",
        "final_summary_index < command_index",
        "command_index < background_task_index",
        "final_summary_start_row < command_start_row",
        "command_start_row < background_task_start_row",
        "background_task_start_row < diff_start_row",
    ]:
        require(render, token, "critical top-region ordering test", failures)

    require(
        structured,
        "terminal_critical_region_top_priority",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w179_terminal_critical_region_priority_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render critical region top priority",
        "phase note",
        failures,
    )

    if failures:
        print("=== W179 terminal critical region priority smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w179_terminal_critical_region_priority_smoke: ok")


if __name__ == "__main__":
    main()
