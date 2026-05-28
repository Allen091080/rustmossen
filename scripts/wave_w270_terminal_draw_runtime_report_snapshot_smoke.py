#!/usr/bin/env python3
"""W270 - draw runtime keeps last report and counters for observability."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    renderer = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "StreamJsonTerminalDrawRuntimeSnapshot",
        "last_runtime_report",
        "runtime_report_count",
        "runtime_dropped_pending_count",
        "record_runtime_report",
        "draw_runtime_snapshot_tracks_last_report_and_counters",
    ]:
        require(renderer, token, "draw runtime report snapshot", failures)

    for token in [
        "terminal_draw_runtime_last_report_snapshot",
        "terminal_draw_runtime_report_counters",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w270_terminal_draw_runtime_report_snapshot_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render draw-runtime report snapshot",
        "phase note",
        failures,
    )

    if failures:
        print("=== W270 terminal draw-runtime report snapshot smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w270_terminal_draw_runtime_report_snapshot_smoke: ok")


if __name__ == "__main__":
    main()
