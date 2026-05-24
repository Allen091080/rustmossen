#!/usr/bin/env python3
"""W264 - terminal priority events bypass low-priority render backlog."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    repl = (ROOT / "crates/mossen-cli/src/repl.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_priority_event_tx",
        "terminal_priority_event_rx",
        "terminal_render_frontend_event_is_priority",
        "terminal_render_drain_superseded_low_priority_frontend_events",
        "TERMINAL_RENDER_LOW_PRIORITY_DRAIN_LIMIT",
        "routes_priority_frontend_events_ahead_of_low_priority_backlog",
        "biased;",
    ]:
        require(repl, token, "priority frontend event routing", failures)

    for token in [
        "terminal_frontend_priority_event_queue",
        "terminal_frontend_priority_bypasses_low_priority_render_events",
        "terminal_frontend_priority_drops_superseded_low_priority_backlog",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w264_terminal_priority_frontend_events_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render priority frontend events",
        "phase note",
        failures,
    )

    if failures:
        print("=== W264 terminal priority frontend events smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w264_terminal_priority_frontend_events_smoke: ok")


if __name__ == "__main__":
    main()
