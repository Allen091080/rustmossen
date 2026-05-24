#!/usr/bin/env python3
"""W268 - priority drain releases manual-scroll end holds."""

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
        "last_drained_scroll_state",
        "terminal_render_submit_follow_up_after_priority_drain",
        "terminal_render_release_manual_scroll_end_after_priority_drain",
        "TERMINAL_RENDER_SCROLL_EVENT_END",
        "priority_drain_manual_scroll_end_releases_hold_and_flushes_pending_draw",
    ]:
        require(repl, token, "priority drain scroll-end follow-up", failures)

    for token in [
        "terminal_priority_drain_releases_manual_scroll_end",
        "terminal_priority_drain_flushes_manual_scroll_pending_draw",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w268_terminal_priority_scroll_end_flush_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render priority-drain manual-scroll end flush",
        "phase note",
        failures,
    )

    if failures:
        print("=== W268 terminal priority scroll-end flush smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w268_terminal_priority_scroll_end_flush_smoke: ok")


if __name__ == "__main__":
    main()
