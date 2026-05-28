#!/usr/bin/env python3
"""W178 - terminal renderer keeps a bounded background task status panel."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    tui_events = (ROOT / "crates/mossen-tui/src/render_events.rs").read_text()
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "BackgroundTaskUpdated",
        "background_task_updated_event",
        "maps_background_bash_summary_to_task_update_event",
        "maps_task_output_background_shell_to_task_update_event",
    ]:
        require(tui_events, token, "render event extraction", failures)

    for token in [
        "current_background_tasks",
        "terminal_background_task_lines",
        "terminal_background_task_items_value",
        "backgroundTaskRegionId",
        "terminal_background_task_panel_persists_after_foreground_command_changes",
        "terminal_background_task_panel_updates_completed_task_without_log_wall",
    ]:
        require(render, token, "stream-json terminal task panel", failures)

    require(
        structured,
        "terminal_background_task_status_panel",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w178_terminal_background_task_status_panel_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render background task status panel",
        "phase note",
        failures,
    )

    if failures:
        print("=== W178 terminal background task status panel smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w178_terminal_background_task_status_panel_smoke: ok")


if __name__ == "__main__":
    main()
