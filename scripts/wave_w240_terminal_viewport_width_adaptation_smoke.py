#!/usr/bin/env python3
"""W240 - terminal draw plans advertise viewport width adaptation."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_draw_viewport_adaptation_contract_value",
        "terminal_viewport_width_profile",
        '"viewportAdaptation"',
        '"viewportAdaptive"',
        '"fullColumnsFrom"',
        '"compactColumnsFrom"',
        '"minimalColumnsBelow"',
        '"recompute_profile_before_pending_flush"',
        "viewport_width_profile",
        "draw_scheduler_advertises_viewport_width_adaptation_contract",
        "draw_executor_reports_viewport_width_profile_tiers",
    ]:
        require(terminal, token, "terminal viewport adaptation contract", failures)

    for token in [
        "terminal_viewport_width_adaptation_contract",
        "terminal_draw_viewport_profile_report",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w240_terminal_viewport_width_adaptation_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render viewport width adaptation contract",
        "phase note",
        failures,
    )

    if failures:
        print("=== W240 terminal viewport width adaptation smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w240_terminal_viewport_width_adaptation_smoke: ok")


if __name__ == "__main__":
    main()
