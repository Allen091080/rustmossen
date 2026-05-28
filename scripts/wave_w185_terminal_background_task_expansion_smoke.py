#!/usr/bin/env python3
"""W185 - terminal renderer supports bounded background-task expansion."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "ToggleBackgroundTaskExpansion",
        "toggle_background_task_panel_expanded",
        "STREAM_JSON_RENDER_BACKGROUND_TASK_EXPANDED_MAX_ITEMS",
        "terminal_background_task_update_mode",
        '"replace_expanded_summary"',
        '"backgroundTaskToggleKey"',
        '"b expand bg"',
        "terminal_background_task_panel_toggle_expands_bounded_task_list",
    ]:
        require(render, token, "background task expansion model", failures)

    for token in [
        "TerminalRenderFrontendEvent::ToggleBackgroundTaskExpansion",
        "KeyCode::Char('b')",
        "maps_widget_toggle_keys_to_frontend_events",
    ]:
        require(repl, token, "frontend background task key bridge", failures)

    for token in [
        "terminal_background_task_expansion_controls",
        "terminal_background_task_expanded_panel",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w185_terminal_background_task_expansion_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render background task expansion",
        "phase note",
        failures,
    )

    if failures:
        print("=== W185 terminal background task expansion smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w185_terminal_background_task_expansion_smoke: ok")


if __name__ == "__main__":
    main()
