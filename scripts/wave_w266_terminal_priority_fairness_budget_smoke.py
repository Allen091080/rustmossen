#!/usr/bin/env python3
"""W266 - terminal priority events yield after a fairness budget."""

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
        "TERMINAL_RENDER_PRIORITY_FAIRNESS_BURST_LIMIT",
        "terminal_priority_events_since_fairness_yield",
        "terminal_render_priority_fairness_allows",
        "terminal_render_priority_fairness_yield_due",
        "terminal_render_note_priority_frontend_event",
        "terminal_render_reset_priority_fairness_budget",
        "tokio::task::yield_now()",
        "priority_frontend_event_fairness_yields_after_burst_limit",
    ]:
        require(repl, token, "terminal priority fairness loop", failures)

    for token in [
        "terminal_frontend_priority_fairness_budget",
        "terminal_frontend_priority_yields_to_sdk_and_permission",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w266_terminal_priority_fairness_budget_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render priority fairness budget",
        "phase note",
        failures,
    )

    if failures:
        print("=== W266 terminal priority fairness budget smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w266_terminal_priority_fairness_budget_smoke: ok")


if __name__ == "__main__":
    main()
