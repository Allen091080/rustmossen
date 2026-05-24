#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "crates/mossen-cli/src/stream_json_terminal_renderer.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    renderer = RENDERER.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "previous_top_region_layouts",
        "render_frame_top_region_layouts",
        "render_patch_top_layout_changed_region_ids",
        "dynamic_top_stack",
        "previousTopRowsToClear",
        "topLayoutCompactsGaps",
        "patch_renderer_redraws_unchanged_top_region_when_stack_offset_changes",
        "draw_scheduler_clears_unoccupied_previous_top_rows_after_stack_compacts",
    ):
        require(renderer, token, f"dynamic top stack token {token}")

    for token in (
        '"terminal_dynamic_top_stack": true',
        '"terminal_dynamic_top_stack"',
    ):
        require(structured_io, token, f"status dynamic top stack metadata {token}")

    require(
        run_all,
        "wave_w154_stream_json_terminal_dynamic_top_stack_smoke",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal dynamic top stack",
        "phase note",
    )

    print("wave_w154_stream_json_terminal_dynamic_top_stack_smoke: ok")


if __name__ == "__main__":
    main()
