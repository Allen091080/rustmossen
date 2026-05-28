#!/usr/bin/env python3
"""W241 - terminal executor selects status/footer line variants by viewport."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, token: str, label: str, failures: list[str]) -> None:
    if token not in text:
        failures.append(f"{label}: missing {token!r}")


def main() -> None:
    events = (ROOT / "crates/mossen-cli/src/stream_json_render_events.rs").read_text()
    terminal = (ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs").read_text()
    structured = (ROOT / "crates/mossen-cli/src/structured_io.rs").read_text()
    run_all = (ROOT / "scripts/run_all_smoke.sh").read_text()
    phase = (ROOT / "phases/03g-rendering-product-grade-plan.md").read_text()

    failures: list[str] = []
    for token in [
        "terminal_status_line_variants",
        "terminal_footer_line_variants",
        "terminal_frame_region_attach_line_variants",
        '"lineVariants"',
        '"viewportSelectableLines"',
        "terminal_frame_exposes_status_and_footer_viewport_line_variants",
    ]:
        require(events, token, "render-event viewport variants", failures)

    for token in [
        "render_patch_line_variants",
        "render_draw_line_variants",
        "terminal_draw_text_for_viewport",
        "terminal_viewport_variant_key_order",
        "viewport_variant_selection_count",
        "draw_plan_preserves_viewport_selectable_line_variants",
        "draw_executor_selects_minimal_line_variant_for_narrow_viewport",
    ]:
        require(terminal, token, "terminal viewport variant selection", failures)

    for token in [
        "terminal_status_footer_viewport_variants",
        "terminal_viewport_line_variant_selection",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w241_terminal_viewport_line_variant_selection_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-24 Terminal-render viewport line variant selection",
        "phase note",
        failures,
    )

    if failures:
        print("=== W241 terminal viewport line variant selection smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w241_terminal_viewport_line_variant_selection_smoke: ok")


if __name__ == "__main__":
    main()
