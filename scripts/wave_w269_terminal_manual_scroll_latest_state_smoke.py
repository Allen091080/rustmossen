#!/usr/bin/env python3
"""W269 - manual scroll coalesces to latest pending state."""

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
        "terminal_render_take_scroll_frontend_event_state",
        "scroll_event_pending.swap(scroll_state",
        "coalesces_opposite_manual_scroll_frontend_events_to_latest_state",
        "TERMINAL_RENDER_SCROLL_EVENT_END",
        "TERMINAL_RENDER_SCROLL_EVENT_START",
    ]:
        require(repl, token, "manual scroll latest-state coalescing", failures)

    for token in [
        "terminal_manual_scroll_latest_state_coalescing",
        "terminal_manual_scroll_opposite_state_supersedes_pending",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w269_terminal_manual_scroll_latest_state_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render manual-scroll latest-state coalescing",
        "phase note",
        failures,
    )

    if failures:
        print("=== W269 terminal manual-scroll latest-state smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w269_terminal_manual_scroll_latest_state_smoke: ok")


if __name__ == "__main__":
    main()
