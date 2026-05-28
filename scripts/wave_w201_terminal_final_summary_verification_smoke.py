#!/usr/bin/env python3
"""W201 - terminal final summary keeps command history and verification status."""

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
        "terminal_command_history_item",
        "terminal_command_history_summary",
        "terminal_final_summary_residual_risk",
        "\"verificationSummary\"",
        "\"commandHistory\"",
        "terminal_final_summary_records_command_history_and_verification",
    ]:
        require(events, token, "final summary renderer", failures)

    for token in [
        "terminal_final_summary_command_history",
        "terminal_final_summary_verification_results",
        "terminal_final_summary_residual_risks",
        "terminal_final_summary_bounded_command_history",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w201_terminal_final_summary_verification_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render final summary verification context",
        "phase note",
        failures,
    )

    if failures:
        print("=== W201 terminal final summary verification smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w201_terminal_final_summary_verification_smoke: ok")


if __name__ == "__main__":
    main()
