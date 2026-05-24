#!/usr/bin/env python3
"""W180 - terminal renderer exposes plan updates as an independent region."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    render = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    terminal_renderer = (
        ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
    ).read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "current_plan_widget: Option<Value>",
        '"planRegionId": if plan_active { "plan" } else { "" }',
        '"plan": {',
        '"replace_plan"',
        "fn terminal_plan_widget",
        "fn terminal_plan_lines",
        "terminal_frame_renders_plan_as_independent_bounded_region",
        "plan_index < command_index",
    ]:
        require(render, token, "independent plan region", failures)

    require(terminal_renderer, '"plan" => {', "plan semantic style", failures)
    require(
        structured,
        "terminal_plan_status_panel",
        "status metadata",
        failures,
    )
    require(
        structured,
        "independent_plan_region",
        "draw contract metadata",
        failures,
    )
    require(
        run_all,
        "wave_w180_terminal_plan_status_panel_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render independent plan status panel",
        "phase note",
        failures,
    )

    if failures:
        print("=== W180 terminal plan status panel smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w180_terminal_plan_status_panel_smoke: ok")


if __name__ == "__main__":
    main()
