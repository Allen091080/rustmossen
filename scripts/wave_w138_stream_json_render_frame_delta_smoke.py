#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / "crates/mossen-cli/src/stream_json_render_events.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"
PHASE = ROOT / "phases/03g-rendering-product-grade-plan.md"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    bridge = BRIDGE.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()
    phase = PHASE.read_text()

    for token in (
        "previous_frame_fingerprint",
        "StreamJsonRenderFrameFingerprint",
        "terminal_frame_value_with_previous",
        "render_frame_region_delta",
        "render_frame_region_hashes",
        "stable_render_region_hash",
        '"regionHash"',
        '"changedRegionIds"',
        '"unchangedRegionIds"',
        '"skipIfFrameHashUnchanged"',
        '"skipDrawWhenUnchanged"',
        "terminal_frame_marks_unchanged_regions_for_skip_draw",
    ):
        require(bridge, token, f"frame delta token {token}")

    for token in (
        '"region_hashes": true',
        '"changed_region_ids": true',
        '"skip_unchanged_regions": true',
        '"frame_hash_excludes_sequence": true',
        'body["runtime"]["render"]["draw_contract"]["skip_unchanged_regions"]',
    ):
        require(structured_io, token, f"status frame delta metadata {token}")

    require(
        run_all,
        "wave_w138_stream_json_render_frame_delta_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json render frame delta",
        "phase note",
    )

    print("wave_w138_stream_json_render_frame_delta_smoke: ok")


if __name__ == "__main__":
    main()
