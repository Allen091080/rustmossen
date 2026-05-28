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
        "STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION",
        "STREAM_JSON_RENDER_FRAME_TYPE",
        "terminal_frame_value",
        "stable_render_frame_hash",
        "terminal_activity_lines",
        "terminal_draw_mode",
        '"render_frame"',
        '"preferredStrategy": "patch_regions"',
        '"replaceWholeScreen": false',
        '"preserveOnActiveUpdate": true',
        "emits_line_oriented_terminal_frame_after_snapshot",
    ):
        require(bridge, token, f"terminal frame bridge token {token}")

    for token in (
        '"frame_stream"',
        '"frame_type"',
        '"frame_schema_version"',
        '"draw_contract"',
        '"preferred_strategy": "patch_regions"',
        '"replace_whole_screen": false',
        'body["runtime"]["render"]["frame_type"], "render_frame"',
    ):
        require(structured_io, token, f"status terminal frame metadata {token}")

    require(
        run_all,
        "wave_w137_stream_json_terminal_frame_contract_smoke.py",
        "run_all registration",
    )
    require(
        phase,
        "2026-05-23 Stream-json terminal frame contract",
        "phase note",
    )

    print("wave_w137_stream_json_terminal_frame_contract_smoke: ok")


if __name__ == "__main__":
    main()
