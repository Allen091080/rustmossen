#!/usr/bin/env python3
"""W262 - terminal resize bursts are coalesced before redraw."""

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
        "terminal_resize_event_pending",
        "terminal_render_try_enqueue_frontend_event_with_resize_coalescing",
        "compare_exchange(false, true",
        "terminal_render_release_resize_frontend_event",
        "coalesces_resize_frontend_events_until_resize_is_handled",
        "emit_terminal_resize_draw_plan_items()",
    ]:
        require(repl, token, "resize burst coalescing frontend gate", failures)

    for token in [
        "terminal_resize_event_pending_gate",
        "terminal_resize_burst_coalesced_before_queue",
        "terminal_resize_pending_released_after_handle",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w262_terminal_resize_burst_coalescing_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render resize burst coalescing",
        "phase note",
        failures,
    )

    if failures:
        print("=== W262 terminal resize burst coalescing smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w262_terminal_resize_burst_coalescing_smoke: ok")


if __name__ == "__main__":
    main()
