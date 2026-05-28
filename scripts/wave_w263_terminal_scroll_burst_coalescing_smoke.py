#!/usr/bin/env python3
"""W263 - terminal manual-scroll bursts are coalesced before redraw."""

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
        "terminal_scroll_event_pending",
        "TERMINAL_RENDER_SCROLL_EVENT_START",
        "TERMINAL_RENDER_SCROLL_EVENT_END",
        "terminal_render_scroll_event_pending_state",
        "terminal_render_release_scroll_frontend_event",
        "coalesces_repeated_manual_scroll_frontend_events_until_state_is_handled",
    ]:
        require(repl, token, "manual-scroll burst coalescing frontend gate", failures)

    for token in [
        "terminal_scroll_event_pending_gate",
        "terminal_scroll_burst_coalesced_before_queue",
        "terminal_scroll_pending_released_after_handle",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w263_terminal_scroll_burst_coalescing_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render manual-scroll burst coalescing",
        "phase note",
        failures,
    )

    if failures:
        print("=== W263 terminal manual-scroll burst coalescing smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w263_terminal_scroll_burst_coalescing_smoke: ok")


if __name__ == "__main__":
    main()
