#!/usr/bin/env python3
"""W177 - terminal renderer summarizes background Bash task lifecycle output."""

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
        "terminal_supplemental_render_events_for_sdk_message",
        "terminal_enrich_tool_summary_event_value",
        "terminal_background_bash_start_summary_keeps_task_id_and_preview",
        "terminal_task_output_background_shell_renders_bounded_task_summary",
        "backgroundTaskId",
        "previewLineItems",
        "background task completed",
    ]:
        require(render, token, "background task render bridge", failures)

    require(
        structured,
        "terminal_background_task_render_summary",
        "status metadata",
        failures,
    )
    require(
        run_all,
        "wave_w177_terminal_background_task_render_summary_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render background task render summary",
        "phase note",
        failures,
    )

    if failures:
        print("=== W177 terminal background task render summary smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w177_terminal_background_task_render_summary_smoke: ok")


if __name__ == "__main__":
    main()
