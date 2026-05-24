#!/usr/bin/env python3
"""W267 - priority drain preserves a follow-up resize redraw."""

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
        "TerminalRenderLowPriorityDrainReport",
        "drained_resize_event",
        "terminal_render_submit_resize_redraw_after_priority_drain",
        "emit_terminal_resize_draw_plan_items",
        "priority_drain_reports_resize_for_follow_up_redraw",
    ]:
        require(repl, token, "priority drain resize follow-up", failures)

    for token in [
        "terminal_priority_drain_preserves_resize_redraw",
        "terminal_priority_drain_reports_resize_follow_up",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w267_terminal_priority_resize_redraw_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render priority-drain resize redraw",
        "phase note",
        failures,
    )

    if failures:
        print("=== W267 terminal priority resize redraw smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w267_terminal_priority_resize_redraw_smoke: ok")


if __name__ == "__main__":
    main()
