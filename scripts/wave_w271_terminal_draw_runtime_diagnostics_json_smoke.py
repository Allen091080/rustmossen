#!/usr/bin/env python3
"""W271 - draw runtime exposes a JSON diagnostics snapshot."""

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
        "runtime_diagnostics_value",
        "terminal_draw_runtime_snapshot_value",
        "terminal_draw_runtime_report_value",
        "terminal_draw_execution_report_value",
        "draw_runtime_diagnostics_value_serializes_last_report_summary",
        "\"lastReport\"",
        "\"execution\"",
    ]:
        require(renderer, token, "draw runtime diagnostics json", failures)

    for token in [
        "terminal_draw_runtime_diagnostics_json",
        "terminal_draw_runtime_last_report_json_summary",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w271_terminal_draw_runtime_diagnostics_json_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render draw-runtime diagnostics JSON",
        "phase note",
        failures,
    )

    if failures:
        print("=== W271 terminal draw-runtime diagnostics json smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w271_terminal_draw_runtime_diagnostics_json_smoke: ok")


if __name__ == "__main__":
    main()
