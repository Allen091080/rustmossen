#!/usr/bin/env python3
"""W186 - terminal renderer reports top-stack clipping diagnostics."""

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
        "topRegionLineCount",
        "topRegionOverflowPolicy",
        "clip_before_bottom_regions",
        "topRegionClipDiagnostics",
        "top_clipped_row_count",
        "reserved_bottom_rows",
        "visible_top_row_budget",
        "draw_executor_clips_top_widgets_before_bottom_regions_on_short_viewports",
    ]:
        require(renderer, token, "top-stack clip diagnostics", failures)

    for token in [
        "terminal_top_stack_clip_diagnostics",
        "terminal_visible_top_budget_report",
    ]:
        require(structured, token, "status metadata", failures)

    require(
        run_all,
        "wave_w186_terminal_top_stack_clip_diagnostics_smoke",
        "run_all registration",
        failures,
    )
    require(
        phase,
        "2026-05-23 Terminal-render top-stack clip diagnostics",
        "phase note",
        failures,
    )

    if failures:
        print("=== W186 terminal top-stack clip diagnostics smoke ===")
        for failure in failures:
            print(f"- {failure}")
        sys.exit(1)
    print("wave_w186_terminal_top_stack_clip_diagnostics_smoke: ok")


if __name__ == "__main__":
    main()
