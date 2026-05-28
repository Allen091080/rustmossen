#!/usr/bin/env python3
"""W272 - diagnostics JSON backs a simulated terminal draw-runtime soak."""

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
        "draw_runtime_diagnostics_soak_tracks_stream_resize_scroll_without_stuck_pending",
        "runtime_diagnostics_value",
        "\"hasPendingDraw\"",
        "\"manualScrollActive\"",
        "\"droppedPendingCount\"",
        "\"terminalOpBudgetExceeded\"",
        "held stream chunk 79",
        "live stream chunk 159",
    ]:
        require(renderer, token, "diagnostics soak", failures)

    for token in [
        "terminal_draw_runtime_diagnostics_soak",
        "terminal_draw_runtime_diagnostics_no_stuck_pending",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w272_terminal_draw_runtime_diagnostics_soak_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render draw-runtime diagnostics soak",
        "phase note",
        failures,
    )

    if failures:
        print("=== W272 terminal draw-runtime diagnostics soak smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w272_terminal_draw_runtime_diagnostics_soak_smoke: ok")


if __name__ == "__main__":
    main()
