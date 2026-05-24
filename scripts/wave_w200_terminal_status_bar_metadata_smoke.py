#!/usr/bin/env python3
"""W200 - terminal status bar exposes model, mode, reasoning, and context."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_status_bar_value",
        "terminal_status_permission_mode",
        "terminal_status_context_tokens",
        "terminal_status_elapsed_label",
        "\"statusBar\"",
        "terminal_status_bar_reports_model_mode_reasoning_and_context",
    ]:
        require(events, token, "status bar renderer", failures)

    for token in [
        "terminal_status_bar_rich_metadata",
        "terminal_status_bar_model_mode_reasoning",
        "terminal_status_bar_context_usage",
        "terminal_status_bar_width_variants",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w200_terminal_status_bar_metadata_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render rich status bar metadata",
        "phase note",
        failures,
    )

    if failures:
        print("=== W200 terminal status bar metadata smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w200_terminal_status_bar_metadata_smoke: ok")


if __name__ == "__main__":
    main()
